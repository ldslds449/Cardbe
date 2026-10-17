use super::*;

pub(super) fn ensure_invite_not_joined(db: &Database, ticket: &Ticket) -> Result<(), DomainError> {
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
        return Err(DomainError::InviteAlreadyJoined);
    }
    Ok(())
}

pub(super) fn update_iroh_connection_inner(
    db: &Database,
    board_id: i64,
    ticket: &str,
) -> Result<(), DomainError> {
    let ticket = parse_ticket(ticket)?;
    let saved = db
        .iroh_remote(board_id)
        .map_err(DomainError::repository)?
        .ok_or(DomainError::InviteNotFound)?;
    let mut remote: RemoteInvitation = serde_json::from_str(&saved).map_err(|error| {
        DomainError::Internal(format!(
            "Could not parse stored shared-board invitation: {error}"
        ))
    })?;
    if remote.node_id != ticket.node_id
        || remote.invite_id != ticket.invite_id
        || remote.secret != ticket.secret
    {
        return Err(DomainError::InviteInvalid);
    }
    remote.endpoint = ticket.endpoint;
    remote.relay_urls = ticket.relay_urls;
    let saved =
        serde_json::to_string(&remote).map_err(|error| DomainError::Internal(error.to_string()))?;
    db.update_iroh_remote(board_id, &saved)
        .map_err(DomainError::repository)
}

#[tauri::command]
pub fn update_iroh_connection(
    app: State<'_, SharedAppData>,
    board_id: i64,
    ticket: String,
) -> Result<(), CommandError> {
    let guard = lock_app(&app)?;
    update_iroh_connection_inner(&guard.database, board_id, &ticket).map_err(CommandError::from)
}

#[tauri::command]
pub async fn join_iroh_invite(
    app: State<'_, SharedAppData>,
    app_handle: AppHandle,
    network: State<'_, IrohShareState>,
    ticket: String,
    request_approval: bool,
) -> Result<Board, CommandError> {
    join_iroh_invite_inner(app, app_handle, network, ticket, request_approval)
        .await
        .map_err(CommandError::from)
}

async fn join_iroh_invite_inner(
    app: State<'_, SharedAppData>,
    app_handle: AppHandle,
    network: State<'_, IrohShareState>,
    ticket: String,
    request_approval: bool,
) -> Result<Board, ShareError> {
    let ticket = parse_ticket(&ticket)?;
    let address = endpoint_address(
        ticket.endpoint.as_ref(),
        &ticket.node_id,
        &ticket.relay_urls,
    )?;
    let key = {
        let mut guard = lock_app(&app)?;
        ensure_invite_not_joined(&guard.database, &ticket)?;
        device_key(&mut guard.database)?
    };
    let device_id = URL_SAFE_NO_PAD.encode(key.public().as_bytes());
    let path = lock_app(&app)?.database.path().to_path_buf();
    let endpoint = ensure_host(&network, path, app_handle).await?;
    let connection = connect(&endpoint, address, &network).await?;
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
                status_only: false,
                owner_pull: false,
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
    record_connection_path(&endpoint, connection.remote_id(), &network).await;
    let snapshot: Snapshot = serde_json::from_slice(&bytes)
        .map_err(|error| {
            log::error!(target: "iroh", "Could not parse the shared-board invitation response: {error}");
            "Invitation was revoked or the owner sent an invalid snapshot"
        })?;
    if !snapshot.ok {
        return Err(snapshot_failure(&snapshot, device_id));
    }
    let mut guard = lock_app(&app)?;
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
        .map_err(DomainError::repository)?;
    guard.boards.push(board.clone());
    Ok(board)
}

#[tauri::command]
pub async fn request_iroh_board_access(
    app: State<'_, SharedAppData>,
    app_handle: AppHandle,
    network: State<'_, IrohShareState>,
    board_id: i64,
    request_approval: bool,
) -> Result<(bool, String, Option<Board>), CommandError> {
    request_iroh_board_access_inner(app, app_handle, network, board_id, request_approval)
        .await
        .map_err(CommandError::from)
}

