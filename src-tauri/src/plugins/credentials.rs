use super::{
    runtime::RuntimeError,
    types::{CredentialDestination, HttpAuth, PluginPackage},
};
use reqwest::Url;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub struct CredentialScope {
    pub domain: String,
    pub path_prefix: String,
    pub auth: Option<HttpAuth>,
}

pub fn valid_header_name(name: &str) -> bool {
    name.len() <= 128
        && reqwest::header::HeaderName::from_bytes(name.as_bytes()).is_ok()
        && !matches!(
            name.to_ascii_lowercase().as_str(),
            "authorization"
                | "host"
                | "cookie"
                | "cookie2"
                | "content-length"
                | "connection"
                | "keep-alive"
                | "proxy-authorization"
                | "proxy-authenticate"
                | "proxy-connection"
                | "te"
                | "trailer"
                | "transfer-encoding"
                | "upgrade"
                | "accept"
                | "content-type"
                | "user-agent"
        )
        && !name.to_ascii_lowercase().starts_with("sec-")
}

pub fn valid_path_prefix(path: &str) -> bool {
    path.starts_with('/')
        && path.len() <= 2048
        && (path == "/" || !path.ends_with('/'))
        && !path.contains("//")
        && !path.split('/').any(|part| matches!(part, "." | ".."))
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/-._~".contains(&b))
}

pub fn valid_rules(package: &PluginPackage) -> bool {
    matches!(package.manifest_version, 1 | 2)
        && package.http_credentials.as_ref().is_none_or(|rules| {
            rules.len() <= 64
                && rules.iter().all(|rule| {
                    (package.manifest_version == 2 || rule.auth.is_none())
                        && valid_path_prefix(&rule.path_prefix)
                        && !(rule.auth.is_some() && rule.bearer_secret.is_some())
                        && rule.auth.as_ref().is_none_or(|auth| match auth {
                            HttpAuth::Bearer { .. } => true,
                            HttpAuth::Header { name, .. } => valid_header_name(name),
                        })
                        && rule
                            .auth
                            .as_ref()
                            .map(HttpAuth::secret)
                            .or(rule.bearer_secret.as_deref())
                            .is_none_or(|key| {
                                package
                                    .settings
                                    .iter()
                                    .any(|f| f.key == key && f.kind == "secret")
                            })
                        && match &rule.destination {
                            CredentialDestination::Domain { domain } => {
                                super::http::contains_domain(&package.domains, domain)
                            }
                            CredentialDestination::BaseUrl { setting } => {
                                package.settings.iter().any(|f| {
                                    f.key == *setting
                                        && f.kind == "text"
                                        && f.default.as_ref().is_none_or(|value| {
                                            value.as_str().is_some_and(|s| {
                                                s.is_empty() || base_url(s).is_some_and(|url| {
                                                crate::commands::plugins::valid_instance_domains(
                                                    package,
                                                    &[super::http::url_domain(&url)],
                                                )
                                            })
                                            })
                                        })
                                })
                            }
                        }
                })
                && rules.iter().enumerate().all(|(index, rule)| {
                    !rules[..index].iter().any(|other| {
                        rule.destination == other.destination
                            && rule.path_prefix == other.path_prefix
                    })
                })
        })
}

pub fn base_url(value: &str) -> Option<Url> {
    // Check the original path before URL parsing removes dot segments.
    let (_, authority_and_path) = value.split_once("://")?;
    let path = authority_and_path
        .find('/')
        .map_or("/", |index| &authority_and_path[index..]);
    if path.contains("//") {
        return None;
    }
    let path = path.strip_suffix('/').unwrap_or(path);
    if !valid_path_prefix(if path.is_empty() { "/" } else { path }) {
        return None;
    }
    let url = super::http::request_url(value).ok()?;
    if url.query().is_some() || value.contains('#') || value.contains('\\') {
        return None;
    }
    let path = url.path().trim_end_matches('/');
    if !valid_path_prefix(if path.is_empty() { "/" } else { path }) {
        return None;
    }
    Some(url)
}

