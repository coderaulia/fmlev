use fmlev_mem::{
    find_fm_process, find_module_bases, LinuxMemoryDriver, ModuleBases,
};
use fmlev_il2cpp::{read_cstring, read_static_singleton, Il2CppString, OffsetsRegistry};
use crate::actions::SquadActions;
use crate::error::{CoreError, Result};
use crate::models::{
    ClubFinances, ClubSearchResult, MemorySearchResults, Player, PlayerAttributes,
    PlayerSearchResult,
};
use tracing::info;

/// High-level live game session for Football Manager 26.
/// Wraps memory driver, module bases, offsets registry, and stateful scan engine.
pub struct LiveSession {
    pub driver: LinuxMemoryDriver,
    pub bases: ModuleBases,
    pub registry: OffsetsRegistry,
    pub scanner_candidates: Vec<usize>,
}

impl LiveSession {
    /// Creates a live session attached to the given PID, automatically resolving module bases.
    pub fn new(pid: i32, registry: OffsetsRegistry) -> Result<Self> {
        let driver = LinuxMemoryDriver::new(pid);
        let bases = find_module_bases(pid).unwrap_or_default();
        Ok(Self {
            driver,
            bases,
            registry,
            scanner_candidates: Vec::new(),
        })
    }

    /// Automatically discovers and attaches to the running Football Manager 26 process.
    pub fn attach_auto(registry: OffsetsRegistry) -> Result<Self> {
        let proc = find_fm_process()?;
        info!("Auto-attached to FM process PID {}", proc.pid);
        Self::new(proc.pid, registry)
    }

    /// Directly constructs a LiveSession with pre-discovered module bases.
    pub fn with_bases(pid: i32, bases: ModuleBases, registry: OffsetsRegistry) -> Self {
        Self {
            driver: LinuxMemoryDriver::new(pid),
            bases,
            registry,
            scanner_candidates: Vec::new(),
        }
    }

    // =========================================================================
    // Memory Reading & Writing (Arbitrary Addresses)
    // =========================================================================

    #[inline]
    pub fn read_u8(&self, addr: usize) -> Result<u8> {
        self.driver.read(addr).map_err(CoreError::from)
    }

    #[inline]
    pub fn write_u8(&self, addr: usize, val: u8) -> Result<()> {
        self.driver.write(addr, &val).map_err(CoreError::from)
    }

    #[inline]
    pub fn read_u16(&self, addr: usize) -> Result<u16> {
        self.driver.read(addr).map_err(CoreError::from)
    }

    #[inline]
    pub fn write_u16(&self, addr: usize, val: u16) -> Result<()> {
        self.driver.write(addr, &val).map_err(CoreError::from)
    }

    #[inline]
    pub fn read_i32(&self, addr: usize) -> Result<i32> {
        self.driver.read(addr).map_err(CoreError::from)
    }

    #[inline]
    pub fn write_i32(&self, addr: usize, val: i32) -> Result<()> {
        self.driver.write(addr, &val).map_err(CoreError::from)
    }

    #[inline]
    pub fn read_u32(&self, addr: usize) -> Result<u32> {
        self.driver.read(addr).map_err(CoreError::from)
    }

    #[inline]
    pub fn write_u32(&self, addr: usize, val: u32) -> Result<()> {
        self.driver.write(addr, &val).map_err(CoreError::from)
    }

    #[inline]
    pub fn read_i64(&self, addr: usize) -> Result<i64> {
        self.driver.read(addr).map_err(CoreError::from)
    }

    #[inline]
    pub fn write_i64(&self, addr: usize, val: i64) -> Result<()> {
        self.driver.write(addr, &val).map_err(CoreError::from)
    }

    #[inline]
    pub fn read_u64(&self, addr: usize) -> Result<u64> {
        self.driver.read(addr).map_err(CoreError::from)
    }

    #[inline]
    pub fn write_u64(&self, addr: usize, val: u64) -> Result<()> {
        self.driver.write(addr, &val).map_err(CoreError::from)
    }

    #[inline]
    pub fn read_f32(&self, addr: usize) -> Result<f32> {
        self.driver.read(addr).map_err(CoreError::from)
    }

    #[inline]
    pub fn write_f32(&self, addr: usize, val: f32) -> Result<()> {
        self.driver.write(addr, &val).map_err(CoreError::from)
    }

