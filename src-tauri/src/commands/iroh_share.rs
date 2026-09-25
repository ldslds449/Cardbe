//! Owner-authoritative Iroh snapshot protocol.  The invitation secret is only
//! present in the copied ticket; it is never placed in a normal backup.
use crate::{
    models::{Board, StoredData},
    state::SharedAppData,
    storage::Database,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use iroh::{endpoint::presets, Endpoint, EndpointAddr, SecretKey, TransportAddr};
use qrcode::{render::svg, QrCode};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, State};

const ALPN: &[u8] = b"cardbe-board/1";
const MAX_MESSAGE: usize = 16 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_HOST_CONNECTIONS: usize = 16;

#[derive(Clone, Serialize, Deserialize)]
struct Ticket {
    version: u8,
    endpoint: EndpointAddr,
    invite_id: String,
    secret: String,
    permission: String,
}
#[derive(Serialize, Deserialize)]
struct Request {
    invite_id: String,
    secret: String,
    #[serde(default = "default_pull")]
    action: String,
    #[serde(default)]
    loro_state_vector: Vec<u8>,
    #[serde(default)]
    loro_update: Vec<u8>,
    #[serde(default)]
    known_revision: Option<i64>,
    #[serde(default)]
    known_permission: Option<String>,
}
fn default_pull() -> String {
    "pull".into()
}

fn local_edits_at_risk(status: &str, changed_during_sync: bool) -> bool {
    changed_during_sync || status == "pending"
}

fn needs_editor_upload(role: &str, status: &str) -> bool {
    role == "editor" && status == "pending"
}
#[derive(Serialize, Deserialize)]
struct Snapshot {
    ok: bool,
    error: Option<String>,
    name: String,
    permission: String,
    revision: i64,
    data: StoredData,
    #[serde(default)]
    loro_update: Vec<u8>,
    #[serde(default)]
    unchanged: bool,
}
#[derive(Clone, Serialize)]
struct RemotePushApplied {
    board_id: i64,
    revision: i64,
}
#[derive(Clone)]
struct HostedInvite {
    permission: String,
    board_id: i64,
    enabled: bool,
}
#[derive(Clone, Serialize)]
pub struct IrohInviteSummary {
    invite_id: String,
    board_id: i64,
    board_name: String,
    permission: String,
    enabled: bool,
    created_at: String,
}

#[derive(Clone, Serialize)]
pub struct IrohInviteAccess {
    ticket: String,
    qr_svg: String,
}
struct Hosted {
    endpoint: Endpoint,
    path: std::path::PathBuf,
}
#[derive(Default)]
pub struct IrohShareState {
    hosted: Mutex<Option<Hosted>>,
    init: tokio::sync::Mutex<()>,
    last_error: Mutex<Option<String>>,
}

fn random_token() -> String {
    URL_SAFE_NO_PAD.encode(SecretKey::generate().to_bytes())
}

fn parse_ticket(ticket: &str) -> Result<Ticket, String> {
    let encoded = ticket_payload(ticket).ok_or("Not a shared-board invitation")?;
    let ticket: Ticket = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| "Invalid invitation")?,
    )
    .map_err(|_| "Invalid invitation")?;
    if ticket.version != 1
        || !matches!(ticket.permission.as_str(), "viewer" | "editor")
        || ticket.invite_id.is_empty()
        || ticket.secret.is_empty()
    {
        return Err("Unsupported shared-board invitation".into());
    }
    Ok(ticket)
}

fn ensure_invite_not_joined(db: &Database, ticket: &Ticket) -> Result<(), String> {
    let already_joined = db
        .iroh_remote_tickets()
        .map_err(|e| e.to_string())?
        .iter()
        .filter_map(|saved| parse_ticket(saved).ok())
        .any(|saved| {
            saved.endpoint.id == ticket.endpoint.id && saved.invite_id == ticket.invite_id
        });
    if already_joined {
        return Err("This invitation has already been joined".into());
    }
    Ok(())
}