pub fn invalid_base_url_settings<'a>(
    package: &'a PluginPackage,
    config: &BTreeMap<String, serde_json::Value>,
) -> Vec<&'a str> {
    package
        .http_credentials
        .iter()
        .flatten()
        .filter_map(|rule| {
            let CredentialDestination::BaseUrl { setting } = &rule.destination else {
                return None;
            };
            let value = config.get(setting)?;
            if value.as_str() == Some("") {
                return None;
            }
            let valid = value.as_str().and_then(base_url).is_some_and(|url| {
                crate::commands::plugins::valid_instance_domains(
                    package,
                    &[super::http::url_domain(&url)],
                )
            });
            (!valid).then_some(setting.as_str())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub fn resolve(
    package: &PluginPackage,
    config: &BTreeMap<String, serde_json::Value>,
) -> Result<Option<Vec<CredentialScope>>, RuntimeError> {
    if !valid_rules(package) || !invalid_base_url_settings(package, config).is_empty() {
        return Err(RuntimeError::PermissionDenied);
    }
    let Some(rules) = &package.http_credentials else {
        return Ok(None);
    };
    let mut scopes = Vec::new();
    for rule in rules {
        let (domain, base_path) = match &rule.destination {
            CredentialDestination::Domain { domain } => (
                super::http::canonical_domain(domain).to_owned(),
                String::new(),
            ),
            CredentialDestination::BaseUrl { setting } => {
                let Some(value) = config
                    .get(setting)
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                else {
                    continue;
                };
                let url = base_url(value).ok_or(RuntimeError::PermissionDenied)?;
                (
                    super::http::url_domain(&url),
                    url.path().trim_end_matches('/').to_owned(),
                )
            }
        };
        let path_prefix = if rule.path_prefix == "/" {
            if base_path.is_empty() {
                "/".into()
            } else {
                base_path
            }
        } else {
            format!("{base_path}{}", rule.path_prefix)
        };
        if scopes.iter().any(|scope: &CredentialScope| {
            scope.domain == domain && scope.path_prefix == path_prefix
        }) {
            return Err(RuntimeError::PermissionDenied);
        }
        scopes.push(CredentialScope {
            domain,
            path_prefix,
            auth: rule.auth.clone().or_else(|| {
                rule.bearer_secret
                    .clone()
                    .map(|secret| HttpAuth::Bearer { secret })
            }),
        });
    }
    Ok(Some(scopes))
}

pub fn changed_base_url_hosts(
    package: &PluginPackage,
    previous: &BTreeMap<String, serde_json::Value>,
    current: &BTreeMap<String, serde_json::Value>,
) -> Vec<String> {
    package
        .http_credentials
        .iter()
        .flatten()
        .filter_map(|rule| {
            let CredentialDestination::BaseUrl { setting } = &rule.destination else {
                return None;
            };
            let host = |config: &BTreeMap<String, serde_json::Value>| {
                config
                    .get(setting)
                    .and_then(|v| v.as_str())
                    .and_then(base_url)
                    .map(|url| super::http::url_domain(&url))
            };
            let new = host(current)?;
            (host(previous).as_ref() != Some(&new)).then_some(new)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub fn path_matches(path: &str, prefix: &str) -> bool {
    prefix == "/"
        || path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

// Decode unreserved characters; reject encodings that can change server routing.
pub fn credential_path(path: &str) -> Result<String, RuntimeError> {
    let bytes = path.as_bytes();
    let mut normalized = String::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let code = path
                .get(index + 1..index + 3)
                .and_then(|s| u8::from_str_radix(s, 16).ok())
                .ok_or(RuntimeError::PermissionDenied)?;
            if b"/\\%?#;".contains(&code) || code < 32 || code == 127 {
                return Err(RuntimeError::PermissionDenied);
            }
            if code.is_ascii_alphanumeric() || b"-._~".contains(&code) {
                normalized.push(code as char);
            } else {
                normalized.push_str(&path[index..index + 3]);
            }
            index += 3;
        } else {
            // URL paths are serialized as ASCII by reqwest::Url.
            normalized.push(bytes[index] as char);
            index += 1;
        }
    }
    if normalized.contains("//") || normalized.contains('\\') || normalized.contains(';') {
        return Err(RuntimeError::PermissionDenied);
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn package() -> PluginPackage {
        serde_json::from_value(json!({
            "id":"example", "name":"Example", "version":"1.0.0", "api_version":"1.0.0", "storage_schema_version":1,
            "domains":["fixed.example.com"], "allow_custom_domains":true,
            "settings":[{"key":"base_url","type":"text","label":"URL"},{"key":"token","type":"secret","label":"Token"}],
            "http_credentials":[
                {"destination":{"type":"base_url","setting":"base_url"},"path_prefix":"/tasks","bearer_secret":"token"},
                {"destination":{"type":"base_url","setting":"base_url"},"path_prefix":"/public"},
                {"destination":{"type":"domain","domain":"fixed.example.com"},"path_prefix":"/","bearer_secret":"token"}
            ]
        })).unwrap()
    }

    #[test]
    fn base_url_ports_are_preserved_and_require_new_grants() {
        let pkg = package();
        let previous =
            BTreeMap::from([("base_url".into(), json!("https://custom.example.com/api"))]);
        let current = BTreeMap::from([(
            "base_url".into(),
            json!("https://custom.example.com:8443/api"),
        )]);
        assert_eq!(
            resolve(&pkg, &current).unwrap().unwrap()[0].domain,
            "custom.example.com:8443"
        );
        assert_eq!(
            changed_base_url_hosts(&pkg, &previous, &current),
            vec!["custom.example.com:8443"]
        );
        let mut restricted = pkg;
        restricted.allow_custom_domains = false;
        restricted.domains = vec!["custom.example.com".into(), "fixed.example.com".into()];
        assert!(!invalid_base_url_settings(&restricted, &current).is_empty());
        restricted.domains.push("custom.example.com:8443".into());
        assert!(invalid_base_url_settings(&restricted, &current).is_empty());
    }

    #[test]
    fn scopes_resolve_base_paths_without_granting_network_access() {
        let pkg = package();
        assert!(valid_rules(&pkg));
        let config =
            BTreeMap::from([("base_url".into(), json!("https://custom.example.com/api/"))]);
        let scopes = resolve(&pkg, &config).unwrap().unwrap();
        assert_eq!(scopes.len(), 3);
        assert_eq!(scopes[0].domain, "custom.example.com");
        assert_eq!(scopes[0].path_prefix, "/api/tasks");
        assert_eq!(scopes[0].auth.as_ref().map(HttpAuth::secret), Some("token"));
        assert_eq!(scopes[1].path_prefix, "/api/public");
        assert!(scopes[1].auth.is_none());
        assert_eq!(scopes[2].domain, "fixed.example.com");
        assert_eq!(scopes[2].path_prefix, "/");
        assert_eq!(resolve(&pkg, &BTreeMap::new()).unwrap().unwrap().len(), 1);
        let mut restricted = pkg.clone();
        restricted.allow_custom_domains = false;
        assert_eq!(
            invalid_base_url_settings(&restricted, &config),
            vec!["base_url"]
        );
        assert!(resolve(&restricted, &config).is_err());
    }

    #[test]
    fn malformed_manifest_rules_and_base_urls_are_rejected() {
        for path in [
            "tasks",
            "/tasks/",
            "/a//b",
            "/a/../b",
            "/a/./b",
            "/a%2fb",
            "/a?token=x",
            "/a#b",
            "/a\\b",
            "/a/*",
        ] {
            let mut pkg = package();
            pkg.http_credentials.as_mut().unwrap()[0].path_prefix = path.into();
            assert!(!valid_rules(&pkg), "{path}");
        }
        for (field, key) in [
            ("secret", "missing"),
            ("secret", "base_url"),
            ("setting", "token"),
            ("domain", "other.example.com"),
        ] {
            let mut pkg = package();
            let rule = &mut pkg.http_credentials.as_mut().unwrap()[0];
            match field {
                "secret" => rule.bearer_secret = Some(key.into()),
                "setting" => {
                    rule.destination = CredentialDestination::BaseUrl {
                        setting: key.into(),
                    }
                }
                _ => rule.destination = CredentialDestination::Domain { domain: key.into() },
            }
            assert!(!valid_rules(&pkg));
        }
        let mut pkg = package();
        let duplicate = pkg.http_credentials.as_ref().unwrap()[0].clone();
        pkg.http_credentials.as_mut().unwrap().push(duplicate);
        assert!(!valid_rules(&pkg));
        pkg.http_credentials = Some(vec![pkg.http_credentials.as_ref().unwrap()[0].clone(); 65]);
        assert!(!valid_rules(&pkg));
        for url in [
            "http://custom.example.com",
            "https://custom.example.com:0",
            "https://user:secret@custom.example.com",
            "https://localhost",
            "https://127.0.0.1",
            "https://10.0.0.1",
            "https://custom.example.com/api?token=x",
            "https://custom.example.com/api?",
            "https://custom.example.com/api#fragment",
            "https://custom.example.com/api%2fother",
            "https://custom.example.com/api\\other",
            "https://custom.example.com//",
            "https://custom.example.com/api//",
            "https://custom.example.com/api//tasks",
            "https://custom.example.com/a/../api",
            "https://custom.example.com/a/./api",
            "https://custom.example.com/%2e%2e/api",
            "https://custom.example.com/api/..",
        ] {
            let config = BTreeMap::from([("base_url".into(), json!(url))]);
            assert!(
                !invalid_base_url_settings(&package(), &config).is_empty(),
                "{url}"
            );
            assert!(resolve(&package(), &config).is_err());
        }
    }

    #[test]
    fn resolved_collisions_are_rejected_and_host_changes_require_new_approval() {
        let mut pkg = package();
        pkg.http_credentials.as_mut().unwrap()[0].path_prefix = "/".into();
        let config = BTreeMap::from([("base_url".into(), json!("https://fixed.example.com"))]);
        assert!(resolve(&pkg, &config).is_err());
        let previous = BTreeMap::from([("base_url".into(), json!("https://old.example.com/api"))]);
        assert_eq!(
            changed_base_url_hosts(&pkg, &previous, &config),
            vec!["fixed.example.com"]
        );
        let same_host = BTreeMap::from([("base_url".into(), json!("https://old.example.com/v2"))]);
        assert!(changed_base_url_hosts(&pkg, &previous, &same_host).is_empty());
    }

    #[test]
    fn paths_match_segments_and_reject_ambiguous_server_routing() {
        for path in ["/tasks", "/tasks/", "/tasks/123"] {
            assert!(path_matches(path, "/tasks"));
        }
        for path in ["/tasks-other", "/Tasks", "/other/tasks"] {
            assert!(!path_matches(path, "/tasks"));
        }
        assert_eq!(
            credential_path("/%74asks/%E4%B8%AD").unwrap(),
            "/tasks/%E4%B8%AD"
        );
        for path in [
            "/tasks%2fpublic",
            "/tasks%5cpublic",
            "/%2574asks",
            "/tasks%3fpublic",
            "/tasks%23public",
            "/tasks%00",
            "/tasks;public",
            "/tasks%3bpublic",
            "/tasks//public",
            "/tasks%",
            "/tasks%GG",
        ] {
            assert!(credential_path(path).is_err(), "{path}");
        }
    }
}
