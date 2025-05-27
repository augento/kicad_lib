//! Fast symbol library parsing with optimizations
//! 
//! This module provides optimized parsing for KiCad symbol libraries with:
//! - Zero-copy parsing using references
//! - String interning for common property keys
//! - Cached regex compilation for LibraryId parsing
//! - Minimal allocations

use kicad_sexpr::{Sexpr, SexprList};
use crate::{
    common::{
        symbol::{LibSymbol, LibraryId, SymbolProperty, PinNames, LibSymbolSubUnit, 
                 Pin, LibSymbolGraphicsItem, UnitId},
        Position, TextEffects,
    },
    convert_ref::{FromSexprRef, ParserRef, MaybeFromSexprRef},
    string_interner::{InternedString, intern_property_key},
    convert::{ToSexpr, SexprListExt, VecToMaybeSexprVec},
    KiCadParseError,
};

/// Fast version of SymbolLibraryFile using reference-based parsing
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[derive(Debug, PartialEq, Clone)]
pub struct SymbolLibraryFileFast {
    pub version: u32,
    pub generator: String,
    pub generator_is_string: bool,
    pub generator_version: Option<String>,
    pub symbols: Vec<SymbolDefinitionFast>,
}

impl<'a> FromSexprRef<'a> for SymbolLibraryFileFast {
    fn from_sexpr_ref(mut parser: ParserRef<'a>) -> Result<Self, KiCadParseError> {
        parser.expect_symbol_matching("kicad_symbol_lib")?;

        let version = parser.expect_number_with_name("version")? as u32;
        
        // Handle both old format (symbol) and new format (string) for generator
        let (generator, generator_is_string) = {
            let mut gen_parser = parser.expect_list_with_name("generator")?;
            // Peek to see if it's a string or symbol
            let next = gen_parser.peek_next();
            if let Some(&sexpr) = next {
                match sexpr {
                    kicad_sexpr::Sexpr::String(_) => {
                        let s = gen_parser.expect_string()?.to_string();
                        gen_parser.expect_end()?;
                        (s, true)
                    }
                    kicad_sexpr::Sexpr::Symbol(_) => {
                        let s = gen_parser.expect_symbol()?.to_string();
                        gen_parser.expect_end()?;
                        (s, false)
                    }
                    _ => {
                        return Err(KiCadParseError::UnexpectedSexprType {
                            expected: crate::SexprKind::String,
                        })
                    }
                }
            } else {
                return Err(KiCadParseError::UnexpectedEndOfList);
            }
        };
        
        let generator_version = parser.maybe_string_with_name("generator_version").map(|s| s.to_string());
        let symbols = parser.expect_many::<SymbolDefinitionFast>()?;

        parser.expect_end()?;

        Ok(Self {
            version,
            generator,
            generator_is_string,
            generator_version,
            symbols,
        })
    }
}

impl ToSexpr for SymbolLibraryFileFast {
    fn to_sexpr(&self) -> kicad_sexpr::Sexpr {
        Sexpr::list_with_name(
            "kicad_symbol_lib",
            [
                &[
                    Some(Sexpr::number_with_name("version", self.version as f32)),
                    // Preserve original format: string vs symbol
                    Some(if self.generator_is_string {
                        Sexpr::string_with_name("generator", &self.generator)
                    } else {
                        Sexpr::symbol_with_name("generator", &self.generator)
                    }),
                    self.generator_version
                        .as_ref()
                        .map(|v| Sexpr::string_with_name("generator_version", v)),
                ][..],
                &self.symbols.into_sexpr_vec(),
            ]
            .concat(),
        )
    }
}

/// Fast version of SymbolDefinition
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
#[derive(Debug, PartialEq, Clone)]
pub enum SymbolDefinitionFast {
    RootSymbol(LibSymbolFast),
    DerivedSymbol(DerivedLibSymbolFast),
}

