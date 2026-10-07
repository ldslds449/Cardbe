//! Owner-authoritative Iroh snapshot protocol.  The invitation secret is only
//! present in the copied ticket; it is never placed in a normal backup.
use crate::errors::{CommandError, DomainError, ShareError};
use crate::{
    models::{
        Board, BoardRole, IrohDeviceStatus, IrohNetworkSettings, IrohPermission, StoredData,
        SyncStatus,
    },
    state::{AppData, SharedAppData},
    storage::Database,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use futures_util::StreamExt;
use iroh::{
    address_lookup::{AddrFilter, PkarrPublisher, PkarrResolver},
    endpoint::{presets, BindOpts, Connection, RelayMode, TransportAddrUsage},
    Endpoint, EndpointAddr, EndpointId, RelayUrl, SecretKey, TransportAddr,
};
use qrcode::{render::svg, QrCode};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
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
    #[serde(default)]
    endpoint: Option<EndpointAddr>,
    #[serde(default)]
    relay_urls: Vec<String>,
    invite_id: String,
    secret: String,
    permission: IrohPermission,
}
#[derive(Serialize, Deserialize)]
struct RemoteInvitation {
    node_id: String,
    #[serde(default)]
    endpoint: Option<EndpointAddr>,
    #[serde(default)]
    relay_urls: Vec<String>,
    invite_id: String,
    secret: String,
}
impl From<&Ticket> for RemoteInvitation {
    fn from(ticket: &Ticket) -> Self {
        Self {
            node_id: ticket.node_id.clone(),
            endpoint: ticket.endpoint.clone(),
            relay_urls: ticket.relay_urls.clone(),
            invite_id: ticket.invite_id.clone(),
            secret: ticket.secret.clone(),
        }
    }
}
fn endpoint_address(
    endpoint: Option<&EndpointAddr>,
    node_id: &str,
    relay_urls: &[String],
) -> Result<EndpointAddr, String> {
    let key: [u8; 32] = URL_SAFE_NO_PAD
        .decode(node_id)
        .map_err(|_| "Invalid invitation node ID")?
        .try_into()
        .map_err(|_| "Invalid invitation node ID")?;
    let id = EndpointId::from_bytes(&key).map_err(|_| "Invalid invitation node ID")?;
    if let Some(endpoint) = endpoint {
        if endpoint.id != id {
            return Err("Invitation endpoint and node ID do not match".into());
        }
        return Ok(endpoint.clone());
    }
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

#[derive(Clone, Debug)]
struct IrohNetworkConfig {
    direct_ip_enabled: bool,
    discovery_enabled: bool,
    discovery_urls: Vec<RelayUrl>,
    relay_mode: RelayMode,
    direct_addresses: Vec<SocketAddr>,
    listen_port: u16,
}

impl IrohNetworkConfig {
    fn from_settings(settings: &IrohNetworkSettings) -> Result<Self, crate::errors::DomainError> {
        use crate::errors::{DomainError, NetworkSettingsField};
        let direct_addresses: Vec<SocketAddr> = settings
            .direct_addresses
            .iter()
            .filter(|_| settings.direct_ip_enabled)
            .map(String::as_str)
            .map(str::trim)
            .filter(|address| !address.is_empty())
            .map(|address| {
                address
                    .parse()
                    .map_err(|_| DomainError::NetworkSettingsInvalid {
                        field: NetworkSettingsField::DirectAddresses,
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !direct_addresses.is_empty() && settings.listen_port == 0 {
            return Err(DomainError::NetworkSettingsInvalid {
                field: NetworkSettingsField::ListenPort,
            });
        }
        let relay_urls = settings
            .relay_urls
            .iter()
            .filter(|_| settings.relay_enabled)
            .map(String::as_str)
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .map(|url| {
                url.parse::<RelayUrl>()
                    .map_err(|_| DomainError::NetworkSettingsInvalid {
                        field: NetworkSettingsField::RelayUrls,
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let relay_mode = if !settings.relay_enabled {
            RelayMode::Disabled
        } else if relay_urls.is_empty() {
            RelayMode::Default
        } else {
            RelayMode::custom(relay_urls)
        };
        let discovery_urls = settings
            .discovery_urls
            .iter()
            .filter(|_| settings.discovery_enabled)
            .map(String::as_str)
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .map(|url| {
                url.parse::<RelayUrl>()
                    .map_err(|_| DomainError::NetworkSettingsInvalid {
                        field: NetworkSettingsField::DiscoveryUrls,
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            direct_ip_enabled: settings.direct_ip_enabled,
            discovery_enabled: settings.discovery_enabled,
            discovery_urls,
            relay_mode,
            direct_addresses,
            listen_port: settings.listen_port,
        })
    }
}

pub(crate) fn validate_network_settings(
    settings: &IrohNetworkSettings,
) -> Result<(), crate::errors::DomainError> {
    if !settings.direct_ip_enabled && !settings.relay_enabled {
        return Err(crate::errors::DomainError::InvalidArgument);
    }
    IrohNetworkConfig::from_settings(settings).map(|_| ())
}

fn endpoint_builder(config: &IrohNetworkConfig) -> Result<iroh::endpoint::Builder, String> {
    let mut builder = if config.discovery_enabled && config.discovery_urls.is_empty() {
        Endpoint::builder(presets::N0DisableRelay)
    } else {
        Endpoint::builder(presets::Minimal)
    };
    if config.discovery_enabled {
        for discovery_url in &config.discovery_urls {
            let publisher = PkarrPublisher::builder(discovery_url.clone().into()).addr_filter(
                if config.direct_ip_enabled {
                    AddrFilter::unfiltered()
                } else {
                    AddrFilter::relay_only()
                },
            );
            builder = builder
                .address_lookup(publisher)
                .address_lookup(PkarrResolver::builder(discovery_url.clone().into()));
        }
    }
    builder = builder.relay_mode(config.relay_mode.clone());
    if !config.direct_ip_enabled {
        return Ok(builder.clear_ip_transports());
    }
    if config.listen_port != 0 {
        builder = builder
            .bind_addr((Ipv4Addr::UNSPECIFIED, config.listen_port))
            .map_err(|error| error.to_string())?
            .bind_addr_with_opts(
                (Ipv6Addr::UNSPECIFIED, config.listen_port),
                BindOpts::default().set_is_required(false),
            )
            .map_err(|error| error.to_string())?;
    }
    for address in &config.direct_addresses {
        builder = builder.external_addr(*address);
    }
    Ok(builder)
}

fn endpoint_address_for_ticket(endpoint: &Endpoint, config: &IrohNetworkConfig) -> EndpointAddr {
    let mut address = endpoint.addr();
    address.addrs.extend(
        config
            .relay_mode
            .relay_map()
            .urls::<Vec<RelayUrl>>()
            .into_iter()
            .map(TransportAddr::Relay),
    );
    if !config.direct_ip_enabled {
        address.addrs.retain(|address| !address.is_ip());
    } else {
        address.addrs.extend(
            config
                .direct_addresses
                .iter()
                .copied()
                .map(TransportAddr::Ip),
        );
    }
    address
}

async fn connect(
    endpoint: &Endpoint,
    address: EndpointAddr,
    network: &IrohShareState,
) -> Result<Connection, String> {
    let direct_addresses: Vec<_> = address.ip_addrs().collect();
    let relay_urls: Vec<_> = address.relay_urls().collect();
    log::debug!(target: "iroh", "Connecting to endpoint {} with direct addresses {:?} and relays {:?}", address.id, direct_addresses, relay_urls);
    // Invitation addresses are hints: the owner may have restarted or changed
    // transports since this board was joined. Refresh without delaying a
    // working local/offline connection.
    let attempt = async {
        let initial = endpoint.connect(address.clone(), ALPN);
        let refresh = refresh_endpoint_address(endpoint, address.clone());
        tokio::pin!(initial, refresh);
        tokio::select! {
            result = &mut initial => match result {
                Ok(connection) => Ok(connection),
                Err(error) => {
                    let refreshed = refresh.await;
                    if refreshed.addrs != address.addrs {
                        endpoint.connect(refreshed, ALPN).await
                    } else {
                        Err(error)
                    }
                }
            },
            refreshed = &mut refresh => {
                if refreshed.addrs != address.addrs {
                    endpoint.connect(refreshed, ALPN).await
                } else {
                    initial.await
                }
            }
        }
    };
    let connection = tokio::time::timeout(CONNECT_TIMEOUT, attempt)
        .await
        .map_err(|_| {
            connection_failure(endpoint, &address, "Connection timed out after 20 seconds")
        })?
        .map_err(|error| connection_failure(endpoint, &address, &error.to_string()))?;
    record_connection_path(endpoint, connection.remote_id(), network).await;
    Ok(connection)
}

async fn refresh_endpoint_address(endpoint: &Endpoint, mut address: EndpointAddr) -> EndpointAddr {
    let Ok(lookup) = endpoint.address_lookup() else {
        return address;
    };
    if lookup.is_empty() {
        return address;
    }
    let resolve = async {
        let results = lookup.resolve(address.id);
        tokio::pin!(results);
        while let Some(result) = results.next().await {
            match result {
                Ok(Ok(item)) => {
                    let fresh = item.into_endpoint_addr();
                    if fresh.id == address.id && !fresh.addrs.is_subset(&address.addrs) {
                        address.addrs.extend(fresh.addrs);
                        break;
                    }
                }
                error => {
                    log::debug!(target: "iroh", "Address refresh for {}: {error:?}", address.id)
                }
            }
        }
    };
    let _ = tokio::time::timeout(Duration::from_secs(3), resolve).await;
    address
}

fn connection_failure(endpoint: &Endpoint, address: &EndpointAddr, detail: &str) -> String {
    let discovery = endpoint
        .address_lookup()
        .is_ok_and(|lookup| !lookup.is_empty());
    let hint = if !discovery {
        "Automatic device discovery is off. The saved invitation address may be outdated; enable discovery on both devices or use the owner's current address."
    } else if address.addrs.is_empty() {
        "No owner address was available in the invitation. Discovery did not produce a reachable path. Check that the owner is sharing and both devices use the same discovery service."
    } else if endpoint.bound_sockets().is_empty() {
        "This device allows relay connections only. Check that the owner has relay enabled and their relay is reachable, or switch both devices to Automatic."
    } else {
        "The owner could not be reached through the available paths. Keep their Cardbe open; direct-only connections can fail across networks. Try Automatic on both devices and check any custom relay or discovery services."
    };
    format!("Could not connect to the board owner. {hint} Details: {detail}")
}

async fn record_connection_path(
    endpoint: &Endpoint,
    endpoint_id: EndpointId,
    network: &IrohShareState,
) {
    let Some(info) = endpoint.remote_info(endpoint_id).await else {
        log::debug!(target: "iroh", "Connection to {endpoint_id} established; Iroh has not reported a path yet");
        return;
    };
    let active_paths: Vec<_> = info
        .addrs()
        .filter(|address| matches!(address.usage(), TransportAddrUsage::Active))
        .map(|address| {
            let transport = address.addr();
            let kind = if transport.is_ip() {
                "Direct"
            } else if transport.is_relay() {
                "Relay"
            } else {
                "Custom"
            };
            IrohConnectionPath {
                kind: kind.into(),
                address: transport.to_string(),
            }
        })
        .collect();
    log::info!(target: "iroh", "Connection to {endpoint_id} established; active paths: {active_paths:?}");
    let observed_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    remember_connection(
        network,
        URL_SAFE_NO_PAD.encode(endpoint_id.as_bytes()),
        IrohConnectionInfo {
            observed_at,
            paths: active_paths,
        },
    );
}

fn remember_connection(network: &IrohShareState, node_id: String, info: IrohConnectionInfo) {
    if let Ok(mut connections) = network.connections.lock() {
        // ponytail: keep 128 recent peers in memory; persist history only if diagnostics need it.
        if connections.len() >= 128 && !connections.contains_key(&node_id) {
            if let Some(oldest) = connections
                .iter()
                .min_by_key(|(_, info)| info.observed_at)
                .map(|(id, _)| id.clone())
            {
                connections.remove(&oldest);
            }
        }
        connections.insert(node_id, info);
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct IrohConnectionPath {
    kind: String,
    address: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct IrohConnectionInfo {
    observed_at: u64,
    paths: Vec<IrohConnectionPath>,
}

#[derive(Serialize)]
pub struct IrohConnectionDetails {
    invitation: Option<IrohConnectionInfo>,
    devices: HashMap<String, IrohConnectionInfo>,
    boards: HashMap<i64, IrohConnectionInfo>,
}

#[derive(Serialize)]
pub struct IrohSyncResult {
    board: Board,
    content_changed: bool,
    owner_sync: Option<OwnerSyncSummary>,
}

#[derive(Serialize)]
struct OwnerSyncSummary {
    synced_devices: usize,
    failed_devices: usize,
}

#[tauri::command]
pub fn get_iroh_connection_details(
    app: State<'_, SharedAppData>,
    network: State<'_, IrohShareState>,
    board_ids: Vec<i64>,
    ticket: Option<String>,
) -> Result<IrohConnectionDetails, CommandError> {
    let devices = network
        .connections
        .lock()
        .map_err(|_| CommandError::internal("Connection details are unavailable"))?
        .clone();
    let invitation = ticket
        .filter(|ticket| !ticket.trim().is_empty())
        .map(|ticket| parse_ticket(&ticket))
        .transpose()?
        .and_then(|ticket| devices.get(&ticket.node_id).cloned());
    let guard = app
        .lock()
        .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
    let mut boards = HashMap::new();
    for board_id in board_ids {
        if let Some(remote) = guard
            .database
            .iroh_remote(board_id)
            .map_err(CommandError::repository)?
        {
            if let Ok(remote) = serde_json::from_str::<RemoteInvitation>(&remote) {
                if let Some(info) = devices.get(&remote.node_id) {
                    boards.insert(board_id, info.clone());
                }
            }
        }
    }
    Ok(IrohConnectionDetails {
        invitation,
        devices,
        boards,
    })
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
    #[serde(default)]
    status_only: bool,
    // An optional field keeps the v1 action enum intact. Older peers reject
    // this request through their existing invitation checks without applying it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    owner_pull: bool,
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

fn editor_upload(local: &[u8], owner: &Snapshot) -> Result<Vec<u8>, String> {
    if owner.permission != IrohPermission::Editor {
        return Ok(Vec::new());
    }
    // Only status responses carry a vector; legacy hosts return a snapshot.
    if owner.unchanged && !owner.loro_update.is_empty() {
        crate::loro_board::diff(Some(local), &owner.loro_update)
    } else {
        Ok(local.to_vec())
    }
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SnapshotErrorCode {
    ApprovalRequired,
    AccessRevoked,
    InvitationDisabled,
    InvitationDeleted,
    #[serde(other)]
    Unknown,
}

fn access_error_code(error: &str) -> Option<SnapshotErrorCode> {
    if error.starts_with("Access was declined or revoked") {
        Some(SnapshotErrorCode::AccessRevoked)
    } else if error == "Invitation is disabled" {
        Some(SnapshotErrorCode::InvitationDisabled)
    } else if error == "Invitation was revoked" {
        Some(SnapshotErrorCode::InvitationDeleted)
    } else if error.starts_with("Waiting for owner approval") {
        Some(SnapshotErrorCode::ApprovalRequired)
    } else {
        None
    }
}

fn snapshot_error(snapshot: &Snapshot) -> ShareError {
    snapshot_failure(snapshot, String::new())
}

fn snapshot_requires_approval(snapshot: &Snapshot) -> bool {
    snapshot.error_code == Some(SnapshotErrorCode::ApprovalRequired)
        // Accept the old wire format, which only sent the user-facing message.
        || (snapshot.error_code.is_none()
            && snapshot
                .error
                .as_deref()
                .is_some_and(|error| error.starts_with("Waiting for owner approval")))
}

// Backward-compatible wire adapter only. Legacy peer wording never crosses IPC.
fn snapshot_access_revoked(snapshot: &Snapshot) -> bool {
    snapshot.error_code == Some(SnapshotErrorCode::AccessRevoked)
        || (snapshot.error_code.is_none()
            && snapshot
                .error
                .as_deref()
                .is_some_and(|error| error.starts_with("Access was declined or revoked")))
}
fn snapshot_failure(snapshot: &Snapshot, device_id: String) -> ShareError {
    if snapshot_requires_approval(snapshot) {
        ShareError::ApprovalRequired { device_id }
    } else if snapshot_access_revoked(snapshot) {
        ShareError::AccessTerminated
    } else if matches!(
        snapshot
            .error_code
            .or_else(|| snapshot.error.as_deref().and_then(access_error_code)),
        Some(SnapshotErrorCode::InvitationDeleted)
    ) {
        ShareError::InvitationDeleted
    } else if matches!(
        snapshot
            .error_code
            .or_else(|| snapshot.error.as_deref().and_then(access_error_code)),
        Some(SnapshotErrorCode::InvitationDisabled)
    ) {
        ShareError::InvitationDisabled
    } else if snapshot.error_code.is_none()
        && snapshot.error.as_deref() == Some("Invitation was revoked or disabled")
    {
        // Older peers cannot distinguish a pause from permanent revocation.
        ShareError::AccessRevoked
    } else {
        ShareError::Internal("Remote sharing request failed".into())
    }
}

fn validate_sync_snapshot(snapshot: &Snapshot) -> Result<(), ShareError> {
    // Rejected requests contain no board snapshot. Keep compatibility with
    // older hosts, which identify these responses by an empty name/revision.
    if !snapshot.ok
        && (snapshot.error_code.is_some() || (snapshot.name.is_empty() && snapshot.revision == 0))
    {
        return Err(snapshot_error(snapshot));
    }
    Ok(())
}

fn refresh_saved_conflict(
    app: &mut AppData,
    board_id: i64,
    owner: &Snapshot,
) -> Result<Option<Board>, ShareError> {
    validate_sync_snapshot(owner)?;
    if !owner.ok || owner.unchanged {
        return Err(snapshot_error(owner));
    }
    // Resolving the conflict while fetching must not recreate it.
    if app
        .database
        .iroh_conflict(board_id)
        .map_err(DomainError::repository)?
        .is_none()
    {
        return Ok(None);
    }
    app.database
        .save_iroh_conflict(
            board_id,
            owner.revision,
            &owner.name,
            owner.permission,
            &owner.data,
            Some(&owner.loro_update),
        )
        .map_err(DomainError::repository)?;
    let board = app
        .boards
        .iter_mut()
        .find(|board| board.id == board_id)
        .ok_or(DomainError::BoardNotFound)?;
    board.shared_role = owner.permission.into();
    board.sync_status = SyncStatus::Conflict;
    Ok(Some(board.clone()))
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
    automatic_direct_port: Mutex<Option<u16>>,
    init: tokio::sync::Mutex<()>,
    last_error: Mutex<Option<String>>,
    connections: Mutex<HashMap<String, IrohConnectionInfo>>,
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

fn parse_ticket(ticket: &str) -> Result<Ticket, DomainError> {
    let encoded = ticket_payload(ticket).ok_or(DomainError::InviteInvalid)?;
    let decoded = URL_SAFE_NO_PAD.decode(encoded).map_err(|error| {
        log::warn!(target: "iroh", "Could not decode shared-board invitation: {error}");
        DomainError::InviteInvalid
    })?;
    let ticket: Ticket = serde_json::from_slice(&decoded).map_err(|error| {
        log::warn!(target: "iroh", "Could not parse shared-board invitation: {error}");
        DomainError::InviteInvalid
    })?;
    if !(1..=2).contains(&ticket.version) || ticket.invite_id.is_empty() || ticket.secret.is_empty()
    {
        return Err(DomainError::InviteInvalid);
    }
    endpoint_address(
        ticket.endpoint.as_ref(),
        &ticket.node_id,
        &ticket.relay_urls,
    )
    .map_err(|_| DomainError::InviteInvalid)?;
    Ok(ticket)
}

fn ensure_invite_not_joined(db: &Database, ticket: &Ticket) -> Result<(), DomainError> {
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
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

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
            tasks: Vec::new(),
        });
        let doc =
            crate::loro_board::apply_local_delta(None, &StoredData::default(), &base).unwrap();
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
                    loro_update: loro_board::diff(Some(&merged), &received.loro_state_vector)
                        .unwrap(),
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
        snapshot.error =
            Some("Access was declined or revoked. You can request access again.".into());
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
        let (outgoing, incoming) =
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
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
        let (outgoing, incoming) =
            tokio::time::timeout(std::time::Duration::from_secs(10), async {
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
}

fn viewer_is_current(request: &Request, permission: IrohPermission, revision: i64) -> bool {
    permission == IrohPermission::Viewer
        && request.known_permission == Some(IrohPermission::Viewer)
        && request.known_revision == Some(revision)
}

// Tickets carry Iroh's native EndpointAddr so direct, relay, and future custom
// transports remain distinguishable. The stable endpoint key stays local.
fn share_address(endpoint: &Endpoint, config: &IrohNetworkConfig) -> (String, EndpointAddr) {
    let address = endpoint_address_for_ticket(endpoint, config);
    (URL_SAFE_NO_PAD.encode(address.id.as_bytes()), address)
}

fn ensure_board_owned(board: Option<&Board>) -> Result<(), DomainError> {
    let board = board.ok_or(DomainError::BoardNotFound)?;
    if board.shared_role != BoardRole::Owner {
        return Err(DomainError::PermissionDenied);
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
    if request.status_only
        || (error.is_none() && viewer_is_current(request, permission, board.sync_revision))
    {
        return Ok(Snapshot {
            ok: true,
            error: None,
            error_code: None,
            name: board.name,
            permission,
            revision: board.sync_revision,
            data: StoredData::default(),
            // Status probes let editors upload only history the owner lacks.
            loro_update: if request.status_only && permission == IrohPermission::Editor {
                crate::loro_board::state_vector(Some(
                    &db.ensure_iroh_loro_doc(board_id)
                        .map_err(|e| e.to_string())?,
                ))?
            } else {
                Vec::new()
            },
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
    let config = {
        let app = app_handle.state::<SharedAppData>();
        let guard = app
            .lock()
            .map_err(|_| "Application state lock is poisoned")?;
        IrohNetworkConfig::from_settings(&guard.stored.settings.iroh_network)
            .map_err(|error| error.to_string())?
    };
    log::debug!(target: "iroh", "Starting Iroh endpoint: direct IP {}, discovery {}, discovery services {:?}, relay {:?}", config.direct_ip_enabled, config.discovery_enabled, config.discovery_urls, config.relay_mode);
    let endpoint = bind_endpoint(&config, key, state).await?;
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
                    Ok(request) if request.owner_pull => {
                        let app = app_handle.state::<SharedAppData>();
                        let result = app.lock()
                            .map_err(|_| "Application state lock is poisoned".to_string())
                            .and_then(|state| {
                                editor_snapshot_for_owner(&state.database, &request, &peer_id)
                                    .map_err(|_| "Owner-initiated sync request rejected".to_string())
                            });
                        result
                    }
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
                        Ok(invite) if request.status_only || matches!(request.action, IrohAction::Pull | IrohAction::Request) => {
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
                                let applied = db.apply_iroh_loro_update(
                                        invite.board_id,
                                        &request.invite_id,
                                        &request.secret,
                                        &peer_id,
                                        &request.loro_update,
                                    );
                                let (changed, _, _) = match applied {
                                    Ok(result) => result,
                                    Err(error) if error.to_string() == "Invitation no longer grants editing access" => {
                                        drop(state);
                                        return snapshot_from_db(
                                            &app_handle, &path, invite.board_id,
                                            Some(error.to_string()), &request, &peer_id,
                                        );
                                    }
                                    Err(error) => return Err(error.to_string()),
                                };
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
                                        state.archives_loaded = true;
                                        state.refresh_labels();
                                        state.undo_history.clear();
                                    }
                                    applied_push = Some((invite.board_id, fresh.sync_revision));
                                }
                                let snapshot = Snapshot {
                                    ok: true, error: None, error_code: None,
                                    name: fresh.name, permission: IrohPermission::Editor,
                                    revision: fresh.sync_revision, data: StoredData::default(),
                                    loro_update: response_update, unchanged: false,
                                };
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
            set_last_error(network, Some(error));
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
        let guard = app
            .lock()
            .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
        (
            guard.database.path().to_path_buf(),
            guard
                .database
                .iroh_host_required()
                .map_err(CommandError::repository)?,
        )
    };
    if active {
        if let Err(error) = ensure_host(&network, path, app_handle).await {
            set_last_error(&network, Some(error.clone()));
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
    let path = if let Some(hosted) = hosted {
        let path = hosted.path;
        hosted.endpoint.close().await;
        path
    } else {
        let app = app_handle.state::<SharedAppData>();
        let guard = app
            .lock()
            .map_err(|_| "Application state lock is poisoned")?;
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
        set_last_error(network, Some(error.clone()));
        return Err(error);
    }
    Ok(())
}

async fn bind_endpoint(
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

#[tauri::command]
pub async fn create_iroh_invite(
    app: State<'_, SharedAppData>,
    network: State<'_, IrohShareState>,
    app_handle: AppHandle,
    board_id: i64,
    permission: IrohPermission,
) -> Result<String, CommandError> {
    let (path, config) = {
        let guard = app
            .lock()
            .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
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
        let mut guard = app
            .lock()
            .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
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
    let guard = app
        .lock()
        .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
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
    app.lock()
        .map_err(|_| CommandError::internal("Application state lock is poisoned"))?
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
        let guard = app
            .lock()
            .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
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
        let mut guard = app
            .lock()
            .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
        guard
            .database
            .update_iroh_invite(&invite_id, permission, enabled)
            .map_err(CommandError::repository)?;
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
) -> Result<(), CommandError> {
    {
        let mut guard = app
            .lock()
            .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
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
        let mut guard = app
            .lock()
            .map_err(|_| DomainError::Internal("Application state lock is poisoned".into()))?;
        ensure_invite_not_joined(&guard.database, &ticket)?;
        device_key(&mut guard.database)?
    };
    let device_id = URL_SAFE_NO_PAD.encode(key.public().as_bytes());
    let path = app
        .lock()
        .map_err(|_| "Application state lock is poisoned")?
        .database
        .path()
        .to_path_buf();
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
    let mut guard = app
        .lock()
        .map_err(|_| DomainError::Internal("Application state lock is poisoned".into()))?;
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
        let mut guard = app
            .lock()
            .map_err(|_| DomainError::Internal("Application state lock is poisoned".into()))?;
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
    let path = app
        .lock()
        .map_err(|_| "Application state lock is poisoned")?
        .database
        .path()
        .to_path_buf();
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
            let guard = app
                .lock()
                .map_err(|_| "Application state lock is poisoned")?;
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
            let mut guard = app
                .lock()
                .map_err(|_| "Application state lock is poisoned")?;
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

async fn exchange_sync_request(
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

// Only the authenticated owner of this exact invitation may read an editor's
// unsent CRDT operations. Reading leaves the editor's pending state untouched.
fn editor_snapshot_for_owner(
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
            || remote.secret != request.secret
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

async fn sync_owned_iroh_board(
    app: &SharedAppData,
    app_handle: AppHandle,
    network: &IrohShareState,
    board_id: i64,
) -> Result<IrohSyncResult, ShareError> {
    let (path, peers) = {
        let guard = app
            .lock()
            .map_err(|_| "Application state lock is poisoned")?;
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
    let endpoint = ensure_host(network, path, app_handle).await?;
    let mut content_changed = false;
    let mut outcomes = HashMap::new();
    for (node_id, (invite_id, secret)) in peers {
        let result = async {
            let update = {
                let guard = app
                    .lock()
                    .map_err(|_| "Application state lock is poisoned")?;
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
            let mut guard = app
                .lock()
                .map_err(|_| "Application state lock is poisoned")?;
            // Recheck approval inside the existing atomic merge, including
            // revocation or permission changes while the request was in flight.
            let (changed, _, _) = guard
                .database
                .apply_iroh_loro_update(
                    board_id,
                    &invite_id,
                    &secret,
                    &node_id,
                    &response.loro_update,
                )
                .map_err(DomainError::repository)?;
            content_changed |= changed;
            guard.boards = guard.database.boards().map_err(DomainError::repository)?;
            if changed && guard.active_board_id == board_id {
                guard.stored = guard
                    .database
                    .load_board(board_id)
                    .map_err(DomainError::repository)?;
                guard.archives_loaded = true;
                guard.refresh_labels();
                guard.undo_history.clear();
            }
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
    let guard = app
        .lock()
        .map_err(|_| "Application state lock is poisoned")?;
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
        let guard = app
            .lock()
            .map_err(|_| "Application state lock is poisoned")?;
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
        let guard = app
            .lock()
            .map_err(|_| DomainError::Internal("Application state lock is poisoned".into()))?;
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
        let guard = app
            .lock()
            .map_err(|_| "Application state lock is poisoned")?;
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
    let path = app
        .lock()
        .map_err(|_| "Application state lock is poisoned")?
        .database
        .path()
        .to_path_buf();
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
    let mut guard = app
        .lock()
        .map_err(|_| "Application state lock is poisoned")?;
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
            guard.stored = guard
                .database
                .load_board(board_id)
                .map_err(DomainError::repository)?;
            guard.archives_loaded = true;
            guard.refresh_labels();
            guard.undo_history.clear();
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
        guard.stored = guard
            .database
            .load_board(board_id)
            .map_err(DomainError::repository)?;
        guard.archives_loaded = true;
        guard.refresh_labels();
        guard.undo_history.clear();
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
        let guard = app
            .lock()
            .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
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
    let mut guard = app
        .lock()
        .map_err(|_| CommandError::internal("Application state lock is poisoned"))?;
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
            guard.stored = guard
                .database
                .load_board(board_id)
                .map_err(CommandError::repository)?;
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
