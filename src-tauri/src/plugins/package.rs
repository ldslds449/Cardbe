use crate::{
    errors::{CommandError, PluginPackageIssue},
    link_preview::safety::{public_addresses, safe_url},
};
use std::io::{Cursor, Read};

pub(crate) const ARCHIVE_LIMIT: usize = 80 * 1024 * 1024;
// Allow bundled language runtimes; execution memory is limited separately.
pub(crate) const COMPONENT_LIMIT: u64 = 64 * 1024 * 1024;

fn invalid(reason: PluginPackageIssue) -> CommandError {
    CommandError::plugin_package(reason)
}

/// Network failures are expected; keep redacted details in logs only.
fn download_failed(source: impl std::fmt::Display) -> CommandError {
    let detail = crate::commands::diagnostics::redact_diagnostic_logs(source.to_string());
    log::warn!(target: "plugins", "Plugin package download failed: {detail}");
    invalid(PluginPackageIssue::DownloadFailed)
}

pub fn unpack(bytes: Vec<u8>) -> Result<(Vec<u8>, Vec<u8>), CommandError> {
    if bytes.len() > ARCHIVE_LIMIT {
        return Err(invalid(PluginPackageIssue::ArchiveTooLarge));
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| invalid(PluginPackageIssue::ArchiveUnreadable))?;
    if archive.len() > 256 {
        return Err(invalid(PluginPackageIssue::ArchiveTooManyEntries));
    }
    let mut manifest = None;
    let mut names = std::collections::HashSet::new();
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|_| invalid(PluginPackageIssue::ArchiveUnreadable))?;
        if file.enclosed_name().is_none()
            || file.name().contains('\\')
            || file.is_symlink()
            || !names.insert(file.name().to_owned())
        {
            return Err(invalid(PluginPackageIssue::UnsafePath));
        }
        if !file.is_dir() && file.name().rsplit('/').next() == Some("manifest.json") {
            if manifest.is_some() {
                return Err(invalid(PluginPackageIssue::ManifestDuplicate));
            }
            manifest = Some(file.name().to_owned());
        }
    }
    let manifest = manifest.ok_or(invalid(PluginPackageIssue::ManifestMissing))?;
    let component = format!(
        "{}component.wasm",
        manifest.strip_suffix("manifest.json").unwrap()
    );
    let mut read = |name: &str,
                    limit: u64,
                    missing: PluginPackageIssue,
                    too_large: PluginPackageIssue|
     -> Result<Vec<u8>, CommandError> {
        let file = archive.by_name(name).map_err(|_| invalid(missing))?;
        if file.is_dir() {
            return Err(invalid(missing));
        }
        if file.size() > limit {
            return Err(invalid(too_large));
        }
        let mut bytes = Vec::new();
        file.take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid(PluginPackageIssue::ArchiveUnreadable))?;
        if bytes.len() as u64 > limit {
            return Err(invalid(too_large));
        }
        Ok(bytes)
    };
    Ok((
        read(
            &manifest,
            65536,
            PluginPackageIssue::ManifestMissing,
            PluginPackageIssue::ManifestTooLarge,
        )?,
        read(
            &component,
            COMPONENT_LIMIT,
            PluginPackageIssue::ComponentMissing,
            PluginPackageIssue::ComponentTooLarge,
        )?,
    ))
}

pub async fn download(value: &str) -> Result<Vec<u8>, CommandError> {
    tokio::time::timeout(std::time::Duration::from_secs(60), download_inner(value))
        .await
        .map_err(|_| download_failed("timed out"))?
}

