//! Unit tests for `native_player` (kept in their own file to respect the 1,500-line limit).
use super::*;
fn fixture() -> (Vec<u64>, NativePlayerRecord) {
    let mut data = vec![0u64; 80];
    let a = data.as_mut_ptr() as usize;
    let driver = LinuxMemoryDriver::new(std::process::id() as i32);
    let r = NativePlayerRecord {
        address: a,
        pid: driver.pid(),
        index: 65304,
        uid: 2000448177,
        player_vtable: 42,
        person_vtable: 43,
        attributes_verified: false,
    };
    driver.write(a, &42usize).unwrap();
    driver.write(a + PERSON, &43usize).unwrap();
    driver.write(a + INDEX, &r.index).unwrap();
    driver.write(a + UID, &r.uid).unwrap();
    driver.write(a + UID + 4, &r.uid).unwrap();
    driver.write(a + POSITIONS, &[1u8; 15]).unwrap();
    driver.write(a + LEFT_FOOT, &40u8).unwrap();
    driver.write(a + RIGHT_FOOT, &100u8).unwrap();
    (data, r)
}
#[test]
fn preferred_moves_read_bitset_and_name_only_verified_bits() {
    let (data, record) = fixture();
    let driver = LinuxMemoryDriver::new(std::process::id() as i32);
    driver.write(record.address + PPM_BITSET_OFFSET, &((1u64 << 39) | (1u64 << 11))).unwrap();
    let before = data.clone();
    let ppm = record.read_preferred_moves(&driver).unwrap();
    assert_eq!(ppm.named_moves, vec!["Cuts Inside From Both Wings"]);
    assert_eq!(ppm.unnamed_set_bits, 1);
    assert_eq!(ppm.bits.iter().map(|b| b.bit).collect::<Vec<_>>(), vec![11, 39]);
    assert_eq!(data, before, "preferred-move reads never write");
    // A record whose identity no longer matches is refused rather than guessed.
    driver.write(record.address + INDEX, &7i32).unwrap();
    assert!(record.read_preferred_moves(&driver).is_err());
}

#[test]
fn calendar_year_edits_preserve_month_day_and_packed_flags() {
    // 2028 is leap: June 1 = day 153; 2029 June 1 = day 152.
    assert_eq!(expiry_with_year((2028 << 16) | 0x8800 | 153, 2029).unwrap(), (2029 << 16) | 0x8800 | 152);
    assert_eq!(expiry_with_year((2028 << 16) | 60, 2029).unwrap(), (2029 << 16) | 59);
    assert!(expiry_with_year((2029 << 16) | 366, 2030).is_err());
    assert!(expiry_with_year((2028 << 16) | 153, 0).is_err());
}

#[test]
fn attribute_schema_has_unique_offsets_and_correct_scales() {
    let fields: Vec<_> = native_attributes().collect();
    assert_eq!(fields.len(), 60);
    let offsets: std::collections::HashSet<_> = fields.iter().map(|f| f.offset).collect();
    assert_eq!(offsets.len(), fields.len());
    assert_eq!(fields.iter().find(|f| f.name == "composure").unwrap().offset, 0x4b);
    assert_eq!(fields.iter().find(|f| f.name == "concentration").unwrap().offset, 0x4c);
    let personality = fields.iter().find(|f| f.name == "professionalism").unwrap();
    assert_eq!(personality.decode(12).unwrap(), 12);
    assert!(personality.decode(60).is_err());
}

#[test]
fn indexed_abilities_require_calibration_and_preserve_unread_contract_fields() {
    let (_data, mut record) = fixture();
    let driver = LinuxMemoryDriver::new(std::process::id() as i32);
    driver.write(record.address + 0x11c, &125u16).unwrap();
    driver.write(record.address + 0x11e, &175u16).unwrap();
    let mut player = Player::default();
    player.contract.wage_per_week = 543;
    player.contract.expiry_year = 2029;
    let contract = player.contract.clone();
    assert!(record.refresh_abilities(&driver, &mut player).is_err());
    let mut observed = PlayerAttributes::default();
    for field in NATIVE_OUTFIELD_ATTRIBUTES {
        driver.write(record.address + field.offset, &50u8).unwrap();
        (field.set)(&mut observed, 10);
    }
    record.verify_attributes(&driver, &observed).unwrap();
    record.refresh_abilities(&driver, &mut player).unwrap();
    assert_eq!((player.current_ability, player.potential_ability), (125, 175));
    assert_eq!(player.contract, contract);
    driver.write(record.address + 0x11c, &201u16).unwrap();
    driver.write(record.address + 0x11e, &0u16).unwrap();
    record.refresh_abilities(&driver, &mut player).unwrap();
    assert_eq!((player.current_ability, player.potential_ability), (0, 0));
}

