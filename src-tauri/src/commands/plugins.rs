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
fn valid_domains(domains: &[String]) -> bool {
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
#[tauri::command]
pub fn get_plugin_state(app: tauri::AppHandle) -> Result<PluginState, CommandError> {
    manager::snapshot(&app).map_err(CommandError::from)
}
#[tauri::command]
pub async fn install_plugin_package(
    app: tauri::AppHandle,
    path: Option<String>,
    url: Option<String>,
) -> Result<PluginPackage, CommandError> {
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
        install_package(app, bytes, component)
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
fn install_package(
    app: tauri::AppHandle,
    bytes: Vec<u8>,
    component: Vec<u8>,
) -> Result<PluginPackage, CommandError> {
    let package: PluginPackage =
        serde_json::from_slice(&bytes).map_err(|_| CommandError::InvalidArgument)?;
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
    if g.database
        .board_role(instance.board_id)
        .map_err(CommandError::repository)?
        == crate::models::BoardRole::Viewer
    {
        return Err(CommandError::PermissionDenied);
    }
    if !valid_domains(&instance.allowed_domains)
        || instance
            .allowed_domains
            .iter()
            .any(|d| !package.domains.contains(d))
    {
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
    let saved = PluginInstance {
        id: id.clone(),
        plugin_id: instance.plugin_id,
        name: instance.name,
        board_id: instance.board_id,
        config: instance.config,
        secret_fields,
        allowed_domains: instance.allowed_domains,
        enabled: instance.enabled,
        interval_seconds: instance.interval_seconds,
        last_run_at: previous.and_then(|p| p.last_run_at),
        last_run_status: previous.and_then(|p| p.last_run_status.clone()),
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
    instance.enabled = enabled;
    instance.failures = 0;
    instance.next_run_at = Some(now());
    storage::put(c, "instance", &instance.id, &instance)?;
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
    fn instance() -> PluginInstance {
        PluginInstance {
            id: "a".into(),
            plugin_id: "example".into(),
            name: "Example".into(),
            board_id: 1,
            config: Default::default(),
            secret_fields: vec![],
            allowed_domains: vec![],
            enabled: false,
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
