//! Streamable HTTP MCP server in the Cardbe process. It binds only to
//! loopback, authenticates every request with a client's Bearer token, and
//! lists and runs operations from the shared registry within that client's grants.
use super::config::{self, McpClient};
use crate::{
    errors::{CommandError, DomainError},
    operations::{
        self, Access, Caller, Grants, OperationContext, OperationError, OperationHost,
        OperationSpec,
    },
};
use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    Router,
};
use rmcp::{
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
    },
    service::RequestContext,
    transport::streamable_http_server::{
        session::never::NeverSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    ErrorData, RoleServer, ServerHandler,
};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::VecDeque,
    net::{Ipv4Addr, SocketAddr},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock,
    },
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use tokio_util::sync::CancellationToken;

pub const PATH: &str = "/mcp";
const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_ACTIVITY: usize = 200;

pub fn endpoint(port: u16) -> String {
    format!("http://127.0.0.1:{port}{PATH}")
}

/// MCP tool names avoid dots, which some clients reject.
fn tool_name(operation: &str) -> String {
    operation.replace('.', "_")
}

struct AuthorizedClient {
    id: String,
    digest: [u8; 32],
    grants: Grants,
}

/// Inserted by the authentication layer; handlers trust only this identity.
#[derive(Clone)]
struct Identity(String);

/// One tool call, kept in memory for the settings view. Arguments and
/// results are not recorded.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Activity {
    /// Unix epoch milliseconds.
    pub at: i64,
    pub client_id: String,
    pub tool: String,
    pub board_id: Option<i64>,
    /// The public error code; `None` when the call succeeded.
    pub error: Option<String>,
    pub duration_ms: u64,
}

struct Shared {
    host: Arc<dyn OperationHost>,
    clients: RwLock<Vec<AuthorizedClient>>,
    /// Newest first.
    activity: Mutex<VecDeque<Activity>>,
    on_activity: Option<Box<dyn Fn() + Send + Sync>>,
}

impl Shared {
    /// Compares against every client in constant time per digest.
    fn identify(&self, token: &str) -> Option<String> {
        let digest = config::token_digest(token);
        let clients = self.clients.read().ok()?;
        let mut found = None;
        for client in clients.iter() {
            if bool::from(client.digest[..].ct_eq(&digest[..])) {
                found = Some(client.id.clone());
            }
        }
        found
    }
    /// Current grants, so changes apply to the next request without a restart.
    fn grants(&self, client_id: &str) -> Option<Grants> {
        self.clients
            .read()
            .ok()?
            .iter()
            .find(|client| client.id == client_id)
            .map(|client| client.grants.clone())
    }
    fn record(&self, activity: Activity) {
        if let Ok(mut recent) = self.activity.lock() {
            recent.push_front(activity);
            recent.truncate(MAX_ACTIVITY);
        }
        if let Some(notify) = &self.on_activity {
            notify();
        }
    }
}

async fn authenticate(
    State(shared): State<Arc<Shared>>,
    mut request: Request,
    next: Next,
) -> Response {
    let identity = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .and_then(|token| shared.identify(token.trim()));
    match identity {
        Some(client_id) => {
            request.extensions_mut().insert(Identity(client_id));
            next.run(request).await
        }
        None => (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
        )
            .into_response(),
    }
}

#[derive(Clone)]
struct Handler {
    shared: Arc<Shared>,
}

impl Handler {
    fn caller(&self, context: &RequestContext<RoleServer>) -> Result<(String, Grants), ErrorData> {
        let client_id = context
            .extensions
            .get::<axum::http::request::Parts>()
            .and_then(|parts| parts.extensions.get::<Identity>())
            .map(|identity| identity.0.clone())
            .ok_or_else(|| ErrorData::invalid_request("Unauthorized", None))?;
        // A client removed after authentication has no grants left.
        let grants = self
            .shared
            .grants(&client_id)
            .ok_or_else(|| ErrorData::invalid_request("Unauthorized", None))?;
        Ok((client_id, grants))
    }
}

fn tool(operation: &OperationSpec) -> Tool {
    let read_only = operation.access == Access::Read;
    Tool::new(
        tool_name(operation.name),
        operation.description,
        Arc::new((operation.input_schema)()),
    )
    .with_raw_output_schema(Arc::new((operation.output_schema)()))
    .with_annotations(
        ToolAnnotations::new()
            .read_only(read_only)
            .destructive(false)
            .idempotent(read_only)
            .open_world(false),
    )
}

