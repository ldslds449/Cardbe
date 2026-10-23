use super::*;
use crate::{errors::CommandError, models::*, state::AppData};
use serde_json::json;
use std::sync::Mutex;

/// An isolated temporary database standing in for the desktop application.
pub(crate) struct TestHost {
    pub app: SharedAppData,
    pub changed: Mutex<Vec<i64>>,
    pub board_id: i64,
    pub other_board_id: i64,
    pub created: idempotency::CreatedTasks,
    // Declared last so the database closes before the directory is removed.
    _dir: TempDir,
}

struct TempDir(std::path::PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        if let Ok(canonical) = self.0.canonicalize() {
            if canonical.starts_with(std::env::temp_dir().canonicalize().unwrap()) {
                let _ = std::fs::remove_dir_all(canonical);
            }
        }
    }
}

impl OperationHost for TestHost {
    fn app_data(&self) -> Result<&SharedAppData, DomainError> {
        Ok(&self.app)
    }
    fn created_tasks(&self) -> Result<&idempotency::CreatedTasks, DomainError> {
        Ok(&self.created)
    }
    fn board_changed(&self, board_id: i64) {
        self.changed.lock().unwrap().push(board_id);
    }
}

fn column(id: i64, name: &str, tasks: Vec<Task>) -> Column {
    Column {
        id,
        name: name.into(),
        color: String::new(),
        sort_order: ColumnSort::Custom,
        sort_rules: Vec::new(),
        tasks,
    }
}

