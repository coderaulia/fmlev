use fmlev_mem::LinuxMemoryDriver;
use crate::error::{Il2CppError, Result};
use crate::registry::SingletonOffset;

/// Reads a null-terminated UTF-8 / ASCII C-string from remote memory.
pub fn read_cstring(driver: &LinuxMemoryDriver, addr: usize, max_len: usize) -> Result<String> {
    if addr == 0 {
        return Ok(String::new());
    }

    let buf_len = max_len.min(1024);
    let mut buf = vec![0u8; buf_len];

    match driver.read_bytes(addr, &mut buf) {
        Ok(n) if n > 0 => {
            let len = buf[..n].iter().position(|&b| b == 0).unwrap_or(n);
            Ok(String::from_utf8_lossy(&buf[..len]).to_string())
        }
        _ => Ok(String::new()),
    }
}

/// Standard Unity IL2CPP Managed Object Header
#[derive(Debug, Clone, Copy)]
pub struct Il2CppObject {
    pub klass: usize,
    pub monitor: usize,
}

impl Il2CppObject {
    pub const HEADER_SIZE: usize = 0x10;

    pub fn read(driver: &LinuxMemoryDriver, addr: usize) -> Result<Self> {
        let klass = driver.read_ptr(addr)?;
        let monitor = driver.read_ptr(addr + 0x08)?;
        Ok(Self { klass, monitor })
    }
}

/// Standard Unity IL2CPP Class metadata representation in memory
#[derive(Debug, Clone)]
pub struct Il2CppClass {
    pub address: usize,
    pub name_ptr: usize,
    pub namespaze_ptr: usize,
    pub static_fields_ptr: usize,
    pub name: String,
    pub namespaze: String,
}

impl Il2CppClass {
    pub const NAME_OFFSET: usize = 0x10;
    pub const NAMESPAZE_OFFSET: usize = 0x18;
    pub const STATIC_FIELDS_OFFSET: usize = 0xB8;

    pub fn read(driver: &LinuxMemoryDriver, addr: usize) -> Result<Self> {
        if addr == 0 {
            return Err(Il2CppError::NullPointer(addr));
        }

        let name_ptr = driver.read_ptr(addr + Self::NAME_OFFSET)?;
        let namespaze_ptr = driver.read_ptr(addr + Self::NAMESPAZE_OFFSET)?;
        let static_fields_ptr = driver.read_ptr(addr + Self::STATIC_FIELDS_OFFSET).unwrap_or(0);

        let name = read_cstring(driver, name_ptr, 256).unwrap_or_default();
        let namespaze = read_cstring(driver, namespaze_ptr, 256).unwrap_or_default();

        Ok(Self {
            address: addr,
            name_ptr,
            namespaze_ptr,
            static_fields_ptr,
            name,
            namespaze,
        })
    }

    /// Read a value from this class's static fields.
    pub fn read_static_field<T: Copy>(&self, driver: &LinuxMemoryDriver, offset: usize) -> Result<T> {
        if self.static_fields_ptr == 0 {
            return Err(Il2CppError::NullPointer(0));
        }
        driver.read::<T>(self.static_fields_ptr + offset).map_err(Into::into)
    }

    /// Read an object pointer from this class's static fields.
    pub fn read_static_field_ptr(&self, driver: &LinuxMemoryDriver, offset: usize) -> Result<usize> {
        self.read_static_field::<usize>(driver, offset)
    }
}

/// Unity IL2CPP UTF-16 String
pub struct Il2CppString;

impl Il2CppString {
    pub const LENGTH_OFFSET: usize = 0x10;
    pub const CHARS_OFFSET: usize = 0x14;

    pub fn read(driver: &LinuxMemoryDriver, addr: usize) -> Result<String> {
        let len: i32 = driver.read(addr + Self::LENGTH_OFFSET)?;
        if len <= 0 || len > 2048 {
            return Ok(String::new());
        }

        let mut u16_buf = vec![0u16; len as usize];
        let byte_len = len as usize * 2;
        let slice = unsafe {
            std::slice::from_raw_parts_mut(u16_buf.as_mut_ptr() as *mut u8, byte_len)
        };

        driver.read_bytes(addr + Self::CHARS_OFFSET, slice)?;
        Ok(String::from_utf16_lossy(&u16_buf))
    }

    pub fn write(driver: &LinuxMemoryDriver, addr: usize, text: &str) -> Result<()> {
        let u16_chars: Vec<u16> = text.encode_utf16().collect();
        let len = u16_chars.len() as i32;

        driver.write(addr + Self::LENGTH_OFFSET, &len)?;

        let byte_len = u16_chars.len() * 2;
        let slice = unsafe {
            std::slice::from_raw_parts(u16_chars.as_ptr() as *const u8, byte_len)
        };
        driver.write_bytes(addr + Self::CHARS_OFFSET, slice)?;
        Ok(())
    }
}

/// Unity IL2CPP Array<T>
pub struct Il2CppArray;

