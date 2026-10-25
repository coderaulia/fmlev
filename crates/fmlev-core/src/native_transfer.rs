//! Native contract termination, loan termination, and player transfers.
//!
//! # Upstream Layout & Discovery
//! - `ACTUAL_PLAYER`:
//!   - `+0x130`: current team pointer (`.?AVTEAM@db@@`)
//!   - `+0x138`: current team copy pointer
//!   - `+0x148`: attribute record view (`NativePlayerRecord.address`)
//! - Attribute record:
//!   - `+0x140`: person object (`.?AVACTUAL_PLAYER@db@@` with subobject offset 0x288)
//!   - `+0x148`: index (`i32`)
//!   - `+0x14c`: UID (`u32`)
//!   - `+0x1e0`: agreement slot (`.?AVTRANSFER_AGREEMENT@db@@` or 0)
//!   - `+0x1e8`: parent contract pointer (`.?AVFULL_CONTRACT@db@@` or 0)
//!   - `+0x1f0`: loan node slot (first qword points to `.?AVLOAN_CONTRACT@db@@` or 0)
//! - `FULL_CONTRACT`:
//!   - `+0x0`: vtable (`.?AVFULL_CONTRACT@db@@`)
//!   - `+0x8`: person back-pointer (matches `record + 0x140`)
//!   - `+0x10`: parent team pointer (`.?AVTEAM@db@@`)
//!   - `+0x18`: parent team copy pointer
//!   - `+0x20`: weekly wage (GBP)
//!   - `+0x48`: packed expiry date
//! - `LOAN_CONTRACT`:
//!   - `+0x0`: vtable (`.?AVLOAN_CONTRACT@db@@`)
//!   - `+0x8`: person back-pointer (matches `record + 0x140`)
//!   - `+0x10`: loan team pointer (`.?AVTEAM@db@@`)
//! - `TEAM`:
//!   - `+0x0`: vtable (`.?AVTEAM@db@@`)
//!   - `+0x8`: index (`i32`)
//!   - `+0xc`: UID (`u32`)
//!   - `+0x10`: UID copy (`u32`)
//!   - `+0x38`: squad vector `[begin, end, capacity]` containing `ACTUAL_PLAYER` pointers

use std::collections::HashSet;
use fmlev_mem::{parse_process_maps, LinuxMemoryDriver};

use crate::error::{CoreError, Result};
use crate::native_player::NativePlayerRecord;

/// `.?AVTEAM@db@@` vtable RVA in game_plugin.dll (PE timestamp 0x6a229e8a).
const TEAM_VTABLE_RVA: usize = 0x44c_13a8;
pub const SQUAD_VECTOR_OFFSET: usize = 0x38;
pub const CURRENT_TEAM_OFFSET: usize = 0x130;
pub const CURRENT_TEAM2_OFFSET: usize = 0x138;
pub const CONTRACT_OFFSET: usize = 0x1e8;
pub const AGREEMENT_OFFSET: usize = 0x1e0;
pub const LOAN_OFFSET: usize = 0x1f0;
pub const PERSON_OFFSET: usize = 0x140;
pub const ACTUAL_PLAYER_OFFSET: usize = 0x148;
pub const CONTRACT_SIZE: usize = 0xc8;
pub const FULL_CONTRACT_VTABLE_RVA: usize = 0x4334df8;
pub const FALLBACK_FULL_CONTRACT_VTABLE: usize = 0x6fffdba64df8;

fn invalid(message: &str) -> CoreError {
    CoreError::InvalidValue(message.to_owned())
}

/// Represents an atomic in-memory edit with before/after state for verification and rollback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeEdit {
    pub address: usize,
    pub old: Vec<u8>,
    pub next: Vec<u8>,
    pub label: &'static str,
}

impl NativeEdit {
    pub fn new(address: usize, old: Vec<u8>, next: Vec<u8>, label: &'static str) -> Self {
        Self { address, old, next, label }
    }
}

/// Bounded representation of a team's squad pointer vector (`std::vector<ACTUAL_PLAYER*>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SquadVector {
    pub team_address: usize,
    pub begin: usize,
    pub end: usize,
    pub capacity: usize,
}

impl SquadVector {
    /// Reads and validates the squad vector bounds from a team object.
    pub fn read(driver: &LinuxMemoryDriver, team_address: usize) -> Result<Self> {
        if team_address <= 0x10000 || team_address % 8 != 0 {
            return Err(invalid("Invalid team pointer for squad vector read"));
        }
        // The team must carry the TEAM vtable (game_plugin.dll RVA, same pinning as competitions and the
        // manager object); test processes have no module and skip the pin.
        if let Some(base) = parse_process_maps(driver.pid())?
            .iter()
            .filter(|m| m.pathname.ends_with("game_plugin.dll"))
            .map(|m| m.start)
            .min()
        {
            if driver.read_ptr(team_address)? != base + TEAM_VTABLE_RVA {
                return Err(invalid("Target is not a TEAM record (vtable mismatch)"));
            }
        }
        let begin = driver.read_ptr(team_address + SQUAD_VECTOR_OFFSET)?;
        let end = driver.read_ptr(team_address + SQUAD_VECTOR_OFFSET + 8)?;
        let capacity = driver.read_ptr(team_address + SQUAD_VECTOR_OFFSET + 16)?;

        if begin % 8 != 0 || end < begin || capacity < end {
            return Err(invalid("Squad vector pointer triple has invalid ordering or alignment"));
        }
        let size = end - begin;
        let cap = capacity - begin;
        if size % 8 != 0 || cap % 8 != 0 || cap > 4096 * 8 {
            return Err(invalid("Squad vector size or capacity is out of bounds"));
        }
        Ok(Self { team_address, begin, end, capacity })
    }