async fn request_iroh_board_access_inner(
    app: State<'_, SharedAppData>,
    app_handle: tauri::AppHandle,
    network: State<'_, IrohShareState>,
    board_id: i64,
    _request_approval: bool,
) -> Result<(bool, String, Option<Board>), ShareError> {
    let (remote, key) = {
        let mut guard = lock_app(&app)?;
        let remote = guard
            .database
            .iroh_remote(board_id)
            .map_err(DomainError::repository)?
            .ok_or(DomainError::InvalidArgument)?;
        (serde_json::from_str::<RemoteInvitation>(&remote).map_err(|error| {
            log::warn!(target: "iroh", "Could not parse the stored shared-board invitation for an access request: {error}");
            "Stored invitation is invalid"
        })?,
            device_key(&mut guard.database)?,
        )
    };
    let device_id = URL_SAFE_NO_PAD.encode(key.public().as_bytes());
    let address = endpoint_address(
        remote.endpoint.as_ref(),
        &remote.node_id,
        &remote.relay_urls,
    )?;
    let path = lock_app(&app)?.database.path().to_path_buf();
    let endpoint = ensure_host(&network, path, app_handle).await?;
    let response = tokio::time::timeout(TRANSFER_TIMEOUT, async {
        let connection = connect(&endpoint, address.clone(), &network).await?;
        let (mut send, mut recv) = connection.open_bi().await.map_err(|e| e.to_string())?;
        let request = Request {
            invite_id: remote.invite_id.clone(),
            secret: remote.secret.clone(),
            // Existing copies must observe revocation before requesting access again.
            // Only joining an invitation may reset a revoked device to pending.
            action: IrohAction::Pull,
            loro_state_vector: Vec::new(),
            loro_update: Vec::new(),
            known_revision: None,
            known_permission: None,
            status_only: true,
            owner_pull: false,
        };
        send.write_all(&serde_json::to_vec(&request).map_err(|e| e.to_string())?)
            .await
            .map_err(|e| e.to_string())?;
        send.finish().map_err(|e| e.to_string())?;
        let bytes = recv
            .read_to_end(MAX_MESSAGE)
            .await
            .map_err(|e| e.to_string())?;
        record_connection_path(&endpoint, connection.remote_id(), &network).await;
        Ok::<_, String>(bytes)
    })
    .await
    .map_err(|_| "Access request timed out".to_string())??;
    let snapshot: Snapshot = serde_json::from_slice(&response).map_err(|error| {
        log::error!(target: "iroh", "Could not parse the shared-board access response: {error}");
        "Owner sent an invalid response"
    })?;
    if snapshot.ok {
        let refresh_conflict = {
            let guard = lock_app(&app)?;
            guard
                .database
                .iroh_conflict(board_id)
                .map_err(DomainError::repository)?
                .is_some_and(|conflict| conflict.permission != snapshot.permission)
        };
        let mut refreshed = None;
        if refresh_conflict {
            let request = Request {
                invite_id: remote.invite_id,
                secret: remote.secret,
                action: IrohAction::Pull,
                loro_state_vector: Vec::new(),
                loro_update: Vec::new(),
                known_revision: None,
                known_permission: None,
                status_only: false,
                owner_pull: false,
            };
            let owner = exchange_sync_request(&endpoint, address, &network, &request).await?;
            let mut guard = lock_app(&app)?;
            refreshed =
                refresh_saved_conflict(&mut guard, board_id, &owner).map_err(
                    |error| match error {
                        ShareError::ApprovalRequired { .. } => ShareError::ApprovalRequired {
                            device_id: device_id.clone(),
                        },
                        error => error,
                    },
                )?;
        }
        return Ok((true, device_id, refreshed));
    }
    if snapshot_requires_approval(&snapshot) {
        return Ok((false, device_id, None));
    }
    Err(snapshot_failure(&snapshot, device_id))
}

