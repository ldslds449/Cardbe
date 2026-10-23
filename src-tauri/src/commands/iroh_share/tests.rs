use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

#[test]
fn invitation_secrets_match_only_identical_values() {
    use super::secrets_match;
    assert!(secrets_match("secret", "secret"));
    assert!(secrets_match("", ""));
    for provided in ["Secret", "secret ", "secre", "", "secret\0"] {
        assert!(!secrets_match("secret", provided), "{provided:?}");
    }
}

#[test]
fn updating_connection_preserves_board_edits_conflicts_and_identity() {
    use super::*;
    let dir = std::env::temp_dir().join(format!("cardbe-update-connection-{}", random_token()));
    let mut loaded = crate::storage::load(&dir).unwrap();
    let owner = SecretKey::generate().public();
    let remote = RemoteInvitation {
        node_id: URL_SAFE_NO_PAD.encode(owner.as_bytes()),
        endpoint: Some(EndpointAddr::from_parts(
            owner,
            [iroh::TransportAddr::Ip(
                "192.168.1.20:12345".parse().unwrap(),
            )],
        )),
        relay_urls: vec![],
        invite_id: "invite".into(),
        secret: "secret".into(),
    };
    let encode = |ticket: &Ticket| {
        format!(
            "cardbe://share/{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(ticket).unwrap())
        )
    };
    let mut ticket = Ticket {
        version: 2,
        node_id: remote.node_id.clone(),
        endpoint: Some(EndpointAddr::from_parts(
            owner,
            [iroh::TransportAddr::Ip(
                "192.168.1.21:23456".parse().unwrap(),
            )],
        )),
        relay_urls: vec![],
        invite_id: remote.invite_id.clone(),
        secret: remote.secret.clone(),
        permission: IrohPermission::Viewer,
    };
    let initial = loaded
        .database
        .ensure_iroh_loro_doc(loaded.database.active_board_id())
        .unwrap();
    let board = loaded
        .database
        .create_received_board(
            "Shared",
            &StoredData::default(),
            IrohPermission::Editor,
            1,
            &serde_json::to_string(&remote).unwrap(),
            Some(&initial),
        )
        .unwrap();
    let mut local = StoredData::default();
    local.templates.push(crate::models::TaskTemplate {
        id: 1,
        name: "Unsent local edit".into(),
        task: Default::default(),
    });
    loaded
        .database
        .replace_board_as_local_edit(board.id, &local)
        .unwrap();
    let pending_state = loaded.database.board_sync_state(board.id).unwrap();
    let pending_loro = loaded.database.iroh_loro_update(board.id).unwrap();
    update_iroh_connection_inner(&loaded.database, board.id, &encode(&ticket)).unwrap();
    assert_eq!(
        loaded.database.board_sync_state(board.id).unwrap(),
        pending_state
    );
    assert_eq!(
        loaded.database.iroh_loro_update(board.id).unwrap(),
        pending_loro
    );
    assert_eq!(
        loaded
            .database
            .read_board_complete(board.id)
            .unwrap()
            .templates,
        local.templates
    );
    assert_eq!(
        loaded.database.board_role(board.id).unwrap(),
        BoardRole::Editor
    );
    let saved: RemoteInvitation =
        serde_json::from_str(&loaded.database.iroh_remote(board.id).unwrap().unwrap()).unwrap();
    assert_eq!(saved.endpoint, ticket.endpoint);
    assert_eq!(saved.node_id, remote.node_id);
    assert_eq!(saved.invite_id, remote.invite_id);
    assert_eq!(saved.secret, remote.secret);
    assert!(matches!(
        ensure_invite_not_joined(&loaded.database, &ticket),
        Err(DomainError::InviteAlreadyJoined)
    ));
    loaded
        .database
        .save_iroh_conflict(
            board.id,
            2,
            "Owner version",
            IrohPermission::Editor,
            &StoredData::default(),
            Some(&initial),
        )
        .unwrap();
    let conflict_state = loaded.database.board_sync_state(board.id).unwrap();
    ticket.version = 1;
    ticket.endpoint = None;
    ticket.relay_urls = vec!["https://relay.example.com".into()];
    update_iroh_connection_inner(&loaded.database, board.id, &encode(&ticket)).unwrap();
    assert_eq!(
        loaded.database.board_sync_state(board.id).unwrap(),
        conflict_state
    );
    assert_eq!(
        loaded
            .database
            .iroh_conflict(board.id)
            .unwrap()
            .unwrap()
            .name,
        "Owner version"
    );
    assert_eq!(
        loaded
            .database
            .read_board_complete(board.id)
            .unwrap()
            .templates,
        local.templates
    );
    let stored = loaded.database.iroh_remote(board.id).unwrap().unwrap();
    drop(loaded);
    let reopened = crate::storage::load(&dir).unwrap();
    assert_eq!(
        reopened.database.iroh_remote(board.id).unwrap().unwrap(),
        stored
    );
    let restored: RemoteInvitation = serde_json::from_str(&stored).unwrap();
    assert_eq!(restored.relay_urls, ticket.relay_urls);
    assert!(restored.endpoint.is_none());
    drop(reopened);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn updating_connection_rejects_unrelated_or_invalid_links_without_writes() {
    use super::*;
    let dir = std::env::temp_dir().join(format!("cardbe-update-connection-{}", random_token()));
    let mut loaded = crate::storage::load(&dir).unwrap();
    let remote = RemoteInvitation {
        node_id: URL_SAFE_NO_PAD.encode(SecretKey::generate().public().as_bytes()),
        endpoint: None,
        relay_urls: vec![],
        invite_id: "invite".into(),
        secret: "secret".into(),
    };
    let stored = serde_json::to_string(&remote).unwrap();
    let board = loaded
        .database
        .create_received_board(
            "Shared",
            &StoredData::default(),
            IrohPermission::Viewer,
            1,
            &stored,
            None,
        )
        .unwrap();
    let valid = Ticket {
        version: 2,
        node_id: remote.node_id.clone(),
        endpoint: None,
        relay_urls: vec!["https://relay.example.com".into()],
        invite_id: remote.invite_id.clone(),
        secret: remote.secret.clone(),
        permission: IrohPermission::Editor,
    };
    for field in ["node_id", "invite_id", "secret", "relay_urls", "endpoint"] {
        let mut ticket = valid.clone();
        match field {
            "node_id" => {
                ticket.node_id = URL_SAFE_NO_PAD.encode(SecretKey::generate().public().as_bytes())
            }
            "invite_id" => ticket.invite_id = "another-invite".into(),
            "secret" => ticket.secret = "wrong".into(),
            "relay_urls" => ticket.relay_urls = vec!["invalid-url".into()],
            "endpoint" => ticket.endpoint = Some(EndpointAddr::new(SecretKey::generate().public())),
            _ => unreachable!(),
        }
        let link = format!(
            "cardbe://share/{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&ticket).unwrap())
        );
        assert!(matches!(
            update_iroh_connection_inner(&loaded.database, board.id, &link),
            Err(DomainError::InviteInvalid)
        ));
        assert_eq!(
            loaded.database.iroh_remote(board.id).unwrap().unwrap(),
            stored
        );
    }
    assert!(matches!(
        update_iroh_connection_inner(&loaded.database, board.id, "bad link"),
        Err(DomainError::InviteInvalid)
    ));
    let link = format!(
        "cardbe://share/{}",
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&valid).unwrap())
    );
    for id in [loaded.database.active_board_id(), i64::MAX] {
        assert!(matches!(
            update_iroh_connection_inner(&loaded.database, id, &link),
            Err(DomainError::InviteNotFound)
        ));
    }
    update_iroh_connection_inner(&loaded.database, board.id, &link).unwrap();
    assert_eq!(
        loaded.database.board_role(board.id).unwrap(),
        BoardRole::Viewer
    );
    assert_eq!(
        loaded.database.board_sync_state(board.id).unwrap(),
        (1, SyncStatus::Synced)
    );
    drop(loaded);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn owner_pull_merges_unsent_edits_and_checks_identity_permission_and_revocation() {
    use super::*;
    let dir = std::env::temp_dir().join(format!("cardbe-owner-pull-{}", random_token()));
    let mut owner = crate::storage::load(&dir.join("owner")).unwrap();
    let mut editor = crate::storage::load(&dir.join("editor")).unwrap();
    let owner_id = owner.database.active_board_id();
    let base = StoredData::default();
    owner
        .database
        .save_iroh_invite("invite", owner_id, "secret", IrohPermission::Editor)
        .unwrap();
    owner
        .database
        .iroh_device_access("invite", "secret", "editor", true)
        .unwrap();
    owner
        .database
        .set_iroh_device_approved("invite", "editor", true)
        .unwrap();
    let initial = owner.database.ensure_iroh_loro_doc(owner_id).unwrap();
    let remote = RemoteInvitation {
        node_id: "owner".into(),
        endpoint: None,
        relay_urls: vec![],
        invite_id: "invite".into(),
        secret: "secret".into(),
    };
    let editor_id = editor
        .database
        .create_received_board(
            "Shared",
            &base,
            IrohPermission::Editor,
            1,
            &serde_json::to_string(&remote).unwrap(),
            Some(&initial),
        )
        .unwrap()
        .id;
    let mut local = base.clone();
    local.templates.push(crate::models::TaskTemplate {
        id: 1,
        name: "Editor offline edit".into(),
        task: Default::default(),
    });
    editor
        .database
        .replace_board_as_local_edit(editor_id, &local)
        .unwrap();
    let mut owner_edit = base.clone();
    owner_edit.templates.push(crate::models::TaskTemplate {
        id: 2,
        name: "Owner concurrent edit".into(),
        task: Default::default(),
    });
    owner
        .database
        .replace_board_as_local_edit(owner_id, &owner_edit)
        .unwrap();
    let owner_doc = owner.database.iroh_loro_update(owner_id).unwrap().unwrap();
    let mut request = Request {
        invite_id: "invite".into(),
        secret: "secret".into(),
        action: IrohAction::LoroSync,
        loro_state_vector: crate::loro_board::state_vector(Some(&owner_doc)).unwrap(),
        loro_update: vec![],
        known_revision: None,
        known_permission: None,
        status_only: false,
        owner_pull: true,
    };
    assert!(editor_snapshot_for_owner(&editor.database, &request, "impostor").is_err());
    request.secret = "wrong".into();
    assert!(editor_snapshot_for_owner(&editor.database, &request, "owner").is_err());
    request.secret = "secret".into();
    let response = editor_snapshot_for_owner(&editor.database, &request, "owner").unwrap();
    assert_eq!(
        editor.database.board_sync_state(editor_id).unwrap().1,
        SyncStatus::Pending
    );
    owner
        .database
        .apply_iroh_loro_update(
            owner_id,
            "invite",
            "secret",
            "editor",
            &response.loro_update,
        )
        .unwrap();
    let merged = owner.database.read_board_complete(owner_id).unwrap();
    assert!(merged
        .templates
        .iter()
        .any(|template| template.name == "Editor offline edit"));
    assert!(merged
        .templates
        .iter()
        .any(|template| template.name == "Owner concurrent edit"));
    owner
        .database
        .set_iroh_device_approved("invite", "editor", false)
        .unwrap();
    assert!(owner
        .database
        .apply_iroh_loro_update(
            owner_id,
            "invite",
            "secret",
            "editor",
            &response.loro_update
        )
        .is_err());
    editor
        .database
        .apply_iroh_snapshot(
            editor_id,
            "Shared",
            IrohPermission::Viewer,
            1,
            &base,
            Some(&initial),
        )
        .unwrap();
    assert!(editor_snapshot_for_owner(&editor.database, &request, "owner").is_err());
    request.owner_pull = false;
    let legacy = serde_json::to_value(&request).unwrap();
    assert!(legacy.get("owner_pull").is_none());
    assert!(
        !serde_json::from_value::<Request>(legacy)
            .unwrap()
            .owner_pull
    );
    drop(editor);
    drop(owner);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn conflict_permission_refresh_preserves_local_edits_and_allows_editor_rebase() {
    use super::*;
    let dir = std::env::temp_dir().join(format!(
        "cardbe-conflict-permission-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    let loaded = crate::storage::load(&dir).unwrap();
    let mut app = AppData::new(
        loaded.stored,
        loaded.database,
        loaded.recovery_messages,
        loaded.archives_loaded,
    )
    .unwrap();
    let mut base = StoredData::default();
    base.columns.push(crate::models::Column {
        id: 1,
        name: "Owner".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: Vec::new(),
    });
    let doc = crate::loro_board::apply_local_delta(None, &StoredData::default(), &base).unwrap();
    let board = app
        .database
        .create_received_board(
            "Shared",
            &base,
            IrohPermission::Editor,
            1,
            "ticket",
            Some(&doc),
        )
        .unwrap();
    app.boards.push(board.clone());
    let mut local = base.clone();
    local.columns[0].name = "Local unsent edit".into();
    app.database
        .replace_board_as_local_edit(board.id, &local)
        .unwrap();
    app.database
        .save_iroh_conflict(
            board.id,
            1,
            "Shared",
            IrohPermission::Editor,
            &base,
            Some(&doc),
        )
        .unwrap();
    let local_doc = app.database.iroh_loro_update(board.id).unwrap().unwrap();
    let mut owner = Snapshot {
        ok: true,
        error: None,
        error_code: None,
        name: "Shared".into(),
        permission: IrohPermission::Viewer,
        revision: 2,
        data: base.clone(),
        loro_update: Vec::new(),
        unchanged: false,
    };
    for permission in [IrohPermission::Viewer, IrohPermission::Editor] {
        owner.permission = permission;
        owner.loro_update = if permission == IrohPermission::Editor {
            doc.clone()
        } else {
            Vec::new()
        };
        let refreshed = refresh_saved_conflict(&mut app, board.id, &owner)
            .unwrap()
            .unwrap();
        assert_eq!(refreshed.shared_role, BoardRole::from(permission));
        assert_eq!(
            app.database.board_sync_state(board.id).unwrap().1,
            SyncStatus::Conflict
        );
        assert_eq!(
            app.database
                .iroh_conflict(board.id)
                .unwrap()
                .unwrap()
                .permission,
            permission
        );
        assert_eq!(
            app.database.read_board_complete(board.id).unwrap().columns,
            local.columns
        );
        assert_eq!(
            app.database.iroh_loro_update(board.id).unwrap().unwrap(),
            local_doc
        );
    }
    app.database.keep_iroh_conflict_local(board.id).unwrap();
    assert_eq!(
        app.database.board_sync_state(board.id).unwrap().1,
        SyncStatus::Pending
    );
    assert_eq!(
        crate::loro_board::project(&app.database.iroh_loro_update(board.id).unwrap().unwrap())
            .unwrap()
            .columns,
        local.columns
    );
    // A delayed response after resolution cannot create another conflict.
    assert!(refresh_saved_conflict(&mut app, board.id, &owner)
        .unwrap()
        .is_none());
    drop(app);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn invitation_validates_both_device_ids_and_keeps_legacy_tickets() {
    let id = iroh::SecretKey::generate().public();
    let mut ticket = super::Ticket {
        version: 2,
        node_id: URL_SAFE_NO_PAD.encode(id.as_bytes()),
        endpoint: Some(iroh::EndpointAddr::new(id)),
        relay_urls: Vec::new(),
        invite_id: "test-invite".into(),
        secret: "test-secret".into(),
        permission: super::IrohPermission::Viewer,
    };
    let parse = |ticket: &super::Ticket| {
        super::parse_ticket(&format!(
            "cardbe://share/{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(ticket).unwrap())
        ))
    };
    assert!(parse(&ticket).is_ok());
    ticket.node_id = URL_SAFE_NO_PAD.encode(iroh::SecretKey::generate().public().as_bytes());
    assert!(matches!(parse(&ticket), Err(DomainError::InviteInvalid)));
    ticket.node_id = "invalid".into();
    assert!(matches!(parse(&ticket), Err(DomainError::InviteInvalid)));
    ticket.node_id = URL_SAFE_NO_PAD.encode(id.as_bytes());
    ticket.version = 1;
    ticket.endpoint = None;
    assert!(parse(&ticket).is_ok());
}

#[tokio::test]
async fn direct_transport_exchanges_loro_updates_and_rejects_invalid_responses() {
    use super::{exchange_sync_request, ALPN, MAX_MESSAGE};
    use crate::{
        loro_board,
        models::{StoredData, Task, TaskTemplate},
    };
    use iroh::{endpoint::presets, Endpoint, EndpointAddr};
    let owner = Endpoint::builder(presets::Minimal)
        .alpns(vec![ALPN.to_vec()])
        .bind_addr("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
        .unwrap()
        .bind()
        .await
        .unwrap();
    let client = Endpoint::builder(presets::Minimal).bind().await.unwrap();
    let address = EndpointAddr::from_parts(
        owner.id(),
        owner
            .bound_sockets()
            .iter()
            .copied()
            .map(iroh::TransportAddr::Ip),
    );
    let initial =
        loro_board::apply_local_delta(None, &StoredData::default(), &StoredData::default())
            .unwrap();
    let mut edited = StoredData::default();
    edited.templates.push(TaskTemplate {
        id: 1,
        name: "Offline".into(),
        task: Task::default(),
    });
    let local =
        loro_board::apply_local_delta(Some(&initial), &StoredData::default(), &edited).unwrap();
    let request = Request {
        invite_id: "invite".into(),
        secret: "secret".into(),
        action: IrohAction::LoroSync,
        loro_state_vector: loro_board::state_vector(Some(&local)).unwrap(),
        loro_update: loro_board::diff(
            Some(&local),
            &loro_board::state_vector(Some(&initial)).unwrap(),
        )
        .unwrap(),
        known_revision: Some(1),
        known_permission: Some(IrohPermission::Editor),
        status_only: false,
        owner_pull: false,
    };
    let server = async {
        for invalid in [false, true] {
            let connection = owner.accept().await.unwrap().await.unwrap();
            let (mut send, mut recv) = connection.accept_bi().await.unwrap();
            let received: Request =
                serde_json::from_slice(&recv.read_to_end(MAX_MESSAGE).await.unwrap()).unwrap();
            assert_eq!(received.secret, "secret");
            assert_eq!(received.action, IrohAction::LoroSync);
            let merged = loro_board::merge(Some(&initial), &received.loro_update).unwrap();
            assert_eq!(
                loro_board::project(&merged).unwrap().templates,
                edited.templates
            );
            let response = Snapshot {
                ok: true,
                error: None,
                error_code: None,
                name: "Board".into(),
                permission: IrohPermission::Editor,
                revision: 2,
                data: Default::default(),
                unchanged: false,
                loro_update: loro_board::diff(Some(&merged), &received.loro_state_vector).unwrap(),
            };
            let bytes = if invalid {
                b"invalid response".to_vec()
            } else {
                serde_json::to_vec(&response).unwrap()
            };
            send.write_all(&bytes).await.unwrap();
            send.finish().unwrap();
            let _ = send.stopped().await;
        }
    };
    let exchanges = async {
        let network = IrohShareState::default();
        let response = exchange_sync_request(&client, address.clone(), &network, &request)
            .await
            .unwrap();
        let merged = loro_board::merge(Some(&local), &response.loro_update).unwrap();
        assert_eq!(
            loro_board::project(&merged).unwrap().templates,
            edited.templates
        );
        assert!(
            matches!(exchange_sync_request(&client, address, &network, &request).await, Err(crate::errors::ShareError::Internal(error)) if error.contains("invalid response"))
        );
    };
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        tokio::join!(server, exchanges);
    })
    .await
    .unwrap();
    client.close().await;
    owner.close().await;
}
use super::{
    endpoint_address, endpoint_address_for_ticket, endpoint_builder, ensure_board_owned,
    local_edits_at_risk, needs_editor_upload, remember_connection, snapshot_failure,
    ticket_payload, validate_network_settings, validate_sync_snapshot, viewer_is_current,
    BoardRole, IrohAction, IrohConnectionInfo, IrohConnectionPath, IrohNetworkConfig,
    IrohPermission, IrohShareState, Request, Snapshot, SnapshotErrorCode, SyncStatus,
};
use crate::models::{Board, IrohNetworkSettings};

#[test]
fn rejected_sync_responses_are_not_board_snapshots() {
    let mut response = Snapshot {
        ok: false,
        error: Some("Access was declined or revoked. You can request access again.".into()),
        error_code: None,
        name: String::new(),
        permission: IrohPermission::Viewer,
        revision: 0,
        data: Default::default(),
        loro_update: Vec::new(),
        unchanged: false,
    };
    assert!(matches!(
        validate_sync_snapshot(&response),
        Err(crate::errors::ShareError::AccessTerminated)
    ));
    response.name = "Board with a conflict".into();
    assert!(validate_sync_snapshot(&response).is_ok());
    response.name.clear();
    response.ok = true;
    assert!(validate_sync_snapshot(&response).is_ok());
}

#[test]
fn access_status_requests_and_legacy_errors_remain_compatible() {
    let mut request: Request =
        serde_json::from_str(r#"{"invite_id":"i","secret":"s","action":"request"}"#).unwrap();
    assert!(!request.status_only);
    request.status_only = true;
    let encoded = serde_json::to_string(&request).unwrap();
    assert!(
        serde_json::from_str::<Request>(&encoded)
            .unwrap()
            .status_only
    );
    for (message, code) in [
        ("Invitation was revoked", "SHARE_INVITATION_DELETED"),
        ("Invitation is disabled", "INVITE_DISABLED"),
        ("Access was declined or revoked", "SHARE_ACCESS_TERMINATED"),
        ("Waiting for owner approval", "SHARE_APPROVAL_REQUIRED"),
        ("Invitation was revoked or disabled", "SHARE_ACCESS_REVOKED"),
    ] {
        let response = Snapshot {
            ok: false,
            error: Some(message.into()),
            error_code: None,
            name: String::new(),
            permission: IrohPermission::Viewer,
            revision: 0,
            data: Default::default(),
            loro_update: vec![],
            unchanged: false,
        };
        let error =
            crate::errors::CommandError::from(validate_sync_snapshot(&response).unwrap_err());
        assert_eq!(serde_json::to_value(error).unwrap()["code"], code);
    }
}

#[test]
fn pending_editor_uploads_support_legacy_snapshots_and_status_vectors() {
    use crate::{
        loro_board,
        models::{StoredData, Task, TaskTemplate},
    };
    let initial =
        loro_board::apply_local_delta(None, &StoredData::default(), &StoredData::default())
            .unwrap();
    let mut edited = StoredData::default();
    edited.templates.push(TaskTemplate {
        id: 1,
        name: "Offline edit".into(),
        task: Task::default(),
    });
    let local =
        loro_board::apply_local_delta(Some(&initial), &StoredData::default(), &edited).unwrap();
    // Old hosts ignore status_only and send their complete document.
    let mut owner: Snapshot = serde_json::from_value(serde_json::json!({
        "ok": true, "error": null, "name": "Board", "permission": "editor",
        "revision": 1, "data": StoredData::default(), "loro_update": initial
    }))
    .unwrap();
    let upload = super::editor_upload(&local, &owner).unwrap();
    assert_eq!(upload, local);
    let merged = loro_board::merge(Some(&initial), &upload).unwrap();
    assert!(loro_board::same_state_vector(&merged, &local).unwrap());

    owner.unchanged = true;
    owner.loro_update = loro_board::state_vector(Some(&initial)).unwrap();
    let upload = super::editor_upload(&local, &owner).unwrap();
    assert_eq!(
        upload,
        loro_board::diff(Some(&local), &owner.loro_update).unwrap()
    );
    let merged = loro_board::merge(Some(&initial), &upload).unwrap();
    assert!(loro_board::same_state_vector(&merged, &local).unwrap());
    owner.loro_update.clear();
    assert_eq!(super::editor_upload(&local, &owner).unwrap(), local);
    owner.permission = IrohPermission::Viewer;
    assert!(super::editor_upload(&local, &owner).unwrap().is_empty());
}

#[test]
fn connection_details_keep_latest_paths_and_bound_peer_history() {
    let network = IrohShareState::default();
    for index in 0..128 {
        remember_connection(
            &network,
            index.to_string(),
            IrohConnectionInfo {
                observed_at: index,
                paths: Vec::new(),
            },
        );
    }
    remember_connection(
        &network,
        "0".into(),
        IrohConnectionInfo {
            observed_at: 200,
            paths: vec![IrohConnectionPath {
                kind: "Direct".into(),
                address: "192.168.1.20:12345".into(),
            }],
        },
    );
    remember_connection(
        &network,
        "new-peer".into(),
        IrohConnectionInfo {
            observed_at: 201,
            paths: Vec::new(),
        },
    );
    let records = network.connections.lock().unwrap();
    assert_eq!(records.len(), 128);
    assert!(!records.contains_key("1"));
    assert!(records.contains_key("new-peer"));
    assert_eq!(records["0"].observed_at, 200);
    assert_eq!(records["0"].paths[0].kind, "Direct");
    assert_eq!(records["0"].paths[0].address, "192.168.1.20:12345");
}
use crate::errors::DomainError;

#[test]
fn structured_and_legacy_peer_failures_produce_safe_ipc_errors() {
    let mut snapshot = Snapshot {
        ok: false,
        error: Some("Waiting for owner approval".into()),
        error_code: None,
        name: String::new(),
        permission: IrohPermission::Viewer,
        revision: 0,
        data: Default::default(),
        loro_update: vec![],
        unchanged: false,
    };
    let serialize = |snapshot: &Snapshot| {
        serde_json::to_value(crate::errors::CommandError::from(snapshot_failure(
            snapshot,
            "device".into(),
        )))
        .unwrap()
    };
    assert_eq!(
        serialize(&snapshot),
        serde_json::json!({"code": "SHARE_APPROVAL_REQUIRED", "device_id": "device"})
    );
    snapshot.error = Some("Access was declined or revoked. You can request access again.".into());
    assert_eq!(
        serialize(&snapshot),
        serde_json::json!({"code": "SHARE_ACCESS_TERMINATED"})
    );
    snapshot.error = Some("private exception token=secret".into());
    snapshot.error_code = Some(SnapshotErrorCode::AccessRevoked);
    assert_eq!(
        serialize(&snapshot),
        serde_json::json!({"code": "SHARE_ACCESS_TERMINATED"})
    );
    snapshot.error_code = Some(SnapshotErrorCode::InvitationDeleted);
    assert_eq!(
        serialize(&snapshot),
        serde_json::json!({"code": "SHARE_INVITATION_DELETED"})
    );
    snapshot.error_code = Some(SnapshotErrorCode::InvitationDisabled);
    assert_eq!(
        serialize(&snapshot),
        serde_json::json!({"code": "INVITE_DISABLED"})
    );
    snapshot.error_code = Some(SnapshotErrorCode::Unknown);
    assert_eq!(
        serialize(&snapshot),
        serde_json::json!({"code": "INTERNAL_ERROR"})
    );
}

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
        is_shared: true,
        sync_status: SyncStatus::Synced,
        sync_revision: 1,
    };
    assert!(matches!(
        ensure_board_owned(None),
        Err(DomainError::BoardNotFound)
    ));
    assert!(matches!(
        ensure_board_owned(Some(&viewer)),
        Err(DomainError::PermissionDenied)
    ));
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
        status_only: false,
        owner_pull: false,
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

#[test]
fn native_ticket_address_keeps_known_direct_ips() {
    use iroh::{EndpointAddr, SecretKey, TransportAddr};

    let expected = "192.168.1.20:12345".parse().unwrap();
    let address = EndpointAddr::from_parts(
        SecretKey::generate().public(),
        [TransportAddr::Ip(expected)],
    );
    let node_id = URL_SAFE_NO_PAD.encode(address.id.as_bytes());
    let restored = endpoint_address(Some(&address), &node_id, &[]).unwrap();
    assert_eq!(restored.id, address.id);
    assert_eq!(
        restored.ip_addrs().copied().collect::<Vec<_>>(),
        vec![expected]
    );
}

#[tokio::test]
async fn ticket_keeps_configured_relays_before_they_are_online() {
    use iroh::{endpoint::presets, Endpoint, RelayMode, RelayUrl, SecretKey};

    let relay = "https://relay.example.com".parse::<RelayUrl>().unwrap();
    let config = IrohNetworkConfig {
        direct_ip_enabled: true,
        discovery_enabled: false,
        discovery_urls: Vec::new(),
        relay_mode: RelayMode::custom([relay.clone()]),
        direct_addresses: Vec::new(),
        listen_port: 0,
    };
    let endpoint = Endpoint::builder(presets::Minimal)
        .relay_mode(config.relay_mode.clone())
        .secret_key(SecretKey::generate())
        .bind()
        .await
        .unwrap();
    let address = endpoint_address_for_ticket(&endpoint, &config);
    assert!(address.relay_urls().any(|url| url == &relay));
    endpoint.close().await;
}

#[test]
fn iroh_is_disabled_by_default_and_legacy_settings() {
    assert!(!IrohNetworkSettings::default().enabled);
    let legacy: IrohNetworkSettings = serde_json::from_str("{}").unwrap();
    assert!(!legacy.enabled);
    let enabled = IrohNetworkSettings {
        enabled: true,
        ..Default::default()
    };
    let restored: IrohNetworkSettings =
        serde_json::from_value(serde_json::to_value(&enabled).unwrap()).unwrap();
    assert_eq!(restored, enabled);
    assert_eq!(
        serde_json::to_value(crate::errors::CommandError::from(
            crate::errors::ShareError::IrohDisabled
        ))
        .unwrap(),
        serde_json::json!({"code": "IROH_DISABLED"})
    );
}

#[test]
fn network_validation_preserves_safe_field_errors() {
    use crate::errors::{CommandError, NetworkSettingsField};
    for (field, settings) in [
        (
            NetworkSettingsField::DirectAddresses,
            IrohNetworkSettings {
                direct_addresses: vec!["private-invalid-address".into()],
                ..Default::default()
            },
        ),
        (
            NetworkSettingsField::RelayUrls,
            IrohNetworkSettings {
                relay_urls: vec!["private-invalid-url".into()],
                ..Default::default()
            },
        ),
        (
            NetworkSettingsField::DiscoveryUrls,
            IrohNetworkSettings {
                discovery_urls: vec!["private-invalid-url".into()],
                ..Default::default()
            },
        ),
        (
            NetworkSettingsField::ListenPort,
            IrohNetworkSettings {
                direct_addresses: vec!["192.168.1.20:12345".into()],
                ..Default::default()
            },
        ),
    ] {
        let error = CommandError::from(validate_network_settings(&settings).unwrap_err());
        let json = serde_json::to_value(error).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "code": "NETWORK_SETTINGS_INVALID", "field": field })
        );
    }
}