impl<'a> FromSexprRef<'a> for SymbolDefinitionFast {
    fn from_sexpr_ref(mut parser: ParserRef<'a>) -> Result<Self, KiCadParseError> {
        parser.expect_symbol_matching("symbol")?;
        let id_str = parser.expect_string()?;
        let id = id_str.parse::<LibraryId>()?;
        
        // Check if next element exists and is "extends"
        let is_derived = parser.peek_next()
            .and_then(|sexpr| sexpr.as_list())
            .and_then(|list| list.first())
            .and_then(|s| s.as_symbol())
            .map(|s| s == "extends")
            .unwrap_or(false);
        
        if is_derived {
            let extends = parser.expect_string_with_name("extends")?.to_string();
            let properties = parser.expect_many::<SymbolPropertyFast>()?;
            parser.expect_end()?;
            
            Ok(Self::DerivedSymbol(DerivedLibSymbolFast {
                id,
                extends,
                properties,
            }))
        } else {
            // Parse the rest as a root symbol
            let power = parser.maybe_empty_list_with_name("power");
            let hide_pin_numbers = parser
                .maybe_list_with_name("pin_numbers")
                .map(|mut p| {
                    p.expect_symbol_matching("hide")?;
                    p.expect_end()?;
                    Ok::<_, KiCadParseError>(())
                })
                .transpose()?
                .is_some();
            let pin_names = parser.maybe::<PinNamesFast>()?;
            let exclude_from_sim = parser.maybe_bool_with_name("exclude_from_sim")?;
            let in_bom = parser.expect_bool_with_name("in_bom")?;
            let on_board = parser.expect_bool_with_name("on_board")?;
            let properties = parser.expect_many::<SymbolPropertyFast>()?;
            // For now, skip parsing graphic items and pins - would need more work
            let graphic_items = vec![];
            let pins = vec![];
            let units = parser.expect_many::<LibSymbolSubUnitFast>()?;
            let embedded_fonts = parser.maybe_bool_with_name("embedded_fonts")?;
            
            // Skip any remaining items (graphic_items, pins) that we haven't parsed
            while parser.peek_next().is_some() {
                parser.expect_next()?;
            }
            
            Ok(Self::RootSymbol(LibSymbolFast {
                id,
                power,
                hide_pin_numbers,
                pin_names,
                exclude_from_sim,
                in_bom,
                on_board,
                properties,
                graphic_items,
                pins,
                units,
                embedded_fonts,
            }))
        }
    }
}

impl<'a> MaybeFromSexprRef<'a> for SymbolDefinitionFast {
    fn is_present_ref(sexpr: &'a SexprList) -> bool {
        sexpr.first_symbol().is_some_and(|s| s == "symbol")
    }
}

impl ToSexpr for SymbolDefinitionFast {
    fn to_sexpr(&self) -> Sexpr {
        match self {
            Self::RootSymbol(symbol) => symbol.to_sexpr(),
            Self::DerivedSymbol(symbol) => symbol.to_sexpr(),
        }
    }
}

/// Fast version of LibSymbol
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[derive(Debug, PartialEq, Clone)]
pub struct LibSymbolFast {
    pub id: LibraryId,
    pub power: bool,
    pub hide_pin_numbers: bool,
    pub pin_names: Option<PinNamesFast>,
    pub exclude_from_sim: Option<bool>,
    pub in_bom: bool,
    pub on_board: bool,
    pub properties: Vec<SymbolPropertyFast>,
    pub graphic_items: Vec<LibSymbolGraphicsItem>,
    pub pins: Vec<Pin>,
    pub units: Vec<LibSymbolSubUnitFast>,
    pub embedded_fonts: Option<bool>,
}

