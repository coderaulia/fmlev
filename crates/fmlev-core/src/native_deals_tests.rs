use super::*;

const OFFER_VT: usize = 0x5a5b_0000_1000;
const PLAYER_VT: usize = 0x5a5b_0000_1008;
const PERSON_VT: usize = 0x5a5b_0000_1010;
const TEAM_VT: usize = 0x5a5b_0000_1018;
const CLUB_VT: usize = 0x5a5b_0000_1020;
const HYBRID_VT: usize = 0x5a5b_0000_1028;
const STAFF_VT: usize = 0x5a5b_0000_1030;

const CLUB: usize = 0x400;
const BUYER: usize = 0x800;
const SELLER: usize = 0x900;
const PLAYER: usize = 0x1000;
const DEAL: usize = 0x3000;
const BLOCK: usize = 0x3400;

fn types() -> DealTypes {
    DealTypes::from_tables(HashMap::from([
        (OFFER_VT, (DealKind::Offer, 0)),
        (PLAYER_VT, (DealKind::Player, 0)),
        (PERSON_VT, (DealKind::Player, 0x288)),
        (TEAM_VT, (DealKind::Team, 0)),
        (CLUB_VT, (DealKind::Club, 0)),
        (HYBRID_VT, (DealKind::Hybrid, 0x380)),
        (STAFF_VT, (DealKind::Staff, 0x100)),
    ]))
}

struct Arena(Vec<u64>);

impl Arena {
    fn new() -> Self {
        Self(vec![0; 0x20000 / 8])
    }
    fn at(&self, offset: usize) -> usize {
        self.0.as_ptr() as usize + offset
    }
    fn bytes(&mut self) -> &mut [u8] {
        let len = self.0.len() * 8;
        unsafe { std::slice::from_raw_parts_mut(self.0.as_mut_ptr().cast::<u8>(), len) }
    }
    fn put(&mut self, offset: usize, value: &[u8]) {
        self.bytes()[offset..offset + value.len()].copy_from_slice(value);
    }
    fn ptr(&mut self, offset: usize, value: usize) {
        self.put(offset, &value.to_le_bytes());
    }
    fn word(&mut self, offset: usize, value: u32) {
        self.put(offset, &value.to_le_bytes());
    }
    fn string(&mut self, offset: usize, text: &str) -> usize {
        self.word(offset, text.len() as u32);
        self.put(offset + 4, text.as_bytes());
        self.at(offset)
    }
    fn pooled(&mut self, slot: usize, holder: usize, string_at: usize, text: &str) {
        let string = self.string(string_at, text);
        self.ptr(holder, string);
        let holder = self.at(holder);
        self.ptr(slot, holder);
    }
}

fn driver() -> LinuxMemoryDriver {
    LinuxMemoryDriver::new(std::process::id() as i32)
}

/// Mbappe-like deal: R. Madrid (seller) -> buying club, fee 92,818,912, join date 4 Jan 2026 with flag bits,
/// two clause vectors matching the checked screen.
fn world() -> Arena {
    let mut a = Arena::new();
    a.ptr(CLUB, CLUB_VT);
    a.word(CLUB + 8, 1089);
    let name = a.string(0x500, "R. Madrid");
    a.ptr(CLUB + 0xc0, name);
    for (team, index) in [(BUYER, 2527), (SELLER, 1700)] {
        a.ptr(team, TEAM_VT);
        a.put(team + 8, &[(index & 0xff) as u8, (index >> 8) as u8, 0, 0, 9, 0, 0, 0, 9, 0, 0, 0]);
        let club = a.at(CLUB);
        a.ptr(team + 0x30, club);
    }
    // Player primary object at PLAYER (record = +0x148, ids at record + 0x148), person sub-object at +0x288.
    a.ptr(PLAYER, PLAYER_VT);
    a.ptr(PLAYER + 0x288, PERSON_VT);
    let ids = PLAYER + 0x148 + 0x148;
    a.word(ids, 25145);
    a.word(ids + 4, 70001);
    a.word(ids + 8, 70001);
    a.pooled(PLAYER + 0x148 + 0x190, 0x2000, 0x2100, "Kylian");
    a.pooled(PLAYER + 0x148 + 0x198, 0x2008, 0x2200, "Mbappé");

    a.ptr(DEAL, OFFER_VT);
    a.word(DEAL + 8, 1);
    let person = a.at(PLAYER + 0x288);
    a.ptr(DEAL + 0x38, person);
    let (buyer, seller) = (a.at(BUYER), a.at(SELLER));
    a.ptr(DEAL + 0x40, buyer);
    a.ptr(DEAL + 0x48, buyer);
    a.ptr(DEAL + 0x50, seller);
    a.word(DEAL + 0x58, 92_818_912);
    a.word(DEAL + 0x64, u32::MAX);
    a.word(DEAL + 0x68, u32::MAX);
    a.word(DEAL + 0x6c, 0x076c_0001);
    a.word(DEAL + 0x70, 0x07ea_1a04); // 2026, flags 0x1a00, day 4
    a.word(DEAL + 0x74, 0x07ea_1a04);
    a.word(DEAL + 0x78, 0x07ea_8a21);
    a.word(DEAL + 0x7c, 0x076c_0001);
    a.put(DEAL + 0x84, &[0]);
    a.word(DEAL + 0x88, 0x701);
    // Clause block: instalments (12-byte entries) then performance clauses (8-byte entries).
    let (inst, perf) = (a.at(0x3500), a.at(0x3600));
    for (k, v) in [(0, inst), (8, inst + 12), (16, inst + 12), (24, perf), (32, perf + 8), (40, perf + 8)] {
        a.ptr(BLOCK + k, v);
    }
    a.word(0x3500, 29_533_388);
    a.word(0x3504, 24);
    a.word(0x3508, 0x50c);
    a.word(0x3600, 41_509_600);
    a.word(0x3604, 20);
    let block = a.at(BLOCK);
    a.ptr(DEAL + 0xa8, block);
    a
}

