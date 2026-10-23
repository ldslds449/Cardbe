//! Embedded MCP server. Disabled by default; see `docs/mcp-server.md`.
pub mod config;
pub mod server;

pub use server::McpServer;
use tauri::Manager;

/// Loads the authorized clients and starts the server when the user enabled
/// automatic start. Failures leave the rest of Cardbe running.
pub fn restore(app: &tauri::AppHandle) {
    let state = app.state::<crate::state::SharedAppData>();
    let loaded = crate::state::lock_app(&state)
        .map_err(crate::errors::DomainError::from)
        .and_then(|guard| config::load(&guard.database.connection));
    let config = match loaded {
        Ok(config) => config,
        Err(error) => {
            log::warn!(target: "mcp", "Could not load MCP server settings: {error}");
            return;
        }
    };
    let server = app.state::<McpServer>();
    server.set_clients(&config.clients);
    if config.enabled && config.auto_start {
        if let Err(error) = server.start(config.port) {
            log::warn!(target: "mcp", "Could not start the MCP server: {error}");
        }
    }
}