#[test]
fn network_settings_validate_direct_and_self_hosted_relay_addresses() {
    let settings = IrohNetworkSettings {
        listen_port: 12345,
        direct_addresses: vec!["192.168.1.20:12345".into(), "[2001:db8::1]:12345".into()],
        relay_urls: vec!["https://relay.example.com".into()],
        discovery_urls: vec!["https://discovery.example.com".into()],
        ..Default::default()
    };
    assert!(validate_network_settings(&settings).is_ok());
    assert!(validate_network_settings(&IrohNetworkSettings {
        listen_port: 0,
        ..settings.clone()
    })
    .is_err());
    assert!(validate_network_settings(&IrohNetworkSettings {
        direct_ip_enabled: false,
        direct_addresses: vec!["not-an-address".into()],
        discovery_enabled: false,
        discovery_urls: vec!["not-a-url".into()],
        ..Default::default()
    })
    .is_ok());
    assert!(validate_network_settings(&IrohNetworkSettings {
        relay_enabled: false,
        relay_urls: vec!["not-a-url".into()],
        ..Default::default()
    })
    .is_ok());
    assert!(validate_network_settings(&IrohNetworkSettings {
        direct_ip_enabled: false,
        relay_enabled: false,
        ..Default::default()
    })
    .is_err());
    assert!(validate_network_settings(&IrohNetworkSettings {
        direct_addresses: vec!["not-an-address".into()],
        ..Default::default()
    })
    .is_err());
    assert!(validate_network_settings(&IrohNetworkSettings {
        discovery_urls: vec!["not-a-url".into()],
        ..Default::default()
    })
    .is_err());
}