fn ticket_payload(ticket: &str) -> Option<&str> {
    let ticket = ticket.trim();
    ticket
        .strip_prefix("cardbe://share/")
        // Old invitations remain usable after upgrading, but new links never
        // expose the transport implementation in their text.
        .or_else(|| ticket.strip_prefix("cardbe+iroh://"))
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_board_owned, local_edits_at_risk, needs_editor_upload, ticket_payload,
        viewer_is_current, Request,
    };
    use crate::models::Board;

    #[test]
    fn new_and_legacy_invitation_prefixes_are_accepted() {
        assert_eq!(ticket_payload("cardbe://share/payload"), Some("payload"));
        assert_eq!(ticket_payload(" cardbe+iroh://payload "), Some("payload"));
        assert_eq!(ticket_payload("https://example.test/payload"), None);
    }

    #[test]
    fn received_boards_cannot_create_an_invitation() {
        let viewer = Board {
            id: 7,
            name: "Received".into(),
            task_count: 0,
            shared_role: "viewer".into(),
            sync_status: "synced".into(),
            sync_revision: 1,
        };
        assert_eq!(ensure_board_owned(None), Err("Board not found".into()));
        assert_eq!(
            ensure_board_owned(Some(&viewer)),
            Err("Only boards you own can be shared".into())
        );
    }

    #[test]
    fn permission_downgrade_only_conflicts_with_unsent_edits() {
        assert!(!local_edits_at_risk("synced", false));
        assert!(local_edits_at_risk("pending", false));
        assert!(local_edits_at_risk("synced", true));
    }

    #[test]
    fn clean_editor_only_sends_its_version_vector() {
        assert!(!needs_editor_upload("editor", "synced"));
        assert!(needs_editor_upload("editor", "pending"));
        assert!(!needs_editor_upload("viewer", "pending"));
    }

    #[test]
    fn viewer_skips_snapshot_only_for_the_same_revision_and_permission() {
        let mut request = Request {
            invite_id: "invite".into(),
            secret: "secret".into(),
            action: "pull".into(),
            loro_state_vector: vec![],
            loro_update: vec![],
            known_revision: Some(4),
            known_permission: Some("viewer".into()),
        };
        assert!(viewer_is_current(&request, "viewer", 4));
        assert!(!viewer_is_current(&request, "viewer", 5));
        assert!(!viewer_is_current(&request, "editor", 4));
        request.known_permission = Some("editor".into());
        assert!(!viewer_is_current(&request, "viewer", 4));
        request.known_permission = Some("viewer".into());
        request.known_revision = None;
        assert!(!viewer_is_current(&request, "viewer", 4));
    }
}

fn viewer_is_current(request: &Request, permission: &str, revision: i64) -> bool {
    permission == "viewer"
        && request.known_permission.as_deref() == Some("viewer")
        && request.known_revision == Some(revision)
}

// Persist and transmit an endpoint identity plus relay route, never a peer's
// observed LAN/WAN IP address. The stable endpoint key is stored locally by
// Database::iroh_endpoint_seed; Iroh/N0 resolves fresh paths after restarts.
fn share_address(endpoint: &Endpoint) -> EndpointAddr {
    let address = endpoint.addr();
    EndpointAddr::from_parts(
        address.id,
        address.relay_urls().cloned().map(TransportAddr::Relay),
    )
}

fn ensure_board_owned(board: Option<&Board>) -> Result<(), String> {
    let board = board.ok_or("Board not found")?;
    if board.shared_role != "owner" {
        return Err("Only boards you own can be shared".into());
    }
    Ok(())
}

fn snapshot_from_db(
    app_handle: &AppHandle,
    path: &std::path::Path,
    board_id: i64,
    error: Option<String>,
    request: &Request,
) -> Result<Snapshot, String> {
    let app_state = app_handle.state::<SharedAppData>();
    let _state = app_state
        .lock()
        .map_err(|_| "Application state lock is poisoned")?;
    let mut db = Database::open(path.to_path_buf()).map_err(|e| e.to_string())?;
    // Recheck after taking the app lock: the owner may revoke or change this
    // invitation while a request is waiting for the board snapshot.
    let permission = db
        .iroh_invites()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|(id, id_board, secret, _, enabled)| {
            id == &request.invite_id
                && *id_board == board_id
                && secret == &request.secret
                && *enabled
        })
        .map(|(_, _, _, permission, _)| permission)
        .ok_or("Invitation was revoked or disabled")?;
    let board = db
        .boards()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|board| board.id == board_id && board.shared_role == "owner")
        .ok_or("Board was deleted")?;
    if error.is_none() && viewer_is_current(request, &permission, board.sync_revision) {
        return Ok(Snapshot {
            ok: true,
            error: None,
            name: board.name,
            permission,
            revision: board.sync_revision,
            data: StoredData::default(),
            loro_update: Vec::new(),
            unchanged: true,
        });
    }
    if request.action == "loro_sync" && error.is_none() {
        return Ok(Snapshot {
            ok: true,
            error: None,
            name: board.name,
            permission,
            revision: board.sync_revision,
            data: StoredData::default(),
            loro_update: Vec::new(),
            unchanged: false,
        });
    }
    let loro_update = if permission == "editor" {
        let loro_update = db
            .ensure_iroh_loro_doc(board_id)
            .map_err(|e| e.to_string())?;
        crate::loro_board::share_snapshot(&loro_update)?
    } else {
        Vec::new()
    };
    let (board, mut data) = db
        .read_board_share_snapshot(board_id)
        .map_err(|e| e.to_string())?;
    // Settings belong to the installation, not to the shared board.
    data.settings = Default::default();
    Ok(Snapshot {
        ok: error.is_none(),
        error,
        name: board.name,
        permission,
        revision: board.sync_revision,
        data,
        loro_update,
        unchanged: false,
    })
}

