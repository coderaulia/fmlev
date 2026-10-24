use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use crate::error::{Il2CppError, Result};

pub const IL2CPP_METADATA_MAGIC: u32 = 0xFAB11BAF;

#[derive(Debug, Clone)]
pub struct MetadataHeader {
    pub magic: u32,
    pub version: u32,
    pub string_literal_offset: u32,
    pub string_literal_size: u32,
    pub string_offset: u32,
    pub string_size: u32,
    pub events_offset: u32,
    pub events_size: u32,
    pub properties_offset: u32,
    pub properties_size: u32,
    pub methods_offset: u32,
    pub methods_size: u32,
    pub parameter_offset: u32,
    pub parameter_size: u32,
    pub field_offset: u32,
    pub field_size: u32,
    pub type_definitions_offset: u32,
    pub type_definitions_size: u32,
}

impl MetadataHeader {
    pub fn read_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = File::open(path)?;
        let mut buf = [0u8; 4];

        file.read_exact(&mut buf)?;
        let magic = u32::from_le_bytes(buf);
        if magic != IL2CPP_METADATA_MAGIC {
            return Err(Il2CppError::InvalidMagic(magic));
        }

        file.read_exact(&mut buf)?;
        let version = u32::from_le_bytes(buf);

        let read_u32 = |f: &mut File| -> Result<u32> {
            let mut b = [0u8; 4];
            f.read_exact(&mut b)?;
            Ok(u32::from_le_bytes(b))
        };

        let string_literal_offset = read_u32(&mut file)?;
        let string_literal_size = read_u32(&mut file)?;
        let _string_literal_data_offset = read_u32(&mut file)?;
        let _string_literal_data_size = read_u32(&mut file)?;
        let string_offset = read_u32(&mut file)?;
        let string_size = read_u32(&mut file)?;
        let events_offset = read_u32(&mut file)?;
        let events_size = read_u32(&mut file)?;
        let properties_offset = read_u32(&mut file)?;
        let properties_size = read_u32(&mut file)?;
        let methods_offset = read_u32(&mut file)?;
        let methods_size = read_u32(&mut file)?;
        let parameter_offset = read_u32(&mut file)?;
        let parameter_size = read_u32(&mut file)?;
        let field_offset = read_u32(&mut file)?;
        let field_size = read_u32(&mut file)?;

        // Seek forward to type_definitions
        file.seek(SeekFrom::Start(0x68))?;
        let type_definitions_offset = read_u32(&mut file)?;
        let type_definitions_size = read_u32(&mut file)?;

        Ok(Self {
            magic,
            version,
            string_literal_offset,
            string_literal_size,
            string_offset,
            string_size,
            events_offset,
            events_size,
            properties_offset,
            properties_size,
            methods_offset,
            methods_size,
            parameter_offset,
            parameter_size,
            field_offset,
            field_size,
            type_definitions_offset,
            type_definitions_size,
        })
    }
}
