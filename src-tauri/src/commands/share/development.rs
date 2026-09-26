use super::{format_host, valid_share_id};
use axum::{
    extract::ws::{Message as AxumMessage, WebSocket},
    http::{header, HeaderValue},
};
use futures_util::{SinkExt, StreamExt};
use std::{path::Path, time::Duration};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message as TungsteniteMessage},
};

pub(super) fn authorized_viewer(
    headers: &axum::http::HeaderMap,
    shares: &std::sync::Arc<std::sync::RwLock<std::collections::HashMap<String, super::LanShare>>>,
) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|cookie| cookie.trim().strip_prefix("cardbe_dev_share="))
        .find(|id| valid_share_id(id) && super::active_share(shares, id).is_some())
        .map(str::to_string)
}

pub(super) fn same_origin(headers: &axum::http::HeaderMap) -> bool {
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|origin| origin == format!("http://{host}"))
}

pub(super) fn share_port() -> Result<u16, String> {
    match std::env::var("CARDBE_SHARE_DEV_PORT") {
        Ok(value) => value
            .parse::<u16>()
            .ok()
            .filter(|port| *port != 0)
            .ok_or_else(|| "CARDBE_SHARE_DEV_PORT must be a port between 1 and 65535".into()),
        Err(std::env::VarError::NotPresent) => Ok(0),
        Err(error) => Err(format!("Invalid CARDBE_SHARE_DEV_PORT: {error}")),
    }
}

pub(super) fn vite_origin(protocol: &str) -> String {
    let host = std::env::var("TAURI_DEV_HOST")
        .ok()
        .filter(|host| !host.is_empty())
        .unwrap_or_else(|| "localhost".into());
    let host = host.parse().map(format_host).unwrap_or(host);
    let port = vite_port();
    format!("{protocol}://{host}:{port}")
}

fn vite_port() -> u16 {
    std::env::var("CARDBE_DEV_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|port| *port != 0)
        .unwrap_or(1420)
}

fn generated_dir() -> String {
    match vite_port() {
        1420 => ".svelte-kit".into(),
        port => format!(".svelte-kit-dev-{port}"),
    }
}

pub(super) fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| format!("Could not initialize Vite proxy: {error}"))
}
pub(super) fn development_target_allowed(target: &str) -> bool {
    let path = target.split('?').next().unwrap_or_default();
    let generated_dir = generated_dir();
    if super::production_asset_allowed(path) {
        return true;
    }
    let safe_share_page = path.strip_prefix("share/").is_some_and(valid_share_id);
    let safe_virtual_module = path.starts_with("@id/")
        && !path.contains('\\')
        && !path.contains('%')
        && !path.contains("..");
    let safe_root_module = !path.contains("..")
        && !path.contains('\\')
        && !path.contains(':')
        && !path.contains('%')
        && (path.starts_with("@vite/")
            || path.starts_with("src/")
            || path.starts_with(&format!("{generated_dir}/generated/"))
            || path.starts_with("node_modules/"));
    if safe_share_page || safe_virtual_module || safe_root_module {
        return true;
    }

    let Some(raw_path) = path.strip_prefix("@fs/") else {
        return false;
    };
    #[cfg(windows)]
    let file_url = format!("file:///{raw_path}");
    #[cfg(not(windows))]
    let file_url = format!("file://{raw_path}");
    let Ok(file_url) = reqwest::Url::parse(&file_url) else {
        return false;
    };
    let Ok(candidate_path) = file_url.to_file_path() else {
        return false;
    };
    let Ok(candidate) = candidate_path.canonicalize() else {
        return false;
    };
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri must have a parent directory");
    ["src", generated_dir.as_str(), "node_modules"]
        .iter()
        .filter_map(|directory| workspace.join(directory).canonicalize().ok())
        .any(|allowed_root| candidate.starts_with(allowed_root))
}

pub(super) fn vite_websocket_protocol(headers: &axum::http::HeaderMap) -> Option<String> {
    let upgrade = headers.get(header::UPGRADE)?.to_str().ok()?;
    if !upgrade.eq_ignore_ascii_case("websocket") {
        return None;
    }
    headers
        .get(header::SEC_WEBSOCKET_PROTOCOL)?
        .to_str()
        .ok()?
        .split(',')
        .map(str::trim)
        .find(|protocol| matches!(*protocol, "vite-hmr" | "vite-ping"))
        .map(str::to_string)
}

