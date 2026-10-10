use super::{
    http::HttpPolicy,
    runtime::{self, RuntimeInput, RuntimeOutput},
    storage,
    types::*,
};
use crate::{
    errors::{CommandError, DomainError, PluginHostError, PluginUserError},
    state::SharedAppData,
};
use rusqlite::OptionalExtension;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::{Emitter, Manager};

#[derive(Debug)]
pub enum ManagerError {
    Domain(DomainError),
    Host(PluginHostError),
    AuthorizationRequired(String),
}
impl From<DomainError> for ManagerError {
    fn from(error: DomainError) -> Self {
        Self::Domain(error)
    }
}
impl From<PluginHostError> for ManagerError {
    fn from(error: PluginHostError) -> Self {
        Self::Host(error)
    }
}
impl From<ManagerError> for CommandError {
    fn from(error: ManagerError) -> Self {
        match error {
            ManagerError::Domain(error) => error.into(),
            ManagerError::Host(error) => error.into(),
            ManagerError::AuthorizationRequired(_) => CommandError::PermissionDenied,
        }
    }
}
fn internal(error: impl std::fmt::Display) -> DomainError {
    DomainError::Internal(error.to_string())
}
fn lock_error() -> DomainError {
    internal("Plugin state lock unavailable")
}
#[derive(Default)]
pub struct PluginManager {
    pub active: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub update: Mutex<Option<PreparedPluginUpdate>>,
    pub safe_mode: AtomicBool,
    initialized: AtomicBool,
}
pub fn package_root(app: &tauri::AppHandle) -> Result<PathBuf, ManagerError> {
    let state = app.state::<SharedAppData>();
    let guard = state.lock().map_err(|_| lock_error())?;
    Ok(guard
        .database
        .path()
        .parent()
        .ok_or(DomainError::InvalidArgument)?
        .join("plugins"))
}
pub fn secret_namespace(app: &tauri::AppHandle, instance: &str) -> Result<String, ManagerError> {
    let state = app.state::<SharedAppData>();
    let guard = state.lock().map_err(|_| lock_error())?;
    Ok(secret_namespace_for_path(guard.database.path(), instance))
}
pub fn secret_namespace_for_path(path: &std::path::Path, instance: &str) -> String {
    let digest = Sha256::digest(path.to_string_lossy().as_bytes());
    format!("{digest:x}:{instance}")
}
fn recover(runs: &mut [PluginRun], instances: &mut [PluginInstance], at: i64) {
    for run in runs.iter_mut().filter(|run| run.status == "running") {
        run.status = "interrupted".into();
        run.finished_at = Some(at);
        if let Some(instance) = instances
            .iter_mut()
            .find(|instance| instance.id == run.instance_id)
        {
            instance.enabled = false;
            instance.running = false;
            instance.last_run_status = Some("interrupted".into());
        }
    }
}
pub fn initialize(app: &tauri::AppHandle) -> Result<(), ManagerError> {
    let manager = app.state::<PluginManager>();
    manager.safe_mode.store(true, Ordering::SeqCst);
    let state = app.state::<SharedAppData>();
    let guard = state.lock().map_err(|_| lock_error())?;
    guard.database.initialize_plugins()?;
    let connection = guard.database.plugin_connection();
    let mut instances: Vec<PluginInstance> = storage::list(connection, "instance")?;
    let mut runs: Vec<PluginRun> = storage::list(connection, "run")?;
    recover(&mut runs, &mut instances, now());
    let transaction = connection.unchecked_transaction().map_err(internal)?;
    for run in &runs {
        storage::put(&transaction, "run", &run.id, run)?;
    }
    for instance in &instances {
        storage::put(&transaction, "instance", &instance.id, instance)?;
    }
    transaction.commit().map_err(internal)?;
    let persisted: Vec<bool> = storage::list(connection, "safe_mode")?;
    manager.safe_mode.store(
        std::env::var_os("CARDBE_PLUGIN_SAFE_MODE").is_some() || persisted.contains(&true),
        Ordering::SeqCst,
    );
    manager.initialized.store(true, Ordering::SeqCst);
    Ok(())
}
pub fn snapshot(app: &tauri::AppHandle) -> Result<PluginState, ManagerError> {
    let state = app.state::<SharedAppData>();
    let guard = state.lock().map_err(|_| lock_error())?;
    let connection = guard.database.plugin_connection();
    let mut instances: Vec<PluginInstance> = storage::list(connection, "instance")?;
    let manager = app.state::<PluginManager>();
    let active = manager.active.lock().map_err(|_| lock_error())?;
    for instance in &mut instances {
        instance.running = active.contains_key(&instance.id);
    }
    Ok(PluginState {
        packages: storage::list(connection, "package")?,
        instances,
        safe_mode: manager.safe_mode.load(Ordering::SeqCst)
            || !manager.initialized.load(Ordering::SeqCst),
    })
}
fn reserve(
    active: &mut HashMap<String, Arc<AtomicBool>>,
    id: &str,
    cancel: Arc<AtomicBool>,
    wait_for_capacity: bool,
) -> Result<bool, DomainError> {
    if active.contains_key(id) {
        return Err(DomainError::InvalidArgument);
    }
    if active.len() >= 2 {
        return if wait_for_capacity {
            Ok(false)
        } else {
            Err(DomainError::InvalidArgument)
        };
    }
    active.insert(id.to_owned(), cancel);
    Ok(true)
}
struct Reservation {
    app: tauri::AppHandle,
    id: String,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        if let Ok(mut active) = self.app.state::<PluginManager>().active.lock() {
            active.remove(&self.id);
        }
        let _ = self.app.emit("cardbe:plugins-changed", ());
    }
}
fn install_committed(
    guard: &mut crate::state::AppData,
    board_id: i64,
    mut stored: crate::models::StoredData,
    changed: bool,
) {
    if !changed || guard.active_board_id != board_id {
        return;
    }
    stored.settings = guard.stored.settings.clone();
    if !guard.archives_loaded {
        stored.archives.clear();
    }
    guard.stored = stored;
    guard.undo_history.clear();
    guard.refresh_labels();
}
fn disable_preparation(
    connection: &rusqlite::Connection,
    instance: &PluginInstance,
    trigger: &str,
    error: CommandError,
) -> Result<(), DomainError> {
    let at = now();
    let mut instance = instance.clone();
    instance.enabled = false;
    instance.last_error = Some(error.clone());
    instance.last_run_at = Some(at);
    instance.last_run_status = Some("failed".into());
    instance.next_run_at = None;
    let run = PluginRun {
        id: id(),
        instance_id: instance.id.clone(),
        trigger: trigger.into(),
        started_at: at,
        finished_at: Some(at),
        status: "failed".into(),
        error: Some(error),
        logs: vec![],
        created: 0,
        updated: 0,
    };
    let transaction = connection.unchecked_transaction().map_err(internal)?;
    storage::put(&transaction, "instance", &instance.id, &instance)?;
    storage::put(&transaction, "run", &run.id, &run)?;
    storage::retain_runs(&transaction, &instance.id)?;
    transaction.commit().map_err(internal)
}
fn same_authorization(original: &PluginInstance, current: &PluginInstance) -> bool {
    current.enabled
        && !current.needs_review
        && current.plugin_id == original.plugin_id
        && current.board_id == original.board_id
        && current.config == original.config
        && current.allowed_domains == original.allowed_domains
        && current.secret_fields == original.secret_fields
}
fn next_run(at: i64, interval: u64, failures: u32, retry: Option<u32>) -> Option<i64> {
    if interval == 0 {
        return None;
    }
    let delay = interval
        .max(60)
        .saturating_mul(1u64 << failures.min(8))
        .min(86400);
    Some(
        at.saturating_add(
            delay
                .max(u64::from(retry.unwrap_or(0).clamp(0, 86400)))
                .saturating_mul(1000) as i64,
        ),
    )
}
fn runtime_error(
    plugin: &PluginPackage,
    error: runtime::RuntimeError,
) -> (ManagerError, Option<u32>) {
    match error {
        runtime::RuntimeError::AuthorizationRequired(domain) => {
            (ManagerError::AuthorizationRequired(domain), None)
        }
        runtime::RuntimeError::Guest {
            code,
            retry_after_seconds,
        } if plugin.error_codes.contains(&code) => {
            let retry = retry_after_seconds.map(|seconds| seconds.clamp(60, 86400));
            let mut params = serde_json::Map::new();
            if let Some(seconds) = retry {
                params.insert("retry_after_seconds".into(), seconds.into());
            }
            if let Some(error) = PluginUserError::new(code, params) {
                (
                    PluginHostError::User {
                        plugin_id: plugin.id.clone(),
                        error,
                    }
                    .into(),
                    retry,
                )
            } else {
                (
                    PluginHostError::ExecutionFailed {
                        plugin_id: plugin.id.clone(),
                        source: "Invalid guest error code".into(),
                    }
                    .into(),
                    None,
                )
            }
        }
        runtime::RuntimeError::PermissionDenied => (
            PluginHostError::PermissionDenied {
                plugin_id: plugin.id.clone(),
                permission: "network".into(),
            }
            .into(),
            None,
        ),
        error => (
            PluginHostError::ExecutionFailed {
                plugin_id: plugin.id.clone(),
                source: error.code().to_owned(),
            }
            .into(),
            None,
        ),
    }
}
async fn execute(
    app: &tauri::AppHandle,
    instance: &PluginInstance,
    package: &PluginPackage,
    input: RuntimeInput,
    cancel: Arc<AtomicBool>,
) -> Result<RuntimeOutput, (ManagerError, Option<u32>)> {
    let preparation = (|| -> Result<_, ManagerError> {
        let root = package_root(app)?.join(&package.id);
        let path = {
            let state = app.state::<SharedAppData>();
            let guard = state.lock().map_err(|_| lock_error())?;
            match storage::revision(guard.database.plugin_connection(), &package.id)? {
                Some(revision) => root.join(revision).join("component.wasm"),
                None => root.join("component.wasm"),
            }
        };
        let namespace = secret_namespace(app, &instance.id)?;
        let token = credential_key(instance, package)
            .map(|key| secret(&namespace, key))
            .transpose()?
            .flatten();
        let policy = HttpPolicy {
            allowed_domains: instance.allowed_domains.clone(),
            requested_domains: package.domains.clone(),
            allow_custom_domains: package.allow_custom_domains,
            token,
            token_domain: package.domains.first().cloned(),
            error_codes: package.error_codes.clone(),
            log_codes: package.log_codes.clone(),
        };
        Ok((path, policy))
    })();
    let (path, policy) = preparation.map_err(|error| (error, None))?;
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = std::fs::read(path).map_err(|_| runtime::RuntimeError::InvalidComponent)?;
        runtime::execute(&bytes, input, policy, cancel)
    })
    .await
    .map_err(|_| {
        (
            ManagerError::Host(PluginHostError::ExecutionFailed {
                plugin_id: package.id.clone(),
                source: "Plugin worker failed".into(),
            }),
            None,
        )
    })?
    .map_err(|error| runtime_error(package, error))
}
fn credential_key<'a>(instance: &'a PluginInstance, package: &PluginPackage) -> Option<&'a str> {
    instance
        .secret_fields
        .iter()
        .find(|key| {
            !instance.credentials_need_review
                && package
                    .settings
                    .iter()
                    .any(|field| field.kind == "secret" && field.key == **key)
        })
        .map(String::as_str)
}
pub async fn run(
    app: tauri::AppHandle,
    instance_id: String,
    trigger: &str,
) -> Result<PluginRun, ManagerError> {
    let manager = app.state::<PluginManager>();
    let cancel = Arc::new(AtomicBool::new(false));
    loop {
        if manager.safe_mode.load(Ordering::SeqCst) || !manager.initialized.load(Ordering::SeqCst) {
            return Err(DomainError::PermissionDenied.into());
        }
        let reserved = {
            let mut active = manager.active.lock().map_err(|_| lock_error())?;
            reserve(
                &mut active,
                &instance_id,
                cancel.clone(),
                trigger == "permission",
            )?
        };
        if reserved {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    let _reservation = Reservation {
        app: app.clone(),
        id: instance_id.clone(),
    };
    let (instance, package, input, mut record) = {
        let state = app.state::<SharedAppData>();
        let guard = state.lock().map_err(|_| lock_error())?;
        let connection = guard.database.plugin_connection();
        let instance = storage::list::<PluginInstance>(connection, "instance")?
            .into_iter()
            .find(|instance| instance.id == instance_id)
            .ok_or(DomainError::InvalidArgument)?;
        if !instance.enabled || instance.needs_review || instance.pending_domain.is_some() {
            return Err(DomainError::PermissionDenied.into());
        }
        match guard
            .database
            .board_role(instance.board_id)
            .map_err(DomainError::repository)
        {
            Ok(crate::models::BoardRole::Viewer) => {
                disable_preparation(
                    connection,
                    &instance,
                    trigger,
                    CommandError::PermissionDenied,
                )?;
                return Err(DomainError::PermissionDenied.into());
            }
            Err(DomainError::BoardNotFound) => {
                disable_preparation(connection, &instance, trigger, CommandError::BoardNotFound)?;
                return Err(DomainError::BoardNotFound.into());
            }
            Err(error) => return Err(error.into()),
            Ok(_) => {}
        }
        let package = storage::list::<PluginPackage>(connection, "package")?
            .into_iter()
            .find(|package| package.id == instance.plugin_id);
        let Some(package) = package else {
            disable_preparation(
                connection,
                &instance,
                trigger,
                CommandError::InvalidArgument,
            )?;
            return Err(DomainError::InvalidArgument.into());
        };
        if !crate::commands::plugins::valid_instance_domains(&package, &instance.allowed_domains) {
            disable_preparation(
                connection,
                &instance,
                trigger,
                CommandError::PermissionDenied,
            )?;
            return Err(DomainError::PermissionDenied.into());
        }
        let board = guard
            .database
            .read_board(instance.board_id)
            .map_err(DomainError::repository)?;
        let raw: Option<String> = connection
            .query_row(
                "SELECT payload FROM cardbe_plugin_data WHERE kind='cursor' AND id=?1",
                [&instance.id],
                |row| row.get(0),
            )
            .optional()
            .map_err(internal)?;
        let record = PluginRun {
            id: id(),
            instance_id: instance.id.clone(),
            trigger: trigger.into(),
            started_at: now(),
            finished_at: None,
            status: "running".into(),
            error: None,
            logs: Vec::new(),
            created: 0,
            updated: 0,
        };
        let input = RuntimeInput {
            instance_id: instance.id.clone(),
            run_id: record.id.clone(),
            config_json: serde_json::to_string(&instance.config).map_err(internal)?,
            board_json: serde_json::to_string(&board.columns).map_err(internal)?,
            cursor: raw
                .map(|value| serde_json::from_str(&value))
                .transpose()
                .map_err(internal)?
                .flatten(),
        };
        storage::put(connection, "run", &record.id, &record)?;
        (instance, package, input, record)
    };
    let result = execute(&app, &instance, &package, input, cancel.clone()).await;
    let mut changed = false;
    let finish = (|| -> Result<(), ManagerError> {
        let state = app.state::<SharedAppData>();
        let mut guard = state.lock().map_err(|_| lock_error())?;
        let mut current =
            storage::list::<PluginInstance>(guard.database.plugin_connection(), "instance")?
                .into_iter()
                .find(|current| current.id == instance.id);
        let mut retry = None;
        if cancel.load(Ordering::SeqCst)
            || manager.safe_mode.load(Ordering::SeqCst)
            || current
                .as_ref()
                .is_none_or(|current| !same_authorization(&instance, current))
        {
            record.status = "cancelled".into();
        } else {
            match result {
                Ok(output) => {
                    let current = current
                        .as_ref()
                        .expect("Authorization checked instance existence");
                    match guard.database.commit_plugin_output(current, &output, || {
                        !cancel.load(Ordering::SeqCst) && !manager.safe_mode.load(Ordering::SeqCst)
                    }) {
                        Ok((created, updated, stored)) => {
                            record.status = "success".into();
                            record.created = created;
                            record.updated = updated;
                            record.logs = output
                                .logs
                                .into_iter()
                                .take(100)
                                .map(|log| RunLog {
                                    level: "info".into(),
                                    code: log.code,
                                    count: log.count,
                                })
                                .collect();
                            changed = created + updated > 0;
                            install_committed(&mut guard, instance.board_id, stored, changed);
                            if changed {
                                match guard.database.boards() {
                                    Ok(boards) => guard.boards = boards,
                                    Err(error) => {
                                        log::warn!(target: "plugins", "{}", crate::commands::diagnostics::redact_diagnostic_logs(error.to_string()))
                                    }
                                }
                            }
                        }
                        Err(_)
                            if cancel.load(Ordering::SeqCst)
                                || manager.safe_mode.load(Ordering::SeqCst) =>
                        {
                            record.status = "cancelled".into();
                        }
                        Err(error) => {
                            record.status = "failed".into();
                            record.error = Some(error.into());
                        }
                    }
                }
                Err((ManagerError::AuthorizationRequired(domain), _)) => {
                    record.status = "awaiting_permission".into();
                    let current = current
                        .as_mut()
                        .expect("Authorization checked instance existence");
                    current.last_error = None;
                    current.pending_domain = Some(PendingDomain {
                        domain,
                        run_id: record.id.clone(),
                        trigger: record.trigger.clone(),
                    });
                }
                Err((error, wait)) => {
                    record.status = "failed".into();
                    record.error = Some(error.into());
                    retry = wait;
                }
            }
        }
        record.finished_at = Some(now());
        let connection = guard.database.plugin_connection();
        let transaction = connection.unchecked_transaction().map_err(internal)?;
        if let Some(current) = current.as_mut() {
            if record.status == "failed" {
                current.failures = current.failures.saturating_add(1);
                current.last_error = record.error.clone();
                if current.failures >= 3 {
                    current.enabled = false;
                }
            } else if record.status == "success" {
                current.failures = 0;
                current.last_error = None;
            }
            current.last_run_at = record.finished_at;
            current.last_run_status = Some(record.status.clone());
            current.next_run_at = if current.pending_domain.is_some() {
                None
            } else {
                next_run(
                    record.finished_at.unwrap_or_else(now),
                    current.interval_seconds,
                    current.failures,
                    retry,
                )
            };
            storage::put(&transaction, "instance", &current.id, current)?;
        }
        storage::put(&transaction, "run", &record.id, &record)?;
        storage::retain_runs(&transaction, &instance.id)?;
        transaction.commit().map_err(internal)?;
        Ok(())
    })();
    if changed {
        let _ = app.emit(
            "cardbe:data-changed",
            serde_json::json!({"kind":"task","boardId":instance.board_id}),
        );
    }
    match finish {
        Ok(()) => Ok(record),
        Err(error) => {
            // A failed history write cannot be recovered safely until the next startup.
            manager.safe_mode.store(true, Ordering::SeqCst);
            manager.initialized.store(false, Ordering::SeqCst);
            if let Ok(active) = manager.active.lock() {
                for cancel in active.values() {
                    cancel.store(true, Ordering::SeqCst);
                }
            }
            Err(error)
        }
    }
}
pub fn secret(namespace: &str, key: &str) -> Result<Option<String>, ManagerError> {
    let entry = keyring::Entry::new("cardbe.plugins", &format!("{namespace}:{key}"))
        .map_err(|_| internal("Secret storage unavailable"))?;
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err(internal("Secret storage unavailable").into()),
    }
}
pub fn write_secret(namespace: &str, key: &str, value: &str) -> Result<(), ManagerError> {
    let entry = keyring::Entry::new("cardbe.plugins", &format!("{namespace}:{key}"))
        .map_err(|_| internal("Secret storage unavailable"))?;
    if value.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(internal("Secret storage unavailable").into()),
        }
    } else {
        entry
            .set_password(value)
            .map_err(|_| internal("Secret storage unavailable").into())
    }
}
pub fn start_scheduler(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
            let Ok(state) = snapshot(&app) else {
                continue;
            };
            if state.safe_mode {
                continue;
            }
            for instance in state.instances {
                if instance.enabled
                    && instance.pending_domain.is_none()
                    && !instance.running
                    && instance.interval_seconds >= 60
                    && instance.next_run_at.is_none_or(|next| next <= now())
                {
                    let handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = run(handle, instance.id, "schedule").await;
                    });
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn updated_credentials_are_not_reused_for_changed_destinations_or_removed_fields() {
        let package: PluginPackage = serde_json::from_value(serde_json::json!({
            "id":"example", "name":"Example", "version":"1.1.0", "api_version":"1.0.0",
            "storage_schema_version":1,"settings":[{"key":"token","type":"secret","label":"Token"}]
        }))
        .unwrap();
        let mut instance = instance("one");
        instance.secret_fields = vec!["retired_token".into(), "token".into()];
        assert_eq!(credential_key(&instance, &package), Some("token"));
        instance.credentials_need_review = true;
        assert_eq!(credential_key(&instance, &package), None);
        instance.credentials_need_review = false;
        instance.secret_fields = vec!["retired_token".into()];
        assert_eq!(credential_key(&instance, &package), None);
    }
    #[test]
    fn expected_preparation_failure_disables_only_affected_instance() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        storage::initialize(&connection).unwrap();
        let affected = instance("one");
        storage::put(&connection, "instance", &affected.id, &affected).unwrap();
        storage::put(&connection, "instance", "two", &instance("two")).unwrap();
        disable_preparation(
            &connection,
            &affected,
            "schedule",
            CommandError::BoardNotFound,
        )
        .unwrap();
        let saved: Vec<PluginInstance> = storage::list(&connection, "instance").unwrap();
        assert!(!saved[0].enabled);
        assert_eq!(saved[0].last_run_status.as_deref(), Some("failed"));
        assert!(saved[0].next_run_at.is_none());
        assert_eq!(
            serde_json::to_value(&saved[0].last_error).unwrap(),
            serde_json::json!({"code":"BOARD_NOT_FOUND"})
        );
        assert!(saved[1].enabled);
        let runs: Vec<PluginRun> = storage::list(&connection, "run").unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "failed");
        assert!(runs[0].finished_at.is_some());
    }
    #[test]
    fn guest_errors_require_allowlisted_codes_and_bounded_public_retry() {
        let package: PluginPackage = serde_json::from_value(serde_json::json!({
            "id":"example", "name":"Example", "version":"1.0.0", "api_version":"1.0.0", "storage_schema_version":1, "error_codes":["RATE_LIMITED"]
        })).unwrap();
        let (error, retry) = runtime_error(
            &package,
            runtime::RuntimeError::Guest {
                code: "RATE_LIMITED".into(),
                retry_after_seconds: Some(u32::MAX),
            },
        );
        assert_eq!(retry, Some(86400));
        assert_eq!(
            serde_json::to_value(CommandError::from(error)).unwrap(),
            serde_json::json!({"code":"PLUGIN_ERROR","plugin_id":"example","plugin_error":{"code":"RATE_LIMITED","params":{"retry_after_seconds":86400}}})
        );
        let (error, retry) = runtime_error(
            &package,
            runtime::RuntimeError::Guest {
                code: "UNDECLARED".into(),
                retry_after_seconds: Some(123),
            },
        );
        assert_eq!(retry, None);
        assert_eq!(
            serde_json::to_value(CommandError::from(error)).unwrap(),
            serde_json::json!({"code":"PLUGIN_EXECUTION_FAILED","plugin_id":"example"})
        );
    }
    #[test]
    fn secret_namespace_isolated_by_database_and_instance() {
        let path = std::path::Path::new("isolated/data.sqlite3");
        let namespace = secret_namespace_for_path(path, "one");
        assert_eq!(namespace, secret_namespace_for_path(path, "one"));
        assert_ne!(namespace, secret_namespace_for_path(path, "two"));
        assert_ne!(
            namespace,
            secret_namespace_for_path(std::path::Path::new("another/data.sqlite3"), "one")
        );
        assert!(!namespace.contains("data.sqlite3"));
    }
    #[test]
    fn committed_board_is_installed_before_history_failure() {
        use crate::{
            models::{Column, StoredData, Task},
            state::AppData,
            storage::Database,
        };
        let directory = std::env::temp_dir().join(format!(
            "cardbe-plugin-manager-{}-{}",
            std::process::id(),
            id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let initial = StoredData::default();
        let mut database = Database::open(directory.join("test.sqlite3")).unwrap();
        database.initialize_boards(&initial).unwrap();
        database.initialize_plugins().unwrap();
        let mut app = AppData::new(initial.clone(), database, vec![], false).unwrap();
        app.undo_history.push(initial.clone());
        let mut committed = initial.clone();
        committed.columns.push(Column {
            id: 1,
            name: "Imported".into(),
            color: String::new(),
            sort_order: Default::default(),
            tasks: vec![Task {
                id: 1,
                title: "Committed task".into(),
                ..Task::default()
            }],
        });
        let board_id = app.active_board_id;
        install_committed(&mut app, board_id, committed, true);
        app.database.plugin_connection().execute_batch("CREATE TRIGGER fail_history BEFORE INSERT ON cardbe_plugin_data WHEN NEW.kind='run' BEGIN SELECT RAISE(ABORT,'isolated test failure'); END;").unwrap();
        assert!(storage::put(
            app.database.plugin_connection(),
            "run",
            "failed-history",
            &true
        )
        .is_err());
        assert_eq!(app.stored.columns[0].tasks[0].title, "Committed task");
        assert!(app.undo_history.is_empty());
        app.undo_history.push(app.stored.clone());
        install_committed(&mut app, board_id, initial, false);
        assert_eq!(app.undo_history.len(), 1);
        assert_eq!(app.stored.columns[0].tasks[0].title, "Committed task");
        drop(app);
        std::fs::remove_dir_all(directory).unwrap();
    }
    fn instance(id: &str) -> PluginInstance {
        PluginInstance {
            id: id.into(),
            plugin_id: "example".into(),
            name: id.into(),
            board_id: 1,
            config: Default::default(),
            secret_fields: vec![],
            allowed_domains: vec![],
            pending_domain: None,
            enabled: true,
            needs_review: false,
            credentials_need_review: false,
            interval_seconds: 60,
            last_run_at: None,
            last_run_status: None,
            next_run_at: None,
            failures: 0,
            last_error: None,
            running: false,
        }
    }
    #[test]
    fn reservations_bound_parallelism_and_prevent_duplicates() {
        let mut active = HashMap::new();
        assert!(reserve(&mut active, "one", Arc::new(AtomicBool::new(false)), false).is_ok());
        assert!(reserve(&mut active, "one", Arc::new(AtomicBool::new(false)), false).is_err());
        assert!(reserve(&mut active, "two", Arc::new(AtomicBool::new(false)), false).is_ok());
        assert!(reserve(
            &mut active,
            "three",
            Arc::new(AtomicBool::new(false)),
            false
        )
        .is_err());
        assert!(!reserve(&mut active, "three", Arc::new(AtomicBool::new(false)), true).unwrap());
        assert!(reserve(&mut active, "one", Arc::new(AtomicBool::new(false)), true).is_err());
        active.remove("one");
        assert!(reserve(&mut active, "three", Arc::new(AtomicBool::new(false)), true).unwrap());
    }
    #[test]
    fn schedules_backoff_and_retry_are_bounded() {
        assert_eq!(next_run(1000, 0, 0, None), None);
        assert_eq!(next_run(1000, 60, 2, None), Some(241000));
        assert_eq!(next_run(1000, 60, 0, Some(600)), Some(601000));
        assert_eq!(
            next_run(1000, u64::MAX, u32::MAX, Some(u32::MAX)),
            Some(86401000)
        );
    }
    #[test]
    fn changed_authorization_prevents_commit() {
        let original = instance("one");
        let mut current = original.clone();
        assert!(same_authorization(&original, &current));
        current.allowed_domains.push("example.com".into());
        assert!(!same_authorization(&original, &current));
        current = original.clone();
        current.board_id = 2;
        assert!(!same_authorization(&original, &current));
        current = original.clone();
        current.config.insert("repository".into(), "changed".into());
        assert!(!same_authorization(&original, &current));
        current = original.clone();
        current.enabled = false;
        assert!(!same_authorization(&original, &current));
    }
    #[test]
    fn restart_marks_interrupted_and_disables_only_affected_instance() {
        let mut instances = vec![instance("one"), instance("two")];
        let mut runs = vec![PluginRun {
            id: "run".into(),
            instance_id: "one".into(),
            trigger: "manual".into(),
            started_at: 1,
            finished_at: None,
            status: "running".into(),
            error: None,
            logs: vec![],
            created: 0,
            updated: 0,
        }];
        recover(&mut runs, &mut instances, 5);
        assert_eq!(runs[0].status, "interrupted");
        assert_eq!(runs[0].finished_at, Some(5));
        assert!(!instances[0].enabled);
        assert!(instances[1].enabled);
    }
}
