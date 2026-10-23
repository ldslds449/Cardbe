//! Embedded Component Model runtime. No WASI imports are linked.
use super::http;
pub use super::http::HttpPolicy;
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use wasmtime::{
    component::{Component, Linker},
    Config, Engine, Store, StoreLimits, StoreLimitsBuilder, UpdateDeadline,
};

pub(crate) mod bindings {
    wasmtime::component::bindgen!({ path: "plugin-api/wit", world: "plugin" });
}

pub struct RuntimeInput {
    pub board_scope: Option<(std::path::PathBuf, i64)>,
    pub instance_id: String,
    pub run_id: String,
    pub config_json: String,
    pub trigger: String,
    pub cursor: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExternalTask {
    pub external_key: String,
    pub title: String,
    pub description: String,
    pub url: String,
    pub state: String,
    pub labels: Vec<String>,
    pub updated_at: String,
    pub column_id: i64,
    /// Last source priority reported through `set-external-task-priority`;
    /// not part of the WIT record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<crate::models::TaskPriority>,
}
/// Either an allowlisted diagnostic code with a count, or a sanitized free-text message.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeLog {
    pub level: String,
    pub code: Option<String>,
    pub count: u32,
    pub message: Option<String>,
}
const MESSAGE_BYTES: usize = 1024;
const MESSAGE_COUNT: usize = 200;
const MESSAGE_TOTAL_BYTES: usize = 64 * 1024;
pub struct RuntimeOutput {
    pub changeset: Option<super::board::ChangeSet>,
    pub snapshot_version: String,
    pub tasks: Vec<ExternalTask>,
    pub cursor: Option<String>,
    pub logs: Vec<RuntimeLog>,
}
#[derive(Debug)]
pub enum RuntimeError {
    InvalidComponent,
    PermissionDenied,
    /// A stored credential cannot be sent as an HTTP header value.
    InvalidCredential,
    AuthorizationRequired(String),
    /// An approved host resolved to intranet addresses without private-network approval.
    PrivateNetworkRequired(String),
    /// DNS answered loopback, link-local, or other reserved addresses.
    BlockedAddress,
    Redirect,
    Network,
    ResourceLimit,
    Timeout,
    Cancelled,
    Trap,
    Guest {
        code: String,
        retry_after_seconds: Option<u32>,
    },
}
impl RuntimeError {
    pub fn code(&self) -> &str {
        match self {
            Self::InvalidComponent => "INVALID_COMPONENT",
            Self::PermissionDenied => "PERMISSION_DENIED",
            Self::InvalidCredential => "INVALID_CREDENTIAL",
            Self::AuthorizationRequired(_)
            | Self::PrivateNetworkRequired(_)
            | Self::BlockedAddress
            | Self::Redirect => "PERMISSION_DENIED",
            Self::Network => "NETWORK_FAILED",
            Self::ResourceLimit => "RESOURCE_LIMIT",
            Self::Timeout => "TIMEOUT",
            Self::Cancelled => "CANCELLED",
            Self::Trap => "WASM_TRAP",
            Self::Guest { code, .. } => code,
        }
    }
}
impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for RuntimeError {}

