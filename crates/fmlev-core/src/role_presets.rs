//! Role presets: raise a player's attributes and positions toward a role profile in three levels.
//!
//! Pure proposal logic with no memory access. A proposal lists field changes (old -> new) that the
//! UI stages and saves through the normal verified player write path.
//!
//! **Unverified role data**: the role names and key/secondary attribute lists below follow the
//! widely used Football Manager role model and have NOT been checked against FM26's own role
//! screens (plan section B1). Every proposal carries `role_data_verified: false` until they are.

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::models::{PlayerAttributes, PlayerPositions};

/// Whether the shipped role table has been compared with FM26's role screens.
pub const ROLE_DATA_VERIFIED: bool = false;

/// CA points added per attribute point raised in `raise_to_fit` mode. **Heuristic**: the game's CA
/// weights are not mapped, so this only keeps CA in the right ballpark; it is shown in the diff.
pub const CA_PER_ATTRIBUTE_POINT: f64 = 1.5;

pub struct RoleDef {
    pub id: &'static str,
    pub name: &'static str,
    pub group: &'static str,
    /// Position keys (`PlayerPositions` field names) that must reach the level's aptitude.
    pub positions: &'static [&'static str],
    pub key: &'static [&'static str],
    pub secondary: &'static [&'static str],
}

pub const ROLES: &[RoleDef] = &[
    RoleDef { id: "sweeper_keeper", name: "Sweeper Keeper", group: "Goalkeeper", positions: &["gk"],
        key: &["reflexes", "handling", "one_on_ones", "command_of_area", "communication", "kicking", "first_touch", "passing", "anticipation", "decisions", "composure"],
        secondary: &["aerial_reach", "rushing_out", "throwing", "concentration", "agility"] },
    RoleDef { id: "ball_playing_defender", name: "Ball-Playing Defender", group: "Defender", positions: &["dc"],
        key: &["marking", "tackling", "heading", "passing", "composure", "anticipation", "positioning", "jumping_reach", "strength"],
        secondary: &["first_touch", "technique", "vision", "decisions", "concentration", "pace"] },
    RoleDef { id: "no_nonsense_centre_back", name: "No-Nonsense Centre-Back", group: "Defender", positions: &["dc"],
        key: &["marking", "tackling", "heading", "anticipation", "positioning", "bravery", "jumping_reach", "strength"],
        secondary: &["aggression", "concentration", "decisions", "pace"] },
    RoleDef { id: "wing_back", name: "Wing-Back", group: "Defender", positions: &["wbl", "wbr"],
        key: &["crossing", "dribbling", "tackling", "work_rate", "teamwork", "acceleration", "pace", "stamina"],
        secondary: &["first_touch", "marking", "passing", "technique", "anticipation", "off_the_ball", "agility", "balance"] },
    RoleDef { id: "anchor", name: "Anchor", group: "Midfielder", positions: &["dm"],
        key: &["marking", "tackling", "anticipation", "composure", "concentration", "decisions", "positioning", "teamwork"],
        secondary: &["heading", "passing", "bravery", "strength", "work_rate"] },
    RoleDef { id: "deep_lying_playmaker", name: "Deep-Lying Playmaker", group: "Midfielder", positions: &["dm", "mc"],
        key: &["first_touch", "passing", "technique", "composure", "decisions", "teamwork", "vision"],
        secondary: &["tackling", "anticipation", "positioning", "balance"] },
    RoleDef { id: "ball_winning_midfielder", name: "Ball-Winning Midfielder", group: "Midfielder", positions: &["dm", "mc"],
        key: &["tackling", "aggression", "anticipation", "bravery", "teamwork", "work_rate", "stamina"],
        secondary: &["marking", "concentration", "determination", "positioning", "pace", "strength"] },
    RoleDef { id: "box_to_box_midfielder", name: "Box-to-Box Midfielder", group: "Midfielder", positions: &["mc"],
        key: &["passing", "tackling", "off_the_ball", "teamwork", "work_rate", "stamina"],
        secondary: &["dribbling", "finishing", "long_shots", "technique", "decisions", "determination", "acceleration", "pace", "strength"] },
    RoleDef { id: "advanced_playmaker", name: "Advanced Playmaker", group: "Midfielder", positions: &["amc", "mc"],
        key: &["first_touch", "passing", "technique", "composure", "decisions", "flair", "off_the_ball", "teamwork", "vision"],
        secondary: &["dribbling", "anticipation", "agility", "balance"] },
    RoleDef { id: "inverted_winger", name: "Inverted Winger", group: "Attacker", positions: &["aml", "amr"],
        key: &["dribbling", "first_touch", "technique", "acceleration", "agility", "pace"],
        secondary: &["crossing", "passing", "composure", "decisions", "flair", "off_the_ball", "teamwork", "vision", "work_rate", "balance", "stamina"] },
    RoleDef { id: "poacher", name: "Poacher", group: "Attacker", positions: &["st"],
        key: &["finishing", "anticipation", "composure", "off_the_ball", "acceleration"],
        secondary: &["first_touch", "heading", "decisions", "concentration", "pace"] },
    RoleDef { id: "target_forward", name: "Target Forward", group: "Attacker", positions: &["st"],
        key: &["heading", "bravery", "strength", "jumping_reach", "balance"],
        secondary: &["finishing", "first_touch", "aggression", "anticipation", "composure", "teamwork", "work_rate"] },
    RoleDef { id: "complete_forward", name: "Complete Forward", group: "Attacker", positions: &["st"],
        key: &["dribbling", "finishing", "first_touch", "heading", "passing", "technique", "anticipation", "composure", "off_the_ball", "acceleration", "agility", "pace", "strength"],
        secondary: &["decisions", "vision", "teamwork", "work_rate", "balance", "jumping_reach", "stamina"] },
];