/// Tool failures the model can act on; internal details stay in the backend log.
fn tool_error(error: Value) -> CallToolResponse {
    CallToolResult::structured_error(error).into()
}

fn public_error(error: CommandError) -> Value {
    serde_json::to_value(error).unwrap_or_default()
}

impl ServerHandler for Handler {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new("cardbe", env!("CARGO_PKG_VERSION")).with_title("Cardbe"),
            )
            .with_instructions(
                "Cardbe boards contain columns of tasks. Start with board_list, then column_list. \
                 Times are Unix epoch milliseconds. Archive tasks instead of deleting them.",
            )
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let (_, grants) = self.caller(&context)?;
        Ok(ListToolsResult::with_all_items(
            operations::visible(&grants).map(tool).collect(),
        ))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let (client_id, grants) = self.caller(&context)?;
        let Some(name) = operations::visible(&grants)
            .find(|operation| tool_name(operation.name) == request.name)
            .map(|operation| operation.name)
        else {
            return Err(ErrorData::invalid_params("Unknown tool", None));
        };
        let arguments = Value::Object(request.arguments.unwrap_or_default());
        let board_id = arguments.get("board_id").and_then(Value::as_i64);
        let started = Instant::now();
        let operation = OperationContext::new(Caller::Mcp { client_id }, grants);
        let host = Arc::clone(&self.shared.host);
        let result = tokio::task::spawn_blocking(move || {
            let result = operations::dispatch(&*host, &operation, name, arguments);
            let Caller::Mcp { client_id } = &operation.caller;
            log::info!(
                target: "mcp",
                "Operation {} {name} by client {client_id}: {}",
                operation.operation_id,
                if result.is_ok() { "succeeded" } else { "failed" }
            );
            (result, operation.caller)
        })
        .await
        .map_err(|_| ErrorData::internal_error("Operation failed", None))?;
        let (result, Caller::Mcp { client_id }) = result;
        let result = match result {
            Ok(value) => Ok(value),
            // Unreachable for visible operations; kept as a protocol error.
            Err(OperationError::UnknownOperation) => {
                return Err(ErrorData::invalid_params("Unknown tool", None))
            }
            // Input errors are tool results so the model can correct its call.
            Err(OperationError::InvalidInput) => Err(public_error(CommandError::InvalidArgument)),
            Err(OperationError::PermissionDenied) => {
                Err(public_error(CommandError::PermissionDenied))
            }
            Err(OperationError::Domain(error)) => Err(public_error(error.into())),
        };
        self.shared.record(Activity {
            at: chrono::Utc::now().timestamp_millis(),
            client_id,
            tool: request.name.to_string(),
            board_id,
            error: result.as_ref().err().map(|error| {
                error["code"]
                    .as_str()
                    .unwrap_or("INTERNAL_ERROR")
                    .to_owned()
            }),
            duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        });
        Ok(match result {
            Ok(value) => CallToolResult::structured(value).into(),
            Err(error) => tool_error(error),
        })
    }
}

fn router(shared: Arc<Shared>, port: u16, cancel: CancellationToken) -> Router {
    let authorities = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    let origins = authorities
        .iter()
        .map(|authority| format!("http://{authority}"));
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_sse_keep_alive(None)
        .with_allowed_hosts(authorities.clone())
        .with_allowed_origins(origins)
        .enforce_origin_validation()
        .with_max_request_body_bytes(MAX_REQUEST_BYTES)
        .with_cancellation_token(cancel);
    let handler = Handler {
        shared: Arc::clone(&shared),
    };
    let service = StreamableHttpService::new(
        move || Ok(handler.clone()),
        Arc::new(NeverSessionManager::default()),
        config,
    );
    Router::new()
        .nest_service(PATH, service)
        .layer(middleware::from_fn_with_state(shared, authenticate))
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Stopped,
    Running,
    /// The server stopped unexpectedly; see the backend log.
    Failed,
}

struct Running {
    port: u16,
    cancel: CancellationToken,
    exited: Arc<AtomicBool>,
    task: tauri::async_runtime::JoinHandle<()>,
}

/// Marks the server task finished, including when it unwinds.
struct ExitGuard(Arc<AtomicBool>);