struct HostState {
    reader: Option<super::board::Reader>,
    queries: usize,
    query_bytes: usize,
    limits: StoreLimits,
    policy: HttpPolicy,
    cancel: Arc<AtomicBool>,
    deadline: Instant,
    handle: tokio::runtime::Handle,
    logs: Vec<RuntimeLog>,
    messages: usize,
    message_bytes: usize,
    requests: usize,
    request_bytes: usize,
    response_bytes: usize,
    failure: Option<RuntimeError>,
    /// `set-task-priority` calls, applied after the returned changeset.
    priorities: Vec<super::board::Operation>,
    /// `set-external-task-priority` reports; the last call per key wins.
    source_priorities: std::collections::BTreeMap<String, Option<crate::models::TaskPriority>>,
    #[cfg(test)]
    fixtures: Option<std::collections::VecDeque<bindings::cardbe::plugin::types::HttpResponse>>,
}
const MAX_STAGED_PRIORITIES: usize = 200;
const MAX_SOURCE_PRIORITIES: usize = 2000;
fn query_error(error: crate::errors::DomainError) -> bindings::cardbe::plugin::board::HostError {
    use crate::errors::DomainError;
    use bindings::cardbe::plugin::board::HostError;
    match error {
        DomainError::InvalidArgument => HostError::InvalidArgument,
        DomainError::PermissionDenied => HostError::PermissionDenied,
        DomainError::TaskNotFound | DomainError::ColumnNotFound | DomainError::BoardNotFound => {
            HostError::NotFound
        }
        _ => HostError::InternalError,
    }
}
impl HostState {
    fn read<T: Serialize>(
        &mut self,
        query: impl FnOnce(&super::board::Reader) -> Result<T, crate::errors::DomainError>,
    ) -> Result<(String, T), bindings::cardbe::plugin::board::HostError> {
        use bindings::cardbe::plugin::board::HostError;
        self.queries += 1;
        if self.queries > 1000
            || self.query_bytes >= 32 * 1024 * 1024
            || self.cancel.load(Ordering::Acquire)
            || Instant::now() >= self.deadline
        {
            return Err(HostError::ResourceLimit);
        }
        let reader = self.reader.as_ref().ok_or(HostError::PermissionDenied)?;
        let data = query(reader).map_err(query_error)?;
        // Account native serialized payload bytes internally; no JSON crosses the WIT boundary.
        let bytes = serde_json::to_vec(&data)
            .map_err(|_| HostError::InternalError)?
            .len()
            + reader.version.len();
        if bytes > 2 * 1024 * 1024 || self.query_bytes.saturating_add(bytes) > 32 * 1024 * 1024 {
            return Err(HostError::ResourceLimit);
        }
        self.query_bytes += bytes;
        Ok((reader.version.clone(), data))
    }
}
impl bindings::cardbe::plugin::board::Host for HostState {
    fn list_boards(
        &mut self,
    ) -> Result<
        bindings::cardbe::plugin::types::BoardList,
        bindings::cardbe::plugin::board::HostError,
    > {
        let (version, boards) = self.read(super::board::Reader::list_boards)?;
        Ok(bindings::cardbe::plugin::types::BoardList {
            version,
            boards: boards.into_iter().map(Into::into).collect(),
        })
    }
    fn get_board(
        &mut self,
    ) -> Result<
        bindings::cardbe::plugin::types::BoardSnapshot,
        bindings::cardbe::plugin::board::HostError,
    > {
        let (version, board) = self.read(super::board::Reader::get_board)?;
        Ok(bindings::cardbe::plugin::types::BoardSnapshot {
            version,
            board: board.into(),
        })
    }
    fn list_columns(
        &mut self,
    ) -> Result<
        bindings::cardbe::plugin::types::ColumnList,
        bindings::cardbe::plugin::board::HostError,
    > {
        let (version, columns) = self.read(super::board::Reader::list_columns)?;
        Ok(bindings::cardbe::plugin::types::ColumnList {
            version,
            columns: columns.into_iter().map(Into::into).collect(),
        })
    }
    fn get_column(
        &mut self,
        id: i64,
    ) -> Result<
        bindings::cardbe::plugin::types::ColumnSnapshot,
        bindings::cardbe::plugin::board::HostError,
    > {
        let (version, column) = self.read(|reader| reader.get_column(id))?;
        Ok(bindings::cardbe::plugin::types::ColumnSnapshot {
            version,
            column: column.into(),
        })
    }
    fn get_task(
        &mut self,
        id: i64,
        archived: bool,
    ) -> Result<
        bindings::cardbe::plugin::types::TaskSnapshot,
        bindings::cardbe::plugin::board::HostError,
    > {
        let (version, task) = self.read(|reader| reader.get_task(id, archived))?;
        Ok(bindings::cardbe::plugin::types::TaskSnapshot {
            version,
            task: task.try_into().map_err(query_error)?,
        })
    }
    fn list_tasks(
        &mut self,
        request: bindings::cardbe::plugin::types::TaskQuery,
    ) -> Result<bindings::cardbe::plugin::types::TaskPage, bindings::cardbe::plugin::board::HostError>
    {
        let offset = request.offset;
        let limit = request.limit;
        let (version, tasks) =
            self.read(|reader| reader.list_tasks(request.into(), offset, limit))?;
        Ok(bindings::cardbe::plugin::types::TaskPage {
            version,
            offset,
            limit,
            tasks: tasks
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()
                .map_err(query_error)?,
        })
    }
    fn get_task_priority(
        &mut self,
        id: i64,
        archived: bool,
    ) -> Result<
        Option<bindings::cardbe::plugin::types::TaskPriority>,
        bindings::cardbe::plugin::board::HostError,
    > {
        let (_, priority) =
            self.read(|reader| reader.get_task(id, archived).map(|info| info.task.priority))?;
        Ok(priority.map(Into::into))
    }
    fn get_done_column(
        &mut self,
    ) -> Result<Option<i64>, bindings::cardbe::plugin::board::HostError> {
        Ok(self.read(|reader| reader.done_column())?.1)
    }
    fn get_task_completed_time(
        &mut self,
        id: i64,
        archived: bool,
    ) -> Result<Option<u64>, bindings::cardbe::plugin::board::HostError> {
        let (_, time) = self.read(|reader| reader.completed_time(id, archived))?;
        Ok(time.and_then(|time| u64::try_from(time).ok()))
    }
    fn set_task_priority(
        &mut self,
        task: bindings::cardbe::plugin::types::TaskReference,
        priority: Option<bindings::cardbe::plugin::types::TaskPriority>,
    ) -> Result<(), bindings::cardbe::plugin::board::HostError> {
        use bindings::cardbe::plugin::{board::HostError, types::TaskReference};
        if self.reader.is_none() {
            return Err(HostError::PermissionDenied);
        }
        if matches!(&task, TaskReference::Temporary(key) if key.is_empty() || key.len() > 128) {
            return Err(HostError::InvalidArgument);
        }
        if self.priorities.len() >= MAX_STAGED_PRIORITIES
            || self.cancel.load(Ordering::Acquire)
            || Instant::now() >= self.deadline
        {
            return Err(HostError::ResourceLimit);
        }
        self.priorities
            .push(super::board::Operation::SetTaskPriority {
                task: task.into(),
                priority: priority.map(Into::into),
            });
        Ok(())
    }
    fn set_external_task_priority(
        &mut self,
        external_key: String,
        priority: Option<bindings::cardbe::plugin::types::TaskPriority>,
    ) -> Result<(), bindings::cardbe::plugin::board::HostError> {
        use bindings::cardbe::plugin::board::HostError;
        if self.reader.is_none() {
            return Err(HostError::PermissionDenied);
        }
        if external_key.is_empty() || external_key.len() > 512 {
            return Err(HostError::InvalidArgument);
        }
        if (self.source_priorities.len() >= MAX_SOURCE_PRIORITIES
            && !self.source_priorities.contains_key(&external_key))
            || self.cancel.load(Ordering::Acquire)
            || Instant::now() >= self.deadline
        {
            return Err(HostError::ResourceLimit);
        }
        self.source_priorities
            .insert(external_key, priority.map(Into::into));
        Ok(())
    }
}
impl bindings::cardbe::plugin::types::Host for HostState {}
impl bindings::cardbe::plugin::host::Host for HostState {
    fn http_get(
        &mut self,
        url: String,
    ) -> Result<bindings::cardbe::plugin::types::HttpResponse, String> {
        self.request(&url, http::Outgoing::get(), false)
    }
    fn http_post(
        &mut self,
        url: String,
        content_type: String,
        body: Vec<u8>,
    ) -> Result<bindings::cardbe::plugin::types::HttpResponse, String> {
        self.request(&url, http::Outgoing::post(&content_type, &body), false)
    }
    fn http_send(
        &mut self,
        request: bindings::cardbe::plugin::types::HttpRequest,
    ) -> Result<bindings::cardbe::plugin::types::HttpResponse, String> {
        use bindings::cardbe::plugin::types::HttpMethod;
        let outgoing = http::Outgoing {
            method: match request.method {
                HttpMethod::Get => reqwest::Method::GET,
                HttpMethod::Post => reqwest::Method::POST,
                HttpMethod::Put => reqwest::Method::PUT,
                HttpMethod::Patch => reqwest::Method::PATCH,
                HttpMethod::Delete => reqwest::Method::DELETE,
            },
            headers: request.headers,
            body: request.body,
        };
        self.request(&request.url, outgoing, true)
    }
    fn log(&mut self, code: String, count: u32) {
        if self.logs.len() - self.messages < 100
            && diagnostic_code(&code)
            && self.policy.log_codes.contains(&code)
        {
            self.logs.push(RuntimeLog {
                level: "info".into(),
                code: Some(code),
                count,
                message: None,
            });
        }
    }
    fn log_message(&mut self, level: bindings::cardbe::plugin::types::LogLevel, message: String) {
        use bindings::cardbe::plugin::types::LogLevel;
        let remaining = MESSAGE_TOTAL_BYTES - self.message_bytes;
        if self.messages >= MESSAGE_COUNT || remaining == 0 {
            return;
        }
        let mut message = message;
        for secret in self
            .policy
            .token
            .iter()
            .chain(self.policy.credentials.values())
            .filter(|secret| !secret.is_empty())
        {
            message = message.replace(secret.as_str(), "***");
        }
        let mut message: String = message
            .chars()
            .map(|c| {
                if c.is_control() && c != '\n' && c != '\t' {
                    '\u{FFFD}'
                } else {
                    c
                }
            })
            .collect();
        let limit = MESSAGE_BYTES.min(remaining);
        if message.len() > limit {
            let mut end = limit;
            while !message.is_char_boundary(end) {
                end -= 1;
            }
            message.truncate(end);
        }
        self.messages += 1;
        self.message_bytes += message.len();
        self.logs.push(RuntimeLog {
            level: match level {
                LogLevel::Debug => "debug",
                LogLevel::Info => "info",
                LogLevel::Warn => "warn",
                LogLevel::Error => "error",
            }
            .into(),
            code: None,
            count: 0,
            message: Some(message),
        });
    }
}
impl HostState {
    /// `recoverable` requests return transport failures to the guest without failing the run.
    fn request(
        &mut self,
        url: &str,
        outgoing: http::Outgoing,
        recoverable: bool,
    ) -> Result<bindings::cardbe::plugin::types::HttpResponse, String> {
        if let Some(error) = self.failure.as_ref() {
            return Err(error.code().to_owned());
        }
        self.requests += 1;
        self.request_bytes += outgoing.body.as_ref().map_or(0, Vec::len);
        let validation = http::validate_request(&outgoing, &self.policy);
        let result = if let Err(error) = validation {
            Err(error)
        } else if self.requests > 100 || self.request_bytes > 32 * 1024 * 1024 {
            Err(RuntimeError::ResourceLimit)
        } else if self.cancel.load(Ordering::Acquire) {
            Err(RuntimeError::Cancelled)
        } else if Instant::now() >= self.deadline {
            Err(RuntimeError::Timeout)
        } else {
            self.fetch(url, &outgoing)
        };
        match result {
            Ok(response) => {
                self.response_bytes += response.body.len();
                if self.response_bytes > 32 * 1024 * 1024 {
                    self.failure = Some(RuntimeError::ResourceLimit);
                    return Err("RESOURCE_LIMIT".into());
                }
                Ok(response)
            }
            Err(RuntimeError::Network) if recoverable => Err(RuntimeError::Network.code().into()),
            Err(error) => {
                let code = error.code().to_owned();
                if self.failure.is_none() {
                    self.failure = Some(error);
                }
                Err(code)
            }
        }
    }
    fn fetch(
        &mut self,
        url: &str,
        outgoing: &http::Outgoing,
    ) -> Result<bindings::cardbe::plugin::types::HttpResponse, RuntimeError> {
        #[cfg(test)]
        if let Some(fixtures) = self.fixtures.as_mut() {
            http::authorized_url(url, &self.policy)?;
            return fixtures.pop_front().ok_or(RuntimeError::Network);
        }
        self.handle.block_on(http::request(
            url,
            outgoing,
            &self.policy,
            self.cancel.clone(),
            self.deadline,
        ))
    }
}
fn diagnostic_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 64
        && code
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

