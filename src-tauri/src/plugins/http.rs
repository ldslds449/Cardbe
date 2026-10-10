use super::runtime::RuntimeError;
use crate::link_preview::safety::{public_addresses, safe_url};
use reqwest::{redirect::Policy, Client, Response, Url};
use std::{
    future::Future,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Default)]
pub struct HttpPolicy {
    pub allowed_domains: Vec<String>,
    pub token: Option<String>,
    pub error_codes: Vec<String>,
    pub log_codes: Vec<String>,
}
pub(crate) fn authorized_url(value: &str, policy: &HttpPolicy) -> Result<Url, RuntimeError> {
    let url = safe_url(value).ok_or(RuntimeError::PermissionDenied)?;
    if url.scheme() != "https"
        || url.port_or_known_default() != Some(443)
        || !policy
            .allowed_domains
            .iter()
            .any(|domain| Some(domain.as_str()) == url.host_str())
    {
        return Err(RuntimeError::PermissionDenied);
    }
    Ok(url)
}
pub(crate) async fn get(
    value: &str,
    policy: &HttpPolicy,
    cancel: Arc<AtomicBool>,
    deadline: Instant,
) -> Result<super::runtime::bindings::cardbe::plugin::types::HttpResponse, RuntimeError> {
    get_with(
        value,
        policy,
        cancel,
        deadline,
        |host| async move {
            Ok(tokio::net::lookup_host((host.as_str(), 443))
                .await
                .map_err(|_| RuntimeError::Network)?
                .collect())
        },
        |url, addresses| async move {
            let host = url.host_str().ok_or(RuntimeError::PermissionDenied)?;
            // Resolve once, reject every private answer, pin DNS, and bypass proxies.
            let client = Client::builder()
                .no_proxy()
                .redirect(Policy::none())
                .timeout(Duration::from_secs(15))
                .user_agent("Cardbe-PluginHost/1.0")
                .resolve_to_addrs(host, &addresses)
                .build()
                .map_err(|_| RuntimeError::Network)?;
            let mut request = client.get(url.clone()).header("Accept", "application/json");
            // shortcut: one bearer secret for one approved origin, add named scopes before supporting multiple credentialed origins.
            if policy.allowed_domains.len() == 1 {
                if let Some(token) = &policy.token {
                    request = request.bearer_auth(token);
                }
            }
            request.send().await.map_err(|_| RuntimeError::Network)
        },
    )
    .await
}
async fn get_with<
    R: Future<Output = Result<Vec<SocketAddr>, RuntimeError>>,
    F: Future<Output = Result<Response, RuntimeError>>,