    pub fn count(&self) -> usize {
        (self.end - self.begin) / 8
    }

    pub fn capacity_count(&self) -> usize {
        (self.capacity - self.begin) / 8
    }

    pub fn spare_slots(&self) -> usize {
        (self.capacity - self.end) / 8
    }

    /// Reads all player pointers currently in the squad vector.
    pub fn entries(&self, driver: &LinuxMemoryDriver) -> Result<Vec<usize>> {
        let count = self.count();
        if count == 0 {
            return Ok(Vec::new());
        }
        let mut bytes = vec![0u8; count * 8];
        driver.read_bytes(self.begin, &mut bytes)?;
        let mut pointers = Vec::with_capacity(count);
        for chunk in bytes.chunks_exact(8) {
            pointers.push(usize::from_le_bytes(chunk.try_into().unwrap()));
        }
        Ok(pointers)
    }

    pub fn contains(&self, driver: &LinuxMemoryDriver, player_actual: usize) -> Result<bool> {
        let list = self.entries(driver)?;
        Ok(list.contains(&player_actual))
    }

    /// Creates atomic patches to remove a player from the squad vector and compact remaining entries.
    pub fn compact_removal_patches(
        &self,
        driver: &LinuxMemoryDriver,
        player_actual: usize,
    ) -> Result<Option<Vec<NativeEdit>>> {
        let entries = self.entries(driver)?;
        if !entries.contains(&player_actual) {
            return Ok(None);
        }
        let old_count = entries.len();
        let compacted: Vec<usize> = entries.into_iter().filter(|&p| p != player_actual).collect();
        let new_count = compacted.len();
        let removed = old_count - new_count;
        if removed == 0 {
            return Ok(None);
        }

        let mut old_bytes = vec![0u8; old_count * 8];
        driver.read_bytes(self.begin, &mut old_bytes)?;

        let mut new_bytes = Vec::with_capacity(old_count * 8);
        for p in &compacted {
            new_bytes.extend_from_slice(&p.to_le_bytes());
        }
        // Zero out the freed tail slots
        new_bytes.resize(old_count * 8, 0u8);

        let new_end = self.begin + new_count * 8;
        let mut edits = Vec::new();
        edits.push(NativeEdit::new(
            self.begin,
            old_bytes,
            new_bytes,
            "squad vector entries compaction",
        ));
        edits.push(NativeEdit::new(
            self.team_address + SQUAD_VECTOR_OFFSET + 8,
            self.end.to_le_bytes().to_vec(),
            new_end.to_le_bytes().to_vec(),
            "squad vector end pointer shrink",
        ));

        Ok(Some(edits))
    }

    /// Creates atomic patches to append a player to the squad vector, using spare capacity.
    pub fn append_patches(
        &self,
        driver: &LinuxMemoryDriver,
        player_actual: usize,
    ) -> Result<Vec<NativeEdit>> {
        if self.spare_slots() == 0 {
            return Err(invalid("Squad vector is at capacity; no spare slots available"));
        }
        let mut old_slot_bytes = vec![0u8; 8];
        driver.read_bytes(self.end, &mut old_slot_bytes)?;

        let new_end = self.end + 8;
        let edits = vec![
            NativeEdit::new(
                self.end,
                old_slot_bytes,
                player_actual.to_le_bytes().to_vec(),
                "squad vector slot append",
            ),
            NativeEdit::new(
                self.team_address + SQUAD_VECTOR_OFFSET + 8,
                self.end.to_le_bytes().to_vec(),
                new_end.to_le_bytes().to_vec(),
                "squad vector end pointer growth",
            ),
        ];
        Ok(edits)
    }
}

/// Applies a sequence of edits with transactional readback and automatic reverse rollback on any failure.
pub fn apply_transaction(driver: &LinuxMemoryDriver, edits: &[NativeEdit]) -> Result<()> {
    // Indices are recorded before each write, so a short or failed write is still rolled back
    // and skipped no-op edits can never shift the rollback range.
    let mut written: Vec<usize> = Vec::new();
    let write_result = (|| -> Result<()> {
        for (index, edit) in edits.iter().enumerate() {
            let mut current = vec![0u8; edit.old.len()];
            if driver.read_bytes(edit.address, &mut current)? != current.len() || current != edit.old {
                return Err(invalid(&format!(
                    "Memory data at 0x{:x} ({}) changed before write; transaction aborted",
                    edit.address, edit.label
                )));
            }
            if edit.old == edit.next {
                continue;
            }
            written.push(index);
            if driver.write_bytes(edit.address, &edit.next)? != edit.next.len() {
                return Err(invalid(&format!("Short write at 0x{:x} ({})", edit.address, edit.label)));
            }
            let mut readback = vec![0u8; edit.next.len()];
            if driver.read_bytes(edit.address, &mut readback)? != readback.len() || readback != edit.next {
                return Err(invalid(&format!(
                    "Readback verification mismatch at 0x{:x} ({})",
                    edit.address, edit.label
                )));
            }
        }
        Ok(())
    })();

    if let Err(error) = write_result {
        let mut rollback_errors = Vec::new();
        for &index in written.iter().rev() {
            let edit = &edits[index];
            if driver.write_bytes(edit.address, &edit.old).is_err() {
                rollback_errors.push(edit.address);
                continue;
            }
            let mut check = vec![0u8; edit.old.len()];
            if driver.read_bytes(edit.address, &mut check).is_err() || check != edit.old {
                rollback_errors.push(edit.address);
            }
        }
        if !rollback_errors.is_empty() {
            return Err(invalid(&format!(
                "Transaction failed ({error}) and rollback failed at addresses: {rollback_errors:x?}"
            )));
        }
        return Err(invalid(&format!("Transaction failed and was rolled back cleanly: {error}")));
    }
    Ok(())
}

