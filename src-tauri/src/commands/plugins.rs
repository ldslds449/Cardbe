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
#[tauri::command]
pub fn get_plugin_state(app: tauri::AppHandle) -> Result<PluginState, CommandError> {
    manager::snapshot(&app).map_err(CommandError::from)
}
#[tauri::command]
pub async fn install_plugin_package(
    app: tauri::AppHandle,
    path: Option<String>,
    url: Option<String>,
) -> Result<PluginPackageSelection, CommandError> {
    let archive = match (path.as_ref(), url.as_ref()) {
        (None, Some(url)) => Some(crate::plugins::package::download(url).await?),
        (Some(_), None) => None,
        _ => return Err(CommandError::InvalidArgument),
    };
    tauri::async_runtime::spawn_blocking(move || {
        let (bytes, component) = if let Some(archive) = archive {
            crate::plugins::package::unpack(archive)?
        } else {
            read_local_package(path.ok_or(CommandError::InvalidArgument)?)?
        };
        select_package(app, bytes, component)
    })
    .await
    .map_err(CommandError::internal)?
}

fn read_local_package(path: String) -> Result<(Vec<u8>, Vec<u8>), CommandError> {
    let selected = std::path::PathBuf::from(path)
        .canonicalize()
        .map_err(CommandError::internal)?;
    if selected
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
    {
        if std::fs::metadata(&selected)
            .map_err(CommandError::internal)?
            .len()
            > crate::plugins::package::ARCHIVE_LIMIT as u64
        {
            return Err(CommandError::plugin_package(
                PluginPackageIssue::ArchiveTooLarge,
            ));
        }
        return crate::plugins::package::unpack(
            std::fs::read(selected).map_err(CommandError::internal)?,
        );
    }
    let root = if selected.is_file() {
        selected
            .parent()
            .ok_or(CommandError::InvalidArgument)?
            .to_owned()
    } else {
        selected
    };
    let manifest = root.join("manifest.json");
    let component = root.join("component.wasm");
    let contained = |path: &std::path::Path, missing| match path.canonicalize() {
        Ok(path) if path.parent() == Some(root.as_path()) => Ok(()),
        Ok(_) => Err(CommandError::plugin_package(PluginPackageIssue::UnsafePath)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(CommandError::plugin_package(missing))
        }
        Err(error) => Err(CommandError::internal(error)),
    };
    contained(&manifest, PluginPackageIssue::ManifestMissing)?;
    contained(&component, PluginPackageIssue::ComponentMissing)?;
    if std::fs::metadata(&manifest)
        .map_err(CommandError::internal)?
        .len()
        > 65536
    {
        return Err(CommandError::plugin_package(
            PluginPackageIssue::ManifestTooLarge,
        ));
    }
    if std::fs::metadata(&component)
        .map_err(CommandError::internal)?
        .len()
        > crate::plugins::package::COMPONENT_LIMIT
    {
        return Err(CommandError::plugin_package(
            PluginPackageIssue::ComponentTooLarge,
        ));
    }
    let bytes = std::fs::read(&manifest).map_err(CommandError::internal)?;
    Ok((
        bytes,
        std::fs::read(component).map_err(CommandError::internal)?,
    ))
}
// shortcut: only stable x.y.z releases are updatable; add prerelease ordering when needed.
fn release_version(value: &str) -> Option<[u64; 3]> {
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts.iter().any(|part| {
            part.is_empty()
                || !part.bytes().all(|b| b.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
        })
    {
        return None;
    }
    Some([
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ])
}
/// 1.1.0 changed the task record, so draft 1.0.0 components are not supported.
const SUPPORTED_API_VERSIONS: [&str; 3] = ["1.1.0", "1.2.0", "1.3.0"];
fn validate_update(old: &PluginPackage, new: &PluginPackage) -> Result<(), CommandError> {
    if old.id != new.id {
        return Err(CommandError::InvalidArgument);
    }
    if !SUPPORTED_API_VERSIONS.contains(&new.api_version.as_str()) {
        return Err(unsupported_api_version(new));
    }
    if old.storage_schema_version != new.storage_schema_version {
        return Err(CommandError::plugin_package(
            PluginPackageIssue::UnsupportedStorageSchema,
        ));
    }
    if !crate::plugins::credentials::valid_rules(new) {
        return Err(CommandError::plugin_package(
            PluginPackageIssue::InvalidCredentialRules,
        ));
    }
    let old_version = release_version(&old.version).ok_or(CommandError::InvalidArgument)?;
    let new_version = release_version(&new.version).ok_or(CommandError::InvalidArgument)?;
    if new_version <= old_version {
        return Err(CommandError::PluginUpdateNotNewer);
    }
    Ok(())
}
fn update_requires_review(old: &PluginPackage, new: &PluginPackage) -> bool {
    old.manifest_version != new.manifest_version
        || old.domains != new.domains
        || old.allow_custom_domains != new.allow_custom_domains
        || old.http_credentials != new.http_credentials
        || serde_json::to_value(&old.settings).ok() != serde_json::to_value(&new.settings).ok()
}
fn credentials_changed(old: &PluginPackage, new: &PluginPackage) -> bool {
    old.settings
        .iter()
        .filter(|field| field.kind == "secret")
        .any(|field| {
            old.domains != new.domains
                || old.http_credentials != new.http_credentials
                || !new
                    .settings
                    .iter()
                    .any(|next| next.kind == "secret" && next.key == field.key)
        })
}
fn update_candidate(
    connection: &rusqlite::Connection,
    package: &PluginPackage,
) -> Result<Option<PluginPackage>, CommandError> {
    let old = storage::list::<PluginPackage>(connection, "package")?
        .into_iter()
        .find(|p| p.id == package.id);
    if let Some(old) = &old {
        validate_update(old, package)?;
    }
    Ok(old)
}
fn validate_component(component: &[u8]) -> Result<(), CommandError> {
    crate::plugins::runtime::validate_component(component)
        .map_err(|_| CommandError::plugin_package(PluginPackageIssue::InvalidComponent))
}
fn select_package(
    app: tauri::AppHandle,
    manifest: Vec<u8>,
    component: Vec<u8>,
) -> Result<PluginPackageSelection, CommandError> {
    let package = parse_package(&manifest)?;
    let old = {
        let state = app.state::<SharedAppData>();
        let guard = state.lock().map_err(CommandError::internal)?;
        update_candidate(guard.database.plugin_connection(), &package)?
    };
    let Some(old) = old else {
        return install_package(app, manifest, component)
            .map(|package| PluginPackageSelection::Installed { package });
    };
    validate_component(&component)?;
    let state = app.state::<SharedAppData>();
    let guard = state.lock().map_err(CommandError::internal)?;
    let current = storage::list::<PluginPackage>(guard.database.plugin_connection(), "package")?
        .into_iter()
        .find(|p| p.id == old.id)
        .ok_or(CommandError::PluginUpdateStale)?;
    if serde_json::to_value(&current).map_err(CommandError::internal)?
        != serde_json::to_value(&old).map_err(CommandError::internal)?
    {
        return Err(CommandError::PluginUpdateStale);
    }
    let preview = PluginUpdatePreview {
        token: id(),
        current_version: old.version.clone(),
        requires_review: update_requires_review(&old, &package),
        package: package.clone(),
    };
    *app.state::<PluginManager>()
        .update
        .lock()
        .map_err(CommandError::internal)? = Some(PreparedPluginUpdate {
        token: preview.token.clone(),
        previous_version: old.version.clone(),
        package,
        manifest,
        component,
    });
    Ok(PluginPackageSelection::Update {
        current_package: old,
        preview: Box::new(preview),
    })
}
#[tauri::command]
pub fn discard_plugin_update(app: tauri::AppHandle, token: String) -> Result<(), CommandError> {
    let manager = app.state::<PluginManager>();
    let mut update = manager.update.lock().map_err(CommandError::internal)?;
    if update.as_ref().is_some_and(|u| u.token == token) {
        *update = None;
    }
    Ok(())
}
fn clean_revision(directory: &std::path::Path) {
    for file in ["manifest.json", "component.wasm"] {
        let _ = std::fs::remove_file(directory.join(file));
    }
    if std::fs::remove_dir(directory).is_err() {
        log::warn!(target: "plugins", "Could not clean up inactive plugin revision");
    }
}
fn update_locked(
    connection: &rusqlite::Connection,
    root: &std::path::Path,
    update: &PreparedPluginUpdate,
    active: &std::collections::HashMap<String, std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Result<PluginPackage, CommandError> {
    use std::io::Write;
    let old = storage::list::<PluginPackage>(connection, "package")?
        .into_iter()
        .find(|p| p.id == update.package.id)
        .ok_or(CommandError::PluginUpdateStale)?;
    if old.version != update.previous_version {
        return Err(CommandError::PluginUpdateStale);
    }
    validate_update(&old, &update.package)?;
    if storage::list::<PluginInstance>(connection, "instance")?
        .iter()
        .any(|i| i.plugin_id == old.id && active.contains_key(&i.id))
    {
        return Err(CommandError::PluginUpdateBusy);
    }
    let previous_revision = storage::revision(connection, &old.id)?;
    let revision = id();
    let directory = root.join(&revision);
    std::fs::create_dir(&directory).map_err(CommandError::internal)?;
    let result = (|| -> Result<(), CommandError> {
        for (name, bytes) in [
            ("manifest.json", &update.manifest),
            ("component.wasm", &update.component),
        ] {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(directory.join(name))
                .map_err(CommandError::internal)?;
            file.write_all(bytes).map_err(CommandError::internal)?;
            file.sync_all().map_err(CommandError::internal)?;
        }
        let transaction = connection
            .unchecked_transaction()
            .map_err(CommandError::internal)?;
        let review = update_requires_review(&old, &update.package);
        for mut instance in storage::list::<PluginInstance>(&transaction, "instance")?
            .into_iter()
            .filter(|i| i.plugin_id == old.id)
        {
            if review {
                instance.enabled = false;
                instance.needs_review = true;
                instance.pending_domain = None;
                instance.credentials_need_review |= credentials_changed(&old, &update.package);
                instance.next_run_at = None;
            }
            storage::put(&transaction, "instance", &instance.id, &instance)?;
        }
        storage::put(&transaction, "package", &old.id, &update.package)?;
        storage::put(&transaction, "revision", &old.id, &revision)?;
        transaction.commit().map_err(CommandError::internal)?;
        Ok(())
    })();
    if let Err(error) = result {
        clean_revision(&directory);
        return Err(error);
    }
    if let Some(previous) = previous_revision {
        let previous = root.join(previous);
        if !std::fs::symlink_metadata(&previous).is_ok_and(|m| m.file_type().is_symlink())
            && previous
                .canonicalize()
                .is_ok_and(|p| p.parent() == Some(root))
        {
            clean_revision(&previous);
        }
    }
    Ok(update.package.clone())
}
#[tauri::command]
pub async fn confirm_plugin_update(
    app: tauri::AppHandle,
    token: String,
) -> Result<PluginPackage, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let package_root = manager::package_root(&app)
            .map_err(CommandError::from)?
            .canonicalize()
            .map_err(CommandError::internal)?;
        let state = app.state::<SharedAppData>();
        let guard = state.lock().map_err(CommandError::internal)?;
        let manager = app.state::<PluginManager>();
        let active = manager.active.lock().map_err(CommandError::internal)?;
        let mut pending = manager.update.lock().map_err(CommandError::internal)?;
        let update = pending
            .as_ref()
            .filter(|u| u.token == token)
            .ok_or(CommandError::PluginUpdateStale)?;
        let root = package_root.join(&update.package.id);
        if !root.exists() {
            return Err(CommandError::PluginUpdateStale);
        }
        let canonical = root.canonicalize().map_err(CommandError::internal)?;
        if canonical.parent() != Some(package_root.as_path())
            || std::fs::symlink_metadata(&root)
                .map_err(CommandError::internal)?
                .file_type()
                .is_symlink()
        {
            return Err(CommandError::InvalidArgument);
        }
        let package = update_locked(
            guard.database.plugin_connection(),
            &canonical,
            update,
            &active,
        )?;
        *pending = None;
        let _ = app.emit("cardbe:plugins-changed", ());
        Ok(package)
    })
    .await
    .map_err(CommandError::internal)?
}
/// Defaults must be values that saving an instance would accept for the field type.
/// Board and column IDs are device-specific, so only their integer shape is checked here.
fn valid_default(field: &SettingField) -> bool {
    let Some(value) = field.default.as_ref().filter(|value| !value.is_null()) else {
        return true;
    };
    match field.kind.as_str() {
        "text" => value.as_str().is_some_and(|s| s.len() <= 4096),
        "number" => value.as_f64().is_some_and(|n| {
            n.is_finite()
                && field.min.is_none_or(|min| n >= min)
                && field.max.is_none_or(|max| n <= max)
        }),
        "boolean" => value.is_boolean(),
        "select" => value
            .as_str()
            .is_some_and(|s| field.options.iter().any(|option| option.value == s)),
        "board" | "column" => value.is_i64(),
        _ => false,
    }
}
/// Only a well-formed `x.y.z` value is echoed back to the frontend.
fn unsupported_api_version(package: &PluginPackage) -> CommandError {
    CommandError::PluginPackageInvalid {
        reason: PluginPackageIssue::UnsupportedApiVersion,
        setting: None,
        api_version: release_version(&package.api_version).map(|_| package.api_version.clone()),
    }
}
fn parse_package(bytes: &[u8]) -> Result<PluginPackage, CommandError> {
    let invalid = CommandError::plugin_package;
    if bytes.len() > 65536 {
        return Err(invalid(PluginPackageIssue::ManifestTooLarge));
    }
    let package: PluginPackage = serde_json::from_slice(bytes)
        .map_err(|_| invalid(PluginPackageIssue::ManifestMalformed))?;
    if !valid_id(&package.id) {
        return Err(invalid(PluginPackageIssue::InvalidId));
    }
    // Installed versions must stay comparable so later in-place updates remain possible.
    if release_version(&package.version).is_none() {
        return Err(invalid(PluginPackageIssue::InvalidVersion));
    }
    if !SUPPORTED_API_VERSIONS.contains(&package.api_version.as_str()) {
        return Err(unsupported_api_version(&package));
    }
    if package.storage_schema_version != 1 {
        return Err(invalid(PluginPackageIssue::UnsupportedStorageSchema));
    }
    if !matches!(package.manifest_version, 1 | 2) {
        return Err(invalid(PluginPackageIssue::UnsupportedManifestVersion));
    }
    if !valid_domains(&package.domains) {
        return Err(invalid(PluginPackageIssue::InvalidDomains));
    }
    if package.error_codes.len() > 64
        || package.log_codes.len() > 64
        || package
            .error_codes
            .iter()
            .chain(&package.log_codes)
            .any(|c| !valid_code(c))
    {
        return Err(invalid(PluginPackageIssue::InvalidCodes));
    }
    if package.http_credentials.is_none()
        && (package
            .settings
            .iter()
            .filter(|f| f.kind == "secret")
            .count()
            > 1
            || (package.settings.iter().any(|f| f.kind == "secret") && package.domains.len() != 1))
    {
        return Err(invalid(PluginPackageIssue::LegacyCredentials));
    }
    if package.settings.len() > 32 {
        return Err(invalid(PluginPackageIssue::InvalidSettings));
    }
    let mut keys = std::collections::HashSet::new();
    for f in &package.settings {
        if !valid_id(&f.key)
            || !keys.insert(&f.key)
            || !matches!(
                f.kind.as_str(),
                "text" | "number" | "boolean" | "select" | "secret" | "board" | "column"
            )
            || f.options.len() > 100
            || f.options.iter().any(|o| o.value.len() > 256)
            || !unique_values(f.options.iter().map(|o| o.value.as_str()))
            || f.default
                .as_ref()
                .is_some_and(|v| v.is_array() || v.is_object() || f.kind == "secret")
            || f.min.is_some_and(|m| !m.is_finite())
            || f.max.is_some_and(|m| !m.is_finite())
            || f.min.zip(f.max).is_some_and(|(min, max)| min > max)
            || !valid_default(f)
        {
            return Err(CommandError::PluginPackageInvalid {
                reason: PluginPackageIssue::InvalidSettings,
                setting: valid_id(&f.key).then(|| f.key.clone()),
                api_version: None,
            });
        }
    }
    if !crate::plugins::credentials::valid_rules(&package) {
        return Err(invalid(PluginPackageIssue::InvalidCredentialRules));
    }
    Ok(package)
}
/// Removes files left by an uninstalled package. Callers verify the ID is not installed.
fn clear_orphaned_package(
    root: &std::path::Path,
    destination: &std::path::Path,
) -> Result<(), CommandError> {
    let Ok(metadata) = std::fs::symlink_metadata(destination) else {
        return Ok(());
    };
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || destination
            .canonicalize()
            .map_err(CommandError::internal)?
            .parent()
            != Some(
                root.canonicalize()
                    .map_err(CommandError::internal)?
                    .as_path(),
            )
    {
        return Err(CommandError::InvalidArgument);
    }
    std::fs::remove_dir_all(destination).map_err(CommandError::internal)
}
fn install_package(
    app: tauri::AppHandle,
    bytes: Vec<u8>,
    component: Vec<u8>,
) -> Result<PluginPackage, CommandError> {
    let package = parse_package(&bytes)?;
    validate_component(&component)?;
    let destination = manager::package_root(&app)
        .map_err(CommandError::from)?
        .join(&package.id);
    let s = app.state::<SharedAppData>();
    let g = s.lock().map_err(CommandError::internal)?;
    let c = g.database.plugin_connection();
    let installed = storage::list::<PluginPackage>(c, "package")?;
    if installed.iter().any(|p| p.id == package.id) {
        return Err(CommandError::InvalidArgument);
    }
    if installed.len() >= 50 {
        return Err(CommandError::plugin_package(
            PluginPackageIssue::PackageLimit,
        ));
    }
    let directory = destination.parent().ok_or(CommandError::InvalidArgument)?;
    std::fs::create_dir_all(directory).map_err(CommandError::internal)?;
    clear_orphaned_package(directory, &destination)?;
    std::fs::create_dir(&destination).map_err(CommandError::internal)?;
    if destination
        .canonicalize()
        .map_err(CommandError::internal)?
        .parent()
        != Some(
            directory
                .canonicalize()
                .map_err(CommandError::internal)?
                .as_path(),
        )
    {
        return Err(CommandError::InvalidArgument);
    }
    let install_result = (|| -> Result<(), CommandError> {
        std::fs::write(destination.join("component.wasm"), component)
            .map_err(CommandError::internal)?;
        std::fs::write(destination.join("manifest.json"), bytes).map_err(CommandError::internal)?;
        storage::put(
            g.database.plugin_connection(),
            "package",
            &package.id,
            &package,
        )?;
        Ok(())
    })();
    if let Err(error) = install_result {
        for file in ["component.wasm", "manifest.json"] {
            let _ = std::fs::remove_file(destination.join(file));
        }
        if std::fs::remove_dir(&destination).is_err() {
            log::warn!(target:"plugins","Could not clean up failed package installation");
        }
        return Err(error);
    }
    let _ = app.emit("cardbe:plugins-changed", ());
    Ok(package)
}
#[tauri::command]
pub fn save_plugin_instance(
    app: tauri::AppHandle,
    instance: InstanceInput,
    secrets: BTreeMap<String, String>,
    package_version: Option<String>,
) -> Result<PluginInstance, CommandError> {
    let id = instance
        .id
        .clone()
        .unwrap_or_else(crate::plugins::types::id);
    busy(&app, &id)?;
    let credential_namespace = manager::secret_namespace(&app, &id).map_err(CommandError::from)?;
    if instance.name.trim().is_empty()
        || instance.name.len() > 256
        || instance.interval_seconds > 86400
        || (instance.interval_seconds != 0 && instance.interval_seconds < 60)
        || serde_json::to_vec(&instance.config)
            .map_err(CommandError::internal)?
            .len()
            > 65536
    {
        return Err(CommandError::InvalidArgument);
    }
    let s = app.state::<SharedAppData>();
    let g = s.lock().map_err(CommandError::internal)?;
    busy(&app, &id)?;
    let c = g.database.plugin_connection();
    let package = storage::list::<PluginPackage>(c, "package")?
        .into_iter()
        .find(|p| p.id == instance.plugin_id)
        .ok_or(CommandError::InvalidArgument)?;
    if package_version
        .as_ref()
        .is_some_and(|version| version != &package.version)
    {
        return Err(CommandError::PluginUpdateStale);
    }
    if g.database
        .board_role(instance.board_id)
        .map_err(CommandError::repository)?
        == crate::models::BoardRole::Viewer
    {
        return Err(CommandError::PermissionDenied);
    }
    if !valid_instance_domains(&package, &instance.allowed_domains) {
        return Err(CommandError::PermissionDenied);
    }
    let columns = g
        .database
        .read_board(instance.board_id)
        .map_err(CommandError::repository)?
        .columns;
    let all: Vec<PluginInstance> = storage::list(c, "instance")?;
    if all.len() >= 100 && instance.id.is_none() {
        return Err(CommandError::InvalidArgument);
    }
    let previous = all.iter().find(|i| i.id == id);
    if previous.is_some_and(|p| p.needs_review) && package_version.is_none() {
        return Err(CommandError::PluginUpdateStale);
    }
    if instance.id.is_some() && previous.is_none() {
        return Err(CommandError::InvalidArgument);
    }
    if previous.is_some_and(|p| p.plugin_id != instance.plugin_id) || !instance.limits.valid() {
        return Err(CommandError::InvalidArgument);
    }
    for (key, value) in &instance.config {
        let f = package
            .settings
            .iter()
            .find(|f| f.key == *key)
            .ok_or(CommandError::InvalidArgument)?;
        let valid = match f.kind.as_str() {
            "secret" => false,
            "text" => value
                .as_str()
                .is_some_and(|s| s.len() <= 4096 && (!f.required || !s.trim().is_empty())),
            "number" => value.as_f64().is_some_and(|n| {
                n.is_finite() && f.min.is_none_or(|m| n >= m) && f.max.is_none_or(|m| n <= m)
            }),
            "boolean" => value.is_boolean(),
            "select" => value
                .as_str()
                .is_some_and(|s| f.options.iter().any(|o| o.value == s)),
            "board" => value.as_i64() == Some(instance.board_id),
            "column" => value
                .as_i64()
                .is_some_and(|id| columns.iter().any(|c| c.id == id)),
            _ => false,
        };
        if !valid {
            return Err(CommandError::InvalidArgument);
        }
    }
    crate::plugins::credentials::resolve(&package, &instance.config)
        .map_err(|_| CommandError::InvalidArgument)?;
    let secret_fields = updated_secret_fields(&package, previous, &secrets)?;
    for f in &package.settings {
        if f.required
            && if f.kind == "secret" {
                !secret_fields.contains(&f.key)
            } else {
                !instance.config.contains_key(&f.key)
            }
        {
            return Err(CommandError::InvalidArgument);
        }
    }
    let reset_cursor =
        previous.is_some_and(|p| p.board_id != instance.board_id || p.config != instance.config);
    let changed_hosts = previous
        .map(|p| {
            crate::plugins::credentials::changed_base_url_hosts(
                &package,
                &p.config,
                &instance.config,
            )
        })
        .unwrap_or_default();
    let allowed_domains: Vec<String> = instance
        .allowed_domains
        .into_iter()
        .filter(|host| !crate::plugins::http::contains_domain(&changed_hosts, host))
        .collect();
    let saved = PluginInstance {
        id: id.clone(),
        plugin_id: instance.plugin_id,
        name: instance.name,
        board_id: instance.board_id,
        config: instance.config,
        secret_fields,
        private_network_domains: retained_private_network_domains(
            instance.private_network_domains,
            &allowed_domains,
        ),
        allowed_domains,
        pending_domain: None,
        enabled: instance.enabled,
        needs_review: false,
        credentials_need_review: false,
        interval_seconds: instance.interval_seconds,
        limits: instance.limits,
        last_run_at: previous.and_then(|p| p.last_run_at),
        last_run_status: previous.and_then(|p| {
            if p.pending_domain.is_some() {
                Some("cancelled".into())
            } else {
                p.last_run_status.clone()
            }
        }),
        next_run_at: Some(now()),
        failures: 0,
        last_error: None,
        running: false,
    };
    let previous_secrets = secrets
        .keys()
        .map(|key| {
            manager::secret(&credential_namespace, key)
                .map(|value| (key.clone(), value))
                .map_err(CommandError::from)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let result = (|| -> Result<(), CommandError> {
        for (key, value) in &secrets {
            manager::write_secret(&credential_namespace, key, value).map_err(CommandError::from)?;
        }
        let tx = c.unchecked_transaction().map_err(CommandError::internal)?;
        if reset_cursor {
            storage::remove(&tx, "cursor", &id)?;
        }
        storage::put(&tx, "instance", &id, &saved)?;
        tx.commit().map_err(CommandError::internal)?;
        Ok(())
    })();
    if let Err(error) = result {
        for (key, value) in previous_secrets {
            if manager::write_secret(&credential_namespace, &key, value.as_deref().unwrap_or(""))
                .is_err()
            {
                log::warn!(target:"plugins","Could not restore plugin credential after failed configuration save");
            }
        }
        return Err(error);
    }
    let _ = app.emit("cardbe:plugins-changed", ());
    Ok(saved)
}
/// Private network access is an option of a host grant, so it never outlives that grant.
fn retained_private_network_domains(
    requested: Vec<String>,
    allowed_domains: &[String],
) -> Vec<String> {
    let mut retained: Vec<String> = Vec::new();
    for host in requested {
        if crate::plugins::http::contains_domain(allowed_domains, &host)
            && !crate::plugins::http::contains_domain(&retained, &host)
        {
            retained.push(host);
        }
    }
    retained
}
fn updated_secret_fields(
    package: &PluginPackage,
    previous: Option<&PluginInstance>,
    secrets: &BTreeMap<String, String>,
) -> Result<Vec<String>, CommandError> {
    let credentials_need_review = previous.is_some_and(|p| p.credentials_need_review);
    let mut secret_fields = previous
        .map(|p| p.secret_fields.clone())
        .unwrap_or_default();
    if credentials_need_review {
        secret_fields.clear();
    }
    for (key, value) in secrets {
        if !package
            .settings
            .iter()
            .any(|f| f.key == *key && f.kind == "secret")
            || value.len() > 8192
            || reqwest::header::HeaderValue::from_str(value).is_err()
        {
            return Err(CommandError::InvalidArgument);
        }
        secret_fields.retain(|k| k != key);
        if !value.is_empty() {
            secret_fields.push(key.clone());
        }
    }
    Ok(secret_fields)
}
#[tauri::command]
pub fn resolve_plugin_domain(
    app: tauri::AppHandle,
    instance_id: String,
    run_id: String,
    allow: bool,
    private_network: Option<bool>,
) -> Result<(), CommandError> {
    if allow
        && app
            .state::<PluginManager>()
            .safe_mode
            .load(Ordering::SeqCst)
    {
        return Err(CommandError::PermissionDenied);
    }
    busy(&app, &instance_id)?;
    let trigger = {
        let state = app.state::<SharedAppData>();
        let guard = state.lock().map_err(CommandError::internal)?;
        busy(&app, &instance_id)?;
        let connection = guard.database.plugin_connection();
        let mut instance = storage::list::<PluginInstance>(connection, "instance")?
            .into_iter()
            .find(|instance| instance.id == instance_id)
            .ok_or(CommandError::InvalidArgument)?;
        let package = storage::list::<PluginPackage>(connection, "package")?
            .into_iter()
            .find(|package| package.id == instance.plugin_id)
            .ok_or(CommandError::InvalidArgument)?;
        if allow
            && guard
                .database
                .board_role(instance.board_id)
                .map_err(CommandError::repository)?
                == crate::models::BoardRole::Viewer
        {
            return Err(CommandError::PermissionDenied);
        }
        let trigger = resolve_domain(
            &mut instance,
            &package,
            &run_id,
            allow,
            private_network.unwrap_or(false),
        )?;
        storage::put(connection, "instance", &instance.id, &instance)?;
        trigger
    };
    let _ = app.emit("cardbe:plugins-changed", ());
    if allow {
        tauri::async_runtime::spawn(async move {
            let _ = manager::run(app, instance_id, &trigger, true).await;
        });
    }
    Ok(())
}

fn resolve_domain(
    instance: &mut PluginInstance,
    package: &PluginPackage,
    run_id: &str,
    allow: bool,
    // A host approval may also grant private network access for a self-hosted service.
    private_network: bool,
) -> Result<String, CommandError> {
    let pending = instance
        .pending_domain
        .as_ref()
        .filter(|pending| pending.run_id == run_id)
        .ok_or(CommandError::InvalidArgument)?;
    let trigger = pending.trigger.clone();
    if instance.needs_review
        || !instance.enabled
        || (allow && !crate::plugins::credentials::valid_rules(package))
    {
        return Err(CommandError::PermissionDenied);
    }
    if allow {
        let mut domains = instance.allowed_domains.clone();
        if !crate::plugins::http::contains_domain(&domains, &pending.domain) {
            // DNS runs only after host approval, so a private network request needs that grant.
            if pending.private_network {
                return Err(CommandError::PermissionDenied);
            }
            domains.push(pending.domain.clone());
        }
        if !valid_instance_domains(package, &domains) {
            return Err(CommandError::PermissionDenied);
        }
        instance.allowed_domains = domains;
        if (pending.private_network || private_network)
            && !crate::plugins::http::contains_domain(
                &instance.private_network_domains,
                &pending.domain,
            )
        {
            instance
                .private_network_domains
                .push(pending.domain.clone());
        }
        instance.next_run_at = Some(now().saturating_add(60_000));
    } else {
        // A declined request pauses this instance until the user enables it again.
        instance.enabled = false;
        instance.next_run_at = None;
        instance.last_run_status = Some("cancelled".into());
    }
    instance.pending_domain = None;
    Ok(trigger)
}

#[tauri::command]
pub fn set_plugin_enabled(
    app: tauri::AppHandle,
    instance_id: String,
    enabled: bool,
) -> Result<(), CommandError> {
    if !enabled {
        cancel_plugin_run(app.clone(), instance_id.clone())?;
    }
    let s = app.state::<SharedAppData>();
    let g = s.lock().map_err(CommandError::internal)?;
    let c = g.database.plugin_connection();
    let mut instance = storage::list::<PluginInstance>(c, "instance")?
        .into_iter()
        .find(|i| i.id == instance_id)
        .ok_or(CommandError::InvalidArgument)?;
    change_enabled(&mut instance, enabled)?;
    let tx = c.unchecked_transaction().map_err(CommandError::internal)?;
    storage::put(&tx, "instance", &instance.id, &instance)?;
    tx.commit().map_err(CommandError::internal)?;
    let _ = app.emit("cardbe:plugins-changed", ());
    Ok(())
}
fn change_enabled(instance: &mut PluginInstance, enabled: bool) -> Result<(), CommandError> {
    if enabled && instance.needs_review {
        return Err(CommandError::PermissionDenied);
    }
    instance.enabled = enabled;
    if !enabled && instance.pending_domain.take().is_some() {
        instance.last_run_status = Some("cancelled".into());
    }
    instance.failures = 0;
    instance.next_run_at = Some(now());
    Ok(())
}
#[tauri::command]
pub async fn run_plugin_instance(
    app: tauri::AppHandle,
    instance_id: String,
) -> Result<PluginRun, CommandError> {
    manager::run(app, instance_id, "manual", false)
        .await
        .map_err(CommandError::from)
}
#[tauri::command]
pub fn cancel_plugin_run(app: tauri::AppHandle, instance_id: String) -> Result<(), CommandError> {
    if let Some(c) = app
        .state::<PluginManager>()
        .active
        .lock()
        .map_err(CommandError::internal)?
        .get(&instance_id)
    {
        c.store(true, Ordering::SeqCst);
    }
    Ok(())
}
#[tauri::command]
pub fn get_plugin_runs(
    app: tauri::AppHandle,
    instance_id: String,
) -> Result<Vec<PluginRun>, CommandError> {
    let s = app.state::<SharedAppData>();
    let g = s.lock().map_err(CommandError::internal)?;
    let mut runs = storage::list::<PluginRun>(g.database.plugin_connection(), "run")?;
    runs.retain(|r| r.instance_id == instance_id);
    runs.sort_by_key(|r| std::cmp::Reverse(r.started_at));
    Ok(runs)
}
#[tauri::command]
pub fn set_plugin_safe_mode(app: tauri::AppHandle, enabled: bool) -> Result<(), CommandError> {
    let m = app.state::<PluginManager>();
    m.safe_mode.store(enabled, Ordering::SeqCst);
    if enabled {
        for c in m.active.lock().map_err(CommandError::internal)?.values() {
            c.store(true, Ordering::SeqCst);
        }
    }
    let s = app.state::<SharedAppData>();
    let g = s.lock().map_err(CommandError::internal)?;
    storage::put(
        g.database.plugin_connection(),
        "safe_mode",
        "global",
        &enabled,
    )?;
    Ok(())
}
fn remove_locked(
    c: &rusqlite::Connection,
    instances: &[PluginInstance],
    namespaces: &BTreeMap<String, String>,
    package: Option<&str>,
) -> Result<(), CommandError> {
    let mut credentials = Vec::new();
    for i in instances {
        for key in &i.secret_fields {
            let namespace = &namespaces[&i.id];
            credentials.push((
                namespace.clone(),
                key.clone(),
                manager::secret(namespace, key).map_err(CommandError::from)?,
            ));
        }
    }
    let result = (|| -> Result<(), CommandError> {
        for (namespace, key, _) in &credentials {
            manager::write_secret(namespace, key, "").map_err(CommandError::from)?;
        }
        let tx = c.unchecked_transaction().map_err(CommandError::internal)?;
        for i in instances {
            storage::remove(&tx, "instance", &i.id)?;
            storage::remove(&tx, "cursor", &i.id)?;
            tx.execute(
                "DELETE FROM cardbe_plugin_sources WHERE instance_id=?1",
                [&i.id],
            )
            .map_err(CommandError::internal)?;
            tx.execute("DELETE FROM cardbe_plugin_data WHERE kind='run' AND json_extract(payload,'$.instance_id')=?1",[&i.id]).map_err(CommandError::internal)?;
        }
        if let Some(package) = package {
            storage::remove(&tx, "package", package)?;
            storage::remove(&tx, "revision", package)?;
        }
        tx.commit().map_err(CommandError::internal)?;
        Ok(())
    })();
    if let Err(error) = result {
        for (namespace, key, value) in credentials {
            if manager::write_secret(&namespace, &key, value.as_deref().unwrap_or("")).is_err() {
                log::warn!(target:"plugins","Could not restore plugin credential after failed removal");
            }
        }
        return Err(error);
    }
    Ok(())
}
#[tauri::command]
pub fn remove_plugin_instance(
    app: tauri::AppHandle,
    instance_id: String,
) -> Result<(), CommandError> {
    let namespace = manager::secret_namespace(&app, &instance_id).map_err(CommandError::from)?;
    let s = app.state::<SharedAppData>();
    let g = s.lock().map_err(CommandError::internal)?;
    busy(&app, &instance_id)?;
    let c = g.database.plugin_connection();
    let instance = storage::list::<PluginInstance>(c, "instance")?
        .into_iter()
        .find(|i| i.id == instance_id)
        .ok_or(CommandError::InvalidArgument)?;
    remove_locked(
        c,
        &[instance],
        &BTreeMap::from([(instance_id.clone(), namespace)]),
        None,
    )?;
    let _ = app.emit("cardbe:plugins-changed", ());
    Ok(())
}
#[tauri::command]
pub fn remove_plugin_package(app: tauri::AppHandle, plugin_id: String) -> Result<(), CommandError> {
    if !valid_id(&plugin_id) {
        return Err(CommandError::InvalidArgument);
    }
    let root = manager::package_root(&app).map_err(CommandError::from)?;
    let target = root.join(&plugin_id);
    let resolved = if target.exists() {
        let resolved = target.canonicalize().map_err(CommandError::internal)?;
        let canonical_root = root.canonicalize().map_err(CommandError::internal)?;
        if resolved.parent() != Some(canonical_root.as_path())
            || std::fs::symlink_metadata(&target)
                .map_err(CommandError::internal)?
                .file_type()
                .is_symlink()
        {
            return Err(CommandError::InvalidArgument);
        }
        Some(resolved)
    } else {
        None
    };
    let s = app.state::<SharedAppData>();
    let g = s.lock().map_err(CommandError::internal)?;
    let c = g.database.plugin_connection();
    let instances = storage::list::<PluginInstance>(c, "instance")?
        .into_iter()
        .filter(|i| i.plugin_id == plugin_id)
        .collect::<Vec<_>>();
    let mut namespaces = BTreeMap::new();
    for i in &instances {
        busy(&app, &i.id)?;
        namespaces.insert(
            i.id.clone(),
            manager::secret_namespace_for_path(g.database.path(), &i.id),
        );
    }
    remove_locked(c, &instances, &namespaces, Some(&plugin_id))?;
    // The package is already removed; installation clears any leftover directory.
    if let Some(resolved) = resolved {
        if std::fs::remove_dir_all(resolved).is_err() {
            log::warn!(target: "plugins", "Could not remove files of an uninstalled plugin");
        }
    }
    let _ = app.emit("cardbe:plugins-changed", ());
    Ok(())
}
#[cfg(test)]
mod tests;