#[test]
fn goalkeeper_attributes_verify_and_calibrate_layout() {
    let (_data, mut record) = fixture();
    let driver = LinuxMemoryDriver::new(std::process::id() as i32);
    driver.write(record.address + 0x11c, &130u16).unwrap();
    driver.write(record.address + 0x11e, &160u16).unwrap();
    let mut observed = PlayerAttributes::default();
    for field in NATIVE_GOALKEEPER_ATTRIBUTES {
        driver.write(record.address + field.offset, &65u8).unwrap();
        (field.set)(&mut observed, 13);
    }
    record.verify_attributes(&driver, &observed).unwrap();
    assert!(record.attributes_verified());
    let mut player = Player::default();
    record.refresh_abilities(&driver, &mut player).unwrap();
    assert_eq!((player.current_ability, player.potential_ability), (130, 160));
}

#[test]
fn mixed_attributes_abilities_and_owned_contract_round_trip() {
    struct ContractTag(String);
    impl std::fmt::Display for ContractTag {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.0) }
    }
    let tag = ContractTag(String::from("contract type"));
    let typed: &dyn std::fmt::Display = &tag;
    // A genuine read-only vtable with an executable destructor, for contract guards.
    let (_, vtable): (usize, usize) = unsafe { std::mem::transmute(typed) };
    let (data, mut r) = fixture();
    let mut contract = vec![0u64; 32];
    let c = contract.as_mut_ptr() as usize;
    let d = LinuxMemoryDriver::new(std::process::id() as i32);
    d.write(c, &vtable).unwrap();
    d.write(c + 8, &(r.address + PERSON)).unwrap();
    d.write(c + 0x20, &542u32).unwrap();
    d.write(c + 0x48, &((2028u32 << 16) | 153)).unwrap();
    d.write(r.address + 0x1e8, &c).unwrap();
    d.write(r.address + 0x11c, &80u16).unwrap();
    d.write(r.address + 0x11e, &104u16).unwrap();
    let mut observed = PlayerAttributes::default();
    for field in native_attributes() {
        d.write(r.address + field.offset, &(10u8 * field.scale)).unwrap();
        (field.set)(&mut observed, 10);
    }
    r.verify_attributes(&d, &observed).unwrap();
    let mut previous = Player { attributes: observed, positions: r.read_positions(&d).unwrap(), ..Player::default() };
    r.refresh_player_fields(&d, &mut previous).unwrap();
    let original = data.clone();
    let original_contract = contract.clone();
    let mut requested = previous.clone();
    requested.attributes.passing = 15;
    requested.attributes.professionalism = 12;
    requested.current_ability = 81;
    requested.potential_ability = 105;
    requested.contract.wage_per_week = 543;
    requested.contract.expiry_year = 2029;
    r.write_player_edits(&d, &previous, &requested).unwrap();
    let mut actual = previous.clone();
    r.refresh_player_fields(&d, &mut actual).unwrap();
    assert!(actual.preferred_moves.is_some(), "refresh publishes the read-only PPM bitset");
    assert_eq!(actual, requested);
    r.write_player_edits(&d, &actual, &previous).unwrap();
    assert_eq!(data, original);
    assert_eq!(contract, original_contract);
    // A contract that now belongs to somebody else is rejected before Passing changes.
    d.write(c + 8, &0usize).unwrap();
    assert!(r.write_player_edits(&d, &previous, &requested).is_err());
    assert_eq!(data, original);
}

