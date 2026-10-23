//! Caller-facing operation types, independent of storage models and WIT.
use super::tasks::{external_times, optional_time};
use crate::{errors::DomainError, models, plugins::board as board_view};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BoardRole {
    Owner,
    Editor,
    Viewer,
}

impl From<models::BoardRole> for BoardRole {
    fn from(role: models::BoardRole) -> Self {
        match role {
            models::BoardRole::Owner => Self::Owner,
            models::BoardRole::Editor => Self::Editor,
            models::BoardRole::Viewer => Self::Viewer,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Board {
    pub id: i64,
    pub name: String,
    pub task_count: i64,
    /// This device's role; shared boards may be read-only.
    pub role: BoardRole,
    pub is_shared: bool,
    /// Whether this caller may change the board.
    pub writable: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BoardList {
    pub boards: Vec<Board>,
    /// Boards on this device that the client was not granted.
    pub hidden_board_count: usize,
}

#[derive(Clone, Copy, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ColumnSort {
    /// Tasks keep their manual order and accept moves to a position.
    Custom,
    DueDateAsc,
    DueDateDesc,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Column {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub sort_order: ColumnSort,
    /// Whether this is the board's done column; tasks in it are completed.
    /// A board has at most one.
    pub done: bool,
}

impl From<&models::ColumnSort> for ColumnSort {
    fn from(sort: &models::ColumnSort) -> Self {
        match sort {
            models::ColumnSort::Custom => Self::Custom,
            models::ColumnSort::DueDateAsc => Self::DueDateAsc,
            models::ColumnSort::DueDateDesc => Self::DueDateDesc,
        }
    }
}

impl Column {
    /// `done_column` is the board's done column, which decides `done` even
    /// when synchronized data flags several columns.
    pub fn from_info(column: board_view::ColumnInfo, done_column: Option<i64>) -> Self {
        Self {
            sort_order: (&column.sort_order).into(),
            done: Some(column.id) == done_column,
            id: column.id,
            name: column.name,
            color: column.color,
        }
    }

    pub fn new(column: &models::Column, done_column: Option<i64>) -> Self {
        Self {
            id: column.id,
            name: column.name.clone(),
            color: column.color.clone(),
            sort_order: (&column.sort_order).into(),
            done: Some(column.id) == done_column,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ColumnResult {
    pub column: Column,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ColumnList {
    pub columns: Vec<Column>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecurrenceFrequency {
    Daily,
    Weekly,
    Monthly,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Recurrence {
    pub frequency: RecurrenceFrequency,
    /// Repeat every `interval` periods, 1 to 10000.
    pub interval: u32,
}

impl From<Recurrence> for models::Recurrence {
    fn from(recurrence: Recurrence) -> Self {
        Self {
            frequency: match recurrence.frequency {
                RecurrenceFrequency::Daily => models::RecurrenceFrequency::Daily,
                RecurrenceFrequency::Weekly => models::RecurrenceFrequency::Weekly,
                RecurrenceFrequency::Monthly => models::RecurrenceFrequency::Monthly,
            },
            interval: recurrence.interval,
        }
    }
}

impl From<models::Recurrence> for Recurrence {
    fn from(recurrence: models::Recurrence) -> Self {
        Self {
            frequency: match recurrence.frequency {
                models::RecurrenceFrequency::Daily => RecurrenceFrequency::Daily,
                models::RecurrenceFrequency::Weekly => RecurrenceFrequency::Weekly,
                models::RecurrenceFrequency::Monthly => RecurrenceFrequency::Monthly,
            },
            interval: recurrence.interval,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    Low,
    Medium,
    High,
    Urgent,
}

impl From<TaskPriority> for models::TaskPriority {
    fn from(priority: TaskPriority) -> Self {
        match priority {
            TaskPriority::Low => Self::Low,
            TaskPriority::Medium => Self::Medium,
            TaskPriority::High => Self::High,
            TaskPriority::Urgent => Self::Urgent,
        }
    }
}

impl From<models::TaskPriority> for TaskPriority {
    fn from(priority: models::TaskPriority) -> Self {
        match priority {
            models::TaskPriority::Low => Self::Low,
            models::TaskPriority::Medium => Self::Medium,
            models::TaskPriority::High => Self::High,
            models::TaskPriority::Urgent => Self::Urgent,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct TaskItem {
    pub id: String,
    pub text: String,
    pub completed: bool,
}

/// Checklist item to save; omit `id` for a new item.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskItemInput {
    #[serde(default)]
    pub id: Option<String>,
    pub text: String,
    #[serde(default)]
    pub completed: bool,
}

pub fn items(input: Vec<TaskItemInput>) -> Vec<models::TaskItem> {
    input
        .into_iter()
        .map(|item| models::TaskItem {
            id: item.id.unwrap_or_else(random_item_id),
            text: item.text,
            completed: item.completed,
        })
        .collect()
}

/// Matches the frontend's `crypto.randomUUID()` item identifiers.
fn random_item_id() -> String {
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&iroh::SecretKey::generate().to_bytes()[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

/// Times are Unix epoch milliseconds. A start or due time at local midnight is
/// a whole day unless its `*_at_midnight` flag marks a chosen 00:00.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Task {
    pub board_id: i64,
    /// Present for active tasks.
    pub column_id: Option<i64>,
    pub archived: bool,
    pub id: i64,
    pub title: String,
    /// Markdown.
    pub description: String,
    pub pinned: bool,
    pub color: String,
    pub start_time: Option<u64>,
    pub due_time: Option<u64>,
    /// Read-only; `null` when unknown.
    pub created_time: Option<u64>,
    pub start_at_midnight: bool,
    pub due_at_midnight: bool,
    pub labels: Vec<String>,
    pub items: Vec<TaskItem>,
    pub recurrence: Option<Recurrence>,
    pub priority: Option<TaskPriority>,
    /// Read-only; when the task entered the board's done column, or `null`
    /// while it is open. Move a task to the done column to complete it.
    pub completed_time: Option<u64>,
    /// Tasks of the same board this one waits on. Only those still open
    /// block it; done, archived and removed tasks no longer do.
    pub blocked_by: Vec<i64>,
}

impl Task {
    pub fn new(
        board_id: i64,
        column_id: Option<i64>,
        archived: bool,
        task: models::Task,
    ) -> Result<Self, DomainError> {
        let (start_time, created_time) = external_times(&task);
        Ok(Self {
            board_id,
            column_id,
            archived,
            id: task.id,
            title: task.title,
            description: task.description,
            pinned: task.pinned,
            color: task.color,
            start_time: optional_time(start_time)?,
            due_time: task
                .due_time
                .map(u64::try_from)
                .transpose()
                .map_err(|_| DomainError::InvalidArgument)?,
            created_time: optional_time(created_time)?,
            start_at_midnight: task.start_at_midnight,
            due_at_midnight: task.due_at_midnight,
            labels: task.labels,
            items: task
                .items
                .into_iter()
                .map(|item| TaskItem {
                    id: item.id,
                    text: item.text,
                    completed: item.completed,
                })
                .collect(),
            recurrence: task.recurrence.map(Recurrence::from),
            priority: task.priority.map(TaskPriority::from),
            completed_time: task
                .completed_at
                .map(u64::try_from)
                .transpose()
                .map_err(|_| DomainError::InvalidArgument)?,
            blocked_by: task.blocked_by,
        })
    }
}

impl TryFrom<board_view::TaskInfo> for Task {
    type Error = DomainError;
    fn try_from(info: board_view::TaskInfo) -> Result<Self, Self::Error> {
        Self::new(info.board_id, info.column_id, info.archived, info.task)
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct TaskResult {
    pub task: Task,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct TaskPage {
    pub tasks: Vec<Task>,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ArchiveResult {
    pub task: Task,
    /// The next occurrence created for a recurring task.
    pub next_task: Option<Task>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    #[default]
    Active,
    Archived,
    All,
    Overdue,
    Recurring,
}

impl TaskStatus {
    pub fn as_filter(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
            Self::All => "all",
            Self::Overdue => "overdue",
            Self::Recurring => "recurring",
        }
    }
}

/// Distinguishes an omitted field from an explicit `null` that clears it.
pub fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(deserializer).map(Some)
}
