use crate::errors::CommandError;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::time::Duration;

const UPDATE_CONFIG: &str = include_str!("../../../src/config/update.json");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateConfig {
    github_owner: String,
    github_repository: String,
    request_timeout_ms: u64,
}

#[derive(Serialize)]
pub struct UpdateInfo {
    version: String,
    url: String,
}

struct ParsedVersion {
    core: [u64; 3],
    prerelease: Vec<String>,
}

/// Resolves the latest release through the github.com redirect instead of the
/// REST API, whose unauthenticated limit (60 requests per hour) is shared by
/// every user behind the same public IP.
#[tauri::command]
pub async fn check_for_update(current_version: String) -> Result<Option<UpdateInfo>, CommandError> {
    let config: UpdateConfig =
        serde_json::from_str(UPDATE_CONFIG).map_err(CommandError::internal)?;
    if !is_valid_github_segment(&config.github_owner)
        || !is_valid_github_segment(&config.github_repository)
    {
        return Err(CommandError::internal(
            "Invalid GitHub repository in update configuration",
        ));
    }

    let url = format!(
        "https://github.com/{}/{}/releases/latest",
        config.github_owner, config.github_repository,
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(config.request_timeout_ms))
        .user_agent(concat!("Cardbe/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(CommandError::internal)?;
    let response = client.head(url).send().await.map_err(|error| {
        let kind = if error.is_timeout() {
            "timeout"
        } else if error.is_connect() {
            "connect"
        } else {
            "request"
        };
        log_failure(&format!("{kind} error: {error}"));
        CommandError::UpdateRequestFailed
    })?;

    let status = response.status();
    if !status.is_redirection() {
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("-")
                .to_owned()
        };
        log_failure(&format!(
            "unexpected status {status} (retry-after: {}, x-ratelimit-remaining: {}, x-ratelimit-reset: {})",
            header("retry-after"),
            header("x-ratelimit-remaining"),
            header("x-ratelimit-reset"),
        ));
        return Err(CommandError::UpdateRequestFailed);
    }

    let location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| response.url().join(value).ok());
    let Some(location) = location else {
        log_failure(&format!("redirect {status} without a valid location"));
        return Err(CommandError::UpdateReleaseDataInvalid);
    };
    match release_tag(&location) {
        Some(tag) => Ok(find_update(&current_version, tag, location.as_str())),
        // Repositories without a published release redirect to the release list.
        None if location.path().ends_with("/releases") => Ok(None),
        None => {
            log_failure(&format!("unexpected redirect location {location}"));
            Err(CommandError::UpdateReleaseDataInvalid)
        }
    }
}

fn log_failure(detail: &str) {
    let detail = crate::commands::diagnostics::redact_diagnostic_logs(detail.to_owned());
    log::warn!(target: "update", "update check failed: {detail}");
}

/// Extracts the tag from `https://github.com/{owner}/{repo}/releases/tag/{tag}`.
fn release_tag(location: &reqwest::Url) -> Option<&str> {
    if location.scheme() != "https" || location.host_str() != Some("github.com") {
        return None;
    }
    match location.path_segments()?.collect::<Vec<_>>()[..] {
        [_, _, "releases", "tag", tag] if !tag.is_empty() => Some(tag),
        _ => None,
    }
}

fn find_update(current_version: &str, tag: &str, url: &str) -> Option<UpdateInfo> {
    let current = parse_version(current_version)?;
    let Some(latest) = parse_version(tag) else {
        log_failure(&format!("unrecognized release tag {tag}"));
        return None;
    };
    (compare_versions(&latest, &current) == Ordering::Greater).then(|| UpdateInfo {
        version: tag.trim_start_matches('v').to_string(),
        url: url.to_string(),
    })
}

fn parse_version(value: &str) -> Option<ParsedVersion> {
    let value = value.trim().strip_prefix('v').unwrap_or(value.trim());
    let (value, _) = value.split_once('+').unwrap_or((value, ""));
    let (core, prerelease) = value.split_once('-').unwrap_or((value, ""));
    let mut parts = core.split('.').map(str::parse::<u64>);
    let core = [
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    ];
    if parts.next().is_some() || (prerelease.is_empty() && value.ends_with('-')) {
        return None;
    }
    let prerelease = if prerelease.is_empty() {
        Vec::new()
    } else {
        let parts = prerelease
            .split('.')
            .map(str::to_string)
            .collect::<Vec<_>>();
        if parts.iter().any(|part| {
            part.is_empty() || !part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        }) {
            return None;
        }
        parts
    };
    Some(ParsedVersion { core, prerelease })
}

fn compare_versions(left: &ParsedVersion, right: &ParsedVersion) -> Ordering {
    left.core.cmp(&right.core).then_with(|| {
        if left.prerelease.is_empty() || right.prerelease.is_empty() {
            return left.prerelease.is_empty().cmp(&right.prerelease.is_empty());
        }
        for (left_part, right_part) in left.prerelease.iter().zip(&right.prerelease) {
            if left_part == right_part {
                continue;
            }
            let left_number = left_part.parse::<u64>();
            let right_number = right_part.parse::<u64>();
            return match (left_number, right_number) {
                (Ok(left), Ok(right)) => left.cmp(&right),
                (Ok(_), Err(_)) => Ordering::Less,
                (Err(_), Ok(_)) => Ordering::Greater,
                (Err(_), Err(_)) => left_part.cmp(right_part),
            };
        }
        left.prerelease.len().cmp(&right.prerelease.len())
    })
}

fn is_valid_github_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
}

#[cfg(test)]
mod tests {
    use super::{find_update, is_valid_github_segment, release_tag};

    #[test]
    fn accepts_github_owner_and_repository_names() {
        assert!(is_valid_github_segment("valid-owner"));
        assert!(is_valid_github_segment("valid.repository_1"));
    }

    #[test]
    fn rejects_values_that_could_change_the_release_url() {
        assert!(!is_valid_github_segment(""));
        assert!(!is_valid_github_segment("owner/repository"));
        assert!(!is_valid_github_segment("github.com?next="));
    }

    #[test]
    fn extracts_the_tag_from_the_latest_release_redirect() {
        let url = |value: &str| reqwest::Url::parse(value).unwrap();
        assert_eq!(
            release_tag(&url("https://github.com/owner/repo/releases/tag/v0.4.0")),
            Some("v0.4.0")
        );
        assert_eq!(
            release_tag(&url("https://github.com/owner/repo/releases")),
            None
        );
        assert_eq!(
            release_tag(&url("https://example.com/owner/repo/releases/tag/v0.4.0")),
            None
        );
        assert_eq!(
            release_tag(&url("http://github.com/owner/repo/releases/tag/v0.4.0")),
            None
        );
        assert_eq!(
            release_tag(&url("https://github.com/owner/repo/releases/tag/")),
            None
        );
    }

    #[test]
    fn reports_only_newer_releases() {
        let update = find_update("0.2.2", "v0.4.0-beta.1", "https://example.com/beta").unwrap();
        assert_eq!(update.version, "0.4.0-beta.1");
        assert_eq!(update.url, "https://example.com/beta");

        let update = find_update("0.4.0-beta.1", "v0.4.0", "https://example.com/stable").unwrap();
        assert_eq!(update.version, "0.4.0");

        assert!(find_update("0.3.0", "v0.3.0", "https://example.com").is_none());
        assert!(find_update("0.3.1", "v0.3.0", "https://example.com").is_none());
        assert!(find_update("invalid", "v0.3.0", "https://example.com").is_none());
        assert!(find_update("0.3.0", "nightly", "https://example.com").is_none());
    }
}
