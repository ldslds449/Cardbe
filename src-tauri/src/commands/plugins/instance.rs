use super::*;

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
pub(super) fn retained_private_network_domains(
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

pub(super) fn updated_secret_fields(
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

pub(super) fn resolve_domain(
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

pub(super) fn change_enabled(
    instance: &mut PluginInstance,
    enabled: bool,
) -> Result<(), CommandError> {
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

pub(super) fn remove_locked(
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