impl Il2CppArray {
    pub const MAX_LENGTH_OFFSET: usize = 0x18;
    pub const ELEMENTS_OFFSET: usize = 0x20;

    pub fn length(driver: &LinuxMemoryDriver, addr: usize) -> Result<usize> {
        let len: usize = driver.read(addr + Self::MAX_LENGTH_OFFSET)?;
        Ok(len)
    }

    pub fn element_ptr(addr: usize, index: usize) -> usize {
        addr + Self::ELEMENTS_OFFSET + (index * 8)
    }
}

/// Resolves an RVA or IDA image-base relative virtual address against a loaded module base.
/// If `rva_or_va >= 0x180000000` (standard 64-bit IDA default PE base), strips the IDA image base.
pub fn resolve_rva(module_base: usize, rva_or_va: usize) -> usize {
    const IDA_PE64_IMAGE_BASE: usize = 0x180000000;
    let rva = if rva_or_va >= IDA_PE64_IMAGE_BASE {
        rva_or_va - IDA_PE64_IMAGE_BASE
    } else {
        rva_or_va
    };
    module_base.wrapping_add(rva)
}

/// Resolves a singleton or global pointer by RVA and pointer chain.
pub fn resolve_singleton(
    driver: &LinuxMemoryDriver,
    module_base: usize,
    rva: usize,
    pointer_chain: &[usize],
) -> Result<usize> {
    let target = resolve_rva(module_base, rva);
    if pointer_chain.is_empty() {
        driver.read_ptr(target).map_err(Into::into)
    } else {
        let initial = driver.read_ptr(target)?;
        if initial == 0 {
            return Err(Il2CppError::NullPointer(target));
        }
        let mut current = initial;
        for &offset in pointer_chain {
            if current == 0 {
                return Err(Il2CppError::NullPointer(current));
            }
            current = driver.read_ptr(current.wrapping_add(offset))?;
        }
        Ok(current)
    }
}

/// Reads a static singleton instance defined by a SingletonOffset descriptor.
pub fn read_static_singleton(
    driver: &LinuxMemoryDriver,
    module_base: usize,
    singleton: &SingletonOffset,
) -> Result<usize> {
    let rva = singleton.parse_rva();
    let chain = singleton.parse_pointer_chain();
    resolve_singleton(driver, module_base, rva, &chain)
}

/// Reads a singleton instance from an Il2CppClass static fields pointer at the given offset.
pub fn read_static_singleton_instance(
    driver: &LinuxMemoryDriver,
    class_addr: usize,
    field_offset: usize,
) -> Result<usize> {
    let class = Il2CppClass::read(driver, class_addr)?;
    class.read_static_field_ptr(driver, field_offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_rva() {
        let base = 0x7f000000;
        let ida_va = 0x18218e590;
        let expected = base + 0x218e590;
        assert_eq!(resolve_rva(base, ida_va), expected);

        let raw_rva = 0x5000;
        assert_eq!(resolve_rva(base, raw_rva), base + 0x5000);
    }

    #[test]
    fn test_il2cpp_class_memory_read() {
        let my_pid = std::process::id() as i32;
        let driver = LinuxMemoryDriver::new(my_pid);

        let class_name = b"WorldClass\0";
        let namespace = b"SportsInteractive.Football\0";

        // Simulated Il2CppClass buffer (at least 0xC0 bytes)
        let mut fake_class_bytes = vec![0u8; 0x100];
        let name_ptr = class_name.as_ptr() as usize;
        let namespaze_ptr = namespace.as_ptr() as usize;
        let fake_static_fields_ptr = 0x12345678usize;

        fake_class_bytes[0x10..0x18].copy_from_slice(&name_ptr.to_ne_bytes());
        fake_class_bytes[0x18..0x20].copy_from_slice(&namespaze_ptr.to_ne_bytes());
        fake_class_bytes[0xB8..0xC0].copy_from_slice(&fake_static_fields_ptr.to_ne_bytes());

        let class_addr = fake_class_bytes.as_ptr() as usize;
        let klass = Il2CppClass::read(&driver, class_addr).unwrap();

        assert_eq!(klass.name, "WorldClass");
        assert_eq!(klass.namespaze, "SportsInteractive.Football");
        assert_eq!(klass.static_fields_ptr, fake_static_fields_ptr);
    }

    #[test]
    fn test_resolve_singleton_chain() {
        let my_pid = std::process::id() as i32;
        let driver = LinuxMemoryDriver::new(my_pid);

        let target_object: u64 = 0xCAFEBABE;
        let target_ptr = &target_object as *const u64 as usize;

        // Pointer chain: base -> ptr1 (with +0x18 offset) -> target
        let mut level1 = vec![0usize; 8];
        level1[3] = target_ptr; // 3 * 8 = 0x18 bytes offset
        let level1_ptr = level1.as_ptr() as usize;

        let root_var = level1_ptr;
        let root_addr = &root_var as *const usize as usize;

        let resolved = resolve_singleton(&driver, root_addr, 0, &[0x18]).unwrap();
        assert_eq!(resolved, target_ptr);
    }
}