pub(super) async fn exchange_sync_request(
    endpoint: &Endpoint,
    address: EndpointAddr,
    network: &IrohShareState,
    request: &Request,
) -> Result<Snapshot, ShareError> {
    let bytes = tokio::time::timeout(TRANSFER_TIMEOUT, async {
        let connection = connect(endpoint, address, network).await?;
        let (mut send, mut recv) = connection.open_bi().await.map_err(|e| e.to_string())?;
        let encoded = serde_json::to_vec(request).map_err(|e| e.to_string())?;
        if encoded.len() > MAX_MESSAGE {
            return Err(DomainError::ShareLimitExceeded.into());
        }
        send.write_all(&encoded).await.map_err(|e| e.to_string())?;
        send.finish().map_err(|e| e.to_string())?;
        let bytes = recv
            .read_to_end(MAX_MESSAGE)
            .await
            .map_err(|e| e.to_string())?;
        record_connection_path(endpoint, connection.remote_id(), network).await;
        Ok::<_, ShareError>(bytes)
    })
    .await
    .map_err(|_| "Shared-board sync timed out after 60 seconds".to_string())??;
    serde_json::from_slice(&bytes).map_err(|_| "Owner sent an invalid response".into())
}

async fn sync_owned_iroh_board(
    app: &SharedAppData,
    app_handle: AppHandle,
    network: &IrohShareState,
    board_id: i64,
) -> Result<IrohSyncResult, ShareError> {
    let (path, peers) = {
        let guard = lock_app(app)?;
        ensure_board_owned(guard.boards.iter().find(|board| board.id == board_id))?;
        let invites = guard
            .database
            .iroh_invites()
            .map_err(DomainError::repository)?;
        let devices = guard
            .database
            .iroh_devices()
            .map_err(DomainError::repository)?;
        let mut peers = Vec::new();
        // ponytail: scan saved invites/devices; use a SQL join if peer lists grow large.
        for (invite_id, id, secret, permission, enabled) in invites {
            if id != board_id || !enabled || permission != IrohPermission::Editor {
                continue;
            }
            for (device_invite, node_id, status) in &devices {
                if device_invite == &invite_id && *status == IrohDeviceStatus::Approved {
                    peers.push((node_id.clone(), (invite_id.clone(), secret.clone())));
                }
            }
        }
        (guard.database.path().to_path_buf(), peers)
    };
    let endpoint = ensure_host(network, path.clone(), app_handle.clone()).await?;
    let mut content_changed = false;
    let mut outcomes = HashMap::new();
    for (node_id, (invite_id, secret)) in peers {
        let result = async {
            let update = {
                let guard = lock_app(app)?;
                guard
                    .database
                    .iroh_loro_update(board_id)
                    .map_err(DomainError::repository)?
                    .ok_or_else(|| {
                        ShareError::Internal("Shared board is missing its Loro document".into())
                    })?
            };
            let mut address = endpoint_address(None, &node_id, &[])?;
            if let Some(info) = endpoint.remote_info(address.id).await {
                address = EndpointAddr::from_parts(
                    address.id,
                    info.addrs().map(|addr| addr.addr().clone()),
                );
            }
            let request = Request {
                invite_id: invite_id.clone(),
                secret: secret.clone(),
                action: IrohAction::LoroSync,
                loro_state_vector: crate::loro_board::state_vector(Some(&update))?,
                loro_update: Vec::new(),
                known_revision: None,
                known_permission: None,
                status_only: false,
                owner_pull: true,
            };
            let response = exchange_sync_request(&endpoint, address, network, &request).await?;
            if !response.ok || response.unchanged || response.permission != IrohPermission::Editor {
                return Err(ShareError::AccessRevoked);
            }
            // The merge rechecks approval when committing, including
            // revocation or permission changes while the request was in flight.
            let pushed = merge_editor_update(
                &app_handle,
                &path,
                board_id,
                &invite_id,
                &secret,
                &node_id,
                response.loro_update,
            )
            .await?;
            content_changed |= pushed.is_some();
            Ok::<_, ShareError>(())
        }
        .await;
        outcomes
            .entry(node_id)
            .and_modify(|ok| *ok &= result.is_ok())
            .or_insert(result.is_ok());
        if let Err(error) = result {
            log::debug!(target: "iroh", "Owner-initiated board sync failed: {}",
                crate::commands::diagnostics::redact_diagnostic_logs(format!("{error:?}")));
        }
    }
    let synced_devices = outcomes.values().filter(|ok| **ok).count();
    let summary = OwnerSyncSummary {
        synced_devices,
        failed_devices: outcomes.len() - synced_devices,
    };
    let guard = lock_app(app)?;
    let board = guard
        .boards
        .iter()
        .find(|board| board.id == board_id)
        .cloned()
        .ok_or(DomainError::BoardNotFound)?;
    Ok(IrohSyncResult {
        board,
        content_changed,
        owner_sync: Some(summary),
    })
}