async fn ensure_host(
    state: &IrohShareState,
    path: std::path::PathBuf,
    app_handle: AppHandle,
) -> Result<Endpoint, String> {
    let _init = state.init.lock().await;
    if let Some(hosted) = state
        .hosted
        .lock()
        .map_err(|_| "Sharing service state is unavailable")?
        .as_ref()
    {
        return Ok(hosted.endpoint.clone());
    }
    let seed = {
        let mut db = Database::open(path.clone()).map_err(|e| e.to_string())?;
        db.iroh_endpoint_seed().map_err(|e| e.to_string())?
    };
    use sha2::{Digest, Sha256};
    let key_bytes: [u8; 32] = Sha256::digest(seed.as_bytes()).into();
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(SecretKey::from_bytes(&key_bytes))
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .map_err(|e| e.to_string())?;
    tokio::time::timeout(CONNECT_TIMEOUT, endpoint.online())
        .await
        .map_err(|_| {
            "The sharing service could not get online within 20 seconds. Check this computer's internet connection and try again."
                .to_string()
        })?;
    let accept_endpoint = endpoint.clone();
    let hosted_path = path.clone();
    let connection_slots = Arc::new(tokio::sync::Semaphore::new(MAX_HOST_CONNECTIONS));
    tauri::async_runtime::spawn(async move {
        while let Some(incoming) = accept_endpoint.accept().await {
            let Ok(slot) = connection_slots.clone().try_acquire_owned() else {
                continue;
            };
            let Ok(connecting) = incoming.accept() else {
                continue;
            };
            let Ok(Ok(connection)) = tokio::time::timeout(CONNECT_TIMEOUT, connecting).await else {
                continue;
            };
            let path = path.clone();
            let app_handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                let _slot = slot;
                let Ok(Ok((mut send, mut recv))) =
                    tokio::time::timeout(CONNECT_TIMEOUT, connection.accept_bi()).await
                else {
                    return;
                };
                let Ok(Ok(bytes)) =
                    tokio::time::timeout(TRANSFER_TIMEOUT, recv.read_to_end(MAX_MESSAGE)).await
                else {
                    return;
                };
                let mut applied_push = None;
                let response = match serde_json::from_slice::<Request>(&bytes) {
                    Ok(request) => match Database::open(path.clone())
                        .and_then(|db| db.iroh_invites())
                        .map_err(|e| e.to_string())
                        .and_then(|saved| {
                            saved
                                .into_iter()
                                .find(|(id, _, secret, _, _)| {
                                    id == &request.invite_id && secret == &request.secret
                                })
                                .map(|(_, board_id, _, permission, enabled)| HostedInvite {
                                    permission,
                                    board_id,
                                    enabled,
                                })
                                .ok_or_else(|| "Invitation was revoked".to_string())
                        }) {
                        Err(error) => Err(error),
                        Ok(invite) if !invite.enabled => Err("Invitation is disabled".into()),
                        Ok(invite) if request.action == "pull" => {
                            snapshot_from_db(&app_handle, &path, invite.board_id, None, &request)
                        }
                        Ok(invite)
                            if request.action == "loro_sync" && invite.permission == "editor" =>
                        {
                            (|| -> Result<Snapshot, String> {
                                let app_state = app_handle.state::<SharedAppData>();
                                let mut state = app_state
                                    .lock()
                                    .map_err(|_| "Application state lock is poisoned")?;
                                let mut db =
                                    Database::open(path.clone()).map_err(|e| e.to_string())?;
                                let (changed, _, _) = db
                                    .apply_iroh_loro_update(
                                        invite.board_id,
                                        &request.invite_id,
                                        &request.secret,
                                        &request.loro_update,
                                    )
                                    .map_err(|e| e.to_string())?;
                                let owner_update = db
                                    .iroh_loro_update(invite.board_id)
                                    .map_err(|e| e.to_string())?
                                    .unwrap_or_default();
                                let response_update = crate::loro_board::diff(
                                    Some(&owner_update),
                                    &request.loro_state_vector,
                                )
                                .map_err(|e| e.to_string())?;
                                let fresh = db
                                    .boards()
                                    .map_err(|e| e.to_string())?
                                    .into_iter()
                                    .find(|b| b.id == invite.board_id)
                                    .ok_or("Board was deleted")?;
                                if changed {
                                    if let Some(board) =
                                        state.boards.iter_mut().find(|b| b.id == invite.board_id)
                                    {
                                        *board = fresh.clone();
                                    }
                                    if state.active_board_id == invite.board_id {
                                        state.stored = state
                                            .database
                                            .load_board(invite.board_id)
                                            .map_err(|e| e.to_string())?;
                                        state.refresh_labels();
                                        state.undo_history.clear();
                                    }
                                    applied_push = Some((invite.board_id, fresh.sync_revision));
                                }
                                drop(state);
                                let mut snapshot = snapshot_from_db(
                                    &app_handle,
                                    &path,
                                    invite.board_id,
                                    None,
                                    &request,
                                )?;
                                snapshot.loro_update = response_update;
                                Ok(snapshot)
                            })()
                        }
                        Ok(invite) => snapshot_from_db(
                            &app_handle,
                            &path,
                            invite.board_id,
                            Some("This invitation is read-only".into()),
                            &request,
                        ),
                    },
                    Err(_) => Err("Invalid request".into()),
                }
                .unwrap_or_else(|error| Snapshot {
                    ok: false,
                    error: Some(error),
                    name: String::new(),
                    permission: String::new(),
                    revision: 0,
                    data: StoredData::default(),
                    loro_update: Vec::new(),
                    unchanged: false,
                });
                if let Some((board_id, revision)) = applied_push {
                    let _ = app_handle.emit(
                        "cardbe:iroh-remote-push",
                        RemotePushApplied { board_id, revision },
                    );
                }
                let bytes = match serde_json::to_vec(&response) {
                    Ok(bytes) if bytes.len() <= MAX_MESSAGE => bytes,
                    _ => serde_json::to_vec(&Snapshot {
                        ok: false,
                        error: Some("Shared board exceeds the 16 MiB transfer limit".into()),
                        name: String::new(),
                        permission: String::new(),
                        revision: 0,
                        data: StoredData::default(),
                        loro_update: Vec::new(),
                        unchanged: false,
                    })
                    .unwrap_or_default(),
                };
                if !bytes.is_empty() {
                    match tokio::time::timeout(TRANSFER_TIMEOUT, send.write_all(&bytes)).await {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => {
                            eprintln!("Shared-board response write failed: {error}");
                            return;
                        }
                        Err(_) => {
                            eprintln!("Shared-board response write timed out");
                            return;
                        }
                    }
                }
                if let Err(error) = send.finish() {
                    eprintln!("Shared-board response finish failed: {error}");
                    return;
                }
                // Keep the connection alive until the peer acknowledges the
                // complete response. Dropping the final connection handle
                // immediately after `finish` can otherwise surface as
                // `connection lost` on slower relay paths.
                if tokio::time::timeout(CONNECT_TIMEOUT, send.stopped())
                    .await
                    .is_err()
                {
                    eprintln!("Shared-board response acknowledgement timed out");
                }
            });
        }
    });
    *state
        .hosted
        .lock()
        .map_err(|_| "Sharing service state is unavailable")? = Some(Hosted {
        endpoint: endpoint.clone(),
        path: hosted_path,
    });
    if let Ok(mut error) = state.last_error.lock() {
        *error = None;
    }
    Ok(endpoint)
}

