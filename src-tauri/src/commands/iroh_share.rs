//! Owner-authoritative Iroh snapshot protocol.  The invitation secret is only
//! present in the copied ticket; it is never placed in a normal backup.
use crate::errors::{CommandError, DomainError, ShareError};
use crate::{
    models::{
        Board, BoardRole, IrohDeviceStatus, IrohNetworkSettings, IrohPermission, StoredData,
        SyncStatus,
    },
    state::{lock_app, AppData, SharedAppData},
    storage::Database,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use futures_util::StreamExt;
use iroh::{
    address_lookup::{AddrFilter, PkarrPublisher, PkarrResolver},
    endpoint::{presets, BindOpts, Connection, RelayMode, TransportAddrUsage},
    Endpoint, EndpointAddr, EndpointId, RelayUrl, SecretKey, TransportAddr,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Mutex,
    time::Duration,
};
use tauri::{AppHandle, State};

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
    let guard = lock_app(&app)?;
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

fn ticket_payload(ticket: &str) -> Option<&str> {
    let ticket = ticket.trim();
    ticket
        .strip_prefix("cardbe://share/")
        // Old invitations remain usable after upgrading, but new links never
        // expose the transport implementation in their text.
        .or_else(|| ticket.strip_prefix("cardbe+iroh://"))
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

mod client;
mod host;
mod invites;
#[cfg(test)]
mod tests;

pub use client::*;
pub use host::*;
pub use invites::*;
