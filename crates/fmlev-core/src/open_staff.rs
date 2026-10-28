//! Read-only: which staff profile is open in FM26 right now.
//!
//! The game binds staff-only properties (`AttributesCoaching*`, `AttributesMedical`,
//! `AttributesBlockInitialStaffSelection`) on the `PersonReference` of the profile on screen, with
//! `IsPlayer = 0`. Observed live (2026-10-28, PID 5668): Iván Ortega's profile bound id 20414, which is his
//! native staff index. Only the index comes from here; the caller resolves identity (uid, name) natively.
//! The bindings are a UI cache: stale entries are possible after leaving a profile, so the result is a
//! hint that the caller must validate, never an authority.

use crate::context::inspector::resolve_binding_subsystem;
use crate::live_dump::{dump_bound_properties, BoundProperty};
use crate::session::LiveSession;

const PERSON_REFERENCE: &str = "PersonReference";

fn is_staff_only_property(property: &str) -> bool {
    property.starts_with("AttributesCoaching") || property == "AttributesMedical" || property == "AttributesBlockInitialStaffSelection"
}

/// Person ids that carry a staff-only attribute binding and are explicitly not players, sorted and unique.
pub fn open_staff_ids(props: &[BoundProperty]) -> Vec<i32> {
    let not_player = |id: i32| props.iter().any(|p| p.reference_class == PERSON_REFERENCE && p.reference_id == id && p.property == "IsPlayer" && p.value == "i64 0");
    let mut ids: Vec<i32> = props
        .iter()
        .filter(|p| p.reference_class == PERSON_REFERENCE && is_staff_only_property(&p.property))
        .map(|p| p.reference_id)
        .filter(|id| not_player(*id))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Staff indexes bound in the live UI. Empty when no staff profile is bound or the bindings are unreadable.
pub fn read_open_staff_ids(session: &LiveSession) -> Vec<i32> {
    resolve_binding_subsystem(session).map(|binding| open_staff_ids(&dump_bound_properties(&session.driver, binding))).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prop(class: &str, id: i32, property: &str, value: &str) -> BoundProperty {
        BoundProperty {
            reference_class: class.into(),
            reference_type: 1,
            reference_id: id,
            property: property.into(),
            value_class: "x".into(),
            value: value.into(),
        }
    }

    #[test]
    fn only_non_player_persons_with_staff_attribute_bindings_count() {
        let props = vec![
            prop("PersonReference", 20414, "AttributesMedical", "list"),
            prop("PersonReference", 20414, "AttributesCoachingMental", "list"),
            prop("PersonReference", 20414, "IsPlayer", "i64 0"),
            // A player profile never has IsPlayer = 0, even if it shared a property name.
            prop("PersonReference", 77, "AttributesCoaching", "list"),
            prop("PersonReference", 77, "IsPlayer", "i64 1"),
            // Missing IsPlayer: not proven to be staff.
            prop("PersonReference", 88, "AttributesMedical", "list"),
            // Wrong reference class.
            prop("ClubReference", 99, "AttributesMedical", "list"),
            prop("PersonReference", 99, "IsPlayer", "i64 0"),
            // Ordinary person bindings do not count.
            prop("PersonReference", 5, "UniqueId", "i64 1"),
            prop("PersonReference", 5, "IsPlayer", "i64 0"),
        ];
        assert_eq!(open_staff_ids(&props), vec![20414]);
    }

    #[test]
    fn several_open_staff_are_all_reported_sorted() {
        let mut props = Vec::new();
        for id in [30, 10] {
            props.push(prop("PersonReference", id, "AttributesBlockInitialStaffSelection", "x"));
            props.push(prop("PersonReference", id, "IsPlayer", "i64 0"));
        }
        assert_eq!(open_staff_ids(&props), vec![10, 30]);
        assert!(open_staff_ids(&[]).is_empty());
    }
}
