use super::safety::{public_addresses, safe_url};
use reqwest::{header, redirect::Policy, Client, Url};
use std::{future::Future, net::SocketAddr, time::Duration};

pub const TIMEOUT: Duration = Duration::from_secs(10);
const MAX_REDIRECTS: usize = 5;
pub const HTML_LIMIT: usize = 1024 * 1024;
pub const IMAGE_LIMIT: usize = 512 * 1024;

fn client(host: &str, addresses: &[SocketAddr]) -> Option<Client> {
    Client::builder()
        .no_proxy()
        .redirect(Policy::none())
        .timeout(TIMEOUT)
        .user_agent("Cardbe-LinkPreview/1.0")
        .resolve_to_addrs(host, addresses)
        .build()
        .ok()
}

// Every hop resolves once, validates every answer, and pins those addresses.
// Disabling proxies prevents a proxy from bypassing the pinned DNS results.
pub async fn fetch(url: Url, image: bool) -> Option<(Url, String, Vec<u8>)> {
    fetch_with(
        url,
        image,
        |host, port| async move {
            tokio::net::lookup_host((host.as_str(), port))
                .await
                .ok()
                .map(|addresses| addresses.collect())
        },
        client,
    )
    .await
}

async fn fetch_with<F: Future<Output = Option<Vec<SocketAddr>>>>(
    mut url: Url,
    image: bool,
    mut resolve: impl FnMut(String, u16) -> F,
    mut connect: impl FnMut(&str, &[SocketAddr]) -> Option<Client>,
) -> Option<(Url, String, Vec<u8>)> {
    for hop in 0..=MAX_REDIRECTS {
        url = safe_url(url.as_str())?;
        let host = url.host_str()?.trim_matches(['[', ']']);
        let port = url.port_or_known_default()?;
        let addresses = resolve(host.to_owned(), port).await?;
        if !public_addresses(&addresses) {
            return None;
        }
        let client = connect(host, &addresses)?;
        let mut response = client.get(url.clone()).send().await.ok()?;
        if response.status().is_redirection() {
            if hop == MAX_REDIRECTS {
                return None;
            }
            url = url
                .join(response.headers().get(header::LOCATION)?.to_str().ok()?)
                .ok()?;
            continue;
        }
        if !response.status().is_success() {
            return None;
        }
        let mime = response
            .headers()
            .get(header::CONTENT_TYPE)?
            .to_str()
            .ok()?
            .split(';')
            .next()?
            .trim()
            .to_ascii_lowercase();
        let allowed = if image {
            matches!(
                mime.as_str(),
                "image/png"
                    | "image/jpeg"
                    | "image/gif"
                    | "image/webp"
                    | "image/x-icon"
                    | "image/vnd.microsoft.icon"
            )
        } else {
            matches!(mime.as_str(), "text/html" | "application/xhtml+xml")
        };
        let limit = if image { IMAGE_LIMIT } else { HTML_LIMIT };
        if !allowed
            || response
                .content_length()
                .is_some_and(|length| length > limit as u64)
        {
            return None;
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.ok()? {
            if body.len() + chunk.len() > limit {
                return None;
            }
            body.extend_from_slice(&chunk);
        }
        return Some((url, mime, body));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        response::{Html, Redirect},
        routing::get,
        Router,
    };
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[tokio::test]
    async fn redirects_revalidate_urls_and_dns_before_sending_another_request() {
        for target_host in [
            "public.example",
            "private.example",
            "mixed.example",
            "127.0.0.1",
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let local = listener.local_addr().unwrap();
            let location = format!("http://{target_host}:{}/target", local.port());
            let target_requests = Arc::new(AtomicUsize::new(0));
            let requests = target_requests.clone();
            let router = Router::new()
                .route(
                    "/start",
                    get(move || {
                        let location = location.clone();
                        async move { Redirect::temporary(&location) }
                    }),
                )
                .route(
                    "/target",
                    get(move || {
                        requests.fetch_add(1, Ordering::SeqCst);
                        async { Html("<title>Public target</title>") }
                    }),
                );
            let server = tokio::spawn(async move { axum::serve(listener, router).await });
            let public = SocketAddr::from(([93, 184, 216, 34], local.port()));
            let mut resolved = Vec::new();
            let mut connected = Vec::new();
            // Keep HTTP real, but replace DNS and route allowed connections to the fixture.
            let result = fetch_with(
                Url::parse(&format!("http://public.example:{}/start", local.port())).unwrap(),
                false,
                |host, port| {
                    assert_eq!(port, local.port());
                    let addresses = match host.as_str() {
                        "public.example" => vec![public],
                        "private.example" => vec![local],
                        "mixed.example" => vec![public, local],
                        _ => panic!("Unexpected DNS lookup: {host}"),
                    };
                    resolved.push(host);
                    std::future::ready(Some(addresses))
                },
                |host, addresses| {
                    assert_eq!(addresses, &[public]);
                    connected.push(host.to_owned());
                    client(host, &[local])
                },
            )
            .await;
            server.abort();
            let allowed = target_host == "public.example";
            assert_eq!(result.is_some(), allowed, "{target_host}");
            assert_eq!(target_requests.load(Ordering::SeqCst), usize::from(allowed));
            assert_eq!(connected.len(), if allowed { 2 } else { 1 });
            assert_eq!(
                resolved,
                if target_host == "127.0.0.1" {
                    vec!["public.example"]
                } else {
                    vec!["public.example", target_host]
                }
            );
            if let Some((url, mime, body)) = result {
                assert_eq!(url.host_str(), Some(target_host));
                assert_eq!(url.path(), "/target");
                assert_eq!(mime, "text/html");
                assert_eq!(body, b"<title>Public target</title>");
            }
        }
    }
}
