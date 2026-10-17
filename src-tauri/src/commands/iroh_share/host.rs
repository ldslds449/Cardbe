use super::*;
use crate::storage::{IrohShareContents, IrohShareDenied, IrohShareRead};
use std::sync::Arc;
use tauri::{Emitter, Manager};

pub(super) fn viewer_is_current(
    request: &Request,
    permission: IrohPermission,
    revision: i64,
) -> bool {
    permission == IrohPermission::Viewer
        && request.known_permission == Some(IrohPermission::Viewer)
        && request.known_revision == Some(revision)
}

/// How the host answers a request once permission and revision are known.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ShareResponseKind {
    /// Revision only; status probes from editors also carry a state vector.
    Unchanged,
    /// An accepted Loro sync with nothing further to send.
    LoroSyncAccepted,
    Full,
}

fn share_response_kind(
    request: &Request,
    error: &Option<String>,
    permission: IrohPermission,
    revision: i64,
) -> ShareResponseKind {
    if request.status_only || (error.is_none() && viewer_is_current(request, permission, revision))
    {
        ShareResponseKind::Unchanged
    } else if request.action == IrohAction::LoroSync && error.is_none() {
        ShareResponseKind::LoroSyncAccepted
    } else {
        ShareResponseKind::Full
    }
}