pub fn role(id: &str) -> Option<&'static RoleDef> {
    ROLES.iter().find(|r| r.id == id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaMode {
    Keep,
    RaiseToFit,
}

/// Level floors: (key attributes, secondary attributes, position aptitude).
pub fn level_floors(level: u8) -> Result<(u8, u8, u8)> {
    match level {
        1 => Ok((14, 12, 15)),
        2 => Ok((16, 14, 18)),
        3 => Ok((18, 16, 20)),
        other => Err(CoreError::InvalidValue(format!("Role level must be 1, 2 or 3 (got {other})"))),
    }
}

/// Level 3 raises CA/PA to fit by default; levels 1 and 2 keep CA unless the user opts in.
pub fn default_ca_mode(level: u8) -> CaMode {
    if level >= 3 { CaMode::RaiseToFit } else { CaMode::Keep }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldChange {
    pub field: String,
    pub old: u32,
    pub new: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoleProposal {
    pub role_id: &'static str,
    pub role_name: &'static str,
    pub level: u8,
    pub ca_mode: CaMode,
    /// False until the role table is compared with FM26's role screens.
    pub role_data_verified: bool,
    /// CA changes use a rough per-point estimate, not the game's formula.
    pub ca_is_heuristic: bool,
    pub changes: Vec<FieldChange>,
}

fn to_map<T: Serialize>(value: &T) -> serde_json::Map<String, serde_json::Value> {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    }
}

/// Builds a proposal. Only raises values to the level floor; never lowers an attribute, position, CA or PA.
pub fn propose(
    role_id: &str,
    level: u8,
    ca_mode: Option<CaMode>,
    attributes: &PlayerAttributes,
    positions: &PlayerPositions,
    current_ability: u8,
    potential_ability: u8,
) -> Result<RoleProposal> {
    let def = role(role_id).ok_or_else(|| CoreError::InvalidValue(format!("Unknown role '{role_id}'")))?;
    let (key_floor, secondary_floor, position_floor) = level_floors(level)?;
    let ca_mode = ca_mode.unwrap_or_else(|| default_ca_mode(level));

    let attrs = to_map(attributes);
    let pos = to_map(positions);
    let mut changes = Vec::new();
    let mut points_added: u32 = 0;

    let mut raise = |map: &serde_json::Map<String, serde_json::Value>, prefix: &str, name: &str, floor: u8, count_points: bool, changes: &mut Vec<FieldChange>| -> Result<()> {
        let current = map
            .get(name)
            .and_then(|v| v.as_u64())
            .ok_or_else(|| CoreError::InvalidValue(format!("Role data names unknown field '{name}'")))? as u32;
        if current == 0 {
            // 0 marks a value that was not read from the game; never build a write on it.
            return Err(CoreError::InvalidValue(format!("{prefix}.{name} has not been read from the game")));
        }
        if current < u32::from(floor) {
            if count_points {
                points_added += u32::from(floor) - current;
            }
            changes.push(FieldChange { field: format!("{prefix}.{name}"), old: current, new: u32::from(floor) });
        }
        Ok(())
    };

    for name in def.positions {
        raise(&pos, "positions", name, position_floor, false, &mut changes)?;
    }
    for name in def.key {
        raise(&attrs, "attributes", name, key_floor, true, &mut changes)?;
    }
    for name in def.secondary {
        // A name listed as key keeps the higher key floor.
        if !def.key.contains(name) {
            raise(&attrs, "attributes", name, secondary_floor, true, &mut changes)?;
        }
    }

    if ca_mode == CaMode::RaiseToFit && points_added > 0 {
        let ca = u32::from(current_ability);
        let pa = u32::from(potential_ability);
        let new_ca = (ca + (f64::from(points_added) * CA_PER_ATTRIBUTE_POINT).ceil() as u32).min(200);
        let new_pa = pa.max(new_ca);
        if new_ca > ca {
            changes.push(FieldChange { field: "current_ability".into(), old: ca, new: new_ca });
        }
        if new_pa > pa {
            changes.push(FieldChange { field: "potential_ability".into(), old: pa, new: new_pa });
        }
    }

    Ok(RoleProposal {
        role_id: def.id,
        role_name: def.name,
        level,
        ca_mode,
        role_data_verified: ROLE_DATA_VERIFIED,
        ca_is_heuristic: true,
        changes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weak() -> (PlayerAttributes, PlayerPositions) {
        let json = |v: u8| {
            let mut m = to_map(&PlayerAttributes::default());
            for value in m.values_mut() {
                *value = serde_json::json!(v);
            }
            serde_json::from_value::<PlayerAttributes>(serde_json::Value::Object(m)).unwrap()
        };
        (json(8), PlayerPositions::default())
    }

    #[test]
    fn every_role_field_name_exists_and_roles_are_unique() {
        let attrs = to_map(&PlayerAttributes::default());
        let pos = to_map(&PlayerPositions::default());
        for (i, r) in ROLES.iter().enumerate() {
            assert!(ROLES[i + 1..].iter().all(|o| o.id != r.id), "{}", r.id);
            assert!(!r.key.is_empty() && !r.positions.is_empty());
            for name in r.key.iter().chain(r.secondary) {
                assert!(attrs.contains_key(*name), "{} unknown attribute {name}", r.id);
            }
            for name in r.positions {
                assert!(pos.contains_key(*name) && !name.contains("foot"), "{} unknown position {name}", r.id);
            }
        }
    }

    #[test]
    fn levels_raise_to_floors_without_lowering() {
        let (mut attrs, positions) = weak();
        attrs.passing = 19;
        attrs.vision = 3;
        let p = propose("deep_lying_playmaker", 2, None, &attrs, &positions, 100, 120).unwrap();
        let get = |f: &str| p.changes.iter().find(|c| c.field == f);
        assert!(get("attributes.passing").is_none(), "19 is above the floor and must not change");
        assert_eq!(get("attributes.vision").map(|c| (c.old, c.new)), Some((3, 16)));
        assert_eq!(get("attributes.tackling").map(|c| c.new), Some(14), "secondary floor");
        assert_eq!(get("positions.dm").map(|c| c.new), Some(18));
        assert!(get("positions.mc").is_none(), "default mc is already 20");
        assert!(p.changes.iter().all(|c| c.new > c.old));
    }

    #[test]
    fn ca_modes_follow_the_level_defaults() {
        let (attrs, positions) = weak();
        let l1 = propose("poacher", 1, None, &attrs, &positions, 100, 120).unwrap();
        let l2 = propose("poacher", 2, None, &attrs, &positions, 100, 120).unwrap();
        let l3 = propose("poacher", 3, None, &attrs, &positions, 100, 120).unwrap();
        assert_eq!((l1.ca_mode, l2.ca_mode, l3.ca_mode), (CaMode::Keep, CaMode::Keep, CaMode::RaiseToFit));
        assert!(!l1.changes.iter().any(|c| c.field.ends_with("_ability")));
        assert!(!l2.changes.iter().any(|c| c.field.ends_with("_ability")));
        let ca = l3.changes.iter().find(|c| c.field == "current_ability").unwrap();
        let pa = l3.changes.iter().find(|c| c.field == "potential_ability").unwrap();
        assert!(ca.new > ca.old && ca.new <= 200);
        assert!(pa.new >= ca.new);
        let opted_in = propose("poacher", 1, Some(CaMode::RaiseToFit), &attrs, &positions, 100, 120).unwrap();
        assert!(opted_in.changes.iter().any(|c| c.field == "current_ability"));
        let opted_out = propose("poacher", 3, Some(CaMode::Keep), &attrs, &positions, 100, 120).unwrap();
        assert!(!opted_out.changes.iter().any(|c| c.field.ends_with("_ability")));
    }

    #[test]
    fn ca_never_exceeds_200_or_lowers_pa() {
        let (attrs, positions) = weak();
        let p = propose("complete_forward", 3, None, &attrs, &positions, 199, 200).unwrap();
        let ca = p.changes.iter().find(|c| c.field == "current_ability").unwrap();
        assert_eq!(ca.new, 200);
        assert!(!p.changes.iter().any(|c| c.field == "potential_ability"));
    }

    #[test]
    fn goalkeeper_role_uses_goalkeeping_block_and_gk_position() {
        let (attrs, positions) = weak();
        let p = propose("sweeper_keeper", 3, None, &attrs, &positions, 100, 150).unwrap();
        assert!(p.changes.iter().any(|c| c.field == "attributes.reflexes" && c.new == 18));
        assert!(p.changes.iter().any(|c| c.field == "positions.gk" && c.new == 20));
    }

    #[test]
    fn already_qualified_player_gets_no_changes() {
        let (mut attrs, mut positions) = weak();
        let mut map = to_map(&attrs);
        for value in map.values_mut() {
            *value = serde_json::json!(20);
        }
        attrs = serde_json::from_value(serde_json::Value::Object(map)).unwrap();
        positions.st = 20;
        let p = propose("poacher", 3, None, &attrs, &positions, 150, 180).unwrap();
        assert!(p.changes.is_empty());
    }

    #[test]
    fn unread_zero_values_are_refused() {
        let (mut attrs, positions) = weak();
        attrs.finishing = 0;
        assert!(propose("poacher", 2, None, &attrs, &positions, 100, 120).is_err());
    }

    #[test]
    fn unknown_role_and_level_are_rejected() {
        let (attrs, positions) = weak();
        assert!(propose("nope", 1, None, &attrs, &positions, 100, 100).is_err());
        assert!(propose("poacher", 0, None, &attrs, &positions, 100, 100).is_err());
        assert!(propose("poacher", 4, None, &attrs, &positions, 100, 100).is_err());
    }
}