#[tokio::test]
async fn automatic_direct_port_survives_direct_relay_direct_switches() {
    let network = IrohShareState::default();
    let key = iroh::SecretKey::generate();
    let direct = super::IrohNetworkConfig::from_settings(&IrohNetworkSettings {
        discovery_enabled: false,
        relay_enabled: false,
        ..Default::default()
    })
    .unwrap();
    let endpoint = super::bind_endpoint(&direct, key.clone(), &network)
        .await
        .unwrap();
    let port = endpoint
        .bound_sockets()
        .iter()
        .find(|address| address.is_ipv4())
        .unwrap()
        .port();
    let id = endpoint.id();
    endpoint.close().await;
    drop(endpoint);
    let relay = super::IrohNetworkConfig::from_settings(&IrohNetworkSettings {
        direct_ip_enabled: false,
        discovery_enabled: false,
        relay_urls: vec!["http://127.0.0.1:9".into()],
        ..Default::default()
    })
    .unwrap();
    let endpoint = super::bind_endpoint(&relay, key.clone(), &network)
        .await
        .unwrap();
    assert!(endpoint.bound_sockets().is_empty());
    endpoint.close().await;
    drop(endpoint);
    let endpoint = super::bind_endpoint(&direct, key, &network).await.unwrap();
    assert_eq!(endpoint.id(), id);
    assert!(endpoint
        .bound_sockets()
        .iter()
        .any(|address| address.is_ipv4() && address.port() == port));
    endpoint.close().await;
}

