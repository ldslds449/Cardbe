use crate::errors::{CommandError, DomainError};
use crate::{
    models::Task,
    state::{lock_app, SharedAppData},
    storage::DueTask,
    task_dates::due_limit_millis,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_notification::NotificationExt;

/// How often due notifications are checked while the app runs.
const CHECK_INTERVAL: Duration = Duration::from_secs(30);
/// Notifications shown individually per check; the rest share one summary.
const MAX_SHOWN: usize = 3;
/// A date-only due time lasts until its local day ends, at most a day plus a
/// daylight saving hour later.
const DATE_ONLY_SPAN: u128 = 25 * 60 * 60 * 1000;
/// Reminder offsets the user can choose; `0` turns reminders off.
pub const REMINDER_MINUTES: [u32; 6] = [0, 5, 15, 30, 60, 1440];

pub(crate) fn current_time_millis() -> Result<u128, DomainError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| DomainError::Internal(error.to_string()))
}

/// Notification text in the current language. The main window supplies it
/// because translations live in the frontend catalogs; titles contain
/// `{board}` and bodies `{task}`.
#[derive(serde::Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct NotificationTexts {
    pub overdue_title: String,
    pub overdue_body: String,
    pub reminder_title: String,
    pub reminder_body: String,
    pub more_title: String,
    pub more_body: String,
}

#[derive(Default)]
pub struct NotificationState {
    texts: Mutex<Option<NotificationTexts>>,
    wake: tokio::sync::Notify,
}

impl NotificationState {
    fn texts(&self) -> Option<NotificationTexts> {
        self.texts
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DueNotice {
    Reminder,
    Overdue,
}

/// What `task` needs in `[since, now)`, the period not yet checked, and when
/// that happened. Overdue replaces a reminder missed in the same period.
fn due_notice(task: &Task, since: u128, now: u128, reminder: u128) -> Option<(u128, DueNotice)> {
    if task.id < 0 || task.title.is_empty() {
        return None;
    }
    let limit = due_limit_millis(task.due_time?, task.due_at_midnight);
    let unchecked = since..now;
    if unchecked.contains(&limit) {
        return Some((limit, DueNotice::Overdue));
    }
    let remind_at = limit.saturating_sub(reminder);
    (reminder > 0 && limit >= now && unchecked.contains(&remind_at))
        .then_some((remind_at, DueNotice::Reminder))
}

/// Notices in the order they happened, skipping columns muted on this device.
fn due_notices(
    tasks: Vec<DueTask>,
    muted_columns: &BTreeMap<i64, BTreeSet<i64>>,
    since: u128,
    now: u128,
    reminder: u128,
) -> Vec<(DueNotice, String, String)> {
    let mut notices = tasks
        .into_iter()
        .filter(|due| {
            !muted_columns
                .get(&due.board_id)
                .is_some_and(|muted| muted.contains(&due.column_id))
        })
        .filter_map(|due| {
            due_notice(&due.task, since, now, reminder)
                .map(|(time, notice)| (time, notice, due.board_name, due.task.title))
        })
        .collect::<Vec<_>>();
    notices.sort_by_key(|(time, ..)| *time);
    notices
        .into_iter()
        .map(|(_, notice, board, task)| (notice, board, task))
        .collect()
}

fn notification_text(
    texts: &NotificationTexts,
    notice: DueNotice,
    board_name: &str,
    task_title: &str,
) -> (String, String) {
    let (title, body) = match notice {
        DueNotice::Overdue => (&texts.overdue_title, &texts.overdue_body),
        DueNotice::Reminder => (&texts.reminder_title, &texts.reminder_body),
    };
    (
        title.replace("{board}", board_name),
        body.replace("{task}", task_title),
    )
}

/// Notifications to show, with one summary in place of all but the first few.
fn notifications(
    texts: &NotificationTexts,
    notices: Vec<(DueNotice, String, String)>,
) -> Vec<(String, String)> {
    let more = notices.len() > MAX_SHOWN;
    let mut shown = notices
        .into_iter()
        .take(if more { MAX_SHOWN - 1 } else { MAX_SHOWN })
        .map(|(notice, board, task)| notification_text(texts, notice, &board, &task))
        .collect::<Vec<_>>();
    if more {
        shown.push((texts.more_title.clone(), texts.more_body.clone()));
    }
    shown
}

fn check_due_tasks(app: &AppHandle) -> Result<(), CommandError> {
    // Wait for the main window's text, so nothing is skipped before it loads.
    let Some(texts) = app.state::<NotificationState>().texts() else {
        return Ok(());
    };
    let Some(state) = app.try_state::<SharedAppData>() else {
        return Ok(());
    };
    let now = current_time_millis()?;
    let mut guard = lock_app(&state)?;
    let settings = &guard.stored.settings;
    if !settings.notify_enabled {
        return Ok(());
    }
    let reminder = u128::from(settings.notify_reminder_minutes) * 60_000;
    // The checked time persists, so restarting does not repeat notifications
    // and tasks that became due while the app was closed notify once.
    let since = guard
        .database
        .notifications_checked_until()
        .map_err(CommandError::repository)?;
    let notices = match since {
        Some(since) if since < now => {
            let tasks = guard
                .database
                .tasks_due_between(since.saturating_sub(DATE_ONLY_SPAN), now + reminder)
                .map_err(CommandError::repository)?;
            due_notices(
                tasks,
                &guard.stored.settings.notify_muted_columns,
                since,
                now,
                reminder,
            )
        }
        // The first check only records where later checks start.
        _ => Vec::new(),
    };
    if since.is_none_or(|since| since < now) {
        guard
            .database
            .set_notifications_checked_until(now)
            .map_err(CommandError::repository)?;
    }
    drop(guard);

    for (title, body) in notifications(&texts, notices) {
        if let Err(error) = app.notification().builder().title(title).body(body).show() {
            log::warn!(target: "notifications", "Could not show a due-task notification: {error}");
        }
    }
    Ok(())
}

/// Checks due tasks in the background, independently of window timers that
/// may be throttled while the window is hidden.
pub fn start_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let handle = app.clone();
            match tauri::async_runtime::spawn_blocking(move || check_due_tasks(&handle)).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    log::warn!(target: "notifications", "Could not check due tasks: {error:?}");
                }
                Err(error) => {
                    log::warn!(target: "notifications", "Due-task check stopped: {error}");
                }
            }
            let notifications = app.state::<NotificationState>();
            tokio::select! {
                () = tokio::time::sleep(CHECK_INTERVAL) => {}
                () = notifications.wake.notified() => {}
            }
        }
    });
}

