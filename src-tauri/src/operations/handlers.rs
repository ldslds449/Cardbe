//! Registry operations. Reads use one read-only SQLite snapshot per call,
//! outside the application lock; writes go through `board_write::commit`.
use super::{
    board_write::{commit, commit_then},
    columns,
    dto::{self, present},
    idempotency::{self, Recorded},
    tasks, Access, Operation, OperationContext, OperationHost,
};
use crate::{
    board_mutations as mutation,
    commands::archive::{archive_task_in, unarchive_task_in},
    commands::task_explorer::MAX_LABEL_FILTERS,
    completion,
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
        let reader = reader(host, input.board_id)?;
        let done_column = reader.done_column()?;
        let columns = reader
            .list_columns()?
            .into_iter()
            .map(|column| dto::Column::from_info(column, done_column))
            .collect();
        Ok(dto::ColumnList { columns })
    }
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateColumnInput {
    pub board_id: i64,
    pub name: String,
    #[serde(default)]
    pub color: String,
    /// Insert before this column; omitted appends at the end.
    #[serde(default)]
    pub before_column_id: Option<i64>,
    /// Retrying with the same key and arguments returns the column the first
    /// call created instead of creating another. Keys (1 to 128 bytes) are
    /// remembered for a day while Cardbe runs; other arguments with a used
    /// key are rejected.
    #[serde(default, skip_serializing)]
    pub idempotency_key: Option<String>,
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
        context: &OperationContext,
        mut input: CreateColumnInput,
    ) -> Result<dto::ColumnResult, DomainError> {
        let Some(key) = input.idempotency_key.take() else {
            return create_column(host, input).map(|(result, _)| result);
        };
        let fingerprint = idempotency::fingerprint(&input)?;
        let board_id = input.board_id;
        match host.creations()?.once(
            &context.caller,
            Self::NAME,
            &key,
            board_id,
            fingerprint,
            || create_column(host, input),
        )? {
            Recorded::New(result) => Ok(result),
            // The column may have been changed or deleted since.
            Recorded::Existing { board_id, id } => {
                let reader = reader(host, board_id)?;
                let done_column = reader.done_column()?;
                Ok(dto::ColumnResult {
                    column: dto::Column::from_info(reader.get_column(id)?, done_column),
                })
            }
        }
    }
}

fn create_column(
    host: &dyn OperationHost,
    input: CreateColumnInput,
) -> Result<(dto::ColumnResult, i64), DomainError> {
    commit(host, input.board_id, |data| {
        let id = columns::create_column(data, input.name.trim().to_owned(), input.color)?;
        mutation::move_column(data, id, input.before_column_id)?;
        let column = &data.columns[mutation::column_index(data, id)?];
        Ok((
            dto::ColumnResult {
                column: dto::Column::new(column, data.done_column_id()),
            },
            id,
        ))
    })
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
    /// `true` makes this the board's only done column, so tasks in it are
    /// completed; `false` leaves the board without one if it was.
    #[serde(default)]
    pub done: Option<bool>,
    /// Replaces the display order: at most three distinct fields, most
    /// significant first. `[]` shows tasks in their manual order.
    #[serde(default)]
    pub sort_rules: Option<Vec<dto::SortRule>>,
}