/// Releases a player as a free agent by detaching all contracts, agreements, loans, team links,
/// and removing the player pointer from any squad vectors.
pub fn terminate_contract(
    driver: &LinuxMemoryDriver,
    player: &NativePlayerRecord,
) -> Result<Vec<usize>> {
    let actual_player = player.address - ACTUAL_PLAYER_OFFSET;
    let contract_addr = driver.read_ptr(player.address + CONTRACT_OFFSET)?;
    if contract_addr <= 0x10000 {
        return Err(invalid("Player has no active contract to terminate"));
    }

    let person_owner = driver.read_ptr(contract_addr + 8)?;
    if person_owner != player.address + PERSON_OFFSET {
        return Err(invalid("Contract person back-pointer mismatch"));
    }

    let mut candidate_teams = HashSet::new();
    let parent_team = driver.read_ptr(contract_addr + 0x10)?;
    if parent_team > 0x10000 { candidate_teams.insert(parent_team); }
    let parent_team2 = driver.read_ptr(contract_addr + 0x18)?;
    if parent_team2 > 0x10000 { candidate_teams.insert(parent_team2); }

    let cur_team = driver.read_ptr(actual_player + CURRENT_TEAM_OFFSET)?;
    if cur_team > 0x10000 { candidate_teams.insert(cur_team); }
    let cur_team2 = driver.read_ptr(actual_player + CURRENT_TEAM2_OFFSET)?;
    if cur_team2 > 0x10000 { candidate_teams.insert(cur_team2); }

    let loan_node = driver.read_ptr(player.address + LOAN_OFFSET)?;
    if loan_node > 0x10000 {
        if let Ok(loan_contract) = driver.read_ptr(loan_node) {
            if loan_contract > 0x10000 {
                if let Ok(loan_team) = driver.read_ptr(loan_contract + 0x10) {
                    if loan_team > 0x10000 { candidate_teams.insert(loan_team); }
                }
            }
        }
    }

    let mut edits = Vec::new();
    let mut cleared_teams = Vec::new();

    for team in candidate_teams {
        let vec = SquadVector::read(driver, team)?;
        if let Some(compaction_edits) = vec.compact_removal_patches(driver, actual_player)? {
            edits.extend(compaction_edits);
            cleared_teams.push(team);
        }
    }

    if cur_team > 0 {
        edits.push(NativeEdit::new(
            actual_player + CURRENT_TEAM_OFFSET,
            cur_team.to_le_bytes().to_vec(),
            0usize.to_le_bytes().to_vec(),
            "clear ACTUAL_PLAYER current_team",
        ));
    }
    if cur_team2 > 0 {
        edits.push(NativeEdit::new(
            actual_player + CURRENT_TEAM2_OFFSET,
            cur_team2.to_le_bytes().to_vec(),
            0usize.to_le_bytes().to_vec(),
            "clear ACTUAL_PLAYER current_team copy",
        ));
    }

    edits.push(NativeEdit::new(
        player.address + CONTRACT_OFFSET,
        contract_addr.to_le_bytes().to_vec(),
        0usize.to_le_bytes().to_vec(),
        "clear parent contract pointer",
    ));

    let agree = driver.read_ptr(player.address + AGREEMENT_OFFSET)?;
    if agree > 0 {
        edits.push(NativeEdit::new(
            player.address + AGREEMENT_OFFSET,
            agree.to_le_bytes().to_vec(),
            0usize.to_le_bytes().to_vec(),
            "clear pending agreement pointer",
        ));
    }

    if loan_node > 0 {
        edits.push(NativeEdit::new(
            player.address + LOAN_OFFSET,
            loan_node.to_le_bytes().to_vec(),
            0usize.to_le_bytes().to_vec(),
            "clear loan node pointer",
        ));
    }

    apply_transaction(driver, &edits)?;
    Ok(cleared_teams)
}