#[tokio::test]
async fn saved_automatic_port_is_reused_after_restart() {
    let config = super::IrohNetworkConfig::from_settings(&IrohNetworkSettings {
        discovery_enabled: false,
        relay_enabled: false,
        ..Default::default()
    })
    .unwrap();
    let key = iroh::SecretKey::generate();
    let first = IrohShareState::default();
    let endpoint = super::bind_endpoint(&config, key.clone(), &first)
        .await
        .unwrap();
    let saved = *first.automatic_direct_port.lock().unwrap();
    assert!(saved.is_some());
    endpoint.close().await;
    drop(endpoint);
    // A new process starts with empty in-memory state.
    let restarted = IrohShareState::default();
    super::restore_automatic_port(&restarted, saved).unwrap();
    let endpoint = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let endpoint = super::bind_endpoint(&config, key.clone(), &restarted)
                .await
                .unwrap();
            if *restarted.automatic_direct_port.lock().unwrap() == saved {
                break endpoint;
            }
            // Windows may release the previous socket shortly after close.
            endpoint.close().await;
            *restarted.automatic_direct_port.lock().unwrap() = saved;
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the saved automatic port should be reused");
    assert!(endpoint
        .bound_sockets()
        .iter()
        .any(|address| address.is_ipv4() && Some(address.port()) == saved));
    endpoint.close().await;
}

