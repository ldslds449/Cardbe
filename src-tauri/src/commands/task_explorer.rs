use crate::{models::TaskSummary, state::SharedAppData, storage::TaskExplorerQuery};
use tauri::State;

const TASK_EXPLORER_PAGE_SIZE: usize = 50;

#[derive(serde::Serialize)]
pub struct AllTaskListItem {
    pub board_id: i64,
    pub board_name: String,
    pub column_name: Option<String>,
    pub archived_at: Option<u128>,
    pub task: TaskSummary,
}

#[derive(serde::Serialize)]
pub struct AllTaskPage {
    pub items: Vec<AllTaskListItem>,
    pub next_cursor: Option<String>,
}

#[tauri::command]
pub fn list_all_tasks(
    state: State<'_, SharedAppData>,
    cursor: Option<String>,
    filter: TaskExplorerQuery,
    limit: Option<usize>,
) -> Result<AllTaskPage, String> {
    let offset = cursor
        .map(|value| value.parse::<i64>())
        .transpose()
        .map_err(|_| "Invalid task cursor".to_string())?
        .unwrap_or(0);
    if !(0..=i64::MAX - 100).contains(&offset) {
        return Err("Invalid task cursor".into());
    }
    if !["all", "active", "overdue", "recurring", "archived"].contains(&filter.status.as_str())
        || !["due", "title", "column", "archived"].contains(&filter.sort.as_str())
        || !["all", "due", "none"].contains(&filter.due.as_str())
    {
        return Err("Invalid task filters".into());
    }
    let limit = limit.unwrap_or(TASK_EXPLORER_PAGE_SIZE).clamp(1, 100);
    let guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    let rows = guard
        .database
        .list_all_task_page(&filter, offset, limit + 1)
        .map_err(|error| error.to_string())?;
    let has_more = rows.len() > limit;
    let items = rows
        .into_iter()
        .take(limit)
        .map(|row| AllTaskListItem {
            board_id: row.board_id,
            board_name: row.board_name,
            column_name: row.column_name,
            archived_at: row.archived_at,
            task: row.task,
        })
        .collect::<Vec<_>>();
    let next_cursor = has_more.then(|| (offset + limit as i64).to_string());
    Ok(AllTaskPage { items, next_cursor })
}
