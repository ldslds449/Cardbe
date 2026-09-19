use axum::{
    body::Body,
    extract::{Request, State as AxumState},
    http::{header, Method, StatusCode},
    response::Response,
    routing::any,
    Router,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, TcpListener as StdTcpListener},
    sync::{Arc, Mutex, RwLock},
    thread,
};
use tauri::{AppHandle, State};
#[cfg(dev)]
mod development;
mod transport;
#[cfg(dev)]
use development::{development_target_allowed, proxy_vite_websocket, vite_websocket_protocol};

#[cfg(dev)]
use axum::extract::{ws::WebSocketUpgrade, FromRequestParts};
#[cfg(all(dev, test))]
use axum::http::HeaderValue;
#[cfg(all(dev, test))]
use std::path::Path;

#[derive(Clone)]
struct ViewerAsset {
    bytes: Vec<u8>,
    mime_type: String,
}

type AssetLoader = Arc<dyn Fn(&str) -> Option<ViewerAsset> + Send + Sync>;

#[derive(Clone)]
struct LanShare {
    snapshot: Value,
    updated_at: String,
    expires_at: Option<DateTime<Utc>>,
}

#[derive(Default)]
pub struct LanShareState {
    shares: Arc<RwLock<HashMap<String, LanShare>>>,
    retired_ids: Mutex<HashSet<String>>,
    server: Mutex<Option<LanServer>>,
}

struct LanServer {
    port: u16,
    host: String,
}

#[derive(Clone)]
struct LanHttpState {
    shares: Arc<RwLock<HashMap<String, LanShare>>>,
    assets: ViewerAssets,
}

#[derive(Clone)]
enum ViewerAssets {
    #[cfg_attr(dev, allow(dead_code))]
    Embedded(AssetLoader),
    #[cfg(dev)]
    Vite(reqwest::Client),
}

#[derive(Serialize)]
pub struct LanShareResponse {
    id: String,
    url: String,
    updated_at: String,
    expires_at: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct PublicShareResponse {
    id: String,
    updated_at: String,
    expires_at: Option<String>,
    snapshot: Value,
}

fn valid_share_id(id: &str) -> bool {
    (22..=64).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn production_asset_allowed(path: &str) -> bool {
    path == "favicon.png"
        || (path.starts_with("_app/")
            && !path.contains("..")
            && !path.contains('\\')
            && !path.contains('%')
            && !path.contains(':'))
}

fn is_public_viewer_asset(path: &str) -> bool {
    if production_asset_allowed(path) {
        return true;
    }

    // During `tauri dev`, Vite serves source modules instead of the hashed
    // `_app` bundle. Keep this allowlist narrow: the LAN viewer must never
    // expose arbitrary files through Vite's `/@fs/` endpoint.
    #[cfg(dev)]
    {
        development_target_allowed(path)
    }
    #[cfg(not(dev))]
    {
        false
    }
}

fn validate_snapshot(snapshot: &Value) -> Result<(), String> {
    let object = snapshot
        .as_object()
        .ok_or_else(|| "Invalid share snapshot".to_string())?;
    if object.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err("Unsupported share schema".into());
    }
    let title = object
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if title.trim().is_empty() || title.chars().count() > 120 {
        return Err("Invalid shared board name".into());
    }
    let columns = object
        .get("columns")
        .and_then(Value::as_array)
        .ok_or_else(|| "Invalid shared columns".to_string())?;
    if columns.is_empty() || columns.len() > 50 {
        return Err("A share must contain between 1 and 50 columns".into());
    }
    let task_count = columns
        .iter()
        .filter_map(|column| column.get("tasks").and_then(Value::as_array))
        .map(Vec::len)
        .sum::<usize>();
    if task_count > 2_000 {
        return Err("A share can contain at most 2000 tasks".into());
    }
    if serde_json::to_vec(snapshot)
        .map_err(|error| error.to_string())?
        .len()
        > 512 * 1024
    {
        return Err("Share payload is too large".into());
    }
    Ok(())
}

fn parse_expiration(expires_at: Option<String>) -> Result<Option<DateTime<Utc>>, String> {
    expires_at
        .map(|value| {
            let parsed = DateTime::parse_from_rfc3339(&value)
                .map_err(|_| "Invalid expiration date".to_string())?
                .with_timezone(&Utc);
            if parsed <= Utc::now() {
                return Err("Expiration must be in the future".into());
            }
            Ok(parsed)
        })
        .transpose()
}

fn format_host(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(ip) => ip.to_string(),
        IpAddr::V6(ip) => format!("[{ip}]"),
    }
}

