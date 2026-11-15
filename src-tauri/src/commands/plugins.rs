use crate::{
    errors::{CommandError, PluginPackageIssue},
    plugins::{
        manager::{self, PluginManager},
        storage,
        types::*,
    },
    state::SharedAppData,
};
use std::{collections::BTreeMap, sync::atomic::Ordering};
use tauri::{Emitter, Manager};
fn busy(app: &tauri::AppHandle, id: &str) -> Result<(), CommandError> {
    if app
        .state::<PluginManager>()
        .active
        .lock()
        .map_err(CommandError::internal)?
        .contains_key(id)
    {
        Err(CommandError::InvalidArgument)
    } else {
        Ok(())
    }
}
fn valid_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 64
        && code
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}
fn valid_id(id: &str) -> bool {
    let reserved = id.to_ascii_lowercase();
    !id.is_empty()
        && id.len() <= 100
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        && !matches!(reserved.as_str(), "con" | "prn" | "aux" | "nul")
        && !(reserved.len() == 4
            && (reserved.starts_with("com") || reserved.starts_with("lpt"))
            && matches!(reserved.as_bytes()[3], b'1'..=b'9'))
}
fn unique_values<'a>(values: impl IntoIterator<Item = &'a str>) -> bool {
    let mut seen = std::collections::HashSet::new();
    values
        .into_iter()
        .all(|value| !value.is_empty() && seen.insert(value))
}
pub(crate) fn valid_domains(domains: &[String]) -> bool {
    domains.len() <= 16
        && unique_values(
            domains
                .iter()
                .map(|domain| crate::plugins::http::canonical_domain(domain)),
        )
        && domains.iter().all(|domain| {
            let (host, valid_port) = match domain.split_once(':') {
                Some((host, port)) => (
                    host,
                    port.parse::<u16>()
                        .is_ok_and(|value| value > 0 && value.to_string() == port),
                ),
                None => (domain.as_str(), true),
            };
            valid_port
                && host.len() <= 253
                && host.split('.').all(|label| {
                    !label.is_empty()
                        && label.len() <= 63
                        && label
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                        && !label.starts_with('-')
                        && !label.ends_with('-')
                })
        })
}
pub(crate) fn valid_instance_domains(package: &PluginPackage, domains: &[String]) -> bool {
    valid_domains(domains)
        && domains.iter().all(|domain| {
            (crate::plugins::http::contains_domain(&package.domains, domain)
                || package.allow_custom_domains)
                && crate::link_preview::safety::safe_url(&format!("https://{domain}")).is_some()
        })
}
pub(crate) mod instance;
pub(crate) mod package;

use instance::*;
#[cfg(test)]
use package::*;

#[cfg(test)]
mod tests;