/// Restore the owner endpoint after launch so existing capability links keep
/// working after restart.  The database contains no rows for a new install.
pub async fn restore_iroh_host(
    network: &IrohShareState,
    path: std::path::PathBuf,
    app_handle: AppHandle,
) {
    let has_invites = Database::open(path.clone())
        .and_then(|db| db.iroh_invites())
        .is_ok_and(|invites| invites.iter().any(|invite| invite.4));
    if has_invites {
        if let Err(error) = ensure_host(network, path, app_handle).await {
            if let Ok(mut last_error) = network.last_error.lock() {
                *last_error = Some(error);
            }
        }
    }
}

#[tauri::command]
pub fn iroh_host_error(network: State<'_, IrohShareState>) -> Option<String> {
    network
        .last_error
        .lock()
        .ok()
        .and_then(|error| error.clone())
}

#[tauri::command]
pub async fn ensure_iroh_host(
    app: State<'_, SharedAppData>,
    network: State<'_, IrohShareState>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let (path, active) = {
        let guard = app
            .lock()
            .map_err(|_| "Application state lock is poisoned")?;
        (
            guard.database.path().to_path_buf(),
            guard
                .database
                .iroh_invites()
                .map_err(|e| e.to_string())?
                .iter()
                .any(|invite| invite.4),
        )
    };
    if active {
        if let Err(error) = ensure_host(&network, path, app_handle).await {
            if let Ok(mut last_error) = network.last_error.lock() {
                *last_error = Some(error.clone());
            }
            return Err(error);
        }
    } else {
        stop_host_if_idle(&network).await?;
    }
    Ok(())
}