fn is_private_lan_address(ip: IpAddr) -> bool {
    match ip {
        // RFC 1918 and IPv4 link-local addresses are reachable only from the
        // local network. Do not accidentally publish through a public address
        // selected by the operating system's default route.
        IpAddr::V4(ip) => ip.is_private() || ip.is_link_local(),
        // IPv6 ULA (fc00::/7) and link-local (fe80::/10) have the same local
        // scope. Global IPv6 addresses are intentionally rejected.
        IpAddr::V6(ip) => ip.is_unicast_link_local() || (ip.segments()[0] & 0xfe00) == 0xfc00,
    }
}

impl LanShareState {
    fn replace_share(
        &self,
        share_id: String,
        share: LanShare,
        allow_reactivate: bool,
    ) -> Result<(), String> {
        // Serialize publication and revocation, including requests already in
        // flight when the user disables or replaces a link.
        let mut retired = self
            .retired_ids
            .lock()
            .map_err(|_| "LAN share lifecycle lock is poisoned".to_string())?;
        if retired.contains(&share_id) {
            if allow_reactivate {
                retired.remove(&share_id);
            } else {
                return Err("This share link was disabled. Create a new share link.".into());
            }
        }
        let mut shares = self
            .shares
            .write()
            .map_err(|_| "LAN share data lock is poisoned".to_string())?;
        // Each capability ID represents an independently managed share. Updating
        // one link must not invalidate other active links.
        shares.insert(share_id, share);
        Ok(())
    }

    fn revoke(&self, share_id: String) -> Result<(), String> {
        let mut retired = self
            .retired_ids
            .lock()
            .map_err(|_| "LAN share lifecycle lock is poisoned".to_string())?;
        let mut shares = self
            .shares
            .write()
            .map_err(|_| "LAN share data lock is poisoned".to_string())?;
        retired.insert(share_id.clone());
        shares.remove(&share_id);
        Ok(())
    }