#[tokio::test]
async fn occupied_automatic_port_uses_an_available_port() {
    let occupied = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
    let port = occupied.local_addr().unwrap().port();
    let network = IrohShareState::default();
    *network.automatic_direct_port.lock().unwrap() = Some(port);
    let config = super::IrohNetworkConfig::from_settings(&IrohNetworkSettings {
        discovery_enabled: false,
        relay_enabled: false,
        ..Default::default()
    })
    .unwrap();
    let endpoint = super::bind_endpoint(&config, iroh::SecretKey::generate(), &network)
        .await
        .unwrap();
    assert_ne!(*network.automatic_direct_port.lock().unwrap(), Some(port));
    endpoint.close().await;
}

#[tokio::test]
async fn usable_invitation_connects_when_discovery_returns_a_stale_address() {
    use iroh::{
        address_lookup::memory::MemoryLookup, endpoint::presets, Endpoint, EndpointAddr,
        TransportAddr,
    };
    let owner = Endpoint::builder(presets::Minimal)
        .alpns(vec![super::ALPN.to_vec()])
        .bind_addr("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap())
        .unwrap()
        .bind()
        .await
        .unwrap();
    let stale = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let lookup = MemoryLookup::new();
    lookup.add_endpoint_info(EndpointAddr::from_parts(
        owner.id(),
        [TransportAddr::Ip(stale.local_addr().unwrap())],
    ));
    let client = Endpoint::builder(presets::Minimal)
        .address_lookup(lookup)
        .bind()
        .await
        .unwrap();
    let address = EndpointAddr::from_parts(
        owner.id(),
        owner.bound_sockets().iter().copied().map(TransportAddr::Ip),
    );
    let network = IrohShareState::default();
    let (outgoing, incoming) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(super::connect(&client, address, &network), async {
            owner.accept().await.unwrap().await.unwrap()
        })
    })
    .await
    .unwrap();
    assert_eq!(outgoing.unwrap().remote_id(), owner.id());
    assert_eq!(incoming.remote_id(), client.id());
    client.close().await;
    owner.close().await;
}

