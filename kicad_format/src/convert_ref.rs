//! Reference-based conversion traits for zero-copy parsing

use std::slice::Iter;
use kicad_sexpr::{Sexpr, SexprList};
use crate::{KiCadParseError, SexprKind};

pub trait FromSexprRef<'a>: Sized {
    fn from_sexpr_ref(parser: ParserRef<'a>) -> Result<Self, KiCadParseError>;
}

pub trait MaybeFromSexprRef<'a> {
    fn is_present_ref(sexpr: &'a SexprList) -> bool;
}

/// A zero-copy parser that works with references
#[derive(Debug, Clone)]
pub struct ParserRef<'a> {
    inner: std::iter::Peekable<Iter<'a, Sexpr>>,
}

impl<'a> ParserRef<'a> {
    pub fn new(inner: &'a SexprList) -> Self {
        Self {
            inner: inner.iter().peekable(),
        }
    }
    
    pub fn expect_next(&mut self) -> Result<&'a Sexpr, KiCadParseError> {
        self.inner
            .next()
            .ok_or(KiCadParseError::UnexpectedEndOfList)
    }
    
    pub fn peek_next(&mut self) -> Option<&&'a Sexpr> {
        self.inner.peek()
    }
    
    pub fn expect_list(&mut self) -> Result<ParserRef<'a>, KiCadParseError> {
        let next = self.expect_next()?;
        
        let Sexpr::List(list) = next else {
            return Err(KiCadParseError::UnexpectedSexprType {
                expected: SexprKind::List,
            });
        };
        
        Ok(ParserRef::new(list))
    }
    
    pub fn expect_list_with_name(&mut self, name: &str) -> Result<ParserRef<'a>, KiCadParseError> {
        let mut list = self.expect_list()?;
        list.expect_symbol_matching(name)?;
        Ok(list)
    }
    
    pub fn maybe_list_with_name(&mut self, name: &str) -> Option<ParserRef<'a>> {
        let Some(&sexpr) = self.peek_next() else {
            return None;
        };
        
        let Sexpr::List(list) = sexpr else {
            return None;
        };
        
        let first_symbol = list.first()?.as_symbol()?;
        if first_symbol == name {
            self.expect_list_with_name(name).ok()
        } else {
            None
        }
    }
    
    pub fn expect_symbol(&mut self) -> Result<&'a str, KiCadParseError> {
        let next = self.expect_next()?;
        
        let Sexpr::Symbol(symbol) = next else {
            return Err(KiCadParseError::UnexpectedSexprType {
                expected: SexprKind::Symbol,
            });
        };
        
        Ok(symbol.as_str())
    }
    
    pub fn expect_symbol_matching(&mut self, expected: &str) -> Result<(), KiCadParseError> {
        let symbol = self.expect_symbol()?;
        
        if symbol != expected {
            return Err(KiCadParseError::NonMatchingSymbol {
                found: symbol.to_string(),
                expected: expected.to_string(),
            });
        }
        
        Ok(())
    }
    
    pub fn expect_string(&mut self) -> Result<&'a str, KiCadParseError> {
        let next = self.expect_next()?;
        
        let Sexpr::String(string) = next else {
            return Err(KiCadParseError::UnexpectedSexprType {
                expected: SexprKind::String,
            });
        };
        
        Ok(string.as_str())
    }
    
    pub fn expect_string_with_name(&mut self, name: &str) -> Result<&'a str, KiCadParseError> {
        self.expect_list_with_name(name)?.expect_string()
    }
    
    pub fn maybe_string_with_name(&mut self, name: &str) -> Option<&'a str> {
        self.maybe_list_with_name(name)?.expect_string().ok()
    }
    
    pub fn expect_number(&mut self) -> Result<f32, KiCadParseError> {
        let next = self.expect_next()?;
        
        let Sexpr::Number(number) = next else {
            return Err(KiCadParseError::UnexpectedSexprType {
                expected: SexprKind::Number,
            });
        };
        
        Ok(*number)
    }
    
    pub fn expect_bool_with_name(&mut self, name: &str) -> Result<bool, KiCadParseError> {
        let mut list = self.expect_list_with_name(name)?;
        let result = list.expect_symbol()?;
        
        match result {
            "yes" => Ok(true),
            "no" => Ok(false),
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(KiCadParseError::InvalidEnumValue {
                value: result.to_string(),
                enum_name: "bool",
            }),
        }
    }
    
    pub fn maybe_empty_list_with_name(&mut self, name: &str) -> bool {
        if let Some(list) = self.maybe_list_with_name(name) {
            list.expect_end().is_ok()
        } else {
            false
        }
    }
    
    pub fn expect_end(mut self) -> Result<(), KiCadParseError> {
        if let Some(next) = self.inner.next() {
            return Err(KiCadParseError::ExpectedEndOfList { 
                found: next.clone() 
            });
        }
        
        Ok(())
    }
    
    pub fn expect_number_with_name(&mut self, name: &str) -> Result<f32, KiCadParseError> {
        self.expect_list_with_name(name)?.expect_number()
    }
    
    pub fn maybe_number(&mut self) -> Option<f32> {
        if let Some(&Sexpr::Number(n)) = self.peek_next() {
            self.inner.next();
            Some(*n)
        } else {
            None
        }
    }
    
    pub fn expect_symbol_with_name(&mut self, name: &str) -> Result<&'a str, KiCadParseError> {
        self.expect_list_with_name(name)?.expect_symbol()
    }
    
    pub fn maybe_bool_with_name(&mut self, name: &str) -> Result<Option<bool>, KiCadParseError> {
        self.maybe_list_with_name(name)
            .map(|mut d| {
                let result = d.expect_symbol()?;
                match result {
                    "yes" => Ok(true),
                    "no" => Ok(false),
                    _ => Err(KiCadParseError::InvalidEnumValue {
                        value: result.to_string(),
                        enum_name: "bool",
                    }),
                }
            })
            .transpose()
    }
    
    pub fn maybe_number_with_name(&mut self, name: &str) -> Option<f32> {
        self.maybe_list_with_name(name)?.expect_number().ok()
    }
    
    pub fn maybe_symbol_matching(&mut self, expected: &str) -> bool {
        if let Some(&sexpr) = self.peek_next() {
            if let Sexpr::Symbol(symbol) = sexpr {
                if symbol.as_str() == expected {
                    self.inner.next();
                    return true;
                }
            }
        }
        false
    }
    
    pub fn expect<T>(&mut self) -> Result<T, KiCadParseError>
    where
        T: FromSexprRef<'a>,
    {
        T::from_sexpr_ref(self.expect_list()?)
    }
    
    pub fn maybe<T>(&mut self) -> Result<Option<T>, KiCadParseError>
    where
        T: FromSexprRef<'a> + MaybeFromSexprRef<'a>,
    {
        let Some(&sexpr) = self.peek_next() else {
            return Ok(None);
        };
        
        let Sexpr::List(list) = sexpr else {
            return Ok(None);
        };
        
        T::is_present_ref(list).then(|| self.expect::<T>()).transpose()
    }
    
    pub fn expect_many<T>(&mut self) -> Result<Vec<T>, KiCadParseError>
    where
        T: FromSexprRef<'a> + MaybeFromSexprRef<'a>,
    {
        let mut result = Vec::new();
        
        while let Some(item) = self.maybe::<T>()? {
            result.push(item);
        }
        
        Ok(result)
    }
}