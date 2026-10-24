use crate::command::{CommandResult, HudCommand, HudController};
use crate::state::{NotificationLevel, OverlayState, OverlayTab};

pub struct TerminalHud;

impl TerminalHud {
    /// Renders a colorized terminal HUD box
    pub fn render(state: &OverlayState) -> String {
        let mut out = String::new();

        // Colors
        let cyan = "\x1b[38;2;6;182;212m";
        let emerald = "\x1b[38;2;16;185;129m";
        let gold = "\x1b[38;2;245;158;11m";
        let red = "\x1b[38;2;239;68;68m";
        let gray = "\x1b[38;2;148;163;184m";
        let bold = "\x1b[1m";
        let reset = "\x1b[0m";

        out.push_str(&format!(
            "{bold}{cyan}╔══════════════════════════════════════════════════════════════════════════════╗{reset}\n"
        ));
        out.push_str(&format!(
            "{bold}{cyan}║       FMLEV 26 - FOOTBALL MANAGER 26 LIVE EDITOR & HUD OVERLAY (LINUX)       ║{reset}\n"
        ));
        out.push_str(&format!(
            "{bold}{cyan}╠══════════════════════════════════════════════════════════════════════════════╣{reset}\n"
        ));

        // Connection Status
        if let Some(pid) = state.target_pid {
            let ga_str = state
                .game_assembly_base
                .map(|b| format!("0x{:x}", b))
                .unwrap_or_else(|| "N/A".to_string());
            let up_str = state
                .unity_player_base
                .map(|b| format!("0x{:x}", b))
                .unwrap_or_else(|| "N/A".to_string());

            out.push_str(&format!(
                "║ Status: {bold}{emerald}● ATTACHED{reset} | PID: {bold}{emerald}{:<6}{reset} | Mode: {bold}{:<15}{reset}║\n",
                pid,
                format!("{:?}", state.mode),
            ));
            out.push_str(&format!(
                "║ GameAssembly.dll Base: {bold}{cyan}{:<18}{reset} | UnityPlayer: {bold}{cyan}{:<18}{reset}║\n",
                ga_str, up_str,
            ));
            if let Some(ref pfx) = state.wine_prefix {
                let pfx_disp = if pfx.len() > 50 {
                    format!("...{}", &pfx[pfx.len() - 47..])
                } else {
                    pfx.clone()
                };
                out.push_str(&format!(
                    "║ Wine Prefix: {gray}{:<62}{reset}║\n",
                    pfx_disp
                ));
            }
        } else {
            out.push_str(&format!(
                "║ Status: {bold}{red}○ DISCONNECTED{reset} (Waiting for Football Manager 26 process...){:<15}║\n",
                ""
            ));
        }

        out.push_str(&format!(
            "{bold}{cyan}╠══════════════════════════════════════════════════════════════════════════════╣{reset}\n"
        ));
        out.push_str(&format!(
            "║ Active Tab: {bold}{gold}{:<16}{reset} | Visibility: {:<33}║\n",
            format!("{:?}", state.active_tab),
            if state.is_visible { "VISIBLE (On Screen)" } else { "HIDDEN" }
        ));

        out.push_str(&format!(
            "{bold}{cyan}╠══════════════════════════════════════════════════════════════════════════════╣{reset}\n"
        ));

        // Content per active tab
        match state.active_tab {
            OverlayTab::Nation => {
                out.push_str("║ [Tab: Nation Editor & Development Rating]                                    ║\n");
                let nat = state.active_nation.as_ref();
                let nat_name = nat.map(|n| n.name.as_str()).unwrap_or("England");
                let nat_code = nat.map(|n| n.three_letter_code.as_str()).unwrap_or("ENG");
                let nat_id = nat.map(|n| n.id).unwrap_or(765);
                let nat_cont = nat.map(|n| n.continent.as_str()).unwrap_or("Europe");
                let yr = nat.map(|n| n.youth_rating).unwrap_or(145);
                let fac = nat.map(|n| n.youth_facilities_avg).unwrap_or(17);
                let imp = nat.map(|n| n.game_importance).unwrap_or(20);
                let econ = nat.map(|n| n.economic_factor).unwrap_or(19);

                out.push_str(&format!(
                    "║ Nation: {bold}{gold}{:<14}{reset} [{:<3}] (ID: {:<4}) | Continent: {:<15}║\n",
                    nat_name, nat_code, nat_id, nat_cont
                ));
                out.push_str(&format!(
                    "║ Youth Rating: {bold}{cyan}{:<3}/200{reset} | Facilities: {:<2}/20 | Importance: {:<2}/20 | Econ: {:<2}/20 ║\n",
                    yr, fac, imp, econ
                ));
                out.push_str(&format!(
                    "{bold}{cyan}╠══════════════════════════════════════════════════════════════════════════════╣{reset}\n"
                ));
                out.push_str("║ Nation Quick Actions / God-Mode Macros:                                      ║\n");
                out.push_str(&format!(
                    "║   {bold}[G]{reset} 🌟 golden_generation (Set Youth Rating 200 & Facilities 20/20)        ║\n"
                ));
                out.push_str(&format!(
                    "║   {bold}[Y]{reset} 📈 max_youth_rating  (Set Youth Rating 200 - Elite Wonderkids)        ║\n"
                ));
                out.push_str(&format!(
                    "║   {bold}[N]{reset} 🌐 select_nation     (Usage: nation <id>)                             ║\n"
                ));
            }
            OverlayTab::Competition => {
                out.push_str("║ [Tab: Competition & Match Rules]                                             ║\n");
                let comp = state.active_competition.as_ref();
                let comp_name = comp.map(|c| c.name.as_str()).unwrap_or("Premier League");
                let comp_short = comp.map(|c| c.short_name.as_str()).unwrap_or("EPL");
                let comp_id = comp.map(|c| c.id).unwrap_or(11);
                let subs = comp.map(|c| c.max_substitutes).unwrap_or(5);
                let bench = comp.map(|c| c.max_bench_size).unwrap_or(9);
                let foreign = comp.map(|c| c.max_foreign_players).unwrap_or(17);
                let squad = comp.map(|c| c.max_squad_size).unwrap_or(25);

                out.push_str(&format!(
                    "║ Competition: {bold}{gold}{:<15}{reset} [{:<4}] (ID: {:<4}) | Squad Cap: {:<4}             ║\n",
                    comp_name, comp_short, comp_id, squad
                ));
                out.push_str(&format!(
                    "║ Max Subs: {bold}{cyan}{:<2}{reset} | Bench: {bold}{cyan}{:<2}{reset} | Windows: 3 | Foreign Player Limit: {bold}{cyan}{:<3}{reset}          ║\n",
                    subs, bench, foreign
                ));
                out.push_str(&format!(
                    "{bold}{cyan}╠══════════════════════════════════════════════════════════════════════════════╣{reset}\n"
                ));
                out.push_str("║ Competition Quick Actions / God-Mode Macros:                                 ║\n");
                out.push_str(&format!(
                    "║   {bold}[R]{reset} 🔓 relax_rules (Foreign Limit 99, Squad Cap 50, Homegrown 0, 12 Bench)║\n"
                ));
                out.push_str(&format!(
                    "║   {bold}[5]{reset} ⚽ five_subs   (Standardize 5 Substitutions Across 3 Windows)         ║\n"
                ));
                out.push_str(&format!(
                    "║   {bold}[C]{reset} 🏆 select_comp (Usage: competition <id>)                               ║\n"
                ));
            }
            OverlayTab::ClubFinances => {
                out.push_str("║ [Tab: Club Facilities & Ownership]                                           ║\n");
                let club = state.active_club.as_ref();
                let club_name = club.map(|c| if c.club_name.is_empty() { "Active Club" } else { c.club_name.as_str() }).unwrap_or("Active Club");
                let tf = club.map(|c| c.training_facilities).unwrap_or(10);
                let yf = club.map(|c| c.youth_facilities).unwrap_or(10);
                let yr = club.map(|c| c.youth_recruitment).unwrap_or(10);
                let jc = club.map(|c| c.junior_coaching).unwrap_or(10);
                let sd = club.map(|c| c.sugar_daddy_type.as_str()).unwrap_or("None");

                out.push_str(&format!(
                    "║ Club: {bold}{gold}{:<18}{reset} | Sugar Daddy: {bold}{cyan}{:<12}{reset}                     ║\n",
                    club_name, sd
                ));
                out.push_str(&format!(
                    "║ Facilities: Training: {:<2}/20 | Youth: {:<2}/20 | Recruitment: {:<2}/20 | Coaching: {:<2}/20║\n",
                    tf, yf, yr, jc
                ));
                out.push_str(&format!(
                    "{bold}{cyan}╠══════════════════════════════════════════════════════════════════════════════╣{reset}\n"
                ));
                out.push_str("║ Club Facilities & Board Quick Actions:                                       ║\n");
                out.push_str(&format!(
                    "║   {bold}[C]{reset} 🏗️ max_facilities        (Set Training, Youth & Coaching to 20/20)    ║\n"
                ));
                out.push_str(&format!(
                    "║   {bold}[F]{reset} 💎 foreground_sugar_daddy (Assign Foreground Tycoon Owner + $500M)    ║\n"
                ));
                out.push_str(&format!(
                    "║   {bold}[S]{reset} 💰 sugar_daddy            (Inject +$100M Transfer Budget)             ║\n"
                ));
            }
            OverlayTab::Affiliates => {
                out.push_str("║ [Tab: Club Affiliate Network]                                                ║\n");
                let affs = if let Some(ref c) = state.active_club {
                    &c.affiliates
                } else {
                    &state.active_affiliates
                };
                out.push_str(&format!(
                    "║ Total Active Affiliates: {bold}{gold}{:<3}{reset}                                                 ║\n",
                    affs.len()
                ));
                if affs.is_empty() {
                    out.push_str("║   (No affiliate clubs linked yet. Use 'affiliates add <id> <name>')          ║\n");
                } else {
                    for aff in affs.iter().take(2) {
                        let line = format!("• {} (ID: {}) - Loan & Shared Scouting", aff.affiliate_name, aff.affiliate_club_id);
                        out.push_str(&format!("║   {:<75}║\n", if line.len() > 75 { &line[..75] } else { &line }));
                    }
                }
                out.push_str(&format!(
                    "{bold}{cyan}╠══════════════════════════════════════════════════════════════════════════════╣{reset}\n"
                ));
                out.push_str("║ Affiliates Quick Actions:                                                    ║\n");
                out.push_str(&format!(
                    "║   {bold}[+]{reset} Add Affiliate Link (Command: affiliates add <id> <name>)                 ║\n"
                ));
            }
            _ => {
                out.push_str("║ Quick Actions / HUD Hotkeys:                                                 ║\n");
                out.push_str(&format!(
                    "║   {bold}[H]{reset} ⚡ Heal Squad (100% Condition & Sharpness)                               ║\n"
                ));
                out.push_str(&format!(
                    "║   {bold}[M]{reset} 🔥 Max Squad Morale (Superb 100)                                         ║\n"
                ));
                out.push_str(&format!(
                    "║   {bold}[F]{reset} 🛡️ Clear Fatigue & Suspensions                                          ║\n"
                ));
                out.push_str(&format!(
                    "║   {bold}[S]{reset} 💰 +$100M Sugar Daddy Transfer Budget                                   ║\n"
                ));
                out.push_str(&format!(
                    "║   {bold}[C]{reset} 🏗️ Max Club Facilities (20/20 Training, Youth, Coaching)               ║\n"
                ));
                out.push_str(&format!(
                    "║   {bold}[G]{reset} 🌟 Golden Generation (Nation Youth Rating 200)                           ║\n"
                ));
                out.push_str(&format!(
                    "║   {bold}[R]{reset} 🔓 Relax Competition Rules (Foreign Cap 99, Squad 50, 12 Bench)         ║\n"
                ));
                out.push_str(&format!(
                    "║   {bold}[D]{reset} 🔓 Unlock QA / Developer Debug Menu                                    ║\n"
                ));
                out.push_str(&format!(
                    "║   {bold}[T]{reset} 🖥️ Switch Full / Compact Overlay HUD Mode                               ║\n"
                ));
            }
        }

        // Notifications
        if !state.notifications.is_empty() {
            out.push_str(&format!(
                "{bold}{cyan}╠══════════════════════════════════════════════════════════════════════════════╣{reset}\n"
            ));
            out.push_str("║ Recent HUD Events / Notifications:                                           ║\n");
            for notif in state.notifications.iter().rev().take(3) {
                let badge = match notif.level {
                    NotificationLevel::Info => format!("{cyan}[INFO]{reset}"),
                    NotificationLevel::Success => format!("{emerald}[OK]{reset}  "),
                    NotificationLevel::Warning => format!("{gold}[WARN]{reset}"),
                    NotificationLevel::Error => format!("{red}[ERR]{reset} "),
                };
                let line = format!("{} {}: {}", badge, notif.title, notif.message);
                let truncated = if line.len() > 74 {
                    format!("{}...", &line[..71])
                } else {
                    line
                };
                out.push_str(&format!("║   {:<75}║\n", truncated));
            }
        }

        out.push_str(&format!(
            "{bold}{cyan}╚══════════════════════════════════════════════════════════════════════════════╝{reset}\n"
        ));

        out
    }
}

/// Interactive Terminal HUD Controller for terminal overlay sessions
pub struct TerminalHudController {
    pub hud: HudController,
}

impl Default for TerminalHudController {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalHudController {
    pub fn new() -> Self {
        Self {
            hud: HudController::new(),
        }
    }

    pub fn from_state(state: OverlayState) -> Self {
        Self {
            hud: HudController { state },
        }
    }

    pub fn state(&self) -> &OverlayState {
        &self.hud.state
    }

    pub fn state_mut(&mut self) -> &mut OverlayState {
        &mut self.hud.state
    }

    pub fn handle_command(&mut self, cmd: HudCommand) -> CommandResult {
        self.hud.handle_command(cmd)
    }

    pub fn execute_command(&mut self, input: &str) -> CommandResult {
        self.hud.execute_command(input)
    }

    pub fn render(&self) -> String {
        TerminalHud::render(&self.hud.state)
    }
}

impl std::ops::Deref for TerminalHudController {
    type Target = HudController;
    fn deref(&self) -> &Self::Target {
        &self.hud
    }
}

impl std::ops::DerefMut for TerminalHudController {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.hud
    }
}