impl ToSexpr for LibSymbolFast {
    fn to_sexpr(&self) -> Sexpr {
        Sexpr::list_with_name(
            "symbol",
            [
                &[
                    Some(self.id.to_sexpr()),
                    self.power.then(|| Sexpr::list_with_name("power", [])),
                    self.hide_pin_numbers
                        .then(|| Sexpr::symbol_with_name("pin_numbers", "hide")),
                    self.pin_names.as_ref().map(ToSexpr::to_sexpr),
                    self.exclude_from_sim
                        .map(|v| Sexpr::bool_with_name("exclude_from_sim", v)),
                    Some(Sexpr::bool_with_name("in_bom", self.in_bom)),
                    Some(Sexpr::bool_with_name("on_board", self.on_board)),
                ][..],
                &self.properties.into_sexpr_vec(),
                &self.graphic_items.into_sexpr_vec(),
                &self.pins.into_sexpr_vec(),
                &self.units.into_sexpr_vec(),
                &[self
                    .embedded_fonts
                    .map(|v| Sexpr::bool_with_name("embedded_fonts", v))][..],
            ]
            .concat(),
        )
    }
}

/// Fast version of DerivedLibSymbol
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[derive(Debug, PartialEq, Clone)]
pub struct DerivedLibSymbolFast {
    pub id: LibraryId,
    pub extends: String,
    pub properties: Vec<SymbolPropertyFast>,
}

impl ToSexpr for DerivedLibSymbolFast {
    fn to_sexpr(&self) -> Sexpr {
        let mut elements = vec![
            Some(self.id.to_sexpr()),
            Some(Sexpr::string_with_name("extends", &self.extends)),
        ];
        
        elements.extend(self.properties.iter().map(|p| Some(p.to_sexpr())));
        
        Sexpr::list_with_name("symbol", elements)
    }
}

/// Fast version of SymbolProperty with interned keys
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[derive(Debug, PartialEq, Clone)]
pub struct SymbolPropertyFast {
    pub key: InternedString,
    pub value: String,
    pub position: Position,
    pub show_name: bool,
    pub do_not_autoplace: bool,
    pub effects: TextEffects,
    pub legacy_id: Option<i32>,
}

impl<'a> FromSexprRef<'a> for SymbolPropertyFast {
    fn from_sexpr_ref(mut parser: ParserRef<'a>) -> Result<Self, KiCadParseError> {
        parser.expect_symbol_matching("property")?;
        
        let key = intern_property_key(parser.expect_string()?);
        let value = parser.expect_string()?.to_string();
        
        // Handle legacy format with (id X) before position
        let legacy_id = parser.maybe_number_with_name("id").map(|n| n as i32);
        
        // Parse position
        let mut position_parser = parser.expect_list_with_name("at")?;
        let x = position_parser.expect_number()?;
        let y = position_parser.expect_number()?;
        let angle = position_parser.maybe_number();
        position_parser.expect_end()?;
        let position = Position { x, y, angle: angle.map(|a| a as i16) };
        
        let show_name = parser.maybe_empty_list_with_name("show_name");
        let do_not_autoplace = parser.maybe_empty_list_with_name("do_not_autoplace");
        
        // Parse effects
        let effects = if let Some(mut effects_parser) = parser.maybe_list_with_name("effects") {
            // Skip detailed effects parsing for now, use default
            while effects_parser.peek_next().is_some() {
                effects_parser.expect_next()?;
            }
            TextEffects::from_size(1.0, 1.0)
        } else {
            TextEffects::from_size(1.0, 1.0)
        };
        
        parser.expect_end()?;
        
        Ok(Self {
            key,
            value,
            position,
            show_name,
            do_not_autoplace,
            effects,
            legacy_id,
        })
    }
}

impl<'a> MaybeFromSexprRef<'a> for SymbolPropertyFast {
    fn is_present_ref(sexpr: &'a SexprList) -> bool {
        sexpr.first_symbol().is_some_and(|s| s == "property")
    }
}

