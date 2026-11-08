use super::*;
use crate::native_catalog::NativeType;
use std::cell::RefCell;
use std::collections::HashMap;

const V_PLAYER: usize = 0x7001_0000;
const V_PERSON: usize = 0x7002_0000;
const V_CLUB: usize = 0x7003_0000;
const V_NATION: usize = 0x7004_0000;

fn catalog() -> RttiCatalog {
    let mut tables = HashMap::new();
    tables.insert(V_PLAYER, NativeType { kind: CatalogKind::Player, subobject_offset: 0 });
    tables.insert(V_PERSON, NativeType { kind: CatalogKind::Player, subobject_offset: 0x288 });
    tables.insert(V_CLUB, NativeType { kind: CatalogKind::Club, subobject_offset: 0 });
    tables.insert(V_NATION, NativeType { kind: CatalogKind::Nation, subobject_offset: 0 });
    RttiCatalog::from_tables(tables)
}

fn words(n: usize) -> Box<[u64]> {
    vec![0u64; n].into_boxed_slice()
}

fn name_block(text: &str) -> Box<[u64]> {
    let mut bytes = (text.len() as u32).to_le_bytes().to_vec();
    bytes.extend_from_slice(text.as_bytes());
    bytes.resize(bytes.len().div_ceil(8) * 8, 0);
    bytes.chunks(8).map(|c| u64::from_le_bytes(c.try_into().unwrap())).collect()
}

fn addr(b: &[u64]) -> usize {
    b.as_ptr() as usize
}

fn raw_record(category: u8, sub: u8, kind: u8, target: usize) -> [u8; 16] {
    let mut r = RelationRecord::homegrown(HomegrownKind::Club, target);
    r.category = category;
    r.sub_context = sub;
    r.kind = kind;
    r.to_bytes()
}

/// A fake player: primary object P, person S, relationship block and vector, plus a club and a nation.
struct Fixture {
    p: Box<[u64]>,
    block: Box<[u64]>,
    vector: Box<[u64]>,
    club: Box<[u64]>,
    nation: Box<[u64]>,
    _names: Vec<Box<[u64]>>,
    driver: LinuxMemoryDriver,
}

impl Fixture {
    /// `existing(club, nation)` fills the vector; `spare` free record slots follow it.
    fn new(existing: impl Fn(usize, usize) -> Vec<[u8; 16]>, spare: usize) -> Self {
        let mut club = words(0x30);
        let mut nation = words(0x10);
        let club_name = name_block("Test FC");
        let nation_name = name_block("Testland");
        club[0] = V_CLUB as u64;
        club[1] = 77;
        club[0xc0 / 8] = addr(&club_name) as u64;
        nation[0] = V_NATION as u64;
        nation[1] = 9;
        nation[0x18 / 8] = addr(&nation_name) as u64;
        let records = existing(addr(&club), addr(&nation));
        let mut p = words(0x80);
        let mut block = words(3);
        let mut vector = words((records.len() + spare) * 2 + 2);
        for (i, r) in records.iter().enumerate() {
            vector[i * 2] = u64::from_le_bytes(r[0..8].try_into().unwrap());
            vector[i * 2 + 1] = u64::from_le_bytes(r[8..16].try_into().unwrap());
        }
        let begin = addr(&vector);
        block[0] = begin as u64;
        block[1] = (begin + records.len() * 16) as u64;
        block[2] = (begin + (records.len() + spare) * 16) as u64;
        p[0] = V_PLAYER as u64;
        p[0x288 / 8] = V_PERSON as u64;
        p[(0x288 + 0x78) / 8] = addr(&block) as u64;
        let driver = LinuxMemoryDriver::new(std::process::id() as i32);
        Self { p, block, vector, club, nation, _names: vec![club_name, nation_name], driver }
    }

    fn record(&self) -> usize {
        addr(&self.p) + 0x148
    }

    fn club(&self) -> usize {
        addr(&self.club)
    }

    fn nation(&self) -> usize {
        addr(&self.nation)
    }

    fn triple(&self) -> [u64; 3] {
        [self.block[0], self.block[1], self.block[2]]
    }

    fn read(&self) -> RelationList {
        read_relations(&self.driver, &catalog(), self.record()).unwrap()
    }

    fn ensure(&self, club: Option<usize>, nation: Option<usize>, allocator: &FakeAllocator) -> Result<EnsureReport> {
        ensure_homegrown(&self.driver, &catalog(), self.record(), club, nation, allocator)
    }
}

struct FakeAllocator {
    calls: RefCell<Vec<usize>>,
    leaked: RefCell<Vec<Box<[u64]>>>,
    address_override: Option<usize>,
    on_allocate: Option<Box<dyn Fn()>>,
}

impl FakeAllocator {
    fn new() -> Self {
        Self { calls: RefCell::default(), leaked: RefCell::default(), address_override: None, on_allocate: None }
    }
}

impl RemoteAllocator for FakeAllocator {
    fn allocate(&self, len: usize) -> Result<usize> {
        self.calls.borrow_mut().push(len);
        if let Some(hook) = &self.on_allocate {
            hook();
        }
        if let Some(address) = self.address_override {
            return Ok(address);
        }
        let block = words(len.div_ceil(8));
        let address = addr(&block);
        self.leaked.borrow_mut().push(block);
        Ok(address)
    }
}

