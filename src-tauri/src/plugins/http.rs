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

pub(crate) fn canonical_domain(domain: &str) -> &str {
    domain.strip_suffix(":443").unwrap_or(domain)
}
pub(crate) fn contains_domain(domains: &[String], domain: &str) -> bool {
    domains
        .iter()
        .any(|value| canonical_domain(value) == canonical_domain(domain))
}
fn url_domain(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    }
}
#[derive(Clone, Default)]
pub struct HttpPolicy {
    pub allowed_domains: Vec<String>,
    pub requested_domains: Vec<String>,
    pub allow_custom_domains: bool,
    pub token: Option<String>,
    pub token_domain: Option<String>,
    pub error_codes: Vec<String>,
    pub log_codes: Vec<String>,
}
impl HttpPolicy {
    fn bearer_token(&self, url: &Url) -> Option<&str> {
        if self.allowed_domains.len() == 1
            && contains_domain(&self.allowed_domains, &url_domain(url))
            && self
                .token_domain
                .as_deref()
                .is_some_and(|domain| canonical_domain(domain) == url_domain(url))
        {
            self.token.as_deref()
        } else {
            None
        }
    }
}
#[cfg(test)]
pub(crate) fn authorized_url(value: &str, policy: &HttpPolicy) -> Result<Url, RuntimeError> {
    let url = request_url(value)?;
    if !contains_domain(&policy.allowed_domains, &url_domain(&url)) {
        return Err(RuntimeError::PermissionDenied);
    }
    Ok(url)
}
fn request_url(value: &str) -> Result<Url, RuntimeError> {
    let url = safe_url(value).ok_or(RuntimeError::PermissionDenied)?;
    if url.scheme() != "https" || url.port_or_known_default().is_none_or(|port| port == 0) {
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
    send(value, None, policy, cancel, deadline).await
}
pub(crate) fn validate_post(content_type: &str, body: &[u8]) -> Result<(), RuntimeError> {
    if body.len() > 10 * 1024 * 1024 {
        return Err(RuntimeError::ResourceLimit);
    }
    if content_type.is_empty()
        || content_type.len() > 256
        || reqwest::header::HeaderValue::from_str(content_type).is_err()
    {
        return Err(RuntimeError::PermissionDenied);
    }
    Ok(())
}
pub(crate) async fn post(
    value: &str,
    content_type: &str,
    body: &[u8],
    policy: &HttpPolicy,
    cancel: Arc<AtomicBool>,
    deadline: Instant,
) -> Result<super::runtime::bindings::cardbe::plugin::types::HttpResponse, RuntimeError> {
    validate_post(content_type, body)?;
    send(value, Some((content_type, body)), policy, cancel, deadline).await
}
fn build_request(
    client: &Client,
    url: Url,
    post: Option<(&str, &[u8])>,
    policy: &HttpPolicy,
) -> reqwest::RequestBuilder {
    let mut request = match post {
        Some((content_type, body)) => client
            .post(url.clone())
            .header("Content-Type", content_type)
            .body(body.to_vec()),
        None => client.get(url.clone()),
    }
    .header("Accept", "application/json");
    // shortcut: bearer auth is limited to the single manifest origin, add named scopes for custom or multiple credentialed origins.
    if let Some(token) = policy.bearer_token(&url) {
        request = request.bearer_auth(token);
    }
    request
}
async fn send(
    value: &str,
    post: Option<(&str, &[u8])>,
    policy: &HttpPolicy,
    cancel: Arc<AtomicBool>,
    deadline: Instant,
) -> Result<super::runtime::bindings::cardbe::plugin::types::HttpResponse, RuntimeError> {
    get_with(
        value,
        policy,
        cancel,
        deadline,
        |(host, port)| async move {
            Ok(tokio::net::lookup_host((host.as_str(), port))
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
            build_request(&client, url, post, policy)
                .send()
                .await
                .map_err(|_| RuntimeError::Network)
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
    resolve: impl FnOnce((String, u16)) -> R,
    request: impl FnOnce(Url, Vec<SocketAddr>) -> F,
) -> Result<super::runtime::bindings::cardbe::plugin::types::HttpResponse, RuntimeError> {
    let url = request_url(value)?;
    let operation = async {
        let host = url
            .host_str()
            .ok_or(RuntimeError::PermissionDenied)?
            .to_owned();
        let domain = url_domain(&url);
        let authorized = contains_domain(&policy.allowed_domains, &domain);
        if !authorized
            && (policy.allowed_domains.len() >= 16
                || !(policy.allow_custom_domains
                    || contains_domain(&policy.requested_domains, &domain))
                || !crate::commands::plugins::valid_domains(std::slice::from_ref(&domain)))
        {
            return Err(RuntimeError::PermissionDenied);
        }
        if !authorized {
            return Err(RuntimeError::AuthorizationRequired(domain));
        }
        let addresses = resolve((
            host,
            url.port_or_known_default()
                .ok_or(RuntimeError::PermissionDenied)?,
        ))
        .await?;
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
    #[test]
    fn post_preserves_body_content_type_and_credential_scope_and_bounds_input() {
        let mut policy = policy();
        policy.token = Some("fixture-token".into());
        policy.token_domain = Some("api.github.com".into());
        let client = Client::builder().no_proxy().build().unwrap();
        let body = b"{\"title\":\"fixture\"}";
        let request = build_request(
            &client,
            Url::parse("https://api.github.com/items").unwrap(),
            Some(("application/json", body)),
            &policy,
        )
        .build()
        .unwrap();
        assert_eq!(request.method(), reqwest::Method::POST);
        assert_eq!(request.headers()["content-type"], "application/json");
        assert_eq!(request.headers()["authorization"], "Bearer fixture-token");
        assert_eq!(request.body().unwrap().as_bytes().unwrap(), body);
        policy.allowed_domains = vec!["custom.example.com".into()];
        let custom = build_request(
            &client,
            Url::parse("https://custom.example.com/items").unwrap(),
            Some(("application/json", body)),
            &policy,
        )
        .build()
        .unwrap();
        assert!(!custom.headers().contains_key("authorization"));
        assert!(validate_post("application/json", &vec![0; 10 * 1024 * 1024]).is_ok());
        assert!(matches!(
            validate_post("application/json", &vec![0; 10 * 1024 * 1024 + 1]),
            Err(RuntimeError::ResourceLimit)
        ));
        for invalid in ["", "application/json\r\nAuthorization: injected"] {
            assert!(validate_post(invalid, body).is_err());
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
    #[test]
    fn custom_ports_scope_get_post_and_credentials_to_the_manifest_destination() {
        let mut policy = policy();
        policy.allowed_domains = vec!["api.github.com:8443".into()];
        policy.token_domain = Some("api.github.com:8443".into());
        policy.token = Some("fixture-token".into());
        let url = authorized_url("https://api.github.com:8443/path", &policy).unwrap();
        let client = Client::new();
        for body in [None, Some(("application/json", b"{}".as_slice()))] {
            let request = build_request(&client, url.clone(), body, &policy)
                .build()
                .unwrap();
            assert_eq!(request.url().port(), Some(8443));
            assert_eq!(request.headers()["authorization"], "Bearer fixture-token");
        }
        let default = Url::parse("https://api.github.com").unwrap();
        assert!(authorized_url(default.as_str(), &policy).is_err());
        assert_eq!(policy.bearer_token(&default), None);
        policy.token_domain = Some("api.github.com".into());
        assert_eq!(policy.bearer_token(&url), None);
        policy.allowed_domains = vec!["api.github.com:443".into()];
        assert!(authorized_url(default.as_str(), &policy).is_ok());
        assert_eq!(policy.bearer_token(&default), Some("fixture-token"));
    }
    #[tokio::test]
    async fn custom_port_approval_precedes_dns_and_preserves_port_without_url_secrets() {
        let mut policy = HttpPolicy {
            requested_domains: vec!["api.example.com:8443".into()],
            ..Default::default()
        };
        let result = get_with(
            "https://api.example.com:8443/path?secret=hidden",
            &policy,
            cancel(),
            deadline(),
            |_| async { panic!("no DNS before approval") },
            |_, _| async { panic!("no HTTP before approval") },
        )
        .await;
        assert!(
            matches!(result, Err(RuntimeError::AuthorizationRequired(domain)) if domain == "api.example.com:8443")
        );
        policy.allowed_domains = policy.requested_domains.clone();
        get_with(
            "https://api.example.com:8443",
            &policy,
            cancel(),
            deadline(),
            |destination| async move {
                assert_eq!(destination, ("api.example.com".into(), 8443));
                Ok(vec!["93.184.216.34:8443".parse().unwrap()])
            },
            |url, addresses| async move {
                assert_eq!(url.port(), Some(8443));
                assert_eq!(addresses[0].port(), 8443);
                Ok(mock_response(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}".to_vec()).await)
            },
        )
        .await
        .unwrap();
        policy.allowed_domains.clear();
        policy.requested_domains = vec!["api.example.com".into()];
        let result = get_with(
            "https://api.example.com:8443",
            &policy,
            cancel(),
            deadline(),
            |_| async { panic!("wrong port cannot resolve") },
            |_, _| async { panic!("wrong port cannot request") },
        )
        .await;
        assert!(matches!(result, Err(RuntimeError::PermissionDenied)));
    }
    #[test]
    fn multiple_hosts_are_exact_and_custom_hosts_never_receive_manifest_credentials() {
        let mut policy = policy();
        policy.token = Some("fixture-token".into());
        policy.token_domain = Some("api.github.com".into());
        let original = Url::parse("https://api.github.com").unwrap();
        let custom = Url::parse("https://custom.example.com").unwrap();
        assert_eq!(policy.bearer_token(&original), Some("fixture-token"));
        policy.allowed_domains.push("custom.example.com".into());
        assert!(authorized_url(custom.as_str(), &policy).is_ok());
        assert!(authorized_url(original.as_str(), &policy).is_ok());
        assert!(authorized_url("https://custom.example.com.evil.test", &policy).is_err());
        assert_eq!(policy.bearer_token(&custom), None);
        assert_eq!(policy.bearer_token(&original), None);
        policy.allowed_domains = vec!["custom.example.com".into()];
        assert_eq!(policy.bearer_token(&custom), None);
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
                assert_eq!(host, ("api.github.com".to_owned(), 443));
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
    async fn unapproved_public_hosts_require_permission_before_sending_any_request() {
        for custom in [false, true] {
            let policy = HttpPolicy {
                requested_domains: if custom {
                    vec![]
                } else {
                    vec!["new.example.com".into()]
                },
                allow_custom_domains: custom,
                ..Default::default()
            };
            let result = get_with(
                "https://NEW.example.com/private?token=never-exposed",
                &policy,
                cancel(),
                deadline(),
                |_| async { panic!("no DNS request before approval") },
                |_, _| async { panic!("no HTTP request before approval") },
            )
            .await;
            assert!(
                matches!(result, Err(RuntimeError::AuthorizationRequired(domain)) if domain == "new.example.com")
            );
        }
        for (url, policy, address) in [
            (
                "https://new.example.com",
                HttpPolicy {
                    allowed_domains: vec!["new.example.com".into()],
                    ..Default::default()
                },
                "127.0.0.1:443",
            ),
            (
                "https://new.example.com",
                HttpPolicy::default(),
                "93.184.216.34:443",
            ),
            (
                "https://api.github.com.evil.example",
                policy(),
                "93.184.216.34:443",
            ),
            (
                "http://new.example.com",
                HttpPolicy {
                    allow_custom_domains: true,
                    ..Default::default()
                },
                "93.184.216.34:443",
            ),
            (
                "https://new.example.com:0",
                HttpPolicy {
                    allow_custom_domains: true,
                    ..Default::default()
                },
                "93.184.216.34:443",
            ),
            (
                "https://user:secret@new.example.com",
                HttpPolicy {
                    allow_custom_domains: true,
                    ..Default::default()
                },
                "93.184.216.34:443",
            ),
            (
                "https://new.example.com",
                HttpPolicy {
                    allowed_domains: (0..16).map(|i| format!("host{i}.example.com")).collect(),
                    allow_custom_domains: true,
                    ..Default::default()
                },
                "93.184.216.34:443",
            ),
        ] {
            let result = get_with(
                url,
                &policy,
                cancel(),
                deadline(),
                |_| async { Ok(vec![address.parse().unwrap()]) },
                |_, _| async { panic!("unsafe or undeclared hosts cannot send requests") },
            )
            .await;
            assert!(
                matches!(result, Err(RuntimeError::PermissionDenied)),
                "{url}"
            );
        }
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
