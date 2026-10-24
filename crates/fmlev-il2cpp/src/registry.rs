use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use crate::error::{Il2CppError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OffsetField {
    pub offset: String,
    #[serde(rename = "type")]
    pub field_type: String,
}

impl OffsetField {
    pub fn parse_offset(&self) -> usize {
        if let Some(stripped) = self.offset.strip_prefix("0x") {
            usize::from_str_radix(stripped, 16).unwrap_or(0)
        } else {
            self.offset.parse::<usize>().unwrap_or(0)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassOffsets {
    pub class_name: String,
    pub fields: HashMap<String, OffsetField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingletonOffset {
    pub base_module: String,
    pub rva: String,
    pub pointer_chain: Vec<String>,
    pub aob_signature: Option<String>,
}

impl SingletonOffset {
    pub fn parse_rva(&self) -> usize {
        if let Some(stripped) = self.rva.strip_prefix("0x") {
            usize::from_str_radix(stripped, 16).unwrap_or(0)
        } else {
            self.rva.parse::<usize>().unwrap_or(0)
        }
    }

    pub fn parse_pointer_chain(&self) -> Vec<usize> {
        self.pointer_chain
            .iter()
            .map(|s| {
                if let Some(stripped) = s.strip_prefix("0x") {
                    usize::from_str_radix(stripped, 16).unwrap_or(0)
                } else {
                    s.parse::<usize>().unwrap_or(0)
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OffsetsProfile {
    pub version: String,
    pub singletons: HashMap<String, SingletonOffset>,
    pub classes: HashMap<String, ClassOffsets>,
}

#[derive(Debug, Clone, Default)]
pub struct OffsetsRegistry {
    pub active_profile: Option<OffsetsProfile>,
}

impl OffsetsRegistry {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        Self::load_from_json(&content)
    }

    pub fn load_from_json(json_str: &str) -> Result<Self> {
        let profile: OffsetsProfile = serde_json::from_str(json_str)?;
        Ok(Self {
            active_profile: Some(profile),
        })
    }

    pub fn get_field_offset(&self, class: &str, field: &str) -> Result<usize> {
        if let Some(profile) = &self.active_profile {
            if let Some(cls) = profile.classes.get(class) {
                if let Some(fld) = cls.fields.get(field) {
                    return Ok(fld.parse_offset());
                }
            }
        }
        Err(Il2CppError::OffsetNotFound(class.to_string(), field.to_string()))
    }

    pub fn get_singleton(&self, name: &str) -> Result<&SingletonOffset> {
        if let Some(profile) = &self.active_profile {
            if let Some(singleton) = profile.singletons.get(name) {
                return Ok(singleton);
            }
        }
        Err(Il2CppError::SingletonNotFound(name.to_string()))
    }

    pub fn get_class(&self, name: &str) -> Result<&ClassOffsets> {
        if let Some(profile) = &self.active_profile {
            if let Some(cls) = profile.classes.get(name) {
                return Ok(cls);
            }
        }
        Err(Il2CppError::OffsetNotFound(name.to_string(), "*".to_string()))
    }
}
