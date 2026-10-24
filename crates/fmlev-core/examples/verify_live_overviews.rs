//! Read-only diagnostic for identity-validated club, nation and competition UI data.
use fmlev_core::GameContextManager;

fn main() -> fmlev_core::Result<()> {
    let mut manager = GameContextManager::new();
    manager.sync_live_context()?;
    println!("pid={:?} screen={} selected-club={}:{} selected-nation={}:{} selected-competition={}:{}", manager.context.pid, manager.context.active_screen, manager.context.club.club_id, manager.context.club.club_name, manager.context.nation.id, manager.context.nation.name, manager.context.competition.id, manager.context.competition.name);
    println!("clubs={}", serde_json::to_string_pretty(&manager.live_clubs).unwrap());
    println!("nations={}", serde_json::to_string_pretty(&manager.live_nations).unwrap());
    println!("competitions={}", serde_json::to_string_pretty(&manager.live_competitions).unwrap());
    Ok(())
}
