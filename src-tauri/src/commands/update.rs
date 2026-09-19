use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

const UPDATE_CONFIG: &str = include_str!("../../../src/config/update.json");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateConfig {
    github_owner: String,
    github_repository: String,
    include_prereleases: bool,
    request_timeout_ms: u64,
}

#[derive(Deserialize, Clone)]
struct GitHubRelease {
    tag_name: Option<String>,
    html_url: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
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

#[tauri::command]
pub async fn check_for_update(current_version: String) -> Result<Option<UpdateInfo>, String> {
    let config: UpdateConfig =
        serde_json::from_str(UPDATE_CONFIG).map_err(|error| error.to_string())?;
    if !is_valid_github_segment(&config.github_owner)
        || !is_valid_github_segment(&config.github_repository)
    {
        return Err("Invalid GitHub repository in update configuration".to_string());
    }

    let url = format!(
        "https://api.github.com/repos/{}/{}/releases?per_page=20",
        config.github_owner, config.github_repository,
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(config.request_timeout_ms))
        .user_agent("Cardbe update checker")
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|error| format!("Could not contact GitHub: {error}"))?;

    if !response.status().is_success() {
        return Err(format!("GitHub returned {}", response.status()));
    }
    let releases: Vec<GitHubRelease> = response
        .json()
        .await
        .map_err(|error| format!("GitHub returned invalid release data: {error}"))?;
    Ok(find_update(
        &current_version,
        &releases,
        config.include_prereleases,
    ))
}

fn find_update(
    current_version: &str,
    releases: &[GitHubRelease],
    include_prereleases: bool,
) -> Option<UpdateInfo> {
    let current = parse_version(current_version)?;
    let release = releases
        .iter()
        .filter(|release| !release.draft && (include_prereleases || !release.prerelease))
        .filter_map(|release| {
            let tag_name = release.tag_name.as_deref()?;
            let html_url = release.html_url.as_deref()?;
            let version = parse_version(tag_name)?;
            Some((release, version, html_url))
        })
        .max_by(|(_, left, _), (_, right, _)| compare_versions(left, right))?;

    (compare_versions(&release.1, &current) == Ordering::Greater).then(|| UpdateInfo {
        version: release
            .0
            .tag_name
            .as_ref()
            .unwrap()
            .trim_start_matches('v')
            .to_string(),
        url: release.2.to_string(),
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

fn is_allowed_external_url(value: &str) -> bool {
    tauri::Url::parse(value)
        .map(|url| matches!(url.scheme(), "http" | "https"))
        .unwrap_or(false)
}

#[tauri::command]
pub fn open_external_url(app: AppHandle, url: String) -> Result<(), String> {
    if !is_allowed_external_url(&url) {
        return Err("Only HTTP and HTTPS links can be opened".to_string());
    }

    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn open_latest_release(app: AppHandle) -> Result<(), String> {
    let config: UpdateConfig =
        serde_json::from_str(UPDATE_CONFIG).map_err(|error| error.to_string())?;
    if !is_valid_github_segment(&config.github_owner)
        || !is_valid_github_segment(&config.github_repository)
    {
        return Err("Invalid GitHub repository in update configuration".to_string());
    }

    let url = format!(
        "https://github.com/{}/{}/releases/latest",
        config.github_owner, config.github_repository,
    );
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{find_update, is_allowed_external_url, is_valid_github_segment, GitHubRelease};

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
    fn allows_only_http_and_https_external_urls() {
        assert!(is_allowed_external_url("https://example.com/docs"));
        assert!(is_allowed_external_url("http://localhost:3000/path"));
        assert!(!is_allowed_external_url("javascript:alert(1)"));
        assert!(!is_allowed_external_url("file:///etc/passwd"));
        assert!(!is_allowed_external_url("not a url"));
    }

    #[test]
    fn selects_the_newest_eligible_release() {
        let releases = vec![
            GitHubRelease {
                tag_name: Some("v0.4.0-beta.1".into()),
                html_url: Some("https://example.com/beta".into()),
                draft: false,
                prerelease: true,
            },
            GitHubRelease {
                tag_name: Some("v0.3.0".into()),
                html_url: Some("https://example.com/stable".into()),
                draft: false,
                prerelease: false,
            },
            GitHubRelease {
                tag_name: Some("v9.0.0".into()),
                html_url: Some("https://example.com/draft".into()),
                draft: true,
                prerelease: false,
            },
        ];

        let update = find_update("0.2.2", &releases, true).unwrap();
        assert_eq!(update.version, "0.4.0-beta.1");
        assert_eq!(update.url, "https://example.com/beta");
    }

    #[test]
    fn ignores_prereleases_when_disabled_and_older_releases() {
        let releases = vec![
            GitHubRelease {
                tag_name: Some("v0.4.0-beta.1".into()),
                html_url: Some("https://example.com/beta".into()),
                draft: false,
                prerelease: true,
            },
            GitHubRelease {
                tag_name: Some("v0.3.0".into()),
                html_url: Some("https://example.com/stable".into()),
                draft: false,
                prerelease: false,
            },
        ];

        assert!(find_update("0.3.0", &releases, false).is_none());
        assert!(find_update("invalid", &releases, true).is_none());
    }
}