fn others() -> Vec<[u8; 16]> {
    vec![raw_record(4, 25, 8, 0x1234_5670), raw_record(1, 106, 23, 0x5550), raw_record(3, 2, 10, 0x2222_2220)]
}

#[test]
fn record_bytes_match_live_layout() {
    let club = RelationRecord::homegrown(HomegrownKind::Club, 0x12ac_21db0);
    let bytes = club.to_bytes();
    assert_eq!(&bytes[0..8], &0x12ac_21db0u64.to_le_bytes());
    assert_eq!(&bytes[8..16], &[0, 0, 1, 72, 1, 0xff, 0, 0xff]);
    let nation = RelationRecord::homegrown(HomegrownKind::Nation, 0x12c8_20300).to_bytes();
    assert_eq!(&nation[8..16], &[0, 0, 8, 70, 1, 0, 0, 0xff]);
    let parsed = RelationRecord::parse(0x10, &bytes);
    assert!(parsed.is_homegrown(HomegrownKind::Club) && !parsed.is_homegrown(HomegrownKind::Nation));
    assert_eq!(parsed.to_bytes(), bytes);
    // category 1 / sub 106 is not homegrown even with the same category.
    assert!(!RelationRecord::parse(0, &raw_record(1, 106, 23, 5)).is_homegrown(HomegrownKind::Club));
}

#[test]
fn read_validates_vtables_and_vector_shape() {
    let fx = Fixture::new(|_, _| others(), 2);
    let list = fx.read();
    assert_eq!(list.records.len(), 3);
    assert_eq!(list.spare_slots(), 2);

    let read_fails = |tweak: &dyn Fn(&mut Fixture)| {
        let mut bad = Fixture::new(|_, _| others(), 2);
        tweak(&mut bad);
        read_relations(&bad.driver, &catalog(), bad.record()).is_err()
    };
    assert!(read_fails(&|f| f.p[0x288 / 8] = 0xdead_0000), "person vtable");
    assert!(read_fails(&|f| f.p[0] = 0xdead_0000), "primary vtable");
    assert!(read_fails(&|f| f.block[1] = f.block[0] - 16), "end before begin");
    assert!(read_fails(&|f| f.block[1] += 5), "size not a multiple of 16");
    assert!(read_fails(&|f| f.block[2] = f.block[0] + (MAX_RECORDS as u64 + 1) * 16), "too many records");
    assert!(read_fails(&|f| f.p[(0x288 + 0x78) / 8] = 0), "no block");
}

#[test]
fn summary_names_homegrown_targets_by_rtti_only() {
    let fx = Fixture::new(
        |club, nation| {
            vec![
                RelationRecord::homegrown(HomegrownKind::Club, club).to_bytes(),
                RelationRecord::homegrown(HomegrownKind::Nation, nation).to_bytes(),
                // homegrown-shaped club record whose target is a nation, not a CLUB
                RelationRecord::homegrown(HomegrownKind::Club, nation).to_bytes(),
                raw_record(4, 25, 8, 0x1234_5670),
            ]
        },
        0,
    );
    let summary = homegrown_summary(&fx.driver, &catalog(), fx.record()).unwrap();
    assert_eq!(summary.clubs.len(), 1);
    assert_eq!(summary.clubs[0].name.as_deref(), Some("Test FC"));
    assert_eq!(summary.clubs[0].id, Some(77));
    assert_eq!(summary.nations[0].name.as_deref(), Some("Testland"));
    assert_eq!(summary.nations[0].id, Some(9));
    assert_eq!(summary.unresolved_targets, 1);
    assert_eq!(summary.relation_count, 4);
}

#[test]
fn existing_records_are_a_noop() {
    let fx = Fixture::new(
        |club, nation| vec![RelationRecord::homegrown(HomegrownKind::Club, club).to_bytes(), RelationRecord::homegrown(HomegrownKind::Nation, nation).to_bytes()],
        0,
    );
    let before = (fx.triple(), fx.vector.clone());
    let allocator = FakeAllocator::new();
    let report = fx.ensure(Some(fx.club()), Some(fx.nation()), &allocator).unwrap();
    assert_eq!((report.club, report.nation, report.new_array), (Outcome::AlreadyPresent, Outcome::AlreadyPresent, None));
    assert!(allocator.calls.borrow().is_empty());
    assert_eq!(before, (fx.triple(), fx.vector.clone()));
}