#[test]
fn squad_status_and_release_clause_read_write_and_guard_loans() {
    struct ContractTag(String);
    impl std::fmt::Display for ContractTag {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.0) }
    }
    let tag = ContractTag(String::from("contract type"));
    let typed: &dyn std::fmt::Display = &tag;
    let (_, vtable): (usize, usize) = unsafe { std::mem::transmute(typed) };
    let (_data, mut r) = fixture();
    // Contract with squad status 4 (Squad Player) and a clause list: appearance fee plus a 1,000,000 release clause.
    let mut contract = vec![0u64; 32];
    let c = contract.as_mut_ptr() as usize;
    let mut clauses = vec![0u8; 16];
    clauses[0..4].copy_from_slice(&1612u32.to_le_bytes());
    clauses[4..6].copy_from_slice(&0xffffu16.to_le_bytes());
    clauses[6..8].copy_from_slice(&0x20u16.to_le_bytes());
    clauses[8..12].copy_from_slice(&1_000_000u32.to_le_bytes());
    clauses[12..14].copy_from_slice(&0xffffu16.to_le_bytes());
    clauses[14..16].copy_from_slice(&0u16.to_le_bytes());
    let begin = clauses.as_ptr() as usize;
    let d = LinuxMemoryDriver::new(std::process::id() as i32);
    d.write(c, &vtable).unwrap();
    d.write(c + 8, &(r.address + PERSON)).unwrap();
    d.write(c + 0x20, &542u32).unwrap();
    d.write(c + 0x48, &((2028u32 << 16) | 153)).unwrap();
    d.write(c + 0x54, &4u8).unwrap();
    d.write(c + 0x56, &4u8).unwrap();
    d.write(c + 0x68, &begin).unwrap();
    d.write(c + 0x70, &(begin + 16)).unwrap();
    d.write(c + 0x78, &(begin + 16)).unwrap();
    d.write(r.address + 0x1e8, &c).unwrap();
    d.write(r.address + 0x11c, &80u16).unwrap();
    d.write(r.address + 0x11e, &104u16).unwrap();
    let mut observed = PlayerAttributes::default();
    for field in native_attributes() {
        d.write(r.address + field.offset, &(10u8 * field.scale)).unwrap();
        (field.set)(&mut observed, 10);
    }
    r.verify_attributes(&d, &observed).unwrap();
    let mut previous = Player { attributes: observed, positions: r.read_positions(&d).unwrap(), ..Player::default() };
    r.refresh_player_fields(&d, &mut previous).unwrap();
    assert_eq!(previous.contract.squad_status, "Squad Player");
    assert_eq!(previous.contract.squad_status_code, Some(4));
    assert_eq!((previous.contract.release_clause, previous.contract.release_clause_present), (1_000_000, Some(true)));
    assert_eq!(previous.contract.happiness, "", "happiness is never read or invented");

    let mut requested = previous.clone();
    requested.contract.squad_status = "Important Player".to_string();
    requested.contract.release_clause = 2_500_000;
    r.write_player_edits(&d, &previous, &requested).unwrap();
    assert_eq!((d.read::<u8>(c + 0x54).unwrap(), d.read::<u8>(c + 0x56).unwrap()), (2, 2));
    assert_eq!(u32::from_le_bytes(clauses[8..12].try_into().unwrap()), 2_500_000);
    assert_eq!(u32::from_le_bytes(clauses[0..4].try_into().unwrap()), 1612, "the other clause entry is untouched");
    let mut actual = previous.clone();
    r.refresh_player_fields(&d, &mut actual).unwrap();
    assert_eq!((actual.contract.squad_status.as_str(), actual.contract.release_clause), ("Important Player", 2_500_000));

    // Invalid values and an unlabelled status are refused before anything changes.
    let mut bad = actual.clone();
    bad.contract.squad_status = "Hot Prospect".to_string();
    assert!(r.write_player_edits(&d, &actual, &bad).is_err());
    let mut zero = actual.clone();
    zero.contract.release_clause = 0;
    assert!(r.write_player_edits(&d, &actual, &zero).is_err());
    assert_eq!(d.read::<u8>(c + 0x54).unwrap(), 2);

    // A loaned player's parent-contract terms are neither shown nor edited.
    // A non-null node with an empty first pointer is not a loan (reserve-team player).
    let mut empty_node = vec![0u64; 4];
    d.write(r.address + LOAN_NODE_OFFSET, &(empty_node.as_mut_ptr() as usize)).unwrap();
    let mut reserve = actual.clone();
    r.refresh_player_fields(&d, &mut reserve).unwrap();
    assert_eq!(reserve.contract.squad_status, "Important Player", "an empty loan node is not a loan");
    // A node whose first pointer is an agreement owned by this player is a loan.
    let mut agreement = vec![0u64; 4];
    agreement[1] = (r.address + PERSON) as u64;
    empty_node[0] = agreement.as_mut_ptr() as u64;
    let mut loaned = actual.clone();
    r.refresh_player_fields(&d, &mut loaned).unwrap();
    assert_eq!((loaned.contract.squad_status.as_str(), loaned.contract.squad_status_code, loaned.contract.release_clause_present), ("", None, None));
    assert!(loaned.contract.terms_note.contains("on loan"));
    let mut edit = actual.clone();
    edit.contract.squad_status = "Star Player".to_string();
    assert!(r.write_player_edits(&d, &actual, &edit).is_err());
    assert_eq!(d.read::<u8>(c + 0x54).unwrap(), 2);
    drop(contract);
}

