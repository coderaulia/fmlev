//! In-memory search index over the native database (plan G).
//!
//! The index holds identity and display fields only (kind, index, uid, name and one detail such as a
//! player's club). It is never a write target: callers revalidate a record by id and uid before opening
//! it, and drop entries that fail (`remove`). Enumeration lives in `native_catalog`.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;

pub const DEFAULT_LIMIT: usize = 25;
pub const MAX_LIMIT: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Player,
    Staff,
    Club,
    Nation,
    Competition,
}

impl EntityKind {
    pub const ALL: [EntityKind; 5] = [Self::Player, Self::Staff, Self::Club, Self::Nation, Self::Competition];

    /// Plural name used by the HTTP API and UI filter chips.
    pub fn plural(self) -> &'static str {
        match self {
            Self::Player => "players",
            Self::Staff => "staff",
            Self::Club => "clubs",
            Self::Nation => "nations",
            Self::Competition => "competitions",
        }
    }

    pub fn from_plural(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.plural() == value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IndexEntry {
    pub kind: EntityKind,
    /// Native record index (the id the rest of the API selects by).
    pub id: i32,
    pub uid: u32,
    pub name: String,
    /// Player and staff: club name. Club and competition: nation name. Only set when the link was validated.
    pub detail: Option<String>,
    #[serde(skip)]
    pub address: usize,
    /// Players only: scouting fields read during enumeration (`None` when unreadable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<PlayerProfile>,
    #[serde(skip)]
    folded: String,
    #[serde(skip)]
    folded_detail: String,
}

impl IndexEntry {
    pub fn new(kind: EntityKind, id: i32, uid: u32, name: String, detail: Option<String>, address: usize) -> Self {
        let folded = fold(&name);
        let folded_detail = detail.as_deref().map(fold).unwrap_or_default();
        Self { kind, id, uid, name, detail, address, profile: None, folded, folded_detail }
    }

    pub fn with_profile(mut self, profile: Option<PlayerProfile>) -> Self {
        self.profile = profile;
        self
    }
}

/// Position keys in the order of `PlayerProfile::positions` (native bytes 0..15; byte 1 is unused).
pub const POSITION_KEYS: [(&str, usize); 14] = [
    ("gk", 0), ("dl", 2), ("dc", 3), ("dr", 4), ("dm", 5), ("ml", 6), ("mc", 7),
    ("mr", 8), ("aml", 9), ("amc", 10), ("amr", 11), ("st", 12), ("wbl", 13), ("wbr", 14),
];

/// Scouting fields of an indexed player, read at enumeration time (a snapshot: ability changes after the
/// build show only after a rebuild; opening the player reads the live values).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PlayerProfile {
    /// 1..=200, `None` when the native value is outside that range.
    pub ca: Option<u8>,
    pub pa: Option<u8>,
    /// Native position ratings (1..=20), see `POSITION_KEYS`.
    pub positions: [u8; 15],
}