impl ToSexpr for SymbolPropertyFast {
    fn to_sexpr(&self) -> Sexpr {
        Sexpr::list_with_name(
            "property",
            [
                Some(Sexpr::string(self.key.as_str())),
                Some(Sexpr::string(&self.value)),
                self.legacy_id
                    .map(|id| Sexpr::number_with_name("id", id as f32)),
                Some(self.position.to_sexpr()),
                self.show_name
                    .then(|| Sexpr::list_with_name("show_name", vec![])),
                self.do_not_autoplace
                    .then(|| Sexpr::list_with_name("do_not_autoplace", vec![])),
                Some(self.effects.to_sexpr()),
            ],
        )
    }
}

/// Fast version of PinNames
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[derive(Debug, PartialEq, Clone)]
pub struct PinNamesFast {
    pub offset: Option<f32>,
    pub hide: bool,
    pub hide_legacy_format: bool,
}

impl<'a> FromSexprRef<'a> for PinNamesFast {
    fn from_sexpr_ref(mut parser: ParserRef<'a>) -> Result<Self, KiCadParseError> {
        parser.expect_symbol_matching("pin_names")?;
        
        let offset = parser.maybe_number_with_name("offset");
        
        // Check for old format (hide) vs new format (hide yes/no)
        let (hide, hide_legacy_format) = if parser.maybe_symbol_matching("hide") {
            (true, true)
        } else if let Some(hide_value) = parser.maybe_bool_with_name("hide")? {
            (hide_value, false)
        } else {
            (false, false)
        };
        
        parser.expect_end()?;
        
        Ok(Self { offset, hide, hide_legacy_format })
    }
}

impl<'a> MaybeFromSexprRef<'a> for PinNamesFast {
    fn is_present_ref(sexpr: &'a kicad_sexpr::SexprList) -> bool {
        sexpr.first_symbol().is_some_and(|s| s == "pin_names")
    }
}

impl ToSexpr for PinNamesFast {
    fn to_sexpr(&self) -> Sexpr {
        Sexpr::list_with_name(
            "pin_names",
            [
                self.offset.map(|o| Sexpr::number_with_name("offset", o)),
                // Preserve original format: legacy uses just "hide", newer uses "hide yes/no"
                if self.hide && self.hide_legacy_format {
                    Some(Sexpr::symbol("hide"))
                } else if self.hide {
                    Some(Sexpr::bool_with_name("hide", true))
                } else {
                    None
                },
            ],
        )
    }
}

/// Fast version of LibSymbolSubUnit
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[derive(Debug, PartialEq, Clone)]
pub struct LibSymbolSubUnitFast {
    pub id: UnitId,
    pub unit_name: Option<String>,
    pub graphic_items: Vec<LibSymbolGraphicsItem>,
    pub pins: Vec<Pin>,
}

impl<'a> FromSexprRef<'a> for LibSymbolSubUnitFast {
    fn from_sexpr_ref(mut parser: ParserRef<'a>) -> Result<Self, KiCadParseError> {
        parser.expect_symbol_matching("symbol")?;
        
        let id_str = parser.expect_string()?;
        let id = id_str.parse::<UnitId>()?;
        let unit_name = parser.maybe_string_with_name("unit_name").map(|s| s.to_string());
        
        // For now, skip parsing graphic items and pins - this would need more work
        let graphic_items = vec![];
        let pins = vec![];
        
        // Skip remaining content
        while parser.peek_next().is_some() {
            parser.expect_next()?;
        }
        
        Ok(Self {
            id,
            unit_name,
            graphic_items,
            pins,
        })
    }
}

impl<'a> MaybeFromSexprRef<'a> for LibSymbolSubUnitFast {
    fn is_present_ref(sexpr: &'a SexprList) -> bool {
        sexpr.first_symbol().is_some_and(|s| s == "symbol")
    }
}

