use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use fmlev_mem::{find_fm_process, find_module_bases};

pub const EMBEDDED_UI_HTML: &str = include_str!("ui.html");
pub const DEFAULT_OFFSETS_JSON: &str = include_str!("../../../offsets/v26.0.0.json");

pub mod routes;
pub mod state;

#[cfg(test)]
mod tests;

pub use routes::*;
pub use state::*;

pub async fn start_server(port: u16) -> anyhow::Result<()> {
    let state = ServerState {
        inner: Arc::new(RwLock::new(AppState::default())),
    };

    // Try auto-attaching on startup in background
    let initial_state = state.clone();
    tokio::spawn(async move {
        if let Ok(proc) = find_fm_process() {
            let pid = proc.pid;
            let bases = find_module_bases(pid).unwrap_or(fmlev_mem::ModuleBases {
                game_assembly_base: None,
                unity_player_base: None,
                fm_exe_base: None,
                game_plugin_base: None,
            });
            let mut w = initial_state.inner.write().await;
            w.pid = Some(pid);
            w.exe_path = Some(proc.exe_path.to_string_lossy().to_string());
            w.is_proton = proc.is_proton;
            w.wine_prefix = proc.wine_prefix.map(|p| p.to_string_lossy().to_string());
            w.game_assembly_base = bases.game_assembly_base;
            w.unity_player_base = bases.unity_player_base;
            w.fm_exe_base = bases.fm_exe_base;
            w.is_attached = true;
            let _ = w.context_mgr.try_attach();
            info!("Auto-attached to FM26 process (PID: {}) on server start", pid);
        }
    });

    let app = create_router(state);
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));

    info!("Starting fmlev Live Editor Server at http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