    #[inline]
    pub fn read_f64(&self, addr: usize) -> Result<f64> {
        self.driver.read(addr).map_err(CoreError::from)
    }

    #[inline]
    pub fn write_f64(&self, addr: usize, val: f64) -> Result<()> {
        self.driver.write(addr, &val).map_err(CoreError::from)
    }

    #[inline]
    pub fn read_ptr(&self, addr: usize) -> Result<usize> {
        self.driver.read_ptr(addr).map_err(CoreError::from)
    }

    #[inline]
    pub fn write_ptr(&self, addr: usize, val: usize) -> Result<()> {
        self.driver.write(addr, &val).map_err(CoreError::from)
    }

    #[inline]
    pub fn read_val<T: Copy>(&self, addr: usize) -> Result<T> {
        self.driver.read::<T>(addr).map_err(CoreError::from)
    }

    #[inline]
    pub fn write_val<T: Copy>(&self, addr: usize, val: &T) -> Result<()> {
        self.driver.write(addr, val).map_err(CoreError::from)
    }

    /// Reads a null-terminated UTF-8 / ASCII C-string from memory.
    pub fn read_utf8(&self, addr: usize, max_len: usize) -> Result<String> {
        read_cstring(&self.driver, addr, max_len).map_err(CoreError::from)
    }

    /// Writes a null-terminated UTF-8 string to memory.
    pub fn write_utf8(&self, addr: usize, text: &str) -> Result<()> {
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(0); // null terminator
        self.driver.write_bytes(addr, &bytes).map_err(CoreError::from)?;
        Ok(())
    }

    /// Reads an IL2CPP UTF-16 managed String from memory.
    pub fn read_utf16(&self, addr: usize) -> Result<String> {
        Il2CppString::read(&self.driver, addr).map_err(CoreError::from)
    }

    /// Writes an IL2CPP UTF-16 managed String to memory.
    pub fn write_utf16(&self, addr: usize, text: &str) -> Result<()> {
        Il2CppString::write(&self.driver, addr, text).map_err(CoreError::from)
    }

    // =========================================================================
    // Memory Search & Model Helpers
    // =========================================================================

    /// Scans memory for both UTF-8 and UTF-16 representations of a search string.
    pub fn search_string_in_memory(&self, query: &str) -> Result<MemorySearchResults> {
        let utf8_matches = self.driver.scan_utf8_string(query)?;
        let utf16_matches = self.driver.scan_utf16_string(query)?;
        Ok(MemorySearchResults {
            query: query.to_string(),
            utf8_matches,
            utf16_matches,
        })
    }

    /// Searches memory for players by name (checking UTF-16 and UTF-8 strings).
    pub fn search_players_by_name(&self, name: &str) -> Result<Vec<PlayerSearchResult>> {
        let mut results = Vec::new();

        // Check UTF-16 (primary Unity/C# format)
        let utf16_addrs = self.driver.scan_utf16_string(name)?;
        for addr in utf16_addrs {
            results.push(PlayerSearchResult {
                name: name.to_string(),
                address: addr,
                is_utf16: true,
            });
        }

        // Check UTF-8 (native engine string format)
        let utf8_addrs = self.driver.scan_utf8_string(name)?;
        for addr in utf8_addrs {
            results.push(PlayerSearchResult {
                name: name.to_string(),
                address: addr,
                is_utf16: false,
            });
        }

        Ok(results)
    }

    /// Searches memory for clubs by name.
    pub fn search_clubs_by_name(&self, name: &str) -> Result<Vec<ClubSearchResult>> {
        let mut results = Vec::new();

        let utf16_addrs = self.driver.scan_utf16_string(name)?;
        for addr in utf16_addrs {
            results.push(ClubSearchResult {
                name: name.to_string(),
                address: addr,
                is_utf16: true,
            });
        }

        let utf8_addrs = self.driver.scan_utf8_string(name)?;
        for addr in utf8_addrs {
            results.push(ClubSearchResult {
                name: name.to_string(),
                address: addr,
                is_utf16: false,
            });
        }

        Ok(results)
    }

    /// Searches memory for occurrences of an integer ID (e.g. Player ID or Club ID).
    pub fn search_by_id(&self, id: i32) -> Result<Vec<usize>> {
        self.driver.scan_value(id).map_err(CoreError::from)
    }