/// Must run on a blocking worker. Every invocation creates and discards its own Store.
/// On failure, `failure_logs` receives the diagnostics the guest recorded before it stopped;
/// successful output carries its own logs.
pub fn execute(
    bytes: &[u8],
    input: RuntimeInput,
    policy: HttpPolicy,
    cancel: Arc<AtomicBool>,
    failure_logs: &mut Vec<RuntimeLog>,
) -> Result<RuntimeOutput, RuntimeError> {
    execute_inner(
        bytes,
        input,
        policy,
        cancel,
        None,
        Duration::from_secs(60),
        failure_logs,
    )
}
fn execute_inner(
    bytes: &[u8],
    input: RuntimeInput,
    policy: HttpPolicy,
    cancel: Arc<AtomicBool>,
    #[allow(unused_variables)] fixtures: Option<Vec<bindings::cardbe::plugin::types::HttpResponse>>,
    timeout: Duration,
    failure_logs: &mut Vec<RuntimeLog>,
) -> Result<RuntimeOutput, RuntimeError> {
    if bytes.len() as u64 > super::package::COMPONENT_LIMIT || input.config_json.len() > 64 * 1024 {
        return Err(RuntimeError::ResourceLimit);
    }
    if cancel.load(Ordering::Acquire) {
        return Err(RuntimeError::Cancelled);
    }
    let (engine, component, linker) = prepare_component(bytes)?;
    let deadline = Instant::now() + timeout;
    if cancel.load(Ordering::Acquire) {
        return Err(RuntimeError::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(RuntimeError::Timeout);
    }
    let reader = input
        .board_scope
        .map(|(path, id)| super::board::Reader::open(path, id))
        .transpose()
        .map_err(|_| RuntimeError::PermissionDenied)?;
    if cancel.load(Ordering::Acquire) {
        return Err(RuntimeError::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(RuntimeError::Timeout);
    }
    let state = HostState {
        reader,
        queries: 0,
        query_bytes: 0,
        limits: StoreLimitsBuilder::new()
            .memory_size(128 * 1024 * 1024)
            .table_elements(10000)
            .instances(8)
            .memories(1)
            .tables(4)
            .trap_on_grow_failure(true)
            .build(),
        policy,
        cancel: cancel.clone(),
        deadline,
        handle: tokio::runtime::Handle::current(),
        logs: vec![],
        messages: 0,
        message_bytes: 0,
        requests: 0,
        request_bytes: 0,
        response_bytes: 0,
        failure: None,
        priorities: vec![],
        source_priorities: Default::default(),
        #[cfg(test)]
        fixtures: fixtures.map(std::collections::VecDeque::from),
    };
    let mut store = Store::new(&engine, state);
    store.limiter(|state| &mut state.limits);
    store
        .set_fuel(500_000_000)
        .map_err(|_| RuntimeError::ResourceLimit)?;
    store.set_epoch_deadline(1);
    store.epoch_deadline_callback(|context| {
        if context.data().cancel.load(Ordering::Acquire)
            || Instant::now() >= context.data().deadline
        {
            return Err(wasmtime::Error::msg("EXECUTION_INTERRUPTED"));
        }
        Ok(UpdateDeadline::Continue(1))
    });
    let stop = Arc::new(AtomicBool::new(false));
    let ticker_stop = stop.clone();
    let ticker_engine = engine.clone();
    let ticker = std::thread::spawn(move || {
        while !ticker_stop.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(25));
            ticker_engine.increment_epoch();
        }
    });
    let result = (|| {
        let instance = bindings::Plugin::instantiate(&mut store, &component, &linker)
            .map_err(|_| RuntimeError::InvalidComponent)?;
        let context = bindings::cardbe::plugin::types::RunContext {
            instance_id: input.instance_id,
            run_id: input.run_id,
            config_json: input.config_json,
            board_id: store.data().reader.as_ref().map_or(0, |r| r.board_id),
            board_version: store
                .data()
                .reader
                .as_ref()
                .map_or_else(String::new, |r| r.version.clone()),
            trigger: input.trigger,
            cursor: input.cursor,
        };
        let result = instance.call_run(&mut store, &context);
        if let Some(error) = store.data_mut().failure.take() {
            return Err(error);
        }
        let result = result.map_err(|_| {
            if store.get_fuel().unwrap_or(0) == 0 {
                RuntimeError::ResourceLimit
            } else {
                RuntimeError::Trap
            }
        })?;
        if cancel.load(Ordering::Acquire) {
            return Err(RuntimeError::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(RuntimeError::Timeout);
        }
        let changeset = result.map_err(|error| RuntimeError::Guest {
            code: if diagnostic_code(&error.code)
                && store.data().policy.error_codes.contains(&error.code)
            {
                error.code
            } else {
                "PLUGIN_FAILED".into()
            },
            retry_after_seconds: error
                .retry_after_seconds
                .map(|seconds| seconds.clamp(60, 86400)),
        })?;
        if changeset.tasks.len() > 2000
            || changeset
                .changeset
                .as_ref()
                .is_some_and(|c| c.operations.len() > 2200)
            || changeset
                .cursor
                .as_ref()
                .is_some_and(|cursor| cursor.len() > 4096)
        {
            return Err(RuntimeError::ResourceLimit);
        }
        let mut general: Option<super::board::ChangeSet> = changeset.changeset.map(Into::into);
        let mut priorities = std::mem::take(&mut store.data_mut().priorities);
        priorities.extend(
            std::mem::take(&mut store.data_mut().source_priorities)
                .into_iter()
                .map(
                    |(external_key, priority)| super::board::Operation::SetExternalTaskPriority {
                        external_key,
                        priority,
                    },
                ),
        );
        if !priorities.is_empty() {
            general
                .get_or_insert_with(|| super::board::ChangeSet {
                    version: context.board_version.clone(),
                    operations: vec![],
                })
                .operations
                .extend(priorities);
        }
        let tasks: Vec<ExternalTask> = changeset.tasks.into_iter().map(Into::into).collect();
        let encoded =
            serde_json::to_vec(&(&general, &tasks)).map_err(|_| RuntimeError::ResourceLimit)?;
        if encoded.len() > 8 * 1024 * 1024 {
            return Err(RuntimeError::ResourceLimit);
        }
        Ok(RuntimeOutput {
            changeset: general,
            snapshot_version: context.board_version,
            tasks,
            cursor: changeset.cursor,
            logs: std::mem::take(&mut store.data_mut().logs),
        })
    })();
    stop.store(true, Ordering::Release);
    let _ = ticker.join();
    let interrupted = if cancel.load(Ordering::Acquire) {
        Some(RuntimeError::Cancelled)
    } else if Instant::now() >= deadline {
        Some(RuntimeError::Timeout)
    } else {
        None
    };
    match (result, interrupted) {
        (Ok(output), None) => Ok(output),
        (Ok(output), Some(error)) => {
            *failure_logs = output.logs;
            Err(error)
        }
        (Err(error), interrupted) => {
            *failure_logs = std::mem::take(&mut store.data_mut().logs);
            Err(interrupted.unwrap_or(error))
        }
    }
}

fn prepare_component(bytes: &[u8]) -> Result<(Engine, Component, Linker<HostState>), RuntimeError> {
    let mut config = Config::new();
    config
        .wasm_component_model(true)
        .consume_fuel(true)
        .epoch_interruption(true);
    config.max_wasm_stack(512 * 1024);
    let engine = Engine::new(&config).map_err(|_| RuntimeError::InvalidComponent)?;
    let component = Component::new(&engine, bytes).map_err(|_| RuntimeError::InvalidComponent)?;
    let mut linker = Linker::new(&engine);
    bindings::Plugin::add_to_linker::<_, wasmtime::component::HasSelf<_>>(&mut linker, |state| {
        state
    })
    .map_err(|_| RuntimeError::InvalidComponent)?;
    Ok((engine, component, linker))
}

pub fn validate_component(bytes: &[u8]) -> Result<(), RuntimeError> {
    if bytes.len() as u64 > super::package::COMPONENT_LIMIT {
        return Err(RuntimeError::ResourceLimit);
    }
    let (_, component, linker) = prepare_component(bytes)?;
    let pre = linker
        .instantiate_pre(&component)
        .map_err(|_| RuntimeError::InvalidComponent)?;
    bindings::PluginPre::new(pre).map_err(|_| RuntimeError::InvalidComponent)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn component_size_limit_is_shared_by_validation_and_execution() {
        let oversized = vec![0; super::super::package::COMPONENT_LIMIT as usize + 1];
        assert!(matches!(
            validate_component(&oversized),
            Err(RuntimeError::ResourceLimit)
        ));
        assert!(matches!(
            execute(
                &oversized,
                input("{}"),
                policy(),
                Arc::new(AtomicBool::new(false)),
                &mut Vec::new()
            ),
            Err(RuntimeError::ResourceLimit)
        ));
        assert!(matches!(
            validate_component(&vec![0; 17 * 1024 * 1024]),
            Err(RuntimeError::InvalidComponent)
        ));
    }
    #[tokio::test(flavor = "multi_thread")]
    async fn priority_and_completion_functions_cross_the_component_boundary() {
        tokio::task::spawn_blocking(|| {
            use crate::models::TaskPriority::{High, Low, Medium, Urgent};
            let dir = std::env::temp_dir().join(format!(
                "cardbe-plugin-priority-{}",
                super::super::types::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let mut loaded = crate::storage::load(&dir).unwrap();
            loaded.database.initialize_plugins().unwrap();
            let path = loaded.database.path().to_owned();
            let board_id = loaded.database.active_board_id();
            let task = |id, title: &str, priority| crate::models::Task {
                id,
                title: title.into(),
                priority: Some(priority),
                ..Default::default()
            };
            loaded
                .database
                .replace_board_as_local_edit(
                    board_id,
                    &crate::models::StoredData {
                        columns: vec![crate::models::Column {
                            id: 1,
                            name: "Todo".into(),
                            color: String::new(),
                            sort_order: Default::default(),
                            sort_rules: Vec::new(),
                            done: false,
                            tasks: vec![task(10, "A", High), task(11, "B", Medium)],
                        }],
                        ..Default::default()
                    },
                )
                .unwrap();
            let bytes = component("runtime-fixture");
            let execute = |mode: &str| {
                let mut context = input(mode);
                context.board_scope = Some((path.clone(), board_id));
                execute_inner(
                    &bytes,
                    context,
                    policy(),
                    Arc::new(AtomicBool::new(false)),
                    None,
                    Duration::from_secs(10),
                    &mut Vec::new(),
                )
                .unwrap()
            };

            let output = execute("priority");
            assert_eq!(output.cursor.as_deref(), Some("Some(TaskPriority::High)"));
            let operations = &output.changeset.as_ref().unwrap().operations;
            assert_eq!(operations.len(), 5);
            assert!(operations[2..].iter().all(|operation| matches!(
                operation,
                super::super::board::Operation::SetTaskPriority { .. }
            )));
            let instance = serde_json::from_value(serde_json::json!({
                "id":"instance","plugin_id":"fixture","name":"Fixture","board_id":board_id,
                "config":{},"allowed_domains":[],"enabled":true,"interval_seconds":0,"failures":0,
            }))
            .unwrap();
            let prepared = super::super::board::prepare_output(
                &loaded.database,
                instance,
                "run".into(),
                &output,
            )
            .unwrap();
            let tasks = &prepared.after.columns[0].tasks;
            let find = |title: &str| tasks.iter().find(|task| task.title == title).unwrap();
            assert_eq!(find("Renamed").priority, Some(Urgent));
            assert_eq!(find("B").priority, None);
            assert_eq!(find("Ranked").priority, Some(Low));

            // Staging is bounded; a run without a changeset still gets one.
            let output = execute("priority-limit");
            let changeset = output.changeset.unwrap();
            assert_eq!(changeset.operations.len(), 200);
            assert_eq!(changeset.version, output.snapshot_version);

            // Completion reads the done column and stored or archive times.
            let mut stored = loaded.database.read_board_complete(board_id).unwrap();
            stored.columns[0].done = false;
            stored.columns.push(crate::models::Column {
                id: 2,
                name: "Done".into(),
                color: String::new(),
                sort_order: Default::default(),
                sort_rules: Vec::new(),
                done: true,
                tasks: vec![crate::models::Task {
                    completed_at: Some(1_800_000_000_000),
                    ..task(11, "Done", Low)
                }],
            });
            stored.columns[0].tasks.retain(|task| task.id != 11);
            stored.archives = vec![
                crate::models::Archive {
                    time: 1_700_000_000_000,
                    task: task(20, "Archived", Low),
                },
                crate::models::Archive {
                    time: 1_700_000_000_000,
                    task: crate::models::Task {
                        completed_at: Some(1_600_000_000_000),
                        ..task(21, "Done then archived", Low)
                    },
                },
            ];
            loaded
                .database
                .replace_board_as_local_edit(board_id, &stored)
                .unwrap();
            let output = execute("completion");
            assert_eq!(
                output.cursor.as_deref(),
                Some(
                    "Some(2) [None, Some(1800000000000), Some(1700000000000), Some(1600000000000)]"
                )
            );

            // Source reports are deduplicated by key and applied after the returned tasks.
            let output = execute("source-priority");
            assert_eq!(output.tasks.len(), 2000);
            let operations = &output.changeset.as_ref().unwrap().operations;
            assert_eq!(operations.len(), 2000);
            assert!(operations.iter().any(|operation| matches!(
                operation,
                super::super::board::Operation::SetExternalTaskPriority {
                    external_key,
                    priority: Some(High),
                } if external_key == "source:0"
            )));
            let instance = serde_json::from_value(serde_json::json!({
                "id":"instance","plugin_id":"fixture","name":"Fixture","board_id":board_id,
                "config":{},"allowed_domains":[],"enabled":true,"interval_seconds":0,"failures":0,
            }))
            .unwrap();
            let prepared = super::super::board::prepare_output(
                &loaded.database,
                instance,
                "run".into(),
                &output,
            )
            .unwrap();
            assert_eq!(prepared.created, 2000);
            let source = prepared.after.columns[0]
                .tasks
                .iter()
                .find(|task| task.title == "Source 0")
                .unwrap();
            assert_eq!(source.priority, Some(High));

            drop(loaded);
            std::fs::remove_dir_all(dir).unwrap();
        })
        .await
        .unwrap();
    }
    #[test]
    fn update_validation_checks_the_host_contract_without_executing_the_guest() {
        assert!(validate_component(&component("runtime-fixture")).is_ok());
        assert!(validate_component(b"invalid").is_err());
        assert!(validate_component(b"(component)").is_err());
        assert!(validate_component(b"(component (import \"wasi:cli/run\" (instance)))").is_err());
    }
    fn input(config: &str) -> RuntimeInput {
        RuntimeInput {
            board_scope: None,
            instance_id: "instance".into(),
            run_id: "run".into(),
            config_json: config.into(),
            trigger: "manual".into(),
            cursor: None,
        }
    }
    fn policy() -> HttpPolicy {
        HttpPolicy {
            allowed_domains: vec!["api.example.com".into()],
            private_network_domains: vec![],
            requested_domains: vec![],
            allow_custom_domains: false,
            token: None,
            token_domain: None,
            credential_scopes: None,
            credentials: Default::default(),
            error_codes: vec!["HTTP_FAILED".into()],
            log_codes: vec!["ALLOWED_LOG".into()],
        }
    }
    fn component(name: &str) -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("test-fixtures")
                .join(name)
                .join("dist/component.wasm"),
        )
        .expect("Build test component first: vp run plugins:test")
    }
    #[tokio::test(flavor = "multi_thread")]
    async fn one_component_combines_queries_changesets_http_and_trap_isolation() {
        tokio::task::spawn_blocking(|| {
            let dir = std::env::temp_dir().join(format!(
                "cardbe-plugin-runtime-{}",
                super::super::types::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let mut loaded = crate::storage::load(&dir).unwrap();
            loaded.database.initialize_plugins().unwrap();
            let path = loaded.database.path().to_owned();
            let board_id = loaded.database.active_board_id();
            let task = crate::models::Task {
                id: 10, title: "工作 🦀".into(), description: "Details".into(), color: "#654321".into(),
                start_time: 1_800_000_000_000, due_time: Some(1_800_000_100_000), created_time: 1_700_000_000_000, labels: vec!["Label".into()],
                items: vec![crate::models::TaskItem { id: "item".into(), text: "Check".into(), completed: true }],
                recurrence: Some(crate::models::Recurrence { frequency: crate::models::RecurrenceFrequency::Weekly, interval: 2 }),
                ..Default::default()
            };
            loaded.database.replace_board_as_local_edit(board_id, &crate::models::StoredData {
                columns: vec![crate::models::Column { id: 1, name: "Existing".into(), color: String::new(), sort_order: Default::default(), sort_rules: Vec::new(), done: false, tasks: vec![task.clone()] }],
                ..Default::default()
            }).unwrap();
            let bytes = std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("test-fixtures/runtime-fixture/dist/component.wasm"),
            )
            .unwrap();
            assert!(validate_component(&bytes).is_ok());
            for mode in ["trap", "board-changeset", "board-post", "board-post-catch", "mixed", "schedule-changeset", "typed-mutations"] {
                let mut context = input(mode);
                context.board_scope = Some((path.clone(), board_id));
                if mode == "schedule-changeset" { context.trigger = "schedule".into(); }
                let mut policy = policy();
                if mode == "board-post-catch" {
                    policy.allowed_domains.clear();
                    policy.requested_domains.push("api.example.com".into());
                }
                let result = execute_inner(
                    &bytes,
                    context,
                    policy,
                    Arc::new(AtomicBool::new(false)),
                    (mode == "board-post").then(|| {
                        vec![bindings::cardbe::plugin::types::HttpResponse {
                            status: 200,
                            headers: vec![],
                            body: b"{}".to_vec(),
                        }]
                    }),
                    Duration::from_secs(10),
                    &mut Vec::new(),
                );
                if mode == "trap" {
                    assert!(matches!(result, Err(RuntimeError::Trap)));
                } else if mode == "board-post-catch" {
                    assert!(matches!(result, Err(RuntimeError::AuthorizationRequired(host)) if host == "api.example.com"));
                } else {
                    let output = result.unwrap();
                    assert!(output.tasks.is_empty());
                    let changeset = output.changeset.as_ref().unwrap();
                    assert_eq!(changeset.operations.len(), if mode == "typed-mutations" { 7 } else if mode == "mixed" { 2 } else { 1 });
                    assert_eq!(
                        changeset.version,
                        super::super::board::version(&loaded.database, board_id).unwrap()
                    );
                    if mode == "typed-mutations" {
                        let instance = serde_json::from_value(serde_json::json!({
                            "id":"instance","plugin_id":"fixture","name":"Fixture","board_id":board_id,
                            "config":{},"allowed_domains":[],"enabled":true,"interval_seconds":0,"failures":0,
                        })).unwrap();
                        let prepared = super::super::board::prepare_output(&loaded.database,instance,"run".into(),&output).unwrap();
                        let column=prepared.after.columns.iter().find(|c|c.name=="Renamed").unwrap();
                        assert_eq!(column.color,"#123456");
                        assert_eq!(column.tasks.iter().find(|t|t.id==10).unwrap(),&task);
                        assert_eq!(column.tasks.len(),3);
                        assert_eq!(loaded.database.read_board_complete(board_id).unwrap().columns.len(),1);
                    }
                }
            }
            drop(loaded);
            let canonical = dir.canonicalize().unwrap();
            assert!(canonical.starts_with(std::env::temp_dir().canonicalize().unwrap()));
            std::fs::remove_dir_all(canonical).unwrap();
        })
        .await
        .unwrap();
    }
    #[test]
    fn fixture_component_imports_only_cardbe_and_exports_run() {
        let engine = Engine::default();
        let component = Component::new(&engine, component("runtime-fixture")).unwrap();
        let ty = component.component_type();
        let mut imports: Vec<_> = ty.imports(&engine).map(|(name, _)| name).collect();
        imports.sort_unstable();
        assert_eq!(
            imports,
            [
                "cardbe:plugin/board@1.2.0",
                "cardbe:plugin/host@1.2.0",
                "cardbe:plugin/types@1.2.0"
            ]
        );
        let exports: Vec<_> = ty.exports(&engine).map(|(name, _)| name).collect();
        assert_eq!(exports, ["run"]);
    }
    #[test]
    fn api_1_0_imports_link_to_the_additive_1_1_host() {
        let linkable = |wat: &str| {
            let (_, component, linker) = prepare_component(wat.as_bytes()).unwrap();
            linker.instantiate_pre(&component).is_ok()
        };
        let host = |version: &str, function: &str| {
            format!(
                r#"(component (import "cardbe:plugin/host@{version}" (instance
                    (export "{function}" (func (param "code" string) (param "count" u32))))))"#
            )
        };
        assert!(linkable(&host("1.0.0", "log")));
        assert!(linkable(&host("1.1.0", "log")));
        assert!(linkable(&host("1.2.0", "log")));
        assert!(!linkable(&host("2.0.0", "log")));
        assert!(!linkable(&host("1.0.0", "missing")));
    }
    fn run(
        mode: &str,
        timeout: Duration,
        cancelled: Arc<AtomicBool>,
    ) -> Result<RuntimeOutput, RuntimeError> {
        execute_inner(
            &component("runtime-fixture"),
            input(mode),
            policy(),
            cancelled,
            None,
            timeout,
            &mut Vec::new(),
        )
    }
    #[tokio::test(flavor = "multi_thread")]
    async fn actual_component_calls_http_and_returns_typed_changeset() {
        for mode in ["http-get-success", "http-post-success"] {
            let response = bindings::cardbe::plugin::types::HttpResponse {
                status: 200,
                headers: vec![],
                body: b"Fixture response".to_vec(),
            };
            let output = tokio::task::spawn_blocking(move || {
                execute_inner(
                    &component("runtime-fixture"),
                    input(mode),
                    policy(),
                    Arc::new(AtomicBool::new(false)),
                    Some(vec![response]),
                    Duration::from_secs(10),
                    &mut Vec::new(),
                )
            })
            .await
            .unwrap()
            .unwrap();
            assert_eq!(output.tasks.len(), 1);
            assert_eq!(output.tasks[0].external_key, "fixture:1");
            assert_eq!(output.tasks[0].title, "HTTP 200");
            assert_eq!(output.tasks[0].description, "Fixture response");
            assert_eq!(output.tasks[0].column_id, 1);
            assert_eq!(output.logs[0].code.as_deref(), Some("ALLOWED_LOG"));
            assert_eq!(output.cursor.as_deref(), Some("next"));
        }
    }
    #[tokio::test(flavor = "multi_thread")]
    async fn http_send_recovers_from_transport_failures_but_not_policy_failures() {
        let result = |mode: &'static str, responses: Option<Vec<_>>| async move {
            tokio::task::spawn_blocking(move || {
                execute_inner(
                    &component("runtime-fixture"),
                    input(mode),
                    policy(),
                    Arc::new(AtomicBool::new(false)),
                    responses,
                    Duration::from_secs(10),
                    &mut Vec::new(),
                )
            })
            .await
            .unwrap()
        };
        let response = bindings::cardbe::plugin::types::HttpResponse {
            status: 204,
            headers: vec![],
            body: vec![],
        };
        let output = result("http-request-success", Some(vec![response]))
            .await
            .unwrap();
        assert_eq!(output.tasks[0].title, "HTTP 204");
        // An empty fixture queue simulates a transport failure.
        let output = result("http-request-recover", Some(vec![])).await.unwrap();
        assert_eq!(output.tasks[0].title, "Recovered NETWORK_FAILED");
        assert!(matches!(
            result("http-request-denied", Some(vec![])).await,
            Err(RuntimeError::PermissionDenied)
        ));
    }
    #[tokio::test(flavor = "multi_thread")]
    async fn traps_limits_permissions_and_failure_do_not_poison_later_store() {
        tokio::task::spawn_blocking(|| {
            for mode in ["trap", "memory", "http"] {
                let error = run(
                    mode,
                    Duration::from_secs(10),
                    Arc::new(AtomicBool::new(false)),
                )
                .err()
                .unwrap();
                if mode == "http" {
                    assert!(matches!(error, RuntimeError::PermissionDenied));
                } else {
                    assert!(
                        matches!(error, RuntimeError::Trap | RuntimeError::ResourceLimit),
                        "{error:?}"
                    );
                }
                assert!(run(
                    "ok",
                    Duration::from_secs(10),
                    Arc::new(AtomicBool::new(false))
                )
                .is_ok());
            }
            assert!(matches!(
                run(
                    "loop",
                    Duration::from_millis(20),
                    Arc::new(AtomicBool::new(false))
                ),
                Err(RuntimeError::Timeout)
            ));
            assert!(matches!(
                execute(
                    &[],
                    input("ok"),
                    policy(),
                    Arc::new(AtomicBool::new(false)),
                    &mut Vec::new()
                ),
                Err(RuntimeError::InvalidComponent)
            ));
        })
        .await
        .unwrap();
    }
    #[tokio::test(flavor = "multi_thread")]
    async fn failed_runs_return_logs_recorded_before_the_failure() {
        tokio::task::spawn_blocking(|| {
            for (mode, timeout, count) in [
                ("logs-error", Duration::from_secs(10), 3),
                ("logs-trap", Duration::from_secs(10), 2),
                ("logs-loop", Duration::from_millis(100), 1),
            ] {
                let mut logs = Vec::new();
                let result = execute_inner(
                    &component("runtime-fixture"),
                    input(mode),
                    policy(),
                    Arc::new(AtomicBool::new(false)),
                    None,
                    timeout,
                    &mut logs,
                );
                assert!(result.is_err(), "{mode}");
                assert_eq!(logs.len(), 1, "{mode}");
                assert_eq!(
                    (logs[0].code.as_deref(), logs[0].count),
                    (Some("ALLOWED_LOG"), count)
                );
            }
            let mut logs = Vec::new();
            assert!(execute_inner(
                &component("runtime-fixture"),
                input("logs"),
                policy(),
                Arc::new(AtomicBool::new(false)),
                None,
                Duration::from_secs(10),
                &mut logs,
            )
            .is_ok());
            assert!(logs.is_empty());
        })
        .await
        .unwrap();
    }
    #[tokio::test(flavor = "multi_thread")]
    async fn text_logs_are_sanitized_redacted_and_bounded() {
        tokio::task::spawn_blocking(|| {
            let mut policy = policy();
            policy.token = Some("fixture-secret".into());
            let mut logs = Vec::new();
            let result = execute_inner(
                &component("runtime-fixture"),
                input("messages"),
                policy,
                Arc::new(AtomicBool::new(false)),
                None,
                Duration::from_secs(10),
                &mut logs,
            );
            assert!(matches!(result, Err(RuntimeError::Guest { .. })));
            assert_eq!(logs.len(), MESSAGE_COUNT);
            let text = |index: usize| (logs[index].level.as_str(), logs[index].message.as_deref());
            assert_eq!(text(0), ("info", Some("fetched 3 items\nnext page")));
            assert_eq!(text(1), ("warn", Some("token=***\u{FFFD}[31m")));
            // A multibyte character crossing the per-message limit is dropped whole.
            assert_eq!(text(2), ("debug", Some("x".repeat(1022).as_str())));
            assert_eq!(text(3), ("error", Some("spam")));
            assert!(logs.iter().all(|log| log.code.is_none()));
        })
        .await
        .unwrap();
    }
    #[tokio::test(flavor = "multi_thread")]
    async fn cancel_and_logs_are_bounded_and_errors_are_allowlisted() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let stop = cancelled.clone();
        let task =
            tokio::task::spawn_blocking(move || run("loop", Duration::from_secs(10), cancelled));
        tokio::time::sleep(Duration::from_millis(150)).await;
        stop.store(true, Ordering::Release);
        assert!(matches!(task.await.unwrap(), Err(RuntimeError::Cancelled)));
        tokio::task::spawn_blocking(|| {
            let result = run(
                "logs",
                Duration::from_secs(10),
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();
            assert_eq!(result.logs.len(), 100);
            let error = run(
                "unknown-error",
                Duration::from_secs(10),
                Arc::new(AtomicBool::new(false)),
            )
            .err()
            .unwrap();
            assert_eq!(error.code(), "PLUGIN_FAILED");
            assert!(run(
                "ok",
                Duration::from_secs(10),
                Arc::new(AtomicBool::new(false))
            )
            .is_ok());
        })
        .await
        .unwrap();
    }
}
