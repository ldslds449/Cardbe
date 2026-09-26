//! Owner-authoritative Iroh snapshot protocol.  The invitation secret is only
//! present in the copied ticket; it is never placed in a normal backup.
use crate::{
    models::{Board, BoardRole, IrohDeviceStatus, IrohPermission, StoredData, SyncStatus},
    state::SharedAppData,
    storage::Database,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use iroh::{endpoint::presets, Endpoint, EndpointAddr, EndpointId, SecretKey, TransportAddr};
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
    node_id: String,
    relay_urls: Vec<String>,
    invite_id: String,
    secret: String,
    permission: IrohPermission,
}
#[derive(Serialize, Deserialize)]
struct RemoteInvitation {
    node_id: String,
    relay_urls: Vec<String>,
    invite_id: String,
    secret: String,
}
impl From<&Ticket> for RemoteInvitation {
    fn from(ticket: &Ticket) -> Self {
        Self {
            node_id: ticket.node_id.clone(),
            relay_urls: ticket.relay_urls.clone(),
            invite_id: ticket.invite_id.clone(),
            secret: ticket.secret.clone(),
        }
    }
}
fn endpoint_address(node_id: &str, relay_urls: &[String]) -> Result<EndpointAddr, String> {
    let key: [u8; 32] = URL_SAFE_NO_PAD
        .decode(node_id)
        .map_err(|_| "Invalid invitation node ID")?
        .try_into()
        .map_err(|_| "Invalid invitation node ID")?;
    let id = EndpointId::from_bytes(&key).map_err(|_| "Invalid invitation node ID")?;
    let addresses = relay_urls
        .iter()
        .map(|url| {
            url.parse()
                .map(TransportAddr::Relay)
                .map_err(|_| "Invalid invitation relay URL")
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(EndpointAddr::from_parts(id, addresses))
}
#[derive(Serialize, Deserialize)]
struct Request {
    invite_id: String,
    secret: String,
    #[serde(default)]
    action: IrohAction,
    #[serde(default)]
    loro_state_vector: Vec<u8>,
    #[serde(default)]
    loro_update: Vec<u8>,
    #[serde(default)]
    known_revision: Option<i64>,
    #[serde(default)]
    known_permission: Option<IrohPermission>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum IrohAction {
    #[default]
    Pull,
    Request,
    LoroSync,
}

fn local_edits_at_risk(status: SyncStatus, changed_during_sync: bool) -> bool {
    changed_during_sync || status == SyncStatus::Pending
}

fn needs_editor_upload(role: BoardRole, status: SyncStatus) -> bool {
    role == BoardRole::Editor && status == SyncStatus::Pending
}
#[derive(Serialize, Deserialize)]
struct Snapshot {
    ok: bool,
    error: Option<String>,
    #[serde(default)]
    error_code: Option<SnapshotErrorCode>,
    name: String,
    permission: IrohPermission,
    revision: i64,
    data: StoredData,
    #[serde(default)]
    loro_update: Vec<u8>,
    #[serde(default)]
    unchanged: bool,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotErrorCode {
    ApprovalRequired,
}

fn snapshot_requires_approval(snapshot: &Snapshot) -> bool {
    snapshot.error_code == Some(SnapshotErrorCode::ApprovalRequired)
        // Accept the old wire format, which only sent the user-facing message.
        || snapshot
            .error
            .as_deref()
            .is_some_and(|error| error.starts_with("Waiting for owner approval"))
}

#[derive(Clone, Serialize)]
struct RemotePushApplied {
    board_id: i64,
    revision: i64,
}
#[derive(Clone)]
struct HostedInvite {
    permission: IrohPermission,
    board_id: i64,
    enabled: bool,
}
#[derive(Clone, Serialize)]
pub struct IrohInviteSummary {
    invite_id: String,
    board_id: i64,
    board_name: String,
    permission: IrohPermission,
    enabled: bool,
    created_at: String,
    devices: Vec<IrohDevice>,
}
#[derive(Clone, Serialize)]
pub struct IrohDevice {
    node_id: String,
    status: IrohDeviceStatus,
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

fn set_last_error(state: &IrohShareState, value: Option<String>) {
    match state.last_error.lock() {
        Ok(mut last_error) => *last_error = value,
        Err(error) => {
            log::error!(target: "iroh", "Could not update the board-sharing service error state: {error}");
        }
    }
}

fn random_token() -> String {
    URL_SAFE_NO_PAD.encode(SecretKey::generate().to_bytes())
}
fn device_key(db: &mut Database) -> Result<SecretKey, String> {
    use sha2::{Digest, Sha256};
    let seed = db.iroh_endpoint_seed().map_err(|e| e.to_string())?;
    let bytes: [u8; 32] = Sha256::digest(seed.as_bytes()).into();
    Ok(SecretKey::from_bytes(&bytes))
}

fn parse_ticket(ticket: &str) -> Result<Ticket, String> {
    let encoded = ticket_payload(ticket).ok_or("Not a shared-board invitation")?;
    let decoded = URL_SAFE_NO_PAD.decode(encoded).map_err(|error| {
        log::warn!(target: "iroh", "Could not decode shared-board invitation: {error}");
        "Invalid invitation"
    })?;
    let ticket: Ticket = serde_json::from_slice(&decoded).map_err(|error| {
        log::warn!(target: "iroh", "Could not parse shared-board invitation: {error}");
        "Invalid invitation"
    })?;
    if ticket.version != 1 || ticket.invite_id.is_empty() || ticket.secret.is_empty() {
        return Err("Unsupported shared-board invitation".into());
    }
    endpoint_address(&ticket.node_id, &ticket.relay_urls)?;
    Ok(ticket)
}

fn ensure_invite_not_joined(db: &Database, ticket: &Ticket) -> Result<(), String> {
    let already_joined = db
        .iroh_remote_tickets()
        .map_err(|e| e.to_string())?
        .iter()
        .filter_map(|saved| match serde_json::from_str::<RemoteInvitation>(saved) {
            Ok(saved) => Some(saved),
            Err(error) => {
                log::warn!(target: "iroh", "Could not parse a stored shared-board invitation while checking for duplicates: {error}");
                None
            }
        })
        .any(|saved| {
            saved.node_id == ticket.node_id && saved.invite_id == ticket.invite_id
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
        viewer_is_current, BoardRole, IrohAction, IrohPermission, Request, SyncStatus,
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
            shared_role: BoardRole::Viewer,
            sync_status: SyncStatus::Synced,
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
        assert!(!local_edits_at_risk(SyncStatus::Synced, false));
        assert!(local_edits_at_risk(SyncStatus::Pending, false));
        assert!(local_edits_at_risk(SyncStatus::Synced, true));
    }

    #[test]
    fn clean_editor_only_sends_its_version_vector() {
        assert!(!needs_editor_upload(BoardRole::Editor, SyncStatus::Synced));
        assert!(needs_editor_upload(BoardRole::Editor, SyncStatus::Pending));
        assert!(!needs_editor_upload(BoardRole::Viewer, SyncStatus::Pending));
    }

    #[test]
    fn viewer_skips_snapshot_only_for_the_same_revision_and_permission() {
        let mut request = Request {
            invite_id: "invite".into(),
            secret: "secret".into(),
            action: IrohAction::Pull,
            loro_state_vector: vec![],
            loro_update: vec![],
            known_revision: Some(4),
            known_permission: Some(IrohPermission::Viewer),
        };
        assert!(viewer_is_current(&request, IrohPermission::Viewer, 4));
        assert!(!viewer_is_current(&request, IrohPermission::Viewer, 5));
        assert!(!viewer_is_current(&request, IrohPermission::Editor, 4));
        request.known_permission = Some(IrohPermission::Editor);
        assert!(!viewer_is_current(&request, IrohPermission::Viewer, 4));
        request.known_permission = Some(IrohPermission::Viewer);
        request.known_revision = None;
        assert!(!viewer_is_current(&request, IrohPermission::Viewer, 4));
    }
}

fn viewer_is_current(request: &Request, permission: IrohPermission, revision: i64) -> bool {
    permission == IrohPermission::Viewer
        && request.known_permission == Some(IrohPermission::Viewer)
        && request.known_revision == Some(revision)
}

// Persist and transmit an endpoint identity plus relay route, never a peer's
// observed LAN/WAN IP address. The stable endpoint key is stored locally by
// Database::iroh_endpoint_seed; Iroh/N0 resolves fresh paths after restarts.
fn share_address(endpoint: &Endpoint) -> (String, Vec<String>) {
    let address = endpoint.addr();
    (
        URL_SAFE_NO_PAD.encode(address.id.as_bytes()),
        address.relay_urls().map(ToString::to_string).collect(),
    )
}

fn ensure_board_owned(board: Option<&Board>) -> Result<(), String> {
    let board = board.ok_or("Board not found")?;
    if board.shared_role != BoardRole::Owner {
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
    peer_id: &str,
) -> Result<Snapshot, String> {
    let app_state = app_handle.state::<SharedAppData>();
    let _state = app_state
        .lock()
        .map_err(|_| "Application state lock is poisoned")?;
    let mut db = Database::open(path.to_path_buf()).map_err(|e| e.to_string())?;
    if db
        .iroh_device_access(&request.invite_id, &request.secret, peer_id, false)
        .map_err(|e| e.to_string())?
        != Some(IrohDeviceStatus::Approved)
    {
        return Err("Access was declined or revoked. You can request access again.".into());
    }
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
        .find(|board| board.id == board_id && board.shared_role == BoardRole::Owner)
        .ok_or("Board was deleted")?;
    if error.is_none() && viewer_is_current(request, permission, board.sync_revision) {
        return Ok(Snapshot {
            ok: true,
            error: None,
            error_code: None,
            name: board.name,
            permission,
            revision: board.sync_revision,
            data: StoredData::default(),
            loro_update: Vec::new(),
            unchanged: true,
        });
    }
    if request.action == IrohAction::LoroSync && error.is_none() {
        return Ok(Snapshot {
            ok: true,
            error: None,
            error_code: None,
            name: board.name,
            permission,
            revision: board.sync_revision,
            data: StoredData::default(),
            loro_update: Vec::new(),
            unchanged: false,
        });
    }
    let loro_update = if permission == IrohPermission::Editor {
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
        error_code: None,
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
    let key = {
        let mut db = Database::open(path.clone()).map_err(|e| e.to_string())?;
        device_key(&mut db)?
    };
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(key)
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
            let slot = match connection_slots.clone().try_acquire_owned() {
                Ok(slot) => slot,
                Err(error) => {
                    log::warn!(target: "iroh", "Rejected shared-board connection: {error}");
                    continue;
                }
            };
            let connecting = match incoming.accept() {
                Ok(connecting) => connecting,
                Err(error) => {
                    log::warn!(target: "iroh", "Could not accept shared-board connection: {error}");
                    continue;
                }
            };
            let connection = match tokio::time::timeout(CONNECT_TIMEOUT, connecting).await {
                Ok(Ok(connection)) => connection,
                Ok(Err(error)) => {
                    log::warn!(target: "iroh", "Shared-board connection failed: {error}");
                    continue;
                }
                Err(error) => {
                    log::warn!(target: "iroh", "Shared-board connection timed out: {error}");
                    continue;
                }
            };
            let peer_id = URL_SAFE_NO_PAD.encode(connection.remote_id().as_bytes());
            let path = path.clone();
            let app_handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                let _slot = slot;
                let (mut send, mut recv) = match tokio::time::timeout(
                    CONNECT_TIMEOUT,
                    connection.accept_bi(),
                )
                .await
                {
                    Ok(Ok(streams)) => streams,
                    Ok(Err(error)) => {
                        log::warn!(target: "iroh", "Could not open shared-board stream: {error}");
                        return;
                    }
                    Err(error) => {
                        log::warn!(target: "iroh", "Opening shared-board stream timed out: {error}");
                        return;
                    }
                };
                let bytes = match tokio::time::timeout(
                    TRANSFER_TIMEOUT,
                    recv.read_to_end(MAX_MESSAGE),
                )
                .await
                {
                    Ok(Ok(bytes)) => bytes,
                    Ok(Err(error)) => {
                        log::warn!(target: "iroh", "Could not read shared-board request: {error}");
                        return;
                    }
                    Err(error) => {
                        log::warn!(target: "iroh", "Reading shared-board request timed out: {error}");
                        return;
                    }
                };
                let mut applied_push = None;
                let mut error_code = None;
                let response = match serde_json::from_slice::<Request>(&bytes) {
                    Ok(request) => match Database::open(path.clone())
                        .map_err(|e| e.to_string())
                        .and_then(|mut db| {
                            let invite = db.iroh_invites().map_err(|e| e.to_string())?
                                .into_iter()
                                .find(|(id, _, secret, _, _)| {
                                    id == &request.invite_id && secret == &request.secret
                                })
                                .map(|(_, board_id, _, permission, enabled)| HostedInvite {
                                    permission,
                                    board_id,
                                    enabled,
                                })
                                .ok_or_else(|| "Invitation was revoked".to_string())?;
                            if !invite.enabled { return Err("Invitation is disabled".into()); }
                            match db.iroh_device_access(&request.invite_id, &request.secret, &peer_id, request.action == IrohAction::Request)
                                .map_err(|e| e.to_string())? {
                                Some(IrohDeviceStatus::Approved) => Ok(invite),
                                Some(IrohDeviceStatus::Pending) => {
                                    error_code = Some(SnapshotErrorCode::ApprovalRequired);
                                    Err(
                                        "Waiting for owner approval. Ask the owner to approve this device, then try again."
                                            .into(),
                                    )
                                }
                                Some(IrohDeviceStatus::Revoked) => Err("Access was declined or revoked. You can request access again.".into()),
                                None => Err("Invitation was revoked".into()),
                            }
                        }) {
                        Err(error) => Err(error),
                        Ok(invite) if matches!(request.action, IrohAction::Pull | IrohAction::Request) => {
                            snapshot_from_db(&app_handle, &path, invite.board_id, None, &request, &peer_id)
                        }
                        Ok(invite)
                            if request.action == IrohAction::LoroSync && invite.permission == IrohPermission::Editor =>
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
                                        &peer_id,
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
                                    &peer_id,
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
                            &peer_id,
                        ),
                    },
                    Err(error) => {
                        log::warn!(target: "iroh", "Could not parse shared-board request: {error}");
                        Err("Invalid request".into())
                    }
                }
                .unwrap_or_else(|error| {
                    log::warn!(target: "iroh", "Shared-board request failed: {error}");
                    Snapshot {
                        ok: false,
                        error: Some(error),
                        error_code: None,
                        name: String::new(),
                        permission: IrohPermission::Viewer,
                        revision: 0,
                        data: StoredData::default(),
                        loro_update: Vec::new(),
                        unchanged: false,
                    }
                });
                let mut response = response;
                response.error_code = error_code;
                if let Some((board_id, revision)) = applied_push {
                    if let Err(error) = app_handle.emit(
                        "cardbe:iroh-remote-push",
                        RemotePushApplied { board_id, revision },
                    ) {
                        log::warn!(target: "iroh", "Could not notify the app about a remote board update: {error}");
                    }
                }
                let bytes = match serde_json::to_vec(&response) {
                    Ok(bytes) if bytes.len() <= MAX_MESSAGE => bytes,
                    Ok(bytes) => {
                        log::warn!(target: "iroh", "Shared-board response exceeded the transfer limit: {} bytes", bytes.len());
                        serde_json::to_vec(&Snapshot {
                            ok: false,
                            error: Some("Shared board exceeds the 16 MiB transfer limit".into()),
                            error_code: None,
                            name: String::new(),
                            permission: IrohPermission::Viewer,
                            revision: 0,
                            data: StoredData::default(),
                            loro_update: Vec::new(),
                            unchanged: false,
                        })
                        .unwrap_or_else(|error| {
                            log::error!(target: "iroh", "Could not serialize the oversized-response fallback: {error}");
                            Vec::new()
                        })
                    }
                    Err(error) => {
                        log::error!(target: "iroh", "Could not serialize shared-board response: {error}");
                        Vec::new()
                    }
                };
                if !bytes.is_empty() {
                    match tokio::time::timeout(TRANSFER_TIMEOUT, send.write_all(&bytes)).await {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => {
                            log::error!(target: "iroh", "Shared-board response write failed: {error}");
                            return;
                        }
                        Err(_) => {
                            log::error!(target: "iroh", "Shared-board response write timed out");
                            return;
                        }
                    }
                }
                if let Err(error) = send.finish() {
                    log::error!(target: "iroh", "Shared-board response finish failed: {error}");
                    return;
                }
                // Keep the connection alive until the peer acknowledges the
                // complete response. Dropping the final connection handle
                // immediately after `finish` can otherwise surface as
                // `connection lost` on slower relay paths.
                match tokio::time::timeout(CONNECT_TIMEOUT, send.stopped()).await {
                    Ok(Ok(_)) => {}
                    Ok(Err(error)) => {
                        log::warn!(target: "iroh", "Shared-board response acknowledgement failed: {error}");
                    }
                    Err(error) => {
                        log::warn!(target: "iroh", "Shared-board response acknowledgement timed out: {error}");
                    }
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
    set_last_error(state, None);
    Ok(endpoint)
}

/// Restore the owner endpoint after launch so existing capability links keep
/// working after restart.  The database contains no rows for a new install.
pub async fn restore_iroh_host(
    network: &IrohShareState,
    path: std::path::PathBuf,
    app_handle: AppHandle,
) {
    let has_invites = match Database::open(path.clone()).and_then(|db| db.iroh_invites()) {
        Ok(invites) => invites.iter().any(|invite| invite.4),
        Err(error) => {
            log::error!(target: "iroh", "Could not inspect saved invitations while restoring the board-sharing service: {error}");
            return;
        }
    };
    if has_invites {
        if let Err(error) = ensure_host(network, path, app_handle).await {
            log::error!(target: "iroh", "Could not restore the board-sharing service: {error}");
            set_last_error(network, Some(error));
        }
    }
}

#[tauri::command]
pub fn iroh_host_error(network: State<'_, IrohShareState>) -> Option<String> {
    match network.last_error.lock() {
        Ok(error) => error.clone(),
        Err(error) => {
            log::error!(target: "iroh", "Could not read the board-sharing service error state: {error}");
            None
        }
    }
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
            set_last_error(&network, Some(error.clone()));
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
            set_last_error(network, None);
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
    permission: IrohPermission,
) -> Result<String, String> {
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
            .save_iroh_invite(&invite_id, board_id, &secret, permission)
            .map_err(|e| e.to_string())?;
    }
    let (node_id, relay_urls) = share_address(&endpoint);
    let ticket = Ticket {
        version: 1,
        node_id,
        relay_urls,
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
    let devices = guard.database.iroh_devices().map_err(|e| e.to_string())?;
    guard
        .database
        .iroh_invite_summaries()
        .map_err(|e| e.to_string())
        .map(|items| {
            items
                .into_iter()
                .map(
                    |(invite_id, board_id, permission, enabled, created_at, _)| IrohInviteSummary {
                        devices: devices
                            .iter()
                            .filter(|(id, _, _)| id == &invite_id)
                            .map(|(_, node_id, status)| IrohDevice {
                                node_id: node_id.clone(),
                                status: *status,
                            })
                            .collect(),
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
pub fn set_iroh_device_approved(
    app: State<'_, SharedAppData>,
    invite_id: String,
    node_id: String,
    approved: bool,
) -> Result<(), String> {
    app.lock()
        .map_err(|_| "Application state lock is poisoned")?
        .database
        .set_iroh_device_approved(&invite_id, &node_id, approved)
        .map_err(|e| e.to_string())
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
    let (node_id, relay_urls) = share_address(&endpoint);
    let ticket = Ticket {
        version: 1,
        node_id,
        relay_urls,
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
    permission: Option<IrohPermission>,
    enabled: Option<bool>,
) -> Result<(), String> {
    let path = {
        let mut guard = app
            .lock()
            .map_err(|_| "Application state lock is poisoned")?;
        guard
            .database
            .update_iroh_invite(&invite_id, permission, enabled)
            .map_err(|e| e.to_string())?;
        guard.database.path().to_path_buf()
    };
    if enabled == Some(true) {
        if let Err(error) = ensure_host(&network, path, app_handle).await {
            log::error!(target: "iroh", "Could not start the board-sharing service after invitation update: {error}");
            set_last_error(&network, Some(error));
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
    request_approval: bool,
) -> Result<Board, String> {
    let ticket = parse_ticket(&ticket)?;
    let address = endpoint_address(&ticket.node_id, &ticket.relay_urls)?;
    let key = {
        let mut guard = app
            .lock()
            .map_err(|_| "Application state lock is poisoned")?;
        ensure_invite_not_joined(&guard.database, &ticket)?;
        device_key(&mut guard.database)?
    };
    let device_id = URL_SAFE_NO_PAD.encode(key.public().as_bytes());
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(key)
        .bind()
        .await
        .map_err(|e| e.to_string())?;
    let connection = tokio::time::timeout(
        CONNECT_TIMEOUT,
        endpoint.connect(address, ALPN),
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
                action: if request_approval {
                    IrohAction::Request
                } else {
                    IrohAction::Pull
                },
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
        .map_err(|error| {
            log::error!(target: "iroh", "Could not parse the shared-board invitation response: {error}");
            "Invitation was revoked or the owner sent an invalid snapshot"
        })?;
    if !snapshot.ok {
        let approval_required = snapshot_requires_approval(&snapshot);
        let error = snapshot
            .error
            .unwrap_or_else(|| "Invitation was revoked".into());
        return Err(if approval_required {
            format!("APPROVAL_REQUIRED:{device_id}")
        } else {
            error
        });
    }
    let mut guard = app
        .lock()
        .map_err(|_| "Application state lock is poisoned")?;
    ensure_invite_not_joined(&guard.database, &ticket)?;
    let board = guard
        .database
        .create_received_board(
            &snapshot.name,
            &snapshot.data,
            snapshot.permission,
            snapshot.revision,
            &serde_json::to_string(&RemoteInvitation::from(&ticket)).map_err(|e| e.to_string())?,
            Some(&snapshot.loro_update),
        )
        .map_err(|e| e.to_string())?;
    guard.boards.push(board.clone());
    Ok(board)
}

#[tauri::command]
pub async fn request_iroh_board_access(
    app: State<'_, SharedAppData>,
    board_id: i64,
    request_approval: bool,
) -> Result<(bool, String), String> {
    let (remote, key) = {
        let mut guard = app
            .lock()
            .map_err(|_| "Application state lock is poisoned")?;
        let remote = guard
            .database
            .iroh_remote(board_id)
            .map_err(|e| e.to_string())?
            .ok_or("This is not a received shared board")?;
        (serde_json::from_str::<RemoteInvitation>(&remote).map_err(|error| {
            log::warn!(target: "iroh", "Could not parse the stored shared-board invitation for an access request: {error}");
            "Stored invitation is invalid"
        })?, device_key(&mut guard.database)?)
    };
    let device_id = URL_SAFE_NO_PAD.encode(key.public().as_bytes());
    let address = endpoint_address(&remote.node_id, &remote.relay_urls)?;
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(key)
        .bind()
        .await
        .map_err(|e| e.to_string())?;
    let response = tokio::time::timeout(TRANSFER_TIMEOUT, async {
        let connection = endpoint
            .connect(address, ALPN)
            .await
            .map_err(|e| e.to_string())?;
        let (mut send, mut recv) = connection.open_bi().await.map_err(|e| e.to_string())?;
        let request = Request {
            invite_id: remote.invite_id,
            secret: remote.secret,
            action: if request_approval {
                IrohAction::Request
            } else {
                IrohAction::Pull
            },
            loro_state_vector: Vec::new(),
            loro_update: Vec::new(),
            known_revision: None,
            known_permission: None,
        };
        send.write_all(&serde_json::to_vec(&request).map_err(|e| e.to_string())?)
            .await
            .map_err(|e| e.to_string())?;
        send.finish().map_err(|e| e.to_string())?;
        recv.read_to_end(MAX_MESSAGE)
            .await
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "Access request timed out".to_string())??;
    endpoint.close().await;
    let snapshot: Snapshot = serde_json::from_slice(&response).map_err(|error| {
        log::error!(target: "iroh", "Could not parse the shared-board access response: {error}");
        "Owner sent an invalid response"
    })?;
    if snapshot.ok {
        return Ok((true, device_id));
    }
    if snapshot_requires_approval(&snapshot) {
        return Ok((false, device_id));
    }
    Err(snapshot
        .error
        .unwrap_or_else(|| "Access request failed".into()))
}

#[tauri::command]
pub async fn sync_iroh_board(
    app: State<'_, SharedAppData>,
    board_id: i64,
) -> Result<Board, String> {
    let (ticket_text, role, revision, status, data, key) = {
        let mut guard = app
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
                .map(|b| b.sync_status)
                .unwrap_or(SyncStatus::Synced),
            guard
                .database
                .read_board_complete(board_id)
                .map_err(|e| e.to_string())?,
            device_key(&mut guard.database)?,
        )
    };
    if status == SyncStatus::Conflict {
        return Err("Resolve the saved sync conflict before syncing again".into());
    }
    let ticket: RemoteInvitation = serde_json::from_str(&ticket_text).map_err(|error| {
        log::warn!(target: "iroh", "Could not parse the stored shared-board invitation for sync: {error}");
        "Stored invitation is invalid"
    })?;
    let address = endpoint_address(&ticket.node_id, &ticket.relay_urls)
        .map_err(|error| {
            log::warn!(target: "iroh", "Stored shared-board invitation has an invalid endpoint: {error}");
            "Stored invitation is invalid"
        })?;
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
    if role == BoardRole::Editor && loro_update.is_empty() {
        return Err("Editable board is missing its Loro document. Rejoin the invitation.".into());
    }
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(key)
        .bind()
        .await
        .map_err(|e| e.to_string())?;
    // Editors always exchange their Loro state when a document exists. A clean
    // editor still needs remote changes merged into its local CRDT history.
    let pushing = role == BoardRole::Editor;
    let permission = match role {
        BoardRole::Viewer => IrohPermission::Viewer,
        BoardRole::Editor => IrohPermission::Editor,
        BoardRole::Owner => return Err("This is not a received shared board".into()),
    };
    let request = Request {
        invite_id: ticket.invite_id,
        secret: ticket.secret,
        action: if pushing {
            IrohAction::LoroSync
        } else {
            IrohAction::Pull
        },
        loro_state_vector,
        // A clean editor only needs to tell the owner what it has seen.
        // Pending edits send the full document until the owner acknowledges them.
        loro_update: if needs_editor_upload(role, status) {
            loro_update.clone()
        } else {
            Vec::new()
        },
        known_revision: Some(revision),
        known_permission: Some(permission),
    };
    let used_loro_sync = request.action == IrohAction::LoroSync;
    let bytes = tokio::time::timeout(TRANSFER_TIMEOUT, async {
        let connection = endpoint
            .connect(address, ALPN)
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
    let snapshot: Snapshot = serde_json::from_slice(&bytes).map_err(|error| {
        log::error!(target: "iroh", "Could not parse the shared-board sync response: {error}");
        "Owner sent an invalid response"
    })?;
    let mut guard = app
        .lock()
        .map_err(|_| "Application state lock is poisoned")?;
    if snapshot.unchanged {
        let state = guard
            .database
            .board_sync_state(board_id)
            .map_err(|e| e.to_string())?;
        if !snapshot.ok
            || role != BoardRole::Viewer
            || snapshot.permission != IrohPermission::Viewer
            || snapshot.revision != revision
            || state.0 != revision
            || state.1 != status
            || guard
                .database
                .board_role(board_id)
                .map_err(|e| e.to_string())?
                != BoardRole::Viewer
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
                snapshot.permission,
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
    let local_edits_at_risk = local_edits_at_risk(status, changed_during_sync);
    let already_applied = !snapshot.ok && !local_edits_at_risk && current == snapshot.data;
    if already_applied {
        guard
            .database
            .apply_iroh_snapshot(
                board_id,
                &snapshot.name,
                snapshot.permission,
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
                snapshot.permission,
                &snapshot.data,
            )
            .map_err(|e| e.to_string())?;
        if let Some(board) = guard.boards.iter_mut().find(|board| board.id == board_id) {
            board.sync_status = SyncStatus::Conflict;
            board.shared_role = snapshot.permission.into();
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
                snapshot.permission,
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
    guard.boards[index].shared_role = snapshot.permission.into();
    guard.boards[index].name = snapshot.name.clone();
    guard.boards[index].task_count = snapshot
        .data
        .columns
        .iter()
        .map(|column| column.tasks.len() as i64)
        .sum();
    guard.boards[index].sync_status = SyncStatus::Synced;
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
        if permission != IrohPermission::Editor {
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
                permission,
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
        board.shared_role = permission.into();
        board.sync_revision = revision;
        board.sync_status = if keep_local {
            SyncStatus::Pending
        } else {
            SyncStatus::Synced
        };
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