    fn ensure_server(
        &self,
        _app: &AppHandle,
        preferred_port: Option<u16>,
    ) -> Result<(String, u16), String> {
        let mut server_guard = self
            .server
            .lock()
            .map_err(|_| "LAN share server lock is poisoned".to_string())?;
        if let Some(server) = server_guard.as_ref() {
            return Ok((server.host.clone(), server.port));
        }

        let ip = local_ip_address::local_ip()
            .map_err(|error| format!("Could not determine this computer's LAN address: {error}"))?;
        if ip.is_loopback() {
            return Err("No LAN network connection was found".into());
        }
        if !is_private_lan_address(ip) {
            return Err("LAN sharing requires a private local-network address".into());
        }
        // Bind only the selected LAN interface, rather than exposing the
        // temporary viewer on every network interface on this computer.
        #[cfg(dev)]
        let requested_port = development::share_port()?;
        #[cfg(not(dev))]
        let requested_port = preferred_port.unwrap_or(0);
        if preferred_port == Some(0) {
            return Err("Invalid preferred LAN sharing port".into());
        }
        let listener = StdTcpListener::bind((ip, requested_port))
            .map_err(|error| format!("Could not start LAN sharing: {error}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("Could not configure LAN sharing: {error}"))?;
        let port = listener
            .local_addr()
            .map_err(|error| error.to_string())?
            .port();
        let host = format_host(ip);
        #[cfg(not(dev))]
        let assets = {
            let app = _app.clone();
            let loader: AssetLoader = Arc::new(move |path| {
                app.asset_resolver()
                    .get(path.to_string())
                    .map(|asset| ViewerAsset {
                        bytes: asset.bytes,
                        mime_type: asset.mime_type,
                    })
            });
            ViewerAssets::Embedded(loader)
        };
        #[cfg(dev)]
        let assets = ViewerAssets::Vite(development::client()?);
        let http_state = LanHttpState {
            shares: Arc::clone(&self.shares),
            assets,
        };
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("cardbe-lan-share-worker")
            .build()
            .map_err(|error| format!("Could not initialize LAN sharing: {error}"))?;
        thread::Builder::new()
            .name("cardbe-lan-share".into())
            .spawn(move || {
                runtime.block_on(async move {
                    let Ok(listener) = tokio::net::TcpListener::from_std(listener) else {
                        return;
                    };
                    let router = lan_router(http_state);
                    let _ = axum::serve(transport::BoundedListener::new(listener), router).await;
                });
            })
            .map_err(|error| format!("Could not start LAN sharing: {error}"))?;
        *server_guard = Some(LanServer {
            port,
            host: host.clone(),
        });
        Ok((host, port))
    }
}

#[tauri::command]
pub fn publish_lan_share(
    app: AppHandle,
    state: State<'_, LanShareState>,
    snapshot: Value,
    expires_at: Option<String>,
    share_id: String,
    allow_reactivate: bool,
    preferred_port: Option<u16>,
) -> Result<LanShareResponse, String> {
    if !valid_share_id(&share_id) {
        return Err("Invalid share ID".into());
    }
    validate_snapshot(&snapshot)?;
    let expires_at = parse_expiration(expires_at)?;
    let (host, port) = state.ensure_server(&app, preferred_port)?;
    let updated_at = Utc::now().to_rfc3339();
    state.replace_share(
        share_id.clone(),
        LanShare {
            snapshot,
            updated_at: updated_at.clone(),
            expires_at,
        },
        allow_reactivate,
    )?;
    Ok(LanShareResponse {
        id: share_id.clone(),
        url: format!("http://{host}:{port}/share/{share_id}"),
        updated_at,
        expires_at: expires_at.map(|value| value.to_rfc3339()),
    })
}

#[tauri::command]
pub fn revoke_lan_share(state: State<'_, LanShareState>, share_id: String) -> Result<(), String> {
    state.revoke(share_id)
}

fn lan_router(state: LanHttpState) -> Router {
    Router::new().fallback(any(lan_request)).with_state(state)
}

async fn lan_request(AxumState(state): AxumState<LanHttpState>, request: Request) -> Response {
    #[cfg(dev)]
    if matches!(&state.assets, ViewerAssets::Vite(_)) {
        // A share URL grants access to development assets. Knowing the LAN
        // address alone must not grant access to source modules or HMR.
        let share_page = request
            .uri()
            .path()
            .strip_prefix("/share/")
            .filter(|id| valid_share_id(id) && active_share(&state.shares, id).is_some());
        let viewer = development::authorized_viewer(request.headers(), &state.shares);
        if share_page.is_none()
            && !request.uri().path().starts_with("/api/shares/")
            && viewer.is_none()
        {
            return secured_response(
                StatusCode::FORBIDDEN,
                "text/plain",
                b"Open an active share link first".to_vec(),
                None,
            );
        }
        if let Some(protocol) = vite_websocket_protocol(request.headers()) {
            let Some(viewer) = viewer else {
                return secured_response(StatusCode::FORBIDDEN, "text/plain", vec![], None);
            };
            if request.uri().path() != "/" || !development::same_origin(request.headers()) {
                return secured_response(StatusCode::FORBIDDEN, "text/plain", vec![], None);
            }
            let target = format!(
                "{}{}",
                development::vite_origin("ws"),
                request
                    .uri()
                    .path_and_query()
                    .map_or("/", |value| value.as_str())
            );
            let (mut parts, _) = request.into_parts();
            let Ok(websocket) = WebSocketUpgrade::from_request_parts(&mut parts, &state).await
            else {
                return secured_response(
                    StatusCode::BAD_REQUEST,
                    "text/plain; charset=utf-8",
                    b"Invalid WebSocket upgrade".to_vec(),
                    None,
                );
            };
            return websocket.protocols(["vite-hmr", "vite-ping"]).on_upgrade(
                move |socket| async move {
                    tokio::select! {
                        _ = proxy_vite_websocket(socket, target, protocol) => {},
                        _ = async {
                            loop {
                                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                                if active_share(&state.shares, &viewer).is_none() { break; }
                            }
                        } => {},
                    }
                },
            );
        }
    }

    if request.method() != Method::GET {
        return secured_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "text/plain; charset=utf-8",
            b"Method not allowed".to_vec(),
            None,
        );
    }

    let path = request.uri().path();
    if let Some(id) = path.strip_prefix("/share/") {
        if valid_share_id(id) && active_share(&state.shares, id).is_some() {
            let target = request
                .uri()
                .path_and_query()
                .map_or(path.trim_start_matches('/'), |value| {
                    value.as_str().trim_start_matches('/')
                });
            let response = viewer_page_response(&state.assets, target).await;
            #[cfg(dev)]
            let response = {
                let mut response = response;
                if matches!(&state.assets, ViewerAssets::Vite(_)) && response.status().is_success()
                {
                    response.headers_mut().insert(
                        header::SET_COOKIE,
                        format!("cardbe_dev_share={id}; Path=/; HttpOnly; SameSite=Strict")
                            .parse()
                            .unwrap(),
                    );
                }
                response
            };
            return response;
        }
        return secured_response(
            StatusCode::NOT_FOUND,
            "text/plain; charset=utf-8",
            b"Share not found".to_vec(),
            None,
        );
    }

    if let Some(id) = path.strip_prefix("/api/shares/") {
        if let Some(share) = active_share(&state.shares, id) {
            let body = serde_json::to_vec(&PublicShareResponse {
                id: id.to_string(),
                updated_at: share.updated_at,
                expires_at: share.expires_at.map(|value| value.to_rfc3339()),
                snapshot: share.snapshot,
            })
            .unwrap_or_else(|_| br#"{"error":"Could not encode share"}"#.to_vec());
            return secured_response(
                StatusCode::OK,
                "application/json; charset=utf-8",
                body,
                None,
            );
        }
        return secured_response(
            StatusCode::NOT_FOUND,
            "application/json; charset=utf-8",
            br#"{"error":"Share not found or expired"}"#.to_vec(),
            None,
        );
    }

    let asset_path = path.trim_start_matches('/');
    if !is_public_viewer_asset(asset_path) {
        return secured_response(
            StatusCode::NOT_FOUND,
            "text/plain; charset=utf-8",
            b"Not found".to_vec(),
            None,
        );
    }
    let target = request
        .uri()
        .path_and_query()
        .map_or(asset_path, |value| value.as_str().trim_start_matches('/'));
    viewer_asset_response(&state.assets, target).await
}

async fn viewer_page_response(assets: &ViewerAssets, _request_target: &str) -> Response {
    let target = match assets {
        ViewerAssets::Embedded(_) => "index.html",
        #[cfg(dev)]
        ViewerAssets::Vite(_) => _request_target,
    };
    viewer_asset_response(assets, target).await
}

async fn viewer_asset_response(assets: &ViewerAssets, target: &str) -> Response {
    let asset = match assets {
        ViewerAssets::Embedded(loader) => loader(target.split('?').next().unwrap_or(target)),
        #[cfg(dev)]
        ViewerAssets::Vite(client) => {
            let vite_target = if target == "index.html" { "" } else { target };
            if !vite_target.is_empty() && !development_target_allowed(vite_target) {
                None
            } else {
                let url = format!("{}/{vite_target}", development::vite_origin("http"));
                match client.get(url).send().await {
                    Ok(response) if response.status().is_success() => {
                        let mime_type = response
                            .headers()
                            .get(header::CONTENT_TYPE)
                            .and_then(|value| value.to_str().ok())
                            .unwrap_or("application/octet-stream")
                            .to_string();
                        response.bytes().await.ok().map(|bytes| ViewerAsset {
                            bytes: bytes.to_vec(),
                            mime_type,
                        })
                    }
                    _ => None,
                }
            }
        }
    };

    if let Some(asset) = asset {
        let browser_csp = content_security_policy(&asset.mime_type, &asset.bytes);
        secured_response(
            StatusCode::OK,
            &asset.mime_type,
            asset.bytes,
            Some(&browser_csp),
        )
    } else {
        secured_response(
            StatusCode::NOT_FOUND,
            "text/plain; charset=utf-8",
            b"Not found".to_vec(),
            None,
        )
    }
}

fn secured_response(
    status: StatusCode,
    content_type: &str,
    body: Vec<u8>,
    asset_csp: Option<&str>,
) -> Response {
    let csp = asset_csp.unwrap_or(
        "default-src 'self'; connect-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'",
    );
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::CONTENT_SECURITY_POLICY, csp)
        .header("Cross-Origin-Resource-Policy", "same-origin")
        .header(
            "Permissions-Policy",
            "camera=(), microphone=(), geolocation=(), payment=(), usb=()",
        )
        .header(header::REFERRER_POLICY, "no-referrer")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .body(Body::from(body))
        .expect("static LAN viewer response headers must be valid")
}

fn content_security_policy(mime_type: &str, body: &[u8]) -> String {
    let hashes = if mime_type.starts_with("text/html") {
        inline_script_hashes(body)
    } else {
        Vec::new()
    };
    let script_sources = if hashes.is_empty() {
        "'self'".to_string()
    } else {
        format!("'self' {}", hashes.join(" "))
    };
    format!(
        "default-src 'self'; connect-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src {script_sources}; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'"
    )
}

fn inline_script_hashes(body: &[u8]) -> Vec<String> {
    let Ok(html) = std::str::from_utf8(body) else {
        return Vec::new();
    };
    let mut hashes = Vec::new();
    let mut cursor = 0;
    while let Some(relative_start) = html[cursor..].find("<script") {
        let tag_start = cursor + relative_start;
        let Some(relative_tag_end) = html[tag_start..].find('>') else {
            break;
        };
        let tag_end = tag_start + relative_tag_end;
        let opening_tag = &html[tag_start..=tag_end];
        let content_start = tag_end + 1;
        let Some(relative_script_end) = html[content_start..].find("</script>") else {
            break;
        };
        let script_end = content_start + relative_script_end;
        if !opening_tag.contains("src=") {
            let digest = Sha256::digest(&html.as_bytes()[content_start..script_end]);
            hashes.push(format!("'sha256-{}'", BASE64.encode(digest)));
        }
        cursor = script_end + "</script>".len();
    }
    hashes
}

fn active_share(shares: &Arc<RwLock<HashMap<String, LanShare>>>, id: &str) -> Option<LanShare> {
    let mut guard = shares.write().ok()?;
    let share = guard.get(id)?;
    if share
        .expires_at
        .is_some_and(|expires_at| expires_at <= Utc::now())
    {
        guard.remove(id);
        return None;
    }
    Some(share.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;

    fn test_asset_loader() -> AssetLoader {
        Arc::new(|path| {
            let (mime_type, body) = match path {
                "index.html" => (
                    "text/html; charset=utf-8",
                    "<main id=\"svelte\">Cardbe<script>window.__cardbe = true;</script></main>",
                ),
                "_app/app.css" => ("text/css; charset=utf-8", ".cardbe { display: block; }"),
                "_app/app.js" => ("text/javascript; charset=utf-8", "console.log('Cardbe')"),
                _ => return None,
            };
            Some(ViewerAsset {
                bytes: body.as_bytes().to_vec(),
                mime_type: mime_type.to_string(),
            })
        })
    }

    fn request_with_method(
        shares: &Arc<RwLock<HashMap<String, LanShare>>>,
        asset_loader: &AssetLoader,
        method: &str,
        path: &str,
    ) -> String {
        let state = LanHttpState {
            shares: Arc::clone(shares),
            assets: ViewerAssets::Embedded(Arc::clone(asset_loader)),
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let response = lan_router(state)
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = response.status();
            let headers = response.headers().clone();
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let mut output = format!(
                "HTTP/1.1 {} {}\r\n",
                status.as_u16(),
                status.canonical_reason().unwrap_or("Unknown")
            );
            for (name, value) in headers {
                if let Some(name) = name {
                    output.push_str(name.as_str());
                    output.push_str(": ");
                    output.push_str(value.to_str().unwrap());
                    output.push_str("\r\n");
                }
            }
            output.push_str("\r\n");
            output.push_str(&String::from_utf8_lossy(&body));
            output
        })
    }

    fn request(
        shares: &Arc<RwLock<HashMap<String, LanShare>>>,
        asset_loader: &AssetLoader,
        path: &str,
    ) -> String {
        request_with_method(shares, asset_loader, "GET", path)
    }

    #[test]
    fn accepts_only_unguessable_url_safe_ids() {
        assert!(valid_share_id("j3V_BXrcsGwtsXv6XAD1jA"));
        assert!(!valid_share_id("short"));
        assert!(!valid_share_id("../../not-a-share-id!!"));
    }

    #[test]
    fn accepts_only_private_or_link_local_lan_addresses() {
        assert!(is_private_lan_address("192.168.1.20".parse().unwrap()));
        assert!(is_private_lan_address("10.0.0.20".parse().unwrap()));
        assert!(is_private_lan_address("172.16.0.20".parse().unwrap()));
        assert!(is_private_lan_address("169.254.10.20".parse().unwrap()));
        assert!(is_private_lan_address("fd00::20".parse().unwrap()));
        assert!(is_private_lan_address("fe80::20".parse().unwrap()));
        assert!(!is_private_lan_address("8.8.8.8".parse().unwrap()));
        assert!(!is_private_lan_address(
            "2001:4860:4860::8888".parse().unwrap()
        ));
    }

    #[test]
    fn viewer_asset_allowlist_rejects_paths_outside_the_frontend() {
        assert!(is_public_viewer_asset("_app/immutable/entry/app.js"));
        assert!(!is_public_viewer_asset("_app/../index.html"));
        assert!(!is_public_viewer_asset("secret.txt"));

        #[cfg(dev)]
        {
            assert!(development_target_allowed("favicon.png"));
            assert!(development_target_allowed("favicon.png?v=1"));
            assert!(!development_target_allowed("_app/%2e%2e/secret.txt"));
            assert!(is_public_viewer_asset("@vite/client"));
            assert!(is_public_viewer_asset(
                "@id/__x00__virtual:__sveltekit/environment"
            ));
            assert!(is_public_viewer_asset(
                ".svelte-kit/generated/client/nodes/3.js"
            ));
            assert!(development_target_allowed("share/j3V_BXrcsGwtsXv6XAD1jA"));
            assert!(!development_target_allowed("share/short"));
            assert!(is_public_viewer_asset("src/routes/share/[id]/+page.svelte"));
            assert!(is_public_viewer_asset("node_modules/.vite/deps/svelte.js"));

            let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
            let app_css = workspace
                .join("src/app.css")
                .to_string_lossy()
                .replace('\\', "/");
            let cargo_manifest = workspace
                .join("src-tauri/Cargo.toml")
                .to_string_lossy()
                .replace('\\', "/");
            assert!(development_target_allowed(&format!("@fs/{app_css}")));
            assert!(!development_target_allowed(&format!(
                "@fs/{cargo_manifest}"
            )));
            let mut headers = axum::http::HeaderMap::new();
            headers.insert(header::UPGRADE, HeaderValue::from_static("websocket"));
            headers.insert(
                header::SEC_WEBSOCKET_PROTOCOL,
                HeaderValue::from_static("vite-hmr"),
            );
            assert_eq!(
                vite_websocket_protocol(&headers).as_deref(),
                Some("vite-hmr")
            );
            headers.insert(
                header::SEC_WEBSOCKET_PROTOCOL,
                HeaderValue::from_static("vite-ping"),
            );
            assert_eq!(
                vite_websocket_protocol(&headers).as_deref(),
                Some("vite-ping")
            );
            headers.remove(header::UPGRADE);
            assert!(vite_websocket_protocol(&headers).is_none());
        }
    }

    #[test]
    fn serves_active_memory_snapshot_and_rejects_unknown_ids() {
        let id = "j3V_BXrcsGwtsXv6XAD1jA";
        let shares = Arc::new(RwLock::new(HashMap::from([(
            id.to_string(),
            LanShare {
                snapshot: serde_json::json!({
                    "schema_version": 1,
                    "title": "LAN board",
                    "columns": [{"id": 1, "name": "Todo", "color": "", "tasks": []}]
                }),
                updated_at: Utc::now().to_rfc3339(),
                expires_at: None,
            },
        )])));
        let asset_loader = test_asset_loader();

        let page = request(&shares, &asset_loader, &format!("/share/{id}"));
        assert!(page.starts_with("HTTP/1.1 200 OK"), "{page:?}");
        assert!(page.contains("content-security-policy: default-src 'self'"));
        assert!(page.contains("script-src 'self' 'sha256-"));
        assert!(!page.contains("script-src 'unsafe-inline'"));
        assert!(page.contains("form-action 'none'"));
        assert!(page.contains("cross-origin-resource-policy: same-origin"));
        assert!(!page.contains("connect-src 'self' ipc:"));

        let css = request(&shares, &asset_loader, "/_app/app.css");
        assert!(css.starts_with("HTTP/1.1 200 OK"), "{css:?}");
        assert!(css.contains("content-type: text/css; charset=utf-8"));
        assert!(css.contains(".cardbe"));

        let javascript = request(&shares, &asset_loader, "/_app/app.js");
        assert!(javascript.starts_with("HTTP/1.1 200 OK"));
        assert!(javascript.contains("content-type: text/javascript; charset=utf-8"));
        assert!(javascript.contains("Cardbe"));

        let cache_busted_javascript = request(&shares, &asset_loader, "/_app/app.js?v=1");
        assert!(cache_busted_javascript.starts_with("HTTP/1.1 200 OK"));

        let api = request(&shares, &asset_loader, &format!("/api/shares/{id}"));
        assert!(api.starts_with("HTTP/1.1 200 OK"));
        assert!(api.contains("LAN board"));

        let missing = request(&shares, &asset_loader, "/share/AAAAAAAAAAAAAAAAAAAAAA");
        assert!(missing.starts_with("HTTP/1.1 404 Not Found"));
    }

    #[test]
    fn disabled_links_cannot_be_resurrected_by_delayed_updates() {
        let state = LanShareState::default();
        let id = "j3V_BXrcsGwtsXv6XAD1jA".to_string();
        let new_id = "k4W_ACyrtHxtuYw7YBE2kB".to_string();
        let share = LanShare {
            snapshot: serde_json::json!({"title": "Private content"}),
            updated_at: Utc::now().to_rfc3339(),
            expires_at: None,
        };
        state
            .replace_share(id.clone(), share.clone(), false)
            .unwrap();
        state.revoke(id.clone()).unwrap();
        assert!(state
            .replace_share(id.clone(), share.clone(), false)
            .is_err());
        assert!(active_share(&state.shares, &id).is_none());
        state
            .replace_share(new_id.clone(), share.clone(), false)
            .unwrap();
        // An older request must not disable or replace the newer share.
        state.revoke(id.clone()).unwrap();
        assert!(state.replace_share(id, share, false).is_err());
        assert!(active_share(&state.shares, &new_id).is_some());
    }

    #[test]
    fn explicitly_reusing_a_link_reactivates_its_retired_id() {
        let state = LanShareState::default();
        let id = "j3V_BXrcsGwtsXv6XAD1jA".to_string();
        let share = LanShare {
            snapshot: serde_json::json!({"title": "Restored content"}),
            updated_at: Utc::now().to_rfc3339(),
            expires_at: None,
        };
        state
            .replace_share(id.clone(), share.clone(), false)
            .unwrap();
        state.revoke(id.clone()).unwrap();

        state.replace_share(id.clone(), share, true).unwrap();

        assert!(active_share(&state.shares, &id).is_some());
        assert!(!state.retired_ids.lock().unwrap().contains(&id));
    }

    #[test]
    fn explicitly_reusing_an_active_link_replaces_its_snapshot() {
        let state = LanShareState::default();
        let id = "j3V_BXrcsGwtsXv6XAD1jA".to_string();
        let original = LanShare {
            snapshot: serde_json::json!({"title": "Original"}),
            updated_at: Utc::now().to_rfc3339(),
            expires_at: None,
        };
        let replacement = LanShare {
            snapshot: serde_json::json!({"title": "Replacement"}),
            updated_at: Utc::now().to_rfc3339(),
            expires_at: None,
        };
        state.replace_share(id.clone(), original, false).unwrap();

        state
            .replace_share(id.clone(), replacement, true)
            .unwrap();

        let shares = state.shares.read().unwrap();
        assert_eq!(shares.len(), 1);
        assert_eq!(shares.get(&id).unwrap().snapshot["title"], "Replacement");
    }

    #[test]
    fn explicitly_reusing_an_unknown_link_creates_that_requested_id() {
        let state = LanShareState::default();
        let id = "j3V_BXrcsGwtsXv6XAD1jA".to_string();
        let share = LanShare {
            snapshot: serde_json::json!({"title": "Recovered elsewhere"}),
            updated_at: Utc::now().to_rfc3339(),
            expires_at: None,
        };

        state.replace_share(id.clone(), share, true).unwrap();

        assert!(active_share(&state.shares, &id).is_some());
        assert!(!state.retired_ids.lock().unwrap().contains(&id));
    }

    #[test]
    fn revoke_before_publication_retires_only_that_id() {
        let state = LanShareState::default();
        let share = LanShare {
            snapshot: serde_json::json!({}),
            updated_at: Utc::now().to_rfc3339(),
            expires_at: None,
        };
        state.revoke("pending".into()).unwrap();
        assert!(state
            .replace_share("pending".into(), share.clone(), false)
            .is_err());
        state
            .replace_share("old".into(), share.clone(), false)
            .unwrap();
        state
            .replace_share("new".into(), share.clone(), false)
            .unwrap();
        assert!(state.replace_share("old".into(), share, false).is_ok());
    }

    #[test]
    fn expired_snapshots_are_not_served_and_are_removed() {
        let state = LanShareState::default();
        let id = "j3V_BXrcsGwtsXv6XAD1jA";
        state
            .replace_share(
                id.into(),
                LanShare {
                    snapshot: serde_json::json!({"title": "Expired secret"}),
                    updated_at: Utc::now().to_rfc3339(),
                    expires_at: Some(Utc::now() - chrono::Duration::seconds(1)),
                },
                false,
            )
            .unwrap();
        let response = request(
            &state.shares,
            &test_asset_loader(),
            &format!("/api/shares/{id}"),
        );
        assert!(response.starts_with("HTTP/1.1 404"));
        assert!(!response.contains("Expired secret"));
        assert!(state.shares.read().unwrap().is_empty());
    }

    #[test]
    fn publishing_another_share_keeps_existing_ids_active() {
        let state = LanShareState::default();
        let old_id = "j3V_BXrcsGwtsXv6XAD1jA".to_string();
        let new_id = "k4W_ACyrtHxtuYw7YBE2kB".to_string();
        let share = |title: &str| LanShare {
            snapshot: serde_json::json!({
                "schema_version": 1,
                "title": title,
                "columns": [{"id": 1, "name": "Todo", "color": "", "tasks": []}]
            }),
            updated_at: Utc::now().to_rfc3339(),
            expires_at: None,
        };

        state
            .replace_share(old_id.clone(), share("Old"), false)
            .unwrap();
        state
            .replace_share(new_id.clone(), share("New"), false)
            .unwrap();

        let shares = state.shares.read().unwrap();
        assert!(shares.contains_key(&old_id));
        assert!(shares.contains_key(&new_id));
        assert_eq!(shares.len(), 2);
    }

    #[test]
    fn lan_server_exposes_only_read_only_share_routes_and_viewer_assets() {
        let id = "j3V_BXrcsGwtsXv6XAD1jA";
        let shares = Arc::new(RwLock::new(HashMap::from([(
            id.to_string(),
            LanShare {
                snapshot: serde_json::json!({
                    "schema_version": 1,
                    "title": "LAN board",
                    "columns": [{"id": 1, "name": "Todo", "color": "", "tasks": []}]
                }),
                updated_at: Utc::now().to_rfc3339(),
                expires_at: None,
            },
        )])));
        let asset_loader = test_asset_loader();

        for method in ["POST", "PUT", "PATCH", "DELETE"] {
            let response =
                request_with_method(&shares, &asset_loader, method, &format!("/api/shares/{id}"));
            assert!(response.starts_with("HTTP/1.1 405 Method Not Allowed"));
        }

        for path in [
            "/",
            "/index.html",
            "/api/board",
            "/api/tasks",
            "/secret.txt",
            "/_app/../index.html",
        ] {
            let response = request(&shares, &asset_loader, path);
            assert!(
                response.starts_with("HTTP/1.1 404 Not Found"),
                "{path}: {response:?}"
            );
        }
    }
}
