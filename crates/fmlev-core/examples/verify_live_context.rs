use fmlev_core::context::GameContextManager;
use fmlev_core::models::PlayerAttributes;

fn main() -> fmlev_core::error::Result<()> {
    let mut mgr = GameContextManager::new();
    println!("Attached: {}, PID: {:?}", mgr.context().attached, mgr.context().pid);
    mgr.sync_live_context()?;

    let ctx = mgr.get_context();
    println!(
        "Active Context -> Screen: {}, Player ID: {}, Name: '{}', Club: '{}', Nationality: '{}', Age: {}, Position: '{}', Condition: {}, Morale: {}",
        ctx.active_screen,
        ctx.player.id,
        ctx.player.name,
        ctx.player.club_name,
        ctx.player.nationality,
        ctx.player.age,
        ctx.player.position,
        ctx.player.condition,
        ctx.player.morale,
    );

    println!("\n=== Active Player in Context ===");
    println!("ID:          {}", ctx.player.id);
    println!("Name:        {}", ctx.player.name);
    println!("Nationality: {}", ctx.player.nationality);
    println!("Club:        {}", ctx.player.club_name);
    println!("Age:         {}", ctx.player.age);
    println!("Position:    {}", ctx.player.position);
    println!("Positions:   {:#?}", ctx.player.positions);
    println!("Condition:   {}", ctx.player.condition);
    println!("Morale:      {}", ctx.player.morale);
    println!("Attributes:  {:#?}", ctx.player.attributes);

    // This diagnostic is deliberately read-only. Verifying persistence requires a
    // separately validated native layout and an explicitly chosen player/edit.

    let live_players: Vec<_> = mgr
        .all_players()
        .into_iter()
        .filter(|p| p.id > 200)
        .collect();
    println!("\nTotal live bound players (>200): {}", live_players.len());
    for p in live_players.iter().take(20) {
        let has_attrs = p.attributes != PlayerAttributes::default();
        println!(
            "  [{:>6}] {:<38} | Age {:>2} | Nat {:<12} | Club {:<14} | Pos {:<22} | HasAttrs: {}",
            p.id, p.name, p.age, p.nationality, p.club_name, p.position, has_attrs
        );
    }

    Ok(())
}
