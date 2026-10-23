use crate::errors::{CommandError, DomainError};
use crate::{
    mcp::{
        config::{self, McpClient, McpConfig},
        server::{self, Activity, Status},
        McpServer,
    },
    plugins::manager::secret_namespace_for_path,
    state::{lock_app, AppData, SharedAppData},
};
use serde::Serialize;
use std::collections::BTreeSet;
use tauri::State;

#[derive(Serialize)]
pub struct McpClientView {
    id: String,
    name: String,
    all_boards: bool,
    boards: Vec<i64>,
    write: bool,
    created_at: i64,
}

#[derive(Serialize)]
pub struct McpServerView {
    enabled: bool,
    auto_start: bool,
    port: u16,
    endpoint: String,
    status: Status,
    clients: Vec<McpClientView>,
}

/// The token is returned only to the settings view that created it.
#[derive(Serialize)]
pub struct McpClientToken {
    server: McpServerView,
    client_id: String,
    token: String,
    /// False when the OS keyring is unavailable, so it cannot be shown again.
    token_saved: bool,
}

fn view(config: &McpConfig, server: &McpServer) -> McpServerView {
    let (status, running_port) = server.status();
    let port = running_port.unwrap_or(config.port);
    McpServerView {
        enabled: config.enabled,
        auto_start: config.auto_start,
        port: config.port,
        endpoint: server::endpoint(port),
        status,
        clients: config
            .clients
            .iter()
            .map(|client| McpClientView {
                id: client.id.clone(),
                name: client.name.clone(),
                all_boards: client.all_boards,
                boards: client.boards.iter().copied().collect(),
                write: client.write,
                created_at: client.created_at,
            })
            .collect(),
    }
}

/// Saves the configuration and the authorized-client snapshot together.
fn update(
    state: &SharedAppData,
    server: &McpServer,
    change: impl FnOnce(&mut McpConfig, &AppData) -> Result<(), DomainError>,
) -> Result<McpConfig, DomainError> {
    let guard = lock_app(state)?;
    let mut config = config::load(&guard.database.connection)?;
    change(&mut config, &guard)?;
    config::save(&guard.database.connection, &config)?;
    server.set_clients(&config.clients);
    Ok(config)
}

fn load(state: &SharedAppData) -> Result<McpConfig, DomainError> {
    config::load(&lock_app(state)?.database.connection)
}

fn namespace(state: &SharedAppData, client_id: &str) -> Result<String, DomainError> {
    Ok(secret_namespace_for_path(
        lock_app(state)?.database.path(),
        client_id,
    ))
}

/// Selected boards are kept only when the client is not granted every board.
fn granted_boards(
    guard: &AppData,
    all_boards: bool,
    boards: Vec<i64>,
) -> Result<BTreeSet<i64>, DomainError> {
    if all_boards {
        return Ok(BTreeSet::new());
    }
    let boards: BTreeSet<i64> = boards.into_iter().collect();
    if boards
        .iter()
        .any(|id| !guard.boards.iter().any(|board| board.id == *id))
    {
        return Err(DomainError::BoardNotFound);
    }
    Ok(boards)
}

fn client<'a>(
    config: &'a mut McpConfig,
    client_id: &str,
) -> Result<&'a mut McpClient, DomainError> {
    config
        .clients
        .iter_mut()
        .find(|client| client.id == client_id)
        .ok_or(DomainError::McpClientNotFound)
}

/// Recent tool calls, newest first; kept in memory until Cardbe restarts.
#[tauri::command]
pub fn get_mcp_activity(server: State<'_, McpServer>) -> Vec<Activity> {
    server.activity()
}

#[tauri::command]
pub fn clear_mcp_activity(server: State<'_, McpServer>) {
    server.clear_activity();
}

#[tauri::command]
pub fn get_mcp_server(
    state: State<'_, SharedAppData>,
    server: State<'_, McpServer>,
) -> Result<McpServerView, CommandError> {
    Ok(view(&load(&state)?, &server))
}