    /// Reads a Player struct from memory at `addr`.
    pub fn read_player(&self, addr: usize) -> Result<Player> {
        Player::read(&self.driver, addr, &self.registry)
    }

    /// Writes Player attributes to memory at `addr`.
    pub fn write_player_attributes(&self, addr: usize, attrs: &PlayerAttributes) -> Result<()> {
        attrs.write(&self.driver, addr, &self.registry)
    }

    /// Reads ClubFinances from memory at `addr`.
    pub fn read_club_finances(&self, addr: usize) -> Result<ClubFinances> {
        ClubFinances::read(&self.driver, addr, &self.registry)
    }

    /// Writes ClubFinances to memory at `addr`.
    pub fn write_club_finances(&self, addr: usize, finances: &ClubFinances) -> Result<()> {
        finances.write(&self.driver, addr, &self.registry)
    }

    // =========================================================================
    // Squad God Mode Actions
    // =========================================================================

    /// Instant Squad Heal: Restores condition and sharpness to 100% (10,000).
    pub fn heal_player(&self, player_ptr: usize, condition_offset: Option<usize>) -> Result<()> {
        let offset = condition_offset.unwrap_or(0x10);
        SquadActions::heal_player(&self.driver, player_ptr, offset)?;
        Ok(())
    }

    /// Instant Squad Morale Boost: Restores morale to maximum (100).
    pub fn boost_morale(&self, player_ptr: usize, morale_offset: Option<usize>) -> Result<()> {
        let offset = morale_offset.unwrap_or(0x14);
        SquadActions::boost_morale(&self.driver, player_ptr, offset)?;
        Ok(())
    }

    /// Adjust Club Financial Budgets (Balance, Transfer Budget, Wage Budget).
    pub fn update_finances(
        &self,
        club_ptr: usize,
        balance: i64,
        transfer_budget: i64,
        wage_budget: i32,
    ) -> Result<()> {
        let balance_offset = self
            .registry
            .get_field_offset("ClubReference", "balance")
            .unwrap_or(0x30);
        let transfer_offset = self
            .registry
            .get_field_offset("ClubReference", "transfer_budget")
            .unwrap_or(0x38);
        let wage_offset = self
            .registry
            .get_field_offset("ClubReference", "wage_budget")
            .unwrap_or(0x40);

        SquadActions::update_finances(
            &self.driver,
            club_ptr,
            balance_offset,
            transfer_offset,
            wage_offset,
            balance,
            transfer_budget,
            wage_budget,
        )?;
        Ok(())
    }

    // =========================================================================
    // Stateful Cheat-Engine-Style Value Scanner
    // =========================================================================

    /// Initial Scan: scans writable memory for value `val` and stores candidate addresses.
    pub fn start_scan<T: Copy + PartialEq>(&mut self, val: T) -> Result<usize> {
        let matches = self.driver.scan_value(val)?;
        self.scanner_candidates = matches;
        Ok(self.scanner_candidates.len())
    }

    /// Next Scan: filters existing candidate addresses by checking which currently hold `val`.
    pub fn next_scan<T: Copy + PartialEq>(&mut self, val: T) -> Result<usize> {
        let filtered = self
            .driver
            .filter_addresses(&self.scanner_candidates, val)?;
        self.scanner_candidates = filtered;
        Ok(self.scanner_candidates.len())
    }

    pub fn start_scan_i32(&mut self, val: i32) -> Result<usize> {
        self.start_scan(val)
    }

    pub fn next_scan_i32(&mut self, val: i32) -> Result<usize> {
        self.next_scan(val)
    }

    pub fn start_scan_i64(&mut self, val: i64) -> Result<usize> {
        self.start_scan(val)
    }

    pub fn next_scan_i64(&mut self, val: i64) -> Result<usize> {
        self.next_scan(val)
    }

    pub fn start_scan_u8(&mut self, val: u8) -> Result<usize> {
        self.start_scan(val)
    }

    pub fn next_scan_u8(&mut self, val: u8) -> Result<usize> {
        self.next_scan(val)
    }

    pub fn start_scan_u16(&mut self, val: u16) -> Result<usize> {
        self.start_scan(val)
    }

