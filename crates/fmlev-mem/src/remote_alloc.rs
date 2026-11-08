//! Allocates anonymous memory inside another Linux process (the equivalent of `VirtualAllocEx`).
//!
//! Method: attach to one thread with `PTRACE_SEIZE` + `PTRACE_INTERRUPT`, save its registers, point
//! `rip` at a `syscall` instruction found in an executable mapping, load the registers for
//! `mmap(0, len, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANONYMOUS, -1, 0)`, single-step exactly that
//! instruction, read `rax`, restore the saved registers byte for byte and detach.
//!
//! `orig_rax` is set to -1 while the injected syscall runs so the kernel does not treat it as a
//! restartable syscall; the original value is put back with the other registers so a thread that was
//! interrupted inside a blocking syscall restarts it normally afterwards.
//!
//! Wine/Proton caveat: Wine can enable syscall user dispatch on its threads, which turns a `syscall`
//! outside the allowed text range into SIGSYS while the thread is in Windows code. Gadgets in Wine's
//! unix `ntdll.so` are preferred for that reason, every thread is tried in turn, and a SIGSYS (or any
//! other unexpected stop) is swallowed: registers are restored and the next thread is tried.
//! The mapping is never freed (it is intentionally left to the target); callers must treat it as a leak.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use crate::driver::LinuxMemoryDriver;
use crate::error::{MemoryError, Result};
use crate::maps::{parse_process_maps, MemoryMapping};
use std::time::{Duration, Instant};

const PAGE: usize = 4096;
const MAX_LEN: usize = 64 << 20;
const USER_MAX: u64 = 0x7fff_ffff_ffff;
const STOP_TIMEOUT: Duration = Duration::from_secs(2);

fn fail(message: impl Into<String>) -> MemoryError {
    MemoryError::Io(std::io::Error::other(message.into()))
}

fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// Finds a `syscall` instruction in an executable mapping. Priority: Wine's unix ntdll, libc, anything
/// else executable. A `syscall; ret` pair is preferred inside each group.
fn find_gadget(pid: i32, maps: &[MemoryMapping]) -> Vec<usize> {
    let driver = LinuxMemoryDriver::new(pid);
    let rank = |m: &MemoryMapping| {
        if m.pathname.contains("ntdll.so") {
            0
        } else if m.pathname.contains("libc") {
            1
        } else {
            2
        }
    };
    let mut candidates: Vec<&MemoryMapping> = maps
        .iter()
        .filter(|m| m.is_readable() && m.is_executable() && !m.pathname.starts_with("[vsyscall") && m.size() <= (64 << 20))
        .collect();
    candidates.sort_by_key(|m| rank(m));
    let mut found = Vec::new();
    for map in candidates {
        let mut buf = vec![0u8; map.size()];
        if driver.read_bytes(map.start, &mut buf).is_err() {
            continue;
        }
        let at = buf.windows(3).position(|w| w == [0x0f, 0x05, 0xc3]).or_else(|| buf.windows(2).position(|w| w == [0x0f, 0x05]));
        if let Some(offset) = at {
            found.push(map.start + offset);
            if found.len() >= 4 {
                break;
            }
        }
    }
    found
}

