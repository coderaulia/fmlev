//! Native club finance block. FM26 keeps the transfer budget in a plain `i32` block
//! `[transfer, transfer, a, b, c, balance]`; the transfer words are authoritative (verified by
//! changing the value and reopening Club > Finances). The balance word is NOT: the game derives
//! it elsewhere, so it is only used here to fingerprint the block and is never written.

use fmlev_mem::{parse_process_maps, LinuxMemoryDriver};

use crate::error::{CoreError, Result};

const BLOCK_WORDS: usize = 6;
const CHUNK: usize = 16 << 20;

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidValue(message.to_owned())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeClubFinance {
    pid: i32,
    address: usize,
    transfer_budget: i32,
    balance: i32,
}

fn words(driver: &LinuxMemoryDriver, address: usize) -> Result<[i32; BLOCK_WORDS]> {
    driver.read::<[i32; BLOCK_WORDS]>(address).map_err(Into::into)
}

fn matches(block: &[i32; BLOCK_WORDS], transfer: i32, balance: i32) -> bool {
    block[0] == transfer
        && block[1] == transfer
        && block[5] == balance
        && block[2..5].iter().all(|v| *v >= 0)
}

impl NativeClubFinance {
    /// Finds the one block whose transfer words and balance word equal the live bindings.
    /// Ambiguous or missing matches are rejected; no address is compiled in.
    pub fn discover(driver: &LinuxMemoryDriver, transfer: i64, balance: i64) -> Result<Self> {
        let (Ok(transfer), Ok(balance)) = (i32::try_from(transfer), i32::try_from(balance)) else {
            return Err(invalid("Club money is outside the verified 32-bit native range"));
        };
        if transfer <= 0 {
            return Err(invalid("A positive live transfer budget binding is required to locate the native block"));
        }
        let mut found = Vec::new();
        for map in parse_process_maps(driver.pid())?.iter().filter(|m| m.is_rw_private()) {
            let mut start = map.start;
            while start < map.end {
                let len = (map.end - start).min(CHUNK);
                let mut buf = vec![0u8; len];
                if driver.read_bytes(start, &mut buf).is_ok() {
                    for (i, chunk) in buf.chunks_exact(4).enumerate() {
                        let word = i32::from_le_bytes(chunk.try_into().unwrap());
                        let address = start + i * 4;
                        if word == transfer && i + BLOCK_WORDS <= len / 4 {
                            let block: [i32; BLOCK_WORDS] = std::array::from_fn(|k| {
                                i32::from_le_bytes(buf[(i + k) * 4..(i + k) * 4 + 4].try_into().unwrap())
                            });
                            if matches(&block, transfer, balance) {
                                found.push(address);
                            }
                        }
                    }
                }
                start += len;
            }
        }
        match found.as_slice() {
            [address] => Ok(Self { pid: driver.pid(), address: *address, transfer_budget: transfer, balance }),
            other => Err(invalid(&format!(
                "Expected one native club finance block matching the live bindings, found {}; open Club > Finances and retry",
                other.len()
            ))),
        }
    }

    fn validate(&self, driver: &LinuxMemoryDriver) -> Result<[i32; BLOCK_WORDS]> {
        if driver.pid() != self.pid {
            return Err(invalid("Native club block belongs to a different process"));
        }
        let block = words(driver, self.address)?;
        if block[0] != block[1] || block[5] != self.balance {
            return Err(invalid("Native club finance block changed; reopen Club > Finances and retry"));
        }
        Ok(block)
    }

    pub fn read_transfer_budget(&self, driver: &LinuxMemoryDriver) -> Result<i32> {
        Ok(self.validate(driver)?[0])
    }

    /// Writes both transfer words, verifies readback and restores the old value on failure.
    pub fn write_transfer_budget(&self, driver: &LinuxMemoryDriver, value: i64) -> Result<()> {
        let value = i32::try_from(value)
            .ok()
            .filter(|v| *v >= 0)
            .ok_or_else(|| invalid("Transfer budget must be between 0 and 2,147,483,647"))?;
        let old = self.validate(driver)?[0];
        if old == value {
            return Ok(());
        }
        let write = |v: i32| -> Result<()> {
            driver.write(self.address, &v)?;
            driver.write(self.address + 4, &v)?;
            Ok(())
        };
        write(value)?;
        let after = self.validate(driver);
        if !after.is_ok_and(|b| b[0] == value && b[1] == value) {
            let rollback = write(old);
            return Err(invalid(&format!(
                "Transfer budget readback failed; rollback: {rollback:?}"
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_shape_requires_equal_transfer_words_and_matching_balance() {
        assert!(matches(&[5, 5, 1, 2, 3, 9], 5, 9));
        assert!(!matches(&[5, 6, 1, 2, 3, 9], 5, 9));
        assert!(!matches(&[5, 5, 1, 2, 3, 8], 5, 9));
        assert!(!matches(&[5, 5, -1, 2, 3, 9], 5, 9));
    }
}
