//! Small RTTI helpers for objects that have no fixed vtable RVA in this repo (competition stages,
//! standings, awards). A type is named by the MSVC type descriptor behind the object's vtable, so no
//! address is compiled in; the layouts built on top are still guarded by the PE timestamp.
//!
//! Test processes have no `game_plugin.dll`: `ModuleImage::discover` returns `None` there and the
//! callers then skip the RTTI pin (same convention as the other native modules).

use fmlev_mem::{parse_process_maps, LinuxMemoryDriver};

use crate::error::{CoreError, Result};
use crate::native_catalog::{is_complete_object_locator, pe_image_size, type_name};

/// Build the stage/league/award layouts were read from.
pub const PE_TIMESTAMP: u32 = 0x6a22_9e8a;
const CHUNK: usize = 16 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleImage {
    pub base: usize,
    pub size: usize,
}

impl ModuleImage {
    pub fn new(base: usize, size: usize) -> Self {
        Self { base, size }
    }

    /// The mapped `game_plugin.dll` after the PE timestamp guard; `Ok(None)` when no module is mapped (tests).
    pub fn discover(driver: &LinuxMemoryDriver) -> Result<Option<Self>> {
        let maps = parse_process_maps(driver.pid())?;
        let Some(base) = maps.iter().filter(|m| m.pathname.ends_with("game_plugin.dll")).map(|m| m.start).min() else {
            return Ok(None);
        };
        let size = pe_image_size(driver, base).ok_or_else(|| invalid("game_plugin.dll headers are not readable"))?;
        let pe = driver.read::<u32>(base + 0x3c)? as usize;
        let stamp = driver.read::<u32>(base + pe + 8)?;
        if stamp != PE_TIMESTAMP {
            return Err(invalid(&format!("Unknown game_plugin.dll build (PE timestamp {stamp:#x}); this native layout is unavailable")));
        }
        Ok(Some(Self { base, size }))
    }

    fn contains(&self, address: usize) -> bool {
        address >= self.base && address - self.base < self.size
    }

    /// MSVC type name (for example `.?AVTEAM@db@@`) of the object at `object`, from its vtable's RTTI.
    pub fn type_of(&self, driver: &LinuxMemoryDriver, object: usize) -> Option<String> {
        let vtable = driver.read_ptr(object).ok()?;
        self.type_of_vtable(driver, vtable)
    }

    pub fn type_of_vtable(&self, driver: &LinuxMemoryDriver, vtable: usize) -> Option<String> {
        if !self.contains(vtable) || vtable < 8 {
            return None;
        }
        let locator = driver.read_ptr(vtable - 8).ok()?;
        if !self.contains(locator) {
            return None;
        }
        let col = driver.read::<[u32; 6]>(locator).ok()?;
        if !is_complete_object_locator(&col, locator, self.base) {
            return None;
        }
        type_name(driver, self.base + col[3] as usize)
    }

    /// True when the object at `object` has exactly this RTTI type name.
    pub fn is_type(&self, driver: &LinuxMemoryDriver, object: usize, name: &str) -> bool {
        self.type_of(driver, object).as_deref() == Some(name)
    }

    /// Primary vtable address of the type `name` (complete-object-locator offset 0). Exactly one must exist.
    pub fn find_vtable(&self, driver: &LinuxMemoryDriver, name: &str) -> Result<usize> {
        let maps = parse_process_maps(driver.pid())?;
        let mut found = Vec::new();
        // Proton maps PE sections anonymously: every read-only, non-executable mapping inside the image counts.
        for map in maps.iter().filter(|m| m.start >= self.base && m.end <= self.base + self.size && m.is_readable() && !m.is_writable() && !m.is_executable()) {
            let mut start = map.start;
            while start < map.end {
                let len = (map.end - start).min(CHUNK);
                let mut buf = vec![0u8; len];
                if driver.read_bytes(start, &mut buf).is_ok() {
                    for i in (0..len.saturating_sub(15)).step_by(8) {
                        let locator = usize::from_le_bytes(buf[i..i + 8].try_into().unwrap());
                        if !self.contains(locator) {
                            continue;
                        }
                        let Ok(col) = driver.read::<[u32; 6]>(locator) else { continue };
                        if !is_complete_object_locator(&col, locator, self.base) || col[1] != 0 {
                            continue;
                        }
                        if type_name(driver, self.base + col[3] as usize).as_deref() == Some(name) {
                            found.push(start + i + 8);
                        }
                    }
                }
                start += len;
            }
        }
        match found.as_slice() {
            [vtable] => Ok(*vtable),
            other => Err(invalid(&format!("Expected one vtable for {name}, found {}", other.len()))),
        }
    }
}

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidValue(message.to_owned())
}

