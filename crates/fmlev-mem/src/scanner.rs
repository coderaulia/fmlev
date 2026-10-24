use crate::driver::LinuxMemoryDriver;
use crate::error::{MemoryError, Result};
use crate::maps::{find_rw_mappings, parse_process_maps, MemoryMapping};

const SCAN_CHUNK_SIZE: usize = 256 * 1024; // 256 KB chunk size for memory reads

/// Represents a parsed Array of Bytes (AOB) signature pattern with optional wildcards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AobPattern {
    pub bytes: Vec<Option<u8>>,
}

impl AobPattern {
    /// Parses an AOB signature pattern string like:
    /// `"48 8B 05 ?? ?? ?? ??"` or `"48 89 5C 24 ? 48 89"`
    pub fn parse(pattern: &str) -> Result<Self> {
        let mut bytes = Vec::new();
        for token in pattern.split_whitespace() {
            if token == "?" || token == "??" {
                bytes.push(None);
            } else {
                match u8::from_str_radix(token, 16) {
                    Ok(b) => bytes.push(Some(b)),
                    Err(_) => {
                        return Err(MemoryError::InvalidPattern(format!(
                            "Invalid hex byte '{}' in pattern '{}'",
                            token, pattern
                        )))
                    }
                }
            }
        }

        if bytes.is_empty() {
            return Err(MemoryError::InvalidPattern("Empty AOB pattern".to_string()));
        }

        Ok(Self { bytes })
    }

    /// Checks if the pattern matches a slice of bytes.
    #[inline]
    pub fn matches(&self, slice: &[u8]) -> bool {
        if slice.len() < self.bytes.len() {
            return false;
        }
        for (i, opt) in self.bytes.iter().enumerate() {
            if let Some(expected) = opt {
                if slice[i] != *expected {
                    return false;
                }
            }
        }
        true
    }

    /// Scans a local buffer for all occurrences of this pattern, adding matching addresses to `results`.
    pub fn scan_buffer(&self, buffer: &[u8], base_addr: usize, results: &mut Vec<usize>) {
        let pat_len = self.bytes.len();
        if buffer.len() < pat_len {
            return;
        }

        let first_concrete = self
            .bytes
            .iter()
            .enumerate()
            .find_map(|(idx, &b)| b.map(|val| (idx, val)));

        if let Some((first_idx, first_val)) = first_concrete {
            let max_pos = buffer.len() - pat_len;
            let mut pos = 0;
            while pos <= max_pos {
                if buffer[pos + first_idx] == first_val && self.matches(&buffer[pos..pos + pat_len]) {
                    results.push(base_addr + pos);
                }
                pos += 1;
            }
        } else {
            // All wildcards: every offset matches
            let max_pos = buffer.len() - pat_len;
            for pos in 0..=max_pos {
                results.push(base_addr + pos);
            }
        }
    }
}

/// Scans a memory mapping for an AOB pattern using chunked reads with boundary overlap.
fn scan_region_aob(
    driver: &LinuxMemoryDriver,
    mapping: &MemoryMapping,
    pattern: &AobPattern,
    results: &mut Vec<usize>,
) {
    let pat_len = pattern.bytes.len();
    if pat_len == 0 || mapping.size() < pat_len {
        return;
    }

    let mut buf = vec![0u8; SCAN_CHUNK_SIZE];
    let overlap = pat_len - 1;

    let mut cur = mapping.start;
    while cur < mapping.end {
        let remaining = mapping.end - cur;
        let to_read = remaining.min(SCAN_CHUNK_SIZE);
        if to_read < pat_len {
            break;
        }

        match driver.read_bytes(cur, &mut buf[..to_read]) {
            Ok(n) if n >= pat_len => {
                pattern.scan_buffer(&buf[..n], cur, results);
                if n < to_read {
                    // Partial read, advance by n
                    cur += n;
                } else if to_read == remaining {
                    break;
                } else {
                    cur += to_read - overlap;
                }
            }
            _ => {
                // Inaccessible page or uncommitted memory, skip by page size (4KB)
                cur += 4096;
            }
        }
    }
}

/// Scans a local buffer for exact byte needle matches.
fn scan_buffer_exact(buffer: &[u8], needle: &[u8], base_addr: usize, results: &mut Vec<usize>) {
    let n_len = needle.len();
    if buffer.len() < n_len || n_len == 0 {
        return;
    }

    let first = needle[0];
    let max_pos = buffer.len() - n_len;
    let mut pos = 0;
    while pos <= max_pos {
        if buffer[pos] == first && &buffer[pos..pos + n_len] == needle {
            results.push(base_addr + pos);
        }
        pos += 1;
    }
}