pub(crate) async fn stop_host_if_idle(network: &IrohShareState) -> Result<(), String> {
    let _init = network.init.lock().await;
    let path = network
        .hosted
        .lock()
        .map_err(|_| "Sharing service state is unavailable")?
        .as_ref()
        .map(|hosted| hosted.path.clone());
    if let Some(path) = path {
        let active = Database::open(path)
            .and_then(|db| db.iroh_invites())
            .map_err(|e| e.to_string())?
            .iter()
            .any(|invite| invite.4);
        if !active {
            let hosted = network
                .hosted
                .lock()
                .map_err(|_| "Sharing service state is unavailable")?
                .take();
            if let Some(hosted) = hosted {
                hosted.endpoint.close().await;
            }
            if let Ok(mut error) = network.last_error.lock() {
                *error = None;
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn create_iroh_invite(
    app: State<'_, SharedAppData>,
    network: State<'_, IrohShareState>,
    app_handle: AppHandle,
    board_id: i64,
    permission: String,
) -> Result<String, String> {
    if permission != "viewer" && permission != "editor" {
        return Err("Permission must be viewer or editor".into());
    }
    let path = {
        let guard = app
            .lock()
            .map_err(|_| "Application state lock is poisoned")?;
        ensure_board_owned(guard.boards.iter().find(|board| board.id == board_id))?;
        guard.database.path().to_path_buf()
    };
    let endpoint = ensure_host(&network, path, app_handle).await?;
    let invite_id = random_token();
    let secret = random_token();
    {
        let mut guard = app
            .lock()
            .map_err(|_| "Application state lock is poisoned")?;
        ensure_board_owned(guard.boards.iter().find(|board| board.id == board_id))?;
        guard
            .database
            .save_iroh_invite(&invite_id, board_id, &secret, &permission)
            .map_err(|e| e.to_string())?;
    }
    let ticket = Ticket {
        version: 1,
        endpoint: share_address(&endpoint),
        invite_id,
        secret,
        permission,
    };
    Ok(format!(
        "cardbe://share/{}",
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&ticket).map_err(|e| e.to_string())?)
    ))
}

#[tauri::command]
pub fn list_iroh_invites(app: State<'_, SharedAppData>) -> Result<Vec<IrohInviteSummary>, String> {
    let guard = app
        .lock()
        .map_err(|_| "Application state lock is poisoned")?;
    let names: HashMap<i64, String> = guard
        .boards
        .iter()
        .map(|b| (b.id, b.name.clone()))
        .collect();
    guard
        .database
        .iroh_invite_summaries()
        .map_err(|e| e.to_string())
        .map(|items| {
            items
                .into_iter()
                .map(
                    |(invite_id, board_id, permission, enabled, created_at, _)| IrohInviteSummary {
                        invite_id,
                        board_id,
                        board_name: names
                            .get(&board_id)
                            .cloned()
                            .unwrap_or_else(|| "Deleted board".into()),
                        permission,
                        enabled,
                        created_at,
                    },
                )
                .collect()
        })
}

#[tauri::command]
pub async fn get_iroh_invite_access(
    app: State<'_, SharedAppData>,
    network: State<'_, IrohShareState>,
    app_handle: AppHandle,
    invite_id: String,
) -> Result<IrohInviteAccess, String> {
    let (path, secret, permission) = {
        let guard = app
            .lock()
            .map_err(|_| "Application state lock is poisoned")?;
        let (_, _, permission, enabled, _, secret) = guard
            .database
            .iroh_invite_summaries()
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|item| item.0 == invite_id)
            .ok_or("Invitation not found")?;
        if !enabled {
            return Err("Enable this invitation before copying it".into());
        }
        (guard.database.path().to_path_buf(), secret, permission)
    };
    let endpoint = ensure_host(&network, path, app_handle).await?;
    let ticket = Ticket {
        version: 1,
        endpoint: share_address(&endpoint),
        invite_id,
        secret,
        permission,
    };
    let ticket = format!(
        "cardbe://share/{}",
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&ticket).map_err(|e| e.to_string())?)
    );
    let qr_svg = iroh_invite_qr_svg(ticket.clone())?;
    Ok(IrohInviteAccess { ticket, qr_svg })
}

#[tauri::command]
pub async fn update_iroh_invite(
    app: State<'_, SharedAppData>,
    network: State<'_, IrohShareState>,
    app_handle: AppHandle,
    invite_id: String,
    permission: Option<String>,
    enabled: Option<bool>,
) -> Result<(), String> {
    if let Some(ref permission) = permission {
        if permission != "viewer" && permission != "editor" {
            return Err("Permission must be viewer or editor".into());
        }
    }
    let path = {
        let mut guard = app
            .lock()
            .map_err(|_| "Application state lock is poisoned")?;
        guard
            .database
            .update_iroh_invite(&invite_id, permission.as_deref(), enabled)
            .map_err(|e| e.to_string())?;
        guard.database.path().to_path_buf()
    };
    if enabled == Some(true) {
        if let Err(error) = ensure_host(&network, path, app_handle).await {
            if let Ok(mut last_error) = network.last_error.lock() {
                *last_error = Some(error);
            }
        }
    }
    if enabled == Some(false) {
        stop_host_if_idle(&network).await?;
    }
    Ok(())
}