/// Filters of a scouting search. Every set field must match; ability and position filters only match
/// players whose profile was readable.
#[derive(Debug, Clone, Default)]
pub struct ScoutFilter {
    pub kind: Option<EntityKind>,
    /// Name tokens (players, staff) and, for staff, the club.
    pub text: String,
    /// Case-insensitive substring of the club name.
    pub club: String,
    pub ca_min: Option<u8>,
    pub ca_max: Option<u8>,
    pub pa_min: Option<u8>,
    pub pa_max: Option<u8>,
    /// Index into `POSITION_KEYS`-style native positions plus the minimum rating.
    pub position: Option<(usize, u8)>,
    pub sort: ScoutSort,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScoutSort {
    #[default]
    Name,
    Ca,
    Pa,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct KindPage {
    pub hits: Vec<IndexEntry>,
    /// Matches before paging.
    pub total: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SearchIndex {
    pub pid: i32,
    pub built_at_unix: u64,
    pub duration_ms: u64,
    pub duplicates_dropped: usize,
    by_kind: BTreeMap<EntityKind, Vec<IndexEntry>>,
}

impl SearchIndex {
    pub fn new(pid: i32, entries: Vec<IndexEntry>, duplicates_dropped: usize, duration_ms: u64) -> Self {
        let mut by_kind: BTreeMap<EntityKind, Vec<IndexEntry>> = BTreeMap::new();
        for entry in entries {
            by_kind.entry(entry.kind).or_default().push(entry);
        }
        for list in by_kind.values_mut() {
            list.sort_by(|a, b| a.folded.cmp(&b.folded).then(a.id.cmp(&b.id)));
        }
        let built_at_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        Self { pid, built_at_unix, duration_ms, duplicates_dropped, by_kind }
    }

    pub fn counts(&self) -> BTreeMap<&'static str, usize> {
        EntityKind::ALL.into_iter().map(|kind| (kind.plural(), self.entries(kind).len())).collect()
    }

    pub fn entries(&self, kind: EntityKind) -> &[IndexEntry] {
        self.by_kind.get(&kind).map_or(&[], Vec::as_slice)
    }

    pub fn get(&self, kind: EntityKind, id: i32) -> Option<&IndexEntry> {
        self.entries(kind).iter().find(|entry| entry.id == id)
    }

    /// Drops a stale entry (record moved or vanished). Returns whether it was present.
    pub fn remove(&mut self, kind: EntityKind, id: i32) -> bool {
        let Some(list) = self.by_kind.get_mut(&kind) else { return false };
        let before = list.len();
        list.retain(|entry| entry.id != id);
        before != list.len()
    }

    /// Ranked, paged matches per kind. `kind = None` searches every kind with the same paging.
    /// Every query token must appear in the folded name; a numeric query also matches id or uid exactly.
    pub fn query(&self, query: &str, kind: Option<EntityKind>, limit: usize, offset: usize) -> BTreeMap<EntityKind, KindPage> {
        let folded = fold(query.trim());
        let tokens: Vec<&str> = folded.split_whitespace().collect();
        let number = query.trim().parse::<i64>().ok();
        let limit = limit.clamp(1, MAX_LIMIT);
        let kinds: Vec<EntityKind> = kind.map_or_else(|| EntityKind::ALL.to_vec(), |kind| vec![kind]);
        let mut pages = BTreeMap::new();
        for kind in kinds {
            let mut scored: Vec<(u8, &IndexEntry)> = if tokens.is_empty() {
                Vec::new()
            } else {
                self.entries(kind)
                    .iter()
                    .filter_map(|entry| score(entry, &folded, &tokens, number).map(|s| (s, entry)))
                    .collect()
            };
            // Entries are pre-sorted by folded name then id, and the sort is stable.
            scored.sort_by_key(|(s, _)| *s);
            let total = scored.len();
            let hits = scored.into_iter().skip(offset).take(limit).map(|(_, entry)| entry.clone()).collect();
            pages.insert(kind, KindPage { hits, total });
        }
        pages
    }
}

impl SearchIndex {
    /// Filtered, sorted, paged listing of players or staff. Returns the page and the total before paging.
    pub fn scout(&self, filter: &ScoutFilter, limit: usize, offset: usize) -> (Vec<IndexEntry>, usize) {
        let kind = filter.kind.unwrap_or(EntityKind::Player);
        let folded = fold(filter.text.trim());
        let tokens: Vec<&str> = folded.split_whitespace().collect();
        let club = fold(filter.club.trim());
        let number = filter.text.trim().parse::<i64>().ok();
        let needs_profile = filter.ca_min.is_some() || filter.ca_max.is_some() || filter.pa_min.is_some()
            || filter.pa_max.is_some() || filter.position.is_some() || filter.sort != ScoutSort::Name;
        let in_range = |value: Option<u8>, min: Option<u8>, max: Option<u8>| match (value, min, max) {
            (_, None, None) => true,
            (Some(v), min, max) => min.is_none_or(|m| v >= m) && max.is_none_or(|m| v <= m),
            (None, _, _) => false,
        };
        let mut hits: Vec<&IndexEntry> = self
            .entries(kind)
            .iter()
            .filter(|entry| {
                if !tokens.is_empty() && score(entry, &folded, &tokens, number).is_none() {
                    return false;
                }
                if !club.is_empty() && !entry.folded_detail.contains(&club) {
                    return false;
                }
                if kind != EntityKind::Player {
                    return !needs_profile;
                }
                let profile = entry.profile.as_ref();
                in_range(profile.and_then(|p| p.ca), filter.ca_min, filter.ca_max)
                    && in_range(profile.and_then(|p| p.pa), filter.pa_min, filter.pa_max)
                    && filter.position.is_none_or(|(slot, min)| profile.is_some_and(|p| p.positions.get(slot).is_some_and(|r| *r >= min)))
            })
            .collect();
        match filter.sort {
            ScoutSort::Name => {}
            ScoutSort::Ca => hits.sort_by_key(|e| std::cmp::Reverse(e.profile.and_then(|p| p.ca).unwrap_or(0))),
            ScoutSort::Pa => hits.sort_by_key(|e| std::cmp::Reverse(e.profile.and_then(|p| p.pa).unwrap_or(0))),
        }
        let total = hits.len();
        (hits.into_iter().skip(offset).take(limit.clamp(1, MAX_LIMIT)).cloned().collect(), total)
    }
}

/// Lower rank is better: 0 exact name or id/uid, 1 name prefix, 2 every token starts a word, 3 substring.
fn score(entry: &IndexEntry, folded: &str, tokens: &[&str], number: Option<i64>) -> Option<u8> {
    if number.is_some_and(|n| n == i64::from(entry.id) || n == i64::from(entry.uid)) {
        return Some(0);
    }
    // Staff are also found by their club ("madrid" lists the club's staff); the rank stays name-based.
    let by_club = entry.kind == EntityKind::Staff;
    if !tokens.iter().all(|token| entry.folded.contains(token) || (by_club && entry.folded_detail.contains(token))) {
        return None;
    }
    if entry.folded == folded {
        return Some(0);
    }
    if entry.folded.starts_with(folded) {
        return Some(1);
    }
    let words: Vec<&str> = entry.folded.split(|c: char| c.is_whitespace() || c == '-' || c == '\'').collect();
    if tokens.iter().all(|token| words.iter().any(|word| word.starts_with(token))) {
        return Some(2);
    }
    Some(3)
}

/// Lowercases and strips diacritics so "mbappe" finds "Mbappé" and "sorloth" finds "Sørloth".
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match fold_char(c) {
            Some(mapped) => out.push_str(mapped),
            None => out.push(c),
        }
    }
    out
}