#[tauri::command]
pub async fn sync_iroh_board(
    app: State<'_, SharedAppData>,
    app_handle: AppHandle,
    network: State<'_, IrohShareState>,
    board_id: i64,
) -> Result<IrohSyncResult, CommandError> {
    sync_iroh_board_inner(app, app_handle, network, board_id)
        .await
        .map_err(CommandError::from)
}

async fn sync_iroh_board_inner(
    app: State<'_, SharedAppData>,
    app_handle: tauri::AppHandle,
    network: State<'_, IrohShareState>,
    board_id: i64,
) -> Result<IrohSyncResult, ShareError> {
    let owned = {
        let guard = lock_app(&app)?;
        guard
            .boards
            .iter()
            .find(|board| board.id == board_id)
            .ok_or(DomainError::BoardNotFound)?
            .shared_role
            == BoardRole::Owner
    };
    if owned {
        return sync_owned_iroh_board(&app, app_handle, &network, board_id).await;
    }
    let (ticket_text, role, revision, status, data, loro_update) = {
        let guard = lock_app(&app)?;
        (
            guard
                .database
                .iroh_remote(board_id)
                .map_err(DomainError::repository)?
                .ok_or(DomainError::InvalidArgument)?,
            guard
                .database
                .board_role(board_id)
                .map_err(DomainError::repository)?,
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
                .map_err(DomainError::repository)?,
            guard
                .database
                .iroh_loro_update(board_id)
                .map_err(DomainError::repository)?
                .unwrap_or_default(),
        )
    };
    let recovering_conflict = if status == SyncStatus::Conflict {
        let guard = lock_app(&app)?;
        guard
            .database
            .iroh_conflict(board_id)
            .map_err(DomainError::repository)?
            .is_some_and(|conflict| {
                conflict.permission == IrohPermission::Editor
                    && conflict
                        .loro_update
                        .as_ref()
                        .is_none_or(|bytes| bytes.is_empty())
            })
    } else {
        false
    };
    if status == SyncStatus::Conflict && !recovering_conflict {
        return Err(ShareError::SyncConflict);
    }
    let ticket: RemoteInvitation = serde_json::from_str(&ticket_text).map_err(|error| {
        log::warn!(target: "iroh", "Could not parse the stored shared-board invitation for sync: {error}");
        "Stored invitation is invalid"
    })?;
    let address = endpoint_address(ticket.endpoint.as_ref(), &ticket.node_id, &ticket.relay_urls)
        .map_err(|error| {
            log::warn!(target: "iroh", "Stored shared-board invitation has an invalid endpoint: {error}");
            "Stored invitation is invalid"
        })?;
    let loro_state_vector = crate::loro_board::state_vector(Some(&loro_update))?;
    if role == BoardRole::Editor && loro_update.is_empty() && !recovering_conflict {
        return Err("Editable board is missing its Loro document. Rejoin the invitation.".into());
    }
    let path = lock_app(&app)?.database.path().to_path_buf();
    let endpoint = ensure_host(&network, path, app_handle).await?;
    let device_id = URL_SAFE_NO_PAD.encode(endpoint.id().as_bytes());
    // Editors always exchange their Loro state when a document exists. A clean
    // editor still needs remote changes merged into its local CRDT history.
    let pushing = role == BoardRole::Editor && !recovering_conflict;
    let permission = match role {
        BoardRole::Viewer => IrohPermission::Viewer,
        BoardRole::Editor => IrohPermission::Editor,
        BoardRole::Owner => return Err(DomainError::InvalidArgument.into()),
    };
    let mut request = Request {
        invite_id: ticket.invite_id,
        secret: ticket.secret,
        action: if pushing {
            IrohAction::LoroSync
        } else {
            IrohAction::Pull
        },
        loro_state_vector,
        // Pending editors probe the owner before constructing their update.
        loro_update: Vec::new(),
        known_revision: Some(revision),
        known_permission: Some(permission),
        status_only: false,
        owner_pull: false,
    };
    if pushing && needs_editor_upload(role, status) {
        request.action = IrohAction::Pull;
        request.status_only = true;
        request.loro_update.clear();
        let owner = exchange_sync_request(&endpoint, address.clone(), &network, &request).await?;
        validate_sync_snapshot(&owner).map_err(|_| snapshot_failure(&owner, device_id.clone()))?;
        if !owner.ok {
            return Err(snapshot_failure(&owner, device_id.clone()));
        }
        request.loro_update = editor_upload(&loro_update, &owner)?;
        request.status_only = false;
        request.action = IrohAction::LoroSync;
    }
    let used_loro_sync = request.action == IrohAction::LoroSync;
    let snapshot = exchange_sync_request(&endpoint, address, &network, &request).await?;
    validate_sync_snapshot(&snapshot)
        .map_err(|_| snapshot_failure(&snapshot, device_id.clone()))?;
    let mut guard = lock_app(&app)?;
    if recovering_conflict {
        if !snapshot.ok || snapshot.unchanged {
            return Err(snapshot_failure(&snapshot, device_id.clone()));
        }
        let still_legacy = guard
            .database
            .iroh_conflict(board_id)
            .map_err(DomainError::repository)?
            .is_some_and(|conflict| {
                conflict.permission == IrohPermission::Editor
                    && conflict
                        .loro_update
                        .as_ref()
                        .is_none_or(|bytes| bytes.is_empty())
            });
        if !still_legacy {
            return Err(
                "The saved conflict changed while fetching the owner version. Try again".into(),
            );
        }
        guard
            .database
            .save_iroh_conflict(
                board_id,
                snapshot.revision,
                &snapshot.name,
                snapshot.permission,
                &snapshot.data,
                Some(&snapshot.loro_update),
            )
            .map_err(DomainError::repository)?;
        return guard
            .boards
            .iter()
            .find(|board| board.id == board_id)
            .cloned()
            .map(|board| IrohSyncResult {
                board,
                content_changed: false,
                owner_sync: None,
            })
            .ok_or_else(|| "Board not found".into());
    }
    if snapshot.unchanged {
        let state = guard
            .database
            .board_sync_state(board_id)
            .map_err(DomainError::repository)?;
        if !snapshot.ok
            || role != BoardRole::Viewer
            || snapshot.permission != IrohPermission::Viewer
            || snapshot.revision != revision
            || state.0 != revision
            || state.1 != status
            || guard
                .database
                .board_role(board_id)
                .map_err(DomainError::repository)?
                != BoardRole::Viewer
        {
            return Err("Owner sent an invalid unchanged response".into());
        }
        return guard
            .boards
            .iter()
            .find(|board| board.id == board_id)
            .cloned()
            .map(|board| IrohSyncResult {
                board,
                content_changed: false,
                owner_sync: None,
            })
            .ok_or_else(|| "Board not found".into());
    }
    if used_loro_sync && snapshot.ok {
        let content_changed = guard
            .database
            .merge_iroh_loro_diff(
                board_id,
                &snapshot.loro_update,
                &snapshot.name,
                snapshot.permission,
                snapshot.revision,
                &loro_update,
            )
            .map_err(DomainError::repository)?;
        let board = guard
            .database
            .boards()
            .map_err(DomainError::repository)?
            .into_iter()
            .find(|b| b.id == board_id)
            .ok_or(DomainError::BoardNotFound)?;
        if let Some(item) = guard.boards.iter_mut().find(|item| item.id == board_id) {
            *item = board.clone();
        }
        if guard.active_board_id == board_id && content_changed {
            guard
                .load_active_board(board_id)
                .map_err(DomainError::repository)?;
        }
        return Ok(IrohSyncResult {
            board,
            content_changed,
            owner_sync: None,
        });
    }
    let mut current = guard
        .database
        .read_board_complete(board_id)
        .map_err(DomainError::repository)?;
    let mut sent_data = data.clone();
    current.settings = Default::default();
    sent_data.settings = Default::default();
    let current_state = guard
        .database
        .board_sync_state(board_id)
        .map_err(DomainError::repository)?;
    let changed_during_sync =
        current != sent_data || current_state.0 != revision || current_state.1 != status;
    // A permission downgrade does not create a conflict for a clean editor.
    // Only unsent local edits need a copy before accepting the owner snapshot.
    let local_edits_at_risk = local_edits_at_risk(status, changed_during_sync);
    let content_changed = current.columns != snapshot.data.columns
        || current.archives != snapshot.data.archives
        || current.templates != snapshot.data.templates
        || guard
            .database
            .board_role(board_id)
            .map_err(DomainError::repository)?
            != BoardRole::from(snapshot.permission);
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
            .map_err(DomainError::repository)?;
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
                Some(&snapshot.loro_update),
            )
            .map_err(DomainError::repository)?;
        if let Some(board) = guard.boards.iter_mut().find(|board| board.id == board_id) {
            board.sync_status = SyncStatus::Conflict;
            board.shared_role = snapshot.permission.into();
        }
        return Err(ShareError::SyncConflict);
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
            .map_err(DomainError::repository)?;
    }
    let index = guard
        .boards
        .iter()
        .position(|b| b.id == board_id)
        .ok_or(DomainError::BoardNotFound)?;
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
    if guard.active_board_id == board_id && content_changed {
        guard
            .load_active_board(board_id)
            .map_err(DomainError::repository)?;
    }
    Ok(IrohSyncResult {
        board: guard.boards[index].clone(),
        content_changed,
        owner_sync: None,
    })
}