/// Build the host response for an authorized request. Reads run in one read
/// transaction on a separate connection without the application state lock;
/// only seeding a missing Loro document takes the lock, because it derives
/// the document from board rows that local edits update under that lock.
fn snapshot_from_db(
    app_handle: &AppHandle,
    path: &std::path::Path,
    board_id: i64,
    error: Option<String>,
    request: &Request,
    peer_id: &str,
) -> Result<Snapshot, String> {
    let mut db = Database::open(path.to_path_buf()).map_err(|e| e.to_string())?;
    let editor_invite = db
        .iroh_invite(&request.invite_id)
        .map_err(|e| e.to_string())?
        .is_some_and(|(_, id_board, _, permission, enabled)| {
            id_board == board_id && enabled && permission == IrohPermission::Editor
        });
    if editor_invite
        && db
            .iroh_loro_update(board_id)
            .map_err(|e| e.to_string())?
            .is_none()
    {
        let app_state = app_handle.state::<SharedAppData>();
        let _state = lock_app(&app_state)?;
        db.ensure_iroh_loro_doc(board_id)
            .map_err(|e| e.to_string())?;
    }
    let contents = db
        .read_iroh_share(
            board_id,
            &request.invite_id,
            &request.secret,
            peer_id,
            |permission, board| {
                let editor = permission == IrohPermission::Editor;
                match share_response_kind(request, &error, permission, board.sync_revision) {
                    ShareResponseKind::Unchanged => IrohShareRead {
                        loro_document: request.status_only && editor,
                        data: false,
                    },
                    ShareResponseKind::LoroSyncAccepted => IrohShareRead {
                        loro_document: false,
                        data: false,
                    },
                    ShareResponseKind::Full => IrohShareRead {
                        loro_document: editor,
                        data: true,
                    },
                }
            },
        )
        .map_err(|e| e.to_string())?
        // v1 peers classify these failures by their exact wording.
        .map_err(|denied| match denied {
            IrohShareDenied::DeviceNotApproved => {
                "Access was declined or revoked. You can request access again."
            }
            IrohShareDenied::InvitationUnavailable => "Invitation was revoked or disabled",
            IrohShareDenied::BoardDeleted => "Board was deleted",
        })?;
    let IrohShareContents {
        board,
        permission,
        loro_document,
        data,
    } = contents;
    let kind = share_response_kind(request, &error, permission, board.sync_revision);
    let loro_update = match (kind, loro_document) {
        // Status probes let editors upload only history the owner lacks.
        (ShareResponseKind::Unchanged, Some(document)) => {
            crate::loro_board::state_vector(Some(&document))?
        }
        (ShareResponseKind::Full, Some(document)) => crate::loro_board::share_snapshot(&document)?,
        _ => Vec::new(),
    };
    if kind != ShareResponseKind::Full {
        return Ok(Snapshot {
            ok: true,
            error: None,
            error_code: None,
            name: board.name,
            permission,
            revision: board.sync_revision,
            data: StoredData::default(),
            loro_update,
            unchanged: kind == ShareResponseKind::Unchanged,
        });
    }
    let mut data = data.ok_or("Board snapshot was not read")?;
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

enum EditorPush {
    Applied {
        snapshot: Box<Snapshot>,
        /// Owner revision after the push when it changed the board.
        pushed_revision: Option<i64>,
    },
    Unauthorized(String),
}

const EDITOR_PUSH_ATTEMPTS: usize = 3;

/// Error text v1 editors receive when their push is no longer authorized.
/// Keep it unchanged until capability negotiation supports error codes.
const EDITING_REVOKED_WIRE_ERROR: &str = "Invitation no longer grants editing access";

/// Merge an editor's Loro update into an owned board and return the new
/// revision when it changed the board. The merge runs on a blocking worker
/// without the application state lock, so a large update does not stall
/// local commands. The lock covers only the commit and the in-memory reload,
/// which keeps local edits ordered relative to the remote write. A commit is
/// skipped when the document changed after merging; the final attempt merges
/// under the lock so the update always makes progress. A push the invitation
/// no longer authorizes fails with `DomainError::PermissionDenied`.
pub(super) async fn merge_editor_update(
    app_handle: &AppHandle,
    path: &std::path::Path,
    board_id: i64,
    invite_id: &str,
    secret: &str,
    peer_id: &str,
    update: Vec<u8>,
) -> Result<Option<i64>, DomainError> {
    let credentials = Arc::new((invite_id.to_owned(), secret.to_owned(), peer_id.to_owned()));
    let update = Arc::new(update);
    for attempt in 1..=EDITOR_PUSH_ATTEMPTS {
        let prepared = if attempt < EDITOR_PUSH_ATTEMPTS {
            let (path, credentials, update) =
                (path.to_path_buf(), credentials.clone(), update.clone());
            let prepared = tauri::async_runtime::spawn_blocking(move || {
                let (invite_id, secret, peer_id) = &*credentials;
                Database::open(path)
                    .and_then(|db| {
                        db.prepare_iroh_loro_update(board_id, invite_id, secret, peer_id, &update)
                    })
                    .map_err(DomainError::repository)
            })
            .await
            .map_err(|error| DomainError::Internal(error.to_string()))??;
            Some(prepared)
        } else {
            None
        };
        let (app_handle, path, credentials, update) = (
            app_handle.clone(),
            path.to_path_buf(),
            credentials.clone(),
            update.clone(),
        );
        let committed = tauri::async_runtime::spawn_blocking(move || {
            let app_state = app_handle.state::<SharedAppData>();
            let mut state = lock_app(&app_state)?;
            let (invite_id, secret, peer_id) = &*credentials;
            let mut db = Database::open(path).map_err(DomainError::repository)?;
            let prepared = match prepared {
                Some(prepared) => prepared,
                None => db
                    .prepare_iroh_loro_update(board_id, invite_id, secret, peer_id, &update)
                    .map_err(DomainError::repository)?,
            };
            let Some((changed, revision, _)) = db
                .commit_iroh_loro_update(prepared, invite_id, secret, peer_id)
                .map_err(DomainError::repository)?
            else {
                return Ok(None);
            };
            if changed {
                let fresh = db
                    .boards()
                    .map_err(DomainError::repository)?
                    .into_iter()
                    .find(|board| board.id == board_id)
                    .ok_or("Board was deleted")?;
                if let Some(board) = state.boards.iter_mut().find(|board| board.id == board_id) {
                    *board = fresh;
                }
                if state.active_board_id == board_id {
                    state
                        .load_active_board(board_id)
                        .map_err(DomainError::repository)?;
                }
            }
            Ok::<_, DomainError>(Some(changed.then_some(revision)))
        })
        .await
        .map_err(|error| DomainError::Internal(error.to_string()))??;
        if let Some(revision) = committed {
            return Ok(revision);
        }
        log::debug!(target: "iroh", "Shared board changed while merging an editor update; merging again");
    }
    Err("Editor update did not commit".into())
}

/// Apply an editor's push and answer with the owner's changes since the
/// editor's state vector.
async fn apply_editor_push(
    app_handle: &AppHandle,
    path: &std::path::Path,
    board_id: i64,
    request: &Request,
    peer_id: &str,
) -> Result<EditorPush, String> {
    let pushed_revision = match merge_editor_update(
        app_handle,
        path,
        board_id,
        &request.invite_id,
        &request.secret,
        peer_id,
        request.loro_update.clone(),
    )
    .await
    {
        Ok(revision) => revision,
        Err(DomainError::PermissionDenied) => {
            return Ok(EditorPush::Unauthorized(EDITING_REVOKED_WIRE_ERROR.into()))
        }
        Err(error) => return Err(error.to_string()),
    };
    // The response diff is computed after releasing the lock; it covers the
    // owner's current document, including any later local edits.
    let (path, state_vector) = (path.to_path_buf(), request.loro_state_vector.clone());
    let snapshot = tauri::async_runtime::spawn_blocking(move || -> Result<Snapshot, String> {
        let db = Database::open(path).map_err(|error| error.to_string())?;
        let owner_update = db
            .iroh_loro_update(board_id)
            .map_err(|error| error.to_string())?
            .unwrap_or_default();
        let loro_update = crate::loro_board::diff(Some(&owner_update), &state_vector)?;
        let fresh = db
            .boards()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|board| board.id == board_id)
            .ok_or("Board was deleted")?;
        Ok(Snapshot {
            ok: true,
            error: None,
            error_code: None,
            name: fresh.name,
            permission: IrohPermission::Editor,
            revision: fresh.sync_revision,
            data: StoredData::default(),
            loro_update,
            unchanged: false,
        })
    })
    .await
    .map_err(|error| error.to_string())??;
    Ok(EditorPush::Applied {
        snapshot: Box::new(snapshot),
        pushed_revision,
    })
}