fn fold_char(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' | 'ǎ' | 'ạ' | 'ả' | 'ấ' | 'ầ' | 'ẩ' | 'ẫ' | 'ậ' | 'ắ' | 'ằ' | 'ẳ' | 'ẵ' | 'ặ' => "a",
        'æ' => "ae",
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => "c",
        'ď' | 'đ' | 'ð' => "d",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' | 'ẹ' | 'ẻ' | 'ẽ' | 'ế' | 'ề' | 'ể' | 'ễ' | 'ệ' | 'ə' => "e",
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => "g",
        'ĥ' | 'ħ' => "h",
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' | 'ǐ' | 'ỉ' | 'ị' => "i",
        'ĵ' => "j",
        'ķ' => "k",
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => "l",
        'ñ' | 'ń' | 'ņ' | 'ň' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' | 'ơ' | 'ǒ' | 'ọ' | 'ỏ' | 'ố' | 'ồ' | 'ổ' | 'ỗ' | 'ộ' | 'ớ' | 'ờ' | 'ở' | 'ỡ' | 'ợ' => "o",
        'œ' => "oe",
        'ŕ' | 'ŗ' | 'ř' => "r",
        'ś' | 'ŝ' | 'ş' | 'š' | 'ș' => "s",
        'ß' => "ss",
        'ţ' | 'ť' | 'ŧ' | 'ț' => "t",
        'þ' => "th",
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' | 'ư' | 'ǔ' | 'ụ' | 'ủ' | 'ứ' | 'ừ' | 'ử' | 'ữ' | 'ự' => "u",
        'ŵ' => "w",
        'ý' | 'ÿ' | 'ŷ' | 'ỳ' | 'ỵ' | 'ỷ' | 'ỹ' => "y",
        'ź' | 'ż' | 'ž' => "z",
        _ => return None,
    })
}