#[test]
fn appends_in_capacity_without_allocating() {
    let fx = Fixture::new(|_, _| others(), 3);
    let old = fx.read();
    let allocator = FakeAllocator::new();
    let report = fx.ensure(Some(fx.club()), Some(fx.nation()), &allocator).unwrap();
    assert_eq!((report.club, report.nation, report.new_array), (Outcome::Appended, Outcome::Appended, None));
    assert!(allocator.calls.borrow().is_empty());
    let list = fx.read();
    assert_eq!(list.begin, old.begin);
    assert_eq!(list.cap, old.cap);
    assert_eq!(list.records.len(), old.records.len() + 2);
    for (a, b) in old.records.iter().zip(&list.records) {
        assert_eq!(a.to_bytes(), b.to_bytes(), "existing records must be untouched");
    }
    assert!(list.has_target(HomegrownKind::Club, fx.club()) && list.has_target(HomegrownKind::Nation, fx.nation()));
    // Running it again changes nothing.
    let again = fx.ensure(Some(fx.club()), Some(fx.nation()), &allocator).unwrap();
    assert_eq!((again.club, again.nation), (Outcome::AlreadyPresent, Outcome::AlreadyPresent));
    assert_eq!(fx.read().records.len(), old.records.len() + 2);
}

#[test]
fn one_free_slot_for_two_records_uses_a_new_array() {
    let fx = Fixture::new(|_, _| others(), 1);
    let allocator = FakeAllocator::new();
    let report = fx.ensure(Some(fx.club()), Some(fx.nation()), &allocator).unwrap();
    assert!(report.new_array.is_some());
    assert_eq!(fx.read().records.len(), 5);
}

#[test]
fn full_vector_is_copied_into_a_new_array_and_swapped() {
    let fx = Fixture::new(|_, _| others(), 0);
    let old = fx.read();
    assert_eq!(old.spare_slots(), 0);
    let old_bytes = fx.vector.clone();
    let allocator = FakeAllocator::new();
    let report = fx.ensure(None, Some(fx.nation()), &allocator).unwrap();
    assert_eq!((report.club, report.nation), (Outcome::NotRequested, Outcome::Appended));
    let (new_begin, cap_records) = report.new_array.expect("new array");
    assert_eq!(cap_records, 16);
    assert_eq!(allocator.calls.borrow().as_slice(), &[cap_records * 16]);
    let list = fx.read();
    assert_eq!(list.begin, new_begin);
    assert_eq!(list.cap, new_begin + cap_records * 16);
    assert_eq!(list.records.len(), old.records.len() + 1);
    for (a, b) in old.records.iter().zip(&list.records) {
        assert_eq!(a.to_bytes(), b.to_bytes());
    }
    assert!(list.records.last().unwrap().is_homegrown(HomegrownKind::Nation));
    assert_eq!(fx.vector, old_bytes, "the old array is left untouched (leaked on purpose)");
    // The next append now fits in the new array without allocating again.
    fx.ensure(Some(fx.club()), None, &allocator).unwrap();
    assert_eq!(allocator.calls.borrow().len(), 1);
    assert_eq!(fx.read().records.len(), old.records.len() + 2);
}

#[test]
fn failed_allocation_or_copy_leaves_the_vector_unchanged() {
    let fx = Fixture::new(|_, _| others(), 0);
    let before = fx.triple();
    let mut allocator = FakeAllocator::new();
    allocator.address_override = Some(0x2_0000); // unmapped: the copy write fails
    assert!(fx.ensure(None, Some(fx.nation()), &allocator).is_err());
    assert_eq!(fx.triple(), before);
    allocator.address_override = Some(0x20); // not a sane pointer
    assert!(fx.ensure(None, Some(fx.nation()), &allocator).is_err());
    assert_eq!(fx.triple(), before);
}

#[test]
fn a_vector_changed_by_the_game_during_the_swap_aborts_without_touching_it() {
    let fx = Fixture::new(|_, _| others(), 0);
    let block = addr(&fx.block);
    let begin = fx.block[0];
    let mut allocator = FakeAllocator::new();
    // Simulates the game growing its own vector between our read and our swap.
    allocator.on_allocate = Some(Box::new(move || unsafe { *((block + 16) as *mut u64) += 0x100 }));
    let error = fx.ensure(None, Some(fx.nation()), &allocator).unwrap_err();
    assert!(error.to_string().contains("changed before write"), "{error}");
    assert_eq!(fx.block[0], begin, "begin untouched");
}

#[test]
fn targets_must_be_the_right_rtti_type() {
    let fx = Fixture::new(|_, _| others(), 4);
    let allocator = FakeAllocator::new();
    let before = (fx.triple(), fx.vector.clone());
    assert!(fx.ensure(Some(fx.nation()), None, &allocator).is_err());
    assert!(fx.ensure(None, Some(fx.club()), &allocator).is_err());
    assert!(fx.ensure(Some(0x40), None, &allocator).is_err());
    assert!(fx.ensure(None, None, &allocator).is_err());
    assert_eq!(before, (fx.triple(), fx.vector.clone()));
}

#[test]
fn nationality_and_current_club_links_are_rtti_checked() {
    let mut fx = Fixture::new(|_, _| others(), 0);
    fx.p[(0x148 + 0x1a8) / 8] = fx.nation() as u64;
    assert_eq!(first_nationality(&fx.driver, &catalog(), fx.record()), Some(fx.nation()));
    fx.p[(0x148 + 0x1a8) / 8] = fx.club() as u64;
    assert_eq!(first_nationality(&fx.driver, &catalog(), fx.record()), None);
    assert_eq!(current_club(&fx.driver, &catalog(), fx.record()), None);
}
