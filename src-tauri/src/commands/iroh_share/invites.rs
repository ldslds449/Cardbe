use super::*;
use qrcode::{render::svg, QrCode};

#[tauri::command]
pub async fn create_iroh_invite(
    app: State<'_, SharedAppData>,
    network: State<'_, IrohShareState>,
    app_handle: AppHandle,
    board_id: i64,
    permission: IrohPermission,
) -> Result<String, CommandError> {
    let (path, config) = {
        let guard = lock_app(&app)?;
        ensure_board_owned(guard.boards.iter().find(|board| board.id == board_id))?;
        (
            guard.database.path().to_path_buf(),
            IrohNetworkConfig::from_settings(&guard.stored.settings.iroh_network)?,
        )
    };
    let endpoint = ensure_host(&network, path, app_handle).await?;
    let invite_id = random_token();
    let secret = random_token();
    {
        let mut guard = lock_app(&app)?;
        ensure_board_owned(guard.boards.iter().find(|board| board.id == board_id))?;
        guard
            .database
            .save_iroh_invite(&invite_id, board_id, &secret, permission)
            .map_err(CommandError::repository)?;
        guard.boards = guard.database.boards().map_err(CommandError::repository)?;
    }
    let (node_id, endpoint_address) = share_address(&endpoint, &config);
    let ticket = Ticket {
        version: 2,
        node_id,
        endpoint: Some(endpoint_address),
        relay_urls: Vec::new(),
        invite_id,
        secret,
        permission,
    };
    Ok(format!(
        "cardbe://share/{}",
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&ticket).map_err(CommandError::internal)?)
    ))
}

#[tauri::command]
pub fn list_iroh_invites(
    app: State<'_, SharedAppData>,
) -> Result<Vec<IrohInviteSummary>, CommandError> {
    let guard = lock_app(&app)?;
    let names: HashMap<i64, String> = guard
        .boards
        .iter()
        .map(|b| (b.id, b.name.clone()))
        .collect();
    let devices = guard
        .database
        .iroh_devices()
        .map_err(CommandError::repository)?;
    guard
        .database
        .iroh_invite_summaries()
        .map_err(CommandError::repository)
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
) -> Result<(), CommandError> {
    lock_app(&app)?
        .database
        .set_iroh_device_approved(&invite_id, &node_id, approved)
        .map_err(CommandError::repository)
}

#[tauri::command]
pub async fn get_iroh_invite_access(
    app: State<'_, SharedAppData>,
    network: State<'_, IrohShareState>,
    app_handle: AppHandle,
    invite_id: String,
) -> Result<IrohInviteAccess, CommandError> {
    let (path, secret, permission, config) = {
        let guard = lock_app(&app)?;
        let (_, _, permission, enabled, _, secret) = guard
            .database
            .iroh_invite_summaries()
            .map_err(CommandError::repository)?
            .into_iter()
            .find(|item| item.0 == invite_id)
            .ok_or(DomainError::InviteNotFound)?;
        if !enabled {
            return Err(CommandError::InviteDisabled);
        }
        (
            guard.database.path().to_path_buf(),
            secret,
            permission,
            IrohNetworkConfig::from_settings(&guard.stored.settings.iroh_network)?,
        )
    };
    let endpoint = ensure_host(&network, path, app_handle).await?;
    let (node_id, endpoint_address) = share_address(&endpoint, &config);
    let ticket = Ticket {
        version: 2,
        node_id,
        endpoint: Some(endpoint_address),
        relay_urls: Vec::new(),
        invite_id,
        secret,
        permission,
    };
    let ticket = format!(
        "cardbe://share/{}",
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&ticket).map_err(CommandError::internal)?)
    );
    let qr_svg = render_invite_qr_svg(&ticket)?;
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
) -> Result<(), CommandError> {
    let path = {
        let mut guard = lock_app(&app)?;
        guard
            .database
            .update_iroh_invite(&invite_id, permission, enabled)
            .map_err(CommandError::repository)?;
        guard.database.path().to_path_buf()
    };
    if enabled == Some(true) {
        if let Err(error) = ensure_host(&network, path, app_handle).await {
            log::error!(target: "iroh", "Could not start the board-sharing service after invitation update: {error}");
            set_last_error(&network, Some(error.to_string()));
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
) -> Result<(), CommandError> {
    {
        let mut guard = lock_app(&app)?;
        guard
            .database
            .delete_iroh_invite(&invite_id)
            .map_err(CommandError::repository)?;
    }
    stop_host_if_idle(&network).await?;
    Ok(())
}

fn render_invite_qr_svg(ticket: &str) -> Result<String, DomainError> {
    let code = QrCode::new(ticket.as_bytes()).map_err(|_| DomainError::InviteInvalid)?;
    Ok(code.render::<svg::Color>().min_dimensions(240, 240).build())
}

#[tauri::command]
pub fn iroh_invite_qr_svg(ticket: String) -> Result<String, CommandError> {
    render_invite_qr_svg(&ticket).map_err(CommandError::from)
}