/// Scans a memory mapping for exact byte needle matches using chunked reads.
fn scan_region_bytes(
    driver: &LinuxMemoryDriver,
    mapping: &MemoryMapping,
    needle: &[u8],
    results: &mut Vec<usize>,
) {
    let n_len = needle.len();
    if n_len == 0 || mapping.size() < n_len {
        return;
    }

    let mut buf = vec![0u8; SCAN_CHUNK_SIZE];
    let overlap = n_len - 1;

    let mut cur = mapping.start;
    while cur < mapping.end {
        let remaining = mapping.end - cur;
        let to_read = remaining.min(SCAN_CHUNK_SIZE);
        if to_read < n_len {
            break;
        }

        match driver.read_bytes(cur, &mut buf[..to_read]) {
            Ok(n) if n >= n_len => {
                scan_buffer_exact(&buf[..n], needle, cur, results);
                if n < to_read {
                    cur += n;
                } else if to_read == remaining {
                    break;
                } else {
                    cur += to_read - overlap;
                }
            }
            _ => {
                cur += 4096;
            }
        }
    }
}

/// High-performance memory scanner wrapping a LinuxMemoryDriver.
#[derive(Debug, Clone, Copy)]
pub struct MemoryScanner {
    driver: LinuxMemoryDriver,
}

impl MemoryScanner {
    pub fn new(driver: LinuxMemoryDriver) -> Self {
        Self { driver }
    }

    pub fn driver(&self) -> &LinuxMemoryDriver {
        &self.driver
    }

    pub fn scan_aob(&self, pattern: &str) -> Result<Vec<usize>> {
        scan_aob(&self.driver, pattern)
    }

    pub fn scan_aob_in_range(&self, start: usize, len: usize, pattern: &str) -> Result<Vec<usize>> {
        scan_aob_in_range(&self.driver, start, len, pattern)
    }

    pub fn scan_utf8_string(&self, needle: &str) -> Result<Vec<usize>> {
        scan_utf8_string(&self.driver, needle)
    }

    pub fn scan_utf16_string(&self, needle: &str) -> Result<Vec<usize>> {
        scan_utf16_string(&self.driver, needle)
    }

    pub fn scan_value<T: Copy + PartialEq>(&self, val: T) -> Result<Vec<usize>> {
        scan_value(&self.driver, val)
    }

    pub fn filter_addresses<T: Copy + PartialEq>(
        &self,
        addrs: &[usize],
        new_val: T,
    ) -> Result<Vec<usize>> {
        filter_addresses(&self.driver, addrs, new_val)
    }
}

/// Scans all readable memory regions for an AOB pattern.
/// Supports hex bytes and `??` or `?` wildcards (e.g. `"48 8B 05 ?? ?? ?? ??"`).
pub fn scan_aob(driver: &LinuxMemoryDriver, pattern: &str) -> Result<Vec<usize>> {
    let pat = AobPattern::parse(pattern)?;
    let mappings = parse_process_maps(driver.pid())?;
    let mut results = Vec::new();

    for m in mappings {
        if m.is_readable() {
            scan_region_aob(driver, &m, &pat, &mut results);
        }
    }

    Ok(results)
}

/// Scans a specific memory range for an AOB pattern.
pub fn scan_aob_in_range(
    driver: &LinuxMemoryDriver,
    start: usize,
    len: usize,
    pattern: &str,
) -> Result<Vec<usize>> {
    let pat = AobPattern::parse(pattern)?;
    let mapping = MemoryMapping {
        start,
        end: start.saturating_add(len),
        perms: "r--p".to_string(),
        pathname: String::new(),
    };
    let mut results = Vec::new();
    scan_region_aob(driver, &mapping, &pat, &mut results);
    Ok(results)
}

/// Scans all readable/writable heap regions (`rw-p`) for a UTF-8 string.
pub fn scan_utf8_string(driver: &LinuxMemoryDriver, needle: &str) -> Result<Vec<usize>> {
    let needle_bytes = needle.as_bytes();
    if needle_bytes.is_empty() {
        return Ok(Vec::new());
    }

    let rw_mappings = find_rw_mappings(driver.pid())?;
    let mut results = Vec::new();

    for m in rw_mappings {
        scan_region_bytes(driver, &m, needle_bytes, &mut results);
    }

    Ok(results)
}