fn wait_stopped(tid: i32) -> Result<i32> {
    let deadline = Instant::now() + STOP_TIMEOUT;
    loop {
        let mut status = 0i32;
        let r = unsafe { libc::waitpid(tid, &mut status, libc::__WALL | libc::WNOHANG) };
        if r == tid {
            if libc::WIFSTOPPED(status) {
                return Ok(status);
            }
            return Err(fail(format!("thread {tid} exited or was killed while waiting for a stop (status {status:#x})")));
        }
        if r < 0 {
            return Err(fail(format!("waitpid({tid}) failed: errno {}", errno())));
        }
        if Instant::now() > deadline {
            return Err(fail(format!("thread {tid} did not stop within {STOP_TIMEOUT:?}")));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn getregs(tid: i32) -> Result<libc::user_regs_struct> {
    let mut regs: libc::user_regs_struct = unsafe { std::mem::zeroed() };
    let r = unsafe { libc::ptrace(libc::PTRACE_GETREGS, tid, 0usize, &mut regs as *mut _ as usize) };
    if r < 0 {
        return Err(fail(format!("PTRACE_GETREGS({tid}) failed: errno {}", errno())));
    }
    Ok(regs)
}

fn setregs(tid: i32, regs: &libc::user_regs_struct) -> Result<()> {
    let r = unsafe { libc::ptrace(libc::PTRACE_SETREGS, tid, 0usize, regs as *const _ as usize) };
    if r < 0 {
        return Err(fail(format!("PTRACE_SETREGS({tid}) failed: errno {}", errno())));
    }
    Ok(())
}

/// Runs `mmap` in thread `tid` using `gadget`. The thread is always detached; its registers are restored
/// before returning, whatever the outcome (a restore failure is reported as the error).
fn inject_mmap(tid: i32, gadget: usize, len: usize) -> Result<usize> {
    let seize = unsafe { libc::ptrace(libc::PTRACE_SEIZE, tid, 0usize, 0usize) };
    if seize < 0 {
        return Err(if errno() == libc::EPERM { MemoryError::PermissionDenied(tid) } else { fail(format!("PTRACE_SEIZE({tid}) failed: errno {}", errno())) });
    }
    let detach = || unsafe { libc::ptrace(libc::PTRACE_DETACH, tid, 0usize, 0usize) };
    if unsafe { libc::ptrace(libc::PTRACE_INTERRUPT, tid, 0usize, 0usize) } < 0 {
        let e = errno();
        detach();
        return Err(fail(format!("PTRACE_INTERRUPT({tid}) failed: errno {e}")));
    }
    if let Err(error) = wait_stopped(tid) {
        detach();
        return Err(error);
    }
    let saved = match getregs(tid) {
        Ok(regs) => regs,
        Err(error) => {
            detach();
            return Err(error);
        }
    };
    let outcome = (|| -> Result<usize> {
        let mut regs = saved;
        regs.rax = libc::SYS_mmap as u64;
        regs.rdi = 0;
        regs.rsi = len as u64;
        regs.rdx = (libc::PROT_READ | libc::PROT_WRITE) as u64;
        regs.r10 = (libc::MAP_PRIVATE | libc::MAP_ANONYMOUS) as u64;
        regs.r8 = u64::MAX;
        regs.r9 = 0;
        regs.rip = gadget as u64;
        regs.orig_rax = u64::MAX;
        setregs(tid, &regs)?;
        if unsafe { libc::ptrace(libc::PTRACE_SINGLESTEP, tid, 0usize, 0usize) } < 0 {
            return Err(fail(format!("PTRACE_SINGLESTEP({tid}) failed: errno {}", errno())));
        }
        let status = wait_stopped(tid)?;
        if libc::WSTOPSIG(status) != libc::SIGTRAP {
            return Err(fail(format!("thread {tid} stopped with signal {} instead of SIGTRAP after the injected syscall", libc::WSTOPSIG(status))));
        }
        let after = getregs(tid)?;
        if after.rip != gadget as u64 + 2 {
            return Err(fail(format!("injected syscall did not complete (rip {:#x}, expected {:#x})", after.rip, gadget + 2)));
        }
        let result = after.rax;
        if (result as i64) < 0 && (result as i64) >= -4095 {
            return Err(fail(format!("remote mmap failed: errno {}", -(result as i64))));
        }
        if result == 0 || result > USER_MAX || result % PAGE as u64 != 0 {
            return Err(fail(format!("remote mmap returned an unusable address {result:#x}")));
        }
        Ok(result as usize)
    })();
    // Always restore the exact original registers, then detach.
    let restored = setregs(tid, &saved);
    detach();
    match (outcome, restored) {
        (Ok(address), Ok(())) => Ok(address),
        (Err(error), Ok(())) => Err(error),
        (_, Err(error)) => Err(fail(format!("could not restore thread {tid} registers after injection: {error}"))),
    }
}

fn thread_ids(pid: i32) -> Result<Vec<i32>> {
    let mut ids: Vec<i32> = std::fs::read_dir(format!("/proc/{pid}/task"))?
        .filter_map(|e| e.ok()?.file_name().to_str()?.parse().ok())
        .collect();
    ids.sort_unstable();
    Ok(ids)
}

/// Allocates `len` bytes (rounded up to whole pages) of private read/write anonymous memory in process
/// `pid` and returns its address. The allocation is verified in `/proc/pid/maps`.
pub fn remote_mmap(pid: i32, len: usize) -> Result<usize> {
    if len == 0 || len > MAX_LEN {
        return Err(fail(format!("refusing remote allocation of {len} bytes")));
    }
    let len = len.div_ceil(PAGE) * PAGE;
    let maps = parse_process_maps(pid)?;
    let gadgets = find_gadget(pid, &maps);
    if gadgets.is_empty() {
        return Err(fail("no syscall instruction found in any executable mapping of the target"));
    }
    let mut last_error = fail("no thread could be used");
    for tid in thread_ids(pid)? {
        for &gadget in &gadgets {
            match inject_mmap(tid, gadget, len) {
                Ok(address) => {
                    let after = parse_process_maps(pid)?;
                    let ok = after.iter().any(|m| m.start <= address && address + len <= m.end && m.is_rw_private());
                    return if ok { Ok(address) } else { Err(fail(format!("allocation at {address:#x} is not an rw private mapping in /proc/{pid}/maps"))) };
                }
                Err(error @ MemoryError::PermissionDenied(_)) => return Err(error),
                Err(error) => last_error = error,
            }
        }
    }
    Err(last_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Command, Stdio};

    struct Guard(Child);
    impl Drop for Guard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn injects_an_rw_mapping_into_a_child_and_leaves_it_running() {
        let child = Command::new("sleep").arg("30").stdout(Stdio::null()).spawn().expect("sleep available");
        let guard = Guard(child);
        let pid = guard.0.id() as i32;
        std::thread::sleep(Duration::from_millis(100));
        let before = parse_process_maps(pid).unwrap();
        let len = 5000; // rounds to two pages
        let address = remote_mmap(pid, len).expect("remote mmap");
        let after = parse_process_maps(pid).unwrap();
        let map = after.iter().find(|m| m.start <= address && address < m.end).expect("mapping present");
        assert!(map.is_rw_private(), "{map:?}");
        assert!(map.size() >= 2 * PAGE);
        assert!(!before.iter().any(|m| m.start <= address && address < m.end), "mapping must be new");
        // The memory is usable through the normal driver.
        let driver = LinuxMemoryDriver::new(pid);
        driver.write_bytes(address, b"fmlev-remote").unwrap();
        let mut back = [0u8; 12];
        driver.read_bytes(address, &mut back).unwrap();
        assert_eq!(&back, b"fmlev-remote");
        // Fresh anonymous memory starts zeroed.
        let mut tail = [1u8; 8];
        driver.read_bytes(address + 2 * PAGE - 8, &mut tail).unwrap();
        assert_eq!(tail, [0u8; 8]);
        // The child still runs after being detached (not stopped, not killed).
        let state = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
        let state = state.rsplit(')').next().unwrap().trim().chars().next().unwrap();
        assert!(matches!(state, 'S' | 'R'), "child state {state}");
        // A second allocation also works (registers were restored cleanly).
        let second = remote_mmap(pid, PAGE).expect("second mmap");
        assert_ne!(second, address);
    }

    #[test]
    fn refuses_absurd_sizes() {
        assert!(remote_mmap(std::process::id() as i32, 0).is_err());
        assert!(remote_mmap(std::process::id() as i32, MAX_LEN + 1).is_err());
    }
}
