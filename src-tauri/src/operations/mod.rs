//! Shared operation layer for external callers. The MCP server lists and runs
//! operations through this registry; plugin changesets apply the same
//! validated column and task mutations (`columns`, `tasks`) and persistence (`board_write`).
pub(crate) mod board_write;
pub(crate) mod columns;
mod dto;
mod handlers;
pub mod idempotency;
pub(crate) mod tasks;

use crate::{errors::DomainError, state::SharedAppData};
use schemars::JsonSchema;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicU64, Ordering},
};
use tauri::{Emitter, Manager};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
}

/// Established by the caller's adapter after authentication; never taken from
/// operation arguments.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Caller {
    Mcp { client_id: String },
}

/// Boards the caller may read, and whether it may also change them.
/// `all_boards` covers boards created later, checked on every call.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Grants {
    pub all_boards: bool,
    pub boards: BTreeSet<i64>,
    pub write: bool,
}

impl Grants {
    pub fn allows(&self, board_id: i64, access: Access) -> bool {
        (self.all_boards || self.boards.contains(&board_id))
            && (access == Access::Read || self.write)
    }
    fn exposes(&self, access: Access) -> bool {
        (self.all_boards || !self.boards.is_empty()) && (access == Access::Read || self.write)
    }
}

pub struct OperationContext {
    pub caller: Caller,
    pub grants: Grants,
    /// Correlates diagnostics for one call; carries no caller data.
    pub operation_id: u64,
}

impl OperationContext {
    pub fn new(caller: Caller, grants: Grants) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            caller,
            grants,
            operation_id: NEXT.fetch_add(1, Ordering::Relaxed),
        }
    }
}

/// Application services available to handlers, so adapters and tests can
/// supply their own state and change notifications.
pub trait OperationHost: Send + Sync {
    fn app_data(&self) -> Result<&SharedAppData, DomainError>;
    fn creations(&self) -> Result<&idempotency::Creations, DomainError>;
    fn board_changed(&self, board_id: i64);
}

impl OperationHost for tauri::AppHandle {
    fn app_data(&self) -> Result<&SharedAppData, DomainError> {
        self.try_state::<SharedAppData>()
            .map(|state| state.inner())
            .ok_or_else(|| "Application data unavailable".into())
    }
    fn creations(&self) -> Result<&idempotency::Creations, DomainError> {
        self.try_state::<idempotency::Creations>()
            .map(|state| state.inner())
            .ok_or_else(|| "Idempotency keys unavailable".into())
    }
    fn board_changed(&self, board_id: i64) {
        let _ = self.emit(
            "cardbe:data-changed",
            serde_json::json!({"kind":"task","boardId":board_id}),
        );
    }
}

#[derive(Debug)]
pub enum OperationError {
    UnknownOperation,
    InvalidInput,
    PermissionDenied,
    Domain(DomainError),
}

impl From<DomainError> for OperationError {
    fn from(error: DomainError) -> Self {
        match error {
            DomainError::InvalidArgument => Self::InvalidInput,
            DomainError::PermissionDenied => Self::PermissionDenied,
            error => Self::Domain(error),
        }
    }
}

pub(crate) trait Operation {
    const NAME: &'static str;
    const DESCRIPTION: &'static str;
    const ACCESS: Access;
    type Input: DeserializeOwned + JsonSchema;
    type Output: Serialize + JsonSchema;
    /// The board checked against the caller's grants before the handler runs;
    /// `None` only for operations that filter boards themselves.
    fn board(input: &Self::Input) -> Option<i64>;
    fn run(
        host: &dyn OperationHost,
        context: &OperationContext,
        input: Self::Input,
    ) -> Result<Self::Output, DomainError>;
}

pub struct OperationSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub access: Access,
    pub input_schema: fn() -> Map<String, Value>,
    pub output_schema: fn() -> Map<String, Value>,
    execute: fn(&dyn OperationHost, &OperationContext, Value) -> Result<Value, OperationError>,
}

const fn spec<O: Operation>() -> OperationSpec {
    OperationSpec {
        name: O::NAME,
        description: O::DESCRIPTION,
        access: O::ACCESS,
        input_schema: schema::<O::Input>,
        output_schema: schema::<O::Output>,
        execute: execute::<O>,
    }
}

/// JSON Schema 2020-12, as MCP tool definitions expect.
fn schema<T: JsonSchema>() -> Map<String, Value> {
    let schema = schemars::generate::SchemaSettings::draft2020_12()
        .into_generator()
        .into_root_schema_for::<T>();
    match serde_json::to_value(schema) {
        Ok(Value::Object(object)) => object,
        _ => Map::new(),
    }
}

fn execute<O: Operation>(
    host: &dyn OperationHost,
    context: &OperationContext,
    arguments: Value,
) -> Result<Value, OperationError> {
    let input: O::Input =
        serde_json::from_value(arguments).map_err(|_| OperationError::InvalidInput)?;
    if O::board(&input).is_some_and(|board| !context.grants.allows(board, O::ACCESS)) {
        return Err(OperationError::PermissionDenied);
    }
    let output = O::run(host, context, input)?;
    serde_json::to_value(output).map_err(|error| DomainError::Internal(error.to_string()).into())
}

pub static OPERATIONS: [OperationSpec; 15] = [
    spec::<handlers::ListBoards>(),
    spec::<handlers::ListColumns>(),
    spec::<handlers::CreateColumn>(),
    spec::<handlers::UpdateColumn>(),
    spec::<handlers::MoveColumn>(),
    spec::<handlers::ListTasks>(),
    spec::<handlers::SearchTasks>(),
    spec::<handlers::GetTask>(),
    spec::<handlers::CreateTask>(),
    spec::<handlers::UpdateTask>(),
    spec::<handlers::MoveTask>(),
    spec::<handlers::ArchiveTask>(),
    spec::<handlers::RestoreTask>(),
    spec::<handlers::CompleteTask>(),
    spec::<handlers::ReopenTask>(),
];

/// Operations the caller's grants allow, so others are never advertised.
pub fn visible(grants: &Grants) -> impl Iterator<Item = &'static OperationSpec> + '_ {
    OPERATIONS
        .iter()
        .filter(|operation| grants.exposes(operation.access))
}

pub fn dispatch(
    host: &dyn OperationHost,
    context: &OperationContext,
    name: &str,
    arguments: Value,
) -> Result<Value, OperationError> {
    let operation = visible(&context.grants)
        .find(|operation| operation.name == name)
        .ok_or(OperationError::UnknownOperation)?;
    (operation.execute)(host, context, arguments)
}

#[cfg(test)]
pub(crate) mod tests;
