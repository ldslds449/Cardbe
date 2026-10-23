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
const SUPPORTED_API_VERSIONS: [&str; 1] = ["1.1.0"];
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
    if previous.is_some_and(|p| p.plugin_id != instance.plugin_id) {
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
mod tests {
    use super::*;
    fn package(version: &str) -> PluginPackage {
        parse_package(
            &serde_json::to_vec(&serde_json::json!({
                "id":"example", "name":"Example", "version":version,
                "api_version":"1.1.0", "storage_schema_version":1,
            }))
            .unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn the_package_entry_installs_new_ids_and_only_previews_newer_installed_versions() {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        storage::initialize(&c).unwrap();
        let old = package("1.0.0");
        assert!(update_candidate(&c, &old).unwrap().is_none());
        storage::put(&c, "package", &old.id, &old).unwrap();
        let new = package("1.1.0");
        assert_eq!(
            update_candidate(&c, &new).unwrap().unwrap().version,
            "1.0.0"
        );
        assert!(matches!(
            update_candidate(&c, &old),
            Err(CommandError::PluginUpdateNotNewer)
        ));
        assert!(matches!(
            update_candidate(&c, &package("0.9.0")),
            Err(CommandError::PluginUpdateNotNewer)
        ));
        let mut different = new.clone();
        different.id = "another".into();
        assert!(update_candidate(&c, &different).unwrap().is_none());
        assert_eq!(
            storage::list::<PluginPackage>(&c, "package").unwrap()[0].version,
            "1.0.0"
        );
        assert!(storage::revision(&c, &old.id).unwrap().is_none());
        let installed = serde_json::to_value(PluginPackageSelection::Installed {
            package: old.clone(),
        })
        .unwrap();
        assert_eq!(installed["kind"], "installed");
        let update = serde_json::to_value(PluginPackageSelection::Update {
            current_package: old,
            preview: Box::new(PluginUpdatePreview {
                token: "preview".into(),
                current_version: "1.0.0".into(),
                requires_review: false,
                package: new,
            }),
        })
        .unwrap();
        assert_eq!(update["kind"], "update");
        assert_eq!(update["current_package"]["version"], "1.0.0");
        assert_eq!(update["preview"]["package"]["version"], "1.1.0");
    }
    #[test]
    fn plugin_updates_validate_identity_schema_and_numeric_release_order() {
        let old = package("1.9.9");
        assert!(validate_update(&old, &package("1.10.0")).is_ok());
        for version in ["1.9.9", "1.9.8", "0.99.0"] {
            assert!(matches!(
                validate_update(&old, &package(version)),
                Err(CommandError::PluginUpdateNotNewer)
            ));
        }
        for version in [
            "1.10",
            "01.10.0",
            "1.10.0-beta",
            "1.10.0+build",
            "1. 10.0",
            "18446744073709551616.0.0",
        ] {
            assert!(release_version(version).is_none());
        }
        for (field, api_version) in [
            ("id", ""),
            ("api_version", "2.0.0"),
            ("api_version", "1.0.0"),
            ("storage_schema_version", ""),
        ] {
            let mut next = package("2.0.0");
            match field {
                "id" => next.id = "other".into(),
                "api_version" => next.api_version = api_version.into(),
                _ => next.storage_schema_version = 2,
            }
            let error = validate_update(&old, &next).unwrap_err();
            match field {
                "id" => assert!(matches!(error, CommandError::InvalidArgument)),
                "api_version" => assert!(matches!(
                    error,
                    CommandError::PluginPackageInvalid {
                        reason: PluginPackageIssue::UnsupportedApiVersion,
                        ..
                    }
                )),
                _ => assert!(matches!(
                    error,
                    CommandError::PluginPackageInvalid {
                        reason: PluginPackageIssue::UnsupportedStorageSchema,
                        ..
                    }
                )),
            }
        }
        assert!(validate_update(&old, &package("2.0.0")).is_ok());
    }
    #[test]
    fn rejected_manifests_report_safe_specific_reasons() {
        let base = serde_json::json!({
            "id":"example", "name":"Example", "version":"1.0.0",
            "api_version":"1.1.0", "storage_schema_version":1,
        });
        let reject = |change: &dyn Fn(&mut serde_json::Value)| {
            let mut manifest = base.clone();
            change(&mut manifest);
            serde_json::to_value(
                parse_package(&serde_json::to_vec(&manifest).unwrap()).unwrap_err(),
            )
            .unwrap()
        };
        let issue =
            |reason: &str| serde_json::json!({"code":"PLUGIN_PACKAGE_INVALID","reason":reason});
        assert_eq!(
            serde_json::to_value(parse_package(b"{not json").unwrap_err()).unwrap(),
            issue("MANIFEST_MALFORMED")
        );
        assert_eq!(
            serde_json::to_value(parse_package(&vec![b' '; 65537]).unwrap_err()).unwrap(),
            issue("MANIFEST_TOO_LARGE")
        );
        assert_eq!(reject(&|m| m["id"] = "Bad ID".into()), issue("INVALID_ID"));
        assert_eq!(
            reject(&|m| m["version"] = "v1".into()),
            issue("INVALID_VERSION")
        );
        assert_eq!(
            reject(&|m| m["api_version"] = "1.0.0".into()),
            serde_json::json!({
                "code":"PLUGIN_PACKAGE_INVALID",
                "reason":"UNSUPPORTED_API_VERSION",
                "api_version":"1.0.0",
            })
        );
        // Arbitrary package text is never echoed back.
        assert_eq!(
            reject(&|m| m["api_version"] = "token=secret".into()),
            issue("UNSUPPORTED_API_VERSION")
        );
        assert_eq!(
            reject(&|m| m["storage_schema_version"] = 2.into()),
            issue("UNSUPPORTED_STORAGE_SCHEMA")
        );
        assert_eq!(
            reject(&|m| m["manifest_version"] = 3.into()),
            issue("UNSUPPORTED_MANIFEST_VERSION")
        );
        assert_eq!(
            reject(&|m| m["domains"] = serde_json::json!(["http://bad"])),
            issue("INVALID_DOMAINS")
        );
        assert_eq!(
            reject(&|m| m["error_codes"] = serde_json::json!(["lower"])),
            issue("INVALID_CODES")
        );
        assert_eq!(
            reject(&|m| {
                m["domains"] = serde_json::json!(["api.example.com"]);
                m["settings"] = serde_json::json!([
                    {"key":"a","label":"A","type":"secret"},
                    {"key":"b","label":"B","type":"secret"},
                ]);
            }),
            issue("LEGACY_CREDENTIALS")
        );
        assert_eq!(
            reject(&|m| m["settings"] = serde_json::json!([
                {"key":"limit","label":"Limit","type":"number","default":"many"},
            ])),
            serde_json::json!({
                "code":"PLUGIN_PACKAGE_INVALID",
                "reason":"INVALID_SETTINGS",
                "setting":"limit",
            })
        );
        assert_eq!(
            reject(&|m| m["settings"] = serde_json::json!([
                {"key":"Bad Key","label":"Bad","type":"text"},
            ])),
            issue("INVALID_SETTINGS")
        );
        assert_eq!(
            reject(&|m| m["http_credentials"] = serde_json::json!([
                {"destination":{"type":"domain","domain":"other.example.com"},"path_prefix":"/"},
            ])),
            issue("INVALID_CREDENTIAL_RULES")
        );
    }
    #[test]
    fn install_requires_updatable_versions_and_typed_setting_defaults() {
        let base = serde_json::json!({
            "id":"example", "name":"Example", "version":"1.0.0",
            "api_version":"1.1.0", "storage_schema_version":1,
        });
        for version in ["v1.0.0", "1.0", "1.0.0-beta", "01.0.0", ""] {
            let mut manifest = base.clone();
            manifest["version"] = version.into();
            assert!(
                parse_package(&serde_json::to_vec(&manifest).unwrap()).is_err(),
                "{version}"
            );
        }
        let field = |kind: &str, default: serde_json::Value| {
            let mut manifest = base.clone();
            manifest["settings"] = serde_json::json!([{
                "key":"field", "label":"Field", "type":kind, "min":1, "max":10,
                "options":[{"value":"open","label":"Open"}], "default":default,
            }]);
            parse_package(&serde_json::to_vec(&manifest).unwrap())
        };
        for (kind, default) in [
            ("text", serde_json::json!("value")),
            ("text", serde_json::Value::Null),
            ("number", serde_json::json!(5)),
            ("boolean", serde_json::json!(true)),
            ("select", serde_json::json!("open")),
            ("board", serde_json::json!(1)),
            ("column", serde_json::json!(2)),
        ] {
            assert!(field(kind, default.clone()).is_ok(), "{kind} {default}");
        }
        for (kind, default) in [
            ("text", serde_json::json!(5)),
            ("text", serde_json::json!("x".repeat(4097))),
            ("number", serde_json::json!("5")),
            ("number", serde_json::json!(11)),
            ("boolean", serde_json::json!("true")),
            ("select", serde_json::json!("closed")),
            ("board", serde_json::json!("1")),
            ("column", serde_json::json!(1.5)),
        ] {
            assert!(field(kind, default.clone()).is_err(), "{kind} {default}");
        }
    }
    #[test]
    fn manifest_versions_default_validate_persist_and_upgrade_independently() {
        let mut manifest = serde_json::to_value(package("1.0.0")).unwrap();
        manifest.as_object_mut().unwrap().remove("manifest_version");
        let old = parse_package(&serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert_eq!(old.manifest_version, 1);
        for version in [
            serde_json::json!(0),
            serde_json::json!(3),
            serde_json::json!(-1),
            serde_json::json!(1.5),
            serde_json::json!("2"),
            serde_json::Value::Null,
            serde_json::json!(4294967296_u64),
        ] {
            manifest["manifest_version"] = version;
            assert!(parse_package(&serde_json::to_vec(&manifest).unwrap()).is_err());
        }
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        storage::initialize(&connection).unwrap();
        let mut legacy = serde_json::to_value(&old).unwrap();
        legacy.as_object_mut().unwrap().remove("manifest_version");
        storage::put(&connection, "package", &old.id, &legacy).unwrap();
        assert_eq!(
            storage::list::<PluginPackage>(&connection, "package").unwrap()[0].manifest_version,
            1
        );
        for version in [1, 2] {
            manifest["manifest_version"] = version.into();
            let current = parse_package(&serde_json::to_vec(&manifest).unwrap()).unwrap();
            assert_eq!(current.manifest_version, version);
            storage::put(&connection, "package", &current.id, &current).unwrap();
            assert_eq!(
                storage::list::<PluginPackage>(&connection, "package").unwrap()[0].manifest_version,
                version
            );
        }
        manifest["version"] = "1.1.0".into();
        let upgraded = parse_package(&serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(validate_update(&old, &upgraded).is_ok());
        assert!(update_requires_review(&old, &upgraded));
        assert!(!credentials_changed(&old, &upgraded));
        let mut future = upgraded;
        future.manifest_version = 3;
        assert!(validate_update(&old, &future).is_err());
    }
    #[test]
    fn plugin_update_switches_revision_atomically_and_preserves_local_data() {
        let root = std::env::temp_dir().join(format!("cardbe-update-test-{}", id()));
        std::fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        std::fs::write(root.join("component.wasm"), b"old").unwrap();
        let c = rusqlite::Connection::open_in_memory().unwrap();
        storage::initialize(&c).unwrap();
        let old = package("1.0.0");
        storage::put(&c, "package", &old.id, &old).unwrap();
        let mut original = instance();
        original.enabled = true;
        original.config.insert("user_value".into(), "keep".into());
        original.secret_fields = vec!["token".into()];
        original.allowed_domains = vec!["api.example.com".into()];
        original.next_run_at = Some(123);
        storage::put(&c, "instance", &original.id, &original).unwrap();
        let mut other = original.clone();
        other.id = "other".into();
        other.plugin_id = "another".into();
        storage::put(&c, "instance", &other.id, &other).unwrap();
        storage::put(&c, "cursor", &original.id, &Some("cursor")).unwrap();
        storage::put(&c, "run", "run", &serde_json::json!({"instance_id":"a"})).unwrap();
        c.execute(
            "INSERT INTO cardbe_plugin_sources VALUES('a','source',1,2,'{}')",
            [],
        )
        .unwrap();
        let mut update = PreparedPluginUpdate {
            token: id(),
            previous_version: "1.0.0".into(),
            package: package("1.1.0"),
            manifest: b"manifest".to_vec(),
            component: b"new".to_vec(),
        };
        let mut active = std::collections::HashMap::new();
        active.insert(
            original.id.clone(),
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        );
        assert!(matches!(
            update_locked(&c, &root, &update, &active),
            Err(CommandError::PluginUpdateBusy)
        ));
        assert!(storage::revision(&c, &old.id).unwrap().is_none());
        active.clear();
        c.execute_batch("CREATE TRIGGER fail_update BEFORE INSERT ON cardbe_plugin_data WHEN NEW.kind='revision' BEGIN SELECT RAISE(ABORT,'isolated test failure'); END;").unwrap();
        assert!(update_locked(&c, &root, &update, &active).is_err());
        assert_eq!(
            storage::list::<PluginPackage>(&c, "package").unwrap()[0].version,
            "1.0.0"
        );
        assert_eq!(std::fs::read(root.join("component.wasm")).unwrap(), b"old");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        c.execute_batch("DROP TRIGGER fail_update;").unwrap();
        update_locked(&c, &root, &update, &active).unwrap();
        let first = storage::revision(&c, &old.id).unwrap().unwrap();
        assert_eq!(
            std::fs::read(root.join(&first).join("component.wasm")).unwrap(),
            b"new"
        );
        let unchanged = storage::list::<PluginInstance>(&c, "instance").unwrap();
        assert_eq!(
            serde_json::to_value(&unchanged[0]).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
        assert!(matches!(
            update_locked(&c, &root, &update, &active),
            Err(CommandError::PluginUpdateStale)
        ));
        update.previous_version = "1.1.0".into();
        update.package.version = "1.2.0".into();
        update.package.domains.push("new.example.com".into());
        c.execute_batch("CREATE TRIGGER fail_review BEFORE INSERT ON cardbe_plugin_data WHEN NEW.kind='revision' BEGIN SELECT RAISE(ABORT,'isolated test failure'); END;").unwrap();
        assert!(update_locked(&c, &root, &update, &active).is_err());
        assert_eq!(storage::revision(&c, &old.id).unwrap(), Some(first.clone()));
        assert_eq!(
            serde_json::to_value(&storage::list::<PluginInstance>(&c, "instance").unwrap()[0])
                .unwrap(),
            serde_json::to_value(&original).unwrap()
        );
        c.execute_batch("DROP TRIGGER fail_review;").unwrap();
        update_locked(&c, &root, &update, &active).unwrap();
        assert!(!root.join(first).exists());
        let saved = storage::list::<PluginInstance>(&c, "instance").unwrap();
        assert!(!saved[0].enabled && saved[0].needs_review && saved[0].next_run_at.is_none());
        assert_eq!(saved[0].config, original.config);
        assert_eq!(saved[0].secret_fields, original.secret_fields);
        assert_eq!(saved[0].allowed_domains, original.allowed_domains);
        assert_eq!(saved[0].interval_seconds, original.interval_seconds);
        assert_eq!(
            serde_json::to_value(&saved[1]).unwrap(),
            serde_json::to_value(&other).unwrap()
        );
        assert_eq!(
            storage::list::<String>(&c, "cursor").unwrap(),
            vec!["cursor"]
        );
        assert_eq!(
            storage::list::<serde_json::Value>(&c, "run").unwrap().len(),
            1
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM cardbe_plugin_sources", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn plugin_update_requires_review_for_settings_and_credential_scope_changes() {
        let old = parse_package(br#"{"id":"example","name":"Example","version":"1.0.0","api_version":"1.1.0","storage_schema_version":1,"domains":["api.example.com"],"settings":[{"key":"token","type":"secret","label":"Token","required":true}]}"#).unwrap();
        let mut next = old.clone();
        next.version = "1.1.0".into();
        assert!(!update_requires_review(&old, &next));
        assert!(!credentials_changed(&old, &next));
        next.domains = vec!["new.example.com".into()];
        assert!(update_requires_review(&old, &next) && credentials_changed(&old, &next));
        next = old.clone();
        next.settings[0].key = "new_token".into();
        assert!(update_requires_review(&old, &next) && credentials_changed(&old, &next));
        next = old.clone();
        next.settings[0].required = false;
        assert!(update_requires_review(&old, &next));
        assert!(!credentials_changed(&old, &next));
        let mut legacy = serde_json::to_value(instance()).unwrap();
        legacy.as_object_mut().unwrap().remove("needs_review");
        legacy
            .as_object_mut()
            .unwrap()
            .remove("credentials_need_review");
        let legacy: PluginInstance = serde_json::from_value(legacy).unwrap();
        assert!(!legacy.needs_review && !legacy.credentials_need_review);
        let mut paused = instance();
        paused.needs_review = true;
        assert!(matches!(
            change_enabled(&mut paused, true),
            Err(CommandError::PermissionDenied)
        ));
        assert!(!paused.enabled);
        assert!(change_enabled(&mut paused, false).is_ok());
        assert!(paused.needs_review);
        paused.needs_review = false;
        assert!(change_enabled(&mut paused, true).is_ok());
        assert!(paused.enabled);
    }
    fn instance() -> PluginInstance {
        PluginInstance {
            id: "a".into(),
            plugin_id: "example".into(),
            name: "Example".into(),
            board_id: 1,
            config: Default::default(),
            secret_fields: vec![],
            allowed_domains: vec![],
            private_network_domains: vec![],
            pending_domain: None,
            enabled: false,
            needs_review: false,
            credentials_need_review: false,
            interval_seconds: 0,
            last_run_at: None,
            last_run_status: None,
            next_run_at: None,
            failures: 0,
            last_error: None,
            running: false,
        }
    }
    #[test]
    fn domain_approval_is_persisted_scoped_and_validated_and_declining_pauses() {
        let package: PluginPackage = serde_json::from_value(serde_json::json!({
            "id":"example", "name":"Example", "version":"1.0.0", "api_version":"1.1.0",
            "storage_schema_version":1, "domains":["api.example.com"]
        }))
        .unwrap();
        let mut pending = instance();
        pending.enabled = true;
        pending.pending_domain = Some(PendingDomain {
            domain: "api.example.com".into(),
            run_id: "run-1".into(),
            trigger: "manual".into(),
            private_network: false,
        });
        assert_eq!(
            resolve_domain(&mut pending.clone(), &package, "run-1", true, false).unwrap(),
            "manual"
        );
        let mut scheduled = pending.clone();
        scheduled.pending_domain.as_mut().unwrap().trigger = "schedule".into();
        assert_eq!(
            resolve_domain(&mut scheduled, &package, "run-1", true, false).unwrap(),
            "schedule"
        );
        assert!(resolve_domain(&mut pending.clone(), &package, "stale-run", true, false).is_err());
        let mut needs_review = pending.clone();
        let mut future = package.clone();
        future.manifest_version = 3;
        assert!(resolve_domain(&mut pending.clone(), &future, "run-1", true, false).is_err());
        let mut declined = pending.clone();
        resolve_domain(&mut declined, &future, "run-1", false, false).unwrap();
        assert!(!declined.enabled && declined.pending_domain.is_none());
        needs_review.needs_review = true;
        assert!(resolve_domain(&mut needs_review, &package, "run-1", true, false).is_err());
        let mut disabled = pending.clone();
        disabled.enabled = false;
        assert!(resolve_domain(&mut disabled, &package, "run-1", true, false).is_err());
        let mut other_host = pending.clone();
        other_host.pending_domain.as_mut().unwrap().domain = "other.example.com".into();
        assert!(resolve_domain(&mut other_host, &package, "run-1", true, false).is_err());
        let mut full = pending.clone();
        full.allowed_domains = (0..16).map(|i| format!("host{i}.example.com")).collect();
        let mut custom_package = package.clone();
        custom_package.allow_custom_domains = true;
        assert!(resolve_domain(&mut full, &custom_package, "run-1", true, false).is_err());
        let mut rejected = pending.clone();
        resolve_domain(&mut rejected, &package, "run-1", false, false).unwrap();
        assert!(
            !rejected.enabled
                && rejected.pending_domain.is_none()
                && rejected.next_run_at.is_none()
        );
        assert!(rejected.allowed_domains.is_empty());
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        storage::initialize(&connection).unwrap();
        storage::put(&connection, "instance", "pending", &pending).unwrap();
        let mut restored: PluginInstance =
            storage::list(&connection, "instance").unwrap().remove(0);
        resolve_domain(&mut restored, &package, "run-1", true, false).unwrap();
        storage::put(&connection, "instance", "pending", &restored).unwrap();
        let restored: PluginInstance = storage::list(&connection, "instance").unwrap().remove(0);
        assert_eq!(restored.allowed_domains, ["api.example.com"]);
        assert!(restored.enabled && restored.pending_domain.is_none());
        assert!(instance().allowed_domains.is_empty());
        assert!(resolve_domain(&mut restored.clone(), &package, "run-1", true, false).is_err());
        let mut legacy = serde_json::to_value(instance()).unwrap();
        legacy.as_object_mut().unwrap().remove("pending_domain");
        assert!(serde_json::from_value::<PluginInstance>(legacy)
            .unwrap()
            .pending_domain
            .is_none());
    }
    #[test]
    fn private_network_access_is_an_option_of_the_host_grant() {
        let package: PluginPackage = serde_json::from_value(serde_json::json!({
            "id":"example", "name":"Example", "version":"1.0.0", "api_version":"1.1.0",
            "storage_schema_version":1, "domains":[], "allow_custom_domains":true
        }))
        .unwrap();
        let mut host_request = instance();
        host_request.enabled = true;
        host_request.pending_domain = Some(PendingDomain {
            domain: "jira.example.com:8443".into(),
            run_id: "run-host".into(),
            trigger: "manual".into(),
            private_network: false,
        });
        // One approval can grant the host and its private network access together.
        let mut both = host_request.clone();
        resolve_domain(&mut both, &package, "run-host", true, true).unwrap();
        assert_eq!(both.allowed_domains, ["jira.example.com:8443"]);
        assert_eq!(both.private_network_domains, ["jira.example.com:8443"]);
        let mut public = host_request.clone();
        resolve_domain(&mut public, &package, "run-host", true, false).unwrap();
        assert_eq!(public.allowed_domains, ["jira.example.com:8443"]);
        assert!(public.private_network_domains.is_empty());
        let mut declined = host_request.clone();
        resolve_domain(&mut declined, &package, "run-host", false, true).unwrap();
        assert!(!declined.enabled && declined.allowed_domains.is_empty());
        assert!(declined.private_network_domains.is_empty());

        // A private network request follows an approved host whose DNS answered privately.
        let mut pending = public.clone();
        pending.pending_domain = Some(PendingDomain {
            domain: "jira.example.com:8443".into(),
            run_id: "run-private".into(),
            trigger: "schedule".into(),
            private_network: true,
        });
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        storage::initialize(&connection).unwrap();
        storage::put(&connection, "instance", "private", &pending).unwrap();
        let mut restored: PluginInstance =
            storage::list(&connection, "instance").unwrap().remove(0);
        assert!(restored.pending_domain.as_ref().unwrap().private_network);
        assert!(resolve_domain(&mut restored.clone(), &package, "stale", true, false).is_err());
        let mut revoked_host = restored.clone();
        revoked_host.allowed_domains.clear();
        assert!(resolve_domain(&mut revoked_host, &package, "run-private", true, false).is_err());
        assert!(revoked_host.private_network_domains.is_empty());
        assert_eq!(
            resolve_domain(&mut restored, &package, "run-private", true, false).unwrap(),
            "schedule"
        );
        assert_eq!(restored.allowed_domains, ["jira.example.com:8443"]);
        assert_eq!(restored.private_network_domains, ["jira.example.com:8443"]);
        assert!(restored.enabled && restored.pending_domain.is_none());
        let mut declined = pending.clone();
        resolve_domain(&mut declined, &package, "run-private", false, false).unwrap();
        assert!(!declined.enabled && declined.private_network_domains.is_empty());
        assert_eq!(declined.allowed_domains, ["jira.example.com:8443"]);

        // Settings grant and revoke it with the host, like other connection permissions.
        let allowed = vec!["jira.example.com:8443".to_owned(), "api.example.com".into()];
        assert_eq!(
            retained_private_network_domains(
                vec![
                    "jira.example.com:8443".into(),
                    "jira.example.com:8443".into(),
                    "api.example.com:443".into(),
                    "removed.example.com".into(),
                ],
                &allowed
            ),
            ["jira.example.com:8443", "api.example.com:443"]
        );
        assert!(retained_private_network_domains(allowed.clone(), &[]).is_empty());
        let mut legacy = serde_json::to_value(&pending).unwrap();
        let object = legacy.as_object_mut().unwrap();
        object.remove("private_network_domains");
        object["pending_domain"]
            .as_object_mut()
            .unwrap()
            .remove("private_network");
        let legacy: PluginInstance = serde_json::from_value(legacy).unwrap();
        assert!(legacy.private_network_domains.is_empty());
        assert!(!legacy.pending_domain.unwrap().private_network);
    }
    #[test]
    fn custom_port_pending_requests_and_grants_survive_restart_and_revocation() {
        let package: PluginPackage = serde_json::from_value(serde_json::json!({
            "id":"example", "name":"Example", "version":"1.0.0", "api_version":"1.1.0",
            "storage_schema_version":1, "domains":["api.example.com:8443"]
        }))
        .unwrap();
        let mut pending = instance();
        pending.enabled = true;
        pending.pending_domain = Some(PendingDomain {
            domain: "api.example.com:8443".into(),
            run_id: "run-port".into(),
            trigger: "schedule".into(),
            private_network: false,
        });
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        storage::initialize(&connection).unwrap();
        storage::put(&connection, "instance", "port", &pending).unwrap();
        let mut restored: PluginInstance =
            storage::list(&connection, "instance").unwrap().remove(0);
        assert_eq!(
            restored.pending_domain.as_ref().unwrap().domain,
            "api.example.com:8443"
        );
        assert!(resolve_domain(&mut restored, &package, "stale", true, false).is_err());
        assert_eq!(
            resolve_domain(&mut restored, &package, "run-port", true, false).unwrap(),
            "schedule"
        );
        storage::put(&connection, "instance", "port", &restored).unwrap();
        let mut restored: PluginInstance =
            storage::list(&connection, "instance").unwrap().remove(0);
        assert_eq!(restored.allowed_domains, ["api.example.com:8443"]);
        restored.allowed_domains.clear();
        restored.pending_domain = pending.pending_domain.clone();
        resolve_domain(&mut restored, &package, "run-port", false, false).unwrap();
        assert!(
            !restored.enabled
                && restored.allowed_domains.is_empty()
                && restored.pending_domain.is_none()
        );
    }
    #[test]
    fn manifest_domains_and_select_values_are_unique_and_valid() {
        assert!(valid_domains(&["api.github.com".into()]));
        assert!(valid_domains(&[
            "api.github.com:8443".into(),
            "api.github.com:443".into()
        ]));
        assert!(valid_domains(&["api.github.com:65535".into()]));
        for invalid in [
            vec!["".into()],
            vec!["api.github.com".into(), "api.github.com".into()],
            vec!["bad..example".into()],
            vec!["-bad.example".into()],
            vec!["example.com/path".into()],
            vec!["EXAMPLE.com".into()],
        ] {
            assert!(!valid_domains(&invalid));
        }
        assert!(unique_values(["open", "closed"]));
        assert!(!unique_values(["open", "open"]));
        assert!(!unique_values([""]));
    }
    #[test]
    fn custom_hosts_require_manifest_opt_in_and_valid_explicit_grants() {
        let mut package: PluginPackage = serde_json::from_value(serde_json::json!({
            "id": "fixture", "name": "Fixture", "version": "1.0.0",
            "api_version": "1.1.0", "storage_schema_version": 1,
            "domains": ["api.example.com"]
        }))
        .unwrap();
        assert!(!package.allow_custom_domains);
        assert!(valid_instance_domains(
            &package,
            &["api.example.com".into()]
        ));
        assert!(!valid_instance_domains(
            &package,
            &["custom.example.com".into()]
        ));
        assert!(valid_instance_domains(
            &package,
            &["api.example.com:443".into()]
        ));
        assert!(!valid_instance_domains(
            &package,
            &["api.example.com:8443".into()]
        ));
        package.domains = vec!["api.example.com:8443".into()];
        assert!(valid_instance_domains(
            &package,
            &["api.example.com:8443".into()]
        ));
        assert!(!valid_instance_domains(
            &package,
            &["api.example.com".into()]
        ));
        package.allow_custom_domains = true;
        assert!(valid_instance_domains(
            &package,
            &["api.example.com".into(), "custom.example.com".into()]
        ));
        assert!(valid_instance_domains(&package, &[]));
        for domains in [
            vec!["localhost".into()],
            vec!["a.localhost".into()],
            vec!["127.0.0.1".into()],
            vec!["10.0.0.1".into()],
            vec!["example.com:0".into()],
            vec!["example.com:65536".into()],
            vec!["example.com:0443".into()],
            vec!["example.com".into(), "example.com:443".into()],
            vec!["*.example.com".into()],
            vec!["custom.example.com".into(), "custom.example.com".into()],
            (0..17).map(|i| format!("host{i}.example.com")).collect(),
        ] {
            assert!(!valid_instance_domains(&package, &domains));
        }
    }
    #[test]
    fn explicit_credential_rules_allow_multiple_secrets_and_require_update_review() {
        let mut manifest = serde_json::json!({
            "id":"scoped", "name":"Scoped", "version":"1.0.0", "api_version":"1.1.0", "storage_schema_version":1,
            "domains":["fixed.example.com"], "allow_custom_domains":true,
            "settings":[{"key":"base_url","type":"text","label":"URL"},{"key":"token","type":"secret","label":"Token"},{"key":"admin_token","type":"secret","label":"Admin token"}],
            "http_credentials":[
                {"destination":{"type":"base_url","setting":"base_url"},"path_prefix":"/tasks","bearer_secret":"token"},
                {"destination":{"type":"base_url","setting":"base_url"},"path_prefix":"/admin","bearer_secret":"admin_token"},
                {"destination":{"type":"domain","domain":"fixed.example.com"},"path_prefix":"/public"}
            ]
        });
        let old = parse_package(&serde_json::to_vec(&manifest).unwrap()).unwrap();
        let mut header = manifest.clone();
        header["http_credentials"][0]
            .as_object_mut()
            .unwrap()
            .remove("bearer_secret");
        header["http_credentials"][0]["auth"] =
            serde_json::json!({"type":"header", "name":"X-API-Key", "secret":"token"});
        assert!(parse_package(&serde_json::to_vec(&header).unwrap()).is_err());
        header["manifest_version"] = 2.into();
        let keyed = parse_package(&serde_json::to_vec(&header).unwrap()).unwrap();
        assert!(update_requires_review(&old, &keyed) && credentials_changed(&old, &keyed));
        assert_eq!(
            parse_package(&serde_json::to_vec(&keyed).unwrap())
                .unwrap()
                .http_credentials,
            keyed.http_credentials
        );
        let mut renamed = header.clone();
        renamed["http_credentials"][0]["auth"]["name"] = "Api-Key".into();
        let renamed = parse_package(&serde_json::to_vec(&renamed).unwrap()).unwrap();
        assert!(update_requires_review(&keyed, &renamed) && credentials_changed(&keyed, &renamed));
        for name in [
            "Authorization",
            "hOsT",
            "Cookie",
            "Content-Length",
            "Connection",
            "Proxy-Authorization",
            "Transfer-Encoding",
            "Sec-Fetch-Site",
            "Content-Type",
            "",
            "X-API-Key\r\nOther",
        ] {
            let mut invalid = header.clone();
            invalid["http_credentials"][0]["auth"]["name"] = name.into();
            assert!(
                parse_package(&serde_json::to_vec(&invalid).unwrap()).is_err(),
                "{name}"
            );
        }
        for legacy in [serde_json::Value::Null, serde_json::json!("token")] {
            let mut invalid = header.clone();
            invalid["http_credentials"][0]["bearer_secret"] = legacy;
            assert!(parse_package(&serde_json::to_vec(&invalid).unwrap()).is_err());
        }
        header["http_credentials"][0]["auth"] =
            serde_json::json!({"type":"bearer", "secret":"token"});
        assert!(parse_package(&serde_json::to_vec(&header).unwrap()).is_ok());
        header["http_credentials"][0]["auth"]["secret"] = "missing".into();
        assert!(parse_package(&serde_json::to_vec(&header).unwrap()).is_err());
        let mut defaults = manifest.clone();
        defaults["allow_custom_domains"] = serde_json::json!(false);
        for url in [
            "https://custom.example.com/api",
            "https://fixed.example.com:8443/api",
            "https://fixed.example.com/api//",
            "https://fixed.example.com/a/../api",
        ] {
            defaults["settings"][0]["default"] = serde_json::json!(url);
            assert!(
                parse_package(&serde_json::to_vec(&defaults).unwrap()).is_err(),
                "{url}"
            );
        }
        for url in [
            "",
            "https://fixed.example.com/",
            "https://fixed.example.com:443/api/",
        ] {
            defaults["settings"][0]["default"] = serde_json::json!(url);
            assert!(
                parse_package(&serde_json::to_vec(&defaults).unwrap()).is_ok(),
                "{url}"
            );
        }
        defaults["allow_custom_domains"] = serde_json::json!(true);
        defaults["settings"][0]["default"] =
            serde_json::json!("https://custom.example.com:8443/api/");
        assert!(parse_package(&serde_json::to_vec(&defaults).unwrap()).is_ok());
        let c = rusqlite::Connection::open_in_memory().unwrap();
        storage::initialize(&c).unwrap();
        storage::put(&c, "package", &old.id, &old).unwrap();
        let loaded = storage::list::<PluginPackage>(&c, "package")
            .unwrap()
            .remove(0);
        assert_eq!(loaded.http_credentials, old.http_credentials);
        let mut next = old.clone();
        next.http_credentials.as_mut().unwrap()[0].path_prefix = "/other".into();
        assert!(update_requires_review(&old, &next) && credentials_changed(&old, &next));
        next = old.clone();
        next.settings[2].key = "retired".into();
        assert!(credentials_changed(&old, &next));
        for key in ["bearer_secret", "path_prefix"] {
            let mut invalid = manifest.clone();
            invalid["http_credentials"][0][key] = serde_json::json!("missing");
            assert!(parse_package(&serde_json::to_vec(&invalid).unwrap()).is_err());
        }
        let mut invalid = manifest.clone();
        invalid["http_credentials"][0]["token"] = serde_json::json!("token");
        assert!(parse_package(&serde_json::to_vec(&invalid).unwrap()).is_err());
        invalid = manifest.clone();
        invalid["error_codes"] = serde_json::json!(["bad code"]);
        assert!(parse_package(&serde_json::to_vec(&invalid).unwrap()).is_err());
        manifest.as_object_mut().unwrap().remove("http_credentials");
        assert!(parse_package(&serde_json::to_vec(&manifest).unwrap()).is_err());
        manifest["http_credentials"] = serde_json::json!([]);
        let explicit_empty = parse_package(&serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert_eq!(explicit_empty.http_credentials, Some(vec![]));
    }
    #[test]
    fn credential_review_never_reactivates_secrets_that_were_not_reentered() {
        let package: PluginPackage = serde_json::from_value(serde_json::json!({
            "id":"fixture", "name":"Fixture", "version":"1.0.0", "api_version":"1.1.0", "storage_schema_version":1,
            "http_credentials":[], "settings":[{"key":"token","type":"secret","label":"Token"},{"key":"other_token","type":"secret","label":"Other"}]
        })).unwrap();
        let mut previous = instance();
        previous.secret_fields = vec!["token".into(), "other_token".into()];
        previous.credentials_need_review = true;
        let secrets = BTreeMap::from([("token".into(), "fixture-new-token".into())]);
        assert_eq!(
            updated_secret_fields(&package, Some(&previous), &secrets).unwrap(),
            vec!["token"]
        );
        assert!(
            updated_secret_fields(&package, Some(&previous), &BTreeMap::new())
                .unwrap()
                .is_empty()
        );
        previous.credentials_need_review = false;
        assert_eq!(
            updated_secret_fields(&package, Some(&previous), &secrets).unwrap(),
            vec!["other_token", "token"]
        );
        for invalid in [
            BTreeMap::from([("missing".into(), "fixture".into())]),
            BTreeMap::from([("token".into(), "x".repeat(8193))]),
            BTreeMap::from([("token".into(), "bad\r\nX-Injected: yes".into())]),
            BTreeMap::from([("token".into(), "bad\0".into())]),
        ] {
            assert!(updated_secret_fields(&package, Some(&previous), &invalid).is_err());
        }
    }
    #[test]
    fn package_ids_cannot_alias_windows_paths() {
        for invalid in [
            "..", "...", "foo.", "CON", "con", "nul", "com1", "lpt9", "a/b", "a\\b", "../a",
        ] {
            assert!(!valid_id(invalid), "{invalid}");
        }
        assert!(valid_id("example-importer"));
    }
    #[test]
    fn uninstall_detaches_sources_and_runs_atomically() {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        storage::initialize(&c).unwrap();
        let i = instance();
        storage::put(&c, "instance", &i.id, &i).unwrap();
        storage::put(&c, "cursor", &i.id, &Some("cursor")).unwrap();
        c.execute(
            "INSERT INTO cardbe_plugin_sources VALUES('a','source',1,2,'{}')",
            [],
        )
        .unwrap();
        storage::put(&c, "run", "run-1", &serde_json::json!({"instance_id":"a"})).unwrap();
        c.execute_batch("CREATE TRIGGER refuse_detach BEFORE DELETE ON cardbe_plugin_sources BEGIN SELECT RAISE(ABORT,'test'); END;").unwrap();
        assert!(remove_locked(&c, std::slice::from_ref(&i), &BTreeMap::new(), None).is_err());
        let count: i64 = c
            .query_row("SELECT count(*) FROM cardbe_plugin_data", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 3);
        c.execute_batch("DROP TRIGGER refuse_detach;").unwrap();
        remove_locked(&c, &[i], &BTreeMap::new(), None).unwrap();
        let count: i64 = c
            .query_row("SELECT count(*) FROM cardbe_plugin_data", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
    #[test]
    fn reinstall_clears_files_left_by_an_uninstalled_package_inside_the_root() {
        let base = std::env::temp_dir().join(format!("cardbe-orphan-test-{}", id()));
        let root = base.join("plugins");
        let destination = root.join("example");
        std::fs::create_dir_all(destination.join("revision")).unwrap();
        std::fs::write(destination.join("revision").join("component.wasm"), b"old").unwrap();
        clear_orphaned_package(&root, &destination).unwrap();
        assert!(!destination.exists());
        clear_orphaned_package(&root, &destination).unwrap();
        std::fs::write(&destination, b"file").unwrap();
        assert!(clear_orphaned_package(&root, &destination).is_err());
        assert!(destination.exists());
        std::fs::remove_file(&destination).unwrap();
        let outside = base.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        assert!(clear_orphaned_package(&root, &outside).is_err());
        assert!(outside.exists());
        std::fs::remove_dir_all(base).unwrap();
    }
}