/// Terminates an active loan, removing the player from the loan team squad vector,
/// restoring current team pointers back to the parent team, and clearing the loan node pointer.
pub fn terminate_loan(
    driver: &LinuxMemoryDriver,
    player: &NativePlayerRecord,
) -> Result<(usize, usize)> {
    let actual_player = player.address - ACTUAL_PLAYER_OFFSET;
    let loan_node = driver.read_ptr(player.address + LOAN_OFFSET)?;
    if loan_node <= 0x10000 {
        return Err(invalid("Player is not currently on loan"));
    }
    let loan_contract = driver.read_ptr(loan_node)?;
    if loan_contract <= 0x10000 {
        return Err(invalid("Invalid loan contract pointer in loan node"));
    }
    let loan_person = driver.read_ptr(loan_contract + 8)?;
    if loan_person != player.address + PERSON_OFFSET {
        return Err(invalid("Loan contract person back-pointer mismatch"));
    }
    let loan_team = driver.read_ptr(loan_contract + 0x10)?;
    if loan_team <= 0x10000 {
        return Err(invalid("Loan contract has no valid team pointer"));
    }

    let parent_contract = driver.read_ptr(player.address + CONTRACT_OFFSET)?;
    if parent_contract <= 0x10000 {
        return Err(invalid("Player has no active parent contract"));
    }
    let parent_team = driver.read_ptr(parent_contract + 0x10)?;
    if parent_team <= 0x10000 {
        return Err(invalid("Parent contract has no valid parent team pointer"));
    }

    let mut edits = Vec::new();

    // 1. Remove player from loan team squad vector
    let loan_vec = SquadVector::read(driver, loan_team)?;
    if let Some(removal_edits) = loan_vec.compact_removal_patches(driver, actual_player)? {
        edits.extend(removal_edits);
    }

    // 2. Add player to parent team squad vector if not already present
    let parent_vec = SquadVector::read(driver, parent_team)?;
    if !parent_vec.contains(driver, actual_player)? {
        let append_edits = parent_vec.append_patches(driver, actual_player)?;
        edits.extend(append_edits);
    }

    // 3. Update current team pointers on ACTUAL_PLAYER back to parent_team
    let cur_team = driver.read_ptr(actual_player + CURRENT_TEAM_OFFSET)?;
    edits.push(NativeEdit::new(
        actual_player + CURRENT_TEAM_OFFSET,
        cur_team.to_le_bytes().to_vec(),
        parent_team.to_le_bytes().to_vec(),
        "restore ACTUAL_PLAYER current_team to parent_team",
    ));
    let cur_team2 = driver.read_ptr(actual_player + CURRENT_TEAM2_OFFSET)?;
    edits.push(NativeEdit::new(
        actual_player + CURRENT_TEAM2_OFFSET,
        cur_team2.to_le_bytes().to_vec(),
        parent_team.to_le_bytes().to_vec(),
        "restore ACTUAL_PLAYER current_team copy to parent_team",
    ));

    // 4. Clear loan node pointer
    edits.push(NativeEdit::new(
        player.address + LOAN_OFFSET,
        loan_node.to_le_bytes().to_vec(),
        0usize.to_le_bytes().to_vec(),
        "clear loan node pointer",
    ));

    apply_transaction(driver, &edits)?;
    Ok((loan_team, parent_team))
}

/// Moves a contracted player from their current team to a target team.
/// Requires the target team squad vector to have spare capacity (`end < capacity`).
pub fn move_player(
    driver: &LinuxMemoryDriver,
    player: &NativePlayerRecord,
    target_team: usize,
) -> Result<(usize, usize)> {
    let actual_player = player.address - ACTUAL_PLAYER_OFFSET;
    let contract_addr = driver.read_ptr(player.address + CONTRACT_OFFSET)?;
    if contract_addr <= 0x10000 {
        return Err(invalid("Player has no active contract; signing unemployed free agents requires game-allocated objects"));
    }

    if target_team <= 0x10000 || target_team % 8 != 0 {
        return Err(invalid("Invalid target team pointer"));
    }

    let target_vec = SquadVector::read(driver, target_team)?;
    if target_vec.contains(driver, actual_player)? {
        return Err(invalid("Player is already a member of the target team"));
    }
    if target_vec.spare_slots() == 0 {
        return Err(invalid(
            "Target team squad vector is at capacity (0 spare slots). Terminate or move an unwanted player from target team first."
        ));
    }

    let cur_team = driver.read_ptr(actual_player + CURRENT_TEAM_OFFSET)?;
    if cur_team == target_team {
        return Ok((cur_team, target_team));
    }

    let mut edits = Vec::new();

    // 1. Remove from current team squad vector if present
    if cur_team > 0x10000 {
        let cur_vec = SquadVector::read(driver, cur_team)?;
        if let Some(compaction_edits) = cur_vec.compact_removal_patches(driver, actual_player)? {
            edits.extend(compaction_edits);
        }
    }

    // 2. Remove from parent contract team squad vector if different from cur_team
    let parent_team = driver.read_ptr(contract_addr + 0x10)?;
    if parent_team > 0x10000 && parent_team != cur_team && parent_team != target_team {
        let parent_vec = SquadVector::read(driver, parent_team)?;
        if let Some(compaction_edits) = parent_vec.compact_removal_patches(driver, actual_player)? {
            edits.extend(compaction_edits);
        }
    }

    // 3. Append to target team squad vector
    let append_edits = target_vec.append_patches(driver, actual_player)?;
    edits.extend(append_edits);

    // 4. Update ACTUAL_PLAYER team pointers
    edits.push(NativeEdit::new(
        actual_player + CURRENT_TEAM_OFFSET,
        cur_team.to_le_bytes().to_vec(),
        target_team.to_le_bytes().to_vec(),
        "update ACTUAL_PLAYER current_team",
    ));
    let cur_team2 = driver.read_ptr(actual_player + CURRENT_TEAM2_OFFSET)?;
    edits.push(NativeEdit::new(
        actual_player + CURRENT_TEAM2_OFFSET,
        cur_team2.to_le_bytes().to_vec(),
        target_team.to_le_bytes().to_vec(),
        "update ACTUAL_PLAYER current_team copy",
    ));

    // 5. Update FULL_CONTRACT team pointers
    edits.push(NativeEdit::new(
        contract_addr + 0x10,
        parent_team.to_le_bytes().to_vec(),
        target_team.to_le_bytes().to_vec(),
        "update FULL_CONTRACT team pointer",
    ));
    let parent_team2 = driver.read_ptr(contract_addr + 0x18)?;
    edits.push(NativeEdit::new(
        contract_addr + 0x18,
        parent_team2.to_le_bytes().to_vec(),
        target_team.to_le_bytes().to_vec(),
        "update FULL_CONTRACT team pointer copy",
    ));

    // 6. If player was on loan, clear loan node
    let loan_node = driver.read_ptr(player.address + LOAN_OFFSET)?;
    if loan_node > 0 {
        edits.push(NativeEdit::new(
            player.address + LOAN_OFFSET,
            loan_node.to_le_bytes().to_vec(),
            0usize.to_le_bytes().to_vec(),
            "clear loan node pointer upon permanent move",
        ));
    }

    apply_transaction(driver, &edits)?;
    Ok((cur_team, target_team))
}

