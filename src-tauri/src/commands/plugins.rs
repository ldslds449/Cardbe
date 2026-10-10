use crate::{
    errors::CommandError,
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
        && unique_values(domains.iter().map(String::as_str))
        && domains.iter().all(|domain| {
            domain.len() <= 253
                && domain.split('.').all(|label| {
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
            (package.domains.contains(domain) || package.allow_custom_domains)
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
            > 20 * 1024 * 1024
        {
            return Err(CommandError::InvalidArgument);
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
    if manifest
        .canonicalize()
        .map_err(CommandError::internal)?
        .parent()
        != Some(root.as_path())
        || component
            .canonicalize()
            .map_err(CommandError::internal)?
            .parent()
            != Some(root.as_path())
        || std::fs::metadata(&manifest)
            .map_err(CommandError::internal)?
            .len()
            > 65536
        || std::fs::metadata(&component)
            .map_err(CommandError::internal)?
            .len()
            > 16 * 1024 * 1024
    {
        return Err(CommandError::InvalidArgument);
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
fn validate_update(old: &PluginPackage, new: &PluginPackage) -> Result<(), CommandError> {
    if old.id != new.id
        || old.api_version != new.api_version
        || old.storage_schema_version != new.storage_schema_version
    {
        return Err(CommandError::InvalidArgument);
    }
    let old_version = release_version(&old.version).ok_or(CommandError::InvalidArgument)?;
    let new_version = release_version(&new.version).ok_or(CommandError::InvalidArgument)?;
    if new_version <= old_version {
        return Err(CommandError::PluginUpdateNotNewer);
    }
    Ok(())
}
fn update_requires_review(old: &PluginPackage, new: &PluginPackage) -> bool {
    old.domains != new.domains
        || old.allow_custom_domains != new.allow_custom_domains
        || serde_json::to_value(&old.settings).ok() != serde_json::to_value(&new.settings).ok()
}
fn credentials_changed(old: &PluginPackage, new: &PluginPackage) -> bool {
    let old_secret = old.settings.iter().find(|field| field.kind == "secret");
    old_secret.is_some_and(|field| {
        old.domains != new.domains
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
    crate::plugins::runtime::validate_component(&component)
        .map_err(|_| CommandError::InvalidArgument)?;
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
        if update_requires_review(&old, &update.package) {
            for mut instance in storage::list::<PluginInstance>(&transaction, "instance")?
                .into_iter()
                .filter(|i| i.plugin_id == old.id)
            {
                instance.enabled = false;
                instance.needs_review = true;
                instance.pending_domain = None;
                instance.credentials_need_review |= credentials_changed(&old, &update.package);
                instance.next_run_at = None;
                storage::put(&transaction, "instance", &instance.id, &instance)?;
            }
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
fn parse_package(bytes: &[u8]) -> Result<PluginPackage, CommandError> {
    if bytes.len() > 65536 {
        return Err(CommandError::InvalidArgument);
    }
    let package: PluginPackage =
        serde_json::from_slice(bytes).map_err(|_| CommandError::InvalidArgument)?;
    if !valid_id(&package.id)
        || package.api_version != "1.0.0"
        || package.storage_schema_version != 1
        || package.settings.len() > 32
        || !valid_domains(&package.domains)
    {
        return Err(CommandError::InvalidArgument);
    }
    if package.error_codes.len() > 64
        || package.log_codes.len() > 64
        || package
            .error_codes
            .iter()
            .chain(&package.log_codes)
            .any(|c| !valid_code(c))
        || package
            .settings
            .iter()
            .filter(|f| f.kind == "secret")
            .count()
            > 1
        || (package.settings.iter().any(|f| f.kind == "secret") && package.domains.len() != 1)
    {
        return Err(CommandError::InvalidArgument);
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
        {
            return Err(CommandError::InvalidArgument);
        }
    }
    Ok(package)
}
fn install_package(
    app: tauri::AppHandle,
    bytes: Vec<u8>,
    component: Vec<u8>,
) -> Result<PluginPackage, CommandError> {
    let package = parse_package(&bytes)?;
    let destination = manager::package_root(&app)
        .map_err(CommandError::from)?
        .join(&package.id);
    let s = app.state::<SharedAppData>();
    let g = s.lock().map_err(CommandError::internal)?;
    let c = g.database.plugin_connection();
    let installed = storage::list::<PluginPackage>(c, "package")?;
    if installed.len() >= 50 || installed.iter().any(|p| p.id == package.id) {
        return Err(CommandError::InvalidArgument);
    }
    let directory = destination.parent().ok_or(CommandError::InvalidArgument)?;
    std::fs::create_dir_all(directory).map_err(CommandError::internal)?;
    if destination.exists() {
        return Err(CommandError::InvalidArgument);
    }
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
    let credentials_need_review = previous.is_some_and(|p| p.credentials_need_review)
        && secrets.values().all(|value| value.is_empty());
    let mut secret_fields = previous
        .map(|p| p.secret_fields.clone())
        .unwrap_or_default();
    for (key, value) in &secrets {
        if !package
            .settings
            .iter()
            .any(|f| f.key == *key && f.kind == "secret")
            || value.len() > 8192
        {
            return Err(CommandError::InvalidArgument);
        }
        secret_fields.retain(|k| k != key);
        if !value.is_empty() {
            secret_fields.push(key.clone());
        }
    }
    for f in &package.settings {
        if f.required
            && if f.kind == "secret" {
                credentials_need_review || !secret_fields.contains(&f.key)
            } else {
                !instance.config.contains_key(&f.key)
            }
        {
            return Err(CommandError::InvalidArgument);
        }
    }
    let reset_cursor =
        previous.is_some_and(|p| p.board_id != instance.board_id || p.config != instance.config);
    let saved = PluginInstance {
        id: id.clone(),
        plugin_id: instance.plugin_id,
        name: instance.name,
        board_id: instance.board_id,
        config: instance.config,
        secret_fields,
        allowed_domains: instance.allowed_domains,
        pending_domain: None,
        enabled: instance.enabled,
        needs_review: false,
        credentials_need_review,
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
#[tauri::command]
pub fn resolve_plugin_domain(
    app: tauri::AppHandle,
    instance_id: String,
    run_id: String,
    allow: bool,
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
    {
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
        resolve_domain(&mut instance, &package, &run_id, allow)?;
        storage::put(connection, "instance", &instance.id, &instance)?;
    }
    let _ = app.emit("cardbe:plugins-changed", ());
    if allow {
        tauri::async_runtime::spawn(async move {
            let _ = manager::run(app, instance_id, "permission").await;
        });
    }
    Ok(())
}

fn resolve_domain(
    instance: &mut PluginInstance,
    package: &PluginPackage,
    run_id: &str,
    allow: bool,
) -> Result<(), CommandError> {
    let pending = instance
        .pending_domain
        .as_ref()
        .filter(|pending| pending.run_id == run_id)
        .ok_or(CommandError::InvalidArgument)?;
    if instance.needs_review || !instance.enabled {
        return Err(CommandError::PermissionDenied);
    }
    if allow {
        let mut domains = instance.allowed_domains.clone();
        if !domains.contains(&pending.domain) {
            domains.push(pending.domain.clone());
        }
        if !valid_instance_domains(package, &domains) {
            return Err(CommandError::PermissionDenied);
        }
        instance.allowed_domains = domains;
        instance.next_run_at = Some(now().saturating_add(60_000));
    } else {
        // A declined request pauses this instance until the user enables it again.
        instance.enabled = false;
        instance.next_run_at = None;
        instance.last_run_status = Some("cancelled".into());
    }
    instance.pending_domain = None;
    Ok(())
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
    storage::put(c, "instance", &instance.id, &instance)?;
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
    manager::run(app, instance_id, "manual")
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
        &BTreeMap::from([(instance_id, namespace)]),
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
    if let Some(resolved) = resolved {
        std::fs::remove_dir_all(resolved).map_err(CommandError::internal)?;
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
                "api_version":"1.0.0", "storage_schema_version":1,
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
        for field in ["id", "api_version", "storage_schema_version"] {
            let mut next = package("2.0.0");
            match field {
                "id" => next.id = "other".into(),
                "api_version" => next.api_version = "2.0.0".into(),
                _ => next.storage_schema_version = 2,
            }
            assert!(matches!(
                validate_update(&old, &next),
                Err(CommandError::InvalidArgument)
            ));
        }
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
        let old = parse_package(br#"{"id":"example","name":"Example","version":"1.0.0","api_version":"1.0.0","storage_schema_version":1,"domains":["api.example.com"],"settings":[{"key":"token","type":"secret","label":"Token","required":true}]}"#).unwrap();
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
            "id":"example", "name":"Example", "version":"1.0.0", "api_version":"1.0.0",
            "storage_schema_version":1, "domains":["api.example.com"]
        }))
        .unwrap();
        let mut pending = instance();
        pending.enabled = true;
        pending.pending_domain = Some(PendingDomain {
            domain: "api.example.com".into(),
            run_id: "run-1".into(),
            trigger: "manual".into(),
        });
        assert!(resolve_domain(&mut pending.clone(), &package, "stale-run", true).is_err());
        let mut needs_review = pending.clone();
        needs_review.needs_review = true;
        assert!(resolve_domain(&mut needs_review, &package, "run-1", true).is_err());
        let mut disabled = pending.clone();
        disabled.enabled = false;
        assert!(resolve_domain(&mut disabled, &package, "run-1", true).is_err());
        let mut other_host = pending.clone();
        other_host.pending_domain.as_mut().unwrap().domain = "other.example.com".into();
        assert!(resolve_domain(&mut other_host, &package, "run-1", true).is_err());
        let mut full = pending.clone();
        full.allowed_domains = (0..16).map(|i| format!("host{i}.example.com")).collect();
        let mut custom_package = package.clone();
        custom_package.allow_custom_domains = true;
        assert!(resolve_domain(&mut full, &custom_package, "run-1", true).is_err());
        let mut rejected = pending.clone();
        resolve_domain(&mut rejected, &package, "run-1", false).unwrap();
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
        resolve_domain(&mut restored, &package, "run-1", true).unwrap();
        storage::put(&connection, "instance", "pending", &restored).unwrap();
        let restored: PluginInstance = storage::list(&connection, "instance").unwrap().remove(0);
        assert_eq!(restored.allowed_domains, ["api.example.com"]);
        assert!(restored.enabled && restored.pending_domain.is_none());
        assert!(instance().allowed_domains.is_empty());
        assert!(resolve_domain(&mut restored.clone(), &package, "run-1", true).is_err());
        let mut legacy = serde_json::to_value(instance()).unwrap();
        legacy.as_object_mut().unwrap().remove("pending_domain");
        assert!(serde_json::from_value::<PluginInstance>(legacy)
            .unwrap()
            .pending_domain
            .is_none());
    }
    #[test]
    fn manifest_domains_and_select_values_are_unique_and_valid() {
        assert!(valid_domains(&["api.github.com".into()]));
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
            "api_version": "1.0.0", "storage_schema_version": 1,
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
            vec!["example.com:443".into()],
            vec!["*.example.com".into()],
            vec!["custom.example.com".into(), "custom.example.com".into()],
            (0..17).map(|i| format!("host{i}.example.com")).collect(),
        ] {
            assert!(!valid_instance_domains(&package, &domains));
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
}