#[test]
fn reads_a_deal_with_typed_player_teams_dates_and_clauses() {
    let a = world();
    let deal = read_deal(&driver(), &types(), a.at(DEAL)).unwrap();
    assert_eq!(deal.deal_type_label, Some("transfer"));
    assert_eq!((deal.player.id, deal.player.uid, deal.player.name.as_deref()), (Some(25145), Some(70001), Some("Kylian Mbappé")));
    assert_eq!(deal.buying_team.index, Some(2527));
    assert_eq!(deal.selling_team.as_ref().unwrap().club_name.as_deref(), Some("R. Madrid"));
    assert_eq!(deal.selling_team.as_ref().unwrap().club_id, Some(1089));
    assert_eq!(deal.fee_gbp, 92_818_912);
    assert_eq!(deal.join_date.as_ref().map(|d| d.iso.as_str()), Some("2026-01-04"));
    assert_eq!(deal.other_dates.len(), 4);
    assert!(deal.other_dates[0].date.is_none(), "0x076c0001 is the game's none word");
    assert_eq!(deal.status_label, Some("Offer Made"));
    assert_eq!(deal.clause_a_raw, -1);
    assert!(deal.involves_club(1089) && !deal.involves_club(5));
    let clauses = &deal.clauses;
    assert!(clauses.present && clauses.unavailable_reason.is_none());
    assert_eq!((clauses.instalments[0].amount_gbp, clauses.instalments[0].count), (29_533_388, Some(2)));
    assert_eq!((clauses.performance[0].amount_gbp, clauses.performance[0].threshold), (41_509_600, 20));
    assert_eq!(clauses.total_with_clauses_gbp, Some(92_818_912 + 29_533_388 + 41_509_600));
}

#[test]
fn unknown_status_is_shown_raw_and_null_clause_pointer_means_no_clauses() {
    let mut a = world();
    a.word(DEAL + 0x88, 0xff02);
    a.put(DEAL + 0x84, &[3]);
    a.ptr(DEAL + 0xa8, 0);
    let deal = read_deal(&driver(), &types(), a.at(DEAL)).unwrap();
    assert_eq!(deal.status_label, None);
    assert_eq!(deal.status_text, "Status 0xff02 / +0x84 3");
    assert!(!deal.clauses.present && deal.clauses.total_with_clauses_gbp.is_none());
    assert_eq!(status_label(0x700, 2), Some("Work Permit"));
    assert_eq!(status_label(0x700, 3), Some("Contract Rejected"));
    assert_eq!(status_label(0x100_0701, 2), Some("Confirmation"));
    assert_eq!(status_label(0x701, 2), None, "unconfirmed combinations are never guessed");
}

#[test]
fn bad_clause_lists_are_reported_unavailable_not_guessed() {
    let mut a = world();
    a.ptr(BLOCK + 8, a.at(0x3500) + 5); // end not a multiple of the entry size
    let deal = read_deal(&driver(), &types(), a.at(DEAL)).unwrap();
    assert!(!deal.clauses.present);
    assert!(deal.clauses.unavailable_reason.as_deref().unwrap().contains("bounds"));
}

#[test]
fn candidates_failing_a_guard_are_rejected_with_a_reason() {
    let reject = |mutate: &dyn Fn(&mut Arena)| {
        let mut a = world();
        mutate(&mut a);
        read_deal(&driver(), &types(), a.at(DEAL)).unwrap_err().to_string()
    };
    assert!(reject(&|a| a.word(DEAL + 8, 9)).contains("deal type"));
    assert!(reject(&|a| a.ptr(PLAYER + 0x288, 0x1234)).contains("player"));
    assert!(reject(&|a| a.ptr(DEAL + 0x40, 0x1234)).contains("buying team"));
    assert!(reject(&|a| a.ptr(DEAL + 0x50, 0x1234)).contains("selling team"));
    assert!(reject(&|a| a.word(DEAL + 0x58, u32::MAX)).contains("negative"));
    assert!(reject(&|a| a.word(DEAL + 0x74, 0x0001_0001)).contains("implausible date"));
    let a = world();
    assert!(read_deal(&driver(), &types(), a.at(DEAL + 0x100)).unwrap_err().to_string().contains("not a FULL_TRANSFER_OFFER"));
}