impl TestHost {
    pub fn new() -> Self {
        let dir =
            std::env::temp_dir().join(format!("cardbe-operations-{}", crate::plugins::types::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut loaded = crate::storage::load(&dir).unwrap();
        let board_id = loaded.database.active_board_id();
        let data = StoredData {
            columns: vec![
                column(
                    1,
                    "待辦 😀",
                    vec![
                        Task {
                            id: 10,
                            title: "Write report".into(),
                            description: "季度報告".into(),
                            created_time: 1_700_000_000_000,
                            ..Task::default()
                        },
                        Task {
                            id: 11,
                            title: "Legacy".into(),
                            // Saved before creation times: start_time is the creation.
                            start_time: 1_600_000_000_000,
                            ..Task::default()
                        },
                    ],
                ),
                column(2, "Done", Vec::new()),
            ],
            ..StoredData::default()
        };
        loaded
            .database
            .replace_board_as_local_edit(board_id, &data)
            .unwrap();
        let other_board_id = loaded
            .database
            .create_board_with_data(
                "Other",
                &StoredData {
                    columns: vec![column(1, "Inbox", Vec::new())],
                    ..StoredData::default()
                },
            )
            .unwrap()
            .id;
        let mut app = AppData::new(
            loaded.stored,
            loaded.database,
            loaded.recovery_messages,
            loaded.archives_loaded,
        )
        .unwrap();
        app.load_active_board(board_id).unwrap();
        Self {
            app: Mutex::new(app),
            changed: Mutex::new(Vec::new()),
            board_id,
            other_board_id,
            created: Default::default(),
            _dir: TempDir(dir),
        }
    }
}

pub(crate) fn context(client: &str, boards: &[i64], write: bool) -> OperationContext {
    OperationContext::new(
        Caller::Mcp {
            client_id: client.into(),
        },
        Grants {
            all_boards: false,
            boards: boards.iter().copied().collect(),
            write,
        },
    )
}

fn call(
    host: &TestHost,
    context: &OperationContext,
    name: &str,
    arguments: serde_json::Value,
) -> Result<serde_json::Value, OperationError> {
    dispatch(host, context, name, arguments)
}

fn domain(error: OperationError) -> serde_json::Value {
    match error {
        OperationError::Domain(error) => serde_json::to_value(CommandError::from(error)).unwrap(),
        other => panic!("expected a domain error, got {other:?}"),
    }
}

#[test]
fn registry_names_are_unique_valid_tool_names_with_object_schemas() {
    let mut names = std::collections::HashSet::new();
    for operation in &OPERATIONS {
        assert!(names.insert(operation.name), "{}", operation.name);
        assert!(operation
            .name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b == b'.' || b == b'_'));
        assert!(!operation.description.is_empty());
        for schema in [(operation.input_schema)(), (operation.output_schema)()] {
            assert_eq!(schema["type"], "object", "{}", operation.name);
        }
    }
    let create = (OPERATIONS
        .iter()
        .find(|operation| operation.name == "task.create")
        .unwrap()
        .input_schema)();
    let required = create["required"].as_array().unwrap();
    for field in ["board_id", "column_id", "title"] {
        assert!(required.contains(&json!(field)));
    }
    assert!(!required.contains(&json!("description")));
    assert_eq!(create["additionalProperties"], false);
}

#[test]
fn registry_writes_are_visible_to_plugin_reads_and_share_their_validation() {
    let host = TestHost::new();
    let writer = context("writer", &[host.board_id], true);
    let created = call(
        &host,
        &writer,
        "task.create",
        json!({"board_id": host.board_id, "column_id": 2, "title": "Plan 🚀",
               "labels": ["工作"], "items": [{"text": "Draft"}], "due_time": 1_800_000_000_000u64}),
    )
    .unwrap();
    let task = &created["task"];
    assert_eq!(task["column_id"], 2);
    assert_eq!(task["archived"], false);
    assert!(task["created_time"].as_u64().unwrap() > 1_700_000_000_000);
    assert_eq!(task["items"][0]["id"].as_str().unwrap().len(), 36);

    // The plugin WIT read path observes the same committed task.
    let path = host.app.lock().unwrap().database.path().to_path_buf();
    let reader = crate::plugins::board::Reader::open(path, host.board_id).unwrap();
    let stored = reader
        .get_task(task["id"].as_i64().unwrap(), false)
        .unwrap();
    assert_eq!(stored.task.title, "Plan 🚀");
    assert_eq!(stored.column_id, Some(2));

    // Both adapters reject the same invalid task through the shared validation.
    assert!(matches!(
        call(
            &host,
            &writer,
            "task.create",
            json!({"board_id": host.board_id, "column_id": 2, "title": "  "})
        ),
        Err(OperationError::InvalidInput)
    ));
    let mut data = host
        .app
        .lock()
        .unwrap()
        .database
        .read_board_complete(host.board_id)
        .unwrap();
    assert!(matches!(
        tasks::create_task(
            &mut data,
            2,
            Task {
                title: "  ".into(),
                ..Task::default()
            },
            1
        ),
        Err(DomainError::InvalidArgument)
    ));
}

#[test]
fn writes_install_into_the_open_board_without_entering_undo() {
    let host = TestHost::new();
    host.app
        .lock()
        .unwrap()
        .undo_history
        .push(StoredData::default().into());
    let writer = context("writer", &[host.board_id], true);
    call(
        &host,
        &writer,
        "task.update",
        json!({"board_id": host.board_id, "task_id": 10, "title": "Final report",
               "due_time": 1_800_000_000_000u64}),
    )
    .unwrap();
    {
        let app = host.app.lock().unwrap();
        assert_eq!(app.stored.columns[0].tasks[0].title, "Final report");
        assert_eq!(app.stored.columns[0].tasks[0].description, "季度報告");
        // Older local snapshots would overwrite this change, so they are dropped.
        assert!(app.undo_history.is_empty());
    }
    assert_eq!(*host.changed.lock().unwrap(), vec![host.board_id]);

    // `null` clears a field; omitted fields stay unchanged.
    let cleared = call(
        &host,
        &writer,
        "task.update",
        json!({"board_id": host.board_id, "task_id": 10, "due_time": null}),
    )
    .unwrap();
    assert!(cleared["task"]["due_time"].is_null());
    assert_eq!(cleared["task"]["title"], "Final report");

    // An unchanged write saves nothing and notifies no one.
    call(
        &host,
        &writer,
        "task.update",
        json!({"board_id": host.board_id, "task_id": 10}),
    )
    .unwrap();
    assert_eq!(host.changed.lock().unwrap().len(), 2);
}

#[test]
fn legacy_tasks_keep_their_creation_time_when_updated() {
    let host = TestHost::new();
    let writer = context("writer", &[host.board_id], true);
    let before = call(
        &host,
        &writer,
        "task.get",
        json!({"board_id": host.board_id, "task_id": 11}),
    )
    .unwrap();
    assert!(before["task"]["start_time"].is_null());
    assert_eq!(before["task"]["created_time"], 1_600_000_000_000u64);
    let after = call(
        &host,
        &writer,
        "task.update",
        json!({"board_id": host.board_id, "task_id": 11, "pinned": true}),
    )
    .unwrap();
    assert!(after["task"]["start_time"].is_null());
    assert_eq!(after["task"]["created_time"], 1_600_000_000_000u64);
}

#[test]
fn archive_and_restore_round_trip_and_recurring_tasks_continue() {
    let host = TestHost::new();
    let writer = context("writer", &[host.board_id], true);
    let archived = call(
        &host,
        &writer,
        "task.archive",
        json!({"board_id": host.board_id, "task_id": 10}),
    )
    .unwrap();
    assert_eq!(archived["task"]["archived"], true);
    assert!(archived["next_task"].is_null());
    let page = call(
        &host,
        &writer,
        "task.list",
        json!({"board_id": host.board_id, "status": "archived"}),
    )
    .unwrap();
    assert_eq!(page["tasks"][0]["id"], 10);
    let restored = call(
        &host,
        &writer,
        "task.restore",
        json!({"board_id": host.board_id, "task_id": 10, "column_id": 2}),
    )
    .unwrap();
    assert_eq!(restored["task"]["column_id"], 2);
    assert_eq!(restored["task"]["description"], "季度報告");

    let due = 1_700_000_000_000u64;
    let recurring = call(
        &host,
        &writer,
        "task.create",
        json!({"board_id": host.board_id, "column_id": 1, "title": "Routine",
               "due_time": due, "recurrence": {"frequency": "daily", "interval": 1}}),
    )
    .unwrap();
    let archived = call(
        &host,
        &writer,
        "task.archive",
        json!({"board_id": host.board_id, "task_id": recurring["task"]["id"]}),
    )
    .unwrap();
    let next = &archived["next_task"];
    assert_ne!(next["id"], recurring["task"]["id"]);
    assert!(next["due_time"].as_u64().unwrap() > due);
}

#[test]
fn moves_respect_custom_order_and_search_finds_unicode_text() {
    let host = TestHost::new();
    let writer = context("writer", &[host.board_id], true);
    let moved = call(
        &host,
        &writer,
        "task.move",
        json!({"board_id": host.board_id, "task_id": 11, "column_id": 1, "before_task_id": 10}),
    )
    .unwrap();
    assert_eq!(moved["task"]["column_id"], 1);
    let columns = host.app.lock().unwrap().stored.columns.clone();
    assert_eq!(columns[0].tasks[0].id, 11);
    let found = call(
        &host,
        &writer,
        "task.list",
        json!({"board_id": host.board_id, "query": "季度報告"}),
    )
    .unwrap();
    assert_eq!(found["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(found["tasks"][0]["id"], 10);
    let listed = call(
        &host,
        &writer,
        "column.list",
        json!({"board_id": host.board_id}),
    )
    .unwrap();
    assert_eq!(listed["columns"][0]["name"], "待辦 😀");
    assert_eq!(listed["columns"][0]["sort_order"], "custom");
}

#[test]
fn grants_hide_write_operations_and_reject_other_boards() {
    let host = TestHost::new();
    let reader = context("reader", &[host.board_id], false);
    assert!(visible(&reader.grants).all(|operation| operation.access == Access::Read));
    assert!(matches!(
        call(
            &host,
            &reader,
            "task.create",
            json!({"board_id": host.board_id, "column_id": 1, "title": "No"})
        ),
        Err(OperationError::UnknownOperation)
    ));
    assert!(matches!(
        call(
            &host,
            &reader,
            "column.list",
            json!({"board_id": host.other_board_id})
        ),
        Err(OperationError::PermissionDenied)
    ));
    let writer = context("writer", &[host.board_id], true);
    assert!(matches!(
        call(
            &host,
            &writer,
            "task.create",
            json!({"board_id": host.other_board_id, "column_id": 1, "title": "No"})
        ),
        Err(OperationError::PermissionDenied)
    ));
    assert_eq!(visible(&context("none", &[], true).grants).count(), 0);
    assert!(host.changed.lock().unwrap().is_empty());
}

#[test]
fn callers_see_only_their_own_boards() {
    let host = TestHost::new();
    let first = context("first", &[host.board_id], true);
    let second = context("second", &[host.other_board_id], false);
    let ids = |context: &OperationContext| {
        call(&host, context, "board.list", json!({})).unwrap()["boards"]
            .as_array()
            .unwrap()
            .iter()
            .map(|board| {
                (
                    board["id"].as_i64().unwrap(),
                    board["writable"].as_bool().unwrap(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&first), vec![(host.board_id, true)]);
    assert_eq!(ids(&second), vec![(host.other_board_id, false)]);
    assert_ne!(first.operation_id, second.operation_id);
    // Only the number of withheld boards is reported, never their names.
    let listed = call(&host, &second, "board.list", json!({})).unwrap();
    assert_eq!(listed["hidden_board_count"], 1);
    assert!(!listed.to_string().contains("待辦"));
}

#[test]
fn all_board_grants_cover_boards_created_later() {
    let host = TestHost::new();
    let mut all = context("all", &[], false);
    all.grants.all_boards = true;
    let later = host
        .app
        .lock()
        .unwrap()
        .database
        .create_board_with_data("Later", &StoredData::default())
        .unwrap()
        .id;
    let listed = call(&host, &all, "board.list", json!({})).unwrap();
    assert_eq!(listed["boards"].as_array().unwrap().len(), 3);
    assert_eq!(listed["hidden_board_count"], 0);
    call(&host, &all, "column.list", json!({"board_id": later})).unwrap();
    // The scope widens boards, not access: writes stay hidden without write.
    assert!(matches!(
        call(
            &host,
            &all,
            "column.create",
            json!({"board_id": later, "name": "New"})
        ),
        Err(OperationError::UnknownOperation)
    ));
}

#[test]
fn columns_are_created_in_place_and_shared_validation_rejects_bad_input() {
    let host = TestHost::new();
    let writer = context("writer", &[host.board_id], true);
    let created = call(
        &host,
        &writer,
        "column.create",
        json!({"board_id": host.board_id, "name": "  進行中 🚧 ", "before_column_id": 2}),
    )
    .unwrap();
    let column = &created["column"];
    assert_eq!(column["name"], "進行中 🚧");
    assert_eq!(column["sort_order"], "custom");
    let id = column["id"].as_i64().unwrap();
    let names = |host: &TestHost| {
        call(
            host,
            &writer,
            "column.list",
            json!({"board_id": host.board_id}),
        )
        .unwrap()["columns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|column| column["id"].as_i64().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&host), vec![1, id, 2]);
    assert_eq!(host.changed.lock().unwrap().as_slice(), [host.board_id]);

    // A task can be created in the new column straight away.
    call(
        &host,
        &writer,
        "task.create",
        json!({"board_id": host.board_id, "column_id": id, "title": "Next"}),
    )
    .unwrap();

    for arguments in [
        json!({"board_id": host.board_id, "name": "   "}),
        json!({"board_id": host.board_id, "name": "A", "color": "x".repeat(65)}),
    ] {
        assert!(matches!(
            call(&host, &writer, "column.create", arguments),
            Err(OperationError::InvalidInput)
        ));
    }
    // A missing anchor fails without leaving a column behind.
    assert!(call(
        &host,
        &writer,
        "column.create",
        json!({"board_id": host.board_id, "name": "Orphan", "before_column_id": 999}),
    )
    .is_err());
    assert_eq!(names(&host).len(), 3);
    assert!(matches!(
        call(
            &host,
            &writer,
            "column.create",
            json!({"board_id": host.other_board_id, "name": "Elsewhere"}),
        ),
        Err(OperationError::PermissionDenied)
    ));
}

#[test]
fn invalid_arguments_and_domain_failures_map_to_public_errors() {
    let host = TestHost::new();
    let writer = context("writer", &[host.board_id], true);
    for arguments in [
        json!({"board_id": host.board_id, "task_id": 10, "unexpected": true}),
        json!({"board_id": "1", "task_id": 10}),
        json!({"task_id": 10}),
        json!([]),
    ] {
        assert!(matches!(
            call(&host, &writer, "task.get", arguments),
            Err(OperationError::InvalidInput)
        ));
    }
    assert!(matches!(
        call(
            &host,
            &writer,
            "task.list",
            json!({"board_id": host.board_id, "limit": 0})
        ),
        Err(OperationError::InvalidInput)
    ));
    assert!(matches!(
        call(&host, &writer, "sql.execute", json!({})),
        Err(OperationError::UnknownOperation)
    ));
    assert_eq!(
        domain(
            call(
                &host,
                &writer,
                "task.get",
                json!({"board_id": host.board_id, "task_id": 999})
            )
            .unwrap_err()
        ),
        json!({"code": "TASK_NOT_FOUND"})
    );
    assert_eq!(
        domain(
            call(
                &host,
                &writer,
                "task.create",
                json!({"board_id": host.board_id, "column_id": 99, "title": "x"})
            )
            .unwrap_err()
        ),
        json!({"code": "COLUMN_NOT_FOUND"})
    );
    // A granted board that was deleted is reported, not exposed as an internal error.
    let missing = context("writer", &[9_999], true);
    assert_eq!(
        domain(call(&host, &missing, "column.list", json!({"board_id": 9_999})).unwrap_err()),
        json!({"code": "BOARD_NOT_FOUND"})
    );
}

#[test]
fn read_only_shared_boards_reject_writes_from_any_caller() {
    let host = TestHost::new();
    host.app
        .lock()
        .unwrap()
        .database
        .set_shared_board(host.board_id, BoardRole::Viewer, SyncStatus::Synced, 1)
        .unwrap();
    let writer = context("writer", &[host.board_id], true);
    assert!(matches!(
        call(
            &host,
            &writer,
            "task.archive",
            json!({"board_id": host.board_id, "task_id": 10})
        ),
        Err(OperationError::PermissionDenied)
    ));
    let boards = call(&host, &writer, "board.list", json!({})).unwrap();
    assert_eq!(boards["boards"][0]["role"], "viewer");
    assert_eq!(boards["boards"][0]["writable"], false);
    assert!(host.changed.lock().unwrap().is_empty());
}

#[test]
fn columns_update_only_given_fields_and_move_in_place() {
    let host = TestHost::new();
    let writer = context("writer", &[host.board_id], true);
    let board = host.board_id;
    let updated = call(
        &host,
        &writer,
        "column.update",
        json!({"board_id": board, "column_id": 1, "color": "#ff0000"}),
    )
    .unwrap();
    assert_eq!(updated["column"]["name"], "待辦 😀");
    assert_eq!(updated["column"]["color"], "#ff0000");
    let renamed = call(
        &host,
        &writer,
        "column.update",
        json!({"board_id": board, "column_id": 1, "name": " Doing "}),
    )
    .unwrap();
    assert_eq!(renamed["column"]["name"], "Doing");
    assert_eq!(renamed["column"]["color"], "#ff0000");
    assert!(matches!(
        call(
            &host,
            &writer,
            "column.update",
            json!({"board_id": board, "column_id": 1, "name": " "}),
        ),
        Err(OperationError::InvalidInput)
    ));
    assert_eq!(
        domain(
            call(
                &host,
                &writer,
                "column.update",
                json!({"board_id": board, "column_id": 99, "name": "x"}),
            )
            .unwrap_err()
        )["code"],
        "COLUMN_NOT_FOUND"
    );

    let order = |value: serde_json::Value| {
        value["columns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|column| column["id"].as_i64().unwrap())
            .collect::<Vec<_>>()
    };
    let moved = call(
        &host,
        &writer,
        "column.move",
        json!({"board_id": board, "column_id": 2, "before_column_id": 1}),
    )
    .unwrap();
    assert_eq!(order(moved), vec![2, 1]);
    let moved = call(
        &host,
        &writer,
        "column.move",
        json!({"board_id": board, "column_id": 2}),
    )
    .unwrap();
    assert_eq!(order(moved), vec![1, 2]);
    assert!(call(
        &host,
        &writer,
        "column.move",
        json!({"board_id": board, "column_id": 99}),
    )
    .is_err());
    // Tasks move with their column.
    let tasks = call(
        &host,
        &writer,
        "task.list",
        json!({"board_id": board, "column_id": 1}),
    )
    .unwrap();
    assert_eq!(tasks["tasks"].as_array().unwrap().len(), 2);
}

#[test]
fn idempotency_keys_return_the_first_task_for_the_same_caller() {
    let host = TestHost::new();
    let writer = context("writer", &[host.board_id], true);
    let create = |context: &OperationContext, title: &str| {
        call(
            &host,
            context,
            "task.create",
            json!({"board_id": host.board_id, "column_id": 2, "title": title,
                   "idempotency_key": "retry-1"}),
        )
    };
    let first = create(&writer, "Once").unwrap();
    let retried = create(&writer, "Once").unwrap();
    assert_eq!(first["task"]["id"], retried["task"]["id"]);
    // A reused key with other arguments is a client error, not a second task.
    assert!(matches!(
        create(&writer, "Different"),
        Err(OperationError::InvalidInput)
    ));
    // Keys are scoped to the caller.
    let other = context("other", &[host.board_id], true);
    assert_ne!(
        create(&other, "Once").unwrap()["task"]["id"],
        first["task"]["id"]
    );
    let stored = host
        .app
        .lock()
        .unwrap()
        .database
        .read_board_complete(host.board_id)
        .unwrap();
    assert_eq!(
        stored.columns[1]
            .tasks
            .iter()
            .filter(|task| task.title == "Once")
            .count(),
        2
    );

    // A retry after archiving returns the archived task.
    let id = first["task"]["id"].as_i64().unwrap();
    call(
        &host,
        &writer,
        "task.archive",
        json!({"board_id": host.board_id, "task_id": id}),
    )
    .unwrap();
    let archived = create(&writer, "Once").unwrap();
    assert_eq!(archived["task"]["id"], id);
    assert_eq!(archived["task"]["archived"], true);

    for key in ["", &"k".repeat(129), "a\nb"] {
        assert!(matches!(
            call(
                &host,
                &writer,
                "task.create",
                json!({"board_id": host.board_id, "column_id": 2, "title": "x",
                       "idempotency_key": key}),
            ),
            Err(OperationError::InvalidInput)
        ));
    }
}