>(
    value: &str,
    policy: &HttpPolicy,
    cancel: Arc<AtomicBool>,
    deadline: Instant,
    resolve: impl FnOnce(String) -> R,
    request: impl FnOnce(Url, Vec<SocketAddr>) -> F,
) -> Result<super::runtime::bindings::cardbe::plugin::types::HttpResponse, RuntimeError> {
    let url = authorized_url(value, policy)?;
    let operation = async {
        let host = url
            .host_str()
            .ok_or(RuntimeError::PermissionDenied)?
            .to_owned();
        let addresses = resolve(host).await?;
        if !public_addresses(&addresses) {
            return Err(RuntimeError::PermissionDenied);
        }
        let mut response = request(url, addresses).await?;
        if response.status().is_redirection() {
            return Err(RuntimeError::PermissionDenied);
        }
        let headers = [
            "link",
            "retry-after",
            "x-ratelimit-remaining",
            "x-ratelimit-reset",
            "content-type",
        ]
        .into_iter()
        .filter_map(|name| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .filter(|value| value.len() <= 8192)
                .map(|value| (name.to_owned(), value.to_owned()))
        })
        .collect();
        let status = response.status().as_u16();
        const LIMIT: usize = 2 * 1024 * 1024;
        if response
            .content_length()
            .is_some_and(|length| length > LIMIT as u64)
        {
            return Err(RuntimeError::ResourceLimit);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| RuntimeError::Network)? {
            if body.len() + chunk.len() > LIMIT {
                return Err(RuntimeError::ResourceLimit);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(
            super::runtime::bindings::cardbe::plugin::types::HttpResponse {
                status,
                headers,
                body,
            },
        )
    };
    tokio::pin!(operation);
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err(RuntimeError::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(RuntimeError::Timeout);
        }
        tokio::select! { result=&mut operation=>return result, _=tokio::time::sleep(Duration::from_millis(50))=>{} }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> HttpPolicy {
        HttpPolicy {
            allowed_domains: vec!["api.github.com".into()],
            ..Default::default()
        }
    }
    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(3)
    }
    fn cancel() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }
    #[test]
    fn capability_rejects_private_wrong_origin_credentials_and_ports() {
        assert!(authorized_url("https://api.github.com/repos/a/b/issues", &policy()).is_ok());
        for url in [
            "http://api.github.com",
            "https://api.github.com:444",
            "https://evil.example",
            "https://api.github.com.evil.example",
            "https://127.0.0.1",
            "https://user:secret@api.github.com",
        ] {
            assert!(authorized_url(url, &policy()).is_err(), "{url}");
        }
    }
    async fn mock_response(bytes: Vec<u8>) -> Response {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = server.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = server.accept().await.unwrap();
            let mut request = [0u8; 4096];
            let _ = socket.read(&mut request).await;
            let _ = socket.write_all(&bytes).await;
        });
        Client::builder()
            .no_proxy()
            .redirect(Policy::none())
            .build()
            .unwrap()
            .get(format!("http://{address}"))
            .send()
            .await
            .unwrap()
    }
    #[tokio::test]
    async fn mixed_private_dns_answers_never_reach_request_and_public_addresses_are_pinned() {
        let result = get_with(
            "https://api.github.com",
            &policy(),
            cancel(),
            deadline(),
            |_| async {
                Ok(vec![
                    "93.184.216.34:443".parse().unwrap(),
                    "127.0.0.1:443".parse().unwrap(),
                ])
            },
            |_, _| async { panic!("private DNS must not reach client") },
        )
        .await;
        assert!(matches!(result, Err(RuntimeError::PermissionDenied)));
        let result = get_with(
            "https://api.github.com",
            &policy(),
            cancel(),
            deadline(),
            |host| async move {
                assert_eq!(host, "api.github.com");
                Ok(vec!["93.184.216.34:443".parse().unwrap()])
            },
            |url, addresses| async move {
                assert_eq!(url.host_str(), Some("api.github.com"));
                assert_eq!(
                    addresses,
                    vec!["93.184.216.34:443".parse::<SocketAddr>().unwrap()]
                );
                Ok(mock_response(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nSet-Cookie: secret=hidden\r\n\r\n{}"
                        .to_vec(),
                )
                .await)
            },
        )
        .await
        .unwrap();
        assert_eq!(result.body, b"{}");
        assert!(!result.headers.iter().any(|(key, _)| key == "set-cookie"));
    }
    #[tokio::test]
    async fn redirects_declared_and_streamed_size_limits_are_rejected() {
        for bytes in [b"HTTP/1.1 302 Found\r\nContent-Length: 0\r\nLocation: http://127.0.0.1/private\r\n\r\n".to_vec(),b"HTTP/1.1 200 OK\r\nContent-Length: 2097153\r\n\r\n".to_vec(),[b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec(),vec![b'x';2097153]].concat()] {
            let result=get_with("https://api.github.com",&policy(),cancel(),deadline(),|_|async {Ok(vec!["93.184.216.34:443".parse().unwrap()])},|_,_|async {Ok(mock_response(bytes).await)}).await;
            assert!(matches!(result,Err(RuntimeError::PermissionDenied|RuntimeError::ResourceLimit)),"{result:?}");
        }
    }
    #[tokio::test]
    async fn cancellation_and_deadline_interrupt_pending_dns() {
        for cancelled in [true, false] {
            let stop = cancel();
            if cancelled {
                stop.store(true, Ordering::Release);
            }
            let result = get_with(
                "https://api.github.com",
                &policy(),
                stop,
                Instant::now() + Duration::from_millis(10),
                |_| std::future::pending::<Result<Vec<SocketAddr>, RuntimeError>>(),
                |_, _| std::future::pending::<Result<Response, RuntimeError>>(),
            )
            .await;
            if cancelled {
                assert!(matches!(result, Err(RuntimeError::Cancelled)));
            } else {
                assert!(matches!(result, Err(RuntimeError::Timeout)));
            }
        }
    }
}