#[tokio::test]
async fn fixed_port_restart_can_retry_after_an_in_flight_endpoint_is_released() {
    let network = IrohShareState::default();
    let key = iroh::SecretKey::generate();
    let mut config = super::IrohNetworkConfig::from_settings(&IrohNetworkSettings {
        discovery_enabled: false,
        relay_enabled: false,
        ..Default::default()
    })
    .unwrap();
    let endpoint = super::bind_endpoint(&config, key.clone(), &network)
        .await
        .unwrap();
    config.listen_port = endpoint
        .bound_sockets()
        .iter()
        .find(|address| address.is_ipv4())
        .unwrap()
        .port();
    assert!(super::bind_endpoint(&config, key.clone(), &network)
        .await
        .is_err());
    let in_flight = endpoint.clone();
    endpoint.close().await;
    drop(endpoint);
    // Closing a service can leave handles held by an in-flight sync. A
    // retry must keep the requested port and stable device identity.
    let first = super::bind_endpoint(&config, key.clone(), &network).await;
    if let Ok(restarted) = first {
        assert_eq!(restarted.id(), key.public());
        assert!(restarted
            .bound_sockets()
            .iter()
            .any(|address| address.is_ipv4() && address.port() == config.listen_port));
        restarted.close().await;
        drop(restarted);
    }
    drop(in_flight);
    // The endpoint's background tasks may release the Windows socket later.
    let restarted = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if let Ok(endpoint) = super::bind_endpoint(&config, key.clone(), &network).await {
                break endpoint;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the released fixed UDP port should become available");
    assert_eq!(restarted.id(), key.public());
    assert!(restarted
        .bound_sockets()
        .iter()
        .any(|address| address.is_ipv4() && address.port() == config.listen_port));
    restarted.close().await;
}

#[tokio::test]
async fn saved_direct_invitation_connects_using_the_owners_updated_address() {
    use iroh::{
        address_lookup::memory::MemoryLookup, endpoint::presets, Endpoint, EndpointAddr,
        TransportAddr,
    };
    let owner = Endpoint::builder(presets::Minimal)
        .relay_mode(iroh::RelayMode::Disabled)
        .alpns(vec![super::ALPN.to_vec()])
        .bind()
        .await
        .unwrap();
    let port = owner
        .bound_sockets()
        .iter()
        .find(|address| address.is_ipv4())
        .unwrap()
        .port();
    let lookup = MemoryLookup::new();
    lookup.add_endpoint_info(EndpointAddr::from_parts(
        owner.id(),
        [TransportAddr::Ip(([127, 0, 0, 1], port).into())],
    ));
    let client = Endpoint::builder(presets::Minimal)
        .relay_mode(iroh::RelayMode::Disabled)
        .address_lookup(lookup)
        .bind()
        .await
        .unwrap();
    let stale = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let address =
        EndpointAddr::from_parts(owner.id(), [TransportAddr::Ip(stale.local_addr().unwrap())]);
    let network = IrohShareState::default();
    let (outgoing, incoming) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(super::connect(&client, address, &network), async {
            owner.accept().await.unwrap().await.unwrap()
        })
    })
    .await
    .unwrap();
    assert_eq!(outgoing.unwrap().remote_id(), owner.id());
    assert_eq!(incoming.remote_id(), client.id());
    client.close().await;
    owner.close().await;
}