/// Finds an aligned, verified all-zero 200-byte block in the entity pool mapping.
pub fn find_zero_contract_slot(driver: &LinuxMemoryDriver, near_addr: usize) -> Result<usize> {
    let mut check_buf = [0u8; CONTRACT_SIZE];

    // 1. First probe known safe zero block runs near the contract pool: 0x138980000..0x138990000
    for offset in (0..0x10000).step_by(CONTRACT_SIZE) {
        let candidate = 0x138980000 + offset;
        if candidate % 16 != 0 {
            continue;
        }
        if driver.read_bytes(candidate, &mut check_buf).is_ok() && check_buf.iter().all(|&b| b == 0) {
            return Ok(candidate);
        }
    }

    // 2. Fallback: query maps to find the rw-p mapping containing near_addr
    if let Ok(mappings) = fmlev_mem::parse_process_maps(driver.pid()) {
        for m in mappings {
            if m.is_readable() && m.is_writable() && m.start <= near_addr && near_addr < m.end {
                let scan_start = m.start + (m.end - m.start) / 2;
                for page in (scan_start..m.end).step_by(4096) {
                    if driver.read_bytes(page, &mut check_buf).is_ok() && check_buf.iter().all(|&b| b == 0) {
                        return Ok(page);
                    }
                }
            }
        }
    }

    Err(invalid("Unable to locate a verified zeroed contract memory slot in target process"))
}

/// Resolves the FULL_CONTRACT MSVC vtable pointer dynamically or via fallback.
pub fn resolve_contract_vtable(driver: &LinuxMemoryDriver) -> usize {
    if let Ok(mappings) = fmlev_mem::parse_process_maps(driver.pid()) {
        for m in mappings {
            if m.pathname.contains("game_plugin.dll")
                && m.is_readable()
                && m.is_executable()
            {
                let vt = m.start + FULL_CONTRACT_VTABLE_RVA;
                let mut check = [0u8; 8];
                if driver.read_bytes(vt, &mut check).is_ok() {
                    return vt;
                }
            }
        }
    }
    FALLBACK_FULL_CONTRACT_VTABLE
}

/// Constructs a valid 200-byte (0xc8) native FULL_CONTRACT buffer.
pub fn build_full_contract_bytes(
    vtable: usize,
    person_addr: usize,
    team_addr: usize,
    wage_gbp: u32,
    expiry_year: u32,
    squad_number: Option<u8>,
) -> Vec<u8> {
    let mut buf = vec![0u8; CONTRACT_SIZE];
    buf[0..8].copy_from_slice(&vtable.to_le_bytes());
    buf[8..16].copy_from_slice(&person_addr.to_le_bytes());
    buf[16..24].copy_from_slice(&team_addr.to_le_bytes());
    buf[24..32].copy_from_slice(&team_addr.to_le_bytes());
    buf[32..36].copy_from_slice(&wage_gbp.to_le_bytes());
    buf[36..40].copy_from_slice(&0x0001000bu32.to_le_bytes());
    buf[56..60].copy_from_slice(&0u32.to_le_bytes());
    buf[60..64].copy_from_slice(&(-1i32).to_le_bytes());
    buf[64..68].copy_from_slice(&(-1i32).to_le_bytes());
    buf[68..72].copy_from_slice(&0x07e900c2u32.to_le_bytes()); // start date 2025/194
    let packed_expiry = (expiry_year << 16) | 181;
    buf[72..76].copy_from_slice(&packed_expiry.to_le_bytes()); // expiry date (e.g. 30 June)
    buf[76..80].copy_from_slice(&0x07e900c2u32.to_le_bytes());
    buf[80..84].copy_from_slice(&0x076c0001u32.to_le_bytes());
    buf[84..88].copy_from_slice(&0x000b000bu32.to_le_bytes());
    buf[88..92].copy_from_slice(&0u32.to_le_bytes());
    let num_byte = squad_number.unwrap_or(255);
    buf[92..96].copy_from_slice(&[0x14, num_byte, 0x00, 0x00]);
    buf[96..104].copy_from_slice(&0x200u64.to_le_bytes());
    buf[176..184].copy_from_slice(&0x076c000100000000u64.to_le_bytes());
    buf[184..192].copy_from_slice(&0x000007ea00000000u64.to_le_bytes());
    buf[192..200].copy_from_slice(&0x000000ff01000000u64.to_le_bytes());
    buf
}