#[tauri::command]
pub async fn delete_iroh_invite(
    app: State<'_, SharedAppData>,
    network: State<'_, IrohShareState>,
    invite_id: String,
) -> Result<(), String> {
    {
        let mut guard = app
            .lock()
            .map_err(|_| "Application state lock is poisoned")?;
        guard
            .database
            .delete_iroh_invite(&invite_id)
            .map_err(|e| e.to_string())?;
    }
    stop_host_if_idle(&network).await?;
    Ok(())
}

#[tauri::command]
pub fn iroh_invite_qr_svg(ticket: String) -> Result<String, String> {
    let code = QrCode::new(ticket.as_bytes()).map_err(|e| e.to_string())?;
    Ok(code.render::<svg::Color>().min_dimensions(240, 240).build())
}

#[tauri::command]
pub async fn join_iroh_invite(
    app: State<'_, SharedAppData>,
    ticket: String,
) -> Result<Board, String> {
    let ticket_string = ticket.clone();
    let ticket = parse_ticket(&ticket)?;
    {
        let guard = app
            .lock()
            .map_err(|_| "Application state lock is poisoned")?;
        ensure_invite_not_joined(&guard.database, &ticket)?;
    }
    let endpoint = Endpoint::bind(presets::N0)
        .await
        .map_err(|e| e.to_string())?;
    let connection = tokio::time::timeout(
        CONNECT_TIMEOUT,
        endpoint.connect(ticket.endpoint.clone(), ALPN),
    )
    .await
    .map_err(|_| "The board owner could not be reached within 20 seconds. Make sure their Cardbe is open and both computers have internet access.".to_string())?
    .map_err(|e| format!("Could not connect to the board owner: {e}"))?;
    let (mut send, mut recv) = tokio::time::timeout(CONNECT_TIMEOUT, connection.open_bi())
        .await
        .map_err(|_| {
            "Connected to the owner, but the sharing service did not respond.".to_string()
        })?
        .map_err(|e| format!("Could not open the board-sharing connection: {e}"))?;
    tokio::time::timeout(
        CONNECT_TIMEOUT,
        send.write_all(
            &serde_json::to_vec(&Request {
                invite_id: ticket.invite_id.clone(),
                secret: ticket.secret.clone(),
                action: "pull".into(),
                loro_state_vector: Vec::new(),
                loro_update: Vec::new(),
                known_revision: None,
                known_permission: None,
            })
            .map_err(|e| e.to_string())?,
        ),
    )
    .await
    .map_err(|_| "The invitation request timed out".to_string())?
    .map_err(|e| e.to_string())?;
    send.finish().map_err(|e| e.to_string())?;
    let bytes = tokio::time::timeout(TRANSFER_TIMEOUT, recv.read_to_end(MAX_MESSAGE))
        .await
        .map_err(|_| {
            "The board owner connected but did not send the board within 60 seconds.".to_string()
        })?
        .map_err(|e| format!("Could not receive the shared board: {e}"))?;
    connection.close(0u32.into(), b"shared board received");
    endpoint.close().await;
    let snapshot: Snapshot = serde_json::from_slice(&bytes)
        .map_err(|_| "Invitation was revoked or the owner sent an invalid snapshot")?;
    if !snapshot.ok {
        return Err(snapshot
            .error
            .unwrap_or_else(|| "Invitation was revoked".into()));
    }
    let mut guard = app
        .lock()
        .map_err(|_| "Application state lock is poisoned")?;
    ensure_invite_not_joined(&guard.database, &ticket)?;
    if !matches!(snapshot.permission.as_str(), "viewer" | "editor") {
        return Err("Owner sent an invalid permission".into());
    }
    let board = guard
        .database
        .create_received_board(
            &snapshot.name,
            &snapshot.data,
            &snapshot.permission,
            snapshot.revision,
            &ticket_string,
            Some(&snapshot.loro_update),
        )
        .map_err(|e| e.to_string())?;
    guard.boards.push(board.clone());
    Ok(board)
}

