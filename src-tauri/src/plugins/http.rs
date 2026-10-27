use super::runtime::RuntimeError;
use crate::link_preview::safety::{public_addresses, public_ip, safe_url};
use reqwest::{redirect::Policy, Client, Response, Url};
use std::{
    future::Future,
    net::{IpAddr, SocketAddr},
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
pub(super) fn url_domain(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    }
}
#[derive(Clone, Default)]
pub struct HttpPolicy {
    pub allowed_domains: Vec<String>,
    /// Approved hosts that may resolve to intranet addresses; a subset of `allowed_domains`.
    pub private_network_domains: Vec<String>,
    pub requested_domains: Vec<String>,
    pub allow_custom_domains: bool,
    pub token: Option<String>,
    pub token_domain: Option<String>,
    pub credential_scopes: Option<Vec<super::credentials::CredentialScope>>,
    pub credentials: std::collections::BTreeMap<String, String>,
    pub error_codes: Vec<String>,
    pub log_codes: Vec<String>,
}
impl HttpPolicy {
    fn request_auth(
        &self,
        url: &Url,
    ) -> Result<Option<(reqwest::header::HeaderName, reqwest::header::HeaderValue)>, RuntimeError>
    {
        if url.scheme() != "https" || !contains_domain(&self.allowed_domains, &url_domain(url)) {
            return Ok(None);
        }
        let (name, value) = if let Some(scopes) = &self.credential_scopes {
            if !scopes.iter().any(|scope| {
                canonical_domain(&scope.domain) == url_domain(url) && scope.auth.is_some()
            }) {
                return Ok(None);
            }
            let path = super::credentials::credential_path(url.path())?;
            let scope = scopes
                .iter()
                .filter(|scope| {
                    canonical_domain(&scope.domain) == url_domain(url)
                        && super::credentials::path_matches(&path, &scope.path_prefix)
                })
                .max_by_key(|scope| scope.path_prefix.len());
            let Some(auth) = scope.and_then(|scope| scope.auth.as_ref()) else {
                return Ok(None);
            };
            let Some(secret) = self.credentials.get(auth.secret()) else {
                return Ok(None);
            };
            match auth {
                super::types::HttpAuth::Bearer { .. } => {
                    ("authorization", format!("Bearer {secret}"))
                }
                super::types::HttpAuth::Header { name, .. } => {
                    if !super::credentials::valid_header_name(name) {
                        return Err(RuntimeError::PermissionDenied);
                    }
                    (name.as_str(), secret.clone())
                }
            }
        } else {
            let Some(token) = self.bearer_token(url) else {
                return Ok(None);
            };
            ("authorization", format!("Bearer {token}"))
        };
        let mut value = reqwest::header::HeaderValue::from_str(&value)
            .map_err(|_| RuntimeError::InvalidCredential)?;
        // Keep credentials out of header debug output and HTTP/2 header compression.
        value.set_sensitive(true);
        Ok(Some((
            reqwest::header::HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| RuntimeError::PermissionDenied)?,
            value,
        )))
    }
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
/// Intranet ranges a user may approve per host; loopback, link-local, and other reserved ranges stay blocked.
fn private_network_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, ..] = ip.octets();
            ip.is_private() || (a == 100 && (64..=127).contains(&b))
        }
        IpAddr::V6(ip) => match ip.to_ipv4_mapped() {
            Some(ip) => private_network_ip(IpAddr::V4(ip)),
            None => ip.segments()[0] & 0xfe00 == 0xfc00,
        },
    }
}
pub(super) fn request_url(value: &str) -> Result<Url, RuntimeError> {
    let url = safe_url(value).ok_or(RuntimeError::PermissionDenied)?;
    if url.scheme() != "https" || url.port_or_known_default().is_none_or(|port| port == 0) {
        return Err(RuntimeError::PermissionDenied);
    }
    Ok(url)
}
/// One guest HTTP request. Legacy GET/POST use the same validation and transport.
#[derive(Clone, Debug)]
pub(crate) struct Outgoing {
    pub method: reqwest::Method,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}