    pub fn next_scan_u16(&mut self, val: u16) -> Result<usize> {
        self.next_scan(val)
    }

    pub fn start_scan_f32(&mut self, val: f32) -> Result<usize> {
        self.start_scan(val)
    }

    pub fn next_scan_f32(&mut self, val: f32) -> Result<usize> {
        self.next_scan(val)
    }

    /// Returns the currently active scan candidate addresses.
    pub fn scan_candidates(&self) -> &[usize] {
        &self.scanner_candidates
    }

    /// Clears the current scan results.
    pub fn clear_scan(&mut self) {
        self.scanner_candidates.clear();
    }

    // =========================================================================
    // Singleton & AOB Pattern Search
    // =========================================================================

    /// Resolves a static singleton instance by name from the active offsets profile.
    pub fn resolve_singleton(&self, name: &str) -> Result<usize> {
        let base = self
            .bases
            .game_assembly_base
            .ok_or_else(|| CoreError::ModuleNotLoaded("GameAssembly.dll".to_string()))?;
        let singleton = self.registry.get_singleton(name)?;
        read_static_singleton(&self.driver, base, singleton).map_err(CoreError::from)
    }

    /// Scans readable memory for an AOB pattern.
    pub fn scan_aob(&self, pattern: &str) -> Result<Vec<usize>> {
        self.driver.scan_aob(pattern).map_err(CoreError::from)
    }

    /// Scans a bounded address range for an AOB pattern.
    pub fn scan_aob_in_range(&self, start: usize, len: usize, pattern: &str) -> Result<Vec<usize>> {
        self.driver.scan_aob_in_range(start, len, pattern).map_err(CoreError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fmlev_mem::ModuleBases;
    use fmlev_il2cpp::OffsetsRegistry;

    #[test]
    fn test_live_session_memory_primitives() {
        let my_pid = std::process::id() as i32;
        let session = LiveSession::with_bases(my_pid, ModuleBases::default(), OffsetsRegistry::default());

        let mut val_i32: i32 = 123456;
        let addr_i32 = &mut val_i32 as *mut i32 as usize;

        assert_eq!(session.read_i32(addr_i32).unwrap(), 123456);
        session.write_i32(addr_i32, 987654).unwrap();
        assert_eq!(val_i32, 987654);

        let mut val_f32: f32 = 42.5;
        let addr_f32 = &mut val_f32 as *mut f32 as usize;
        assert_eq!(session.read_f32(addr_f32).unwrap(), 42.5);
        session.write_f32(addr_f32, 100.25).unwrap();
        assert_eq!(val_f32, 100.25);
    }

    #[test]
    fn test_live_session_squad_actions() {
        let my_pid = std::process::id() as i32;
        let session = LiveSession::with_bases(my_pid, ModuleBases::default(), OffsetsRegistry::default());

        // Buffer representing Player structure
        let mut fake_player = vec![0u8; 64];
        let player_ptr = fake_player.as_mut_ptr() as usize;

        session.heal_player(player_ptr, Some(0x10)).unwrap();
        let condition = u16::from_ne_bytes(fake_player[0x10..0x12].try_into().unwrap());
        let sharpness = u16::from_ne_bytes(fake_player[0x12..0x14].try_into().unwrap());
        assert_eq!(condition, 10000);
        assert_eq!(sharpness, 10000);

        session.boost_morale(player_ptr, Some(0x14)).unwrap();
        assert_eq!(fake_player[0x14], 100);
    }

    #[test]
    fn test_live_session_stateful_scan() {
        let my_pid = std::process::id() as i32;
        let mut session = LiveSession::with_bases(my_pid, ModuleBases::default(), OffsetsRegistry::default());

        let test_target: i32 = 777888;
        let addr = &test_target as *const i32 as usize;

        // Simulate initial scan finding this address among candidates
        session.scanner_candidates = vec![addr, 0x1234000];

        // Next scan when value hasn't changed
        let count = session.next_scan_i32(777888).unwrap();
        assert_eq!(count, 1);
        assert_eq!(session.scan_candidates(), &[addr]);

        // Next scan with non-matching value
        let count2 = session.next_scan_i32(999111).unwrap();
        assert_eq!(count2, 0);
        assert!(session.scan_candidates().is_empty());
    }
}
