//! String interner for reducing memory usage of repeated strings

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct InternedString(Cow<'static, str>);

impl InternedString {
    pub fn new(s: impl Into<Cow<'static, str>>) -> Self {
        Self(s.into())
    }
    
    pub fn as_str(&self) -> &str {
        &self.0
    }
    
    pub fn from_static(s: &'static str) -> Self {
        Self(Cow::Borrowed(s))
    }
    
    pub fn from_string(s: String) -> Self {
        Self(Cow::Owned(s))
    }
}

impl std::ops::Deref for InternedString {
    type Target = str;
    
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::fmt::Display for InternedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&'static str> for InternedString {
    fn from(s: &'static str) -> Self {
        Self::from_static(s)
    }
}

impl From<String> for InternedString {
    fn from(s: String) -> Self {
        Self::from_string(s)
    }
}

// Global string interner for common property keys
static PROPERTY_KEY_INTERNER: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();

fn init_property_keys() -> HashMap<&'static str, &'static str> {
    let mut map = HashMap::new();
    map.insert("Value", "Value");
    map.insert("Reference", "Reference");
    map.insert("Footprint", "Footprint");
    map.insert("Datasheet", "Datasheet");
    map.insert("ki_keywords", "ki_keywords");
    map.insert("ki_description", "ki_description");
    map.insert("ki_fp_filters", "ki_fp_filters");
    map.insert("D", "D");
    map.insert("in_bom", "in_bom");
    map.insert("on_board", "on_board");
    map.insert("pin_numbers", "pin_numbers");
    map.insert("power", "power");
    map.insert("extends", "extends");
    map
}

pub fn intern_property_key(key: &str) -> InternedString {
    let interner = PROPERTY_KEY_INTERNER.get_or_init(init_property_keys);
    
    if let Some(&static_key) = interner.get(key) {
        InternedString::from_static(static_key)
    } else {
        InternedString::from_string(key.to_string())
    }
}