/// Scans all readable/writable heap regions (`rw-p`) for a UTF-16 little-endian string (IL2CPP / C# strings).
pub fn scan_utf16_string(driver: &LinuxMemoryDriver, needle: &str) -> Result<Vec<usize>> {
    let needle_u16: Vec<u8> = needle
        .encode_utf16()
        .flat_map(|c| c.to_le_bytes())
        .collect();

    if needle_u16.is_empty() {
        return Ok(Vec::new());
    }

    let rw_mappings = find_rw_mappings(driver.pid())?;
    let mut results = Vec::new();

    for m in rw_mappings {
        scan_region_bytes(driver, &m, &needle_u16, &mut results);
    }

    Ok(results)
}

/// Scans all readable/writable heap regions (`rw-p`) for an arbitrary typed value (e.g. i32, i64, f32).
pub fn scan_value<T: Copy + PartialEq>(driver: &LinuxMemoryDriver, val: T) -> Result<Vec<usize>> {
    let val_bytes = unsafe {
        std::slice::from_raw_parts(&val as *const T as *const u8, std::mem::size_of::<T>())
    };

    if val_bytes.is_empty() {
        return Ok(Vec::new());
    }

    let rw_mappings = find_rw_mappings(driver.pid())?;
    let mut results = Vec::new();

    for m in rw_mappings {
        scan_region_bytes(driver, &m, val_bytes, &mut results);
    }

    Ok(results)
}

/// Filters candidate addresses by reading the current value at each address and comparing with `new_val`.
/// This implements Cheat-Engine-style "Next Scan" refinement.
pub fn filter_addresses<T: Copy + PartialEq>(
    driver: &LinuxMemoryDriver,
    addrs: &[usize],
    new_val: T,
) -> Result<Vec<usize>> {
    let mut matched = Vec::new();

    for &addr in addrs {
        if let Ok(current) = driver.read::<T>(addr) {
            if current == new_val {
                matched.push(addr);
            }
        }
    }

    Ok(matched)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aob_pattern_parsing() {
        let pattern = AobPattern::parse("48 8B 05 ?? ?? ?? ?? 48 89").unwrap();
        assert_eq!(pattern.bytes.len(), 9);
        assert_eq!(pattern.bytes[0], Some(0x48));
        assert_eq!(pattern.bytes[1], Some(0x8B));
        assert_eq!(pattern.bytes[2], Some(0x05));
        assert_eq!(pattern.bytes[3], None);
        assert_eq!(pattern.bytes[4], None);
        assert_eq!(pattern.bytes[5], None);
        assert_eq!(pattern.bytes[6], None);
        assert_eq!(pattern.bytes[7], Some(0x48));
        assert_eq!(pattern.bytes[8], Some(0x89));
    }

    #[test]
    fn test_aob_scan_buffer() {
        let pattern = AobPattern::parse("48 8B ?? 10").unwrap();
        let buffer = vec![0x90, 0x90, 0x48, 0x8B, 0x55, 0x10, 0xCC, 0x48, 0x8B, 0x00, 0x10];
        let mut results = Vec::new();
        pattern.scan_buffer(&buffer, 0x1000, &mut results);
        assert_eq!(results, vec![0x1002, 0x1007]);
    }

    #[test]
    fn test_exact_bytes_scan() {
        let needle = b"Arsenal";
        let buffer = b"Hello Arsenal FC! Welcome to Arsenal!";
        let mut results = Vec::new();
        scan_buffer_exact(buffer, needle, 0x2000, &mut results);
        assert_eq!(results, vec![0x2006, 0x201D]);
    }

    #[test]
    fn test_self_process_memory_scan() {
        let my_pid = std::process::id() as i32;
        let driver = LinuxMemoryDriver::new(my_pid);

        let secret_value: i32 = 0x12345678;
        let addr = &secret_value as *const i32 as usize;

        // Verify driver read
        let read_val: i32 = driver.read(addr).unwrap();
        assert_eq!(read_val, secret_value);

        // Filter address test
        let addrs = vec![addr, 0x1000];
        let filtered = filter_addresses(&driver, &addrs, secret_value).unwrap();
        assert_eq!(filtered, vec![addr]);
    }
}
