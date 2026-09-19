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

fn notification_key(task: &Task, now: u128) -> Option<(i64, u128)> {
    let due_time = task.due_time?;
    (due_time < now && task.id >= 0 && !task.title.is_empty()).then_some((task.id, due_time))
}

#[tauri::command]
pub fn get_expired_tasks(state: State<'_, SharedAppData>) -> Result<Vec<Task>, String> {
    let now = current_time_millis()?;
    let guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    Ok(guard
        .stored
        .columns
        .iter()
        .flat_map(|column| column.tasks.iter())
        .filter(|task| notification_key(task, now).is_some())
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
        .stored
        .columns
        .iter()
        .flat_map(|column| column.tasks.iter())
        .filter_map(|task| notification_key(task, now).map(|key| (key, task.title.clone())))
        .collect::<Vec<_>>();

    let titles = expired
        .into_iter()
        .filter_map(|(key, title)| guard.notified_tasks.insert(key).then_some(title))
        .collect::<Vec<_>>();
    drop(guard);

    for title in titles {
        let _ = app
            .notification()
            .builder()
            .title("Task Expired")
            .body(format!("Task \"{title}\" has passed its due date"))
            .show();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::notification_key;
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

        assert_eq!(notification_key(&first, 300), Some((7, 100)));
        assert_eq!(notification_key(&rescheduled, 300), Some((7, 200)));
    }

    #[test]
    fn notification_key_only_exists_for_expired_valid_tasks() {
        let mut task = task_with_due_time(500);
        assert_eq!(notification_key(&task, 300), None);

        task.due_time = None;
        assert_eq!(notification_key(&task, 300), None);

        task.due_time = Some(100);
        task.title.clear();
        assert_eq!(notification_key(&task, 300), None);
    }
}