impl Outgoing {
    pub(crate) fn get() -> Self {
        Self {
            method: reqwest::Method::GET,
            headers: vec![],
            body: None,
        }
    }
    pub(crate) fn post(content_type: &str, body: &[u8]) -> Self {
        Self {
            method: reqwest::Method::POST,
            headers: vec![("Content-Type".into(), content_type.into())],
            body: Some(body.to_vec()),
        }
    }
}
const BODY_LIMIT: usize = 10 * 1024 * 1024;
const RESPONSE_LIMIT: usize = 64 * 1024 * 1024;
/// Guest headers may not replace host-managed, transport, or credential headers.
fn guest_header(name: &str, policy: &HttpPolicy) -> bool {
    let lower = name.to_ascii_lowercase();
    (matches!(lower.as_str(), "accept" | "content-type")
        || (lower != "expect" && super::credentials::valid_header_name(name)))
        && !policy
            .credential_scopes
            .iter()
            .flatten()
            .filter_map(|scope| match &scope.auth {
                Some(super::types::HttpAuth::Header { name, .. }) => Some(name),
                _ => None,
            })
            .any(|credential| credential.eq_ignore_ascii_case(name))
}
pub(crate) fn validate_request(
    request: &Outgoing,
    policy: &HttpPolicy,
) -> Result<(), RuntimeError> {
    if request
        .body
        .as_ref()
        .is_some_and(|body| body.len() > BODY_LIMIT)
        || request.headers.len() > 32
        || request
            .headers
            .iter()
            .map(|(name, value)| name.len() + value.len())
            .sum::<usize>()
            > 32 * 1024
    {
        return Err(RuntimeError::ResourceLimit);
    }
    let mut names = std::collections::HashSet::new();
    for (name, value) in &request.headers {
        if !names.insert(name.to_ascii_lowercase())
            || !guest_header(name, policy)
            || value.len() > 8192
            || reqwest::header::HeaderValue::from_str(value).is_err()
        {
            return Err(RuntimeError::PermissionDenied);
        }
    }
    let content_type = request
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        .map(|(_, value)| value);
    if request.method == reqwest::Method::GET && request.body.is_some()
        || request.body.is_some()
            && content_type.is_none_or(|value| value.is_empty() || value.len() > 256)
    {
        return Err(RuntimeError::PermissionDenied);
    }
    Ok(())
}
pub(crate) async fn request(
    value: &str,
    outgoing: &Outgoing,
    policy: &HttpPolicy,
    cancel: Arc<AtomicBool>,
    deadline: Instant,
) -> Result<super::runtime::bindings::cardbe::plugin::types::HttpResponse, RuntimeError> {
    validate_request(outgoing, policy)?;
    send(value, outgoing, policy, cancel, deadline).await
}
fn build_request(
    client: &Client,
    url: Url,
    outgoing: &Outgoing,
    policy: &HttpPolicy,
) -> Result<reqwest::RequestBuilder, RuntimeError> {
    let mut request = client.request(outgoing.method.clone(), url.clone());
    if !outgoing
        .headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("accept"))
    {
        request = request.header("Accept", "application/json");
    }
    for (name, value) in &outgoing.headers {
        request = request.header(name.as_str(), value.as_str());
    }
    if let Some(body) = &outgoing.body {
        request = request.body(body.clone());
    }
    if let Some((name, value)) = policy.request_auth(&url)? {
        request = request.header(name, value);
    }
    Ok(request)
}
async fn send(
    value: &str,
    outgoing: &Outgoing,
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
            build_request(&client, url, outgoing, policy)?
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
        policy.request_auth(&url)?;
        let addresses = resolve((
            host,
            url.port_or_known_default()
                .ok_or(RuntimeError::PermissionDenied)?,
        ))
        .await?;
        if !public_addresses(&addresses) {
            if addresses.is_empty()
                || !addresses
                    .iter()
                    .all(|address| public_ip(address.ip()) || private_network_ip(address.ip()))
            {
                return Err(RuntimeError::BlockedAddress);
            }
            if !contains_domain(&policy.private_network_domains, &domain) {
                return Err(RuntimeError::PrivateNetworkRequired(domain));
            }
        }
        let mut response = request(url, addresses).await?;
        // 304 answers a conditional request and is not a redirect.
        if response.status().is_redirection()
            && response.status() != reqwest::StatusCode::NOT_MODIFIED
        {
            return Err(RuntimeError::Redirect);
        }
        let headers = [
            "link",
            "retry-after",
            "x-ratelimit-remaining",
            "x-ratelimit-reset",
            "content-type",
            "etag",
            "last-modified",
            "location",
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
        if response
            .content_length()
            .is_some_and(|length| length > RESPONSE_LIMIT as u64)
        {
            return Err(RuntimeError::ResourceLimit);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| RuntimeError::Network)? {
            if body.len() + chunk.len() > RESPONSE_LIMIT {
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
    use crate::plugins::{credentials::CredentialScope, types::HttpAuth};
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
            &legacy(Some(("application/json", body))),
            &policy,
        )
        .unwrap()
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
            &legacy(Some(("application/json", body))),
            &policy,
        )
        .unwrap()
        .build()
        .unwrap();
        assert!(!custom.headers().contains_key("authorization"));
        let policy = HttpPolicy::default();
        assert!(validate_request(
            &Outgoing::post("application/json", &vec![0; BODY_LIMIT]),
            &policy
        )
        .is_ok());
        assert!(matches!(
            validate_request(
                &Outgoing::post("application/json", &vec![0; BODY_LIMIT + 1]),
                &policy
            ),
            Err(RuntimeError::ResourceLimit)
        ));
        for invalid in ["", "application/json\r\nAuthorization: injected"] {
            assert!(validate_request(&Outgoing::post(invalid, body), &policy).is_err());
        }
    }
    fn legacy(post: Option<(&str, &[u8])>) -> Outgoing {
        post.map_or_else(Outgoing::get, |(content_type, body)| {
            Outgoing::post(content_type, body)
        })
    }
    fn send_request(
        method: reqwest::Method,
        headers: &[(&str, &str)],
        body: Option<&[u8]>,
    ) -> Outgoing {
        Outgoing {
            method,
            headers: headers
                .iter()
                .map(|(name, value)| ((*name).into(), (*value).into()))
                .collect(),
            body: body.map(<[u8]>::to_vec),
        }
    }
    #[test]
    fn http_send_methods_carry_guest_headers_without_overriding_host_or_credential_headers() {
        use reqwest::Method;
        let mut policy = scoped_policy();
        policy
            .credential_scopes
            .as_mut()
            .unwrap()
            .push(CredentialScope {
                domain: "custom.example.com".into(),
                path_prefix: "/keys".into(),
                auth: Some(HttpAuth::Header {
                    name: "X-API-Key".into(),
                    secret: "token".into(),
                }),
            });
        let client = Client::builder().no_proxy().build().unwrap();
        for method in [Method::PUT, Method::PATCH, Method::DELETE, Method::POST] {
            let outgoing = send_request(
                method.clone(),
                &[
                    ("Content-Type", "application/merge-patch+json"),
                    ("Accept", "application/vnd.github+json"),
                    ("If-Match", "\"v1\""),
                    ("X-GitHub-Api-Version", "2022-11-28"),
                ],
                Some(b"{}"),
            );
            validate_request(&outgoing, &policy).unwrap();
            let request = build_request(
                &client,
                Url::parse("https://custom.example.com/api/tasks/1").unwrap(),
                &outgoing,
                &policy,
            )
            .unwrap()
            .build()
            .unwrap();
            assert_eq!(request.method(), method);
            let headers = request.headers();
            assert_eq!(headers.get_all("accept").iter().count(), 1);
            assert_eq!(headers["accept"], "application/vnd.github+json");
            assert_eq!(headers["content-type"], "application/merge-patch+json");
            assert_eq!(headers["if-match"], "\"v1\"");
            assert_eq!(headers["x-github-api-version"], "2022-11-28");
            assert_eq!(headers["authorization"], "Bearer fixture-token");
        }
        let delete = send_request(Method::DELETE, &[], None);
        validate_request(&delete, &policy).unwrap();
        let request = build_request(
            &client,
            Url::parse("https://custom.example.com/api/tasks/1").unwrap(),
            &delete,
            &policy,
        )
        .unwrap()
        .build()
        .unwrap();
        assert_eq!(request.headers()["accept"], "application/json");
        assert!(request.body().is_none());
        for (method, headers, body) in [
            (
                Method::GET,
                vec![("Content-Type", "application/json")],
                Some(&b"{}"[..]),
            ),
            (Method::PUT, vec![], Some(&b"{}"[..])),
            (Method::PUT, vec![("Content-Type", "")], Some(&b"{}"[..])),
            (Method::PATCH, vec![("Authorization", "Bearer guest")], None),
            (Method::PATCH, vec![("x-api-key", "guest")], None),
            (Method::PATCH, vec![("Cookie", "a=b")], None),
            (Method::PATCH, vec![("Host", "evil.example")], None),
            (Method::PATCH, vec![("Expect", "100-continue")], None),
            (Method::PATCH, vec![("Sec-Fetch-Site", "none")], None),
            (Method::PATCH, vec![("Bad Name", "x")], None),
            (Method::PATCH, vec![("X-Value", "a\r\nInjected: yes")], None),
            (Method::PATCH, vec![("X-Dup", "a"), ("x-dup", "b")], None),
        ] {
            assert!(
                matches!(
                    validate_request(&send_request(method, &headers, body), &policy),
                    Err(RuntimeError::PermissionDenied)
                ),
                "{headers:?}"
            );
        }
        let many: Vec<_> = (0..33)
            .map(|i| (format!("X-{i}"), "v".to_owned()))
            .collect();
        let many: Vec<_> = many.iter().map(|(n, v)| (n.as_str(), v.as_str())).collect();
        assert!(matches!(
            validate_request(&send_request(Method::GET, &many, None), &policy),
            Err(RuntimeError::ResourceLimit)
        ));
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
            let request = build_request(&client, url.clone(), &legacy(body), &policy)
                .unwrap()
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
    fn scoped_policy() -> HttpPolicy {
        HttpPolicy {
            allowed_domains: vec!["custom.example.com".into(), "fixed.example.com".into()],
            allow_custom_domains: true,
            token: Some("legacy-must-not-leak".into()),
            token_domain: Some("custom.example.com".into()),
            credential_scopes: Some(vec![
                CredentialScope {
                    domain: "custom.example.com".into(),
                    path_prefix: "/api/tasks".into(),
                    auth: Some(HttpAuth::Bearer {
                        secret: "token".into(),
                    }),
                },
                CredentialScope {
                    domain: "custom.example.com".into(),
                    path_prefix: "/api/tasks/public".into(),
                    auth: None,
                },
                CredentialScope {
                    domain: "custom.example.com".into(),
                    path_prefix: "/api/admin".into(),
                    auth: Some(HttpAuth::Bearer {
                        secret: "admin_token".into(),
                    }),
                },
                CredentialScope {
                    domain: "fixed.example.com".into(),
                    path_prefix: "/tasks".into(),
                    auth: Some(HttpAuth::Bearer {
                        secret: "fixed_token".into(),
                    }),
                },
            ]),
            credentials: std::collections::BTreeMap::from([
                ("token".into(), "fixture-token".into()),
                ("admin_token".into(), "fixture-admin".into()),
                ("fixed_token".into(), "fixture-fixed".into()),
            ]),
            ..Default::default()
        }
    }
    #[test]
    fn scoped_credentials_include_exact_ports() {
        let mut policy = scoped_policy();
        policy
            .allowed_domains
            .push("custom.example.com:8443".into());
        policy.credential_scopes.as_mut().unwrap()[0].domain = "custom.example.com:8443".into();
        assert_eq!(
            policy
                .request_auth(&Url::parse("https://custom.example.com:8443/api/tasks").unwrap())
                .unwrap()
                .map(|(_, value)| value.to_str().unwrap().to_owned()),
            Some("Bearer fixture-token".to_owned())
        );
        assert_eq!(
            policy
                .request_auth(&Url::parse("https://custom.example.com/api/tasks").unwrap())
                .unwrap()
                .map(|(_, value)| value.to_str().unwrap().to_owned()),
            None
        );
        assert_eq!(
            policy
                .request_auth(&Url::parse("https://custom.example.com:9443/api/tasks").unwrap())
                .unwrap()
                .map(|(_, value)| value.to_str().unwrap().to_owned()),
            None
        );
    }

    fn api_key_policy() -> HttpPolicy {
        let mut policy = scoped_policy();
        policy
            .allowed_domains
            .push("custom.example.com:8443".into());
        policy.credential_scopes = Some(vec![
            CredentialScope {
                domain: "custom.example.com:8443".into(),
                path_prefix: "/".into(),
                auth: Some(HttpAuth::Bearer {
                    secret: "fixed_token".into(),
                }),
            },
            CredentialScope {
                domain: "custom.example.com:8443".into(),
                path_prefix: "/api/tasks".into(),
                auth: Some(HttpAuth::Header {
                    name: "X-API-Key".into(),
                    secret: "token".into(),
                }),
            },
            CredentialScope {
                domain: "custom.example.com:8443".into(),
                path_prefix: "/api/tasks/public".into(),
                auth: None,
            },
        ]);
        policy
    }
    #[test]
    fn api_key_headers_are_scoped_for_get_and_post_without_bearer_or_secret_fallback() {
        let client = Client::new();
        let mut policy = api_key_policy();
        for (url, expected) in [
            (
                "https://custom.example.com:8443/api/tasks/1?x=ignored",
                Some("fixture-token"),
            ),
            ("https://custom.example.com:8443/api/tasks/public/1", None),
            ("https://custom.example.com/api/tasks", None),
            ("https://custom.example.com:9443/api/tasks", None),
            ("https://fixed.example.com/api/tasks", None),
        ] {
            for post in [None, Some(("application/json", b"{}".as_slice()))] {
                let request =
                    build_request(&client, Url::parse(url).unwrap(), &legacy(post), &policy)
                        .unwrap()
                        .build()
                        .unwrap();
                assert_eq!(
                    request
                        .headers()
                        .get("x-api-key")
                        .map(|v| v.to_str().unwrap()),
                    expected,
                    "{url}"
                );
                assert!(request
                    .headers()
                    .get("x-api-key")
                    .is_none_or(|value| value.is_sensitive()));
                assert!(!request.headers().contains_key("authorization"));
            }
        }
        policy.credentials.remove("token");
        let request = build_request(
            &client,
            Url::parse("https://custom.example.com:8443/api/tasks").unwrap(),
            &Outgoing::get(),
            &policy,
        )
        .unwrap()
        .build()
        .unwrap();
        assert!(
            !request.headers().contains_key("x-api-key")
                && !request.headers().contains_key("authorization")
        );
    }
    #[tokio::test]
    async fn api_key_approval_and_header_validation_precede_dns_and_http() {
        let mut policy = api_key_policy();
        policy.allowed_domains.clear();
        policy
            .credentials
            .insert("token".into(), "bad\r\nX-Injected: yes".into());
        let url = "https://custom.example.com:8443/api/tasks";
        let result = get_with(
            url,
            &policy,
            cancel(),
            deadline(),
            |_| async { panic!("no DNS before approval") },
            |_, _| async { panic!("no HTTP before approval") },
        )
        .await;
        assert!(
            matches!(result, Err(RuntimeError::AuthorizationRequired(host)) if host == "custom.example.com:8443")
        );
        policy
            .allowed_domains
            .push("custom.example.com:8443".into());
        let result = get_with(
            url,
            &policy,
            cancel(),
            deadline(),
            |_| async { panic!("no DNS for invalid header") },
            |_, _| async { panic!("no HTTP for invalid header") },
        )
        .await;
        assert!(matches!(result, Err(RuntimeError::InvalidCredential)));
    }

    #[test]
    fn scoped_credentials_are_per_host_and_path_for_get_and_post() {
        let client = Client::builder().no_proxy().build().unwrap();
        let policy = scoped_policy();
        for (url, expected) in [
            (
                "https://custom.example.com/api/tasks",
                Some("Bearer fixture-token"),
            ),
            (
                "https://custom.example.com/api/tasks/123?other=ignored",
                Some("Bearer fixture-token"),
            ),
            (
                "https://custom.example.com/api/%74asks/123",
                Some("Bearer fixture-token"),
            ),
            ("https://custom.example.com/api/tasks/public", None),
            ("https://custom.example.com/api/tasks/public/123", None),
            (
                "https://custom.example.com/api/tasks/public-other",
                Some("Bearer fixture-token"),
            ),
            ("https://custom.example.com/api/tasks-other", None),
            (
                "https://custom.example.com/api/admin",
                Some("Bearer fixture-admin"),
            ),
            (
                "https://fixed.example.com/tasks",
                Some("Bearer fixture-fixed"),
            ),
            ("https://fixed.example.com/api/tasks", None),
            ("https://custom.example.com.evil.test/api/tasks", None),
            ("http://custom.example.com/api/tasks", None),
            ("https://custom.example.com:444/api/tasks", None),
        ] {
            for body in [None, Some(("application/json", &b"{}"[..]))] {
                let request =
                    build_request(&client, Url::parse(url).unwrap(), &legacy(body), &policy)
                        .unwrap()
                        .build()
                        .unwrap();
                assert_eq!(
                    request
                        .headers()
                        .get("authorization")
                        .map(|v| v.to_str().unwrap()),
                    expected,
                    "{url}"
                );
                assert!(request
                    .headers()
                    .get("authorization")
                    .is_none_or(|value| value.is_sensitive()));
            }
        }
        let mut policy = policy;
        policy.credentials.remove("token");
        assert_eq!(
            policy
                .request_auth(&Url::parse("https://custom.example.com/api/tasks").unwrap())
                .unwrap()
                .map(|(_, value)| value.to_str().unwrap().to_owned()),
            None
        );
        policy
            .credential_scopes
            .as_mut()
            .unwrap()
            .push(CredentialScope {
                domain: "custom.example.com".into(),
                path_prefix: "/".into(),
                auth: Some(HttpAuth::Bearer {
                    secret: "fixed_token".into(),
                }),
            });
        assert_eq!(
            policy
                .request_auth(&Url::parse("https://custom.example.com/api/tasks").unwrap())
                .unwrap()
                .map(|(_, value)| value.to_str().unwrap().to_owned()),
            None
        );
        policy.credential_scopes = Some(vec![]);
        policy.allowed_domains = vec!["custom.example.com".into()];
        assert_eq!(
            policy
                .request_auth(&Url::parse("https://custom.example.com/api/tasks").unwrap())
                .unwrap()
                .map(|(_, value)| value.to_str().unwrap().to_owned()),
            None
        );
    }

    #[tokio::test]
    async fn scoped_credentials_do_not_bypass_approval_or_resolve_ambiguous_paths() {
        let mut policy = scoped_policy();
        policy.allowed_domains.clear();
        let result = get_with(
            "https://custom.example.com/api/tasks?secret=not-persisted",
            &policy,
            cancel(),
            deadline(),
            |_| async { panic!("DNS before approval") },
            |_, _| async { panic!("HTTP before approval") },
        )
        .await;
        assert!(
            matches!(result, Err(RuntimeError::AuthorizationRequired(host)) if host == "custom.example.com")
        );
        policy.allowed_domains.push("custom.example.com".into());
        let result = get_with(
            "https://custom.example.com/api/tasks%2fpublic",
            &policy,
            cancel(),
            deadline(),
            |_| async { panic!("DNS for ambiguous credential path") },
            |_, _| async { panic!("HTTP for ambiguous credential path") },
        )
        .await;
        assert!(matches!(result, Err(RuntimeError::PermissionDenied)));
        policy.allowed_domains.clear();
        let url = Url::parse("https://custom.example.com/api/tasks").unwrap();
        assert_eq!(policy.request_auth(&url).unwrap(), None);
        policy.allow_custom_domains = false;
        let result = get_with(
            url.as_str(),
            &policy,
            cancel(),
            deadline(),
            |_| async { panic!("DNS outside network capability") },
            |_, _| async { panic!("HTTP outside network capability") },
        )
        .await;
        assert!(matches!(result, Err(RuntimeError::PermissionDenied)));
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
    async fn private_network_answers_require_per_host_approval_and_reserved_ranges_stay_blocked() {
        let resolve = |answer: &'static str| {
            move |_: (String, u16)| async move { Ok(vec![answer.parse::<SocketAddr>().unwrap()]) }
        };
        for answer in [
            "10.1.2.3:443",
            "172.16.0.1:443",
            "192.168.1.10:443",
            "100.100.1.1:443",
            "[fd00::1]:443",
            "[::ffff:10.1.2.3]:443",
        ] {
            let result = get_with(
                "https://api.github.com/search?token=hidden",
                &policy(),
                cancel(),
                deadline(),
                resolve(answer),
                |_, _| async { panic!("no HTTP before private network approval") },
            )
            .await;
            assert!(
                matches!(&result, Err(RuntimeError::PrivateNetworkRequired(domain)) if domain == "api.github.com"),
                "{answer}"
            );
        }
        let mut granted = policy();
        granted.private_network_domains = vec!["api.github.com:443".into()];
        for answer in [
            "127.0.0.1:443",
            "169.254.169.254:443",
            "0.0.0.0:443",
            "[::1]:443",
            "[fe80::1]:443",
        ] {
            let result = get_with(
                "https://api.github.com",
                &granted,
                cancel(),
                deadline(),
                resolve(answer),
                |_, _| async { panic!("reserved addresses must not reach client") },
            )
            .await;
            assert!(
                matches!(result, Err(RuntimeError::BlockedAddress)),
                "{answer}"
            );
        }
        let mut other = granted.clone();
        other.private_network_domains = vec!["api.github.com:8443".into()];
        let result = get_with(
            "https://api.github.com",
            &other,
            cancel(),
            deadline(),
            resolve("10.1.2.3:443"),
            |_, _| async { panic!("a grant covers only its exact host and port") },
        )
        .await;
        assert!(matches!(
            result,
            Err(RuntimeError::PrivateNetworkRequired(_))
        ));
        let result = get_with(
            "https://api.github.com",
            &granted,
            cancel(),
            deadline(),
            |_| async {
                Ok(vec![
                    "93.184.216.34:443".parse().unwrap(),
                    "10.1.2.3:443".parse().unwrap(),
                ])
            },
            |_, addresses| async move {
                assert_eq!(addresses.len(), 2);
                Ok(mock_response(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}".to_vec()).await)
            },
        )
        .await
        .unwrap();
        assert_eq!(result.body, b"{}");
        let result = get_with(
            "https://api.github.com",
            &granted,
            cancel(),
            deadline(),
            resolve("10.1.2.3:443"),
            |_, _| async {
                Ok(mock_response(
                    b"HTTP/1.1 302 Found\r\nLocation: https://sso.example.com/\r\nContent-Length: 0\r\n\r\n"
                        .to_vec(),
                )
                .await)
            },
        )
        .await;
        assert!(matches!(result, Err(RuntimeError::Redirect)));
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
        assert!(matches!(result, Err(RuntimeError::BlockedAddress)));
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
            if address.starts_with("127.") {
                assert!(matches!(result, Err(RuntimeError::BlockedAddress)), "{url}");
            } else {
                assert!(
                    matches!(result, Err(RuntimeError::PermissionDenied)),
                    "{url}"
                );
            }
        }
    }

    #[tokio::test]
    async fn redirects_declared_and_streamed_size_limits_are_rejected() {
        for bytes in [b"HTTP/1.1 302 Found\r\nContent-Length: 0\r\nLocation: http://127.0.0.1/private\r\n\r\n".to_vec(),format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",RESPONSE_LIMIT+1).into_bytes(),[b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec(),vec![b'x';RESPONSE_LIMIT+1]].concat()] {
            let result=get_with("https://api.github.com",&policy(),cancel(),deadline(),|_|async {Ok(vec!["93.184.216.34:443".parse().unwrap()])},|_,_|async {Ok(mock_response(bytes).await)}).await;
            assert!(matches!(result,Err(RuntimeError::Redirect|RuntimeError::ResourceLimit)),"{result:?}");
        }
        for bytes in [
            [
                format!("HTTP/1.1 200 OK\r\nContent-Length: {RESPONSE_LIMIT}\r\n\r\n").into_bytes(),
                vec![b'x'; RESPONSE_LIMIT],
            ]
            .concat(),
            [
                b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".to_vec(),
                vec![b'x'; RESPONSE_LIMIT],
            ]
            .concat(),
        ] {
            let result = get_with(
                "https://api.github.com",
                &policy(),
                cancel(),
                deadline(),
                |_| async { Ok(vec!["93.184.216.34:443".parse().unwrap()]) },
                |_, _| async { Ok(mock_response(bytes).await) },
            )
            .await;
            assert_eq!(result.unwrap().body.len(), RESPONSE_LIMIT);
        }
    }
    #[tokio::test]
    async fn conditional_and_created_responses_expose_cache_and_location_headers() {
        for (bytes, status) in [
            (
                b"HTTP/1.1 304 Not Modified\r\nETag: \"v2\"\r\nLast-Modified: Sat, 17 Oct 2026 00:00:00 GMT\r\nSet-Cookie: secret=1\r\n\r\n".to_vec(),
                304,
            ),
            (
                b"HTTP/1.1 201 Created\r\nContent-Length: 0\r\nLocation: /items/2\r\nETag: \"v1\"\r\n\r\n".to_vec(),
                201,
            ),
        ] {
            let response = get_with(
                "https://api.github.com",
                &policy(),
                cancel(),
                deadline(),
                |_| async { Ok(vec!["93.184.216.34:443".parse().unwrap()]) },
                |_, _| async { Ok(mock_response(bytes).await) },
            )
            .await
            .unwrap();
            assert_eq!(response.status, status);
            let names: Vec<_> = response.headers.iter().map(|(name, _)| name.as_str()).collect();
            assert!(names.contains(&"etag"));
            assert!(!names.contains(&"set-cookie"));
            if status == 304 {
                assert!(names.contains(&"last-modified"));
            } else {
                assert!(response
                    .headers
                    .contains(&("location".into(), "/items/2".into())));
            }
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
