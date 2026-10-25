use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use fmlev_core::{ClubAffiliate, GameContextManager, Player};

#[derive(Clone)]
pub struct ServerState {
    pub inner: Arc<RwLock<AppState>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanMatch {
    pub address: String,
    pub address_val: usize,
    pub value: String,
}

pub struct AppState {
    pub pid: Option<i32>,
    pub exe_path: Option<String>,
    pub is_proton: bool,
    pub wine_prefix: Option<String>,
    pub game_assembly_base: Option<usize>,
    pub unity_player_base: Option<usize>,
    pub fm_exe_base: Option<usize>,
    pub is_attached: bool,
    pub scan_results: Vec<ScanMatch>,
    pub last_scan_type: String,

    pub context_mgr: GameContextManager,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            pid: None,
            exe_path: None,
            is_proton: false,
            wine_prefix: None,
            game_assembly_base: None,
            unity_player_base: None,
            fm_exe_base: None,
            is_attached: false,
            scan_results: Vec::new(),
            last_scan_type: String::new(),
            context_mgr: GameContextManager::new(),
        }
    }
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub attached: bool,
    pub pid: Option<i32>,
    pub exe_path: Option<String>,
    pub is_proton: bool,
    pub wine_prefix: Option<String>,
    pub game_assembly_base: Option<String>,
    pub unity_player_base: Option<String>,
    pub fm_exe_base: Option<String>,
    pub yama_diagnostics: String,
}

#[derive(Deserialize)]
pub struct AttachRequest {
    pub pid: Option<i32>,
}

#[derive(Deserialize)]
pub struct MemReadRequest {
    pub address: String,
    #[serde(default = "default_type")]
    pub r#type: String,
    #[serde(default = "default_size")]
    pub size: usize,
}

fn default_type() -> String {
    "bytes".to_string()
}

fn default_size() -> usize {
    64
}

#[derive(Deserialize)]
pub struct MemWriteRequest {
    pub address: String,
    pub r#type: String,
    pub value: serde_json::Value,
}

#[derive(Deserialize)]
pub struct HealRequest {
    #[serde(default)]
    pub player_ptr: usize,
    #[serde(default = "default_condition_offset")]
    pub condition_offset: usize,
    #[serde(default)]
    pub heal_squad: bool,
}

fn default_condition_offset() -> usize {
    0x20
}

#[derive(Deserialize)]
pub struct FinanceRequest {
    #[serde(default = "default_club_ptr")]
    pub club_ptr: usize,
    pub balance: i64,
    pub transfer_budget: i64,
    pub wage_budget: i32,
    #[serde(default = "default_balance_offset")]
    pub balance_offset: usize,
    #[serde(default = "default_transfer_offset")]
    pub transfer_offset: usize,
    #[serde(default = "default_wage_offset")]
    pub wage_offset: usize,
}

fn default_club_ptr() -> usize {
    0
}

fn default_balance_offset() -> usize {
    0x30
}

fn default_transfer_offset() -> usize {
    0x38
}

fn default_wage_offset() -> usize {
    0x40
}

#[derive(Deserialize)]
pub struct GodModeRequest {
    pub action: String,
    #[serde(default)]
    pub player_ptr: Option<usize>,
}

#[derive(Deserialize)]
pub struct MemScanRequest {
    pub value: String,
    #[serde(default = "default_scan_type")]
    pub r#type: String,
}

fn default_scan_type() -> String {
    "i32".to_string()
}

#[derive(Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: String,
}

/// `number` 1..=99, or null/0 to clear the squad number.
#[derive(Deserialize)]
pub struct SquadNumberRequest {
    pub player_id: i32,
    pub number: Option<u8>,
}

/// `first_prize`/`last_prize`/`teams` identify the table by what the Rules page shows.
/// Omit `prizes` to read; provide it to write one raw-GBP amount per position.
#[derive(Deserialize)]
pub struct PrizeRequest {
    pub teams: usize,
    pub first_prize: i32,
    pub last_prize: i32,
    /// Raw-GBP slack for rounded displays (default 0 = exact).
    #[serde(default)]
    pub tolerance: i32,
    pub prizes: Option<Vec<i32>>,
}

#[derive(Deserialize)]
pub struct ContextSelectRequest {
    pub player_id: Option<i32>,
    pub club_id: Option<i32>,
    pub nation_id: Option<i32>,
    pub competition_id: Option<i32>,
    pub unpin: Option<bool>,
}

#[derive(Deserialize)]
pub struct PlayerUpdateRequest {
    #[serde(flatten)]
    pub player: Player,
    #[serde(default)]
    pub changed_fields: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct PlayerPresetRequest {
    pub preset: String,
    pub player_id: Option<i32>,
}

#[derive(Deserialize)]
pub struct ClubPresetRequest {
    pub preset: String,
    pub club_id: Option<i32>,
}

#[derive(Deserialize)]
pub struct NationPresetRequest {
    pub preset: String,
    pub nation_id: Option<i32>,
}

#[derive(Deserialize)]
pub struct CompetitionPresetRequest {
    pub preset: String,
    pub competition_id: Option<i32>,
}

#[derive(Deserialize)]
pub struct AffiliateActionRequest {
    pub club_id: i32,
    pub affiliate: Option<ClubAffiliate>,
    pub affiliate_id: Option<i32>,
}
