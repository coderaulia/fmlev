use std::io::{IoSlice, IoSliceMut};
use nix::sys::uio::{process_vm_readv, process_vm_writev, RemoteIoVec};
use nix::unistd::Pid;
use crate::error::{MemoryError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinuxMemoryDriver {
    pid: Pid,
}

impl LinuxMemoryDriver {
    pub fn new(pid: i32) -> Self {
        Self {
            pid: Pid::from_raw(pid),
        }
    }

    pub fn pid(&self) -> i32 {
        self.pid.as_raw()
    }

    pub fn read_bytes(&self, remote_addr: usize, buf: &mut [u8]) -> Result<usize> {
        let mut local_iov = [IoSliceMut::new(buf)];
        let remote_iov = [RemoteIoVec {
            base: remote_addr,
            len: local_iov[0].len(),
        }];

        process_vm_readv(self.pid, &mut local_iov, &remote_iov).map_err(|errno| {
            if errno == nix::errno::Errno::EPERM {
                MemoryError::PermissionDenied(self.pid.as_raw())
            } else {
                MemoryError::ReadFailed(local_iov[0].len(), remote_addr, errno.to_string())
            }
        })
    }

    pub fn write_bytes(&self, remote_addr: usize, data: &[u8]) -> Result<usize> {
        let local_iov = [IoSlice::new(data)];
        let remote_iov = [RemoteIoVec {
            base: remote_addr,
            len: data.len(),
        }];

        process_vm_writev(self.pid, &local_iov, &remote_iov).map_err(|errno| {
            if errno == nix::errno::Errno::EPERM {
                MemoryError::PermissionDenied(self.pid.as_raw())
            } else {
                MemoryError::WriteFailed(data.len(), remote_addr, errno.to_string())
            }
        })
    }

    pub fn read<T: Copy>(&self, remote_addr: usize) -> Result<T> {
        let mut val = std::mem::MaybeUninit::<T>::uninit();
        let slice = unsafe {
            std::slice::from_raw_parts_mut(val.as_mut_ptr() as *mut u8, std::mem::size_of::<T>())
        };
        self.read_bytes(remote_addr, slice)?;
        Ok(unsafe { val.assume_init() })
    }

    pub fn write<T: Copy>(&self, remote_addr: usize, value: &T) -> Result<()> {
        let slice = unsafe {
            std::slice::from_raw_parts(value as *const T as *const u8, std::mem::size_of::<T>())
        };
        self.write_bytes(remote_addr, slice)?;
        Ok(())
    }

    pub fn read_ptr(&self, remote_addr: usize) -> Result<usize> {
        self.read::<usize>(remote_addr)
    }

    pub fn resolve_pointer_chain(&self, base_addr: usize, offsets: &[usize]) -> Result<usize> {
        let mut current = base_addr;
        for &offset in offsets {
            if current == 0 {
                return Err(MemoryError::InvalidPointer(current));
            }
            current = self.read_ptr(current.wrapping_add(offset))?;
        }
        Ok(current)
    }

    /// High-performance AOB signature scan across all readable regions.
    pub fn scan_aob(&self, pattern: &str) -> Result<Vec<usize>> {
        crate::scanner::scan_aob(self, pattern)
    }

    /// High-performance AOB signature scan in a bounded address range.
    pub fn scan_aob_in_range(&self, start: usize, len: usize, pattern: &str) -> Result<Vec<usize>> {
        crate::scanner::scan_aob_in_range(self, start, len, pattern)
    }

    /// Scans readable/writable heap memory (`rw-p`) for a UTF-8 string needle.
    pub fn scan_utf8_string(&self, needle: &str) -> Result<Vec<usize>> {
        crate::scanner::scan_utf8_string(self, needle)
    }

    /// Scans readable/writable heap memory (`rw-p`) for a UTF-16 little-endian string needle.
    pub fn scan_utf16_string(&self, needle: &str) -> Result<Vec<usize>> {
        crate::scanner::scan_utf16_string(self, needle)
    }

    /// Scans readable/writable heap memory (`rw-p`) for an arbitrary value (Cheat Engine initial scan).
    pub fn scan_value<T: Copy + PartialEq>(&self, val: T) -> Result<Vec<usize>> {
        crate::scanner::scan_value(self, val)
    }

    /// Filters candidate addresses by reading the current value at each address (Cheat Engine next scan).
    pub fn filter_addresses<T: Copy + PartialEq>(
        &self,
        addrs: &[usize],
        new_val: T,
    ) -> Result<Vec<usize>> {
        crate::scanner::filter_addresses(self, addrs, new_val)
    }
}
