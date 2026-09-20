use crate::{models::Task, state::SharedAppData};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;
use tauri_plugin_notification::NotificationExt;

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

fn expired_notification(board_name: &str, task_title: &str) -> (String, String) {
    (
        format!("Task Expired — {board_name}"),
        format!("Task \"{task_title}\" has passed its due date"),
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
pub fn check_expired_tasks(
    app: tauri::AppHandle,
    state: State<'_, SharedAppData>,
) -> Result<(), String> {
    let now = current_time_millis()?;
    let mut guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;

    let expired = guard
        .boards
        .iter()
        .filter_map(|board| {
            guard
                .database
                .read_board(board.id)
                .ok()
                .map(|data| (board.id, board.name.clone(), data))
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
        let (title, body) = expired_notification(&board_name, &task_title);
        let _ = app.notification().builder().title(title).body(body).show();
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
            expired_notification("Work", "Follow up"),
            (
                "Task Expired — Work".into(),
                "Task \"Follow up\" has passed its due date".into()
            )
        );
    }
}
