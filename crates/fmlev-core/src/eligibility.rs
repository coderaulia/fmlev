//! Registration/eligibility doctor: turns the registration gates that are mapped into a gate list
//! with `ok | blocked | unknown` status. A gate whose data is not mapped is `unknown`, never `ok`.

use serde::Serialize;

use crate::native_registration::RegistrationGates;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GateStatus {
    Ok,
    Blocked,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct GateReport {
    pub id: &'static str,
    pub title: &'static str,
    pub status: GateStatus,
    pub evidence: String,
    /// What the editor can do about it, if anything.
    pub fix: Option<&'static str>,
}

fn gate(id: &'static str, title: &'static str, status: GateStatus, evidence: impl Into<String>, fix: Option<&'static str>) -> GateReport {
    GateReport { id, title, status, evidence: evidence.into(), fix }
}

pub fn evaluate(gates: &RegistrationGates) -> Vec<GateReport> {
    let mut out = Vec::new();

    let bypass = gates.manager_registration_bit;
    let (status, evidence) = match (gates.stage_gate_open, bypass) {
        (true, _) => (GateStatus::Ok, "The competition's registration gate is open.".to_string()),
        (false, true) => (GateStatus::Ok, "The competition gate is closed, but Register Anytime is on for the manager.".to_string()),
        (false, false) => (GateStatus::Blocked, "The competition's registration gate is closed and Register Anytime is off. The game labels this 'Registration Restricted' and can still allow a limited number of registrations (the limit is not mapped).".to_string()),
    };
    out.push(gate(
        "registration_window",
        "Registration window",
        status,
        evidence,
        (status == GateStatus::Blocked).then_some("Turn on Allow Squad Registration Anytime for this competition."),
    ));

    out.push(gate(
        "squad_number_window",
        "Squad number window",
        if gates.manager_squad_number_bit || gates.stage_gate_open { GateStatus::Ok } else { GateStatus::Unknown },
        if gates.manager_squad_number_bit {
            "Squad-number-anytime is on for the manager.".to_string()
        } else {
            "Squad numbers also depend on calendar windows that are not read here.".to_string()
        },
        None,
    ));

    out.push(gate(
        "squad_size",
        "Registered squad size",
        GateStatus::Unknown,
        format!(
            "Competition allows {} registered players. The club's current registered count is not mapped, so this cannot be checked.",
            gates.max_squad_size
        ),
        None,
    ));

    out.push(gate(
        "homegrown",
        "Home-grown players",
        GateStatus::Unknown,
        if gates.min_homegrown == 255 {
            "No home-grown requirement for this competition. Player training history is not mapped.".to_string()
        } else {
            format!(
                "Native data says {} home-grown players are required, but unconfirmed: the game's La Liga registration screen did not list a home-grown rule. Nationality alone does not make a player home-grown (training history decides), and that history is not mapped, so this cannot be checked.",
                gates.min_homegrown
            )
        },
        None,
    ));

    out.push(gate(
        "squad_number_cap",
        "Highest squad number",
        GateStatus::Unknown,
        if gates.max_squad_number == 255 {
            "No squad number cap for this competition.".to_string()
        } else {
            format!("Squad numbers are capped at {}.", gates.max_squad_number)
        },
        None,
    ));

    for (id, title, why) in [
        ("continental_registration", "Continental squad registration", "Competition registration type is not mapped. Check that competition's registration deadline and enable Register Anytime."),
        ("non_eu_quota", "Non-EU signing slots (Serie A)", "Per-club non-EU slot counters are not mapped."),
        ("work_permit", "Work permit", "Work-permit status is not mapped. Copy the exact in-game message into the support report."),
        ("appearance_limit", "Appearance limit or other eligibility rule", "Not mapped. Record the competition and the exact in-game message in the support report."),
    ] {
        out.push(gate(id, title, GateStatus::Unknown, why, None));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gates(open: bool, bypass: bool) -> RegistrationGates {
        RegistrationGates {
            manager_registration_bit: bypass,
            manager_squad_number_bit: bypass,
            stage_gate_open: open,
            competition_type: 8,
            max_squad_size: 25,
            min_homegrown: 8,
            max_squad_number: 255,
        }
    }
    fn status(reports: &[GateReport], id: &str) -> GateStatus {
        reports.iter().find(|r| r.id == id).unwrap().status
    }

    #[test]
    fn closed_window_without_bypass_is_blocked_with_a_fix() {
        let reports = evaluate(&gates(false, false));
        assert_eq!(status(&reports, "registration_window"), GateStatus::Blocked);
        assert!(reports.iter().find(|r| r.id == "registration_window").unwrap().fix.is_some());
    }

    #[test]
    fn open_gate_or_bypass_is_ok() {
        assert_eq!(status(&evaluate(&gates(true, false)), "registration_window"), GateStatus::Ok);
        assert_eq!(status(&evaluate(&gates(false, true)), "registration_window"), GateStatus::Ok);
    }

    #[test]
    fn unmapped_gates_are_unknown_never_ok() {
        let reports = evaluate(&gates(true, true));
        for id in ["squad_size", "homegrown", "continental_registration", "non_eu_quota", "work_permit", "appearance_limit"] {
            assert_eq!(status(&reports, id), GateStatus::Unknown, "{id}");
        }
        assert!(reports.iter().find(|r| r.id == "homegrown").unwrap().evidence.contains("8 home-grown"));
    }
}