#[tauri::command]
pub async fn sync_iroh_board(
    app: State<'_, SharedAppData>,
    board_id: i64,
) -> Result<Board, String> {
    let (ticket_text, role, revision, status, data) = {
        let guard = app
            .lock()
            .map_err(|_| "Application state lock is poisoned")?;
        (
            guard
                .database
                .iroh_remote(board_id)
                .map_err(|e| e.to_string())?
                .ok_or("This is not a received shared board")?,
            guard
                .database
                .board_role(board_id)
                .map_err(|e| e.to_string())?,
            guard
                .boards
                .iter()
                .find(|b| b.id == board_id)
                .map(|b| b.sync_revision)
                .unwrap_or(0),
            guard
                .boards
                .iter()
                .find(|b| b.id == board_id)
                .map(|b| b.sync_status.clone())
                .unwrap_or_else(|| "synced".into()),
            guard
                .database
                .read_board_complete(board_id)
                .map_err(|e| e.to_string())?,
        )
    };
    if status == "conflict" {
        return Err("Resolve the saved sync conflict before syncing again".into());
    }
    let ticket = parse_ticket(&ticket_text).map_err(|_| "Stored invitation is invalid")?;
    let (loro_update, loro_state_vector) = {
        let guard = app
            .lock()
            .map_err(|_| "Application state lock is poisoned")?;
        let update = guard
            .database
            .iroh_loro_update(board_id)
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let vector = crate::loro_board::state_vector(Some(&update)).map_err(|e| e.to_string())?;
        (update, vector)
    };
    if role == "editor" && loro_update.is_empty() {
        return Err("Editable board is missing its Loro document. Rejoin the invitation.".into());
    }
    let endpoint = Endpoint::bind(presets::N0)
        .await
        .map_err(|e| e.to_string())?;
    // Editors always exchange their Loro state when a document exists. A clean
    // editor still needs remote changes merged into its local CRDT history.
    let pushing = role == "editor";
    let request = Request {
        invite_id: ticket.invite_id,
        secret: ticket.secret,
        action: if pushing {
            "loro_sync".into()
        } else {
            "pull".into()
        },
        loro_state_vector,
        // A clean editor only needs to tell the owner what it has seen.
        // Pending edits send the full document until the owner acknowledges them.
        loro_update: if needs_editor_upload(&role, &status) {
            loro_update.clone()
        } else {
            Vec::new()
        },
        known_revision: Some(revision),
        known_permission: Some(role.clone()),
    };
    let used_loro_sync = request.action == "loro_sync";
    let bytes = tokio::time::timeout(TRANSFER_TIMEOUT, async {
        let connection = endpoint
            .connect(ticket.endpoint, ALPN)
            .await
            .map_err(|e| e.to_string())?;
        let (mut send, mut recv) = connection.open_bi().await.map_err(|e| e.to_string())?;
        let encoded = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
        if encoded.len() > MAX_MESSAGE {
            return Err("Shared board exceeds the 16 MiB transfer limit".into());
        }
        send.write_all(&encoded).await.map_err(|e| e.to_string())?;
        send.finish().map_err(|e| e.to_string())?;
        recv.read_to_end(MAX_MESSAGE)
            .await
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "Shared-board sync timed out after 60 seconds".to_string())??;
    endpoint.close().await;
    let snapshot: Snapshot =
        serde_json::from_slice(&bytes).map_err(|_| "Owner sent an invalid response")?;
    if !matches!(snapshot.permission.as_str(), "viewer" | "editor") {
        return Err(snapshot
            .error
            .unwrap_or_else(|| "Owner sent an invalid permission".into()));
    }
    let mut guard = app
        .lock()
        .map_err(|_| "Application state lock is poisoned")?;
    if snapshot.unchanged {
        let state = guard
            .database
            .board_sync_state(board_id)
            .map_err(|e| e.to_string())?;
        if !snapshot.ok
            || role != "viewer"
            || snapshot.permission != "viewer"
            || snapshot.revision != revision
            || state.0 != revision
            || state.1 != status
            || guard
                .database
                .board_role(board_id)
                .map_err(|e| e.to_string())?
                != "viewer"
        {
            return Err("Owner sent an invalid unchanged response".into());
        }
        return guard
            .boards
            .iter()
            .find(|board| board.id == board_id)
            .cloned()
            .ok_or_else(|| "Board not found".into());
    }
    if used_loro_sync && snapshot.ok {
        guard
            .database
            .merge_iroh_loro_diff(
                board_id,
                &snapshot.loro_update,
                &snapshot.name,
                &snapshot.permission,
                snapshot.revision,
                &loro_update,
            )
            .map_err(|e| e.to_string())?;
        let board = guard
            .database
            .boards()
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|b| b.id == board_id)
            .ok_or("Board not found")?;
        if let Some(item) = guard.boards.iter_mut().find(|item| item.id == board_id) {
            *item = board.clone();
        }
        if guard.active_board_id == board_id {
            guard.stored = guard
                .database
                .load_board(board_id)
                .map_err(|e| e.to_string())?;
            guard.refresh_labels();
            guard.undo_history.clear();
        }
        return Ok(board);
    }
    let mut current = guard
        .database
        .read_board_complete(board_id)
        .map_err(|e| e.to_string())?;
    let mut sent_data = data.clone();
    current.settings = Default::default();
    sent_data.settings = Default::default();
    let current_state = guard
        .database
        .board_sync_state(board_id)
        .map_err(|e| e.to_string())?;
    let changed_during_sync =
        current != sent_data || current_state.0 != revision || current_state.1 != status;
    // A permission downgrade does not create a conflict for a clean editor.
    // Only unsent local edits need a copy before accepting the owner snapshot.
    let local_edits_at_risk = local_edits_at_risk(&status, changed_during_sync);
    let already_applied = !snapshot.ok && !local_edits_at_risk && current == snapshot.data;
    if already_applied {
        guard
            .database
            .apply_iroh_snapshot(
                board_id,
                &snapshot.name,
                &snapshot.permission,
                snapshot.revision,
                &snapshot.data,
                Some(&snapshot.loro_update),
            )
            .map_err(|e| e.to_string())?;
    }
    if !already_applied && (changed_during_sync || (!snapshot.ok && local_edits_at_risk)) {
        guard
            .database
            .save_iroh_conflict(
                board_id,
                snapshot.revision,
                &snapshot.name,
                &snapshot.permission,
                &snapshot.data,
            )
            .map_err(|e| e.to_string())?;
        if let Some(board) = guard.boards.iter_mut().find(|board| board.id == board_id) {
            board.sync_status = "conflict".into();
            board.shared_role = snapshot.permission;
        }
        return Err(if changed_during_sync {
            "Local changes arrived during sync. Both versions were saved for review".into()
        } else {
            snapshot
                .error
                .unwrap_or_else(|| "Sync conflict: both versions were saved for review".into())
        });
    }
    if !already_applied {
        guard
            .database
            .apply_iroh_snapshot(
                board_id,
                &snapshot.name,
                &snapshot.permission,
                snapshot.revision,
                &snapshot.data,
                Some(&snapshot.loro_update),
            )
            .map_err(|e| e.to_string())?;
    }
    let index = guard
        .boards
        .iter()
        .position(|b| b.id == board_id)
        .ok_or("Board not found")?;
    guard.boards[index].sync_revision = snapshot.revision;
    guard.boards[index].shared_role = snapshot.permission;
    guard.boards[index].name = snapshot.name.clone();
    guard.boards[index].task_count = snapshot
        .data
        .columns
        .iter()
        .map(|column| column.tasks.len() as i64)
        .sum();
    guard.boards[index].sync_status = "synced".into();
    if guard.active_board_id == board_id {
        guard.stored = guard
            .database
            .load_board(board_id)
            .map_err(|e| e.to_string())?;
        guard.archives_loaded = true;
        guard.refresh_labels();
        guard.undo_history.clear();
    }
    Ok(guard.boards[index].clone())
}