/// Signs an unemployed free agent to a target team.
/// Allocates a verified contract block, updates player pointers, and appends the player to the target squad vector.
pub fn sign_free_agent(
    driver: &LinuxMemoryDriver,
    player: &NativePlayerRecord,
    target_team: usize,
    wage_gbp: Option<u32>,
    expiry_year: Option<u32>,
) -> Result<usize> {
    let actual_player = player.address - ACTUAL_PLAYER_OFFSET;
    let existing_contract = driver.read_ptr(player.address + CONTRACT_OFFSET)?;
    if existing_contract > 0x10000 {
        return Err(invalid("Player already has an active contract. Use Move Player instead."));
    }

    if target_team <= 0x10000 || target_team % 8 != 0 {
        return Err(invalid("Invalid target team pointer"));
    }

    let target_vec = SquadVector::read(driver, target_team)?;
    if target_vec.contains(driver, actual_player)? {
        return Err(invalid("Player is already a member of the target team"));
    }
    if target_vec.spare_slots() == 0 {
        return Err(invalid(
            "Target team squad vector is at capacity (0 spare slots). Terminate or move an unwanted player from target team first."
        ));
    }

    // Check if player has an arranged future transfer agreement already
    let agreement_ptr = driver.read_ptr(player.address + AGREEMENT_OFFSET).unwrap_or(0);
    if agreement_ptr > 0x10000 {
        let (_, completed_team) = complete_pending_transfer(driver, player)?;
        return Ok(completed_team);
    }

    let contract_slot = find_zero_contract_slot(driver, player.address)?;
    let mut old_contract_bytes = vec![0u8; CONTRACT_SIZE];
    driver.read_bytes(contract_slot, &mut old_contract_bytes)?;
    if old_contract_bytes.iter().any(|&b| b != 0) {
        return Err(invalid("Contract memory slot was unexpectedly non-zero"));
    }

    let vtable = resolve_contract_vtable(driver);
    let person_addr = player.address + PERSON_OFFSET;
    let wage = wage_gbp.unwrap_or(1000);
    let expiry = expiry_year.unwrap_or(2027);
    if wage > i32::MAX as u32 {
        return Err(invalid("Wage is out of range"));
    }
    if !(2000..=2200).contains(&expiry) {
        return Err(invalid("Contract expiry year must be between 2000 and 2200"));
    }

    let contract_payload = build_full_contract_bytes(vtable, person_addr, target_team, wage, expiry, None);

    let cur_team = driver.read_ptr(actual_player + CURRENT_TEAM_OFFSET).unwrap_or(0);
    let cur_team2 = driver.read_ptr(actual_player + CURRENT_TEAM2_OFFSET).unwrap_or(0);

    let mut edits = Vec::new();

    // 1. Populate the new FULL_CONTRACT block
    edits.push(NativeEdit::new(
        contract_slot,
        old_contract_bytes,
        contract_payload,
        "populate FULL_CONTRACT block",
    ));

    // 2. Point player's record + 0x1e8 to the new contract
    edits.push(NativeEdit::new(
        player.address + CONTRACT_OFFSET,
        existing_contract.to_le_bytes().to_vec(),
        contract_slot.to_le_bytes().to_vec(),
        "set parent contract pointer",
    ));

    // 3. Update ACTUAL_PLAYER team pointers
    edits.push(NativeEdit::new(
        actual_player + CURRENT_TEAM_OFFSET,
        cur_team.to_le_bytes().to_vec(),
        target_team.to_le_bytes().to_vec(),
        "set ACTUAL_PLAYER current_team",
    ));
    edits.push(NativeEdit::new(
        actual_player + CURRENT_TEAM2_OFFSET,
        cur_team2.to_le_bytes().to_vec(),
        target_team.to_le_bytes().to_vec(),
        "set ACTUAL_PLAYER current_team copy",
    ));

    // 4. Append to target squad vector
    let append_edits = target_vec.append_patches(driver, actual_player)?;
    edits.extend(append_edits);

    apply_transaction(driver, &edits)?;
    Ok(target_team)
}

/// Helper to scan for a transfer queue entry belonging to a person.
/// Each entry in the queue is 0x30 bytes:
/// +0x00: u32 tag, u32 id
/// +0x08: person pointer
/// +0x10: join date packed (u32)
/// +0x14: transfer ID (u32)
pub fn find_queue_entry_for_person(
    driver: &LinuxMemoryDriver,
    person_addr: usize,
) -> Result<Option<(usize, Vec<u8>)>> {
    let start = 0x2603f0000;
    let end = 0x260400000;
    let mut buf = vec![0u8; end - start];
    if driver.read_bytes(start, &mut buf).is_ok() {
        let person_bytes = person_addr.to_le_bytes();
        if let Some(pos) = buf.windows(8).position(|w| w == person_bytes) {
            if pos >= 8 && pos + 0x28 <= buf.len() {
                let entry_addr = start + pos - 8;
                let entry_bytes = buf[pos - 8..pos - 8 + 0x30].to_vec();
                return Ok(Some((entry_addr, entry_bytes)));
            }
        }
    }
    Ok(None)
}

