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
    wasmtime::component::bindgen!({ path: "plugin-api/wit", world: "importer" });
}

pub struct RuntimeInput {
    pub instance_id: String,
    pub run_id: String,
    pub config_json: String,
    pub board_json: String,
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
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RuntimeLog {
    pub code: String,
    pub count: u32,
}
pub struct RuntimeOutput {
    pub tasks: Vec<ExternalTask>,
    pub cursor: Option<String>,
    pub logs: Vec<RuntimeLog>,
}
#[derive(Debug)]
pub enum RuntimeError {
    InvalidComponent,
    PermissionDenied,
    AuthorizationRequired(String),
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
            Self::AuthorizationRequired(_) => "PERMISSION_DENIED",
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
    limits: StoreLimits,
    policy: HttpPolicy,
    cancel: Arc<AtomicBool>,
    deadline: Instant,
    handle: tokio::runtime::Handle,
    logs: Vec<RuntimeLog>,
    requests: usize,
    response_bytes: usize,
    failure: Option<RuntimeError>,
    #[cfg(test)]
    fixtures: Option<std::collections::VecDeque<bindings::cardbe::plugin::types::HttpResponse>>,
}
impl bindings::cardbe::plugin::types::Host for HostState {}
impl bindings::cardbe::plugin::host::Host for HostState {
    fn http_get(
        &mut self,
        url: String,
    ) -> Result<bindings::cardbe::plugin::types::HttpResponse, String> {
        if let Some(error) = self.failure.as_ref() {
            return Err(error.code().to_owned());
        }
        self.requests += 1;
        let result = if self.requests > 100 {
            Err(RuntimeError::ResourceLimit)
        } else if self.cancel.load(Ordering::Acquire) {
            Err(RuntimeError::Cancelled)
        } else if Instant::now() >= self.deadline {
            Err(RuntimeError::Timeout)
        } else {
            self.fetch(&url)
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
            Err(error) => {
                let code = error.code().to_owned();
                if self.failure.is_none() {
                    self.failure = Some(error);
                }
                Err(code)
            }
        }
    }
    fn log(&mut self, code: String, count: u32) {
        if self.logs.len() < 100 && diagnostic_code(&code) && self.policy.log_codes.contains(&code)
        {
            self.logs.push(RuntimeLog { code, count });
        }
    }
}
impl HostState {
    fn fetch(
        &mut self,
        url: &str,
    ) -> Result<bindings::cardbe::plugin::types::HttpResponse, RuntimeError> {
        #[cfg(test)]
        if let Some(fixtures) = self.fixtures.as_mut() {
            http::authorized_url(url, &self.policy)?;
            return fixtures.pop_front().ok_or(RuntimeError::Network);
        }
        self.handle.block_on(http::get(
            url,
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
pub fn execute(
    bytes: &[u8],
    input: RuntimeInput,
    policy: HttpPolicy,
    cancel: Arc<AtomicBool>,
) -> Result<RuntimeOutput, RuntimeError> {
    execute_inner(bytes, input, policy, cancel, None, Duration::from_secs(60))
}
fn execute_inner(
    bytes: &[u8],
    input: RuntimeInput,
    policy: HttpPolicy,
    cancel: Arc<AtomicBool>,
    #[allow(unused_variables)] fixtures: Option<Vec<bindings::cardbe::plugin::types::HttpResponse>>,
    timeout: Duration,
) -> Result<RuntimeOutput, RuntimeError> {
    if bytes.len() > 16 * 1024 * 1024
        || input.config_json.len() > 64 * 1024
        || input.board_json.len() > 2 * 1024 * 1024
    {
        return Err(RuntimeError::ResourceLimit);
    }
    if cancel.load(Ordering::Acquire) {
        return Err(RuntimeError::Cancelled);
    }
    let (engine, component, linker) = prepare_component(bytes)?;
    let deadline = Instant::now() + timeout;
    let state = HostState {
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
        requests: 0,
        response_bytes: 0,
        failure: None,
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
        let instance = bindings::Importer::instantiate(&mut store, &component, &linker)
            .map_err(|_| RuntimeError::InvalidComponent)?;
        let context = bindings::cardbe::plugin::types::RunContext {
            instance_id: input.instance_id,
            run_id: input.run_id,
            config_json: input.config_json,
            board_json: input.board_json,
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
                .cursor
                .as_ref()
                .is_some_and(|cursor| cursor.len() > 4096)
        {
            return Err(RuntimeError::ResourceLimit);
        }
        let tasks: Vec<_> = changeset
            .tasks
            .into_iter()
            .map(|task| ExternalTask {
                external_key: task.external_key,
                title: task.title,
                description: task.description,
                url: task.url,
                state: task.state,
                labels: task.labels,
                updated_at: task.updated_at,
                column_id: task.column_id,
            })
            .collect();
        let encoded = serde_json::to_vec(&tasks).map_err(|_| RuntimeError::ResourceLimit)?;
        if encoded.len() > 8 * 1024 * 1024 {
            return Err(RuntimeError::ResourceLimit);
        }
        Ok(RuntimeOutput {
            tasks,
            cursor: changeset.cursor,
            logs: std::mem::take(&mut store.data_mut().logs),
        })
    })();
    stop.store(true, Ordering::Release);
    let _ = ticker.join();
    if cancel.load(Ordering::Acquire) {
        return Err(RuntimeError::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(RuntimeError::Timeout);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn update_validation_checks_the_host_contract_without_executing_the_guest() {
        assert!(validate_component(&component("runtime-fixture")).is_ok());
        assert!(validate_component(b"invalid").is_err());
        assert!(validate_component(b"(component)").is_err());
        assert!(validate_component(b"(component (import \"wasi:cli/run\" (instance)))").is_err());
    }
    fn input(config: &str) -> RuntimeInput {
        RuntimeInput {
            instance_id: "instance".into(),
            run_id: "run".into(),
            config_json: config.into(),
            board_json: "{}".into(),
            cursor: None,
        }
    }
    fn policy() -> HttpPolicy {
        HttpPolicy {
            allowed_domains: vec!["api.example.com".into()],
            requested_domains: vec![],
            allow_custom_domains: false,
            token: None,
            token_domain: None,
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
    #[test]
    fn fixture_component_imports_only_cardbe_and_exports_run() {
        let engine = Engine::default();
        let component = Component::new(&engine, component("runtime-fixture")).unwrap();
        let ty = component.component_type();
        let mut imports: Vec<_> = ty.imports(&engine).map(|(name, _)| name).collect();
        imports.sort_unstable();
        assert_eq!(
            imports,
            ["cardbe:plugin/host@1.0.0", "cardbe:plugin/types@1.0.0"]
        );
        let exports: Vec<_> = ty.exports(&engine).map(|(name, _)| name).collect();
        assert_eq!(exports, ["run"]);
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
        )
    }
    #[tokio::test(flavor = "multi_thread")]
    async fn actual_component_calls_http_and_returns_typed_changeset() {
        let response = bindings::cardbe::plugin::types::HttpResponse {
            status: 200,
            headers: vec![],
            body: b"Fixture response".to_vec(),
        };
        let output = tokio::task::spawn_blocking(move || {
            execute_inner(
                &component("runtime-fixture"),
                input("http-success"),
                policy(),
                Arc::new(AtomicBool::new(false)),
                Some(vec![response]),
                Duration::from_secs(10),
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
        assert_eq!(output.logs[0].code, "ALLOWED_LOG");
        assert_eq!(output.cursor.as_deref(), Some("next"));
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
                execute(&[], input("ok"), policy(), Arc::new(AtomicBool::new(false))),
                Err(RuntimeError::InvalidComponent)
            ));
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
    bindings::Importer::add_to_linker::<_, wasmtime::component::HasSelf<_>>(
        &mut linker,
        |state: &mut HostState| state,
    )
    .map_err(|_| RuntimeError::InvalidComponent)?;
    Ok((engine, component, linker))
}

pub fn validate_component(bytes: &[u8]) -> Result<(), RuntimeError> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(RuntimeError::ResourceLimit);
    }
    let (_, component, linker) = prepare_component(bytes)?;
    let pre = linker
        .instantiate_pre(&component)
        .map_err(|_| RuntimeError::InvalidComponent)?;
    bindings::ImporterPre::new(pre).map_err(|_| RuntimeError::InvalidComponent)?;
    Ok(())
}
