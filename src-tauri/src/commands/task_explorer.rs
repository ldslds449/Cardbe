use crate::errors::{CommandError, DomainError};
use crate::{
    models::TaskSummary,
    state::{lock_app, SharedAppData},
    storage::TaskExplorerQuery,
};
use tauri::Manager;

const TASK_EXPLORER_PAGE_SIZE: usize = 50;
const MAX_LABEL_FILTERS: usize = 100;

#[derive(serde::Serialize)]
pub struct AllTaskListItem {
    pub board_id: i64,
    pub board_name: String,
    pub column_id: Option<i64>,
    pub column_name: Option<String>,
    pub archived_at: Option<u128>,
    pub done: bool,
    pub waiting_count: i64,
    pub task: TaskSummary,
}

#[derive(serde::Serialize)]
pub struct AllTaskPage {
    pub items: Vec<AllTaskListItem>,
    pub next_cursor: Option<String>,
}

#[tauri::command]
pub async fn list_all_tasks(
    app: tauri::AppHandle,
    cursor: Option<String>,
    filter: TaskExplorerQuery,
    limit: Option<usize>,
) -> Result<AllTaskPage, CommandError> {
    let offset = cursor
        .map(|value| value.parse::<i64>())
        .transpose()
        .map_err(|_| DomainError::InvalidArgument)?
        .unwrap_or(0);
    if !(0..=i64::MAX - 100).contains(&offset) {
        return Err(CommandError::InvalidArgument);
    }
    if !["all", "active", "overdue", "recurring", "archived"].contains(&filter.status.as_str())
        || !["due", "title", "column", "archived", "priority", "created"]
            .contains(&filter.sort.as_str())
        || !["all", "due", "none"].contains(&filter.due.as_str())
        || filter.priorities.as_ref().is_some_and(|priorities| {
            priorities.len() > 5
                || priorities.iter().any(|priority| {
                    !["none", "low", "medium", "high", "urgent"].contains(&priority.as_str())
                })
        })
        || filter.completion.as_ref().is_some_and(|states| {
            states.len() > 3
                || states
                    .iter()
                    .any(|state| !["open", "done", "archived"].contains(&state.as_str()))
        })
        || filter
            .labels
            .as_ref()
            .is_some_and(|labels| labels.len() > MAX_LABEL_FILTERS)
        || filter.waiting.as_ref().is_some_and(|waiting| {
            !["waiting", "not_waiting", "waited_on"].contains(&waiting.as_str())
        })
    {
        return Err(CommandError::InvalidArgument);
    }
    if filter
        .search_options
        .is_some_and(|options| !options.valid())
    {
        return Err(CommandError::InvalidArgument);
    }
    let limit = limit.unwrap_or(TASK_EXPLORER_PAGE_SIZE).clamp(1, 100);
    let path = {
        let state = app.state::<SharedAppData>();
        let guard = lock_app(&state)?;
        guard.database.path().to_owned()
    };
    let rows = tauri::async_runtime::spawn_blocking(move || {
        crate::storage::Database::open_search_reader(path)
            .and_then(|reader| reader.list_all_task_page(&filter, offset, limit + 1))
            .map_err(CommandError::repository)
    })
    .await
    .map_err(CommandError::internal)??;
    let has_more = rows.len() > limit;
    let items = rows
        .into_iter()
        .take(limit)
        .map(|row| AllTaskListItem {
            board_id: row.board_id,
            board_name: row.board_name,
            column_id: row.column_id,
            column_name: row.column_name,
            archived_at: row.archived_at,
            done: row.done,
            waiting_count: row.waiting_count,
            task: row.task,
        })
        .collect::<Vec<_>>();
    let next_cursor = has_more.then(|| (offset + limit as i64).to_string());
    Ok(AllTaskPage { items, next_cursor })
}
