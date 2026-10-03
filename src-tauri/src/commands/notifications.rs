use crate::{
    models::{Task, TaskSummary},
    state::SharedAppData,
};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;
use tauri_plugin_notification::NotificationExt;

const EXPIRED_PAGE_SIZE: usize = 50;

#[derive(serde::Serialize)]
pub struct ExpiredTaskPage {
    pub items: Vec<TaskSummary>,
    pub next_cursor: Option<String>,
}

fn parse_cursor(cursor: Option<String>) -> Result<Option<(u128, i64)>, String> {
    cursor
        .map(|value| {
            let (time, task_id) = value
                .split_once(':')
                .ok_or_else(|| "Invalid expired-task cursor".to_string())?;
            Ok((
                time.parse()
                    .map_err(|_| "Invalid expired-task cursor".to_string())?,
                task_id
                    .parse()
                    .map_err(|_| "Invalid expired-task cursor".to_string())?,
            ))
        })
        .transpose()
}

fn matches_query(task: &Task, query: &str) -> bool {
    query.is_empty()
        || task.title.to_lowercase().contains(query)
        || task.description.to_lowercase().contains(query)
        || task
            .labels
            .iter()
            .any(|label| label.to_lowercase().contains(query))
        || task
            .items
            .iter()
            .any(|item| item.text.to_lowercase().contains(query))
}

fn current_time_millis() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| error.to_string())
}

fn notification_key(board_id: i64, task: &Task, now: u128) -> Option<(i64, i64, u128)> {
    let due_time = task.due_time?;
    (due_time < now && task.id >= 0 && !task.title.is_empty())
        .then_some((board_id, task.id, due_time))
}

fn expired_notification(
    board_name: &str,
    task_title: &str,
    title_template: &str,
    body_template: &str,
) -> (String, String) {
    (
        title_template.replace("{board}", board_name),
        body_template.replace("{task}", task_title),
    )
}

#[tauri::command]
pub fn get_expired_tasks(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
) -> Result<Vec<Task>, String> {
    let now = current_time_millis()?;
    let guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    if guard.active_board_id != expected_board_id {
        return Err("Stale board request".into());
    }
    Ok(guard
        .stored
        .columns
        .iter()
        .flat_map(|column| column.tasks.iter())
        .filter(|task| notification_key(expected_board_id, task, now).is_some())
        .cloned()
        .collect())
}

#[tauri::command]
pub fn list_expired_tasks(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
    cursor: Option<String>,
    query: Option<String>,
    limit: Option<usize>,
) -> Result<ExpiredTaskPage, String> {
    let cursor = parse_cursor(cursor)?;
    let limit = limit.unwrap_or(EXPIRED_PAGE_SIZE).clamp(1, 100);
    let now = current_time_millis()?;
    let guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    if guard.active_board_id != expected_board_id {
        return Err("Stale board request".into());
    }
    // ponytail: expired-task filtering scans persisted JSON; normalize due/title fields if this becomes a hotspot.
    let query = query.unwrap_or_default().trim().to_lowercase();
    let mut tasks = guard
        .database
        .load_board_tasks(expected_board_id)
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|task| {
            task.due_time.is_some_and(|due_time| due_time < now) && matches_query(task, &query)
        })
        .collect::<Vec<_>>();
    tasks.sort_by(|left, right| {
        right
            .due_time
            .cmp(&left.due_time)
            .then_with(|| right.id.cmp(&left.id))
    });
    if let Some((due_time, task_id)) = cursor {
        tasks.retain(|task| {
            task.due_time.is_some_and(|task_due| {
                task_due < due_time || (task_due == due_time && task.id < task_id)
            })
        });
    }
    let has_more = tasks.len() > limit;
    let items = tasks
        .into_iter()
        .take(limit)
        .map(|task| TaskSummary::from(&task))
        .collect::<Vec<_>>();
    let next_cursor = has_more.then(|| {
        let last = items.last().expect("a non-empty page has a last item");
        format!(
            "{}:{}",
            last.due_time.expect("expired tasks have due times"),
            last.id
        )
    });
    Ok(ExpiredTaskPage { items, next_cursor })
}

