use crate::errors::{CommandError, DomainError};
use crate::{
    models::{StoredData, Task},
    state::{lock_app, SharedAppData},
    task_dates::due_limit_millis,
};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;
use tauri_plugin_notification::NotificationExt;

pub(crate) fn current_time_millis() -> Result<u128, DomainError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| DomainError::Internal(error.to_string()))
}

/// Whether `task` became overdue in `[since, now)`, the period not yet checked.
fn became_overdue(task: &Task, since: u128, now: u128) -> bool {
    task.due_time.is_some_and(|due_time| {
        (since..now).contains(&due_limit_millis(due_time, task.due_at_midnight))
    }) && task.id >= 0
        && !task.title.is_empty()
}

/// Board and task names to notify about, skipping columns muted on this device.
fn overdue_task_titles(
    boards: impl IntoIterator<Item = (i64, String, StoredData)>,
    muted_columns: &BTreeMap<i64, BTreeSet<i64>>,
    since: u128,
    now: u128,
) -> Vec<(String, String)> {
    let mut titles = Vec::new();
    for (board_id, board_name, data) in boards {
        let muted = muted_columns.get(&board_id);
        for column in data.columns {
            if muted.is_some_and(|muted| muted.contains(&column.id)) {
                continue;
            }
            titles.extend(
                column
                    .tasks
                    .into_iter()
                    .filter(|task| became_overdue(task, since, now))
                    .map(|task| (board_name.clone(), task.title)),
            );
        }
    }
    titles
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
pub fn get_task_detail(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
    task_id: i64,
) -> Result<Task, CommandError> {
    let guard = lock_app(&state)?;
    if guard.active_board_id != expected_board_id {
        return Err(CommandError::StaleBoardRequest);
    }
    guard
        .database
        .load_task(expected_board_id, task_id)
        .map_err(CommandError::repository)
        .and_then(|task| task.ok_or(CommandError::TaskNotFound))
}

#[tauri::command]
pub fn check_expired_tasks(
    app: tauri::AppHandle,
    state: State<'_, SharedAppData>,
    title_template: String,
    body_template: String,
) -> Result<(), CommandError> {
    let now = current_time_millis()?;
    let mut guard = lock_app(&state)?;
    if !guard.stored.settings.notify_enabled {
        return Ok(());
    }
    // The checked time persists, so restarting does not repeat notifications
    // and tasks that became overdue while the app was closed notify once.
    let since = guard
        .database
        .notifications_checked_until()
        .map_err(CommandError::repository)?;
    let titles = match since {
        Some(since) => {
            let boards = guard
                .boards
                .iter()
                .filter_map(|board| match guard.database.read_board(board.id) {
                    Ok(data) => Some((board.id, board.name.clone(), data)),
                    Err(error) => {
                        log::warn!(target: "notifications", "Could not read board {} while checking expired tasks: {error}", board.id);
                        None
                    }
                })
                .collect::<Vec<_>>();
            overdue_task_titles(
                boards,
                &guard.stored.settings.notify_muted_columns,
                since,
                now,
            )
        }
        // The first check only records where later checks start.
        None => Vec::new(),
    };
    if since.is_none_or(|since| since < now) {
        guard
            .database
            .set_notifications_checked_until(now)
            .map_err(CommandError::repository)?;
    }
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
    use super::{became_overdue, expired_notification, overdue_task_titles};
    use crate::models::{Column, StoredData, Task};
    use std::collections::{BTreeMap, BTreeSet};

    fn task_with_due_time(due_time: u128) -> Task {
        Task {
            id: 7,
            title: "Follow up".into(),
            due_time: Some(due_time),
            ..Task::default()
        }
    }

    fn board_with_columns(columns: Vec<(i64, Vec<Task>)>) -> StoredData {
        StoredData {
            columns: columns
                .into_iter()
                .map(|(id, tasks)| Column {
                    id,
                    name: String::new(),
                    color: String::new(),
                    sort_order: Default::default(),
                    sort_rules: Vec::new(),
                    tasks,
                })
                .collect(),
            ..StoredData::default()
        }
    }

    #[test]
    fn tasks_notify_only_when_they_become_overdue_in_the_unchecked_period() {
        let task = task_with_due_time(200);

        assert!(!became_overdue(&task, 100, 200));
        assert!(became_overdue(&task, 100, 201));
        assert!(became_overdue(&task, 200, 300));
        // Already checked before an earlier restart.
        assert!(!became_overdue(&task, 201, 300));
    }

    #[test]
    fn rescheduled_tasks_notify_again_when_they_become_overdue() {
        let rescheduled = task_with_due_time(400);

        assert!(!became_overdue(&rescheduled, 300, 350));
        assert!(became_overdue(&rescheduled, 350, 450));
    }

    #[test]
    fn only_valid_tasks_with_due_times_notify() {
        let mut task = task_with_due_time(100);
        task.due_time = None;
        assert!(!became_overdue(&task, 0, 300));

        task.due_time = Some(100);
        task.title.clear();
        assert!(!became_overdue(&task, 0, 300));

        let unsaved = Task {
            id: -1,
            ..task_with_due_time(100)
        };
        assert!(!became_overdue(&unsaved, 0, 300));
    }

    #[test]
    fn date_only_due_dates_notify_after_the_day_ends() {
        use chrono::{Local, TimeZone};
        let midnight = |day| {
            u128::try_from(
                Local
                    .with_ymd_and_hms(2026, 10, day, 0, 0, 0)
                    .earliest()
                    .unwrap()
                    .timestamp_millis(),
            )
            .unwrap()
        };
        let task = task_with_due_time(midnight(14));
        let since = midnight(14) - 1;

        assert!(!became_overdue(&task, since, midnight(14) + 3_600_000));
        assert!(became_overdue(&task, since, midnight(15) + 1));

        let chosen = Task {
            due_at_midnight: true,
            ..task
        };
        assert!(became_overdue(&chosen, since, midnight(14) + 3_600_000));
    }

    #[test]
    fn muted_columns_do_not_notify_on_their_board_only() {
        let task = task_with_due_time(100);
        let other = Task {
            id: 8,
            title: "Review".into(),
            ..task.clone()
        };
        let boards = vec![
            (
                1,
                "Work".to_owned(),
                board_with_columns(vec![(10, vec![task.clone()]), (11, vec![other])]),
            ),
            (
                2,
                "Home".to_owned(),
                board_with_columns(vec![(10, vec![task])]),
            ),
        ];
        let muted = BTreeMap::from([(1, BTreeSet::from([10]))]);

        assert_eq!(
            overdue_task_titles(boards, &muted, 0, 200),
            vec![
                ("Work".to_owned(), "Review".to_owned()),
                ("Home".to_owned(), "Follow up".to_owned()),
            ]
        );
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