async fn download_inner(value: &str) -> Result<Vec<u8>, CommandError> {
    let mut url = safe_url(value).ok_or(invalid(PluginPackageIssue::DownloadUrlInvalid))?;
    for _ in 0..6 {
        if url.scheme() != "https" || url.port_or_known_default() != Some(443) {
            return Err(invalid(PluginPackageIssue::DownloadUrlInvalid));
        }
        let host = url
            .host_str()
            .ok_or(invalid(PluginPackageIssue::DownloadUrlInvalid))?;
        let addresses: Vec<_> = tokio::net::lookup_host((host, 443))
            .await
            .map_err(download_failed)?
            .collect();
        if !public_addresses(&addresses) {
            return Err(invalid(PluginPackageIssue::DownloadUrlInvalid));
        }
        // Installation redirects are validated and DNS-pinned at every hop, independently of guest HTTP.
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(30))
            .resolve_to_addrs(host, &addresses)
            .user_agent("Cardbe-PluginInstaller/1.0")
            .build()
            .map_err(CommandError::internal)?;
        let mut response = client
            .get(url.clone())
            .send()
            .await
            .map_err(download_failed)?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| download_failed("redirect without location"))?;
            url = safe_url(
                url.join(location)
                    .map_err(|_| invalid(PluginPackageIssue::DownloadUrlInvalid))?
                    .as_str(),
            )
            .ok_or(invalid(PluginPackageIssue::DownloadUrlInvalid))?;
            continue;
        }
        if !response.status().is_success() {
            return Err(download_failed(format!("HTTP {}", response.status())));
        }
        if response
            .content_length()
            .is_some_and(|n| n > ARCHIVE_LIMIT as u64)
        {
            return Err(invalid(PluginPackageIssue::ArchiveTooLarge));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(download_failed)? {
            if bytes.len() + chunk.len() > ARCHIVE_LIMIT {
                return Err(invalid(PluginPackageIssue::ArchiveTooLarge));
            }
            bytes.extend_from_slice(&chunk);
        }
        return Ok(bytes);
    }
    Err(download_failed("too many redirects"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in files {
            archive
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            archive.write_all(bytes).unwrap();
        }
        archive.finish().unwrap().into_inner()
    }
    #[test]
    fn packages_require_one_manifest_and_its_component() {
        for prefix in ["", "release/"] {
            assert_eq!(
                unpack(zip(&[
                    (&format!("{prefix}manifest.json"), b"{}"),
                    (&format!("{prefix}component.wasm"), b"wasm")
                ]))
                .unwrap(),
                (b"{}".to_vec(), b"wasm".to_vec())
            );
        }
        for (files, issue) in [
            (
                vec![("manifest.json", &b"{}"[..])],
                PluginPackageIssue::ComponentMissing,
            ),
            (
                vec![("component.wasm", &b"wasm"[..])],
                PluginPackageIssue::ManifestMissing,
            ),
            (
                vec![("../manifest.json", &b"{}"[..])],
                PluginPackageIssue::UnsafePath,
            ),
            (
                vec![
                    ("manifest.json", &b"{}"[..]),
                    ("other/manifest.json", &b"{}"[..]),
                ],
                PluginPackageIssue::ManifestDuplicate,
            ),
        ] {
            assert_issue(unpack(zip(&files)), issue);
        }
        assert_issue(unpack(vec![0; 8]), PluginPackageIssue::ArchiveUnreadable);
        assert_issue(
            unpack(zip(&[
                ("manifest.json", &vec![0; 65537]),
                ("component.wasm", b"wasm"),
            ])),
            PluginPackageIssue::ManifestTooLarge,
        );
        let entries: Vec<_> = (0..257).map(|i| format!("file-{i}")).collect();
        assert_issue(
            unpack(zip(&entries
                .iter()
                .map(|name| (name.as_str(), &b""[..]))
                .collect::<Vec<_>>())),
            PluginPackageIssue::ArchiveTooManyEntries,
        );
    }
    fn assert_issue<T: std::fmt::Debug>(
        result: Result<T, CommandError>,
        issue: PluginPackageIssue,
    ) {
        match result {
            Err(CommandError::PluginPackageInvalid { reason, .. }) => assert_eq!(reason, issue),
            other => panic!("expected {issue:?}, got {other:?}"),
        }
    }
    #[test]
    fn component_size_boundaries() {
        let mut component = vec![0; COMPONENT_LIMIT as usize];
        assert_eq!(
            unpack(zip(&[
                ("manifest.json", b"{}"),
                ("component.wasm", &component)
            ]))
            .unwrap()
            .1
            .len(),
            COMPONENT_LIMIT as usize
        );
        component.push(0);
        assert_issue(
            unpack(zip(&[
                ("manifest.json", b"{}"),
                ("component.wasm", &component),
            ])),
            PluginPackageIssue::ComponentTooLarge,
        );
        assert_issue(
            unpack(vec![0; ARCHIVE_LIMIT + 1]),
            PluginPackageIssue::ArchiveTooLarge,
        );
    }
    #[tokio::test]
    async fn unsafe_downloads_are_rejected() {
        for url in [
            "http://example.com/package.zip",
            "https://127.0.0.1/package.zip",
            "https://user:pass@example.com/package.zip",
            "https://example.com:444/package.zip",
        ] {
            assert_issue(download(url).await, PluginPackageIssue::DownloadUrlInvalid);
        }
    }
}
