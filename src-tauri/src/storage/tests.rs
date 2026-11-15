#[test]
fn legacy_iroh_enablement_preserves_usage_and_explicit_preferences() {
    for usage in ["none", "invite", "disabled_invite", "remote", "revocation"] {
        for preference in [None, Some(false), Some(true)] {
            let dir = test_dir("iroh-enablement-migration");
            let mut loaded = load(&dir).unwrap();
            assert!(!loaded.stored.settings.iroh_network.enabled);
            let db = &mut loaded.database;
            let board_id = db.active_board_id();
            match usage {
                "invite" | "disabled_invite" | "revocation" => {
                    db.save_iroh_invite("invite", board_id, "secret", IrohPermission::Viewer)
                        .unwrap();
                    if usage == "disabled_invite" {
                        db.update_iroh_invite("invite", None, Some(false)).unwrap();
                    } else if usage == "revocation" {
                        db.delete_iroh_invite("invite").unwrap();
                    }
                }
                "remote" => {
                    db.create_received_board(
                        "Received",
                        &StoredData::default(),
                        IrohPermission::Viewer,
                        1,
                        "fixture-ticket",
                        None,
                    )
                    .unwrap();
                }
                _ => {}
            }
            let mut expected = loaded.stored.settings.clone();
            expected.notify_enabled = true;
            expected.iroh_network.listen_port = 12345;
            expected.iroh_network.relay_urls = vec!["https://relay.example.com".into()];
            let mut value = serde_json::to_value(&expected).unwrap();
            if let Some(enabled) = preference {
                value["iroh_network"]["enabled"] = enabled.into();
            } else {
                value["iroh_network"]
                    .as_object_mut()
                    .unwrap()
                    .remove("enabled");
            }
            db.connection
                .execute(
                    "UPDATE cardbe_global_metadata SET value=?1 WHERE key='settings'",
                    [value.to_string()],
                )
                .unwrap();
            let boards = db.boards().unwrap();
            drop(loaded);

            expected.iroh_network.enabled = preference.unwrap_or(usage != "none");
            for _ in 0..2 {
                let reopened = load(&dir).unwrap();
                assert_eq!(reopened.stored.settings, expected, "{usage} {preference:?}");
                assert_eq!(reopened.database.boards().unwrap(), boards);
                let persisted: crate::models::Settings =
                    namespaced_metadata_json(&reopened.database.connection, "settings")
                        .unwrap()
                        .unwrap();
                assert_eq!(persisted, expected);
            }
            fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn access_probes_preserve_revocation_until_a_fresh_join_request() {
    let dir = test_dir("iroh-revocation-probe");
    let mut loaded = load(&dir).unwrap();
    let db = &mut loaded.database;
    db.save_iroh_invite(
        "invite",
        db.active_board_id(),
        "secret",
        IrohPermission::Editor,
    )
    .unwrap();
    db.iroh_device_access("invite", "secret", "device", true)
        .unwrap();
    db.set_iroh_device_approved("invite", "device", true)
        .unwrap();
    db.set_iroh_device_approved("invite", "device", false)
        .unwrap();
    assert_eq!(
        db.iroh_device_access("invite", "secret", "device", false)
            .unwrap(),
        Some(IrohDeviceStatus::Revoked)
    );
    assert_eq!(
        db.iroh_device_access("invite", "secret", "device", true)
            .unwrap(),
        Some(IrohDeviceStatus::Pending)
    );
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn deleting_received_board_clears_scoped_data_and_preserves_local_boards() {
    let dir = test_dir("iroh-revoked-copy");
    let mut loaded = load(&dir).unwrap();
    let db = &mut loaded.database;
    let local_id = db.active_board_id();
    let note = db
        .create_note_with_content(1, "Local note".into(), "Keep".into())
        .unwrap();
    crate::plugins::storage::initialize(&db.connection).unwrap();
    let data: StoredData = serde_json::from_value(serde_json::json!({
            "columns": [{"id": 1, "name": "Shared column", "tasks": [{"id": 1, "title": "Shared task"}]}],
            "archives": [{"time": 1, "task": {"id": 2, "title": "Archived task"}}],
            "templates": [{"id": 1, "name": "Shared template", "task": {"title": "Template task"}}]
        })).unwrap();
    let doc = crate::loro_board::apply_local_delta(None, &StoredData::default(), &data).unwrap();
    let received = db
        .create_received_board(
            "Shared",
            &data,
            IrohPermission::Editor,
            1,
            "ticket",
            Some(&doc),
        )
        .unwrap();
    db.save_iroh_conflict(
        received.id,
        2,
        "Shared",
        IrohPermission::Editor,
        &data,
        Some(&doc),
    )
    .unwrap();
    let mut pending = data.clone();
    pending.columns[0].tasks[0].title = "Unsent local edit".into();
    db.replace_board_as_local_edit(received.id, &pending)
        .unwrap();
    assert!(db.iroh_loro_update(received.id).unwrap().is_some());
    assert_eq!(db.archived_task_count(received.id).unwrap(), 1);
    for board_id in [local_id, received.id] {
        db.connection
                .execute(
                    "INSERT INTO cardbe_plugin_sources(instance_id,external_key,board_id,task_id,last_import) VALUES('instance','key',?1,1,'')",
                    [board_id],
                )
                .unwrap();
    }
    db.delete_board(received.id).unwrap();
    assert_eq!(db.archived_task_count(received.id).unwrap(), 0);
    let sources: Vec<i64> = db
        .connection
        .prepare("SELECT board_id FROM cardbe_plugin_sources")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(sources, vec![local_id]);
    assert!(db.board_exists(local_id).unwrap());
    assert!(!db.board_exists(received.id).unwrap());
    assert!(db.iroh_remote(received.id).unwrap().is_none());
    assert!(db.iroh_conflict(received.id).unwrap().is_none());
    assert!(db.iroh_loro_update(received.id).unwrap().is_none());
    for table in [
        "cardbe_columns",
        "cardbe_tasks",
        "cardbe_archives",
        "cardbe_templates",
        "cardbe_board_metadata",
    ] {
        let remaining: i64 = db
            .connection
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE board_id=?1"),
                [received.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 0);
    }
    assert!(db
        .get_notes()
        .unwrap()
        .iter()
        .any(|item| item.id == note.id));
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn deleted_invitations_keep_revocation_service_available_after_restart() {
    for cascade in [false, true] {
        let dir = test_dir("iroh-revocation-host");
        let mut loaded = load(&dir).unwrap();
        let db = &mut loaded.database;
        assert!(!db.iroh_host_required().unwrap());
        let board = db.create_board("Shared").unwrap();
        db.save_iroh_invite("invite", board.id, "secret", IrohPermission::Viewer)
            .unwrap();
        assert_eq!(
            db.iroh_invite("invite").unwrap(),
            Some((
                "invite".into(),
                board.id,
                "secret".into(),
                IrohPermission::Viewer,
                true
            ))
        );
        assert_eq!(db.iroh_invite("other").unwrap(), None);
        assert!(db.iroh_host_required().unwrap());
        db.update_iroh_invite("invite", None, Some(false)).unwrap();
        assert!(!db.iroh_host_required().unwrap());
        if cascade {
            db.delete_board(board.id).unwrap();
        } else {
            db.delete_iroh_invite("invite").unwrap();
        }
        assert!(db.iroh_invites().unwrap().is_empty());
        assert!(db.iroh_host_required().unwrap());
        drop(loaded);
        let reopened = load(&dir).unwrap();
        assert!(reopened.database.iroh_host_required().unwrap());
        drop(reopened);
        fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn local_writer_is_stable_until_snapshot_replacement_or_reopen() {
    let dir = test_dir("loro-session-peer");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let board_id = loaded.database.active_board_id();
    let mut data = StoredData::default();
    for index in 0..10 {
        data.templates = vec![TaskTemplate {
            id: 1,
            name: format!("Edit {index}"),
            task: Task::default(),
        }];
        loaded
            .database
            .replace_board_as_local_edit(board_id, &data)
            .unwrap();
        let update = loaded.database.iroh_loro_update(board_id).unwrap().unwrap();
        let vector =
            loro::VersionVector::decode(&crate::loro_board::state_vector(Some(&update)).unwrap())
                .unwrap();
        assert_eq!(vector.len(), 1);
    }
    let update = loaded.database.iroh_loro_update(board_id).unwrap().unwrap();
    // Even a replacement containing our complete history starts a new writer.
    loaded
        .database
        .apply_iroh_snapshot(
            board_id,
            "Board",
            IrohPermission::Editor,
            1,
            &data,
            Some(&update),
        )
        .unwrap();
    data.templates[0].name = "After replacement".into();
    loaded
        .database
        .replace_board_as_local_edit(board_id, &data)
        .unwrap();
    let update = loaded.database.iroh_loro_update(board_id).unwrap().unwrap();
    let vector =
        loro::VersionVector::decode(&crate::loro_board::state_vector(Some(&update)).unwrap())
            .unwrap();
    assert_eq!(vector.len(), 2);
    drop(loaded);
    let mut loaded = load(&dir).unwrap();
    data.templates[0].name = "After restart".into();
    loaded
        .database
        .replace_board_as_local_edit(board_id, &data)
        .unwrap();
    let update = loaded.database.iroh_loro_update(board_id).unwrap().unwrap();
    let vector =
        loro::VersionVector::decode(&crate::loro_board::state_vector(Some(&update)).unwrap())
            .unwrap();
    assert_eq!(vector.len(), 3);
    assert_eq!(
        crate::loro_board::project(&update).unwrap().templates,
        data.templates
    );
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}
use super::*;
use crate::models::TaskItem;

#[test]
fn board_summaries_identify_received_and_hosted_sharing() {
    let dir = test_dir("shared-board-summaries");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let owner = loaded.database.active_board_id();
    assert!(!loaded.database.boards().unwrap()[0].is_shared);
    loaded
        .database
        .save_iroh_invite("invite", owner, "secret", IrohPermission::Viewer)
        .unwrap();
    assert!(loaded.database.boards().unwrap()[0].is_shared);
    loaded.database.delete_iroh_invite("invite").unwrap();
    assert!(!loaded.database.boards().unwrap()[0].is_shared);
    for role in [BoardRole::Viewer, BoardRole::Editor] {
        loaded
            .database
            .set_shared_board(owner, role, SyncStatus::Synced, 1)
            .unwrap();
        assert!(loaded.database.boards().unwrap()[0].is_shared);
    }
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn missing_boards_preserve_domain_errors_at_the_command_boundary() {
    let dir = test_dir("missing-board-errors");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let active = loaded.database.active_board_id();
    for error in [
        loaded.database.board_role(-1).unwrap_err(),
        loaded
            .database
            .move_tasks_to_board(active, &[1], -1, 1)
            .unwrap_err(),
    ] {
        let value = serde_json::to_value(crate::errors::CommandError::repository(error)).unwrap();
        assert_eq!(value, serde_json::json!({"code": "BOARD_NOT_FOUND"}));
    }
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn task_explorer_keeps_date_only_due_dates_on_time_until_the_day_ends() {
    use chrono::{Local, TimeZone};
    let at = |day, hour| {
        Local
            .with_ymd_and_hms(2026, 10, day, hour, 0, 0)
            .earliest()
            .unwrap()
            .timestamp_millis()
    };
    let dir = test_dir("task-explorer-date-only-overdue");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let board_id = loaded.database.active_board_id();
    let data = StoredData {
        columns: vec![Column {
            id: 1,
            name: "Todo".into(),
            color: String::new(),
            sort_order: Default::default(),
            sort_rules: Vec::new(),
            done: false,
            tasks: [
                (1, at(13, 0), false),
                (2, at(14, 0), false),
                (3, at(14, 9), false),
                (4, at(14, 11), false),
                (5, at(14, 0), true),
            ]
            .into_iter()
            .map(|(id, due_time, due_at_midnight)| Task {
                id,
                title: format!("Task {id}"),
                due_time: Some(u128::try_from(due_time).unwrap()),
                due_at_midnight,
                ..Task::default()
            })
            .collect(),
        }],
        ..StoredData::default()
    };
    loaded
        .database
        .replace_board_as_local_edit(board_id, &data)
        .unwrap();
    let filter = TaskExplorerQuery {
        status: "overdue".into(),
        sort: "due".into(),
        now: at(14, 10),
        ..Default::default()
    };

    let rows = loaded.database.list_all_task_page(&filter, 0, 10).unwrap();
    assert_eq!(
        rows.iter().map(|row| row.task.id).collect::<Vec<_>>(),
        vec![1, 5, 3]
    );
}

#[test]
fn task_explorer_filters_and_sorts_by_priority() {
    use crate::models::TaskPriority::{High, Low, Urgent};
    let dir = test_dir("task-explorer-priority");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let board_id = loaded.database.active_board_id();
    let data = StoredData {
        columns: vec![Column {
            id: 1,
            name: "Todo".into(),
            color: String::new(),
            sort_order: Default::default(),
            sort_rules: Vec::new(),
            done: false,
            tasks: [
                (1, None, Some(5)),
                (2, Some(Low), None),
                (3, Some(Urgent), Some(9)),
                (4, Some(Urgent), Some(2)),
                (5, Some(High), None),
                (6, Some(Urgent), None),
            ]
            .into_iter()
            .map(|(id, priority, due_time)| Task {
                id,
                title: format!("Task {id}"),
                priority,
                due_time,
                ..Task::default()
            })
            .collect(),
        }],
        ..StoredData::default()
    };
    loaded
        .database
        .replace_board_as_local_edit(board_id, &data)
        .unwrap();
    let ids = |filter: &TaskExplorerQuery| {
        loaded
            .database
            .list_all_task_page(filter, 0, 10)
            .unwrap()
            .iter()
            .map(|row| row.task.id)
            .collect::<Vec<_>>()
    };

    // Most important first, then by due date with undated tasks last.
    let sorted = TaskExplorerQuery {
        status: "all".into(),
        sort: "priority".into(),
        ..Default::default()
    };
    assert_eq!(ids(&sorted), vec![4, 3, 6, 5, 2, 1]);

    let filtered = TaskExplorerQuery {
        priorities: Some(vec!["urgent".into(), "none".into()]),
        sort: "due".into(),
        ..sorted
    };
    assert_eq!(ids(&filtered), vec![4, 1, 3, 6]);

    // Searches filter by priority before ranking.
    let searched = TaskExplorerQuery {
        query: "Task".into(),
        priorities: Some(vec!["high".into(), "low".into()]),
        ..filtered
    };
    assert_eq!(ids(&searched), vec![2, 5]);
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn task_explorer_filters_by_completion_labels_waiting_and_recurrence() {
    let dir = test_dir("task-explorer-smart-filters");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let board_id = loaded.database.active_board_id();
    let task = |id: i64, labels: &[&str], blocked_by: &[i64]| Task {
        id,
        title: format!("Task {id}"),
        labels: labels.iter().map(|label| label.to_string()).collect(),
        blocked_by: blocked_by.to_vec(),
        created_time: 1_000 + id as u128,
        ..Task::default()
    };
    let column = |id: i64, done: bool, tasks: Vec<Task>| Column {
        id,
        name: format!("Column {id}"),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done,
        tasks,
    };
    let data = StoredData {
        columns: vec![
            column(
                1,
                false,
                vec![
                    // Waits on an open task and on a done one.
                    task(1, &["工作"], &[2, 3]),
                    task(2, &["home"], &[]),
                    Task {
                        recurrence: Some(crate::models::Recurrence {
                            frequency: crate::models::RecurrenceFrequency::Daily,
                            interval: 1,
                        }),
                        ..task(4, &[], &[3, 9])
                    },
                ],
            ),
            column(2, true, vec![task(3, &["工作", "home"], &[])]),
        ],
        archives: vec![Archive {
            time: 5,
            task: task(5, &["工作"], &[2]),
        }],
        ..StoredData::default()
    };
    loaded
        .database
        .replace_board_as_local_edit(board_id, &data)
        .unwrap();
    let rows = |filter: TaskExplorerQuery| {
        loaded
            .database
            .list_all_task_page(
                &TaskExplorerQuery {
                    status: "all".into(),
                    sort: "title".into(),
                    ..filter
                },
                0,
                10,
            )
            .unwrap()
    };
    let ids = |filter: TaskExplorerQuery| {
        rows(filter)
            .iter()
            .map(|row| row.task.id)
            .collect::<Vec<_>>()
    };

    let all = rows(TaskExplorerQuery::default());
    let row = |id: i64| all.iter().find(|row| row.task.id == id).unwrap();
    assert!(row(3).done && !row(1).done && !row(5).done);
    assert_eq!(row(1).waiting_count, 1);
    assert_eq!(row(3).column_id, Some(2));
    assert_eq!(row(5).column_id, None);
    // References to done, missing and archived tasks never block.
    assert_eq!(row(4).waiting_count, 0);
    assert_eq!(row(5).waiting_count, 0);

    let completion = |states: &[&str]| TaskExplorerQuery {
        completion: Some(states.iter().map(|state| state.to_string()).collect()),
        ..Default::default()
    };
    assert_eq!(ids(completion(&["open"])), vec![1, 2, 4]);
    assert_eq!(ids(completion(&["done", "archived"])), vec![3, 5]);
    assert_eq!(
        ids(TaskExplorerQuery {
            labels: Some(vec!["工作".into()]),
            ..completion(&["open", "done"])
        }),
        vec![1, 3]
    );
    let waiting = |value: &str| TaskExplorerQuery {
        waiting: Some(value.into()),
        ..Default::default()
    };
    assert_eq!(ids(waiting("waiting")), vec![1]);
    assert_eq!(ids(waiting("not_waiting")), vec![2, 3, 4, 5]);
    // Only open tasks that open tasks wait on; the archived task does not count.
    assert_eq!(ids(waiting("waited_on")), vec![2]);
    let recurring = |value: bool| TaskExplorerQuery {
        recurring: Some(value),
        ..Default::default()
    };
    assert_eq!(ids(recurring(true)), vec![4]);
    assert_eq!(ids(recurring(false)), vec![1, 2, 3, 5]);
    let columns = |refs: &[(i64, i64)]| TaskExplorerQuery {
        columns: Some(
            refs.iter()
                .map(|&(board_id, column_id)| crate::focuses::ColumnRef {
                    board_id,
                    column_id,
                })
                .collect(),
        ),
        ..Default::default()
    };
    // Archived tasks have no column; other boards' same column IDs differ.
    assert_eq!(ids(columns(&[(board_id, 1)])), vec![1, 2, 4]);
    assert_eq!(
        ids(columns(&[(board_id, 1), (board_id, 2)])),
        vec![1, 2, 3, 4]
    );
    assert!(ids(columns(&[(board_id + 1, 1)])).is_empty());
    assert_eq!(
        ids(TaskExplorerQuery {
            query: "Task".into(),
            ..columns(&[(board_id, 2)])
        }),
        vec![3]
    );

    // Searches apply the same filters before ranking.
    assert_eq!(
        ids(TaskExplorerQuery {
            query: "Task".into(),
            ..waiting("waited_on")
        }),
        vec![2]
    );
    let newest = loaded
        .database
        .list_all_task_page(
            &TaskExplorerQuery {
                status: "active".into(),
                sort: "created".into(),
                ..Default::default()
            },
            0,
            10,
        )
        .unwrap();
    assert_eq!(
        newest.iter().map(|row| row.task.id).collect::<Vec<_>>(),
        vec![4, 3, 2, 1]
    );
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn task_explorer_filters_and_sorts_before_paginating() {
    let dir = test_dir("task-explorer-pagination");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let first = loaded.database.active_board_id();
    let second = loaded.database.create_board("Second").unwrap().id;
    for (board_id, name) in [(first, "Zulu"), (second, "Alpha")] {
        let data = StoredData {
            columns: vec![Column {
                id: 1,
                name: name.into(),
                color: String::new(),
                sort_order: Default::default(),
                sort_rules: Vec::new(),
                done: false,
                tasks: (1..=60)
                    .map(|id| Task {
                        id,
                        title: format!("Task {:02}", 61 - id),
                        due_time: Some((61 - id) as u128),
                        recurrence: (id == 60).then_some(crate::models::Recurrence {
                            frequency: crate::models::RecurrenceFrequency::Daily,
                            interval: 1,
                        }),
                        ..Task::default()
                    })
                    .chain(std::iter::once(Task {
                        id: 61,
                        title: "No date".into(),
                        ..Task::default()
                    }))
                    .collect(),
            }],
            archives: vec![Archive {
                time: 100,
                task: Task {
                    id: 62,
                    title: "Archived".into(),
                    due_time: Some(1),
                    ..Task::default()
                },
            }],
            ..StoredData::default()
        };
        loaded
            .database
            .replace_board_as_local_edit(board_id, &data)
            .unwrap();
    }
    loaded
        .database
        .set_shared_board(second, BoardRole::Viewer, SyncStatus::Synced, 1)
        .unwrap();
    let mut filter = TaskExplorerQuery {
        status: "active".into(),
        sort: "due".into(),
        ..Default::default()
    };
    let page = loaded.database.list_all_task_page(&filter, 0, 50).unwrap();
    assert_eq!(page[0].task.due_time, Some(1));
    assert_eq!(page[49].task.due_time, Some(25));
    let next = loaded.database.list_all_task_page(&filter, 50, 50).unwrap();
    assert_eq!(next[0].task.due_time, Some(26));
    assert!(page.iter().all(|row| !next
        .iter()
        .any(|other| row.board_id == other.board_id && row.task.id == other.task.id)));

    filter.board_ids = Some(vec![first, second]);
    let selected = loaded.database.list_all_task_page(&filter, 0, 50).unwrap();
    assert_eq!(
        selected
            .iter()
            .map(|row| (row.board_id, row.task.id))
            .collect::<Vec<_>>(),
        page.iter()
            .map(|row| (row.board_id, row.task.id))
            .collect::<Vec<_>>()
    );
    filter.board_ids = Some(vec![]);
    assert!(loaded
        .database
        .list_all_task_page(&filter, 0, 50)
        .unwrap()
        .is_empty());
    filter.board_ids = Some(vec![first, first]);
    let selected = loaded.database.list_all_task_page(&filter, 0, 200).unwrap();
    assert_eq!(selected.len(), 61);
    assert!(selected.iter().all(|row| row.board_id == first));

    filter.board_ids = Some(vec![second]);
    filter.column_id = Some(1);
    filter.due_start = Some(10);
    filter.due_end = Some(12);
    let rows = loaded.database.list_all_task_page(&filter, 0, 50).unwrap();
    assert_eq!(
        rows.iter().map(|row| row.task.due_time).collect::<Vec<_>>(),
        vec![Some(10), Some(11)]
    );
    assert!(rows.iter().all(|row| row.board_id == second));
    filter.due_start = None;
    filter.due_end = None;
    filter.status = "overdue".into();
    filter.now = 2;
    assert_eq!(
        loaded
            .database
            .list_all_task_page(&filter, 0, 50)
            .unwrap()
            .len(),
        1
    );
    filter.status = "recurring".into();
    assert_eq!(
        loaded.database.list_all_task_page(&filter, 0, 50).unwrap()[0]
            .task
            .id,
        60
    );
    filter.status = "active".into();
    filter.due = "none".into();
    assert_eq!(
        loaded.database.list_all_task_page(&filter, 0, 50).unwrap()[0]
            .task
            .id,
        61
    );
    filter.due = "all".into();
    filter.column_id = None;
    filter.status = "archived".into();
    filter.archive_start = Some(100);
    filter.archive_end = Some(101);
    assert_eq!(
        loaded
            .database
            .list_all_task_page(&filter, 0, 50)
            .unwrap()
            .len(),
        1
    );
    filter.archive_end = Some(100);
    assert!(loaded
        .database
        .list_all_task_page(&filter, 0, 50)
        .unwrap()
        .is_empty());
    filter = TaskExplorerQuery {
        status: "active".into(),
        sort: "column".into(),
        ..Default::default()
    };
    assert_eq!(
        loaded.database.list_all_task_page(&filter, 0, 1).unwrap()[0]
            .column_name
            .as_deref(),
        Some("Alpha")
    );
    filter.sort = "title".into();
    assert_eq!(
        loaded.database.list_all_task_page(&filter, 0, 1).unwrap()[0]
            .task
            .title,
        "No date"
    );
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn task_search_finds_title_description_labels_and_checklist_text() {
    let dir = test_dir("task-search");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let board_id = loaded.database.active_board_id();
    let data = StoredData {
        columns: vec![Column {
            id: 1,
            name: "Todo".into(),
            color: String::new(),
            sort_order: Default::default(),
            sort_rules: Vec::new(),
            done: false,
            tasks: vec![Task {
                id: 2,
                title: "Write release notes".into(),
                description: "Prepare the public changelog".into(),
                labels: vec!["launch".into()],
                items: vec![TaskItem {
                    id: "item_1".into(),
                    text: "Add screenshots".into(),
                    completed: false,
                }],
                ..Task::default()
            }],
        }],
        ..StoredData::default()
    };
    loaded
        .database
        .replace_board_as_local_edit(board_id, &data)
        .unwrap();
    let all_tasks = loaded
        .database
        .list_all_task_page(
            &TaskExplorerQuery {
                query: "release notes".into(),
                ..Default::default()
            },
            0,
            10,
        )
        .unwrap();
    assert_eq!(all_tasks.len(), 1);
    assert_eq!(all_tasks[0].board_id, board_id);
    assert_eq!(all_tasks[0].column_name.as_deref(), Some("Todo"));
    assert_eq!(all_tasks[0].task.title, "Write release notes");

    for query in ["RELEASE NOTES", "public changelog", "launch", "screenshots"] {
        assert_eq!(
            loaded.database.search_tasks(board_id, query).unwrap(),
            vec![2]
        );
    }
    assert!(loaded
        .database
        .search_tasks(board_id, "not found")
        .unwrap()
        .is_empty());

    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn hybrid_search_ranking_filters_and_live_changes() {
    let dir = test_dir("hybrid-search");
    let mut loaded = load(&dir).unwrap();
    let first = loaded.database.active_board_id();
    let second = loaded.database.create_board("Second").unwrap().id;
    let task = |id, title: &str, description: &str| Task {
        id,
        title: title.into(),
        description: description.into(),
        due_time: Some(10),
        ..Task::default()
    };
    let mut data = StoredData {
        columns: vec![Column {
            id: 1,
            name: "Todo".into(),
            color: String::new(),
            sort_order: Default::default(),
            sort_rules: Vec::new(),
            done: false,
            tasks: vec![
                task(1, "Alpha", ""),
                task(2, "Alpha beta", ""),
                task(3, "a long phrase", ""),
                task(4, "Notes", "alpha in description"),
                task(5, "臺灣工作報告", ""),
                task(6, "🚀 工作報告", ""),
                task(7, "臺灣 Release", ""),
            ],
        }],
        archives: vec![Archive {
            time: 100,
            task: task(8, "a long phrase", ""),
        }],
        ..StoredData::default()
    };
    loaded
        .database
        .replace_board_as_local_edit(first, &data)
        .unwrap();
    loaded
        .database
        .replace_board_as_local_edit(second, &data)
        .unwrap();
    let ids = loaded.database.search_tasks(first, "alpha").unwrap();
    assert_eq!(&ids[..2], &[1, 2]);
    assert_eq!(ids.len(), 5);
    for id in [3, 4, 8] {
        assert!(ids.contains(&id));
    }
    assert_eq!(ids, loaded.database.search_tasks(first, "alpha").unwrap());
    for (query, id) in [("臺報", 5), ("🚀報", 6), ("臺R", 7)] {
        assert_eq!(
            loaded.database.search_tasks(first, query).unwrap(),
            vec![id]
        );
    }
    assert!(loaded
        .database
        .search_tasks(first, "  ")
        .unwrap()
        .is_empty());
    assert!(loaded
        .database
        .search_tasks(first, "missing-xyz")
        .unwrap()
        .is_empty());
    let mut filter = TaskExplorerQuery {
        query: "alpha".into(),
        status: "active".into(),
        sort: "title".into(),
        search_options: Some(crate::search::SearchOptions {
            candidate_limit: 1,
            result_limit: 2,
            ..Default::default()
        }),
        ..Default::default()
    };
    let invalid_filter = TaskExplorerQuery {
        search_options: Some(crate::search::SearchOptions {
            candidate_limit: 0,
            ..Default::default()
        }),
        ..Default::default()
    };
    let error = loaded
        .database
        .list_all_task_page(&invalid_filter, 0, 10)
        .err()
        .unwrap();
    assert!(matches!(
        error.downcast_ref::<DomainError>(),
        Some(DomainError::InvalidArgument)
    ));
    // Board restrictions must precede Top K, even when another board has identical IDs.
    filter.board_ids = Some(vec![second]);
    let rows = loaded.database.list_all_task_page(&filter, 0, 10).unwrap();
    assert!(!rows.is_empty());
    assert!(rows
        .iter()
        .all(|row| row.board_id == second && row.archived_at.is_none()));
    filter.search_options = None;
    filter.board_ids = None;
    assert_eq!(
        loaded
            .database
            .list_all_task_page(&filter, 0, 10)
            .unwrap()
            .len(),
        8
    );
    filter.board_ids = Some(vec![first]);
    let all = loaded.database.list_all_task_page(&filter, 0, 10).unwrap();
    let page = loaded.database.list_all_task_page(&filter, 1, 2).unwrap();
    assert_eq!(
        page.iter().map(|row| row.task.id).collect::<Vec<_>>(),
        all[1..3].iter().map(|row| row.task.id).collect::<Vec<_>>()
    );
    let count = |status: &str, filter: &TaskExplorerQuery| {
        loaded
            .database
            .list_all_task_page(
                &TaskExplorerQuery {
                    status: status.into(),
                    column_id: if status == "archived" {
                        None
                    } else {
                        filter.column_id
                    },
                    ..filter.clone()
                },
                0,
                100,
            )
            .unwrap()
            .len() as i64
    };
    for query in ["alpha", "", "a"] {
        let filter = TaskExplorerQuery {
            query: query.into(),
            now: 11,
            ..filter.clone()
        };
        assert_eq!(
            loaded.database.count_all_tasks(&filter).unwrap(),
            TaskStatusCounts {
                all: count("all", &filter),
                active: count("active", &filter),
                overdue: count("overdue", &filter),
                recurring: count("recurring", &filter),
                archived: count("archived", &filter),
            },
            "{query}"
        );
    }
    // Description hits carry an excerpt; title hits do not.
    let notes = all.iter().find(|row| row.task.id == 4).unwrap();
    assert_eq!(notes.excerpt.as_deref(), Some("alpha in description"));
    assert!(all
        .iter()
        .find(|row| row.task.id == 1)
        .unwrap()
        .excerpt
        .is_none());
    // Relevance keeps the fused order (exact title, then prefix) across pages.
    filter.sort = "relevance".into();
    let ranked = loaded.database.list_all_task_page(&filter, 0, 10).unwrap();
    assert_eq!(
        ranked
            .iter()
            .map(|row| row.task.id)
            .take(2)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(ranked.len(), all.len());
    let page = loaded.database.list_all_task_page(&filter, 1, 2).unwrap();
    assert_eq!(
        page.iter().map(|row| row.task.id).collect::<Vec<_>>(),
        ranked[1..3]
            .iter()
            .map(|row| row.task.id)
            .collect::<Vec<_>>()
    );
    filter.sort = "title".into();
    filter.status = "archived".into();
    assert_eq!(
        loaded.database.list_all_task_page(&filter, 0, 10).unwrap()[0]
            .task
            .id,
        8
    );
    filter.archive_start = Some(101);
    assert!(loaded
        .database
        .list_all_task_page(&filter, 0, 10)
        .unwrap()
        .is_empty());
    filter.archive_start = None;
    filter.status = "active".into();
    filter.column_id = Some(99);
    assert!(loaded
        .database
        .list_all_task_page(&filter, 0, 10)
        .unwrap()
        .is_empty());
    filter.column_id = Some(1);
    filter.due_start = Some(11);
    assert!(loaded
        .database
        .list_all_task_page(&filter, 0, 10)
        .unwrap()
        .is_empty());
    filter.due_start = None;
    filter.status = "overdue".into();
    filter.now = 11;
    assert_eq!(
        loaded
            .database
            .list_all_task_page(&filter, 0, 10)
            .unwrap()
            .len(),
        4
    );
    filter.due = "none".into();
    assert!(loaded
        .database
        .list_all_task_page(&filter, 0, 10)
        .unwrap()
        .is_empty());

    data.columns[0].tasks[0].title = "Delta".into();
    data.columns[0].tasks.retain(|task| task.id != 2);
    let archived = data.columns[0].tasks.remove(1);
    data.archives.push(Archive {
        time: 200,
        task: archived,
    });
    data.columns[0].tasks.push(task(9, "Alphabet", ""));
    loaded
        .database
        .replace_board_as_local_edit(first, &data)
        .unwrap();
    let reader = Database::open_search_reader(loaded.database.path().to_owned()).unwrap();
    let ids = reader.search_tasks(first, "alpha").unwrap();
    assert!(!ids.contains(&1) && !ids.contains(&2));
    assert!(ids.contains(&3) && ids.contains(&9));
    drop(reader);
    let restored = data.archives.pop().unwrap().task;
    data.columns[0].tasks.push(restored);
    loaded
        .database
        .replace_board_as_local_edit(first, &data)
        .unwrap();
    filter.due = "all".into();
    filter.status = "active".into();
    assert!(loaded
        .database
        .list_all_task_page(&filter, 0, 10)
        .unwrap()
        .iter()
        .any(|row| row.task.id == 3));
    loaded
        .database
        .move_tasks_to_board(first, &[9], second, 1)
        .unwrap();
    assert!(!loaded
        .database
        .search_tasks(first, "alphabet")
        .unwrap()
        .contains(&9));
    // Moving allocates a fresh ID on the destination board.
    let moved = loaded.database.search_tasks(second, "alphabet").unwrap();
    assert!(!moved.is_empty());
    assert_eq!(
        loaded
            .database
            .load_task(second, moved[0])
            .unwrap()
            .unwrap()
            .title,
        "Alphabet"
    );
    data.columns[0].tasks = vec![task(10, "Remote release notes", "")];
    data.archives.clear();
    loaded
        .database
        .apply_iroh_snapshot(first, "Shared", IrohPermission::Viewer, 20, &data, None)
        .unwrap();
    assert_eq!(
        loaded.database.search_tasks(first, "rrn").unwrap(),
        vec![10]
    );
    assert!(loaded
        .database
        .search_tasks(first, "alpha")
        .unwrap()
        .is_empty());
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "Synthetic search benchmark: run alone with --ignored --nocapture"]
fn hybrid_search_benchmark() {
    struct SearchLogger;
    impl log::Log for SearchLogger {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.target() == "storage.search"
        }
        fn log(&self, record: &log::Record) {
            if self.enabled(record.metadata()) {
                println!("{}", record.args());
            }
        }
        fn flush(&self) {}
    }
    static LOGGER: SearchLogger = SearchLogger;
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Debug);
    for size in [1_000, 10_000, 50_000, 100_000] {
        let dir = test_dir(&format!("hybrid-bench-{size}"));
        let mut loaded = load(&dir).unwrap();
        let board = loaded.database.active_board_id();
        let data = StoredData {
            columns: vec![Column {
                id: 1,
                name: "Todo".into(),
                color: String::new(),
                sort_order: Default::default(),
                sort_rules: Vec::new(),
                done: false,
                tasks: vec![],
            }],
            ..Default::default()
        };
        loaded
            .database
            .replace_board_as_local_edit(board, &data)
            .unwrap();
        let started = std::time::Instant::now();
        let tx = loaded.database.connection.transaction().unwrap();
        {
            let mut insert = tx.prepare("INSERT INTO cardbe_tasks(board_id,id,column_id,position,payload) VALUES(?1,?2,1,?2,?3)").unwrap();
            for id in 1..=size {
                let task = Task {
                    id,
                    title: format!("Release planning task {id} 臺灣 🚀"),
                    description: "Public changelog and release documentation".into(),
                    ..Default::default()
                };
                insert
                    .execute(params![board, id, serde_json::to_string(&task).unwrap()])
                    .unwrap();
            }
        }
        tx.commit().unwrap();
        println!(
            "tasks={size} fixture_and_fts_init_ms={} pid={}",
            started.elapsed().as_millis(),
            std::process::id()
        );
        for query in ["release", "rpt", "臺🚀", "no-match-xyz"] {
            println!("query={query}");
            for _ in 0..3 {
                let reader =
                    Database::open_search_reader(loaded.database.path().to_owned()).unwrap();
                let started = std::time::Instant::now();
                let ids = reader.search_tasks(board, query).unwrap();
                assert!(ids.len() <= crate::search::SearchOptions::default().result_limit);
                println!("command_storage_us={}", started.elapsed().as_micros());
            }
        }
        #[cfg(windows)]
        {
            let memory = std::process::Command::new("powershell.exe")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    &format!("(Get-Process -Id {}).PeakWorkingSet64", std::process::id()),
                ])
                .output()
                .unwrap();
            assert!(memory.status.success());
            println!(
                "tasks={size} process_peak_working_set_bytes={}",
                String::from_utf8(memory.stdout).unwrap().trim()
            );
        }
        drop(loaded);
        fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn permission_downgrade_after_probe_rejects_upload_and_preserves_local_conflict() {
    let owner_dir = test_dir("downgrade-owner");
    let editor_dir = test_dir("downgrade-editor");
    let mut owner = load(&owner_dir).unwrap();
    let mut editor = load(&editor_dir).unwrap();
    let owner_id = owner.database.active_board_id();
    let mut base = StoredData::default();
    base.columns.push(Column {
        id: 1,
        name: "Before".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: vec![],
    });
    owner
        .database
        .replace_board_as_local_edit(owner_id, &base)
        .unwrap();
    owner
        .database
        .save_iroh_invite("invite", owner_id, "secret", IrohPermission::Editor)
        .unwrap();
    owner
        .database
        .iroh_device_access("invite", "secret", "device", true)
        .unwrap();
    owner
        .database
        .set_iroh_device_approved("invite", "device", true)
        .unwrap();
    let original = owner.database.iroh_loro_update(owner_id).unwrap().unwrap();
    let revision = owner.database.board_sync_state(owner_id).unwrap().0;
    let editor_id = editor
        .database
        .create_received_board(
            "Shared",
            &base,
            IrohPermission::Editor,
            revision,
            "ticket",
            Some(&original),
        )
        .unwrap()
        .id;
    let mut local = base.clone();
    local.columns[0].name = "Unsent local edit".into();
    editor
        .database
        .replace_board_as_local_edit(editor_id, &local)
        .unwrap();
    let upload = editor
        .database
        .iroh_loro_update(editor_id)
        .unwrap()
        .unwrap();
    // The editor has already received the owner's state vector when the
    // owner changes permission, before the upload transaction begins.
    let vector = crate::loro_board::state_vector(Some(&original)).unwrap();
    let diff = crate::loro_board::diff(Some(&upload), &vector).unwrap();
    owner
        .database
        .update_iroh_invite("invite", Some(IrohPermission::Viewer), None)
        .unwrap();
    assert!(owner
        .database
        .apply_iroh_loro_update(owner_id, "invite", "secret", "device", &diff)
        .is_err());
    assert_eq!(
        owner
            .database
            .read_board_complete(owner_id)
            .unwrap()
            .columns,
        base.columns
    );
    assert_eq!(
        owner.database.iroh_loro_update(owner_id).unwrap().unwrap(),
        original
    );
    assert_eq!(
        owner.database.board_sync_state(owner_id).unwrap().0,
        revision
    );
    editor
        .database
        .save_iroh_conflict(
            editor_id,
            revision,
            "Shared",
            IrohPermission::Viewer,
            &base,
            None,
        )
        .unwrap();
    assert_eq!(
        editor
            .database
            .read_board_complete(editor_id)
            .unwrap()
            .columns,
        local.columns
    );
    assert_eq!(
        editor
            .database
            .iroh_loro_update(editor_id)
            .unwrap()
            .unwrap(),
        upload
    );
    assert_eq!(
        editor.database.board_sync_state(editor_id).unwrap().1,
        SyncStatus::Conflict
    );
    assert_eq!(
        editor.database.board_role(editor_id).unwrap(),
        BoardRole::Viewer
    );
    assert_eq!(
        editor
            .database
            .iroh_conflict(editor_id)
            .unwrap()
            .unwrap()
            .data
            .columns,
        base.columns
    );
    let copy = editor
        .database
        .use_iroh_conflict_remote(editor_id, "Shared", &local)
        .unwrap();
    assert_eq!(
        editor
            .database
            .read_board_complete(copy.id)
            .unwrap()
            .columns,
        local.columns
    );
    assert_eq!(
        editor
            .database
            .read_board_complete(editor_id)
            .unwrap()
            .columns,
        base.columns
    );
    drop(owner);
    drop(editor);
    fs::remove_dir_all(owner_dir).unwrap();
    fs::remove_dir_all(editor_dir).unwrap();
}

#[test]
fn loro_push_is_atomic_idempotent_and_checks_current_permission() {
    let dir = test_dir("loro-capability");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let board_id = loaded.database.active_board_id();
    let mut base = StoredData::default();
    base.columns.push(Column {
        id: 1,
        name: "Todo".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: vec![Task {
            id: 2,
            title: "Before".into(),
            ..Task::default()
        }],
    });
    loaded
        .database
        .replace_board_as_local_edit(board_id, &base)
        .unwrap();
    loaded
        .database
        .save_iroh_invite("editor", board_id, "secret", IrohPermission::Editor)
        .unwrap();
    loaded
        .database
        .iroh_device_access("editor", "secret", "device", true)
        .unwrap();
    loaded
        .database
        .set_iroh_device_approved("editor", "device", true)
        .unwrap();
    let original = loaded.database.iroh_loro_update(board_id).unwrap().unwrap();
    let mut edited = base.clone();
    edited.columns[0].tasks[0].title = "After".into();
    let update = crate::loro_board::apply_local_delta(Some(&original), &base, &edited).unwrap();
    let (changed, revision, data) = loaded
        .database
        .apply_iroh_loro_update(board_id, "editor", "secret", "device", &update)
        .unwrap();
    assert!(changed);
    assert_eq!(data.columns[0].tasks[0].title, "After");
    assert!(
        !loaded
            .database
            .apply_iroh_loro_update(board_id, "editor", "secret", "device", &update)
            .unwrap()
            .0
    );
    assert!(
        !loaded
            .database
            .apply_iroh_loro_update(board_id, "editor", "secret", "device", &[])
            .unwrap()
            .0
    );
    assert_eq!(
        loaded.database.board_sync_state(board_id).unwrap().0,
        revision
    );
    loaded
        .database
        .update_iroh_invite("editor", None, Some(false))
        .unwrap();
    assert!(loaded
        .database
        .apply_iroh_loro_update(board_id, "editor", "secret", "device", &update)
        .is_err());
    assert_eq!(
        loaded.database.board_sync_state(board_id).unwrap().0,
        revision
    );
    loaded
        .database
        .update_iroh_invite("editor", None, Some(true))
        .unwrap();
    loaded
        .database
        .connection
        .execute("DELETE FROM cardbe_loro_docs WHERE board_id=?1", [board_id])
        .unwrap();
    assert!(loaded
        .database
        .apply_iroh_loro_update(board_id, "editor", "secret", "device", &[])
        .is_err());
    assert_eq!(
        loaded
            .database
            .read_board_complete(board_id)
            .unwrap()
            .columns[0]
            .tasks[0]
            .title,
        "After"
    );
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

fn columns(count: usize) -> StoredData {
    StoredData {
        columns: (1..=count as i64)
            .map(|id| Column {
                id,
                name: format!("Column {id}"),
                color: String::new(),
                sort_order: Default::default(),
                sort_rules: Vec::new(),
                done: false,
                tasks: Vec::new(),
            })
            .collect(),
        ..StoredData::default()
    }
}

fn is_share_limit(error: Box<dyn std::error::Error>) -> bool {
    matches!(
        DomainError::repository(error),
        DomainError::ShareLimitExceeded
    )
}

#[test]
fn remote_size_bounds_reject_growth_but_keep_oversized_boards_syncing() {
    let at_limit = columns(REMOTE_MAX_COLUMNS);
    let over_limit = columns(REMOTE_MAX_COLUMNS + 1);
    assert!(validate_remote_board_size(None, &at_limit).is_ok());
    assert!(is_share_limit(
        validate_remote_board_size(None, &over_limit).unwrap_err()
    ));
    assert!(is_share_limit(
        validate_remote_board_size(Some(&at_limit), &over_limit).unwrap_err()
    ));
    // A board that grew locally past a bound may still receive changes
    // that do not enlarge it further.
    assert!(validate_remote_board_size(Some(&over_limit), &over_limit).is_ok());
    assert!(validate_remote_board_size(Some(&over_limit), &at_limit).is_ok());
    assert!(is_share_limit(
        validate_remote_board_size(Some(&over_limit), &columns(REMOTE_MAX_COLUMNS + 2))
            .unwrap_err()
    ));

    let archived = StoredData {
        archives: (0..=REMOTE_MAX_TASKS as i64)
            .map(|id| Archive {
                time: 0,
                task: Task {
                    id,
                    ..Task::default()
                },
            })
            .collect(),
        ..StoredData::default()
    };
    assert!(is_share_limit(
        validate_remote_board_size(None, &archived).unwrap_err()
    ));

    let large = StoredData {
        columns: vec![Column {
            id: 1,
            name: "x".repeat(REMOTE_MAX_CONTENT_BYTES),
            color: String::new(),
            sort_order: Default::default(),
            sort_rules: Vec::new(),
            done: false,
            tasks: Vec::new(),
        }],
        ..StoredData::default()
    };
    assert!(is_share_limit(
        validate_remote_board_size(None, &large).unwrap_err()
    ));
}

#[test]
fn prepared_editor_push_is_not_committed_over_a_newer_document() {
    let dir = test_dir("loro-stale-prepare");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let board_id = loaded.database.active_board_id();
    let base = columns(2);
    loaded
        .database
        .replace_board_as_local_edit(board_id, &base)
        .unwrap();
    loaded
        .database
        .save_iroh_invite("editor", board_id, "secret", IrohPermission::Editor)
        .unwrap();
    loaded
        .database
        .iroh_device_access("editor", "secret", "device", true)
        .unwrap();
    loaded
        .database
        .set_iroh_device_approved("editor", "device", true)
        .unwrap();
    let original = loaded.database.iroh_loro_update(board_id).unwrap().unwrap();
    let revision = loaded.database.board_sync_state(board_id).unwrap().0;
    let mut first = base.clone();
    first.columns[0].name = "First".into();
    let first = crate::loro_board::apply_local_delta(Some(&original), &base, &first).unwrap();
    let mut second = base.clone();
    second.columns[1].name = "Second".into();
    let second = crate::loro_board::apply_local_delta(Some(&original), &base, &second).unwrap();

    let stale = loaded
        .database
        .prepare_iroh_loro_update(board_id, "editor", "secret", "device", &first)
        .unwrap();
    loaded
        .database
        .apply_iroh_loro_update(board_id, "editor", "secret", "device", &second)
        .unwrap();
    let after_second = loaded.database.iroh_loro_update(board_id).unwrap().unwrap();
    assert!(loaded
        .database
        .commit_iroh_loro_update(stale, "editor", "secret", "device")
        .unwrap()
        .is_none());
    assert_eq!(
        loaded.database.iroh_loro_update(board_id).unwrap().unwrap(),
        after_second
    );
    assert_eq!(
        loaded.database.board_sync_state(board_id).unwrap().0,
        revision + 1
    );

    let (changed, pushed, _) = loaded
        .database
        .apply_iroh_loro_update(board_id, "editor", "secret", "device", &first)
        .unwrap();
    assert!(changed);
    assert_eq!(pushed, revision + 2);
    let names = loaded
        .database
        .read_board_complete(board_id)
        .unwrap()
        .columns
        .into_iter()
        .map(|column| column.name)
        .collect::<Vec<_>>();
    assert_eq!(names, ["First", "Second"]);

    // Revocation between preparation and commit wins.
    let unchanged = loaded
        .database
        .prepare_iroh_loro_update(board_id, "editor", "secret", "device", &first)
        .unwrap();
    loaded
        .database
        .update_iroh_invite("editor", None, Some(false))
        .unwrap();
    assert!(matches!(
        DomainError::repository(
            loaded
                .database
                .commit_iroh_loro_update(unchanged, "editor", "secret", "device")
                .unwrap_err()
        ),
        DomainError::PermissionDenied
    ));
    assert!(matches!(
        DomainError::repository(
            loaded
                .database
                .prepare_iroh_loro_update(board_id, "editor", "secret", "device", &first)
                .err()
                .unwrap()
        ),
        DomainError::PermissionDenied
    ));
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn iroh_share_reads_check_access_and_see_one_committed_state() {
    let dir = test_dir("iroh-share-read");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let board_id = loaded.database.active_board_id();
    let base = columns(1);
    loaded
        .database
        .replace_board_as_local_edit(board_id, &base)
        .unwrap();
    loaded
        .database
        .save_iroh_invite("editor", board_id, "secret", IrohPermission::Editor)
        .unwrap();
    loaded
        .database
        .iroh_device_access("editor", "secret", "device", true)
        .unwrap();
    let read = |db: &Database, node_id: &str, board_id: i64| {
        db.read_iroh_share(board_id, "editor", "secret", node_id, |_, _| {
            IrohShareRead {
                loro_document: true,
                data: true,
            }
        })
        .unwrap()
    };
    assert_eq!(
        read(&loaded.database, "device", board_id).err(),
        Some(IrohShareDenied::DeviceNotApproved)
    );
    loaded
        .database
        .set_iroh_device_approved("editor", "device", true)
        .unwrap();
    assert_eq!(
        read(&loaded.database, "device", board_id + 1).err(),
        Some(IrohShareDenied::InvitationUnavailable)
    );
    let contents = read(&loaded.database, "device", board_id).unwrap();
    assert_eq!(contents.permission, IrohPermission::Editor);
    assert_eq!(contents.data.unwrap().columns, base.columns);
    assert!(contents.loro_document.is_some());
    let summary = loaded
        .database
        .read_iroh_share(board_id, "editor", "secret", "device", |_, _| {
            IrohShareRead {
                loro_document: false,
                data: false,
            }
        })
        .unwrap()
        .unwrap();
    assert!(summary.loro_document.is_none() && summary.data.is_none());

    // A write committed through another connection during the read is
    // not observed by the rest of that read.
    let mut writer = Database::open(dir.join("data.sqlite3")).unwrap();
    let (before, after) = loaded
        .database
        .read_consistent(|db| {
            let before = db.read_board_complete(board_id).unwrap().columns;
            writer
                .replace_board_as_local_edit(board_id, &columns(2))
                .unwrap();
            (before, db.read_board_complete(board_id).unwrap().columns)
        })
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(
        loaded
            .database
            .read_board_complete(board_id)
            .unwrap()
            .columns,
        columns(2).columns
    );

    loaded
        .database
        .update_iroh_invite("editor", None, Some(false))
        .unwrap();
    assert_eq!(
        read(&loaded.database, "device", board_id).err(),
        Some(IrohShareDenied::DeviceNotApproved)
    );
    drop(writer);
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn oversized_editor_push_is_rejected_without_writing() {
    let dir = test_dir("loro-size-limit");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let board_id = loaded.database.active_board_id();
    let base = columns(1);
    loaded
        .database
        .replace_board_as_local_edit(board_id, &base)
        .unwrap();
    loaded
        .database
        .save_iroh_invite("editor", board_id, "secret", IrohPermission::Editor)
        .unwrap();
    loaded
        .database
        .iroh_device_access("editor", "secret", "device", true)
        .unwrap();
    loaded
        .database
        .set_iroh_device_approved("editor", "device", true)
        .unwrap();
    let original = loaded.database.iroh_loro_update(board_id).unwrap().unwrap();
    let revision = loaded.database.board_sync_state(board_id).unwrap().0;
    let update = crate::loro_board::apply_local_delta(
        Some(&original),
        &base,
        &columns(REMOTE_MAX_COLUMNS + 1),
    )
    .unwrap();
    assert!(is_share_limit(
        loaded
            .database
            .apply_iroh_loro_update(board_id, "editor", "secret", "device", &update)
            .unwrap_err()
    ));
    assert_eq!(
        loaded.database.board_sync_state(board_id).unwrap().0,
        revision
    );
    assert_eq!(
        loaded.database.iroh_loro_update(board_id).unwrap().unwrap(),
        original
    );
    assert_eq!(
        loaded
            .database
            .read_board_complete(board_id)
            .unwrap()
            .columns,
        base.columns
    );
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn offline_owner_and_editor_edits_converge_after_exchange() {
    let owner_dir = test_dir("loro-owner");
    let editor_dir = test_dir("loro-editor");
    fs::create_dir_all(&owner_dir).unwrap();
    fs::create_dir_all(&editor_dir).unwrap();
    let mut owner = load(&owner_dir).unwrap();
    let mut editor = load(&editor_dir).unwrap();
    let owner_id = owner.database.active_board_id();
    let mut base = StoredData::default();
    base.columns.push(Column {
        id: 1,
        name: "Todo".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: vec![
            Task {
                id: 2,
                title: "A".into(),
                ..Task::default()
            },
            Task {
                id: 3,
                title: "B".into(),
                ..Task::default()
            },
        ],
    });
    owner
        .database
        .replace_board_as_local_edit(owner_id, &base)
        .unwrap();
    owner
        .database
        .save_iroh_invite("invite", owner_id, "secret", IrohPermission::Editor)
        .unwrap();
    owner
        .database
        .iroh_device_access("invite", "secret", "device", true)
        .unwrap();
    owner
        .database
        .set_iroh_device_approved("invite", "device", true)
        .unwrap();
    let initial = owner.database.iroh_loro_update(owner_id).unwrap().unwrap();
    let editor_id = editor
        .database
        .create_received_board(
            "Shared",
            &base,
            IrohPermission::Editor,
            1,
            "ticket",
            Some(&initial),
        )
        .unwrap()
        .id;
    let mut owner_edit = base.clone();
    owner_edit.columns[0].tasks[0].title = "Owner changed A".into();
    owner
        .database
        .replace_board_as_local_edit(owner_id, &owner_edit)
        .unwrap();
    let mut editor_edit = base.clone();
    editor_edit.columns[0].tasks[1].title = "Editor changed B".into();
    editor
        .database
        .replace_board_as_local_edit(editor_id, &editor_edit)
        .unwrap();
    let sent = editor
        .database
        .iroh_loro_update(editor_id)
        .unwrap()
        .unwrap();
    let vector = crate::loro_board::state_vector(Some(&sent)).unwrap();
    owner
        .database
        .apply_iroh_loro_update(owner_id, "invite", "secret", "device", &sent)
        .unwrap();
    let owner_doc = owner.database.iroh_loro_update(owner_id).unwrap().unwrap();
    let diff = crate::loro_board::diff(Some(&owner_doc), &vector).unwrap();
    let revision = owner.database.board_sync_state(owner_id).unwrap().0;
    let content_changed = editor
        .database
        .merge_iroh_loro_diff(
            editor_id,
            &diff,
            "Shared",
            IrohPermission::Editor,
            revision,
            &sent,
        )
        .unwrap();
    assert!(content_changed);
    let current = editor
        .database
        .iroh_loro_update(editor_id)
        .unwrap()
        .unwrap();
    let empty_diff = crate::loro_board::diff(
        Some(&owner_doc),
        &crate::loro_board::state_vector(Some(&current)).unwrap(),
    )
    .unwrap();
    editor
        .database
        .connection
        .execute_batch(
            "CREATE TEMP TRIGGER reject_empty_sync_rewrite BEFORE DELETE ON cardbe_tasks
             BEGIN SELECT RAISE(ABORT, 'empty sync rewrote tasks'); END;
             CREATE TEMP TRIGGER reject_empty_doc_rewrite BEFORE UPDATE ON cardbe_loro_docs
             BEGIN SELECT RAISE(ABORT, 'empty sync rewrote history'); END;",
        )
        .unwrap();
    assert!(!editor
        .database
        .merge_iroh_loro_diff(
            editor_id,
            &empty_diff,
            "Shared",
            IrohPermission::Editor,
            revision,
            &current
        )
        .unwrap());
    editor
        .database
        .connection
        .execute_batch(
            "DROP TRIGGER reject_empty_sync_rewrite; DROP TRIGGER reject_empty_doc_rewrite;",
        )
        .unwrap();
    let owner_data = owner.database.read_board_complete(owner_id).unwrap();
    let editor_data = editor.database.read_board_complete(editor_id).unwrap();
    assert_eq!(owner_data.columns, editor_data.columns);
    assert_eq!(owner_data.columns[0].tasks[0].title, "Owner changed A");
    assert_eq!(owner_data.columns[0].tasks[1].title, "Editor changed B");
    assert_eq!(
        editor.database.board_sync_state(editor_id).unwrap(),
        (revision, SyncStatus::Synced)
    );
    let mut during_sync = editor_data.clone();
    during_sync.columns[0].tasks[1].color = "green".into();
    editor
        .database
        .replace_board_as_local_edit(editor_id, &during_sync)
        .unwrap();
    assert!(!editor
        .database
        .merge_iroh_loro_diff(
            editor_id,
            &empty_diff,
            "Shared",
            IrohPermission::Editor,
            revision,
            &current
        )
        .unwrap());
    assert_eq!(
        editor.database.board_sync_state(editor_id).unwrap().1,
        SyncStatus::Pending
    );
    drop(editor);
    let reopened = load(&editor_dir).unwrap();
    assert_eq!(
        reopened
            .database
            .read_board_complete(editor_id)
            .unwrap()
            .columns,
        during_sync.columns
    );
    assert!(reopened
        .database
        .iroh_loro_update(editor_id)
        .unwrap()
        .is_some());
    drop(reopened);
    drop(owner);
    fs::remove_dir_all(editor_dir).unwrap();
    fs::remove_dir_all(owner_dir).unwrap();
}

#[test]
fn received_loro_changes_are_logged_as_remote_events() {
    use crate::task_events::{EventKind::*, EventOrigin::*};
    let owner_dir = test_dir("loro-events-owner");
    let editor_dir = test_dir("loro-events-editor");
    fs::create_dir_all(&owner_dir).unwrap();
    fs::create_dir_all(&editor_dir).unwrap();
    let mut owner = load(&owner_dir).unwrap();
    let mut editor = load(&editor_dir).unwrap();
    let owner_id = owner.database.active_board_id();
    let column = |id, tasks| Column {
        id,
        name: String::new(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks,
    };
    let task = |id| Task {
        id,
        ..Task::default()
    };
    let base = StoredData {
        columns: vec![column(1, vec![task(10), task(11)]), column(2, Vec::new())],
        next_column_id: 3,
        next_task_id: 12,
        ..StoredData::default()
    };
    owner
        .database
        .replace_board_as_local_edit(owner_id, &base)
        .unwrap();
    owner
        .database
        .save_iroh_invite("invite", owner_id, "secret", IrohPermission::Editor)
        .unwrap();
    owner
        .database
        .iroh_device_access("invite", "secret", "device", true)
        .unwrap();
    owner
        .database
        .set_iroh_device_approved("invite", "device", true)
        .unwrap();
    let initial = owner.database.iroh_loro_update(owner_id).unwrap().unwrap();
    let editor_id = editor
        .database
        .create_received_board(
            "Shared",
            &base,
            IrohPermission::Editor,
            1,
            "ticket",
            Some(&initial),
        )
        .unwrap()
        .id;
    assert!(editor
        .database
        .task_events(editor_id, None)
        .unwrap()
        .is_empty());

    // The editor moves 10; the owner receives it.
    let mut editor_edit = base.clone();
    let moved = editor_edit.columns[0].tasks.remove(0);
    editor_edit.columns[1].tasks.push(moved);
    editor
        .database
        .replace_board_as_local_edit(editor_id, &editor_edit)
        .unwrap();
    let sent = editor
        .database
        .iroh_loro_update(editor_id)
        .unwrap()
        .unwrap();
    owner
        .database
        .apply_iroh_loro_update(owner_id, "invite", "secret", "device", &sent)
        .unwrap();
    // The owner moves 11; the editor receives it.
    let mut owner_edit = owner.database.read_board_complete(owner_id).unwrap();
    let moved = owner_edit.columns[0].tasks.remove(0);
    owner_edit.columns[1].tasks.push(moved);
    owner
        .database
        .replace_board_as_local_edit(owner_id, &owner_edit)
        .unwrap();
    let owner_doc = owner.database.iroh_loro_update(owner_id).unwrap().unwrap();
    let vector = crate::loro_board::state_vector(Some(&sent)).unwrap();
    let diff = crate::loro_board::diff(Some(&owner_doc), &vector).unwrap();
    let revision = owner.database.board_sync_state(owner_id).unwrap().0;
    assert!(editor
        .database
        .merge_iroh_loro_diff(
            editor_id,
            &diff,
            "Shared",
            IrohPermission::Editor,
            revision,
            &sent,
        )
        .unwrap());

    let events = |database: &Database, board| {
        database
            .task_events(board, None)
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e.origin,
                    e.kind,
                    e.task_id,
                    e.from_column_id,
                    e.to_column_id,
                )
            })
            .collect::<Vec<_>>()
    };
    let created = [
        (Local, Created, 10, None, Some(1)),
        (Local, Created, 11, None, Some(1)),
    ];
    assert_eq!(
        events(&owner.database, owner_id),
        [
            &created[..],
            &[
                (Remote, Moved, 10, Some(1), Some(2)),
                (Local, Moved, 11, Some(1), Some(2)),
            ],
        ]
        .concat()
    );
    assert_eq!(
        events(&editor.database, editor_id),
        vec![
            (Local, Moved, 10, Some(1), Some(2)),
            (Remote, Moved, 11, Some(1), Some(2)),
        ]
    );
    drop(editor);
    drop(owner);
    fs::remove_dir_all(editor_dir).unwrap();
    fs::remove_dir_all(owner_dir).unwrap();
}

#[test]
fn shared_snapshot_replaces_deleted_rows_and_preserves_global_settings() {
    let dir = test_dir("shared-snapshot-replace");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let board_id = loaded.database.active_board_id();
    let mut first = StoredData::default();
    first.columns.push(Column {
        id: 1,
        name: "Old".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: vec![Task {
            id: 1,
            title: "Remove me".into(),
            ..Task::default()
        }],
    });
    first.next_column_id = 2;
    first.next_task_id = 2;
    loaded
        .database
        .apply_iroh_snapshot(board_id, "Shared", IrohPermission::Viewer, 1, &first, None)
        .unwrap();
    assert!(loaded
        .database
        .column_overviews()
        .unwrap()
        .contains(&ColumnOverview {
            board_id,
            id: 1,
            name: "Old".into(),
            done: false,
            task_count: 1,
        }));
    let empty = StoredData::default();
    loaded
        .database
        .apply_iroh_snapshot(board_id, "Shared", IrohPermission::Viewer, 2, &empty, None)
        .unwrap();
    assert!(loaded
        .database
        .read_board_complete(board_id)
        .unwrap()
        .columns
        .is_empty());
    assert_eq!(loaded.database.boards().unwrap()[0].task_count, 0);
    assert!(loaded
        .database
        .column_overviews()
        .unwrap()
        .iter()
        .all(|column| column.board_id != board_id));
    use crate::task_events::{EventKind, EventOrigin};
    assert_eq!(
        loaded
            .database
            .task_events(board_id, None)
            .unwrap()
            .iter()
            .map(|e| (
                e.origin,
                e.kind,
                e.task_id,
                e.from_column_id,
                e.to_column_id
            ))
            .collect::<Vec<_>>(),
        vec![
            (EventOrigin::Remote, EventKind::Created, 1, None, Some(1)),
            (EventOrigin::Remote, EventKind::Deleted, 1, Some(1), None),
        ]
    );
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn startup_normalizes_an_active_viewer_board_without_granting_edit_access() {
    let dir = test_dir("viewer-startup-normalization");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let owner_id = loaded.database.active_board_id();
    let owner = loaded.database.read_board_complete(owner_id).unwrap();
    let mut shared = StoredData::default();
    shared.columns.push(Column {
        id: 100,
        name: "Shared column".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: vec![Task {
            id: 500,
            title: "Shared task".into(),
            ..Task::default()
        }],
    });
    shared.archives.push(Archive {
        time: 123,
        task: Task {
            id: 501,
            title: "Archived task".into(),
            ..Task::default()
        },
    });
    shared.templates.push(TaskTemplate {
        id: 1000,
        name: "Shared template".into(),
        task: Task::default(),
    });
    shared.settings.iroh_network.relay_enabled = false;
    let received = loaded
        .database
        .create_received_board(
            "Read only",
            &shared,
            IrohPermission::Viewer,
            7,
            "invitation",
            None,
        )
        .unwrap();
    loaded.database.load_board(received.id).unwrap();
    drop(loaded);

    let mut expected = shared;
    expected.settings = owner.settings.clone();
    let (expected, changed) = expected.migrate().unwrap();
    assert!(changed);
    let mut reopened = load(&dir).unwrap();
    let mut visible = expected.clone();
    visible.archives.clear();
    assert!(!reopened.archives_loaded);
    assert_eq!(reopened.database.active_board_id(), received.id);
    assert_eq!(reopened.stored, visible);
    assert_eq!(
        reopened.database.read_board_complete(received.id).unwrap(),
        expected
    );
    assert_eq!(
        reopened.database.read_board_complete(owner_id).unwrap(),
        owner
    );
    assert_eq!(
        reopened.database.board_role(received.id).unwrap(),
        BoardRole::Viewer
    );
    assert_eq!(
        reopened.database.board_sync_state(received.id).unwrap(),
        (7, SyncStatus::Synced)
    );
    assert_eq!(
        reopened
            .database
            .iroh_remote(received.id)
            .unwrap()
            .as_deref(),
        Some("invitation")
    );
    let mut edited = expected.clone();
    edited.columns.clear();
    assert!(reopened.database.persist_diff(&expected, &edited).is_err());
    drop(reopened);
    let reopened = load(&dir).unwrap();
    assert_eq!(reopened.stored, visible);
    assert_eq!(
        reopened.database.read_board_complete(received.id).unwrap(),
        expected
    );
    drop(reopened);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn local_edit_revision_is_atomic_and_viewer_cannot_write() {
    let dir = test_dir("shared-local-revision");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let board_id = loaded.database.active_board_id();
    let before = loaded.database.read_board_complete(board_id).unwrap();
    let mut after = before.clone();
    after.columns.push(Column {
        id: 1,
        name: "Added".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: vec![],
    });
    after.next_column_id = 2;
    loaded.database.persist_diff(&before, &after).unwrap();
    assert_eq!(loaded.database.board_sync_state(board_id).unwrap().0, 1);
    loaded
        .database
        .set_shared_board(board_id, BoardRole::Viewer, SyncStatus::Synced, 1)
        .unwrap();
    let mut blocked = after.clone();
    blocked.columns.clear();
    assert!(loaded.database.persist_diff(&after, &blocked).is_err());
    assert_eq!(loaded.database.board_sync_state(board_id).unwrap().0, 1);
    assert_eq!(
        loaded
            .database
            .read_board_complete(board_id)
            .unwrap()
            .columns
            .len(),
        1
    );
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn conflict_keeps_local_board_and_using_remote_creates_a_copy() {
    let dir = test_dir("shared-conflict-copy");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let board_id = loaded.database.active_board_id();
    let mut local = StoredData::default();
    local.columns.push(Column {
        id: 1,
        name: "Local".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: vec![],
    });
    let mut remote = StoredData::default();
    remote.columns.push(Column {
        id: 2,
        name: "Owner".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: vec![],
    });
    let local_doc =
        crate::loro_board::apply_local_delta(None, &StoredData::default(), &local).unwrap();
    let remote_doc =
        crate::loro_board::apply_local_delta(Some(&local_doc), &local, &remote).unwrap();
    loaded
        .database
        .apply_iroh_snapshot(
            board_id,
            "Shared",
            IrohPermission::Editor,
            3,
            &local,
            Some(&local_doc),
        )
        .unwrap();
    loaded
        .database
        .save_iroh_conflict(
            board_id,
            4,
            "Shared",
            IrohPermission::Editor,
            &remote,
            Some(&remote_doc),
        )
        .unwrap();
    // Simulate a pre-migration conflict, then refresh it with the owner's document.
    loaded
        .database
        .connection
        .execute(
            "UPDATE cardbe_iroh_conflicts SET loro_update=NULL WHERE board_id=?1",
            [board_id],
        )
        .unwrap();
    assert!(loaded.database.keep_iroh_conflict_local(board_id).is_err());
    assert!(loaded
        .database
        .use_iroh_conflict_remote(board_id, "Shared", &local)
        .is_err());
    assert_eq!(
        loaded
            .database
            .read_board_complete(board_id)
            .unwrap()
            .columns,
        local.columns
    );
    loaded
        .database
        .save_iroh_conflict(
            board_id,
            4,
            "Shared",
            IrohPermission::Editor,
            &remote,
            Some(&remote_doc),
        )
        .unwrap();
    let mut newer_local = local.clone();
    newer_local.columns[0].name = "Local edited after conflict".into();
    loaded.database.persist_diff(&local, &newer_local).unwrap();
    assert_eq!(
        loaded.database.board_sync_state(board_id).unwrap().1,
        SyncStatus::Conflict
    );
    assert_eq!(
        loaded
            .database
            .read_board_complete(board_id)
            .unwrap()
            .columns[0]
            .name,
        "Local edited after conflict"
    );
    drop(loaded);
    let mut loaded = load(&dir).unwrap();
    let saved = loaded.database.iroh_conflict(board_id).unwrap().unwrap();
    assert_eq!(saved.loro_update.as_deref(), Some(remote_doc.as_slice()));
    let copy = loaded
        .database
        .use_iroh_conflict_remote(board_id, "Shared", &newer_local)
        .unwrap();
    assert_eq!(
        loaded
            .database
            .read_board_complete(copy.id)
            .unwrap()
            .columns[0]
            .name,
        "Local edited after conflict"
    );
    assert_eq!(
        loaded
            .database
            .read_board_complete(board_id)
            .unwrap()
            .columns[0]
            .name,
        "Owner"
    );
    assert!(loaded.database.iroh_conflict(board_id).unwrap().is_none());
    let restored = loaded.database.iroh_loro_update(board_id).unwrap().unwrap();
    assert!(crate::loro_board::same_state_vector(&restored, &remote_doc).unwrap());
    let vector = crate::loro_board::state_vector(Some(&restored)).unwrap();
    let diff = crate::loro_board::diff(Some(&remote_doc), &vector).unwrap();
    assert!(!loaded
        .database
        .merge_iroh_loro_diff(
            board_id,
            &diff,
            "Shared",
            IrohPermission::Editor,
            4,
            &restored
        )
        .unwrap());
    assert_eq!(
        loaded
            .database
            .read_board_complete(board_id)
            .unwrap()
            .columns,
        remote.columns
    );

    let mut next = remote.clone();
    next.columns[0].name = "Next edit".into();
    loaded
        .database
        .replace_board_as_local_edit(board_id, &next)
        .unwrap();
    let upload = loaded.database.iroh_loro_update(board_id).unwrap().unwrap();
    let owner =
        crate::loro_board::project(&crate::loro_board::merge(Some(&remote_doc), &upload).unwrap())
            .unwrap();
    assert_eq!(owner.columns, next.columns);

    loaded
        .database
        .save_iroh_conflict(board_id, 5, "Shared", IrohPermission::Viewer, &remote, None)
        .unwrap();
    loaded
        .database
        .use_iroh_conflict_remote(board_id, "Shared", &next)
        .unwrap();
    assert!(loaded
        .database
        .iroh_loro_update(board_id)
        .unwrap()
        .is_none());
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn keeping_local_conflict_rebases_on_the_owner_document() {
    let dir = test_dir("shared-conflict-rebase");
    let mut loaded = load(&dir).unwrap();
    let board_id = loaded.database.active_board_id();
    let mut local = StoredData::default();
    local.columns.push(Column {
        id: 1,
        name: "Local".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: vec![],
    });
    let local_doc =
        crate::loro_board::apply_local_delta(None, &StoredData::default(), &local).unwrap();
    loaded
        .database
        .apply_iroh_snapshot(
            board_id,
            "Shared",
            IrohPermission::Editor,
            1,
            &local,
            Some(&local_doc),
        )
        .unwrap();
    let mut remote = local.clone();
    remote.columns[0].name = "Owner".into();
    remote.columns.push(Column {
        id: 2,
        name: "Owner-only".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: vec![],
    });
    let remote_doc =
        crate::loro_board::apply_local_delta(Some(&local_doc), &local, &remote).unwrap();
    loaded
        .database
        .save_iroh_conflict(
            board_id,
            2,
            "Shared",
            IrohPermission::Editor,
            &remote,
            Some(&remote_doc),
        )
        .unwrap();
    loaded.database.keep_iroh_conflict_local(board_id).unwrap();
    assert_eq!(
        loaded.database.board_sync_state(board_id).unwrap(),
        (2, SyncStatus::Pending)
    );
    assert!(loaded.database.iroh_conflict(board_id).unwrap().is_none());
    let upload = loaded.database.iroh_loro_update(board_id).unwrap().unwrap();
    let owner =
        crate::loro_board::project(&crate::loro_board::merge(Some(&remote_doc), &upload).unwrap())
            .unwrap();
    assert_eq!(owner.columns, local.columns);
    assert_eq!(
        loaded
            .database
            .read_board_complete(board_id)
            .unwrap()
            .columns,
        local.columns
    );
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

fn test_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "cardbe-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn automatic_sharing_port_persists_across_reopen() {
    let dir = test_dir("iroh-automatic-port");
    fs::create_dir_all(&dir).unwrap();
    let loaded = load(&dir).unwrap();
    let path = loaded.database.path().to_path_buf();
    assert_eq!(loaded.database.iroh_automatic_port().unwrap(), None);
    drop(loaded);
    let mut database = Database::open(path.clone()).unwrap();
    database.set_iroh_automatic_port(51_234).unwrap();
    database.set_iroh_automatic_port(51_235).unwrap();
    drop(database);
    let database = Database::open(path).unwrap();
    assert_eq!(database.iroh_automatic_port().unwrap(), Some(51_235));
    drop(database);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn notification_check_time_persists_across_reopen() {
    let dir = test_dir("notifications-checked-until");
    fs::create_dir_all(&dir).unwrap();
    let loaded = load(&dir).unwrap();
    let path = loaded.database.path().to_path_buf();
    assert_eq!(loaded.database.notifications_checked_until().unwrap(), None);
    drop(loaded);
    let mut database = Database::open(path.clone()).unwrap();
    database.set_notifications_checked_until(100).unwrap();
    database.set_notifications_checked_until(200).unwrap();
    drop(database);
    let database = Database::open(path).unwrap();
    assert_eq!(database.notifications_checked_until().unwrap(), Some(200));
    drop(database);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn due_tasks_are_found_by_due_time_without_archives() {
    let dir = test_dir("due-tasks");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let task = |id, due_time| Task {
        id,
        title: format!("Task {id}"),
        due_time,
        ..Task::default()
    };
    let mut data = StoredData::default();
    data.columns.push(Column {
        id: 5,
        name: "Todo".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: vec![
            task(1, Some(100)),
            task(2, Some(200)),
            task(3, None),
            task(4, Some(99)),
        ],
    });
    // Completed tasks are not reminded.
    data.columns.push(Column {
        id: 6,
        name: "Done".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: true,
        tasks: vec![task(7, Some(150))],
    });
    data.archives.push(crate::models::Archive {
        time: 1,
        task: task(6, Some(150)),
    });
    let board = loaded
        .database
        .create_board_with_data("Work 工作", &data)
        .unwrap();

    let due = loaded.database.tasks_due_between(100, 200).unwrap();
    assert_eq!(
        due.iter()
            .map(|due| (
                due.board_id,
                due.board_name.as_str(),
                due.column_id,
                due.task.id
            ))
            .collect::<Vec<_>>(),
        vec![(board.id, "Work 工作", 5, 1)]
    );

    // Tasks received from other members of a shared board notify too.
    data.columns.truncate(1);
    data.columns[0].tasks = vec![task(9, Some(150))];
    data.archives.clear();
    loaded
        .database
        .apply_iroh_snapshot(board.id, "Shared", IrohPermission::Viewer, 2, &data, None)
        .unwrap();
    let due = loaded.database.tasks_due_between(100, 200).unwrap();
    assert_eq!(
        due.iter()
            .map(|due| (due.board_name.as_str(), due.task.id))
            .collect::<Vec<_>>(),
        vec![("Shared", 9)]
    );
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn note_search_fuses_content_and_fuzzy_title_matches() {
    use crate::search::SearchOptions;
    let dir = test_dir("note-search");
    fs::create_dir_all(&dir).unwrap();
    let mut database = Database::open(dir.join("data.sqlite3")).unwrap();
    let mut note = |title: &str, content: &str| {
        database
            .create_note_with_content(1, title.into(), content.into())
            .unwrap()
            .id
    };
    let body = note("Weekly", "Release checklist for alpha");
    let fuzzy = note("Annual planning", "");
    let prefix = note("Release notes", "");
    let exact = note("release", "");
    let chinese = note("會議紀錄", "討論 🚀 上線時程");
    let search = |database: &Database, query: &str| {
        database
            .search_notes(query, SearchOptions::default())
            .unwrap()
    };

    assert_eq!(search(&database, "release"), vec![exact, prefix, body]);
    assert_eq!(search(&database, "anpl"), vec![fuzzy]);
    assert_eq!(search(&database, "會紀"), vec![chinese]);
    assert_eq!(search(&database, "上線時程"), vec![chinese]);
    assert_eq!(search(&database, "🚀"), vec![chinese]);
    assert_eq!(search(&database, "ch"), vec![body]);
    assert!(search(&database, "  ").is_empty());
    assert!(search(&database, "missing-xyz").is_empty());

    let mut edited = database
        .get_notes()
        .unwrap()
        .into_iter()
        .find(|note| note.id == body)
        .unwrap();
    edited.content = "Nothing here".into();
    database.update_note(&edited).unwrap();
    database.delete_note(exact).unwrap();
    assert_eq!(search(&database, "release"), vec![prefix]);
    assert!(search(&database, "checklist").is_empty());
    assert!(database
        .search_notes(
            "release",
            SearchOptions {
                candidate_limit: 0,
                ..SearchOptions::default()
            }
        )
        .is_err());

    drop(database);
    let reopened = Database::open(dir.join("data.sqlite3")).unwrap();
    assert_eq!(search(&reopened, "release"), vec![prefix]);
    drop(reopened);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn notes_are_created_updated_sorted_and_deleted() {
    let dir = test_dir("notes");
    fs::create_dir_all(&dir).unwrap();
    let mut database = Database::open(dir.join("data.sqlite3")).unwrap();

    let older = database.create_note(100).unwrap();
    let newer = database
        .create_note_with_content(200, "Captured".into(), "From quick add".into())
        .unwrap();
    assert_eq!(newer.title, "Captured");
    assert_eq!(newer.content, "From quick add");
    let mut pinned = older.clone();
    pinned.title = "Pinned idea".into();
    pinned.content = "Keep this at the top".into();
    pinned.pinned = true;
    pinned.updated_at = 300;
    database.update_note(&pinned).unwrap();

    let notes = database.get_notes().unwrap();
    assert_eq!(notes, vec![pinned.clone(), newer.clone()]);

    database.delete_note(pinned.id).unwrap();
    assert_eq!(database.get_notes().unwrap(), vec![newer]);
    for error in [
        database.delete_note(pinned.id).unwrap_err(),
        database.update_note(&pinned).unwrap_err(),
    ] {
        assert_eq!(
            serde_json::to_value(crate::errors::CommandError::repository(error)).unwrap(),
            serde_json::json!({"code": "NOTE_NOT_FOUND"})
        );
    }
    database.initialize_boards(&StoredData::default()).unwrap();
    assert_eq!(
        serde_json::to_value(crate::errors::CommandError::repository(
            database
                .update_iroh_invite("missing", None, Some(false))
                .unwrap_err()
        ))
        .unwrap(),
        serde_json::json!({"code": "INVITE_NOT_FOUND"})
    );
    assert_eq!(
        serde_json::to_value(crate::errors::CommandError::repository(
            database
                .set_iroh_device_approved("missing", "device", true)
                .unwrap_err()
        ))
        .unwrap(),
        serde_json::json!({"code": "DEVICE_REQUEST_NOT_FOUND"})
    );
    drop(database);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn migrates_json_and_preserves_the_source() {
    let dir = test_dir("sqlite-migration");
    fs::create_dir_all(&dir).unwrap();
    let mut stored = StoredData::default();
    stored.columns.push(Column {
        id: 0,
        name: "Todo".into(),
        color: String::new(),
        sort_order: ColumnSort::DueDateDesc,
        sort_rules: Vec::new(),
        done: false,
        tasks: vec![Task {
            id: 0,
            title: "Migrated".into(),
            ..Task::default()
        }],
    });
    stored.next_column_id = 1;
    stored.next_task_id = 1;
    write_document(&dir.join("data.json"), &stored).unwrap();
    // Data written before sort rules existed keeps its legacy sort.
    stored.columns[0].reconcile_sort();
    assert_eq!(stored.columns[0].sort_rules.len(), 1);

    let loaded = load(&dir).unwrap();
    assert_eq!(loaded.stored, stored);
    assert_eq!(loaded.database.boards().unwrap().len(), 1);
    assert_eq!(loaded.database.boards().unwrap()[0].name, "My board");
    assert!(dir.join("data.sqlite3").exists());
    assert!(dir.join("data.migrated.json").exists());
    drop(loaded);

    let reloaded = load(&dir).unwrap();
    assert_eq!(reloaded.stored, stored);
    drop(reloaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn migrates_legacy_sqlite_before_persisting_schema_changes() {
    let dir = test_dir("sqlite-schema-migration");
    fs::create_dir_all(&dir).unwrap();
    let database_path = dir.join("data.sqlite3");
    let legacy = StoredData {
        schema_version: CURRENT_SCHEMA_VERSION - 1,
        columns: vec![Column {
            id: 0,
            name: "Legacy".into(),
            color: String::new(),
            sort_order: Default::default(),
            sort_rules: Vec::new(),
            done: false,
            tasks: Vec::new(),
        }],
        next_column_id: 1,
        ..Default::default()
    };

    let mut database = Database::open(database_path).unwrap();
    database.replace_all(&legacy).unwrap();
    drop(database);

    let loaded = load(&dir).unwrap();
    assert_ne!(loaded.database.active_board_id(), 0);
    assert_eq!(loaded.database.boards().unwrap().len(), 1);
    assert_eq!(loaded.stored.columns[0].name, "Legacy");
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn incomplete_database_creation_retries_json_migration() {
    let dir = test_dir("interrupted-migration");
    fs::create_dir_all(&dir).unwrap();
    let mut stored = StoredData::default();
    stored.columns.push(Column {
        id: 0,
        name: "Still here".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: Vec::new(),
    });
    stored.next_column_id = 1;
    write_document(&dir.join("data.json"), &stored).unwrap();

    // Opening creates the SQLite file and schema, but intentionally does
    // not commit imported application data or the initialization marker.
    drop(Database::open(dir.join("data.sqlite3")).unwrap());

    let loaded = load(&dir).unwrap();
    assert_eq!(loaded.stored, stored);
    assert!(loaded.database.is_initialized().unwrap());
    assert!(dir.join("data.migrated.json").exists());
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn failed_json_migration_rolls_back_and_can_be_retried() {
    let dir = test_dir("failed-migration-retry");
    fs::create_dir_all(&dir).unwrap();
    let mut stored = StoredData::default();
    stored.columns.push(Column {
        id: 0,
        name: "Must survive".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: Vec::new(),
    });
    stored.next_column_id = 1;
    write_document(&dir.join("data.json"), &stored).unwrap();

    let database_path = dir.join("data.sqlite3");
    let database = Database::open(database_path.clone()).unwrap();
    database
        .connection
        .execute_batch(
            "CREATE TRIGGER reject_migration
                 BEFORE INSERT ON columns
                 BEGIN
                   SELECT RAISE(ABORT, 'simulated migration failure');
                 END;",
        )
        .unwrap();
    drop(database);

    let error = match load(&dir) {
        Ok(_) => panic!("migration should fail"),
        Err(error) => error,
    };
    assert!(error
        .to_string()
        .contains("source files were left unchanged"));
    assert!(dir.join("data.json").exists());
    assert!(!dir.join("data.migrated.json").exists());
    let database = Database::open(database_path).unwrap();
    assert!(!database.is_initialized().unwrap());
    assert!(database.load().unwrap().columns.is_empty());
    database
        .connection
        .execute_batch("DROP TRIGGER reject_migration;")
        .unwrap();
    drop(database);

    let loaded = load(&dir).unwrap();
    assert_eq!(loaded.stored, stored);
    assert!(loaded.database.is_initialized().unwrap());
    assert!(dir.join("data.migrated.json").exists());
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn updating_one_task_only_persists_that_task() {
    let dir = test_dir("partial-update");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let mut before = StoredData::default();
    before.columns.push(Column {
        id: 0,
        name: "Todo".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: vec![
            Task {
                id: 0,
                title: "First".into(),
                ..Task::default()
            },
            Task {
                id: 1,
                title: "Second".into(),
                ..Task::default()
            },
        ],
    });
    before.next_column_id = 1;
    before.next_task_id = 2;
    loaded.database.replace_all(&before).unwrap();
    let mut after = before.clone();
    after.columns[0].tasks[1].title = "Changed".into();

    let stats = loaded.database.persist_diff(&before, &after).unwrap();
    assert_eq!(stats.tasks_changed, 1);
    assert_eq!(stats.columns_changed, 0);
    assert_eq!(stats.metadata_changed, 0);
    assert_eq!(loaded.database.load().unwrap(), after);
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn moving_a_task_only_updates_the_moved_row() {
    let dir = test_dir("sparse-task-move");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let mut before = StoredData::default();
    before.columns.push(Column {
        id: 0,
        name: "Todo".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: (0..5)
            .map(|id| Task {
                id,
                title: format!("Task {id}"),
                ..Task::default()
            })
            .collect(),
    });
    before.next_column_id = 1;
    before.next_task_id = 5;
    loaded.database.replace_all(&before).unwrap();

    let mut after = before.clone();
    let moved = after.columns[0].tasks.remove(0);
    after.columns[0].tasks.push(moved);
    let stats = loaded.database.persist_diff(&before, &after).unwrap();

    assert_eq!(stats.tasks_changed, 1);
    assert_eq!(loaded.database.load().unwrap(), after);
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn database_uses_normal_synchronous_mode() {
    let dir = test_dir("normal-synchronous");
    fs::create_dir_all(&dir).unwrap();
    let database = Database::open(dir.join("data.sqlite3")).unwrap();
    let synchronous: i64 = database
        .connection
        .query_row("PRAGMA synchronous", [], |row| row.get(0))
        .unwrap();
    assert_eq!(synchronous, 1);
    drop(database);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn conflicting_text_boards_table_is_preserved_and_ignored() {
    let dir = test_dir("foreign-boards-table");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("data.sqlite3");
    {
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE boards(id TEXT PRIMARY KEY, name TEXT NOT NULL);
                 INSERT INTO boards(id,name) VALUES ('foreign-id','Foreign board');",
            )
            .unwrap();
    }
    let loaded = load(&dir).unwrap();
    assert_eq!(loaded.database.boards().unwrap().len(), 1);
    let foreign: (String, String) = loaded
        .database
        .connection
        .query_row("SELECT id,name FROM boards", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(foreign, ("foreign-id".into(), "Foreign board".into()));
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn archives_are_loaded_on_demand_after_initialization() {
    let dir = test_dir("lazy-archives");
    fs::create_dir_all(&dir).unwrap();
    let mut first = load(&dir).unwrap();
    let before = first.stored.clone();
    let mut after = before.clone();
    after.archives.push(Archive {
        time: 123,
        task: Task {
            id: 42,
            title: "Archived".into(),
            ..Task::default()
        },
    });
    after.next_task_id = 43;
    first.database.persist_diff(&before, &after).unwrap();
    drop(first);

    let mut second = load(&dir).unwrap();
    assert!(!second.archives_loaded);
    assert!(second.stored.archives.is_empty());
    assert_eq!(second.database.load_archives().unwrap(), after.archives);
    // Legacy databases can have archived cards but no CRDT document yet.
    second
        .database
        .connection
        .execute("DELETE FROM cardbe_loro_docs", [])
        .unwrap();
    let mut edited = second.stored.clone();
    edited.columns.push(Column {
        id: 1,
        name: "Edited".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: Vec::new(),
    });
    second
        .database
        .persist_diff_without_archives(&second.stored, &edited)
        .unwrap();
    let update = second
        .database
        .ensure_iroh_loro_doc(second.database.active_board_id())
        .unwrap();
    assert_eq!(
        crate::loro_board::project(&update).unwrap().archives,
        after.archives
    );
    assert_eq!(second.database.load_archives().unwrap(), after.archives);
    // Existing documents must not read archived payloads during ordinary edits.
    // Valid JSON with an invalid task shape makes accidental hydration fail.
    second
        .database
        .connection
        .execute("UPDATE cardbe_archives SET payload='null'", [])
        .unwrap();
    let mut edited_again = edited.clone();
    edited_again.columns[0].name = "Another edit".into();
    second
        .database
        .persist_diff_without_archives(&edited, &edited_again)
        .unwrap();
    let update = second
        .database
        .ensure_iroh_loro_doc(second.database.active_board_id())
        .unwrap();
    let projected = crate::loro_board::project(&update).unwrap();
    assert_eq!(projected.archives, after.archives);
    assert_eq!(projected.columns, edited_again.columns);
    drop(second);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn failed_transaction_keeps_the_previous_complete_state() {
    let dir = test_dir("transaction-rollback");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let mut before = StoredData::default();
    before.columns.push(Column {
        id: 0,
        name: "Todo".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: vec![Task {
            id: 0,
            title: "Safe".into(),
            ..Task::default()
        }],
    });
    before.next_column_id = 1;
    before.next_task_id = 1;
    loaded.database.replace_all(&before).unwrap();
    loaded
        .database
        .connection
        .execute_batch(
            "CREATE TRIGGER reject_task_update
                 BEFORE UPDATE OF payload ON cardbe_tasks
                 BEGIN
                   SELECT RAISE(ABORT, 'simulated write failure');
                 END;",
        )
        .unwrap();

    let mut after = before.clone();
    after.columns[0].tasks[0].title = "Must roll back".into();
    assert!(loaded.database.persist_diff(&before, &after).is_err());
    assert_eq!(loaded.database.load().unwrap(), before);
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn corrupt_json_recovers_from_backup_before_migration() {
    let dir = test_dir("backup-migration");
    fs::create_dir_all(&dir).unwrap();
    let mut stored = StoredData::default();
    stored.columns.push(Column {
        id: 0,
        name: "Recovered".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: Vec::new(),
    });
    write_document(&backup_path(&dir.join("data.json")), &stored).unwrap();
    fs::write(dir.join("data.json"), "not json").unwrap();

    let loaded = load(&dir).unwrap();
    assert_eq!(loaded.stored.columns[0].name, "Recovered");
    assert!(!loaded.recovery_messages.is_empty());
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn corrupt_current_json_does_not_resurrect_stale_legacy_data() {
    let dir = test_dir("no-stale-legacy");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("data.json"), "not json").unwrap();
    fs::write(
        dir.join("tasks.json"),
        r#"[{"name":"Stale","color":"","tasks":[]}]"#,
    )
    .unwrap();

    let loaded = load(&dir).unwrap();
    assert!(loaded.stored.columns.is_empty());
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn boards_isolate_same_ids_and_restore_active_board() {
    let dir = test_dir("board-isolation");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let first_id = loaded.database.active_board_id();
    let second = loaded.database.create_board("Second").unwrap();

    let first = loaded.stored.clone();
    let mut first_changed = first.clone();
    first_changed.columns.push(Column {
        id: 0,
        name: "First".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: vec![Task {
            id: 0,
            title: "Only first".into(),
            ..Task::default()
        }],
    });
    first_changed.next_column_id = 1;
    first_changed.next_task_id = 1;
    loaded
        .database
        .persist_diff(&first, &first_changed)
        .unwrap();

    let second_before = loaded.database.load_board(second.id).unwrap();
    let mut second_changed = second_before.clone();
    second_changed.columns.push(Column {
        id: 0,
        name: "Second".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: vec![Task {
            id: 0,
            title: "Only second".into(),
            ..Task::default()
        }],
    });
    second_changed.next_column_id = 1;
    second_changed.archives.push(Archive {
        time: 456,
        task: Task {
            id: 1,
            title: "Archived second".into(),
            ..Task::default()
        },
    });
    second_changed.next_task_id = 2;
    loaded
        .database
        .persist_diff(&second_before, &second_changed)
        .unwrap();

    let summaries = loaded.database.boards().unwrap();
    assert_eq!(summaries.len(), 2);
    assert_eq!(summaries[0].task_count, 1);
    assert_eq!(summaries[1].task_count, 1);

    let first_again = loaded.database.load_board(first_id).unwrap();
    assert_eq!(first_again.columns[0].name, "First");
    assert_eq!(first_again.columns[0].tasks[0].title, "Only first");
    drop(loaded);

    let reopened = load(&dir).unwrap();
    assert_eq!(reopened.database.active_board_id(), first_id);
    assert_eq!(reopened.stored.columns[0].name, "First");
    drop(reopened);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cross_board_template_copy_preserves_source_and_syncs_independent_copy() {
    let dir = test_dir("cross-board-template-copy");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let mut data = StoredData::default();
    data.templates.push(TaskTemplate {
        id: 0,
        name: "Template".into(),
        task: Task {
            title: "Keep content 中文".into(),
            description: "Details".into(),
            labels: vec!["label".into()],
            ..Task::default()
        },
    });
    data.next_template_id = 1;
    let source = loaded
        .database
        .create_board_with_data("Source", &data)
        .unwrap()
        .id;
    let target = loaded
        .database
        .create_board_with_data("Target", &data)
        .unwrap()
        .id;
    for (from, template, to, code) in [
        (source, 0, source, "INVALID_ARGUMENT"),
        (source, 99, target, "TEMPLATE_NOT_FOUND"),
        (source, 0, -1, "BOARD_NOT_FOUND"),
        (-1, 0, target, "BOARD_NOT_FOUND"),
    ] {
        let error = loaded
            .database
            .copy_task_template_to_board(from, template, to)
            .unwrap_err();
        assert_eq!(
            serde_json::to_value(crate::errors::CommandError::repository(error)).unwrap()["code"],
            code
        );
    }
    loaded
        .database
        .connection
        .execute(
            "UPDATE cardbe_boards SET shared_role='viewer' WHERE id=?1",
            [target],
        )
        .unwrap();
    let error = loaded
        .database
        .copy_task_template_to_board(source, 0, target)
        .unwrap_err();
    assert_eq!(
        serde_json::to_value(crate::errors::CommandError::repository(error)).unwrap()["code"],
        "PERMISSION_DENIED"
    );
    assert_eq!(
        loaded
            .database
            .read_board_complete(target)
            .unwrap()
            .templates,
        data.templates
    );
    loaded
        .database
        .connection
        .execute(
            "UPDATE cardbe_boards SET shared_role='editor' WHERE id=?1",
            [target],
        )
        .unwrap();
    // Reading a viewer board is allowed; only the destination is modified.
    loaded
        .database
        .connection
        .execute(
            "UPDATE cardbe_boards SET shared_role='viewer' WHERE id=?1",
            [source],
        )
        .unwrap();
    loaded
        .database
        .copy_task_template_to_board(source, 0, target)
        .unwrap();
    assert_eq!(
        loaded
            .database
            .read_board_complete(source)
            .unwrap()
            .templates,
        data.templates
    );
    let copied = loaded.database.read_board_complete(target).unwrap();
    let mut expected = data.templates[0].clone();
    expected.id = copied.templates[1].id;
    assert_ne!(expected.id, data.templates[0].id);
    assert_eq!(copied.templates, vec![data.templates[0].clone(), expected]);
    let update = loaded.database.iroh_loro_update(target).unwrap().unwrap();
    assert_eq!(
        crate::loro_board::project(&update).unwrap().templates,
        copied.templates
    );
    drop(loaded);
    let loaded = load(&dir).unwrap();
    assert_eq!(
        loaded
            .database
            .read_board_complete(target)
            .unwrap()
            .templates,
        copied.templates
    );
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cross_board_batch_move_keeps_board_order_or_moves_nothing() {
    let dir = test_dir("cross-board-batch-move");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let task = |id, title: &str| Task {
        id,
        title: title.into(),
        ..Task::default()
    };
    let column = |id, tasks| Column {
        id,
        name: "Tasks".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks,
    };
    let data = StoredData {
        columns: vec![
            column(
                0,
                vec![
                    Task {
                        blocked_by: vec![1],
                        ..task(0, "A")
                    },
                    task(1, "B"),
                ],
            ),
            column(1, vec![task(2, "C")]),
        ],
        next_column_id: 2,
        next_task_id: 3,
        ..StoredData::default()
    };
    let source = loaded
        .database
        .create_board_with_data("Source", &data)
        .unwrap()
        .id;
    let target = loaded
        .database
        .create_board_with_data("Target", &data)
        .unwrap()
        .id;
    assert!(loaded
        .database
        .move_tasks_to_board(source, &[0, 99], target, 1)
        .is_err());
    assert_eq!(
        loaded.database.read_board_complete(source).unwrap().columns,
        data.columns
    );

    let remaining = loaded
        .database
        .move_tasks_to_board(source, &[2, 0], target, 1)
        .unwrap();
    let titles = |columns: &[Column]| -> Vec<Vec<String>> {
        columns
            .iter()
            .map(|column| column.tasks.iter().map(|t| t.title.clone()).collect())
            .collect()
    };
    assert_eq!(titles(&remaining.columns), vec![vec!["B"], vec![]]);
    let moved = loaded.database.read_board_complete(target).unwrap();
    assert_eq!(
        titles(&moved.columns),
        vec![vec!["A", "B"], vec!["C", "A", "C"]]
    );
    // Waiting on a task of the source board means nothing on the target.
    assert!(moved.columns[1].tasks[1].blocked_by.is_empty());
    let new_ids = [moved.columns[1].tasks[1].id, moved.columns[1].tasks[2].id];
    assert!(new_ids[0] != new_ids[1] && new_ids.iter().all(|id| *id > 2));
    use crate::task_events::EventKind::{TransferredIn, TransferredOut};
    let events = |database: &Database, board| {
        database
            .task_events(board, None)
            .unwrap()
            .iter()
            .map(|e| (e.kind, e.task_id, e.from_column_id, e.to_column_id))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        events(&loaded.database, source),
        vec![
            (TransferredOut, 0, Some(0), None),
            (TransferredOut, 2, Some(1), None),
        ]
    );
    assert_eq!(
        events(&loaded.database, target),
        vec![
            (TransferredIn, new_ids[0], None, Some(1)),
            (TransferredIn, new_ids[1], None, Some(1)),
        ]
    );
    // The log belongs to its board and is removed with it.
    loaded.database.delete_board(source).unwrap();
    let remaining_ops: i64 = loaded
        .database
        .connection
        .query_row(
            "SELECT COUNT(*) FROM cardbe_task_event_ops WHERE board_id=?1",
            [source],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(remaining_ops, 0);
    assert_eq!(events(&loaded.database, target).len(), 2);
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cross_board_move_preserves_task_and_rejects_read_only_boards() {
    let dir = test_dir("cross-board-move");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let mut data = StoredData::default();
    let task = Task {
        id: 0,
        title: "Move me".into(),
        description: "Keep content".into(),
        labels: vec!["label".into()],
        ..Task::default()
    };
    data.columns.push(Column {
        id: 0,
        name: "Tasks".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: vec![task.clone()],
    });
    data.next_column_id = 1;
    data.next_task_id = 1;
    let source = loaded
        .database
        .create_board_with_data("Source", &data)
        .unwrap()
        .id;
    let target = loaded
        .database
        .create_board_with_data("Target", &data)
        .unwrap()
        .id;
    assert!(loaded
        .database
        .move_tasks_to_board(source, &[0], target, 99)
        .is_err());
    for id in [target, source] {
        loaded
            .database
            .connection
            .execute(
                "UPDATE cardbe_boards SET shared_role='viewer' WHERE id=?1",
                [id],
            )
            .unwrap();
        assert!(loaded
            .database
            .move_tasks_to_board(source, &[0], target, 0)
            .is_err());
        assert_eq!(
            loaded.database.read_board_complete(source).unwrap().columns[0].tasks,
            vec![task.clone()]
        );
        assert_eq!(
            loaded.database.read_board_complete(target).unwrap().columns[0].tasks,
            vec![task.clone()]
        );
        loaded
            .database
            .connection
            .execute(
                "UPDATE cardbe_boards SET shared_role='owner' WHERE id=?1",
                [id],
            )
            .unwrap();
    }
    loaded
        .database
        .move_tasks_to_board(source, &[0], target, 0)
        .unwrap();
    assert!(
        loaded.database.read_board_complete(source).unwrap().columns[0]
            .tasks
            .is_empty()
    );
    let moved = loaded.database.read_board_complete(target).unwrap();
    let mut expected = task.clone();
    expected.id = moved.columns[0].tasks[1].id;
    assert_ne!(expected.id, task.id);
    assert_eq!(moved.columns[0].tasks, vec![task, expected]);
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn done_columns_and_completion_times_persist() {
    let dir = test_dir("done-column");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let board_id = loaded.database.active_board_id();
    let mut data = StoredData::default();
    data.columns.push(Column {
        id: 1,
        name: "Done".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: true,
        tasks: vec![Task {
            id: 2,
            completed_at: Some(1_800_000_000_000),
            ..Task::default()
        }],
    });
    loaded
        .database
        .replace_board_as_local_edit(board_id, &data)
        .unwrap();
    let read = loaded.database.read_board_complete(board_id).unwrap();
    assert_eq!(read.columns, data.columns);

    data.columns[0].done = false;
    data.columns[0].tasks[0].completed_at = None;
    loaded
        .database
        .replace_board_as_local_edit(board_id, &data)
        .unwrap();
    let read = loaded.database.read_board_complete(board_id).unwrap();
    assert_eq!(read.columns, data.columns);
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn replace_all_boards_round_trips_same_logical_ids_and_global_data() {
    let dir = test_dir("all-board-round-trip");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();

    let mut first = StoredData::default();
    first.columns.push(Column {
        id: 0,
        name: "First column".into(),
        color: String::new(),
        sort_order: Default::default(),
        sort_rules: Vec::new(),
        done: false,
        tasks: vec![Task {
            id: 0,
            title: "First task".into(),
            pinned: true,
            ..Task::default()
        }],
    });
    first.next_column_id = 1;
    first.next_task_id = 1;
    let mut second = first.clone();
    second.columns[0].name = "Second column".into();
    second.columns[0].tasks[0].title = "Second task".into();
    let settings = crate::models::Settings {
        theme: Some("morandi-dark-sage".into()),
        iroh_network: crate::models::IrohNetworkSettings {
            enabled: true,
            ..Default::default()
        },
        language: crate::models::LanguagePreference::TraditionalChinese,
        notify_enabled: true,
        global_shortcuts_enabled: false,
        ..Default::default()
    };
    let notes = vec![Note {
        id: 88,
        title: "Global note".into(),
        content: "Preserved across boards".into(),
        pinned: true,
        created_at: 1,
        updated_at: 2,
    }];

    let (boards, active) = loaded
        .database
        .replace_all_boards(
            &[
                ("First".into(), first, Default::default()),
                ("Second".into(), second, Default::default()),
            ],
            1,
            &notes,
            &settings,
            Some(&[crate::focuses::Focus {
                id: "second".into(),
                name: "Second board".into(),
                filter: crate::focuses::FocusFilter {
                    board_ids: Some(vec![1]),
                    ..Default::default()
                },
                sort: Default::default(),
                group: Default::default(),
            }]),
        )
        .unwrap();

    assert_eq!(boards.len(), 2);
    // Backup positions become the restored boards' IDs.
    let views = crate::focuses::load(&loaded.database.connection)
        .unwrap()
        .unwrap();
    assert_eq!(views[0].filter.board_ids, Some(vec![boards[1].id]));
    assert_eq!(active.columns[0].tasks[0].title, "Second task");
    assert_eq!(loaded.database.active_board_id(), boards[1].id);
    assert_eq!(loaded.database.get_notes().unwrap(), notes);
    let restored_first = loaded.database.read_board(boards[0].id).unwrap();
    let restored_second = loaded.database.read_board(boards[1].id).unwrap();
    assert_eq!(restored_first.columns[0].id, restored_second.columns[0].id);
    assert_eq!(
        restored_first.columns[0].tasks[0].id,
        restored_second.columns[0].tasks[0].id
    );
    assert_eq!(restored_first.columns[0].tasks[0].title, "First task");
    assert!(restored_first.columns[0].tasks[0].pinned);
    assert_eq!(restored_second.columns[0].tasks[0].title, "Second task");
    assert_eq!(restored_first.settings, settings);

    drop(loaded);
    let reopened = load(&dir).unwrap();
    assert_eq!(reopened.database.active_board_id(), boards[1].id);
    assert_eq!(reopened.stored.columns[0].tasks[0].title, "Second task");
    assert!(reopened.stored.columns[0].tasks[0].pinned);
    assert_eq!(reopened.database.get_notes().unwrap(), notes);
    assert_eq!(reopened.stored.settings, settings);
    drop(reopened);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn replace_all_boards_rolls_back_everything_when_notes_violate_a_constraint() {
    let dir = test_dir("all-board-rollback");
    fs::create_dir_all(&dir).unwrap();
    let mut loaded = load(&dir).unwrap();
    let original_id = loaded.database.active_board_id();
    let original = loaded.database.read_board(original_id).unwrap();
    let original_note = loaded
        .database
        .create_note_with_content(1, "Original".into(), "Keep me".into())
        .unwrap();
    let invalid_notes = vec![original_note.clone(), original_note.clone()];

    let result = loaded.database.replace_all_boards(
        &[(
            "Replacement".into(),
            StoredData::default(),
            Default::default(),
        )],
        0,
        &invalid_notes,
        &crate::models::Settings {
            language: crate::models::LanguagePreference::English,
            notify_enabled: true,
            global_shortcuts_enabled: false,
            ..Default::default()
        },
        None,
    );

    assert!(result.is_err());
    assert_eq!(loaded.database.active_board_id(), original_id);
    assert_eq!(loaded.database.boards().unwrap().len(), 1);
    assert_eq!(loaded.database.read_board(original_id).unwrap(), original);
    assert_eq!(loaded.database.get_notes().unwrap(), vec![original_note]);
    drop(loaded);
    fs::remove_dir_all(dir).unwrap();
}