/// Resolve a saved conflict. Keeping local changes explicitly rebases them on
/// the last owner snapshot; the next sync still uses revision checking.
#[tauri::command]
pub fn resolve_iroh_conflict(
    app: State<'_, SharedAppData>,
    board_id: i64,
    keep_local: bool,
) -> Result<(), String> {
    let mut guard = app
        .lock()
        .map_err(|_| "Application state lock is poisoned")?;
    let (revision, name, permission, remote) = guard
        .database
        .iroh_conflict(board_id)
        .map_err(|e| e.to_string())?
        .ok_or("No saved sync conflict")?;
    let local = guard
        .database
        .read_board_complete(board_id)
        .map_err(|e| e.to_string())?;
    if keep_local {
        if permission != "editor" {
            return Err("The invitation is read-only; save your changes as a copy instead".into());
        }
        guard
            .database
            .keep_iroh_conflict_local(board_id, revision)
            .map_err(|e| e.to_string())?;
    } else {
        let local_name = guard
            .boards
            .iter()
            .find(|board| board.id == board_id)
            .map(|board| board.name.clone())
            .ok_or("Board not found")?;
        let copy = guard
            .database
            .use_iroh_conflict_remote(
                board_id,
                &local_name,
                &local,
                &name,
                &permission,
                revision,
                &remote,
            )
            .map_err(|e| e.to_string())?;
        guard.boards.push(copy);
        if guard.active_board_id == board_id {
            guard.stored = guard
                .database
                .load_board(board_id)
                .map_err(|e| e.to_string())?;
            guard.archives_loaded = true;
            guard.refresh_labels();
            guard.undo_history.clear();
        }
    }
    if let Some(board) = guard.boards.iter_mut().find(|board| board.id == board_id) {
        board.shared_role = permission;
        board.sync_revision = revision;
        board.sync_status = if keep_local { "pending" } else { "synced" }.into();
        if !keep_local {
            board.name = name;
            board.task_count = remote
                .columns
                .iter()
                .map(|column| column.tasks.len() as i64)
                .sum();
        }
    }
    Ok(())
}