#[tauri::command]
pub fn set_notification_texts(
    notifications: State<'_, NotificationState>,
    texts: NotificationTexts,
) -> Result<(), CommandError> {
    let mut current = notifications
        .texts
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let first = current.is_none();
    *current = Some(texts);
    drop(current);
    if first {
        // Check right away instead of waiting for the next interval.
        notifications.wake.notify_one();
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::{due_notice, due_notices, notification_text, notifications, DueNotice};
    use super::{NotificationTexts, MAX_SHOWN};
    use crate::models::Task;
    use crate::storage::DueTask;
    use std::collections::{BTreeMap, BTreeSet};

    const HOUR: u128 = 3_600_000;

    fn task_with_due_time(due_time: u128) -> Task {
        Task {
            id: 7,
            title: "Follow up".into(),
            due_time: Some(due_time),
            ..Task::default()
        }
    }

    fn due(board_id: i64, column_id: i64, task: Task) -> DueTask {
        DueTask {
            board_id,
            board_name: format!("Board {board_id}"),
            column_id,
            task,
        }
    }

    fn texts() -> NotificationTexts {
        NotificationTexts {
            overdue_title: "Task Expired — {board}".into(),
            overdue_body: "Task \"{task}\" has passed its due date".into(),
            reminder_title: "Task Due Soon — {board}".into(),
            reminder_body: "Task \"{task}\" is due soon".into(),
            more_title: "More tasks".into(),
            more_body: "Open Cardbe".into(),
        }
    }

    #[test]
    fn tasks_notify_only_when_they_become_overdue_in_the_unchecked_period() {
        let task = task_with_due_time(200);
        let overdue = Some((200, DueNotice::Overdue));

        assert_eq!(due_notice(&task, 100, 200, 0), None);
        assert_eq!(due_notice(&task, 100, 201, 0), overdue);
        assert_eq!(due_notice(&task, 200, 300, 0), overdue);
        // Already checked before an earlier restart.
        assert_eq!(due_notice(&task, 201, 300, 0), None);
    }

    #[test]
    fn rescheduled_tasks_notify_again_when_they_become_overdue() {
        let rescheduled = task_with_due_time(400);

        assert_eq!(due_notice(&rescheduled, 300, 350, 0), None);
        assert_eq!(
            due_notice(&rescheduled, 350, 450, 0),
            Some((400, DueNotice::Overdue))
        );
    }

    #[test]
    fn reminders_notify_before_the_due_time_once() {
        let task = task_with_due_time(10 * HOUR);
        let reminder = Some((9 * HOUR, DueNotice::Reminder));

        assert_eq!(due_notice(&task, 8 * HOUR, 9 * HOUR, HOUR), None);
        assert_eq!(due_notice(&task, 8 * HOUR, 9 * HOUR + 1, HOUR), reminder);
        assert_eq!(due_notice(&task, 9 * HOUR + 1, 10 * HOUR, HOUR), None);
        assert_eq!(
            due_notice(&task, 9 * HOUR + 1, 10 * HOUR + 1, HOUR),
            Some((10 * HOUR, DueNotice::Overdue))
        );
        // Reminders are off.
        assert_eq!(due_notice(&task, 8 * HOUR, 9 * HOUR + 1, 0), None);
    }

    #[test]
    fn overdue_replaces_a_reminder_missed_in_the_same_period() {
        let task = task_with_due_time(10 * HOUR);

        assert_eq!(
            due_notice(&task, 8 * HOUR, 11 * HOUR, HOUR),
            Some((10 * HOUR, DueNotice::Overdue))
        );
    }

    #[test]
    fn only_valid_tasks_with_due_times_notify() {
        let mut task = task_with_due_time(100);
        task.due_time = None;
        assert_eq!(due_notice(&task, 0, 300, 0), None);

        task.due_time = Some(100);
        task.title.clear();
        assert_eq!(due_notice(&task, 0, 300, 0), None);

        let unsaved = Task {
            id: -1,
            ..task_with_due_time(100)
        };
        assert_eq!(due_notice(&unsaved, 0, 300, 0), None);
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

        assert_eq!(due_notice(&task, since, midnight(14) + HOUR, 0), None);
        assert_eq!(
            due_notice(&task, since, midnight(15) + 1, 0),
            Some((midnight(15) - 1, DueNotice::Overdue))
        );

        let chosen = Task {
            due_at_midnight: true,
            ..task
        };
        assert_eq!(
            due_notice(&chosen, since, midnight(14) + HOUR, 0),
            Some((midnight(14), DueNotice::Overdue))
        );
    }

    #[test]
    fn muted_columns_do_not_notify_on_their_board_only() {
        let task = task_with_due_time(100);
        let other = Task {
            id: 8,
            title: "Review".into(),
            due_time: Some(50),
            ..task.clone()
        };
        let tasks = vec![
            due(1, 10, task.clone()),
            due(1, 11, other),
            due(2, 10, task),
        ];
        let muted = BTreeMap::from([(1, BTreeSet::from([10]))]);

        assert_eq!(
            due_notices(tasks, &muted, 0, 200, 0),
            vec![
                (
                    DueNotice::Overdue,
                    "Board 1".to_owned(),
                    "Review".to_owned()
                ),
                (
                    DueNotice::Overdue,
                    "Board 2".to_owned(),
                    "Follow up".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn many_notices_are_summarized() {
        let notice = |task: &str| (DueNotice::Overdue, "Work".to_owned(), task.to_owned());
        let few = (0..MAX_SHOWN)
            .map(|index| notice(&index.to_string()))
            .collect::<Vec<_>>();
        assert_eq!(notifications(&texts(), few).len(), MAX_SHOWN);

        let many = (0..=MAX_SHOWN)
            .map(|index| notice(&index.to_string()))
            .collect::<Vec<_>>();
        let shown = notifications(&texts(), many);
        assert_eq!(shown.len(), MAX_SHOWN);
        assert_eq!(
            shown.last(),
            Some(&("More tasks".to_owned(), "Open Cardbe".to_owned()))
        );
    }

    #[test]
    fn notification_identifies_the_board() {
        assert_eq!(
            notification_text(&texts(), DueNotice::Overdue, "Work", "Follow up"),
            (
                "Task Expired — Work".into(),
                "Task \"Follow up\" has passed its due date".into()
            )
        );
        assert_eq!(
            notification_text(&texts(), DueNotice::Reminder, "Work", "Follow up"),
            (
                "Task Due Soon — Work".into(),
                "Task \"Follow up\" is due soon".into()
            )
        );
        let chinese = NotificationTexts {
            overdue_title: "任務已逾期 — {board}".into(),
            overdue_body: "任務「{task}」已超過截止時間".into(),
            ..texts()
        };
        assert_eq!(
            notification_text(&chinese, DueNotice::Overdue, "工作 {board}", "跟進 {task}"),
            (
                "任務已逾期 — 工作 {board}".into(),
                "任務「跟進 {task}」已超過截止時間".into()
            )
        );
    }
}