/// Completes an arranged future transfer immediately, moving the player to their agreed future club.
pub fn complete_pending_transfer(
    driver: &LinuxMemoryDriver,
    player: &NativePlayerRecord,
) -> Result<(usize, usize)> {
    let actual_player = player.address - ACTUAL_PLAYER_OFFSET;
    let agreement_slot = driver.read_ptr(player.address + AGREEMENT_OFFSET)?;
    if agreement_slot <= 0x10000 {
        return Err(invalid("Player has no arranged future transfer agreement"));
    }

    let vec_begin = driver.read_ptr(agreement_slot)?;
    if vec_begin <= 0x10000 {
        return Err(invalid("Invalid agreement vector begin pointer"));
    }
    let agree_entry = driver.read_ptr(vec_begin)?;
    if agree_entry <= 0x10000 {
        return Err(invalid("Invalid agreement entry pointer"));
    }

    let future_contract = driver.read_ptr(agree_entry + 0x10)?;
    if future_contract <= 0x10000 {
        return Err(invalid("Agreement has no valid future contract pointer"));
    }

    let person_owner = driver.read_ptr(future_contract + 8)?;
    if person_owner != player.address + PERSON_OFFSET {
        return Err(invalid("Future contract person back-pointer mismatch"));
    }

    let future_team = driver.read_ptr(future_contract + 0x10)?;
    if future_team <= 0x10000 {
        return Err(invalid("Future contract has no valid team pointer"));
    }

    let future_vec = SquadVector::read(driver, future_team)?;
    if future_vec.contains(driver, actual_player)? {
        return Err(invalid("Player is already in future team squad"));
    }
    if future_vec.spare_slots() == 0 {
        return Err(invalid(
            "Future team squad vector is at capacity (0 spare slots). Terminate or move an unwanted player first."
        ));
    }

    let cur_team = driver.read_ptr(actual_player + CURRENT_TEAM_OFFSET)?;
    let old_contract = driver.read_ptr(player.address + CONTRACT_OFFSET)?;

    let mut edits = Vec::new();

    // 1. Remove from current team squad vector if present
    if cur_team > 0x10000 {
        if let Ok(cur_vec) = SquadVector::read(driver, cur_team) {
            if let Ok(Some(compaction_edits)) = cur_vec.compact_removal_patches(driver, actual_player) {
                edits.extend(compaction_edits);
            }
        }
    }

    // 2. Append to future team squad vector
    let append_edits = future_vec.append_patches(driver, actual_player)?;
    edits.extend(append_edits);

    // 3. Update ACTUAL_PLAYER team pointers
    edits.push(NativeEdit::new(
        actual_player + CURRENT_TEAM_OFFSET,
        cur_team.to_le_bytes().to_vec(),
        future_team.to_le_bytes().to_vec(),
        "update ACTUAL_PLAYER current_team to future_team",
    ));
    let cur_team2 = driver.read_ptr(actual_player + CURRENT_TEAM2_OFFSET)?;
    edits.push(NativeEdit::new(
        actual_player + CURRENT_TEAM2_OFFSET,
        cur_team2.to_le_bytes().to_vec(),
        future_team.to_le_bytes().to_vec(),
        "update ACTUAL_PLAYER current_team copy to future_team",
    ));

    // 4. Update parent contract pointer to the future_contract
    edits.push(NativeEdit::new(
        player.address + CONTRACT_OFFSET,
        old_contract.to_le_bytes().to_vec(),
        future_contract.to_le_bytes().to_vec(),
        "switch parent contract to future_contract",
    ));

    // 5. Clear agreement slot
    edits.push(NativeEdit::new(
        player.address + AGREEMENT_OFFSET,
        agreement_slot.to_le_bytes().to_vec(),
        0usize.to_le_bytes().to_vec(),
        "clear pending agreement slot",
    ));

    // 6. If transfer queue has a scheduled event for this person, clear it
    if let Ok(Some((q_addr, q_old))) = find_queue_entry_for_person(driver, player.address + PERSON_OFFSET) {
        let mut q_new = q_old.clone();
        q_new[8..16].copy_from_slice(&0u64.to_le_bytes());
        q_new[16..20].copy_from_slice(&0u32.to_le_bytes());
        edits.push(NativeEdit::new(
            q_addr,
            q_old,
            q_new,
            "clear scheduled transfer queue entry",
        ));
    }

    apply_transaction(driver, &edits)?;
    Ok((cur_team, future_team))
}