#[test]
fn staff_and_hybrid_deals_report_no_player_id() {
    let mut a = world();
    a.ptr(0x4000, STAFF_VT);
    a.pooled(0x4000 + 0x50, 0x4800, 0x4900, "Coach");
    let staff = a.at(0x4000);
    a.ptr(DEAL + 0x38, staff);
    let deal = read_deal(&driver(), &types(), a.at(DEAL)).unwrap();
    assert_eq!((deal.player.kind, deal.player.id, deal.player.name.as_deref()), ("staff", None, Some("Coach")));
    a.ptr(0x4000, HYBRID_VT);
    let deal = read_deal(&driver(), &types(), a.at(DEAL)).unwrap();
    assert_eq!((deal.player.kind, deal.player.id), ("player_and_staff", None));
    assert!(deal.player.unavailable_reason.is_some());
    assert!(update_deal_with(&driver(), &types(), a.at(DEAL), 25145, DealUpdate { fee_gbp: Some(1), join_date: None }).is_err());
}

#[test]
fn scan_finds_valid_deals_by_vtable_in_this_process() {
    let a = world();
    let d = driver();
    let maps = parse_process_maps(d.pid()).unwrap();
    // The fake vtable only occurs in the arena (plus possibly stale stack copies, which fail validation).
    let scan = scan_deals(&d, &types(), &maps);
    let mine: Vec<_> = scan.deals.iter().filter(|x| x.address >= a.at(0) && x.address < a.at(0x20000)).collect();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].address, a.at(DEAL));
}

#[test]
fn update_changes_only_fee_and_join_date_and_keeps_date_flags() {
    let a = world();
    let d = driver();
    let before = d.read::<[u8; 0xd0]>(a.at(DEAL)).unwrap();
    let deal = update_deal_with(&d, &types(), a.at(DEAL), 25145, DealUpdate { fee_gbp: Some(50_000_000), join_date: Some((2026, 40)) }).unwrap();
    assert_eq!(deal.fee_gbp, 50_000_000);
    let join = deal.join_date.unwrap();
    assert_eq!((join.year, join.day, join.raw), (2026, 40, 0x07ea_1a28), "flag bits 0x1a00 are kept");
    let after = d.read::<[u8; 0xd0]>(a.at(DEAL)).unwrap();
    let changed: Vec<usize> = (0..0xd0).filter(|i| before[*i] != after[*i]).collect();
    assert!(changed.iter().all(|i| (0x58..0x5c).contains(i) || (0x70..0x72).contains(i)), "only fee and join date bytes changed: {changed:?}");
    // Unrelated fields stay.
    assert_eq!(deal.status_raw, 0x701);
    assert_eq!(deal.clauses.performance[0].threshold, 20);
}

#[test]
fn update_refuses_bad_input_and_foreign_players_without_writing() {
    let mut a = world();
    let d = driver();
    let t = types();
    let at = a.at(DEAL);
    let snapshot = || d.read::<[u8; 0xd0]>(at).unwrap();
    let before = snapshot();
    let fee = |f| DealUpdate { fee_gbp: Some(f), join_date: None };
    for bad in [
        update_deal_with(&d, &t, at, 25145, DealUpdate::default()),
        update_deal_with(&d, &t, at, 999, fee(1)),
        update_deal_with(&d, &t, at, 25145, fee(-1)),
        update_deal_with(&d, &t, at, 25145, fee(i64::from(i32::MAX) + 1)),
        update_deal_with(&d, &t, at, 25145, DealUpdate { fee_gbp: None, join_date: Some((2026, 366)) }),
        update_deal_with(&d, &t, at, 25145, DealUpdate { fee_gbp: None, join_date: Some((1800, 1)) }),
    ] {
        assert!(bad.is_err());
    }
    assert_eq!(snapshot(), before);
    // A deal without a join date never gets one invented.
    a.word(DEAL + 0x70, 0x076c_0001);
    let err = update_deal_with(&d, &t, at, 25145, DealUpdate { fee_gbp: None, join_date: Some((2026, 5)) }).unwrap_err();
    assert!(err.to_string().contains("no join date"));
}

#[test]
fn dates_decode_and_format() {
    assert_eq!(decode_date(0x07ea_1605).unwrap().unwrap().iso, "2026-01-05");
    assert_eq!(decode_date(0x07b4_0000 | 60).unwrap().unwrap().iso, "1972-02-29");
    assert!(decode_date(0x076c_0001).unwrap().is_none());
    assert!(decode_date(0x07ea_0000 | 400).is_err());
    assert_eq!(date_with(0x07ea_1a04, 2027, 365), 0x07eb_1b6d);
}
