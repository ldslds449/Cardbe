//! Registry operations. Reads use one read-only SQLite snapshot per call,
//! outside the application lock; writes go through `board_write::commit`.
use super::{
    board_write::commit,
    columns,
    dto::{self, present},
    idempotency::{self, Recorded},
    tasks, Access, Operation, OperationContext, OperationHost,
};
use crate::{
    board_mutations as mutation,
    commands::archive::{archive_task_in, unarchive_task_in},
    errors::DomainError,
    models::{self, StoredData},
    plugins::board::Reader,
    state::lock_app,
    storage::{Database, TaskExplorerQuery},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn database_path(host: &dyn OperationHost) -> Result<PathBuf, DomainError> {
    Ok(lock_app(host.app_data()?)?.database.path().to_path_buf())
}

fn reader(host: &dyn OperationHost, board_id: i64) -> Result<Reader, DomainError> {
    Reader::open(database_path(host)?, board_id)
}

fn now() -> u128 {
    crate::plugins::types::now() as u128
}

fn active_task(data: &StoredData, board_id: i64, task_id: i64) -> Result<dto::Task, DomainError> {
    let (column, index) = mutation::task_position(data, task_id)?;
    dto::Task::new(
        board_id,
        Some(data.columns[column].id),
        false,
        data.columns[column].tasks[index].clone(),
    )
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoInput {}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoardInput {
    pub board_id: i64,
}

pub struct ListBoards;
impl Operation for ListBoards {
    const NAME: &'static str = "board.list";
    const DESCRIPTION: &'static str =
        "List the Cardbe boards this client may access, with the device's role on each.";
    const ACCESS: Access = Access::Read;
    type Input = NoInput;
    type Output = dto::BoardList;
    fn board(_: &NoInput) -> Option<i64> {
        None
    }
    fn run(
        host: &dyn OperationHost,
        context: &OperationContext,
        _: NoInput,
    ) -> Result<dto::BoardList, DomainError> {
        let database =
            Database::open_search_reader(database_path(host)?).map_err(DomainError::repository)?;
        let all = database.boards().map_err(DomainError::repository)?;
        let total = all.len();
        let boards: Vec<_> = all
            .into_iter()
            .filter(|board| context.grants.allows(board.id, Access::Read))
            .map(|board| dto::Board {
                writable: context.grants.allows(board.id, Access::Write)
                    && board.shared_role != models::BoardRole::Viewer,
                id: board.id,
                name: board.name,
                task_count: board.task_count,
                role: board.shared_role.into(),
                is_shared: board.is_shared,
            })
            .collect();
        Ok(dto::BoardList {
            hidden_board_count: total - boards.len(),
            boards,
        })
    }
}

pub struct ListColumns;
impl Operation for ListColumns {
    const NAME: &'static str = "column.list";
    const DESCRIPTION: &'static str = "List a board's columns in display order.";
    const ACCESS: Access = Access::Read;
    type Input = BoardInput;
    type Output = dto::ColumnList;
    fn board(input: &BoardInput) -> Option<i64> {
        Some(input.board_id)
    }
    fn run(
        host: &dyn OperationHost,
        _: &OperationContext,
        input: BoardInput,
    ) -> Result<dto::ColumnList, DomainError> {
        let columns = reader(host, input.board_id)?
            .list_columns()?
            .into_iter()
            .map(dto::Column::from)
            .collect();
        Ok(dto::ColumnList { columns })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateColumnInput {
    pub board_id: i64,
    pub name: String,
    #[serde(default)]
    pub color: String,
    /// Insert before this column; omitted appends at the end.
    #[serde(default)]
    pub before_column_id: Option<i64>,
}

pub struct CreateColumn;
impl Operation for CreateColumn {
    const NAME: &'static str = "column.create";
    const DESCRIPTION: &'static str =
        "Create a manually ordered column, at the end or before another column.";
    const ACCESS: Access = Access::Write;
    type Input = CreateColumnInput;
    type Output = dto::ColumnResult;
    fn board(input: &CreateColumnInput) -> Option<i64> {
        Some(input.board_id)
    }
    fn run(
        host: &dyn OperationHost,
        _: &OperationContext,
        input: CreateColumnInput,
    ) -> Result<dto::ColumnResult, DomainError> {
        commit(host, input.board_id, |data| {
            let id = columns::create_column(data, input.name.trim().to_owned(), input.color)?;
            mutation::move_column(data, id, input.before_column_id)?;
            let column = &data.columns[mutation::column_index(data, id)?];
            Ok(dto::ColumnResult {
                column: column.into(),
            })
        })
    }
}

/// Omitted fields keep their values.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateColumnInput {
    pub board_id: i64,
    pub column_id: i64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

pub struct UpdateColumn;
impl Operation for UpdateColumn {
    const NAME: &'static str = "column.update";
    const DESCRIPTION: &'static str = "Rename or recolor a column. Its sort order is unchanged.";
    const ACCESS: Access = Access::Write;
    type Input = UpdateColumnInput;
    type Output = dto::ColumnResult;
    fn board(input: &UpdateColumnInput) -> Option<i64> {
        Some(input.board_id)
    }
    fn run(
        host: &dyn OperationHost,
        _: &OperationContext,
        input: UpdateColumnInput,
    ) -> Result<dto::ColumnResult, DomainError> {
        commit(host, input.board_id, |data| {
            let current = &data.columns[mutation::column_index(data, input.column_id)?];
            let name = input
                .name
                .map_or_else(|| current.name.clone(), |name| name.trim().to_owned());
            let color = input.color.unwrap_or_else(|| current.color.clone());
            let index = columns::update_column(data, input.column_id, name, color)?;
            Ok(dto::ColumnResult {
                column: (&data.columns[index]).into(),
            })
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MoveColumnInput {
    pub board_id: i64,
    pub column_id: i64,
    /// Move before this column; omitted moves to the end.
    #[serde(default)]
    pub before_column_id: Option<i64>,
}

pub struct MoveColumn;
impl Operation for MoveColumn {
    const NAME: &'static str = "column.move";
    const DESCRIPTION: &'static str =
        "Move a column before another column or to the end; returns the new order.";
    const ACCESS: Access = Access::Write;
    type Input = MoveColumnInput;
    type Output = dto::ColumnList;
    fn board(input: &MoveColumnInput) -> Option<i64> {
        Some(input.board_id)
    }
    fn run(
        host: &dyn OperationHost,
        _: &OperationContext,
        input: MoveColumnInput,
    ) -> Result<dto::ColumnList, DomainError> {
        commit(host, input.board_id, |data| {
            columns::move_column(data, input.column_id, input.before_column_id)?;
            Ok(dto::ColumnList {
                columns: data.columns.iter().map(dto::Column::from).collect(),
            })
        })
    }
}

fn default_limit() -> u32 {
    50
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListTasksInput {
    pub board_id: i64,
    /// Matches titles, descriptions, labels and checklist text; empty lists all.
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub column_id: Option<i64>,
    #[serde(default)]
    pub status: dto::TaskStatus,
    #[serde(default)]
    pub offset: u32,
    /// 1 to 100.
    #[serde(default = "default_limit")]
    pub limit: u32,
}

pub struct ListTasks;
impl Operation for ListTasks {
    const NAME: &'static str = "task.list";
    const DESCRIPTION: &'static str = "Search or page through a board's tasks.";
    const ACCESS: Access = Access::Read;
    type Input = ListTasksInput;
    type Output = dto::TaskPage;
    fn board(input: &ListTasksInput) -> Option<i64> {
        Some(input.board_id)
    }
    fn run(
        host: &dyn OperationHost,
        _: &OperationContext,
        input: ListTasksInput,
    ) -> Result<dto::TaskPage, DomainError> {
        let filter = TaskExplorerQuery {
            query: input.query,
            column_id: input.column_id,
            status: input.status.as_filter().into(),
            ..TaskExplorerQuery::default()
        };
        let tasks = reader(host, input.board_id)?
            .list_tasks(filter, input.offset, input.limit)?
            .into_iter()
            .map(dto::Task::try_from)
            .collect::<Result<_, _>>()?;
        Ok(dto::TaskPage {
            tasks,
            offset: input.offset,
            limit: input.limit,
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetTaskInput {
    pub board_id: i64,
    pub task_id: i64,
    /// Look the task up among archived tasks.
    #[serde(default)]
    pub archived: bool,
}

pub struct GetTask;
impl Operation for GetTask {
    const NAME: &'static str = "task.get";
    const DESCRIPTION: &'static str = "Get one task, including its description and checklist.";
    const ACCESS: Access = Access::Read;
    type Input = GetTaskInput;
    type Output = dto::TaskResult;
    fn board(input: &GetTaskInput) -> Option<i64> {
        Some(input.board_id)
    }
    fn run(
        host: &dyn OperationHost,
        _: &OperationContext,
        input: GetTaskInput,
    ) -> Result<dto::TaskResult, DomainError> {
        let task = reader(host, input.board_id)?.get_task(input.task_id, input.archived)?;
        Ok(dto::TaskResult {
            task: task.try_into()?,
        })
    }
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateTaskInput {
    pub board_id: i64,
    pub column_id: i64,
    pub title: String,
    /// Markdown.
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub items: Vec<dto::TaskItemInput>,
    /// Unix epoch milliseconds.
    #[serde(default)]
    pub start_time: Option<u64>,
    /// Unix epoch milliseconds; local midnight means the whole day.
    #[serde(default)]
    pub due_time: Option<u64>,
    #[serde(default)]
    pub start_at_midnight: bool,
    #[serde(default)]
    pub due_at_midnight: bool,
    #[serde(default)]
    pub recurrence: Option<dto::Recurrence>,
    /// Retrying with the same key and arguments returns the task the first
    /// call created instead of creating another. Keys (1 to 128 bytes) are
    /// remembered for a day while Cardbe runs; other arguments with a used
    /// key are rejected.
    #[serde(default, skip_serializing)]
    pub idempotency_key: Option<String>,
}

pub struct CreateTask;
impl Operation for CreateTask {
    const NAME: &'static str = "task.create";
    const DESCRIPTION: &'static str = "Create a task at the end of a column.";
    const ACCESS: Access = Access::Write;
    type Input = CreateTaskInput;
    type Output = dto::TaskResult;
    fn board(input: &CreateTaskInput) -> Option<i64> {
        Some(input.board_id)
    }
    fn run(
        host: &dyn OperationHost,
        context: &OperationContext,
        mut input: CreateTaskInput,
    ) -> Result<dto::TaskResult, DomainError> {
        let Some(key) = input.idempotency_key.take() else {
            return create_task(host, input).map(|(result, _)| result);
        };
        let fingerprint = idempotency::fingerprint(&input)?;
        let board_id = input.board_id;
        match host
            .created_tasks()?
            .once(&context.caller, &key, board_id, fingerprint, || {
                create_task(host, input)
            })? {
            Recorded::New(result) => Ok(result),
            // The task may have been archived since.
            Recorded::Existing { board_id, task_id } => {
                let reader = reader(host, board_id)?;
                let task = match reader.get_task(task_id, false) {
                    Err(DomainError::TaskNotFound) => reader.get_task(task_id, true)?,
                    task => task?,
                };
                Ok(dto::TaskResult {
                    task: task.try_into()?,
                })
            }
        }
    }
}

fn create_task(
    host: &dyn OperationHost,
    input: CreateTaskInput,
) -> Result<(dto::TaskResult, i64), DomainError> {
    let task = models::Task {
        title: input.title,
        description: input.description,
        color: input.color,
        pinned: input.pinned,
        labels: input.labels,
        items: dto::items(input.items),
        start_time: input.start_time.map_or(0, u128::from),
        due_time: input.due_time.map(u128::from),
        start_at_midnight: input.start_at_midnight,
        due_at_midnight: input.due_at_midnight,
        recurrence: input.recurrence.map(models::Recurrence::from),
        ..models::Task::default()
    };
    commit(host, input.board_id, |data| {
        let id = tasks::create_task(data, input.column_id, task, now())?;
        Ok((
            dto::TaskResult {
                task: active_task(data, input.board_id, id)?,
            },
            id,
        ))
    })
}

/// Omitted fields keep their values; `null` clears an optional field.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateTaskInput {
    pub board_id: i64,
    pub task_id: i64,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub pinned: Option<bool>,
    #[serde(default)]
    pub labels: Option<Vec<String>>,
    /// Replaces the whole checklist; keep `id` to preserve an existing item.
    #[serde(default)]
    pub items: Option<Vec<dto::TaskItemInput>>,
    #[serde(default, deserialize_with = "present")]
    pub start_time: Option<Option<u64>>,
    #[serde(default, deserialize_with = "present")]
    pub due_time: Option<Option<u64>>,
    #[serde(default)]
    pub start_at_midnight: Option<bool>,
    #[serde(default)]
    pub due_at_midnight: Option<bool>,
    #[serde(default, deserialize_with = "present")]
    pub recurrence: Option<Option<dto::Recurrence>>,
}

pub struct UpdateTask;
impl Operation for UpdateTask {
    const NAME: &'static str = "task.update";
    const DESCRIPTION: &'static str = "Change fields of an active task.";
    const ACCESS: Access = Access::Write;
    type Input = UpdateTaskInput;
    type Output = dto::TaskResult;
    fn board(input: &UpdateTaskInput) -> Option<i64> {
        Some(input.board_id)
    }
    fn run(
        host: &dyn OperationHost,
        _: &OperationContext,
        input: UpdateTaskInput,
    ) -> Result<dto::TaskResult, DomainError> {
        commit(host, input.board_id, |data| {
            let (column, index) = mutation::task_position(data, input.task_id)?;
            let mut task = data.columns[column].tasks[index].clone();
            // Callers see a legacy creation time as no start; the update keeps it.
            task.start_time = tasks::external_times(&task).0;
            task.created_time = 0;
            if let Some(title) = input.title {
                task.title = title;
            }
            if let Some(description) = input.description {
                task.description = description;
            }
            if let Some(color) = input.color {
                task.color = color;
            }
            if let Some(pinned) = input.pinned {
                task.pinned = pinned;
            }
            if let Some(labels) = input.labels {
                task.labels = labels;
            }
            if let Some(items) = input.items {
                task.items = dto::items(items);
            }
            if let Some(start_time) = input.start_time {
                task.start_time = start_time.map_or(0, u128::from);
            }
            if let Some(due_time) = input.due_time {
                task.due_time = due_time.map(u128::from);
            }
            if let Some(flag) = input.start_at_midnight {
                task.start_at_midnight = flag;
            }
            if let Some(flag) = input.due_at_midnight {
                task.due_at_midnight = flag;
            }
            if let Some(recurrence) = input.recurrence {
                task.recurrence = recurrence.map(models::Recurrence::from);
            }
            tasks::update_task(data, input.task_id, task)?;
            active_task(data, input.board_id, input.task_id)
        })
        .map(|task| dto::TaskResult { task })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MoveTaskInput {
    pub board_id: i64,
    pub task_id: i64,
    /// Destination column; it must use custom ordering.
    pub column_id: i64,
    /// Place the task before this task; omit to place it last.
    #[serde(default)]
    pub before_task_id: Option<i64>,
}

pub struct MoveTask;
impl Operation for MoveTask {
    const NAME: &'static str = "task.move";
    const DESCRIPTION: &'static str = "Move an active task to a position in a column.";
    const ACCESS: Access = Access::Write;
    type Input = MoveTaskInput;
    type Output = dto::TaskResult;
    fn board(input: &MoveTaskInput) -> Option<i64> {
        Some(input.board_id)
    }
    fn run(
        host: &dyn OperationHost,
        _: &OperationContext,
        input: MoveTaskInput,
    ) -> Result<dto::TaskResult, DomainError> {
        commit(host, input.board_id, |data| {
            tasks::move_task(data, input.task_id, input.column_id, input.before_task_id)?;
            active_task(data, input.board_id, input.task_id)
        })
        .map(|task| dto::TaskResult { task })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskInput {
    pub board_id: i64,
    pub task_id: i64,
}

pub struct ArchiveTask;
impl Operation for ArchiveTask {
    const NAME: &'static str = "task.archive";
    const DESCRIPTION: &'static str = "Archive an active task; task.restore brings it back. A recurring task is replaced by its next occurrence.";
    const ACCESS: Access = Access::Write;
    type Input = TaskInput;
    type Output = dto::ArchiveResult;
    fn board(input: &TaskInput) -> Option<i64> {
        Some(input.board_id)
    }
    fn run(
        host: &dyn OperationHost,
        _: &OperationContext,
        input: TaskInput,
    ) -> Result<dto::ArchiveResult, DomainError> {
        commit(host, input.board_id, |data| {
            let next = archive_task_in(data, input.task_id, now())?;
            let archived = data
                .archives
                .last()
                .map(|archive| archive.task.clone())
                .ok_or(DomainError::TaskNotFound)?;
            Ok(dto::ArchiveResult {
                task: dto::Task::new(input.board_id, None, true, archived)?,
                next_task: next
                    .map(|id| active_task(data, input.board_id, id))
                    .transpose()?,
            })
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RestoreTaskInput {
    pub board_id: i64,
    pub task_id: i64,
    /// Column that receives the task at its end.
    pub column_id: i64,
}

pub struct RestoreTask;
impl Operation for RestoreTask {
    const NAME: &'static str = "task.restore";
    const DESCRIPTION: &'static str = "Restore an archived task to the end of a column.";
    const ACCESS: Access = Access::Write;
    type Input = RestoreTaskInput;
    type Output = dto::TaskResult;
    fn board(input: &RestoreTaskInput) -> Option<i64> {
        Some(input.board_id)
    }
    fn run(
        host: &dyn OperationHost,
        _: &OperationContext,
        input: RestoreTaskInput,
    ) -> Result<dto::TaskResult, DomainError> {
        commit(host, input.board_id, |data| {
            unarchive_task_in(data, input.column_id, input.task_id)?;
            active_task(data, input.board_id, input.task_id)
        })
        .map(|task| dto::TaskResult { task })
    }
}