/// Turning the server on starts it now; `auto_start` also starts it at launch.
#[tauri::command]
pub async fn set_mcp_server(
    state: State<'_, SharedAppData>,
    server: State<'_, McpServer>,
    enabled: bool,
    auto_start: bool,
    port: u16,
) -> Result<McpServerView, CommandError> {
    if !config::valid_port(port) {
        return Err(CommandError::InvalidArgument);
    }
    let previous = load(&state)?;
    let config = update(&state, &server, |config, _| {
        config.enabled = enabled;
        config.auto_start = auto_start;
        config.port = port;
        Ok(())
    })?;
    let (status, running_port) = server.status();
    let restart = status == Status::Failed || running_port.is_some_and(|current| current != port);
    if !enabled || restart {
        server.stop().await;
    }
    if enabled && (restart || !previous.enabled) {
        server.start(port)?;
    }
    Ok(view(&config, &server))
}

/// Retries an enabled server that is stopped or failed.
#[tauri::command]
pub async fn start_mcp_server(
    state: State<'_, SharedAppData>,
    server: State<'_, McpServer>,
) -> Result<McpServerView, CommandError> {
    let config = load(&state)?;
    if !config.enabled {
        return Err(CommandError::InvalidArgument);
    }
    if server.status().0 == Status::Failed {
        server.stop().await;
    }
    server.start(config.port)?;
    Ok(view(&config, &server))
}

#[tauri::command]
pub fn create_mcp_client(
    state: State<'_, SharedAppData>,
    server: State<'_, McpServer>,
    name: String,
    all_boards: bool,
    boards: Vec<i64>,
    write: bool,
) -> Result<McpClientToken, CommandError> {
    let name = config::client_name(&name)?;
    let token = config::generate_token();
    let client_id = crate::plugins::types::id();
    let config = update(&state, &server, |config, guard| {
        if config.clients.len() >= config::MAX_CLIENTS {
            return Err(DomainError::InvalidArgument);
        }
        config.clients.push(McpClient {
            id: client_id.clone(),
            name,
            token_hash: config::token_hash(&token),
            all_boards,
            boards: granted_boards(guard, all_boards, boards)?,
            write,
            created_at: crate::plugins::types::now(),
        });
        Ok(())
    })?;
    let token_saved = config::store_token(&namespace(&state, &client_id)?, &client_id, &token);
    Ok(McpClientToken {
        server: view(&config, &server),
        client_id,
        token,
        token_saved,
    })
}

#[tauri::command]
pub fn update_mcp_client(
    state: State<'_, SharedAppData>,
    server: State<'_, McpServer>,
    client_id: String,
    name: String,
    all_boards: bool,
    boards: Vec<i64>,
    write: bool,
) -> Result<McpServerView, CommandError> {
    let name = config::client_name(&name)?;
    let config = update(&state, &server, |config, guard| {
        let boards = granted_boards(guard, all_boards, boards)?;
        let client = client(config, &client_id)?;
        client.name = name;
        client.all_boards = all_boards;
        client.boards = boards;
        client.write = write;
        Ok(())
    })?;
    Ok(view(&config, &server))
}

/// The previous token stops working immediately.
#[tauri::command]
pub fn regenerate_mcp_client_token(
    state: State<'_, SharedAppData>,
    server: State<'_, McpServer>,
    client_id: String,
) -> Result<McpClientToken, CommandError> {
    let token = config::generate_token();
    let config = update(&state, &server, |config, _| {
        client(config, &client_id)?.token_hash = config::token_hash(&token);
        Ok(())
    })?;
    let token_saved = config::store_token(&namespace(&state, &client_id)?, &client_id, &token);
    Ok(McpClientToken {
        server: view(&config, &server),
        client_id,
        token,
        token_saved,
    })
}

/// `None` when the keyring is unavailable or no longer holds the token.
#[tauri::command]
pub fn get_mcp_client_token(
    state: State<'_, SharedAppData>,
    client_id: String,
) -> Result<Option<String>, CommandError> {
    let config = load(&state)?;
    let client = config
        .clients
        .iter()
        .find(|client| client.id == client_id)
        .ok_or(DomainError::McpClientNotFound)?;
    let token = config::read_token(&namespace(&state, &client_id)?, &client_id);
    // A stale keyring entry from an older token is never shown.
    Ok(token.filter(|token| config::token_hash(token) == client.token_hash))
}

#[tauri::command]
pub fn delete_mcp_client(
    state: State<'_, SharedAppData>,
    server: State<'_, McpServer>,
    client_id: String,
) -> Result<McpServerView, CommandError> {
    let config = update(&state, &server, |config, _| {
        client(config, &client_id)?;
        config.clients.retain(|client| client.id != client_id);
        Ok(())
    })?;
    config::delete_token(&namespace(&state, &client_id)?, &client_id);
    Ok(view(&config, &server))
}