pub(super) async fn proxy_vite_websocket(mut browser: WebSocket, target: String, protocol: String) {
    let mut upstream_request = match target.into_client_request() {
        Ok(request) => request,
        Err(error) => {
            log::warn!(target: "share", "Could not create the Vite WebSocket request: {error}");
            return;
        }
    };
    let protocol_header = match HeaderValue::from_str(&protocol) {
        Ok(header) => header,
        Err(error) => {
            log::warn!(target: "share", "Invalid Vite WebSocket protocol header: {error}");
            return;
        }
    };
    upstream_request
        .headers_mut()
        .insert(header::SEC_WEBSOCKET_PROTOCOL, protocol_header);

    let (mut vite, _) =
        match tokio::time::timeout(Duration::from_secs(5), connect_async(upstream_request)).await {
            Ok(Ok(connection)) => connection,
            Ok(Err(error)) => {
                log::warn!(target: "share", "Could not connect to the Vite WebSocket: {error}");
                return;
            }
            Err(error) => {
                log::warn!(target: "share", "Connecting to the Vite WebSocket timed out: {error}");
                return;
            }
        };

    loop {
        tokio::select! {
            browser_message = browser.recv() => {
                let message = match browser_message {
                    Some(Ok(message)) => message,
                    Some(Err(error)) => {
                        log::warn!(target: "share", "Could not read the viewer WebSocket message: {error}");
                        break;
                    }
                    None => break,
                };
                let message = match message {
                    AxumMessage::Text(value) => TungsteniteMessage::Text(value.to_string().into()),
                    AxumMessage::Binary(value) => TungsteniteMessage::Binary(value.to_vec().into()),
                    AxumMessage::Ping(value) => TungsteniteMessage::Ping(value.to_vec().into()),
                    AxumMessage::Pong(value) => TungsteniteMessage::Pong(value.to_vec().into()),
                    AxumMessage::Close(_) => TungsteniteMessage::Close(None),
                };
                match tokio::time::timeout(Duration::from_secs(5), vite.send(message)).await {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        log::warn!(target: "share", "Could not forward a viewer message to Vite: {error}");
                        break;
                    }
                    Err(error) => {
                        log::warn!(target: "share", "Forwarding a viewer message to Vite timed out: {error}");
                        break;
                    }
                }
            }
            vite_message = vite.next() => {
                let message = match vite_message {
                    Some(Ok(message)) => message,
                    Some(Err(error)) => {
                        log::warn!(target: "share", "Could not read the Vite WebSocket message: {error}");
                        break;
                    }
                    None => break,
                };
                let message = match message {
                    TungsteniteMessage::Text(value) => AxumMessage::Text(value.to_string().into()),
                    TungsteniteMessage::Binary(value) => AxumMessage::Binary(value.to_vec().into()),
                    TungsteniteMessage::Ping(value) => AxumMessage::Ping(value.to_vec().into()),
                    TungsteniteMessage::Pong(value) => AxumMessage::Pong(value.to_vec().into()),
                    TungsteniteMessage::Close(_) => AxumMessage::Close(None),
                    TungsteniteMessage::Frame(_) => continue,
                };
                match tokio::time::timeout(Duration::from_secs(5), browser.send(message)).await {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        log::warn!(target: "share", "Could not forward a Vite message to the viewer: {error}");
                        break;
                    }
                    Err(error) => {
                        log::warn!(target: "share", "Forwarding a Vite message to the viewer timed out: {error}");
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::share::{
        lan_router, transport::BoundedListener, LanHttpState, LanShare, ViewerAssets,
    };
    use std::{
        collections::HashMap,
        sync::{Arc, RwLock},
    };

    #[tokio::test]
    async fn development_routes_require_an_active_share() {
        use axum::{body::Body, extract::Request, http::StatusCode};
        use tower::ServiceExt;
        let shares = Arc::new(RwLock::new(HashMap::new()));
        let router = lan_router(LanHttpState {
            shares: Arc::clone(&shares),
            assets: ViewerAssets::Vite(client().unwrap()),
        });
        for path in [
            "/@vite/client",
            "/src/app.css",
            "/@fs/secret",
            "/?token=known-vite-token",
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .header(header::COOKIE, "cardbe_dev_share=j3V_BXrcsGwtsXv6XAD1jA")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path}");
        }
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(header::HOST, "192.168.1.20:1422".parse().unwrap());
        headers.insert(header::ORIGIN, "https://attacker.example".parse().unwrap());
        assert!(!same_origin(&headers));
        headers.insert(header::ORIGIN, "http://192.168.1.20:1422".parse().unwrap());
        assert!(same_origin(&headers));
    }

    // Run with `pnpm dev` already running. Uses a temporary self-accepting module
    // to exercise real Vite transforms, the HMR token, and a file-change update.
    #[tokio::test]
    #[ignore = "requires the Vite development server on port 1420"]
    async fn live_vite_assets_and_hmr_update_through_share_proxy() {
        struct Fixture(std::path::PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let module_name = format!("__cardbe_hmr_smoke_{}.js", std::process::id());
        let module_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("src")
            .join(&module_name);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&module_path)
            .unwrap();
        let fixture = Fixture(module_path);
        use std::io::Write;
        file.write_all(b"export const value = 1; if (import.meta.hot) import.meta.hot.accept();")
            .unwrap();
        drop(file);

        let id = "j3V_BXrcsGwtsXv6XAD1jA";
        let shares = Arc::new(RwLock::new(HashMap::from([(
            id.into(),
            LanShare {
                snapshot: serde_json::json!({"schema_version": 1}),
                updated_at: "2026-09-05T00:00:00Z".into(),
                expires_at: None,
            },
        )])));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = lan_router(LanHttpState {
            shares: Arc::clone(&shares),
            assets: ViewerAssets::Vite(client().unwrap()),
        });
        let serving = tokio::spawn(async move {
            axum::serve(BoundedListener::new(listener), router)
                .await
                .unwrap();
        });
        let http = client().unwrap();
        let origin = format!("http://{address}");
        assert_eq!(
            http.get(format!("{origin}/@vite/client"))
                .send()
                .await
                .unwrap()
                .status(),
            reqwest::StatusCode::FORBIDDEN
        );
        let page = http
            .get(format!("{origin}/share/{id}"))
            .send()
            .await
            .unwrap();
        assert_eq!(page.status(), reqwest::StatusCode::OK);
        let cookie = page
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(header::COOKIE, cookie.parse().unwrap());
        let http = reqwest::Client::builder()
            .no_proxy()
            .default_headers(headers)
            .build()
            .unwrap();
        for path in [
            format!("share/{id}"),
            format!("api/shares/{id}"),
            "favicon.png?v=1".into(),
        ] {
            let response = http.get(format!("{origin}/{path}")).send().await.unwrap();
            assert_eq!(response.status(), reqwest::StatusCode::OK, "{path}");
        }
        let vite_client = http
            .get(format!("{origin}/@vite/client"))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .text()
            .await
            .unwrap();
        let token_literal = vite_client
            .split_once("const wsToken = ")
            .unwrap()
            .1
            .split(';')
            .next()
            .unwrap();
        assert!(
            vite_client.contains("const hmrPort = null;"),
            "HMR must inherit the share port"
        );
        assert!(
            vite_client.contains("${null || importMetaUrl.hostname}"),
            "HMR must inherit the share host"
        );
        let token: String = serde_json::from_str(token_literal.trim()).unwrap();
        let mut request = format!("ws://{address}/?token={token}")
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            header::SEC_WEBSOCKET_PROTOCOL,
            HeaderValue::from_static("vite-hmr"),
        );
        request
            .headers_mut()
            .insert(header::COOKIE, cookie.parse().unwrap());
        request
            .headers_mut()
            .insert(header::ORIGIN, origin.parse().unwrap());
        let (mut socket, _) = connect_async(request).await.unwrap();
        let connected = tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(connected.into_text().unwrap().contains("connected"));
        let transformed = http
            .get(format!("{origin}/src/{module_name}"))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(transformed.contains("createHotContext"));
        std::fs::write(
            &fixture.0,
            "export const value = 2; if (import.meta.hot) import.meta.hot.accept();",
        )
        .unwrap();
        let update = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let message = socket.next().await.unwrap().unwrap();
                if let TungsteniteMessage::Text(text) = message {
                    let event: serde_json::Value = serde_json::from_str(&text).unwrap();
                    if event["type"] == "update" {
                        break event;
                    }
                }
            }
        })
        .await
        .expect("Vite should deliver a file-change update through the share proxy");
        assert!(update["updates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["path"] == format!("/src/{module_name}")));
        shares.write().unwrap().clear();
        assert_eq!(
            http.get(format!("{origin}/@vite/client"))
                .send()
                .await
                .unwrap()
                .status(),
            reqwest::StatusCode::FORBIDDEN
        );
        tokio::time::timeout(Duration::from_secs(3), async {
            while let Some(Ok(message)) = socket.next().await {
                if matches!(message, TungsteniteMessage::Close(_)) {
                    break;
                }
            }
        })
        .await
        .expect("revoking the share must disconnect HMR");
        serving.abort();
    }
}