#[test]
fn calibrated_type_discovers_another_player_and_rejects_wrong_vtable() {
    let (_seed_data, mut seed) = fixture();
    let (_peer_data, peer) = fixture();
    let d = LinuxMemoryDriver::new(std::process::id() as i32);
    let mut text = vec![0u64; 4];
    let string = text.as_mut_ptr() as usize;
    d.write(string, &4u32).unwrap();
    d.write(string + 4, b"Peer").unwrap();
    let reference = vec![string];
    let mut attrs = PlayerAttributes::default();
    for record in [&seed, &peer] {
        for field in native_attributes() { d.write(record.address + field.offset, &(10u8 * field.scale)).unwrap(); (field.set)(&mut attrs, 10); }
        for offset in [0x190, 0x198] { d.write(record.address + offset, &(reference.as_ptr() as usize)).unwrap(); }
    }
    d.write(peer.address + INDEX, &54762i32).unwrap();
    d.write(peer.address + UID, &2000249370u32).unwrap();
    d.write(peer.address + UID + 4, &2000249370u32).unwrap();
    assert!(seed.peer_at(&d, peer.address).is_err());
    seed.verify_attributes(&d, &attrs).unwrap();
    let found = seed.peer_at(&d, peer.address).unwrap();
    assert_eq!(found.index(), 54762);
    assert_eq!(found.name(&d).unwrap(), "Peer Peer");
    d.write(peer.address + PERSON, &44usize).unwrap();
    assert!(seed.peer_at(&d, peer.address).is_err());
}

#[test]
fn attributes_and_positions_save_together_without_touching_other_bytes() {
    let (data, mut r) = fixture();
    let d = LinuxMemoryDriver::new(std::process::id() as i32);
    let mut observed = PlayerAttributes::default();
    for field in native_attributes() {
        d.write(r.address + field.offset, &if field.scale == 5 { 53u8 } else { 11u8 }).unwrap();
        (field.set)(&mut observed, 11);
    }
    r.verify_attributes(&d, &observed).unwrap();
    let before = data.clone();
    let previous = Player { positions: r.read_positions(&d).unwrap(), attributes: observed, ..Player::default() };
    let mut requested = previous.clone();
    requested.attributes.passing = 20;
    requested.positions.dr = 19;
    r.write_player_edits(&d, &previous, &requested).unwrap();
    let mut actual = PlayerAttributes::default();
    r.refresh_attributes(&d, &mut actual).unwrap();
    assert_eq!(actual.passing, 20);
    assert_eq!(r.read_positions(&d).unwrap().dr, 19);
    for i in 0..data.len() * 8 {
        if [0x1e, 12].contains(&i) { continue; }
        let expected = (before[i / 8] >> ((i % 8) * 8)) as u8;
        assert_eq!(d.read::<u8>(r.address + i).unwrap(), expected, "byte {i:x}");
    }
}