/// Keeps ids seen once per kind; ids found at several addresses are dropped rather than guessed.
/// Returns the unique entries and how many were dropped.
pub fn drop_duplicate_ids(entries: Vec<IndexEntry>) -> (Vec<IndexEntry>, usize) {
    let mut counts: HashMap<(EntityKind, i32), usize> = HashMap::new();
    for entry in &entries {
        *counts.entry((entry.kind, entry.id)).or_default() += 1;
    }
    let before = entries.len();
    let unique: Vec<IndexEntry> = entries.into_iter().filter(|entry| counts[&(entry.kind, entry.id)] == 1).collect();
    let dropped = before - unique.len();
    (unique, dropped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: EntityKind, id: i32, name: &str) -> IndexEntry {
        IndexEntry::new(kind, id, 1000 + id as u32, name.to_string(), None, 0x1000 * id as usize)
    }

    fn sample() -> SearchIndex {
        SearchIndex::new(
            42,
            vec![
                entry(EntityKind::Player, 1, "Kylian Mbappé"),
                entry(EntityKind::Player, 2, "Lamine Yamal"),
                entry(EntityKind::Player, 3, "Alexander Sørloth"),
                entry(EntityKind::Player, 4, "Mbappe Junior"),
                entry(EntityKind::Player, 5, "Ethan Mbappé"),
                entry(EntityKind::Club, 10, "FC Barcelona"),
                entry(EntityKind::Club, 11, "Barcelona SC"),
                entry(EntityKind::Nation, 20, "Côte d'Ivoire"),
                entry(EntityKind::Staff, 30, "Igor Tolić"),
            ],
            0,
            5,
        )
    }

    #[test]
    fn staff_are_also_found_by_club_but_players_are_not() {
        let with_club = |kind, id, name: &str, club: &str| {
            IndexEntry::new(kind, id, 1000 + id as u32, name.to_string(), Some(club.to_string()), 0x1000 * id as usize)
        };
        let index = SearchIndex::new(
            42,
            vec![
                with_club(EntityKind::Staff, 1, "Antonio Pintus", "R. Madrid"),
                with_club(EntityKind::Staff, 2, "Iván Ortega", "R. Madrid"),
                with_club(EntityKind::Staff, 3, "Madrid Smith", "Barcelona"),
                with_club(EntityKind::Player, 4, "Kylian Mbappé", "R. Madrid"),
            ],
            0,
            5,
        );
        let ids = |q: &str, kind| index.query(q, Some(kind), 10, 0)[&kind].hits.iter().map(|h| h.id).collect::<Vec<_>>();
        assert_eq!(ids("madrid", EntityKind::Staff), vec![3, 1, 2], "name match ranks first, then club matches");
        assert_eq!(ids("ortega madrid", EntityKind::Staff), vec![2], "every token must match name or club");
        assert!(ids("madrid", EntityKind::Player).is_empty(), "player search stays name-only");
    }

    #[test]
    fn folds_case_and_diacritics() {
        assert_eq!(fold("Kylian MBAPPÉ"), "kylian mbappe");
        assert_eq!(fold("Sørloth Ødegaard"), "sorloth odegaard");
        assert_eq!(fold("Müßig Łukasz Çalhanoğlu"), "mussig lukasz calhanoglu");
        assert_eq!(fold("Côte d'Ivoire"), "cote d'ivoire");
    }

    #[test]
    fn accent_free_query_finds_accented_names() {
        let index = sample();
        let pages = index.query("sorloth", Some(EntityKind::Player), 25, 0);
        assert_eq!(pages[&EntityKind::Player].hits[0].id, 3);
        let pages = index.query("tolic", None, 25, 0);
        assert_eq!(pages[&EntityKind::Staff].hits[0].name, "Igor Tolić");
        assert_eq!(index.query("cote", None, 25, 0)[&EntityKind::Nation].total, 1);
    }

    #[test]
    fn every_token_must_match() {
        let index = sample();
        let pages = index.query("kylian mbappe", Some(EntityKind::Player), 25, 0);
        assert_eq!(pages[&EntityKind::Player].total, 1);
        assert_eq!(index.query("kylian yamal", Some(EntityKind::Player), 25, 0)[&EntityKind::Player].total, 0);
    }

    #[test]
    fn ranks_exact_then_prefix_then_word_prefix_then_substring() {
        let index = sample();
        let names: Vec<String> = index.query("mbappe", Some(EntityKind::Player), 25, 0)[&EntityKind::Player]
            .hits
            .iter()
            .map(|e| e.name.clone())
            .collect();
        // Name prefix first, then word prefixes in name order.
        assert_eq!(names, ["Mbappe Junior", "Ethan Mbappé", "Kylian Mbappé"]);
        let clubs = index.query("barcelona", Some(EntityKind::Club), 25, 0);
        assert_eq!(clubs[&EntityKind::Club].hits[0].name, "Barcelona SC");
        let exact = index.query("fc barcelona", Some(EntityKind::Club), 25, 0);
        assert_eq!(exact[&EntityKind::Club].hits[0].id, 10);
        let substring = index.query("appe", Some(EntityKind::Player), 25, 0);
        assert_eq!(substring[&EntityKind::Player].total, 3);
    }

    #[test]
    fn numeric_query_matches_id_or_uid() {
        let index = sample();
        assert_eq!(index.query("2", Some(EntityKind::Player), 25, 0)[&EntityKind::Player].hits[0].name, "Lamine Yamal");
        assert_eq!(index.query("1003", Some(EntityKind::Player), 25, 0)[&EntityKind::Player].hits[0].id, 3);
    }

    #[test]
    fn pages_with_total_and_clamped_limit() {
        let index = sample();
        let first = index.query("mbappe", Some(EntityKind::Player), 2, 0);
        assert_eq!(first[&EntityKind::Player].total, 3);
        assert_eq!(first[&EntityKind::Player].hits.len(), 2);
        let second = index.query("mbappe", Some(EntityKind::Player), 2, 2);
        assert_eq!(second[&EntityKind::Player].hits.len(), 1);
        assert_eq!(second[&EntityKind::Player].hits[0].name, "Kylian Mbappé");
        let zero = index.query("mbappe", Some(EntityKind::Player), 0, 0);
        assert_eq!(zero[&EntityKind::Player].hits.len(), 1);
        let all = index.query("a", None, 25, 0);
        assert_eq!(all.len(), EntityKind::ALL.len());
        assert!(index.query("   ", None, 25, 0).values().all(|page| page.total == 0));
    }

    #[test]
    fn stale_entries_are_dropped() {
        let mut index = sample();
        assert!(index.remove(EntityKind::Player, 2));
        assert!(!index.remove(EntityKind::Player, 2));
        assert_eq!(index.query("yamal", None, 25, 0)[&EntityKind::Player].total, 0);
        assert_eq!(index.counts()["players"], 4);
    }

    #[test]
    fn duplicate_ids_are_dropped_not_guessed() {
        let (unique, dropped) = drop_duplicate_ids(vec![
            entry(EntityKind::Player, 1, "A"),
            entry(EntityKind::Player, 1, "A copy"),
            entry(EntityKind::Club, 1, "Club one"),
            entry(EntityKind::Player, 2, "B"),
        ]);
        assert_eq!(dropped, 2);
        assert_eq!(unique.iter().map(|e| (e.kind, e.id)).collect::<Vec<_>>(), [(EntityKind::Club, 1), (EntityKind::Player, 2)]);
    }

    #[test]
    fn kind_names_round_trip() {
        for kind in EntityKind::ALL {
            assert_eq!(EntityKind::from_plural(kind.plural()), Some(kind));
        }
        assert_eq!(EntityKind::from_plural("all"), None);
    }

    fn scouted() -> SearchIndex {
        let profile = |ca, pa, st, gk| Some(PlayerProfile { ca, pa, positions: { let mut p = [1u8; 15]; p[12] = st; p[0] = gk; p } });
        SearchIndex::new(
            1,
            vec![
                IndexEntry::new(EntityKind::Player, 1, 11, "Alpha Striker".into(), Some("Real Madrid".into()), 0).with_profile(profile(Some(150), Some(170), 20, 1)),
                IndexEntry::new(EntityKind::Player, 2, 12, "Bravo Striker".into(), Some("Getafe".into()), 0).with_profile(profile(Some(90), Some(140), 16, 1)),
                IndexEntry::new(EntityKind::Player, 3, 13, "Charlie Keeper".into(), Some("Real Madrid".into()), 0).with_profile(profile(Some(120), None, 1, 19)),
                IndexEntry::new(EntityKind::Player, 4, 14, "Delta Unknown".into(), None, 0),
                IndexEntry::new(EntityKind::Staff, 30, 31, "Coach Real".into(), Some("Real Madrid".into()), 0),
            ],
            0,
            1,
        )
    }

    #[test]
    fn scout_filters_by_ability_position_and_club() {
        let index = scouted();
        let names = |filter: &ScoutFilter| index.scout(filter, 50, 0).0.into_iter().map(|e| e.name).collect::<Vec<_>>();
        assert_eq!(names(&ScoutFilter::default()).len(), 4, "no filter lists every player");
        assert_eq!(names(&ScoutFilter { ca_min: Some(100), ..Default::default() }), ["Alpha Striker", "Charlie Keeper"]);
        assert_eq!(names(&ScoutFilter { pa_max: Some(150), ..Default::default() }), ["Bravo Striker"], "unknown PA never matches a range");
        assert_eq!(names(&ScoutFilter { position: Some((12, 15)), ..Default::default() }), ["Alpha Striker", "Bravo Striker"]);
        assert_eq!(names(&ScoutFilter { position: Some((12, 18)), ..Default::default() }), ["Alpha Striker"]);
        assert_eq!(names(&ScoutFilter { club: "real".into(), ..Default::default() }), ["Alpha Striker", "Charlie Keeper"]);
        assert_eq!(names(&ScoutFilter { text: "striker".into(), club: "getafe".into(), ..Default::default() }), ["Bravo Striker"]);
    }

    #[test]
    fn scout_sorts_pages_and_handles_staff() {
        let index = scouted();
        let sorted = ScoutFilter { ca_min: Some(1), sort: ScoutSort::Ca, ..Default::default() };
        let (page, total) = index.scout(&sorted, 2, 0);
        assert_eq!((total, page.iter().map(|e| e.id).collect::<Vec<_>>()), (3, vec![1, 3]));
        assert_eq!(index.scout(&sorted, 2, 2).0.iter().map(|e| e.id).collect::<Vec<_>>(), [2]);
        let staff = ScoutFilter { kind: Some(EntityKind::Staff), club: "real".into(), ..Default::default() };
        assert_eq!(index.scout(&staff, 50, 0).1, 1);
        let by_ca = ScoutFilter { kind: Some(EntityKind::Staff), ca_min: Some(1), ..Default::default() };
        assert_eq!(index.scout(&by_ca, 50, 0).1, 0, "staff have no ability profile");
    }
}