impl Drop for ExitGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// The one owner of the server's lifecycle and the authorized-client snapshot.
pub struct McpServer {
    shared: Arc<Shared>,
    running: Mutex<Option<Running>>,
}

impl McpServer {
    /// `on_activity` runs after each recorded tool call.
    pub fn new(
        host: Arc<dyn OperationHost>,
        on_activity: Option<Box<dyn Fn() + Send + Sync>>,
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                host,
                clients: RwLock::new(Vec::new()),
                activity: Mutex::new(VecDeque::new()),
                on_activity,
            }),
            running: Mutex::new(None),
        }
    }

    /// Recent tool calls, newest first. Lost when Cardbe restarts.
    pub fn activity(&self) -> Vec<Activity> {
        self.shared
            .activity
            .lock()
            .map(|recent| recent.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn clear_activity(&self) {
        if let Ok(mut recent) = self.shared.activity.lock() {
            recent.clear();
        }
    }

    /// Replaces the authorized clients; malformed digests never authenticate.
    pub fn set_clients(&self, clients: &[McpClient]) {
        let authorized = clients
            .iter()
            .filter_map(|client| {
                Some(AuthorizedClient {
                    id: client.id.clone(),
                    digest: config::parse_hash(&client.token_hash)?,
                    grants: client.grants(),
                })
            })
            .collect();
        if let Ok(mut current) = self.shared.clients.write() {
            *current = authorized;
        }
    }

    pub fn status(&self) -> (Status, Option<u16>) {
        match self.running.lock().ok().as_deref() {
            Some(Some(running)) if running.exited.load(Ordering::SeqCst) => {
                (Status::Failed, Some(running.port))
            }
            Some(Some(running)) => (Status::Running, Some(running.port)),
            _ => (Status::Stopped, None),
        }
    }

    /// Starts on `port` (0 picks one), or returns the running server's port.
    pub fn start(&self, port: u16) -> Result<u16, DomainError> {
        let mut running = self
            .running
            .lock()
            .map_err(|_| DomainError::Internal("MCP server state is poisoned".into()))?;
        if let Some(current) = running.as_ref() {
            if !current.exited.load(Ordering::SeqCst) {
                return Ok(current.port);
            }
        }
        let listener = std::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
            .map_err(|error| match error.kind() {
            std::io::ErrorKind::AddrInUse | std::io::ErrorKind::PermissionDenied => {
                DomainError::McpPortUnavailable
            }
            _ => DomainError::Internal(format!("Could not start the MCP server: {error}")),
        })?;
        listener
            .set_nonblocking(true)
            .map_err(|error| DomainError::Internal(error.to_string()))?;
        let port = listener
            .local_addr()
            .map_err(|error| DomainError::Internal(error.to_string()))?
            .port();
        let cancel = CancellationToken::new();
        let exited = Arc::new(AtomicBool::new(false));
        let app = router(Arc::clone(&self.shared), port, cancel.child_token());
        let task = tauri::async_runtime::spawn({
            let cancel = cancel.clone();
            let guard = ExitGuard(Arc::clone(&exited));
            async move {
                let _guard = guard;
                let listener = match tokio::net::TcpListener::from_std(listener) {
                    Ok(listener) => listener,
                    Err(error) => {
                        log::error!(target: "mcp", "Could not start the MCP listener: {error}");
                        return;
                    }
                };
                let shutdown = cancel.clone();
                if let Err(error) = axum::serve(listener, app)
                    .with_graceful_shutdown(async move { shutdown.cancelled().await })
                    .await
                {
                    log::error!(target: "mcp", "MCP server stopped unexpectedly: {error}");
                }
                if !cancel.is_cancelled() {
                    log::error!(target: "mcp", "MCP server stopped unexpectedly");
                }
            }
        });
        log::info!(target: "mcp", "MCP server listening on {}", endpoint(port));
        *running = Some(Running {
            port,
            cancel,
            exited,
            task,
        });
        Ok(port)
    }

    /// Stops accepting requests and waits briefly for in-flight calls.
    pub async fn stop(&self) {
        let Some(running) = self.running.lock().ok().and_then(|mut slot| slot.take()) else {
            return;
        };
        running.cancel.cancel();
        let mut task = running.task;
        if tokio::time::timeout(Duration::from_secs(5), &mut task)
            .await
            .is_err()
        {
            task.abort();
        }
        log::info!(target: "mcp", "MCP server stopped");
    }
}

#[cfg(test)]
mod tests;