pub(super) async fn ensure_host(
    state: &IrohShareState,
    path: std::path::PathBuf,
    app_handle: AppHandle,
) -> Result<Endpoint, ShareError> {
    let _init = state.init.lock().await;
    {
        let app = app_handle.state::<SharedAppData>();
        let guard = lock_app(&app)?;
        if !guard.stored.settings.iroh_network.enabled {
            return Err(ShareError::IrohDisabled);
        }
    }
    if let Some(hosted) = state
        .hosted
        .lock()
        .map_err(|_| "Sharing service state is unavailable")?
        .as_ref()
    {
        return Ok(hosted.endpoint.clone());
    }
    let (key, saved_port) = {
        let mut db = Database::open(path.clone()).map_err(|e| e.to_string())?;
        let saved_port = db.iroh_automatic_port().unwrap_or_else(|error| {
            log::warn!(target: "iroh", "Could not read the saved sharing port: {error}");
            None
        });
        (device_key(&mut db)?, saved_port)
    };
    restore_automatic_port(state, saved_port)?;
    let config = {
        let app = app_handle.state::<SharedAppData>();
        let guard = lock_app(&app)?;
        IrohNetworkConfig::from_settings(&guard.stored.settings.iroh_network)
            .map_err(|error| error.to_string())?
    };
    log::debug!(target: "iroh", "Starting Iroh endpoint: direct IP {}, discovery {}, discovery services {:?}, relay {:?}", config.direct_ip_enabled, config.discovery_enabled, config.discovery_urls, config.relay_mode);
    let endpoint = bind_endpoint(&config, key, state).await?;
    persist_automatic_port(state, &path, saved_port);
    let online_endpoint = endpoint.clone();
    tauri::async_runtime::spawn(async move {
        if tokio::time::timeout(CONNECT_TIMEOUT, online_endpoint.online())
            .await
            .is_err()
        {
            log::debug!(target: "iroh", "No relay became online during startup; direct and discovery paths remain available");
        } else {
            log::debug!(target: "iroh", "A relay is online; direct paths remain available and Iroh can select the best path");
        }
    });
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
            let path = path.clone();
            let app_handle = app_handle.clone();
            let connection_endpoint = accept_endpoint.clone();
            tauri::async_runtime::spawn(async move {
                let _slot = slot;
                let connection = match tokio::time::timeout(CONNECT_TIMEOUT, connecting).await {
                    Ok(Ok(connection)) => connection,
                    Ok(Err(error)) => {
                        log::warn!(target: "iroh", "Shared-board connection failed: {error}");
                        return;
                    }
                    Err(error) => {
                        log::warn!(target: "iroh", "Shared-board connection timed out: {error}");
                        return;
                    }
                };
                record_connection_path(
                    &connection_endpoint,
                    connection.remote_id(),
                    &app_handle.state::<IrohShareState>(),
                )
                .await;
                let peer_id = URL_SAFE_NO_PAD.encode(connection.remote_id().as_bytes());

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
                    // Read-only: local edits persist before releasing the
                    // application state lock, so one read transaction on a
                    // separate connection sees a consistent board without it.
                    Ok(request) if request.owner_pull => Database::open(path.clone())
                        .map_err(|e| e.to_string())
                        .and_then(|db| {
                            db.read_consistent(|db| editor_snapshot_for_owner(db, &request, &peer_id))
                                .map_err(|e| e.to_string())?
                                .map_err(|_| "Owner-initiated sync request rejected".to_string())
                        }),
                    Ok(request) => match Database::open(path.clone())
                        .map_err(|e| e.to_string())
                        .and_then(|mut db| {
                            let invite = db.iroh_invite(&request.invite_id).map_err(|e| e.to_string())?
                                .filter(|(_, _, secret, _, _)| secrets_match(secret, &request.secret))
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
                        Ok(invite) if request.status_only || matches!(request.action, IrohAction::Pull | IrohAction::Request) => {
                            snapshot_from_db(&app_handle, &path, invite.board_id, None, &request, &peer_id)
                        }
                        Ok(invite)
                            if request.action == IrohAction::LoroSync && invite.permission == IrohPermission::Editor =>
                        {
                            match apply_editor_push(&app_handle, &path, invite.board_id, &request, &peer_id).await {
                                Ok(EditorPush::Applied { snapshot, pushed_revision }) => {
                                    applied_push = pushed_revision.map(|revision| (invite.board_id, revision));
                                    Ok(*snapshot)
                                }
                                Ok(EditorPush::Unauthorized(error)) => snapshot_from_db(
                                    &app_handle, &path, invite.board_id,
                                    Some(error), &request, &peer_id,
                                ),
                                Err(error) => Err(error),
                            }
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
                // v1 peers reject unknown enum variants. Keep the existing wire
                // format until capability negotiation supports additional codes.
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
                record_connection_path(
                    &connection_endpoint,
                    connection.remote_id(),
                    &app_handle.state::<IrohShareState>(),
                )
                .await;
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
    if !app_handle
        .state::<SharedAppData>()
        .lock()
        .map(|guard| guard.stored.settings.iroh_network.enabled)
        .unwrap_or(false)
    {
        return;
    }
    let has_invites = match Database::open(path.clone()).and_then(|db| db.iroh_host_required()) {
        Ok(required) => required,
        Err(error) => {
            log::error!(target: "iroh", "Could not inspect saved invitations while restoring the board-sharing service: {error}");
            return;
        }
    };
    if has_invites {
        if let Err(error) = ensure_host(network, path, app_handle).await {
            log::error!(target: "iroh", "Could not restore the board-sharing service: {error}");
            set_last_error(network, Some(error.to_string()));
        }
    }
}

#[tauri::command]
pub fn iroh_host_error(network: State<'_, IrohShareState>) -> Option<CommandError> {
    match network.last_error.lock() {
        Ok(error) => error.as_ref().map(|_| CommandError::ShareFailed),
        Err(error) => {
            log::error!(target: "iroh", "Could not read the board-sharing service error state: {error}");
            Some(CommandError::InternalError)
        }
    }
}

#[tauri::command]
pub async fn ensure_iroh_host(
    app: State<'_, SharedAppData>,
    network: State<'_, IrohShareState>,
    app_handle: AppHandle,
) -> Result<(), CommandError> {
    let (path, active) = {
        let guard = lock_app(&app)?;
        (
            guard.database.path().to_path_buf(),
            guard.stored.settings.iroh_network.enabled
                && guard
                    .database
                    .iroh_host_required()
                    .map_err(CommandError::repository)?,
        )
    };
    if active {
        if let Err(error) = ensure_host(&network, path, app_handle).await {
            set_last_error(&network, Some(error.to_string()));
            return Err(error.into());
        }
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
            .and_then(|db| Ok(!db.iroh_remote_tickets()?.is_empty() || db.iroh_host_required()?))
            .map_err(|e| e.to_string())?;
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

pub(crate) async fn restart_host_if_running(
    network: &IrohShareState,
    app_handle: AppHandle,
) -> Result<(), String> {
    let _init = network.init.lock().await;
    let hosted = network
        .hosted
        .lock()
        .map_err(|_| "Sharing service state is unavailable")?
        .take();
    network
        .connections
        .lock()
        .map_err(|_| "Connection details are unavailable")?
        .clear();
    let enabled = lock_app(&app_handle.state::<SharedAppData>())?
        .stored
        .settings
        .iroh_network
        .enabled;
    let path = if let Some(hosted) = hosted {
        let path = hosted.path;
        hosted.endpoint.close().await;
        if !enabled {
            set_last_error(network, None);
            return Ok(());
        }
        path
    } else {
        if !enabled {
            set_last_error(network, None);
            return Ok(());
        }
        let app = app_handle.state::<SharedAppData>();
        let guard = lock_app(&app)?;
        if guard
            .database
            .iroh_remote_tickets()
            .map_err(|error| error.to_string())?
            .is_empty()
            && !guard
                .database
                .iroh_host_required()
                .map_err(|error| error.to_string())?
        {
            return Ok(());
        }
        guard.database.path().to_path_buf()
    };
    drop(_init);
    if let Err(error) = ensure_host(network, path, app_handle).await {
        set_last_error(network, Some(error.to_string()));
        return Err(error.to_string());
    }
    Ok(())
}

/// Prefer the port saved by a previous launch when this process has not bound
/// an automatic port yet. `bind_endpoint` falls back if it is now occupied.
pub(super) fn restore_automatic_port(
    state: &IrohShareState,
    saved: Option<u16>,
) -> Result<(), String> {
    let mut port = state
        .automatic_direct_port
        .lock()
        .map_err(|_| "Sharing service state is unavailable")?;
    if port.is_none() {
        *port = saved;
    }
    Ok(())
}

fn persist_automatic_port(state: &IrohShareState, path: &std::path::Path, saved: Option<u16>) {
    let Ok(current) = state.automatic_direct_port.lock().map(|port| *port) else {
        return;
    };
    let Some(port) = current.filter(|port| Some(*port) != saved) else {
        return;
    };
    if let Err(error) =
        Database::open(path.to_path_buf()).and_then(|mut db| db.set_iroh_automatic_port(port))
    {
        log::warn!(target: "iroh", "Could not save the sharing port: {error}");
    }
}

pub(super) async fn bind_endpoint(
    config: &IrohNetworkConfig,
    key: SecretKey,
    network: &IrohShareState,
) -> Result<Endpoint, String> {
    let automatic_port = config.direct_ip_enabled && config.listen_port == 0;
    let mut binding = config.clone();
    if automatic_port {
        binding.listen_port = network
            .automatic_direct_port
            .lock()
            .map_err(|_| "Sharing service state is unavailable")?
            .unwrap_or(0);
    }
    let endpoint = endpoint_builder(&binding)?
        .secret_key(key.clone())
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await;
    let endpoint = match endpoint {
        Ok(endpoint) => endpoint,
        Err(error) if automatic_port && binding.listen_port != 0 => {
            // An in-flight sync can still hold the closed endpoint's UDP socket.
            // Keep automatic allocation available; discovery refreshes the new address.
            log::warn!(target: "iroh", "Could not reuse automatic UDP port {}: {error}; choosing an available port", binding.listen_port);
            endpoint_builder(config)?
                .secret_key(key)
                .alpns(vec![ALPN.to_vec()])
                .bind()
                .await
                .map_err(|error| error.to_string())?
        }
        Err(error) => return Err(error.to_string()),
    };
    if automatic_port {
        *network
            .automatic_direct_port
            .lock()
            .map_err(|_| "Sharing service state is unavailable")? = endpoint
            .bound_sockets()
            .iter()
            .find(|address| address.is_ipv4())
            .map(SocketAddr::port);
    }
    Ok(endpoint)
}

// Only the authenticated owner of this exact invitation may read an editor's
// unsent CRDT operations. Reading leaves the editor's pending state untouched.
/// Compare an invitation secret received from a peer without an early exit
/// on the first differing byte. Hashing first gives both sides a fixed length.
pub(super) fn secrets_match(expected: &str, provided: &str) -> bool {
    use sha2::{Digest, Sha256};
    Sha256::digest(expected.as_bytes())
        .iter()
        .zip(Sha256::digest(provided.as_bytes()).iter())
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

pub(super) fn editor_snapshot_for_owner(
    db: &Database,
    request: &Request,
    peer_id: &str,
) -> Result<Snapshot, ShareError> {
    if request.action != IrohAction::LoroSync || !request.loro_update.is_empty() {
        return Err(DomainError::InvalidArgument.into());
    }
    for board in db.boards().map_err(DomainError::repository)? {
        if board.shared_role != BoardRole::Editor || board.sync_status == SyncStatus::Conflict {
            continue;
        }
        let Some(saved) = db.iroh_remote(board.id).map_err(DomainError::repository)? else {
            continue;
        };
        let remote: RemoteInvitation = serde_json::from_str(&saved)
            .map_err(|_| ShareError::Internal("Stored invitation is invalid".into()))?;
        if remote.node_id != peer_id
            || remote.invite_id != request.invite_id
            || !secrets_match(&remote.secret, &request.secret)
        {
            continue;
        }
        let update = db
            .iroh_loro_update(board.id)
            .map_err(DomainError::repository)?
            .ok_or_else(|| {
                ShareError::Internal("Editable board is missing its Loro document".into())
            })?;
        return Ok(Snapshot {
            ok: true,
            error: None,
            error_code: None,
            name: board.name,
            permission: IrohPermission::Editor,
            revision: board.sync_revision,
            data: StoredData::default(),
            loro_update: crate::loro_board::diff(Some(&update), &request.loro_state_vector)?,
            unchanged: false,
        });
    }
    Err(ShareError::AccessRevoked)
}