#[tauri::command]
pub fn get_task_detail(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
    task_id: i64,
) -> Result<Task, String> {
    let guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    if guard.active_board_id != expected_board_id {
        return Err("Stale board request".into());
    }
    guard
        .database
        .load_task(expected_board_id, task_id)
        .map_err(|error| error.to_string())
        .and_then(|task| task.ok_or_else(|| format!("Task not found: {task_id}")))
}

#[tauri::command]
pub fn check_expired_tasks(
    app: tauri::AppHandle,
    state: State<'_, SharedAppData>,
    title_template: String,
    body_template: String,
) -> Result<(), String> {
    let now = current_time_millis()?;
    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;

    let expired = guard
        .boards
        .iter()
        .filter_map(|board| match guard.database.read_board(board.id) {
            Ok(data) => Some((board.id, board.name.clone(), data)),
            Err(error) => {
                log::warn!(target: "notifications", "Could not read board {} while checking expired tasks: {error}", board.id);
                None
            }
        })
        .flat_map(|(board_id, board_name, data)| {
            data.columns.into_iter().flat_map(move |column| {
                let board_name = board_name.clone();
                column.tasks.into_iter().filter_map(move |task| {
                    notification_key(board_id, &task, now)
                        .map(|key| (key, board_name.clone(), task.title))
                })
            })
        })
        .collect::<Vec<_>>();

    let titles = expired
        .into_iter()
        .filter_map(|(key, board_name, title)| {
            guard
                .notified_tasks
                .insert(key)
                .then_some((board_name, title))
        })
        .collect::<Vec<_>>();
    drop(guard);

    for (board_name, task_title) in titles {
        let (title, body) =
            expired_notification(&board_name, &task_title, &title_template, &body_template);
        if let Err(error) = app.notification().builder().title(title).body(body).show() {
            log::warn!(target: "notifications", "Could not show an expired-task notification: {error}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{expired_notification, notification_key};
    use crate::models::Task;

    fn task_with_due_time(due_time: u128) -> Task {
        Task {
            id: 7,
            title: "Follow up".into(),
            due_time: Some(due_time),
            ..Task::default()
        }
    }

    #[test]
    fn notification_key_changes_when_task_is_rescheduled() {
        let first = task_with_due_time(100);
        let rescheduled = task_with_due_time(200);

        assert_eq!(notification_key(1, &first, 300), Some((1, 7, 100)));
        assert_eq!(notification_key(1, &rescheduled, 300), Some((1, 7, 200)));
    }

    #[test]
    fn notification_key_keeps_same_task_id_on_different_boards_distinct() {
        let task = task_with_due_time(100);
        assert_ne!(
            notification_key(1, &task, 300),
            notification_key(2, &task, 300)
        );
    }

    #[test]
    fn notification_key_only_exists_for_expired_valid_tasks() {
        let mut task = task_with_due_time(500);
        assert_eq!(notification_key(1, &task, 300), None);

        task.due_time = None;
        assert_eq!(notification_key(1, &task, 300), None);

        task.due_time = Some(100);
        task.title.clear();
        assert_eq!(notification_key(1, &task, 300), None);
    }

    #[test]
    fn notification_identifies_the_board() {
        assert_eq!(
            expired_notification(
                "Work",
                "Follow up",
                "Task Expired — {board}",
                "Task \"{task}\" has passed its due date"
            ),
            (
                "Task Expired — Work".into(),
                "Task \"Follow up\" has passed its due date".into()
            )
        );
        assert_eq!(
            expired_notification(
                "工作 {board}",
                "跟進 {task}",
                "任務已逾期 — {board}",
                "任務「{task}」已超過截止時間"
            ),
            (
                "任務已逾期 — 工作 {board}".into(),
                "任務「跟進 {task}」已超過截止時間".into()
            )
        );
    }
}