impl ToSexpr for LibSymbolSubUnitFast {
    fn to_sexpr(&self) -> Sexpr {
        Sexpr::list_with_name(
            "symbol",
            [
                &[
                    Some(self.id.to_sexpr()),
                    self.unit_name
                        .as_ref()
                        .map(|s| Sexpr::string_with_name("unit_name", s)),
                ][..],
                &self.graphic_items.into_sexpr_vec(),
                &self.pins.into_sexpr_vec(),
            ]
            .concat(),
        )
    }
}

/// Parse a symbol library file using the fast parser
pub fn parse_symbol_library_file_fast(input: &str) -> Result<SymbolLibraryFileFast, KiCadParseError> {
    let sexpr = kicad_sexpr::from_str(input)?;
    
    let Some(list) = sexpr.as_list() else {
        return Err(KiCadParseError::UnexpectedSexprType {
            expected: crate::SexprKind::List,
        });
    };
    
    SymbolLibraryFileFast::from_sexpr_ref(ParserRef::new(list))
}

/// Convert fast symbol library to regular symbol library
impl From<SymbolLibraryFileFast> for crate::symbol_library::SymbolLibraryFile {
    fn from(fast: SymbolLibraryFileFast) -> Self {
        Self {
            version: fast.version,
            generator: fast.generator,
            generator_is_string: fast.generator_is_string,
            generator_version: fast.generator_version,
            symbols: fast.symbols.into_iter().map(|s| s.into()).collect(),
        }
    }
}

impl From<SymbolDefinitionFast> for crate::symbol_library::SymbolDefinition {
    fn from(fast: SymbolDefinitionFast) -> Self {
        match fast {
            SymbolDefinitionFast::RootSymbol(s) => {
                crate::symbol_library::SymbolDefinition::RootSymbol(s.into())
            }
            SymbolDefinitionFast::DerivedSymbol(s) => {
                crate::symbol_library::SymbolDefinition::DerivedSymbol(s.into())
            }
        }
    }
}

impl From<LibSymbolFast> for LibSymbol {
    fn from(fast: LibSymbolFast) -> Self {
        Self {
            id: fast.id,
            power: fast.power,
            hide_pin_numbers: fast.hide_pin_numbers,
            pin_names: fast.pin_names.map(|p| p.into()),
            exclude_from_sim: fast.exclude_from_sim,
            in_bom: fast.in_bom,
            on_board: fast.on_board,
            properties: fast.properties.into_iter().map(|p| p.into()).collect(),
            graphic_items: fast.graphic_items,
            pins: fast.pins,
            units: fast.units.into_iter().map(|u| u.into()).collect(),
            embedded_fonts: fast.embedded_fonts,
        }
    }
}

impl From<DerivedLibSymbolFast> for crate::symbol_library::DerivedLibSymbol {
    fn from(fast: DerivedLibSymbolFast) -> Self {
        Self {
            id: fast.id,
            extends: fast.extends,
            properties: fast.properties.into_iter().map(|p| p.into()).collect(),
        }
    }
}

impl From<SymbolPropertyFast> for SymbolProperty {
    fn from(fast: SymbolPropertyFast) -> Self {
        Self {
            key: fast.key.to_string(),
            value: fast.value,
            position: fast.position,
            show_name: fast.show_name,
            do_not_autoplace: fast.do_not_autoplace,
            effects: fast.effects,
            legacy_id: fast.legacy_id,
        }
    }
}

impl From<PinNamesFast> for PinNames {
    fn from(fast: PinNamesFast) -> Self {
        Self {
            offset: fast.offset,
            hide: fast.hide,
            hide_legacy_format: fast.hide_legacy_format,
        }
    }
}

impl From<LibSymbolSubUnitFast> for LibSymbolSubUnit {
    fn from(fast: LibSymbolSubUnitFast) -> Self {
        Self {
            id: fast.id,
            unit_name: fast.unit_name,
            graphic_items: fast.graphic_items,
            pins: fast.pins,
        }
    }
}