#[test]
fn invalid_last_attribute_prevents_every_write() {
    let (data, mut r) = fixture();
    let d = LinuxMemoryDriver::new(std::process::id() as i32);
    let mut attrs = PlayerAttributes::default();
    for field in native_attributes() {
        d.write(r.address + field.offset, &(10u8 * field.scale)).unwrap();
        (field.set)(&mut attrs, 10);
    }
    r.verify_attributes(&d, &attrs).unwrap();
    let before = data.clone();
    let previous = Player { attributes: attrs, positions: r.read_positions(&d).unwrap(), ..Player::default() };
    let mut requested = previous.clone();
    requested.attributes.passing = 20;
    requested.attributes.controversy = 21;
    assert!(r.write_player_edits(&d, &previous, &requested).is_err());
    assert_eq!(data, before);
}

#[test]
fn conflict_rolls_back_exact_fractional_bytes() {
    let (_data, r) = fixture();
    let d = LinuxMemoryDriver::new(std::process::id() as i32);
    d.write(r.address + 0x1e, &53u8).unwrap();
    d.write(r.address + 0x20, &61u8).unwrap();
    assert!(r.apply_byte_edits(&d, &[(0x1e, 53, 100), (0x20, 60, 100)]).is_err());
    assert_eq!(d.read::<u8>(r.address + 0x1e).unwrap(), 53);
    assert_eq!(d.read::<u8>(r.address + 0x20).unwrap(), 61);
}

#[test]
fn attribute_verification_requires_all_observed_fields() {
    let (_data, mut r) = fixture();
    let d = LinuxMemoryDriver::new(std::process::id() as i32);
    let attrs = PlayerAttributes::default();
    assert!(r.verify_attributes(&d, &attrs).is_err());
    assert!(!r.attributes_verified());
    assert!(r.refresh_attributes(&d, &mut PlayerAttributes::default()).is_err());
}

#[test]
fn attribute_scale_rounds_without_truncation_or_overflow() {
    for (raw, value) in [(0, 1), (4, 1), (5, 1), (7, 1), (8, 2), (52, 10), (53, 11), (99, 20), (100, 20)] {
        assert_eq!(displayed_rating(raw).unwrap(), value);
    }
    assert!(displayed_rating(101).is_err());
}

#[test]
fn native_round_trip_preserves_identity_and_surrounding_bytes() {
    let (data, r) = fixture();
    let before = data.clone();
    let d = LinuxMemoryDriver::new(std::process::id() as i32);
    let mut p = r.read_positions(&d).unwrap();
    p.dl = 15;
    p.wbr = 14;
    p.left_foot = 15;
    r.write_positions(&d, &p).unwrap();
    assert_eq!(r.read_positions(&d).unwrap(), p);
    for i in 0..data.len() * 8 {
        if (POSITIONS..POSITIONS + 15).contains(&i) || i == LEFT_FOOT || i == RIGHT_FOOT {
            continue;
        }
        let expected = (before[i / 8] >> ((i % 8) * 8)) as u8;
        assert_eq!(d.read::<u8>(r.address + i).unwrap(), expected, "byte {i:x}");
    }
}
#[test]
fn rollback_restores_all_fields_but_refuses_recycled_record() {
    let (_data, r) = fixture();
    let d = LinuxMemoryDriver::new(std::process::id() as i32);
    let old = d.read::<[u8; 15]>(r.address + POSITIONS).unwrap();
    d.write(r.address + POSITIONS, &[20u8; 15]).unwrap();
    d.write(r.address + LEFT_FOOT, &75u8).unwrap();
    d.write(r.address + RIGHT_FOOT, &75u8).unwrap();
    r.rollback_positions(&d, &old, 40, 100).unwrap();
    assert_eq!(d.read::<[u8; 15]>(r.address + POSITIONS).unwrap(), old);
    assert_eq!(d.read::<u8>(r.address + LEFT_FOOT).unwrap(), 40);
    assert_eq!(d.read::<u8>(r.address + RIGHT_FOOT).unwrap(), 100);
    d.write(r.address + INDEX, &99i32).unwrap();
    assert!(r.rollback_positions(&d, &[20u8; 15], 75, 75).is_err());
    assert_eq!(d.read::<[u8; 15]>(r.address + POSITIONS).unwrap(), old);
    assert_eq!(d.read::<u8>(r.address + LEFT_FOOT).unwrap(), 40);
}
#[test]
fn rejects_both_stale_uids_and_vtables() {
    let (_data, r) = fixture();
    let d = LinuxMemoryDriver::new(std::process::id() as i32);
    for offset in [0, PERSON, UID, UID + 4] {
        let original = d.read::<u32>(r.address + offset).unwrap();
        d.write(r.address + offset, &0u32).unwrap();
        assert!(r.read_positions(&d).is_err(), "identity offset {offset:x}");
        assert!(r.rollback_positions(&d, &[20u8; 15], 100, 100).is_err());
        d.write(r.address + offset, &original).unwrap();
    }
}
#[test]
fn rating_boundaries_and_fractional_feet() {
    let (_data, r) = fixture();
    let d = LinuxMemoryDriver::new(std::process::id() as i32);
    for value in [1, 20] {
        let mut p = r.read_positions(&d).unwrap();
        p.dl = value;
        p.left_foot = value;
        p.right_foot = value;
        r.write_positions(&d, &p).unwrap();
        assert_eq!(r.read_positions(&d).unwrap(), p);
    }
    let p = r.read_positions(&d).unwrap();
    for value in [0, 21] {
        let mut invalid = p.clone();
        invalid.left_foot = value;
        assert!(r.write_positions(&d, &invalid).is_err());
        invalid = p.clone();
        invalid.right_foot = value;
        assert!(r.write_positions(&d, &invalid).is_err());
        invalid = p.clone();
        invalid.dr = value;
        assert!(r.write_positions(&d, &invalid).is_err());
        assert_eq!(r.read_positions(&d).unwrap(), p);
    }
    // Off-scale native feet are valid (generated players); only the 1..=100 range is enforced.
    d.write(r.address + LEFT_FOOT, &99u8).unwrap();
    assert_eq!(r.read_positions(&d).unwrap().left_foot, 20);
    d.write(r.address + LEFT_FOOT, &101u8).unwrap();
    assert!(r.read_positions(&d).is_err());
}
#[test]
fn rejects_stale_identity_and_invalid_ratings_before_writing() {
    let (_data, r) = fixture();
    let d = LinuxMemoryDriver::new(std::process::id() as i32);
    let mut p = r.read_positions(&d).unwrap();
    p.dl = 21;
    assert!(r.write_positions(&d, &p).is_err());
    d.write(r.address + INDEX, &7i32).unwrap();
    assert!(r.read_positions(&d).is_err());
    assert!(r.write_positions(&d, &PlayerPositions::default()).is_err());
}

