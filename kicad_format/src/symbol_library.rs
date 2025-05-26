//! Symbol library file format (`.kicad_sym` files)

use kicad_sexpr::Sexpr;

use crate::{
    common::symbol::{LibSymbol, LibraryId, SymbolProperty},
    convert::{FromSexpr, Parser, SexprListExt, ToSexpr, VecToMaybeSexprVec},
    simple_maybe_from_sexpr, KiCadParseError,
};

/// Stores a collection of symbols which may or may not be derived from other
/// symbols within the library
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[derive(Debug, PartialEq, Clone)]
pub struct SymbolLibraryFile {
    /// The `version` token attribute defines the symbol library version using
    /// the YYYYMMDD date format.
    pub version: u32,
    /// The `generator` token attribute defines the program used to write the file.
    pub generator: String,
    /// Whether the generator was originally a string (newer format) or symbol (legacy format)
    pub generator_is_string: bool,
    /// Optional generator version for newer KiCad formats
    pub generator_version: Option<String>,
    /// The symbol definitions go here. Symbol library files can have zero or more symbols.
    pub symbols: Vec<SymbolDefinition>,
}

impl FromSexpr for SymbolLibraryFile {
    fn from_sexpr(mut parser: Parser) -> Result<Self, KiCadParseError> {
        parser.expect_symbol_matching("kicad_symbol_lib")?;

        let version = parser.expect_number_with_name("version")? as u32;
        // Handle both old format (symbol) and new format (string) for generator
        let (generator, generator_is_string) = {
            let mut gen_parser = parser.expect_list_with_name("generator")?;
            // Peek to see if it's a string or symbol
            let next = gen_parser.peek_next();
            if let Some(sexpr) = next {
                match sexpr {
                    kicad_sexpr::Sexpr::String(_) => {
                        let s = gen_parser.expect_string()?;
                        gen_parser.expect_end()?;
                        (s, true)
                    }
                    kicad_sexpr::Sexpr::Symbol(_) => {
                        let s = gen_parser.expect_symbol()?;
                        gen_parser.expect_end()?;
                        (s, false)
                    }
                    _ => return Err(KiCadParseError::UnexpectedSexprType { expected: crate::SexprKind::String }),
                }
            } else {
                return Err(KiCadParseError::UnexpectedEndOfList);
            }
        };
        let generator_version = parser.maybe_string_with_name("generator_version")?;
        let symbols = parser.expect_many::<SymbolDefinition>()?;

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

impl ToSexpr for SymbolLibraryFile {
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
                    self.generator_version.as_ref().map(|v| Sexpr::string_with_name("generator_version", v)),
                ][..],
                &self.symbols.into_sexpr_vec(),
            ]
            .concat(),
        )
    }
}

/// A symbol definition can be a root symbol or a derived symbol
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "serde", serde(tag = "type"))]
#[derive(Debug, PartialEq, Clone)]
pub enum SymbolDefinition {
    RootSymbol(LibSymbol),
    DerivedSymbol(DerivedLibSymbol),
}

impl FromSexpr for SymbolDefinition {
    fn from_sexpr(mut parser: Parser) -> Result<Self, KiCadParseError> {
        // We need to look ahead to determine if this is a derived symbol or a
        // root symbol, so we make a copy of the original parser. That way, we
        // can do out initial inspection to determine the symbol type (which
        // consumes tokens), and then send it to the correct FromSexpr
        // implementation later.
        let backup = parser.clone();

        // Both types will have a `symbol` token and a string ID.
        parser.expect_symbol_matching("symbol")?;
        let _id = parser.expect_string()?.parse::<LibraryId>()?;

        // The derived symbol will have an `extends` token and a string ID.
        let extends = parser.maybe_string_with_name("extends")?;

        Ok(match extends {
            Some(_) => Self::DerivedSymbol(DerivedLibSymbol::from_sexpr(backup)?),
            None => Self::RootSymbol(LibSymbol::from_sexpr(backup)?),
        })
    }
}

simple_maybe_from_sexpr!(SymbolDefinition, symbol);

impl ToSexpr for SymbolDefinition {
    fn to_sexpr(&self) -> Sexpr {
        match self {
            Self::RootSymbol(symbol) => symbol.to_sexpr(),
            Self::DerivedSymbol(symbol) => symbol.to_sexpr(),
        }
    }
}

/// A symbol which has been derived from another (root) symbol within the
/// library
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[derive(Debug, PartialEq, Clone)]
pub struct DerivedLibSymbol {
    pub id: LibraryId,
    pub extends: String,
    pub properties: Vec<SymbolProperty>,
}

impl FromSexpr for DerivedLibSymbol {
    fn from_sexpr(mut parser: Parser) -> Result<Self, KiCadParseError> {
        parser.expect_symbol_matching("symbol")?;

        let id = parser.expect_string()?.parse::<LibraryId>()?;
        let extends = parser.expect_string_with_name("extends")?;
        let properties = parser.expect_many::<SymbolProperty>()?;

        parser.expect_end()?;

        Ok(Self {
            id,
            extends,
            properties,
        })
    }
}

impl ToSexpr for DerivedLibSymbol {
    fn to_sexpr(&self) -> Sexpr {
        Sexpr::list_with_name(
            "symbol",
            [
                &[
                    Some(self.id.to_sexpr()),
                    Some(Sexpr::string_with_name("extends", &self.extends)),
                ][..],
                &self.properties.into_sexpr_vec(),
            ]
            .concat(),
        )
    }
}