pub struct UpdateColumn;
impl Operation for UpdateColumn {
    const NAME: &'static str = "column.update";
    const DESCRIPTION: &'static str =
        "Rename, recolor, or re-sort a column, or make it the board's done column.";
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
            if let Some(rules) = input.sort_rules {
                let rules: Vec<models::SortRule> = rules.into_iter().map(Into::into).collect();
                if !models::valid_sort_rules(&rules) {
                    return Err(DomainError::InvalidArgument);
                }
                data.columns[index].set_sort_rules(rules);
            }
            match input.done {
                Some(true) => completion::set_done_column(data, Some(input.column_id))?,
                Some(false) if data.done_column_id() == Some(input.column_id) => {
                    completion::set_done_column(data, None)?
                }
                _ => {}
            }
            Ok(dto::ColumnResult {
                column: dto::Column::new(&data.columns[index], data.done_column_id()),
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
            let done_column = data.done_column_id();
            Ok(dto::ColumnList {
                columns: data
                    .columns
                    .iter()
                    .map(|column| dto::Column::new(column, done_column))
                    .collect(),
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
    const DESCRIPTION: &'static str =
        "Search or page through one board's tasks; task.search filters across boards.";
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
        let (tasks, has_more) =
            reader(host, input.board_id)?.list_task_page(filter, input.offset, input.limit)?;
        Ok(dto::TaskPage {
            tasks: tasks
                .into_iter()
                .map(dto::Task::try_from)
                .collect::<Result<_, _>>()?,
            offset: input.offset,
            limit: input.limit,
            // Offsets are u32, so a page beyond them cannot be requested.
            next_offset: has_more
                .then(|| input.offset.checked_add(input.limit))
                .flatten(),
        })
    }
}

fn default_completion() -> Vec<dto::Completion> {
    vec![dto::Completion::Open, dto::Completion::Done]
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchTasksInput {
    /// Boards to search; omitted searches every board this client may read.
    #[serde(default)]
    pub board_ids: Option<Vec<i64>>,
    /// Matches titles, descriptions, labels and checklist text; empty lists all.
    #[serde(default)]
    pub query: String,
    /// Defaults to open and done tasks; add `archived` to include archived ones.
    #[serde(default = "default_completion")]
    pub completion: Vec<dto::Completion>,
    /// Active tasks past their due time only.
    #[serde(default)]
    pub overdue: bool,
    /// Omitted or empty keeps every priority.
    #[serde(default)]
    pub priorities: Vec<dto::PriorityFilter>,
    /// Tasks with any of these labels, at most 100; empty keeps all.
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub waiting: Option<dto::WaitingFilter>,
    /// `true` keeps only recurring tasks, `false` only the others.
    #[serde(default)]
    pub recurring: Option<bool>,
    /// `true` keeps tasks with a due time, `false` those without.
    #[serde(default)]
    pub has_due: Option<bool>,
    /// Unix epoch milliseconds; tasks due at or after this time.
    #[serde(default)]
    pub due_from: Option<u64>,
    /// Unix epoch milliseconds; tasks due before this time.
    #[serde(default)]
    pub due_before: Option<u64>,
    #[serde(default)]
    pub sort: dto::TaskSort,
    #[serde(default)]
    pub offset: u32,
    /// 1 to 100.
    #[serde(default = "default_limit")]
    pub limit: u32,
}

pub struct SearchTasks;
impl Operation for SearchTasks {
    const NAME: &'static str = "task.search";
    const DESCRIPTION: &'static str = "Search tasks across boards by text, completion, priority, labels, waiting, recurrence and due time.";
    const ACCESS: Access = Access::Read;
    type Input = SearchTasksInput;
    type Output = dto::TaskPage;
    fn board(_: &SearchTasksInput) -> Option<i64> {
        None
    }
    fn run(
        host: &dyn OperationHost,
        context: &OperationContext,
        input: SearchTasksInput,
    ) -> Result<dto::TaskPage, DomainError> {
        let due_time = |time: Option<u64>| {
            time.map(i64::try_from)
                .transpose()
                .map_err(|_| DomainError::InvalidArgument)
        };
        if !(1..=100).contains(&input.limit)
            || input.query.len() > 4096
            || input.completion.is_empty()
            || input.labels.len() > MAX_LABEL_FILTERS
        {
            return Err(DomainError::InvalidArgument);
        }
        let database =
            Database::open_search_reader(database_path(host)?).map_err(DomainError::repository)?;
        let existing: Vec<i64> = database
            .boards()
            .map_err(DomainError::repository)?
            .into_iter()
            .map(|board| board.id)
            .collect();
        let board_ids = match input.board_ids {
            None => existing
                .into_iter()
                .filter(|&id| context.grants.allows(id, Access::Read))
                .collect(),
            Some(mut ids) => {
                for &id in &ids {
                    if !context.grants.allows(id, Access::Read) {
                        return Err(DomainError::PermissionDenied);
                    }
                    if !existing.contains(&id) {
                        return Err(DomainError::BoardNotFound);
                    }
                }
                ids.sort_unstable();
                ids.dedup();
                ids
            }
        };
        let list = |values: Vec<&'static str>| {
            (!values.is_empty()).then(|| values.into_iter().map(String::from).collect())
        };
        let filter = TaskExplorerQuery {
            query: input.query,
            board_ids: Some(board_ids),
            status: if input.overdue { "overdue" } else { "all" }.into(),
            sort: input.sort.as_filter().into(),
            due: match input.has_due {
                None => "all",
                Some(true) => "due",
                Some(false) => "none",
            }
            .into(),
            due_start: due_time(input.due_from)?,
            due_end: due_time(input.due_before)?,
            now: crate::plugins::types::now(),
            priorities: list(input.priorities.iter().map(|p| p.as_filter()).collect()),
            completion: list(input.completion.iter().map(|c| c.as_filter()).collect()),
            labels: (!input.labels.is_empty()).then_some(input.labels),
            waiting: input.waiting.map(|waiting| waiting.as_filter().into()),
            recurring: input.recurring,
            ..TaskExplorerQuery::default()
        };
        let limit = input.limit as usize;
        // One extra row tells whether another page follows.
        let rows = database
            .list_all_task_page(&filter, input.offset.into(), limit + 1)
            .map_err(DomainError::repository)?;
        let has_more = rows.len() > limit;
        let mut tasks = Vec::with_capacity(limit);
        for row in rows.into_iter().take(limit) {
            let archived = row.archived_at.is_some();
            let task = if archived {
                database.load_archive_task(row.board_id, row.task.id)
            } else {
                database.load_task(row.board_id, row.task.id)
            }
            .map_err(DomainError::repository)?;
            // The search reads its own snapshot; a task changed since is
            // skipped rather than failing the page.
            if let Some(task) = task {
                tasks.push(dto::Task::new(row.board_id, row.column_id, archived, task)?);
            }
        }
        Ok(dto::TaskPage {
            tasks,
            offset: input.offset,
            limit: input.limit,
            next_offset: has_more
                .then(|| input.offset.checked_add(input.limit))
                .flatten(),
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
    #[serde(default)]
    pub priority: Option<dto::TaskPriority>,
    /// Active tasks of the same board to wait on, at most 50.
    #[serde(default)]
    pub blocked_by: Vec<i64>,
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
        match host.creations()?.once(
            &context.caller,
            Self::NAME,
            &key,
            board_id,
            fingerprint,
            || create_task(host, input),
        )? {
            Recorded::New(result) => Ok(result),
            // The task may have been archived since.
            Recorded::Existing { board_id, id } => {
                let reader = reader(host, board_id)?;
                let task = match reader.get_task(id, false) {
                    Err(DomainError::TaskNotFound) => reader.get_task(id, true)?,
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
        priority: input.priority.map(models::TaskPriority::from),
        blocked_by: input.blocked_by,
        ..models::Task::default()
    };
    commit_then(
        host,
        input.board_id,
        |data| tasks::create_task(data, input.column_id, task, now()),
        |data, id| {
            Ok((
                dto::TaskResult {
                    task: active_task(data, input.board_id, id)?,
                },
                id,
            ))
        },
    )
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
    #[serde(default, deserialize_with = "present")]
    pub priority: Option<Option<dto::TaskPriority>>,
    /// Replaces the tasks this one waits on: active tasks of the same board,
    /// at most 50, never one that already waits on this task.
    #[serde(default)]
    pub blocked_by: Option<Vec<i64>>,
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
            if let Some(priority) = input.priority {
                task.priority = priority.map(models::TaskPriority::from);
            }
            if let Some(blocked_by) = input.blocked_by {
                task.blocked_by = blocked_by;
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
    /// Destination column. A column with sort rules displays tasks sorted;
    /// the position still sets the manual order shown without them.
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
        commit_then(
            host,
            input.board_id,
            |data| tasks::move_task(data, input.task_id, input.column_id, input.before_task_id),
            |data, ()| active_task(data, input.board_id, input.task_id),
        )
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
        commit_then(
            host,
            input.board_id,
            |data| unarchive_task_in(data, input.column_id, input.task_id),
            |data, _| active_task(data, input.board_id, input.task_id),
        )
        .map(|task| dto::TaskResult { task })
    }
}

pub struct CompleteTask;
impl Operation for CompleteTask {
    const NAME: &'static str = "task.complete";
    const DESCRIPTION: &'static str = "Complete an active task by moving it to the end of the board's done column, or by archiving it when the board has none. A recurring task continues with its next occurrence.";
    const ACCESS: Access = Access::Write;
    type Input = TaskInput;
    type Output = dto::TaskResult;
    fn board(input: &TaskInput) -> Option<i64> {
        Some(input.board_id)
    }
    fn run(
        host: &dyn OperationHost,
        _: &OperationContext,
        input: TaskInput,
    ) -> Result<dto::TaskResult, DomainError> {
        commit_then(
            host,
            input.board_id,
            |data| completion::complete_tasks(data, &[input.task_id], now()),
            |data, ()| {
                if mutation::task_position(data, input.task_id).is_ok() {
                    return active_task(data, input.board_id, input.task_id);
                }
                let archived = data
                    .archives
                    .iter()
                    .rev()
                    .find(|archive| archive.task.id == input.task_id)
                    .map(|archive| archive.task.clone())
                    .ok_or(DomainError::TaskNotFound)?;
                dto::Task::new(input.board_id, None, true, archived)
            },
        )
        .map(|task| dto::TaskResult { task })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReopenTaskInput {
    pub board_id: i64,
    pub task_id: i64,
    /// Column that receives the task at its end; not the done column.
    pub column_id: i64,
}

pub struct ReopenTask;
impl Operation for ReopenTask {
    const NAME: &'static str = "task.reopen";
    const DESCRIPTION: &'static str = "Reopen a completed task: move it out of the done column, or restore it from the archive, to the end of another column.";
    const ACCESS: Access = Access::Write;
    type Input = ReopenTaskInput;
    type Output = dto::TaskResult;
    fn board(input: &ReopenTaskInput) -> Option<i64> {
        Some(input.board_id)
    }
    fn run(
        host: &dyn OperationHost,
        _: &OperationContext,
        input: ReopenTaskInput,
    ) -> Result<dto::TaskResult, DomainError> {
        commit_then(
            host,
            input.board_id,
            |data| completion::reopen_tasks(data, &[input.task_id], input.column_id),
            |data, ()| active_task(data, input.board_id, input.task_id),
        )
        .map(|task| dto::TaskResult { task })
    }
}