#[test]
fn generated_player_with_an_off_scale_foot_is_read_and_not_normalised_by_a_position_edit() {
    // Reno Salampessy (65949): right foot 74 (shown as "Strong" in game), left foot 100.
    let (data, r) = fixture();
    let d = LinuxMemoryDriver::new(std::process::id() as i32);
    d.write(r.address + LEFT_FOOT, &100u8).unwrap();
    d.write(r.address + RIGHT_FOOT, &74u8).unwrap();
    let mut p = r.read_positions(&d).unwrap();
    assert_eq!((p.left_foot, p.right_foot), (20, 15), "74 reads as the nearest 1-20 rating");
    // An unrelated position edit leaves both foot bytes exactly as they were.
    p.dl = 12;
    r.write_positions(&d, &p).unwrap();
    assert_eq!((d.read::<u8>(r.address + LEFT_FOOT).unwrap(), d.read::<u8>(r.address + RIGHT_FOOT).unwrap()), (100, 74));
    // Changing the foot rating does write the new 5 x rating.
    p.right_foot = 17;
    r.write_positions(&d, &p).unwrap();
    assert_eq!(d.read::<u8>(r.address + RIGHT_FOOT).unwrap(), 85);
    // Out-of-range native values are still rejected.
    d.write(r.address + RIGHT_FOOT, &101u8).unwrap();
    assert!(r.read_positions(&d).is_err());
    d.write(r.address + RIGHT_FOOT, &0u8).unwrap();
    assert!(r.read_positions(&d).is_err());
    drop(data);
}

#[test]
fn foot_rating_is_the_nearest_step_and_matches_the_old_scale_for_multiples_of_five() {
    for rating in 1..=20u8 {
        assert_eq!(foot_rating(rating * 5), rating);
    }
    assert_eq!((foot_rating(1), foot_rating(2), foot_rating(3), foot_rating(74), foot_rating(77), foot_rating(78), foot_rating(100)), (1, 1, 1, 15, 15, 16, 20));
}
