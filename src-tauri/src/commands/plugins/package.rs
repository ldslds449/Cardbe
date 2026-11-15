use super::*;

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

pub(super) fn read_local_package(path: String) -> Result<(Vec<u8>, Vec<u8>), CommandError> {
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
pub(super) fn release_version(value: &str) -> Option<[u64; 3]> {
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
pub(super) const SUPPORTED_API_VERSIONS: [&str; 3] = ["1.1.0", "1.2.0", "1.3.0"];

pub(super) fn validate_update(
    old: &PluginPackage,
    new: &PluginPackage,
) -> Result<(), CommandError> {
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

pub(super) fn update_requires_review(old: &PluginPackage, new: &PluginPackage) -> bool {
    old.manifest_version != new.manifest_version
        || old.domains != new.domains
        || old.allow_custom_domains != new.allow_custom_domains
        || old.http_credentials != new.http_credentials
        || serde_json::to_value(&old.settings).ok() != serde_json::to_value(&new.settings).ok()
}

pub(super) fn credentials_changed(old: &PluginPackage, new: &PluginPackage) -> bool {
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

pub(super) fn update_candidate(
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

pub(super) fn validate_component(component: &[u8]) -> Result<(), CommandError> {
    crate::plugins::runtime::validate_component(component)
        .map_err(|_| CommandError::plugin_package(PluginPackageIssue::InvalidComponent))
}

pub(super) fn select_package(
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

pub(super) fn clean_revision(directory: &std::path::Path) {
    for file in ["manifest.json", "component.wasm"] {
        let _ = std::fs::remove_file(directory.join(file));
    }
    if std::fs::remove_dir(directory).is_err() {
        log::warn!(target: "plugins", "Could not clean up inactive plugin revision");
    }
}

pub(super) fn update_locked(
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
pub(super) fn valid_default(field: &SettingField) -> bool {
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
pub(super) fn unsupported_api_version(package: &PluginPackage) -> CommandError {
    CommandError::PluginPackageInvalid {
        reason: PluginPackageIssue::UnsupportedApiVersion,
        setting: None,
        api_version: release_version(&package.api_version).map(|_| package.api_version.clone()),
    }
}

pub(super) fn parse_package(bytes: &[u8]) -> Result<PluginPackage, CommandError> {
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
pub(super) fn clear_orphaned_package(
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

pub(super) fn install_package(
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
