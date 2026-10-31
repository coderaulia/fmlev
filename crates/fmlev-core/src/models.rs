use serde::{Deserialize, Serialize};
use fmlev_mem::LinuxMemoryDriver;
use fmlev_il2cpp::OffsetsRegistry;
use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayerContract {
    #[serde(default)]
    pub wage_per_week: i32,
    #[serde(default)]
    pub expiry_year: i32,
    #[serde(default)]
    pub release_clause: i64,
    #[serde(default)]
    pub squad_status: String, // "Star Player", "Important Player", "Regular Starter", "Squad Player", "Breakthrough Prospect"
    #[serde(default)]
    pub happiness: String,    // "Delighted", "Happy", "Content"
    /// Live only: the native squad status code (`+0x54`), `None` when unreadable or not a native read.
    #[serde(default)]
    pub squad_status_code: Option<u8>,
    /// Live only: `Some(true)` when the contract has a native release clause entry, `Some(false)` when it has
    /// none, `None` when unreadable or not a native read.
    #[serde(default)]
    pub release_clause_present: Option<bool>,
    /// Live only: why squad status or the release clause is unavailable (empty when both were read).
    #[serde(default)]
    pub terms_note: String,
}

impl Default for PlayerContract {
    fn default() -> Self {
        Self {
            wage_per_week: 50_000,
            expiry_year: 2028,
            release_clause: 0,
            squad_status: "Regular Starter".to_string(),
            happiness: "Content".to_string(),
            squad_status_code: None,
            release_clause_present: None,
            terms_note: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayerPositions {
    #[serde(default = "default_pos_one")] pub gk: u8,
    #[serde(default = "default_pos_one")] pub dl: u8,
    #[serde(default = "default_pos_one")] pub dc: u8,
    #[serde(default = "default_pos_one")] pub dr: u8,
    #[serde(default = "default_pos_one")] pub wbl: u8,
    #[serde(default = "default_pos_one")] pub wbr: u8,
    #[serde(default = "default_pos_one")] pub dm: u8,
    #[serde(default = "default_pos_one")] pub ml: u8,
    #[serde(default = "default_pos_one")] pub mc: u8,
    #[serde(default = "default_pos_one")] pub mr: u8,
    #[serde(default = "default_pos_one")] pub aml: u8,
    #[serde(default = "default_pos_one")] pub amc: u8,
    #[serde(default = "default_pos_one")] pub amr: u8,
    #[serde(default = "default_pos_one")] pub st: u8,
    #[serde(default = "default_foot_weak")] pub left_foot: u8,
    #[serde(default = "default_foot_strong")] pub right_foot: u8,
}

fn default_pos_one() -> u8 { 1 }
fn default_foot_weak() -> u8 { 10 }
fn default_foot_strong() -> u8 { 20 }

impl Default for PlayerPositions {
    fn default() -> Self {
        Self {
            gk: 1,
            dl: 1,
            dc: 1,
            dr: 1,
            wbl: 1,
            wbr: 1,
            dm: 1,
            ml: 1,
            mc: 20,
            mr: 1,
            aml: 1,
            amc: 1,
            amr: 1,
            st: 1,
            left_foot: 10,
            right_foot: 20,
        }
    }
}

impl PlayerPositions {
    /// Zero marks a live rating that has not been read from FM26.
    pub fn unknown() -> Self {
        Self {
            gk: 0, dl: 0, dc: 0, dr: 0, wbl: 0, wbr: 0, dm: 0,
            ml: 0, mc: 0, mr: 0, aml: 0, amc: 0, amr: 0, st: 0,
            left_foot: 0, right_foot: 0,
        }
    }

    /// Parses an FM position string like `"AM (R), ST (C)"` or `"D/WB (L)"` or `"DM, M (C)"` into 1-20 ratings.
    pub fn from_position_string(pos_str: &str) -> Self {
        let mut p = Self {
            mc: 1,
            ..Default::default()
        };
        let u = pos_str.to_uppercase();
        if u.contains("GK") {
            p.gk = 20;
            return p;
        }
        for part in u.split(',') {
            let s = part.trim();
            let has_l = s.contains("(L") || s.contains("L)") || s.contains("LC") || s.contains("RL")
                || s.ends_with("L") || s.contains(" L") || s.contains("/L");
            let has_r = s.contains("(R") || s.contains("R)") || s.contains("RC") || s.contains("RL")
                || s.ends_with("R") || s.contains(" R") || s.contains("/R");
            let has_c = s.contains("(C") || s.contains("C)") || s.ends_with("C") || s.contains(" C") || s.contains("/C")
                || (!has_l && !has_r);

            if s.starts_with("D/WB") || s.contains("D/WB") {
                if has_l { p.dl = 20; p.wbl = 20; p.ml = 12; }
                if has_r { p.dr = 20; p.wbr = 20; p.mr = 12; }
                if has_c { p.dc = 20; }
            } else if s.starts_with("D") && !s.starts_with("DM") {
                if has_l { p.dl = 20; p.wbl = 14; }
                if has_r { p.dr = 20; p.wbr = 14; }
                if has_c { p.dc = 20; }
            }
            if s.contains("WB") && !s.contains("D/WB") {
                if has_l { p.wbl = 20; p.dl = p.dl.max(15); }
                if has_r { p.wbr = 20; p.dr = p.dr.max(15); }
            }
            if s.contains("DM") {
                p.dm = 20;
                p.mc = p.mc.max(15);
            }
            if s.starts_with("M/AM") || s.contains("M/AM") {
                if has_l { p.ml = 20; p.aml = 20; }
                if has_r { p.mr = 20; p.amr = 20; }
                if has_c { p.mc = 20; p.amc = 20; }
            } else if (s.starts_with("M ") || s == "M" || s.starts_with("M(") || s.contains(" M (") || s == "MC" || s == "ML" || s == "MR")
                && !s.contains("AM") && !s.contains("DM")
            {
                if has_l { p.ml = 20; p.aml = p.aml.max(14); }
                if has_r { p.mr = 20; p.amr = p.amr.max(14); }
                if has_c { p.mc = 20; p.dm = p.dm.max(14); }
            }
            if s.contains("AM") && !s.contains("M/AM") {
                if has_l { p.aml = 20; p.ml = p.ml.max(14); }
                if has_r { p.amr = 20; p.mr = p.mr.max(14); }
                if has_c { p.amc = 20; p.mc = p.mc.max(14); }
            }
            if s.contains("ST") || s.contains("S (C)") {
                p.st = 20;
            }
        }
        if p.gk == 1 && p.dl == 1 && p.dc == 1 && p.dr == 1 && p.wbl == 1 && p.wbr == 1
            && p.dm == 1 && p.ml == 1 && p.mc == 1 && p.mr == 1 && p.aml == 1 && p.amc == 1 && p.amr == 1 && p.st == 1
        {
            p.mc = 20;
        }
        p
    }

    /// Formats the natural (>= 15) positions into a compact FM position summary string.
    pub fn to_position_string(&self) -> String {
        if self.gk >= 15 {
            return "GK".to_string();
        }
        let mut tags = Vec::new();
        let d_sides = Self::format_sides(self.dr >= 15, self.dl >= 15, self.dc >= 15);
        if !d_sides.is_empty() {
            tags.push(format!("D ({})", d_sides));
        }
        let wb_sides = Self::format_sides(self.wbr >= 15, self.wbl >= 15, false);
        if !wb_sides.is_empty() {
            tags.push(format!("WB ({})", wb_sides));
        }
        if self.dm >= 15 {
            tags.push("DM".to_string());
        }
        let m_sides = Self::format_sides(self.mr >= 15, self.ml >= 15, self.mc >= 15);
        if !m_sides.is_empty() {
            tags.push(format!("M ({})", m_sides));
        }
        let am_sides = Self::format_sides(self.amr >= 15, self.aml >= 15, self.amc >= 15);
        if !am_sides.is_empty() {
            tags.push(format!("AM ({})", am_sides));
        }
        if self.st >= 15 {
            tags.push("ST (C)".to_string());
        }
        if tags.is_empty() {
            "M (C)".to_string()
        } else {
            tags.join(", ")
        }
    }

    fn format_sides(r: bool, l: bool, c: bool) -> String {
        let mut s = String::new();
        if r { s.push('R'); }
        if l { s.push('L'); }
        if c { s.push('C'); }
        s
    }

    /// Returns the FM internal PositionMask bitfield matching `PlayingDataFlags.PositionMask`.
    pub fn to_position_mask(&self) -> u32 {
        let mut mask: u32 = 0;
        if self.gk >= 15 { mask |= 1; }
        if self.dr >= 15 { mask |= 4; }
        if self.dl >= 15 { mask |= 8; }
        if self.dc >= 15 { mask |= 16; }
        if self.wbr >= 15 { mask |= 32; }
        if self.wbl >= 15 { mask |= 64; }
        if self.dm >= 15 { mask |= 128; }
        if self.mr >= 15 { mask |= 256; }
        if self.ml >= 15 { mask |= 512; }
        if self.mc >= 15 { mask |= 1024; }
        if self.amr >= 15 { mask |= 2048; }
        if self.aml >= 15 { mask |= 4096; }
        if self.amc >= 15 { mask |= 8192; }
        if self.st >= 15 { mask |= 16384; }
        mask
    }

    /// Creates positions from an FM internal PositionMask bitfield.
    pub fn from_position_mask(mask: u32) -> Self {
        let mut p = Self {
            mc: 1,
            ..Default::default()
        };
        if mask & 1 != 0 { p.gk = 20; }
        if mask & 4 != 0 { p.dr = 20; }
        if mask & 8 != 0 { p.dl = 20; }
        if mask & 16 != 0 { p.dc = 20; }
        if mask & 32 != 0 { p.wbr = 20; }
        if mask & 64 != 0 { p.wbl = 20; }
        if mask & 128 != 0 { p.dm = 20; }
        if mask & 256 != 0 { p.mr = 20; }
        if mask & 512 != 0 { p.ml = 20; }
        if mask & 1024 != 0 { p.mc = 20; }
        if mask & 2048 != 0 { p.amr = 20; }
        if mask & 4096 != 0 { p.aml = 20; }
        if mask & 8192 != 0 { p.amc = 20; }
        if mask & 16384 != 0 { p.st = 20; }
        p
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Player {
    pub id: i32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub club_id: i32,
    #[serde(default)]
    pub current_ability: u8,   // 1 - 200
    #[serde(default)]
    pub potential_ability: u8, // 1 - 200
    #[serde(default)]
    pub condition: u16,        // 0 - 10000 (100.00%)
    #[serde(default)]
    pub sharpness: u16,        // 0 - 10000 (100.00%)
    #[serde(default)]
    pub morale: u8,            // 0 - 100
    #[serde(default)]
    pub attributes: PlayerAttributes,

    #[serde(default)]
    pub age: u8,
    #[serde(default)]
    pub nationality: String,
    #[serde(default)]
    pub position: String,      // e.g. "ST (C), AM (RL)"
    #[serde(default)]
    pub positions: PlayerPositions,
    #[serde(default)]
    pub club_name: String,
    #[serde(default)]
    pub morale_label: String,  // e.g. "Superb"
    #[serde(default)]
    pub is_injured: bool,
    #[serde(default)]
    pub is_suspended: bool,
    #[serde(default)]
    pub contract: PlayerContract,
    #[serde(default)]
    pub user_modified: bool,
    /// Read-only PPM bitset (bit table incomplete). `None` when not natively readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_moves: Option<crate::preferred_moves::PreferredMoves>,
}

impl Player {
    /// Reads a Player struct from memory at `addr` using configured class offsets.
    pub fn read(driver: &LinuxMemoryDriver, addr: usize, registry: &OffsetsRegistry) -> Result<Self> {
        let id_offset = registry
            .get_field_offset("PersonReference", "m_idHash")
            .unwrap_or(0x18);
        let id: i32 = driver.read(addr + id_offset).unwrap_or(0);

        let ca_offset = registry
            .get_field_offset("PlayerAttributes", "current_ability")
            .unwrap_or(0x18);
        let pa_offset = registry
            .get_field_offset("PlayerAttributes", "potential_ability")
            .unwrap_or(0x19);

        let current_ability: u8 = driver.read(addr + ca_offset).unwrap_or(100);
        let potential_ability: u8 = driver.read(addr + pa_offset).unwrap_or(150);

        let condition: u16 = driver.read(addr + 0x10).unwrap_or(10000);
        let sharpness: u16 = driver.read(addr + 0x12).unwrap_or(10000);
        let morale: u8 = driver.read(addr + 0x14).unwrap_or(100);

        let attributes = PlayerAttributes::read(driver, addr, registry)?;

        let morale_label = if morale >= 90 {
            "Superb".to_string()
        } else if morale >= 75 {
            "Very Good".to_string()
        } else if morale >= 50 {
            "Good".to_string()
        } else {
            "Okay".to_string()
        };

        Ok(Self {
            id,
            name: format!("Player_{}", id),
            club_id: 0,
            current_ability,
            potential_ability,
            condition,
            sharpness,
            morale,
            attributes,
            age: 24,
            nationality: "Unknown".to_string(),
            position: "MC".to_string(),
            positions: PlayerPositions::from_position_string("MC"),
            club_name: "Unknown Club".to_string(),
            morale_label,
            is_injured: condition < 5000,
            is_suspended: false,
            contract: PlayerContract::default(),
            user_modified: false,
            preferred_moves: None,
        })
    }

    /// Writes Player attributes and core status values to memory at `addr`.
    pub fn write(&self, driver: &LinuxMemoryDriver, addr: usize, registry: &OffsetsRegistry) -> Result<()> {
        if let Ok(ca_offset) = registry.get_field_offset("PlayerAttributes", "current_ability") {
            driver.write(addr + ca_offset, &self.current_ability)?;
        }
        if let Ok(pa_offset) = registry.get_field_offset("PlayerAttributes", "potential_ability") {
            driver.write(addr + pa_offset, &self.potential_ability)?;
        }
        driver.write(addr + 0x10, &self.condition)?;
        driver.write(addr + 0x12, &self.sharpness)?;
        driver.write(addr + 0x14, &self.morale)?;

        self.attributes.write(driver, addr, registry)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PlayerAttributes {
    #[serde(default)] pub marking: u8,
    #[serde(default)] pub long_throws: u8,
    #[serde(default)] pub punching: u8,
    #[serde(default)] pub dirtiness: u8,
    #[serde(default)] pub controversy: u8,

    // Technical
    #[serde(default)] pub corners: u8,
    #[serde(default)] pub crossing: u8,
    #[serde(default)] pub dribbling: u8,
    #[serde(default)] pub finishing: u8,
    #[serde(default)] pub first_touch: u8,
    #[serde(default)] pub free_kicks: u8,
    #[serde(default)] pub heading: u8,
    #[serde(default)] pub long_shots: u8,
    #[serde(default)] pub passing: u8,
    #[serde(default)] pub penalty_taking: u8,
    #[serde(default)] pub tackling: u8,
    #[serde(default)] pub technique: u8,

    // Mental
    #[serde(default)] pub aggression: u8,
    #[serde(default)] pub anticipation: u8,
    #[serde(default)] pub bravery: u8,
    #[serde(default)] pub composure: u8,
    #[serde(default)] pub concentration: u8,
    #[serde(default)] pub decisions: u8,
    #[serde(default)] pub determination: u8,
    #[serde(default)] pub flair: u8,
    #[serde(default)] pub leadership: u8,
    #[serde(default)] pub off_the_ball: u8,
    #[serde(default)] pub positioning: u8,
    #[serde(default)] pub teamwork: u8,
    #[serde(default)] pub vision: u8,
    #[serde(default)] pub work_rate: u8,

    // Physical
    #[serde(default)] pub acceleration: u8,
    #[serde(default)] pub agility: u8,
    #[serde(default)] pub balance: u8,
    #[serde(default)] pub jumping_reach: u8,
    #[serde(default)] pub natural_fitness: u8,
    #[serde(default)] pub pace: u8,
    #[serde(default)] pub stamina: u8,
    #[serde(default)] pub strength: u8,

    // Hidden Attributes
    #[serde(default)] pub consistency: u8,
    #[serde(default)] pub important_matches: u8,
    #[serde(default)] pub injury_proneness: u8,
    #[serde(default)] pub versatility: u8,
    #[serde(default)] pub adaptability: u8,
    #[serde(default)] pub ambition: u8,
    #[serde(default)] pub loyalty: u8,
    #[serde(default)] pub pressure: u8,
    #[serde(default)] pub professionalism: u8,
    #[serde(default)] pub sportsmanship: u8,
    #[serde(default)] pub temperament: u8,

    // Goalkeeping Attributes
    #[serde(default)] pub aerial_reach: u8,
    #[serde(default)] pub command_of_area: u8,
    #[serde(default)] pub communication: u8,
    #[serde(default)] pub eccentricity: u8,
    #[serde(default)] pub handling: u8,
    #[serde(default)] pub kicking: u8,
    #[serde(default)] pub one_on_ones: u8,
    #[serde(default)] pub reflexes: u8,
    #[serde(default)] pub rushing_out: u8,
    #[serde(default)] pub throwing: u8,
}

impl PlayerAttributes {
    pub fn read(driver: &LinuxMemoryDriver, addr: usize, registry: &OffsetsRegistry) -> Result<Self> {
        let finishing_off = registry
            .get_field_offset("PlayerAttributes", "finishing")
            .unwrap_or(0x20);
        let dribbling_off = registry
            .get_field_offset("PlayerAttributes", "dribbling")
            .unwrap_or(0x21);
        let passing_off = registry
            .get_field_offset("PlayerAttributes", "passing")
            .unwrap_or(0x22);
        let tackling_off = registry
            .get_field_offset("PlayerAttributes", "tackling")
            .unwrap_or(0x23);
        let pace_off = registry
            .get_field_offset("PlayerAttributes", "pace")
            .unwrap_or(0x30);
        let acceleration_off = registry
            .get_field_offset("PlayerAttributes", "acceleration")
            .unwrap_or(0x31);
        let stamina_off = registry
            .get_field_offset("PlayerAttributes", "stamina")
            .unwrap_or(0x32);

        Ok(Self {
            finishing: driver.read(addr + finishing_off).unwrap_or(10),
            dribbling: driver.read(addr + dribbling_off).unwrap_or(10),
            passing: driver.read(addr + passing_off).unwrap_or(10),
            tackling: driver.read(addr + tackling_off).unwrap_or(10),
            pace: driver.read(addr + pace_off).unwrap_or(10),
            acceleration: driver.read(addr + acceleration_off).unwrap_or(10),
            stamina: driver.read(addr + stamina_off).unwrap_or(10),
            ..Default::default()
        })
    }

    pub fn write(&self, driver: &LinuxMemoryDriver, addr: usize, registry: &OffsetsRegistry) -> Result<()> {
        let fields = [
            ("finishing", self.finishing),
            ("dribbling", self.dribbling),
            ("passing", self.passing),
            ("tackling", self.tackling),
            ("pace", self.pace),
            ("acceleration", self.acceleration),
            ("stamina", self.stamina),
            ("corners", self.corners),
            ("crossing", self.crossing),
            ("first_touch", self.first_touch),
            ("free_kicks", self.free_kicks),
            ("heading", self.heading),
            ("long_shots", self.long_shots),
            ("penalty_taking", self.penalty_taking),
            ("technique", self.technique),
            ("aggression", self.aggression),
            ("anticipation", self.anticipation),
            ("bravery", self.bravery),
            ("composure", self.composure),
            ("concentration", self.concentration),
            ("decisions", self.decisions),
            ("determination", self.determination),
            ("flair", self.flair),
            ("leadership", self.leadership),
            ("off_the_ball", self.off_the_ball),
            ("positioning", self.positioning),
            ("teamwork", self.teamwork),
            ("vision", self.vision),
            ("work_rate", self.work_rate),
            ("agility", self.agility),
            ("balance", self.balance),
            ("jumping_reach", self.jumping_reach),
            ("natural_fitness", self.natural_fitness),
            ("strength", self.strength),
            ("consistency", self.consistency),
            ("important_matches", self.important_matches),
            ("injury_proneness", self.injury_proneness),
            ("versatility", self.versatility),
            ("adaptability", self.adaptability),
            ("ambition", self.ambition),
            ("loyalty", self.loyalty),
            ("pressure", self.pressure),
            ("professionalism", self.professionalism),
            ("sportsmanship", self.sportsmanship),
            ("temperament", self.temperament),
            ("aerial_reach", self.aerial_reach),
            ("command_of_area", self.command_of_area),
            ("communication", self.communication),
            ("eccentricity", self.eccentricity),
            ("handling", self.handling),
            ("kicking", self.kicking),
            ("one_on_ones", self.one_on_ones),
            ("reflexes", self.reflexes),
            ("rushing_out", self.rushing_out),
            ("throwing", self.throwing),
        ];

        for (field_name, val) in fields {
            if let Ok(off) = registry.get_field_offset("PlayerAttributes", field_name) {
                driver.write(addr + off, &val)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClubAffiliate {
    pub affiliate_club_id: i32,
    pub affiliate_name: String,
    #[serde(default)]
    pub is_parent: bool,
    #[serde(default)]
    pub first_option_to_buy: bool,
    #[serde(default)]
    pub send_players_on_loan: bool,
    #[serde(default)]
    pub shared_scouting: bool,
    #[serde(default)]
    pub annual_fee: i64,
    #[serde(default)]
    pub work_permit_haven: bool,
}

impl Default for ClubAffiliate {
    fn default() -> Self {
        Self {
            affiliate_club_id: 0,
            affiliate_name: String::new(),
            is_parent: false,
            first_option_to_buy: true,
            send_players_on_loan: true,
            shared_scouting: true,
            annual_fee: 100_000,
            work_permit_haven: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ClubFinances {
    pub club_id: i32,
    pub club_name: String,
    pub balance: i64,
    pub transfer_budget: i64,
    pub wage_budget: i32,

    #[serde(default)]
    pub league_name: String,
    #[serde(default)]
    pub reputation: u16,           // 1 - 10000
    #[serde(default)]
    pub wage_used: i32,
    #[serde(default)]
    pub training_facilities: u8,   // 1 - 20
    #[serde(default)]
    pub youth_facilities: u8,      // 1 - 20
    #[serde(default)]
    pub youth_recruitment: u8,     // 1 - 20
    #[serde(default)]
    pub junior_coaching: u8,       // 1 - 20

    // Advanced facilities & stadium
    #[serde(default = "default_stat_ten")]
    pub data_analysis_facilities: u8, // 1 - 20
    #[serde(default = "default_stat_ten")]
    pub corporate_facilities: u8,     // 1 - 20
    #[serde(default = "default_stadium_name")]
    pub stadium_name: String,
    #[serde(default = "default_stadium_cap")]
    pub stadium_capacity: u32,
    #[serde(default = "default_stadium_exp")]
    pub stadium_expansion_cap: u32,

    // Board & ownership
    #[serde(default = "default_sugar_daddy")]
    pub sugar_daddy_type: String, // "None", "FrontEnd", "Underwriter", "Foreground"
    #[serde(default = "default_stat_ten")]
    pub board_patience: u8,       // 1 - 20
    #[serde(default = "default_confidence")]
    pub fan_confidence: u8,       // 1 - 100
    #[serde(default = "default_confidence")]
    pub board_confidence: u8,     // 1 - 100
    #[serde(default = "default_retention")]
    pub transfer_retention_pct: u8, // 0 - 100%
    #[serde(default = "default_max_wage")]
    pub max_wage_allowance: i32,

    // Affiliates
    #[serde(default)]
    pub affiliates: Vec<ClubAffiliate>,
}

fn default_stat_ten() -> u8 { 10 }
fn default_stat_one() -> u8 { 1 }
fn default_stat_fifteen() -> u8 { 15 }
fn default_stadium_name() -> String { "Club Stadium".to_string() }
fn default_stadium_cap() -> u32 { 45_000 }
fn default_stadium_exp() -> u32 { 60_000 }
fn default_sugar_daddy() -> String { "None".to_string() }
fn default_confidence() -> u8 { 75 }
fn default_retention() -> u8 { 100 }
fn default_max_wage() -> i32 { 350_000 }
fn default_reputation() -> u16 { 7500 }
fn default_youth_rating() -> u8 { 110 }
fn default_tax_rate() -> u8 { 25 }
fn default_comp_reputation() -> u16 { 8000 }
fn default_subs() -> u8 { 5 }
fn default_bench() -> u8 { 9 }
fn default_windows() -> u8 { 3 }
fn default_true() -> bool { true }
fn default_squad_size() -> u8 { 25 }
fn default_foreign() -> u8 { 17 }
fn default_hg_club() -> u8 { 4 }
fn default_hg_nation() -> u8 { 8 }
fn default_yellow_ban() -> u8 { 5 }
fn default_population() -> u32 { 50_000_000 }

impl ClubFinances {
    pub fn read(driver: &LinuxMemoryDriver, addr: usize, registry: &OffsetsRegistry) -> Result<Self> {
        let balance_off = registry
            .get_field_offset("ClubReference", "balance")
            .unwrap_or(0x30);
        let transfer_off = registry
            .get_field_offset("ClubReference", "transfer_budget")
            .unwrap_or(0x38);
        let wage_off = registry
            .get_field_offset("ClubReference", "wage_budget")
            .unwrap_or(0x40);

        let balance: i64 = driver.read(addr + balance_off).unwrap_or(0);
        let transfer_budget: i64 = driver.read(addr + transfer_off).unwrap_or(0);
        let wage_budget: i32 = driver.read(addr + wage_off).unwrap_or(0);

        let id_offset = registry
            .get_field_offset("ClubReference", "m_idHash")
            .unwrap_or(0x18);
        let club_id: i32 = driver.read(addr + id_offset).unwrap_or(0);

        let training_facilities: u8 = registry
            .get_field_offset("ClubReference", "training_facilities")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(10);
        let youth_facilities: u8 = registry
            .get_field_offset("ClubReference", "youth_facilities")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(10);
        let youth_recruitment: u8 = registry
            .get_field_offset("ClubReference", "youth_recruitment")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(10);
        let junior_coaching: u8 = registry
            .get_field_offset("ClubReference", "junior_coaching")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(10);
        let data_analysis_facilities: u8 = registry
            .get_field_offset("ClubReference", "data_analysis")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(10);
        let corporate_facilities: u8 = registry
            .get_field_offset("ClubReference", "corporate_facilities")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(10);

        let sugar_daddy_raw: u8 = registry
            .get_field_offset("ClubReference", "sugar_daddy_type")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(0);
        let sugar_daddy_type = match sugar_daddy_raw {
            1 => "FrontEnd".to_string(),
            2 => "Underwriter".to_string(),
            3 => "Foreground".to_string(),
            _ => "None".to_string(),
        };

        let board_patience: u8 = registry
            .get_field_offset("ClubReference", "board_patience")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(10);
        let fan_confidence: u8 = registry
            .get_field_offset("ClubReference", "fan_confidence")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(75);
        let board_confidence: u8 = registry
            .get_field_offset("ClubReference", "board_confidence")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(75);
        let transfer_retention_pct: u8 = registry
            .get_field_offset("ClubReference", "transfer_retention_pct")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(100);
        let max_wage_allowance: i32 = registry
            .get_field_offset("ClubReference", "max_wage_allowance")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(350_000);
        let stadium_capacity: u32 = registry
            .get_field_offset("ClubReference", "stadium_capacity")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(45_000);
        let stadium_expansion_cap: u32 = registry
            .get_field_offset("ClubReference", "stadium_expansion_cap")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(60_000);

        Ok(Self {
            club_id,
            club_name: String::new(),
            balance,
            transfer_budget,
            wage_budget,
            league_name: String::new(),
            reputation: 5000,
            wage_used: 0,
            training_facilities,
            youth_facilities,
            youth_recruitment,
            junior_coaching,
            data_analysis_facilities,
            corporate_facilities,
            stadium_name: "Club Stadium".to_string(),
            stadium_capacity,
            stadium_expansion_cap,
            sugar_daddy_type,
            board_patience,
            fan_confidence,
            board_confidence,
            transfer_retention_pct,
            max_wage_allowance,
            affiliates: Vec::new(),
        })
    }

    pub fn write(&self, driver: &LinuxMemoryDriver, addr: usize, registry: &OffsetsRegistry) -> Result<()> {
        let balance_off = registry
            .get_field_offset("ClubReference", "balance")
            .unwrap_or(0x30);
        let transfer_off = registry
            .get_field_offset("ClubReference", "transfer_budget")
            .unwrap_or(0x38);
        let wage_off = registry
            .get_field_offset("ClubReference", "wage_budget")
            .unwrap_or(0x40);

        driver.write(addr + balance_off, &self.balance)?;
        driver.write(addr + transfer_off, &self.transfer_budget)?;
        driver.write(addr + wage_off, &self.wage_budget)?;

        if let Ok(off) = registry.get_field_offset("ClubReference", "training_facilities") {
            driver.write(addr + off, &self.training_facilities)?;
        }
        if let Ok(off) = registry.get_field_offset("ClubReference", "youth_facilities") {
            driver.write(addr + off, &self.youth_facilities)?;
        }
        if let Ok(off) = registry.get_field_offset("ClubReference", "youth_recruitment") {
            driver.write(addr + off, &self.youth_recruitment)?;
        }
        if let Ok(off) = registry.get_field_offset("ClubReference", "junior_coaching") {
            driver.write(addr + off, &self.junior_coaching)?;
        }
        if let Ok(off) = registry.get_field_offset("ClubReference", "data_analysis") {
            driver.write(addr + off, &self.data_analysis_facilities)?;
        }
        if let Ok(off) = registry.get_field_offset("ClubReference", "corporate_facilities") {
            driver.write(addr + off, &self.corporate_facilities)?;
        }

        let sd_raw: u8 = match self.sugar_daddy_type.as_str() {
            "FrontEnd" => 1,
            "Underwriter" => 2,
            "Foreground" => 3,
            _ => 0,
        };
        if let Ok(off) = registry.get_field_offset("ClubReference", "sugar_daddy_type") {
            driver.write(addr + off, &sd_raw)?;
        }
        if let Ok(off) = registry.get_field_offset("ClubReference", "board_patience") {
            driver.write(addr + off, &self.board_patience)?;
        }
        if let Ok(off) = registry.get_field_offset("ClubReference", "fan_confidence") {
            driver.write(addr + off, &self.fan_confidence)?;
        }
        if let Ok(off) = registry.get_field_offset("ClubReference", "board_confidence") {
            driver.write(addr + off, &self.board_confidence)?;
        }
        if let Ok(off) = registry.get_field_offset("ClubReference", "transfer_retention_pct") {
            driver.write(addr + off, &self.transfer_retention_pct)?;
        }
        if let Ok(off) = registry.get_field_offset("ClubReference", "max_wage_allowance") {
            driver.write(addr + off, &self.max_wage_allowance)?;
        }
        if let Ok(off) = registry.get_field_offset("ClubReference", "stadium_capacity") {
            driver.write(addr + off, &self.stadium_capacity)?;
        }
        if let Ok(off) = registry.get_field_offset("ClubReference", "stadium_expansion_cap") {
            driver.write(addr + off, &self.stadium_expansion_cap)?;
        }

        Ok(())
    }

    /// God mode helper: boosts all club training and youth facilities to 20/20.
    pub fn max_all_facilities(&mut self) {
        self.training_facilities = 20;
        self.youth_facilities = 20;
        self.youth_recruitment = 20;
        self.junior_coaching = 20;
        self.data_analysis_facilities = 20;
        self.corporate_facilities = 20;
    }

    /// Sets sugar daddy type and upgrades board confidence and transfer retention.
    pub fn apply_sugar_daddy(&mut self, sd_type: &str) {
        self.sugar_daddy_type = sd_type.to_string();
        self.board_patience = 20;
        self.fan_confidence = 100;
        self.board_confidence = 100;
        self.transfer_retention_pct = 100;
        if sd_type == "Foreground" {
            self.balance = self.balance.max(500_000_000);
            self.transfer_budget = self.transfer_budget.max(250_000_000);
            self.wage_budget = self.wage_budget.max(6_000_000);
            self.max_wage_allowance = self.max_wage_allowance.max(800_000);
        }
    }
}

/// Represents nation details including hidden youth rating and economic factors.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NationDetails {
    pub id: i32,
    pub name: String,
    pub three_letter_code: String,
    pub continent: String,

    #[serde(default = "default_reputation")]
    pub reputation: u16,           // 1 - 10000
    #[serde(default = "default_youth_rating")]
    pub youth_rating: u8,          // 1 - 200
    #[serde(default = "default_stat_ten")]
    pub economic_factor: u8,       // 1 - 20
    #[serde(default = "default_stat_one")]
    pub state_of_development: u8,  // 0: Third World, 1: Developing, 2: Developed
    #[serde(default = "default_stat_ten")]
    pub fa_financial_power: u8,    // 1 - 20
    #[serde(default = "default_stat_fifteen")]
    pub game_importance: u8,       // 1 - 20
    #[serde(default = "default_tax_rate")]
    pub tax_rate: u8,              // 0 - 100 (%)
    #[serde(default = "default_population")]
    pub population: u32,
    #[serde(default = "default_stat_one")]
    pub corruption_index: u8,      // 0 - 20 (match fixing/scandal factor)
    #[serde(default = "default_stat_ten")]
    pub youth_facilities_avg: u8,  // 1 - 20
    #[serde(default = "default_stat_ten")]
    pub youth_coaching_avg: u8,    // 1 - 20
    #[serde(default = "default_stat_ten")]
    pub youth_recruitment_avg: u8, // 1 - 20
}

impl Default for NationDetails {
    fn default() -> Self {
        Self {
            id: 765,
            name: "England".to_string(),
            three_letter_code: "ENG".to_string(),
            continent: "Europe".to_string(),
            reputation: 8800,
            youth_rating: 145,
            economic_factor: 19,
            state_of_development: 2,
            fa_financial_power: 18,
            game_importance: 20,
            tax_rate: 45,
            population: 57_690_300,
            corruption_index: 2,
            youth_facilities_avg: 17,
            youth_coaching_avg: 16,
            youth_recruitment_avg: 17,
        }
    }
}

impl NationDetails {
    pub fn read(driver: &LinuxMemoryDriver, addr: usize, registry: &OffsetsRegistry) -> Result<Self> {
        let id_offset = registry
            .get_field_offset("NationReference", "m_idHash")
            .unwrap_or(0x18);
        let id: i32 = driver.read(addr + id_offset).unwrap_or(0);

        let reputation: u16 = registry
            .get_field_offset("NationReference", "reputation")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(7500);

        let youth_rating: u8 = registry
            .get_field_offset("NationReference", "youth_rating")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(120);

        let economic_factor: u8 = registry
            .get_field_offset("NationReference", "economic_factor")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(15);

        let state_of_development: u8 = registry
            .get_field_offset("NationReference", "state_of_development")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(2);

        let fa_financial_power: u8 = registry
            .get_field_offset("NationReference", "fa_financial_power")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(15);

        let game_importance: u8 = registry
            .get_field_offset("NationReference", "game_importance")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(18);

        let tax_rate: u8 = registry
            .get_field_offset("NationReference", "tax_rate")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(30);

        let corruption_index: u8 = registry
            .get_field_offset("NationReference", "corruption_index")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(2);

        Ok(Self {
            id,
            name: "Nation".to_string(),
            three_letter_code: "NAT".to_string(),
            continent: "World".to_string(),
            reputation,
            youth_rating,
            economic_factor,
            state_of_development,
            fa_financial_power,
            game_importance,
            tax_rate,
            population: 50_000_000,
            corruption_index,
            youth_facilities_avg: 12,
            youth_coaching_avg: 12,
            youth_recruitment_avg: 12,
        })
    }

    pub fn write(&self, driver: &LinuxMemoryDriver, addr: usize, registry: &OffsetsRegistry) -> Result<()> {
        if let Ok(off) = registry.get_field_offset("NationReference", "reputation") {
            driver.write(addr + off, &self.reputation)?;
        }
        if let Ok(off) = registry.get_field_offset("NationReference", "youth_rating") {
            driver.write(addr + off, &self.youth_rating)?;
        }
        if let Ok(off) = registry.get_field_offset("NationReference", "economic_factor") {
            driver.write(addr + off, &self.economic_factor)?;
        }
        if let Ok(off) = registry.get_field_offset("NationReference", "state_of_development") {
            driver.write(addr + off, &self.state_of_development)?;
        }
        if let Ok(off) = registry.get_field_offset("NationReference", "fa_financial_power") {
            driver.write(addr + off, &self.fa_financial_power)?;
        }
        if let Ok(off) = registry.get_field_offset("NationReference", "game_importance") {
            driver.write(addr + off, &self.game_importance)?;
        }
        if let Ok(off) = registry.get_field_offset("NationReference", "tax_rate") {
            driver.write(addr + off, &self.tax_rate)?;
        }
        if let Ok(off) = registry.get_field_offset("NationReference", "corruption_index") {
            driver.write(addr + off, &self.corruption_index)?;
        }
        Ok(())
    }

    /// God mode helper: sets national youth rating to 200 (maximum possible) and facilities to 20.
    pub fn golden_generation(&mut self) {
        self.youth_rating = 200;
        self.youth_facilities_avg = 20;
        self.youth_coaching_avg = 20;
        self.youth_recruitment_avg = 20;
        self.game_importance = 20;
        self.fa_financial_power = 20;
    }
}

/// Represents competition details, match substitution rules, and financial awards.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompetitionDetails {
    pub id: i32,
    pub name: String,
    pub short_name: String,
    pub nation_name: String,
    pub nation_id: i32,

    #[serde(default = "default_comp_reputation")]
    pub reputation: u16,            // 1 - 10000
    #[serde(default = "default_subs")]
    pub max_substitutes: u8,        // e.g. 5
    #[serde(default = "default_bench")]
    pub max_bench_size: u8,         // e.g. 9 or 12
    #[serde(default = "default_windows")]
    pub substitution_windows: u8,   // e.g. 3
    #[serde(default = "default_true")]
    pub var_active: bool,
    #[serde(default = "default_squad_size")]
    pub max_squad_size: u8,         // e.g. 25
    #[serde(default = "default_foreign")]
    pub max_foreign_players: u8,    // foreign / non-EU quota
    #[serde(default = "default_hg_club")]
    pub min_homegrown_club: u8,     // club-trained minimum
    #[serde(default = "default_hg_nation")]
    pub min_homegrown_nation: u8,   // nation-trained minimum
    #[serde(default)]
    pub winner_prize_money: i64,
    #[serde(default)]
    pub tv_rights_pool: i64,
    #[serde(default = "default_yellow_ban")]
    pub yellow_cards_for_ban: u8,   // e.g. 5
}

impl Default for CompetitionDetails {
    fn default() -> Self {
        Self {
            id: 11,
            name: "Premier League".to_string(),
            short_name: "EPL".to_string(),
            nation_name: "England".to_string(),
            nation_id: 765,
            reputation: 9200,
            max_substitutes: 5,
            max_bench_size: 9,
            substitution_windows: 3,
            var_active: true,
            max_squad_size: 25,
            max_foreign_players: 17,
            min_homegrown_club: 4,
            min_homegrown_nation: 8,
            winner_prize_money: 44_000_000,
            tv_rights_pool: 2_400_000_000,
            yellow_cards_for_ban: 5,
        }
    }
}

impl CompetitionDetails {
    pub fn read(driver: &LinuxMemoryDriver, addr: usize, registry: &OffsetsRegistry) -> Result<Self> {
        let id_offset = registry
            .get_field_offset("CompReference", "m_idHash")
            .unwrap_or(0x18);
        let id: i32 = driver.read(addr + id_offset).unwrap_or(0);

        let reputation: u16 = registry
            .get_field_offset("CompReference", "reputation")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(8000);

        let max_substitutes: u8 = registry
            .get_field_offset("CompReference", "max_substitutes")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(5);

        let max_bench_size: u8 = registry
            .get_field_offset("CompReference", "max_bench_size")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(9);

        let substitution_windows: u8 = registry
            .get_field_offset("CompReference", "substitution_windows")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(3);

        let var_raw: u8 = registry
            .get_field_offset("CompReference", "var_active")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(1);
        let var_active = var_raw != 0;

        let max_squad_size: u8 = registry
            .get_field_offset("CompReference", "max_squad_size")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(25);

        let max_foreign_players: u8 = registry
            .get_field_offset("CompReference", "max_foreign_players")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(17);

        let min_homegrown_club: u8 = registry
            .get_field_offset("CompReference", "min_homegrown_club")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(4);

        let min_homegrown_nation: u8 = registry
            .get_field_offset("CompReference", "min_homegrown_nation")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(8);

        let winner_prize_money: i64 = registry
            .get_field_offset("CompReference", "winner_prize_money")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(30_000_000);

        let tv_rights_pool: i64 = registry
            .get_field_offset("CompReference", "tv_rights_pool")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(1_500_000_000);

        let yellow_cards_for_ban: u8 = registry
            .get_field_offset("CompReference", "yellow_cards_for_ban")
            .ok()
            .and_then(|off| driver.read(addr + off).ok())
            .unwrap_or(5);

        Ok(Self {
            id,
            name: "Competition".to_string(),
            short_name: "COMP".to_string(),
            nation_name: "Nation".to_string(),
            nation_id: 0,
            reputation,
            max_substitutes,
            max_bench_size,
            substitution_windows,
            var_active,
            max_squad_size,
            max_foreign_players,
            min_homegrown_club,
            min_homegrown_nation,
            winner_prize_money,
            tv_rights_pool,
            yellow_cards_for_ban,
        })
    }

    pub fn write(&self, driver: &LinuxMemoryDriver, addr: usize, registry: &OffsetsRegistry) -> Result<()> {
        if let Ok(off) = registry.get_field_offset("CompReference", "reputation") {
            driver.write(addr + off, &self.reputation)?;
        }
        if let Ok(off) = registry.get_field_offset("CompReference", "max_substitutes") {
            driver.write(addr + off, &self.max_substitutes)?;
        }
        if let Ok(off) = registry.get_field_offset("CompReference", "max_bench_size") {
            driver.write(addr + off, &self.max_bench_size)?;
        }
        if let Ok(off) = registry.get_field_offset("CompReference", "substitution_windows") {
            driver.write(addr + off, &self.substitution_windows)?;
        }
        let var_val: u8 = if self.var_active { 1 } else { 0 };
        if let Ok(off) = registry.get_field_offset("CompReference", "var_active") {
            driver.write(addr + off, &var_val)?;
        }
        if let Ok(off) = registry.get_field_offset("CompReference", "max_squad_size") {
            driver.write(addr + off, &self.max_squad_size)?;
        }
        if let Ok(off) = registry.get_field_offset("CompReference", "max_foreign_players") {
            driver.write(addr + off, &self.max_foreign_players)?;
        }
        if let Ok(off) = registry.get_field_offset("CompReference", "min_homegrown_club") {
            driver.write(addr + off, &self.min_homegrown_club)?;
        }
        if let Ok(off) = registry.get_field_offset("CompReference", "min_homegrown_nation") {
            driver.write(addr + off, &self.min_homegrown_nation)?;
        }
        if let Ok(off) = registry.get_field_offset("CompReference", "winner_prize_money") {
            driver.write(addr + off, &self.winner_prize_money)?;
        }
        if let Ok(off) = registry.get_field_offset("CompReference", "tv_rights_pool") {
            driver.write(addr + off, &self.tv_rights_pool)?;
        }
        if let Ok(off) = registry.get_field_offset("CompReference", "yellow_cards_for_ban") {
            driver.write(addr + off, &self.yellow_cards_for_ban)?;
        }
        Ok(())
    }

    /// God mode helper: relaxes registration constraints (unlimited foreign players, 12 subs on bench).
    pub fn relax_rules(&mut self) {
        self.max_foreign_players = 99;
        self.max_squad_size = 50;
        self.min_homegrown_club = 0;
        self.min_homegrown_nation = 0;
        self.max_substitutes = 5;
        self.max_bench_size = 12;
        self.substitution_windows = 3;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActiveContext {
    pub attached: bool,
    pub pid: Option<i32>,
    pub active_screen: String, // "player", "club", "squad", "nation", "competition"
    pub sync_mode: String,     // "auto_follow", "pinned"
    pub player: Player,
    pub club: ClubFinances,
    #[serde(default)]
    pub nation: NationDetails,
    #[serde(default)]
    pub competition: CompetitionDetails,
}

impl Default for ActiveContext {
    fn default() -> Self {
        Self {
            attached: false,
            pid: None,
            active_screen: "player".to_string(),
            sync_mode: "auto_follow".to_string(),
            player: Player::default(),
            club: ClubFinances::default(),
            nation: NationDetails::default(),
            competition: CompetitionDetails::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerSearchResult {
    pub name: String,
    pub address: usize,
    pub is_utf16: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClubSearchResult {
    pub name: String,
    pub address: usize,
    pub is_utf16: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResults {
    pub query: String,
    pub utf8_matches: Vec<usize>,
    pub utf16_matches: Vec<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_player_positions_parsing_and_mask() {
        let p = PlayerPositions::from_position_string("AM (RL), ST (C)");
        assert_eq!(p.st, 20);
        assert_eq!(p.aml, 20);
        assert_eq!(p.amr, 20);
        let mask = p.to_position_mask();
        assert_eq!(mask & 16384, 16384); // ST
        assert_eq!(mask & 4096, 4096);   // AML
        assert_eq!(mask & 2048, 2048);   // AMR

        let roundtrip = PlayerPositions::from_position_mask(mask);
        assert_eq!(roundtrip.st, 20);
        assert_eq!(roundtrip.aml, 20);
        assert_eq!(roundtrip.amr, 20);

        let p_dr = PlayerPositions::from_position_string("D (R)");
        assert_eq!(p_dr.dr, 20);
        assert_eq!(p_dr.dl, 1);
        assert_eq!(p_dr.dc, 1);

        let p_dr2 = PlayerPositions::from_position_string("D(R)");
        assert_eq!(p_dr2.dr, 20);
        assert_eq!(p_dr2.dl, 1);
        assert_eq!(p_dr2.dc, 1);
    }
}
