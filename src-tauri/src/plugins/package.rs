use crate::{
    errors::CommandError,
    link_preview::safety::{public_addresses, safe_url},
};
use std::io::{Cursor, Read};

const ARCHIVE_LIMIT: usize = 20 * 1024 * 1024;
pub(super) const COMPONENT_LIMIT: u64 = 16 * 1024 * 1024;

pub fn unpack(bytes: Vec<u8>) -> Result<(Vec<u8>, Vec<u8>), CommandError> {
    if bytes.len() > ARCHIVE_LIMIT {
        return Err(CommandError::InvalidArgument);
    }
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| CommandError::InvalidArgument)?;
    if archive.len() > 256 {
        return Err(CommandError::InvalidArgument);
    }
    let mut manifest = None;
    let mut names = std::collections::HashSet::new();
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|_| CommandError::InvalidArgument)?;
        if file.enclosed_name().is_none()
            || file.name().contains('\\')
            || file.is_symlink()
            || !names.insert(file.name().to_owned())
        {
            return Err(CommandError::InvalidArgument);
        }
        if !file.is_dir() && file.name().rsplit('/').next() == Some("manifest.json") {
            if manifest.is_some() {
                return Err(CommandError::InvalidArgument);
            }
            manifest = Some(file.name().to_owned());
        }
    }
    let manifest = manifest.ok_or(CommandError::InvalidArgument)?;
    let component = format!(
        "{}component.wasm",
        manifest.strip_suffix("manifest.json").unwrap()
    );
    let mut read = |name: &str, limit: u64| -> Result<Vec<u8>, CommandError> {
        let file = archive
            .by_name(name)
            .map_err(|_| CommandError::InvalidArgument)?;
        if file.size() > limit || file.is_dir() {
            return Err(CommandError::InvalidArgument);
        }
        let mut bytes = Vec::new();
        file.take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| CommandError::InvalidArgument)?;
        if bytes.len() as u64 > limit {
            return Err(CommandError::InvalidArgument);
        }
        Ok(bytes)
    };
    Ok((read(&manifest, 65536)?, read(&component, COMPONENT_LIMIT)?))
}

pub async fn download(value: &str) -> Result<Vec<u8>, CommandError> {
    tokio::time::timeout(std::time::Duration::from_secs(60), download_inner(value))
        .await
        .map_err(|_| CommandError::InternalError)?
}

async fn download_inner(value: &str) -> Result<Vec<u8>, CommandError> {
    let mut url = safe_url(value).ok_or(CommandError::InvalidArgument)?;
    for _ in 0..6 {
        if url.scheme() != "https" || url.port_or_known_default() != Some(443) {
            return Err(CommandError::InvalidArgument);
        }
        let host = url.host_str().ok_or(CommandError::InvalidArgument)?;
        let addresses: Vec<_> = tokio::net::lookup_host((host, 443))
            .await
            .map_err(CommandError::internal)?
            .collect();
        if !public_addresses(&addresses) {
            return Err(CommandError::InvalidArgument);
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
            .map_err(CommandError::internal)?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or(CommandError::InvalidArgument)?;
            url = safe_url(
                url.join(location)
                    .map_err(|_| CommandError::InvalidArgument)?
                    .as_str(),
            )
            .ok_or(CommandError::InvalidArgument)?;
            continue;
        }
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|n| n > ARCHIVE_LIMIT as u64)
        {
            return Err(CommandError::InvalidArgument);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(CommandError::internal)? {
            if bytes.len() + chunk.len() > ARCHIVE_LIMIT {
                return Err(CommandError::InvalidArgument);
            }
            bytes.extend_from_slice(&chunk);
        }
        return Ok(bytes);
    }
    Err(CommandError::InvalidArgument)
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
        for files in [
            vec![("manifest.json", &b"{}"[..])],
            vec![("../manifest.json", &b"{}"[..])],
            vec![
                ("manifest.json", &b"{}"[..]),
                ("other/manifest.json", &b"{}"[..]),
            ],
        ] {
            assert!(unpack(zip(&files)).is_err());
        }
        assert!(unpack(vec![0; 8]).is_err());
        assert!(unpack(zip(&[
            ("manifest.json", &vec![0; 65537]),
            ("component.wasm", b"wasm")
        ]))
        .is_err());
    }
    #[tokio::test]
    async fn unsafe_downloads_are_rejected() {
        for url in [
            "http://example.com/package.zip",
            "https://127.0.0.1/package.zip",
            "https://user:pass@example.com/package.zip",
            "https://example.com:444/package.zip",
        ] {
            assert!(download(url).await.is_err());
        }
    }
}