#[tokio::test]
async fn fixed_udp_port_is_bound_and_forwarded_address_is_advertised() {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();
    drop(socket);
    let external: std::net::SocketAddr = "192.0.2.1:54321".parse().unwrap();
    let config = IrohNetworkConfig::from_settings(&IrohNetworkSettings {
        listen_port: port,
        direct_addresses: vec![external.to_string()],
        discovery_enabled: false,
        relay_enabled: false,
        ..Default::default()
    })
    .unwrap();
    let endpoint = endpoint_builder(&config).unwrap().bind().await.unwrap();
    assert!(endpoint
        .bound_sockets()
        .iter()
        .any(|address| address.is_ipv4() && address.port() == port));
    assert!(endpoint
        .bound_sockets()
        .iter()
        .all(|address| address.port() == port));
    assert!(endpoint
        .addr()
        .ip_addrs()
        .any(|address| *address == external));
    endpoint.close().await;

    let config = IrohNetworkConfig::from_settings(&IrohNetworkSettings {
        listen_port: port,
        direct_ip_enabled: false,
        discovery_enabled: false,
        ..Default::default()
    })
    .unwrap();
    let endpoint = endpoint_builder(&config).unwrap().bind().await.unwrap();
    assert!(endpoint.bound_sockets().is_empty());
    endpoint.close().await;
}