/// Length-prefixed UTF-8 string (`u32 len`, bytes) at `pointer`; `None` when unreadable or implausible.
pub fn read_len_string(driver: &LinuxMemoryDriver, pointer: usize, max: usize) -> Option<String> {
    if pointer <= 0x10000 {
        return None;
    }
    let len = driver.read::<u32>(pointer).ok()? as usize;
    if len == 0 || len > max {
        return None;
    }
    let mut buf = vec![0u8; len];
    if driver.read_bytes(pointer + 4, &mut buf).ok()? != len {
        return None;
    }
    let text = String::from_utf8(buf).ok()?;
    (!text.chars().any(char::is_control)).then_some(text)
}

#[cfg(test)]
pub(crate) mod fake {
    //! An in-process fake `game_plugin.dll` image with type descriptors, locators and vtables.
    use super::*;

    pub struct FakeImage {
        pub buf: Vec<u64>,
        next_td: usize,
        next_col: usize,
        next_vt: usize,
    }

    impl FakeImage {
        pub fn new() -> Self {
            Self { buf: vec![0u64; 0x2000], next_td: 0x1000 * 8, next_col: 0x1400 * 8, next_vt: 0x1800 * 8 }
        }
        pub fn base(&self) -> usize {
            self.buf.as_ptr() as usize
        }
        pub fn image(&self) -> ModuleImage {
            ModuleImage::new(self.base(), self.buf.len() * 8)
        }
        fn bytes(&mut self) -> &mut [u8] {
            let len = self.buf.len() * 8;
            unsafe { std::slice::from_raw_parts_mut(self.buf.as_mut_ptr() as *mut u8, len) }
        }
        /// Registers a type and returns its vtable address (the locator pointer sits at vtable - 8).
        pub fn add_type(&mut self, name: &str) -> usize {
            let (td, col, vt) = (self.next_td, self.next_col, self.next_vt);
            self.next_td += 0x80;
            self.next_col += 0x40;
            self.next_vt += 0x20;
            let base = self.base();
            let bytes = self.bytes();
            bytes[td + 16..td + 16 + name.len()].copy_from_slice(name.as_bytes());
            let words: [u32; 6] = [1, 0, 0, td as u32, 0, col as u32];
            for (i, word) in words.iter().enumerate() {
                bytes[col + i * 4..col + i * 4 + 4].copy_from_slice(&word.to_le_bytes());
            }
            bytes[vt - 8..vt].copy_from_slice(&((base + col) as u64).to_le_bytes());
            base + vt
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::FakeImage;
    use super::*;

    #[test]
    fn type_names_come_from_the_vtable_locator() {
        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        let mut image = FakeImage::new();
        let team = image.add_type(".?AVTEAM@db@@");
        let object = Box::new([team as u64, 0, 0, 0]);
        let address = object.as_ptr() as usize;
        let module = image.image();
        assert_eq!(module.type_of(&driver, address).as_deref(), Some(".?AVTEAM@db@@"));
        assert!(module.is_type(&driver, address, ".?AVTEAM@db@@"));
        assert!(!module.is_type(&driver, address, ".?AVCLUB@db@@"));
        let stray = Box::new([0x1234u64, 0, 0, 0]);
        assert_eq!(module.type_of(&driver, stray.as_ptr() as usize), None);
    }

    #[test]
    fn length_prefixed_strings_reject_garbage() {
        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        let mut good = vec![0u8; 32];
        good[..4].copy_from_slice(&5u32.to_le_bytes());
        good[4..9].copy_from_slice(b"Award");
        assert_eq!(read_len_string(&driver, good.as_ptr() as usize, 64).as_deref(), Some("Award"));
        let mut bad = vec![0u8; 32];
        bad[..4].copy_from_slice(&999u32.to_le_bytes());
        assert_eq!(read_len_string(&driver, bad.as_ptr() as usize, 64), None);
        let mut ctl = vec![0u8; 32];
        ctl[..4].copy_from_slice(&2u32.to_le_bytes());
        ctl[4] = 7;
        assert_eq!(read_len_string(&driver, ctl.as_ptr() as usize, 64), None);
    }
}