/// Accelerates the scheduled join date of a pending future transfer.
/// Returns (old_date_raw, new_date_raw).
pub fn accelerate_pending_transfer(
    driver: &LinuxMemoryDriver,
    player: &NativePlayerRecord,
    new_join_date: Option<u32>,
) -> Result<(u32, u32)> {
    let agreement_slot = driver.read_ptr(player.address + AGREEMENT_OFFSET)?;
    if agreement_slot <= 0x10000 {
        return Err(invalid("Player has no arranged future transfer agreement"));
    }

    let vec_begin = driver.read_ptr(agreement_slot)?;
    if vec_begin <= 0x10000 {
        return Err(invalid("Invalid agreement vector begin pointer"));
    }
    let agree_entry = driver.read_ptr(vec_begin)?;
    if agree_entry <= 0x10000 {
        return Err(invalid("Invalid agreement entry pointer"));
    }

    let future_contract = driver.read_ptr(agree_entry + 0x10)?;
    if future_contract <= 0x10000 {
        return Err(invalid("Agreement has no valid future contract pointer"));
    }

    let old_date_raw: u32 = driver.read(future_contract + 0x44)?;
    let next_date_raw = new_join_date.unwrap_or(0x07e90104);

    let mut edits = Vec::new();

    // 1. Update future contract start date at +0x44
    edits.push(NativeEdit::new(
        future_contract + 0x44,
        old_date_raw.to_le_bytes().to_vec(),
        next_date_raw.to_le_bytes().to_vec(),
        "accelerate future contract start date",
    ));

    // 2. Update transfer queue entry date if found
    if let Ok(Some((q_addr, q_old))) = find_queue_entry_for_person(driver, player.address + PERSON_OFFSET) {
        let mut q_new = q_old.clone();
        q_new[16..20].copy_from_slice(&next_date_raw.to_le_bytes());
        edits.push(NativeEdit::new(
            q_addr,
            q_old,
            q_new,
            "accelerate transfer queue entry date",
        ));
    }

    apply_transaction(driver, &edits)?;
    Ok((old_date_raw, next_date_raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_transaction_rolls_back_writes_made_after_a_no_op_edit() {
        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        let mut cells = vec![1u64, 2, 3];
        let base = cells.as_mut_ptr() as usize;
        let edit = |slot: usize, old: u64, next: u64| NativeEdit::new(base + slot * 8, old.to_le_bytes().to_vec(), next.to_le_bytes().to_vec(), "cell");
        // A leading no-op must not shift the rollback range; the third edit's old value is wrong.
        let edits = vec![edit(0, 1, 1), edit(1, 2, 20), edit(2, 99, 30)];
        let error = apply_transaction(&driver, &edits).unwrap_err().to_string();
        assert!(error.contains("rolled back cleanly"), "{error}");
        assert_eq!(cells, vec![1, 2, 3]);
    }

    #[test]
    fn squad_vector_read_and_entries_round_trip() {
        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        let mut team_data = vec![0usize; 32];
        let mut squad_entries = vec![0x1000usize, 0x2000usize, 0x3000usize, 0usize];

        let team_addr = team_data.as_mut_ptr() as usize;
        let entries_addr = squad_entries.as_mut_ptr() as usize;

        // Squad vector at team + 0x38: begin, end, capacity
        team_data[SQUAD_VECTOR_OFFSET / 8] = entries_addr;
        team_data[SQUAD_VECTOR_OFFSET / 8 + 1] = entries_addr + 3 * 8; // 3 entries
        team_data[SQUAD_VECTOR_OFFSET / 8 + 2] = entries_addr + 4 * 8; // capacity 4

        let vec = SquadVector::read(&driver, team_addr).expect("valid squad vector");
        assert_eq!(vec.count(), 3);
        assert_eq!(vec.capacity_count(), 4);
        assert_eq!(vec.spare_slots(), 1);

        let entries = vec.entries(&driver).expect("read entries");
        assert_eq!(entries, vec![0x1000, 0x2000, 0x3000]);
        assert!(vec.contains(&driver, 0x2000).unwrap());
        assert!(!vec.contains(&driver, 0x4000).unwrap());
    }

    #[test]
    fn squad_vector_compaction_and_append() {
        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        let mut team_data = vec![0usize; 32];
        let mut squad_entries = vec![0x1000usize, 0x2000usize, 0x3000usize, 0usize];

        let team_addr = team_data.as_mut_ptr() as usize;
        let entries_addr = squad_entries.as_mut_ptr() as usize;

        team_data[SQUAD_VECTOR_OFFSET / 8] = entries_addr;
        team_data[SQUAD_VECTOR_OFFSET / 8 + 1] = entries_addr + 3 * 8;
        team_data[SQUAD_VECTOR_OFFSET / 8 + 2] = entries_addr + 4 * 8;

        let vec = SquadVector::read(&driver, team_addr).unwrap();
        let removal = vec.compact_removal_patches(&driver, 0x2000).unwrap().expect("patches");
        apply_transaction(&driver, &removal).expect("apply compaction");

        let fresh_vec = SquadVector::read(&driver, team_addr).unwrap();
        assert_eq!(fresh_vec.count(), 2);
        assert_eq!(fresh_vec.entries(&driver).unwrap(), vec![0x1000, 0x3000]);
        assert_eq!(fresh_vec.spare_slots(), 2);

        // Now append 0x4000
        let append = fresh_vec.append_patches(&driver, 0x4000).unwrap();
        apply_transaction(&driver, &append).expect("apply append");

        let fresh_vec2 = SquadVector::read(&driver, team_addr).unwrap();
        assert_eq!(fresh_vec2.count(), 3);
        assert_eq!(fresh_vec2.entries(&driver).unwrap(), vec![0x1000, 0x3000, 0x4000]);
    }

    #[test]
    fn transaction_rollback_on_mismatch() {
        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        let mut target = vec![42usize, 99usize];
        let addr = target.as_mut_ptr() as usize;

        let edits = vec![
            NativeEdit::new(addr, 42usize.to_le_bytes().to_vec(), 100usize.to_le_bytes().to_vec(), "first"),
            NativeEdit::new(addr + 8, 888usize.to_le_bytes().to_vec(), 200usize.to_le_bytes().to_vec(), "mismatch"),
        ];

        let res = apply_transaction(&driver, &edits);
        assert!(res.is_err());
        // Verify first was rolled back to 42
        assert_eq!(target[0], 42);
        assert_eq!(target[1], 99);
    }

    #[test]
    fn full_contract_bytes_layout_matches_native_spec() {
        let vtable = 0x6fffdba64df8usize;
        let person = 0x139480e60usize;
        let team = 0x12d348970usize;
        let wage = 5000u32;
        let expiry_year = 2027u32;
        let bytes = build_full_contract_bytes(vtable, person, team, wage, expiry_year, Some(10));

        assert_eq!(bytes.len(), CONTRACT_SIZE);
        assert_eq!(usize::from_le_bytes(bytes[0..8].try_into().unwrap()), vtable);
        assert_eq!(usize::from_le_bytes(bytes[8..16].try_into().unwrap()), person);
        assert_eq!(usize::from_le_bytes(bytes[16..24].try_into().unwrap()), team);
        assert_eq!(usize::from_le_bytes(bytes[24..32].try_into().unwrap()), team);
        assert_eq!(u32::from_le_bytes(bytes[32..36].try_into().unwrap()), wage);
        // Expiry year
        let packed_expiry = u32::from_le_bytes(bytes[72..76].try_into().unwrap());
        assert_eq!(packed_expiry >> 16, 2027);
        assert_eq!(packed_expiry & 0x1ff, 181);
        // Squad number at +0x5d (which is 92 + 1 = 93)
        assert_eq!(bytes[0x5d], 10);
    }
}