/// Resolve a saved conflict. Keeping local changes explicitly rebases them on
/// the last owner snapshot; the next sync still uses revision checking.
#[tauri::command]
pub async fn resolve_iroh_conflict(
    app: State<'_, SharedAppData>,
    app_handle: AppHandle,
    network: State<'_, IrohShareState>,
    board_id: i64,
    keep_local: bool,
) -> Result<(), CommandError> {
    let needs_recovery = {
        let guard = lock_app(&app)?;
        guard
            .database
            .iroh_conflict(board_id)
            .map_err(CommandError::repository)?
            .is_some_and(|conflict| {
                conflict.permission == IrohPermission::Editor
                    && conflict
                        .loro_update
                        .as_ref()
                        .is_none_or(|bytes| bytes.is_empty())
            })
    };
    if needs_recovery {
        sync_iroh_board(app.clone(), app_handle, network, board_id).await?;
    }
    let mut guard = lock_app(&app)?;
    let conflict = guard
        .database
        .iroh_conflict(board_id)
        .map_err(CommandError::repository)?
        .ok_or(DomainError::SyncConflictNotFound)?;
    let (revision, name, permission, remote) = (
        conflict.revision,
        conflict.name,
        conflict.permission,
        conflict.data,
    );
    let local = guard
        .database
        .read_board_complete(board_id)
        .map_err(CommandError::repository)?;
    if keep_local {
        if permission != IrohPermission::Editor {
            return Err(CommandError::PermissionDenied);
        }
        guard
            .database
            .keep_iroh_conflict_local(board_id)
            .map_err(CommandError::repository)?;
    } else {
        let local_name = guard
            .boards
            .iter()
            .find(|board| board.id == board_id)
            .map(|board| board.name.clone())
            .ok_or(DomainError::BoardNotFound)?;
        let copy = guard
            .database
            .use_iroh_conflict_remote(board_id, &local_name, &local)
            .map_err(CommandError::repository)?;
        guard.boards.push(copy);
        if guard.active_board_id == board_id {
            guard
                .load_active_board(board_id)
                .map_err(CommandError::repository)?;
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
