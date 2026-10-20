use super::board::*;
use super::types::{id, PluginInstance, PluginRun};
use crate::{errors::DomainError, models::*, storage::Database};
use serde_json::{json, Value};

struct Fixture {
    dir: std::path::PathBuf,
    db: Database,
    instance: PluginInstance,
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("cardbe-board-plugin-{}", id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut loaded = crate::storage::load(&dir).unwrap();
        loaded.database.initialize_plugins().unwrap();
        let instance:PluginInstance=serde_json::from_value(json!({
            "id":"fixture", "plugin_id":"fixture", "name":"Fixture", "board_id":loaded.database.active_board_id(),
            "config":{},"allowed_domains":[],"enabled":true,"interval_seconds":0,"failures":0,
        })).unwrap();
        let data = StoredData {
            columns: vec![Column {
                id: 1,
                name: "待辦 😀".into(),
                color: String::new(),
                sort_order: ColumnSort::Custom,
                sort_rules: Vec::new(),
                tasks: (10..261)
                    .map(|id| Task {
                        id,
                        title: format!("任務 {id:03}"),
                        description: "內容".into(),
                        ..Task::default()
                    })
                    .collect(),
            }],
            ..StoredData::default()
        };
        loaded
            .database
            .replace_board_as_local_edit(instance.board_id, &data)
            .unwrap();
        super::storage::put(
            loaded.database.plugin_connection(),
            "instance",
            &instance.id,
            &instance,
        )
        .unwrap();
        let run: PluginRun = serde_json::from_value(
            json!({"id":"run","instance_id":"fixture","trigger":"manual",
            "started_at":0,"status":"running","logs":[],"created":0,"updated":0}),
        )
        .unwrap();
        super::storage::put(loaded.database.plugin_connection(), "run", "run", &run).unwrap();
        Self {
            dir,
            db: loaded.database,
            instance,
        }
    }
    fn raw(&self, operations: Value) -> String {
        json!({"version":version(&self.db,self.instance.board_id).unwrap(),"operations":operations})
            .to_string()
    }
    fn prepare(&self, operations: Value) -> Result<PreparedChangeSet, DomainError> {
        prepare(
            &self.db,
            self.instance.clone(),
            "run".into(),
            serde_json::from_str(&self.raw(operations))
                .map_err(|_| DomainError::InvalidArgument)?,
        )
    }
}
fn cleanup(fixture: Fixture) {
    let dir = fixture.dir.clone();
    drop(fixture);
    let canonical = dir.canonicalize().unwrap();
    assert!(canonical.starts_with(std::env::temp_dir().canonicalize().unwrap()));
    std::fs::remove_dir_all(canonical).unwrap();
}

#[test]
fn read_only_output_commits_checkpoint_without_loading_or_rewriting_large_board() {
    let mut f = Fixture::new();
    let mut data = f.db.read_board_complete(f.instance.board_id).unwrap();
    for task in data.columns[0].tasks.iter_mut().take(100) {
        task.description = "x".repeat(100_000);
    }
    assert!(serde_json::to_vec(&data.columns).unwrap().len() > 8 * 1024 * 1024);
    // A large synthetic projection avoids benchmarking Loro fixture construction.
    let tx = f.db.plugin_connection().unchecked_transaction().unwrap();
    for task in data.columns[0].tasks.iter().take(100) {
        tx.execute(
            "UPDATE cardbe_tasks SET payload=?1 WHERE board_id=?2 AND id=?3",
            rusqlite::params![
                serde_json::to_string(task).unwrap(),
                f.instance.board_id,
                task.id
            ],
        )
        .unwrap();
    }
    tx.commit().unwrap();
    let token = version(&f.db, f.instance.board_id).unwrap();
    let output = super::runtime::RuntimeOutput {
        changeset: None,
        snapshot_version: token.clone(),
        tasks: vec![],
        cursor: Some("read-only".into()),
        logs: vec![],
    };
    let prepared = prepare_output(&f.db, f.instance.clone(), "run".into(), &output).unwrap();
    assert!(!prepared.has_board_operations);
    assert!(prepared.after.columns.is_empty());
    f.db.plugin_connection().execute_batch("CREATE TRIGGER forbid_board_write BEFORE UPDATE ON cardbe_tasks BEGIN SELECT RAISE(ABORT,'read-only run'); END;").unwrap();
    assert!(f
        .db
        .commit_changeset_if(&prepared, || true)
        .unwrap()
        .is_none());
    assert_eq!(version(&f.db, f.instance.board_id).unwrap(), token);
    assert_eq!(
        super::storage::list::<Option<String>>(f.db.plugin_connection(), "cursor").unwrap(),
        vec![Some("read-only".into())]
    );
    cleanup(f);
}

#[test]
fn mixed_operations_sources_cursor_and_history_commit_or_rollback_together() {
    let mut f = Fixture::new();
    let token = version(&f.db, f.instance.board_id).unwrap();
    let incoming = super::runtime::ExternalTask {
        external_key: "mixed:2".into(),
        title: "Array upsert".into(),
        description: String::new(),
        url: String::new(),
        state: "open".into(),
        labels: vec![],
        updated_at: String::new(),
        column_id: 1,
    };
    let mut nested = incoming.clone();
    nested.external_key = "mixed:1".into();
    nested.title = "Nested upsert".into();
    let raw = f.raw(json!([
        {"kind":"create_column","key":"new","name":"Mixed","color":""},
        {"kind":"upsert_external_task","column":"new","task":nested}
    ]));
    let output = super::runtime::RuntimeOutput {
        changeset: Some(serde_json::from_str(&raw).unwrap()),
        snapshot_version: token.clone(),
        tasks: vec![incoming],
        cursor: Some("mixed-cursor".into()),
        logs: vec![],
    };
    let prepared = prepare_output(&f.db, f.instance.clone(), "run".into(), &output).unwrap();
    assert_eq!(prepared.created, 3);
    let before_crdt = f.db.iroh_loro_update(f.instance.board_id).unwrap();
    f.db.plugin_connection().execute_batch("CREATE TRIGGER fail_mixed BEFORE UPDATE ON cardbe_plugin_data WHEN NEW.kind='run' BEGIN SELECT RAISE(ABORT,'synthetic rollback'); END;").unwrap();
    assert!(f.db.commit_changeset(&prepared).is_err());
    assert_eq!(version(&f.db, f.instance.board_id).unwrap(), token);
    assert_eq!(
        f.db.iroh_loro_update(f.instance.board_id).unwrap(),
        before_crdt
    );
    assert_eq!(
        f.db.plugin_connection()
            .query_row("SELECT count(*) FROM cardbe_plugin_sources", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(
        super::storage::list::<Option<String>>(f.db.plugin_connection(), "cursor")
            .unwrap()
            .is_empty()
    );
    f.db.plugin_connection()
        .execute_batch("DROP TRIGGER fail_mixed")
        .unwrap();
    let after = f.db.commit_changeset(&prepared).unwrap();
    assert_eq!(after.columns[1].tasks[0].title, "Nested upsert");
    assert!(after.columns[0]
        .tasks
        .iter()
        .any(|t| t.title == "Array upsert"));
    assert_eq!(
        super::storage::list::<Option<String>>(f.db.plugin_connection(), "cursor").unwrap(),
        vec![Some("mixed-cursor".into())]
    );
    assert_eq!(
        f.db.plugin_connection()
            .query_row("SELECT count(*) FROM cardbe_plugin_sources", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        super::storage::list::<PluginRun>(f.db.plugin_connection(), "run").unwrap()[0].created,
        3
    );
    cleanup(f);
}

#[test]
fn source_mapping_conflicts_and_grant_revocation_invalidate_output() {
    let mut f = Fixture::new();
    let task = super::runtime::ExternalTask {
        external_key: "source:conflict".into(),
        title: "Source".into(),
        description: String::new(),
        url: String::new(),
        state: "open".into(),
        labels: vec![],
        updated_at: String::new(),
        column_id: 1,
    };
    let prepared = f
        .prepare(json!([{"kind":"upsert_external_task","task":task}]))
        .unwrap();
    let expected_version = prepared.version.clone();
    let link = &prepared.sources["source:conflict"].link;
    f.db.plugin_connection().execute("INSERT INTO cardbe_plugin_sources(instance_id,external_key,board_id,task_id,last_import) VALUES(?1,?2,?3,?4,?5)", rusqlite::params![f.instance.id,"source:conflict",f.instance.board_id,link.task_id,serde_json::to_string(link).unwrap()]).unwrap();
    assert!(matches!(
        f.db.commit_changeset(&prepared),
        Err(DomainError::StaleBoardRequest)
    ));
    assert_eq!(
        version(&f.db, f.instance.board_id).unwrap(),
        expected_version
    );
    let prepared = f
        .prepare(json!([{"kind":"create_column","key":"new","name":"Grant","color":""}]))
        .unwrap();
    f.instance.enabled = false;
    super::storage::put(
        f.db.plugin_connection(),
        "instance",
        &f.instance.id,
        &f.instance,
    )
    .unwrap();
    assert!(matches!(
        f.db.commit_changeset(&prepared),
        Err(DomainError::PermissionDenied)
    ));
    cleanup(f);
}

#[test]
fn scoped_queries_page_full_unicode_tasks_and_reject_foreign_filters() {
    let mut f = Fixture::new();
    let foreign = StoredData {
        columns: vec![Column {
            id: 1,
            name: "Foreign".into(),
            color: String::new(),
            sort_order: ColumnSort::Custom,
            sort_rules: Vec::new(),
            tasks: vec![Task {
                id: 10,
                title: "Foreign".into(),
                ..Task::default()
            }],
        }],
        ..StoredData::default()
    };
    f.db.create_board_with_data("Foreign", &foreign).unwrap();
    let reader = Reader::open(f.db.path().to_owned(), f.instance.board_id).unwrap();
    let boards = reader.list_boards().unwrap();
    assert_eq!(boards.len(), 1);
    assert_eq!(boards[0].id, f.instance.board_id);
    assert_eq!(reader.get_board().unwrap().id, f.instance.board_id);
    assert_ne!(reader.get_task(10, false).unwrap().task.title, "Foreign");
    let columns = reader.list_columns().unwrap();
    assert_eq!(columns[0].name, "待辦 😀");
    assert_eq!(reader.get_column(1).unwrap(), columns[0]);
    let mut ids = std::collections::HashSet::new();
    for offset in [0, 100, 200] {
        let filter = crate::storage::TaskExplorerQuery {
            status: "active".into(),
            column_id: Some(1),
            ..Default::default()
        };
        for row in reader.list_tasks(filter, offset, 100).unwrap() {
            assert_eq!(row.task.description, "內容");
            assert_eq!(row.column_id, Some(1));
            assert!(ids.insert(row.task.id));
        }
    }
    assert_eq!(ids.len(), 251);
    assert!(reader.list_tasks(Default::default(), 0, 101).is_err());
    assert!(reader
        .list_tasks(
            crate::storage::TaskExplorerQuery {
                board_ids: Some(vec![999]),
                ..Default::default()
            },
            0,
            1
        )
        .is_err());
    assert!(reader.get_task(999, false).is_err());
    assert!(reader.get_column(999).is_err());
    drop(reader);
    cleanup(f);
}

#[test]
fn reader_snapshot_stays_consistent_and_new_invocation_sees_local_edits_and_archives() {
    let mut f = Fixture::new();
    let reader = Reader::open(f.db.path().to_owned(), f.instance.board_id).unwrap();
    let original = reader.get_task(10, false).unwrap();
    let mut data = f.db.read_board_complete(f.instance.board_id).unwrap();
    data.columns[0].tasks[0].title = "Local".into();
    data.archives.push(Archive {
        time: 10,
        task: Task {
            id: 999,
            title: "Archived".into(),
            ..Task::default()
        },
    });
    f.db.replace_board_as_local_edit(f.instance.board_id, &data)
        .unwrap();
    assert_eq!(reader.get_task(10, false).unwrap(), original);
    let fresh = Reader::open(f.db.path().to_owned(), f.instance.board_id).unwrap();
    assert_ne!(fresh.version, reader.version);
    assert_eq!(fresh.get_task(10, false).unwrap().task.title, "Local");
    let page = fresh
        .list_tasks(
            crate::storage::TaskExplorerQuery {
                status: "archived".into(),
                ..Default::default()
            },
            0,
            1,
        )
        .unwrap();
    assert_eq!(page[0].task.id, 999);
    assert_eq!(page[0].column_id, None);
    drop(reader);
    drop(fresh);
    cleanup(f);
}

#[test]
fn dependent_changeset_is_private_until_commit_and_writes_one_crdt_delta() {
    let mut f = Fixture::new();
    let token = version(&f.db, f.instance.board_id).unwrap();
    let old_revision = f.db.board_sync_state(f.instance.board_id).unwrap().0;
    let p=f.prepare(json!([
        {"kind":"create_column","key":"new","name":"分類","color":""},
        {"kind":"move_task","task_id":10,"column":"new","before_task_id":null},
        {"kind":"move_task","task_id":11,"column":"new","before_task_id":10},
        {"kind":"create_task","key":"created","column":"new","task":{"title":"新任務"}},
        {"kind":"update_task","task_id":12,"task":{"title":"更新","description":"😀","labels":["標籤"]}},
        {"kind":"move_column","column":"new","before":1}
    ])).unwrap();
    assert_eq!(version(&f.db, f.instance.board_id).unwrap(), token);
    let column = p.after.columns[0].id;
    let created = p.after.columns[0].tasks[2].id;
    let committed = f.db.commit_changeset(&p).unwrap();
    assert_eq!(committed.columns[0].id, column);
    assert_eq!(
        committed.columns[0]
            .tasks
            .iter()
            .map(|t| t.id)
            .collect::<Vec<_>>(),
        vec![11, 10, created]
    );
    assert_eq!(
        f.db.load_task(f.instance.board_id, 12)
            .unwrap()
            .unwrap()
            .title,
        "更新"
    );
    assert_eq!(
        f.db.board_sync_state(f.instance.board_id).unwrap().0,
        old_revision + 1
    );
    let update = f.db.iroh_loro_update(f.instance.board_id).unwrap().unwrap();
    let projected = crate::loro_board::project(&update).unwrap();
    assert_eq!(projected.columns, committed.columns);
    // A duplicate commit cannot pass the original content precondition.
    assert!(matches!(
        f.db.commit_changeset(&p),
        Err(DomainError::StaleBoardRequest)
    ));
    cleanup(f);
}

#[test]
fn invalid_operations_and_permissions_never_write() {
    let mut f = Fixture::new();
    let token = version(&f.db, f.instance.board_id).unwrap();
    assert!(!f.prepare(json!([])).unwrap().has_board_operations);
    assert_eq!(version(&f.db, f.instance.board_id).unwrap(), token);
    for operations in [
        json!([{"kind":"move_task","task_id":10,"column":"future","before_task_id":null}]),
        json!([{"kind":"create_column","key":"new","name":"New","color":""},{"kind":"move_task","task_id":999,"column":"new","before_task_id":null}]),
        json!([{"kind":"update_task","task_id":10,"task":{"title":" "}}]),
        json!([{"kind":"update_task","task_id":10,"task":{"title":"Valid","labels":["same","same"]}}]),
        json!([{"kind":"update_task","task_id":10,"task":{"title":"Valid","items":[{"id":""}]}}]),
        json!([{"kind":"update_task","task_id":10,"task":{"title":"Valid","items":[{"id":"same"},{"id":"same"}]}}]),
        json!([{"kind":"move_column","column":999,"before":999}]),
        json!([{"kind":"delete_task","task_id":10}]),
        json!([{"kind":"create_task","key":"x","column":1,"task":{"title":"X"}},{"kind":"move_task","task_id":10,"column":"x","before_task_id":null}]),
    ] {
        assert!(f.prepare(operations).is_err());
    }
    assert_eq!(version(&f.db, f.instance.board_id).unwrap(), token);
    let p = f
        .prepare(json!([{"kind":"create_column","key":"new","name":"New","color":""}]))
        .unwrap();
    let mut revoked = f.instance.clone();
    revoked.enabled = false;
    super::storage::put(f.db.plugin_connection(), "instance", &revoked.id, &revoked).unwrap();
    assert!(matches!(
        f.db.commit_changeset(&p),
        Err(DomainError::PermissionDenied)
    ));
    super::storage::put(
        f.db.plugin_connection(),
        "instance",
        &f.instance.id,
        &f.instance,
    )
    .unwrap();
    f.db.connection
        .execute(
            "UPDATE cardbe_boards SET shared_role='viewer' WHERE id=?1",
            [f.instance.board_id],
        )
        .unwrap();
    assert!(matches!(
        f.db.commit_changeset(&p),
        Err(DomainError::PermissionDenied)
    ));
    assert!(matches!(
        f.prepare(json!([{"kind":"create_column","key":"new","name":"New","color":""}])),
        Err(DomainError::PermissionDenied)
    ));
    assert_eq!(version(&f.db, f.instance.board_id).unwrap(), token);
    cleanup(f);
}

#[test]
fn remote_snapshot_invalidates_changeset_without_overwriting_remote_content() {
    let mut f = Fixture::new();
    let p = f
        .prepare(json!([{"kind":"create_column","key":"new","name":"New","color":""}]))
        .unwrap();
    let mut data = f.db.read_board_complete(f.instance.board_id).unwrap();
    data.columns[0].tasks[0].title = "Remote".into();
    let update = crate::loro_board::apply_local_delta(None, &StoredData::default(), &data).unwrap();
    f.db.apply_iroh_snapshot(
        f.instance.board_id,
        "Remote board",
        IrohPermission::Editor,
        1,
        &data,
        Some(&update),
    )
    .unwrap();
    assert!(matches!(
        f.db.commit_changeset(&p),
        Err(DomainError::StaleBoardRequest)
    ));
    assert_eq!(
        f.db.load_task(f.instance.board_id, 10)
            .unwrap()
            .unwrap()
            .title,
        "Remote"
    );
    cleanup(f);
}

#[test]
fn source_upserts_rebase_after_concurrent_edits_but_general_operations_stay_stale() {
    let mut f = Fixture::new();
    let snapshot = version(&f.db, f.instance.board_id).unwrap();
    let mut data = f.db.read_board_complete(f.instance.board_id).unwrap();
    data.columns[0].tasks[0].title = "Concurrent".into();
    f.db.replace_board_as_local_edit(f.instance.board_id, &data)
        .unwrap();
    let task = super::runtime::ExternalTask {
        external_key: "source:1".into(),
        title: "Imported".into(),
        description: String::new(),
        url: String::new(),
        state: "open".into(),
        labels: vec![],
        updated_at: String::new(),
        column_id: 1,
    };
    let general = super::runtime::RuntimeOutput {
        changeset: Some(ChangeSet {
            version: snapshot.clone(),
            operations: vec![Operation::CreateColumn {
                key: "new".into(),
                name: "New".into(),
                color: String::new(),
            }],
        }),
        snapshot_version: snapshot.clone(),
        tasks: vec![task.clone()],
        cursor: None,
        logs: vec![],
    };
    assert!(matches!(
        prepare_output(&f.db, f.instance.clone(), "run".into(), &general),
        Err(DomainError::StaleBoardRequest)
    ));
    let declared = super::runtime::RuntimeOutput {
        changeset: Some(ChangeSet {
            version: "other".into(),
            operations: vec![],
        }),
        snapshot_version: snapshot.clone(),
        tasks: vec![task.clone()],
        cursor: None,
        logs: vec![],
    };
    assert!(matches!(
        prepare_output(&f.db, f.instance.clone(), "run".into(), &declared),
        Err(DomainError::StaleBoardRequest)
    ));
    let upserts = super::runtime::RuntimeOutput {
        changeset: None,
        snapshot_version: snapshot,
        tasks: vec![task],
        cursor: Some("next".into()),
        logs: vec![],
    };
    let prepared = prepare_output(&f.db, f.instance.clone(), "run".into(), &upserts).unwrap();
    f.db.commit_changeset(&prepared).unwrap();
    let board = f.db.read_board_complete(f.instance.board_id).unwrap();
    assert_eq!(board.columns[0].tasks[0].title, "Concurrent");
    assert!(board.columns[0]
        .tasks
        .iter()
        .any(|task| task.title == "Imported"));
    cleanup(f);
}

#[test]
fn source_limit_prunes_only_deleted_task_mappings_before_failing() {
    let mut f = Fixture::new();
    let board = f.instance.board_id;
    let insert = |f: &Fixture, key: &str, task: i64, payload: &str| {
        f.db.plugin_connection()
            .execute(
                "INSERT INTO cardbe_plugin_sources VALUES(?1,?2,?3,?4,?5)",
                rusqlite::params![f.instance.id, key, board, task, payload],
            )
            .unwrap();
    };
    let tx = f.db.plugin_connection().unchecked_transaction().unwrap();
    for index in 0..20_001 {
        insert(&f, &format!("deleted:{index}"), 1_000_000 + index, "{}");
    }
    insert(&f, "live", 10, "{}");
    tx.commit().unwrap();
    let output = |f: &Fixture, key: &str| super::runtime::RuntimeOutput {
        changeset: None,
        snapshot_version: version(&f.db, board).unwrap(),
        tasks: vec![super::runtime::ExternalTask {
            external_key: key.into(),
            title: "Imported".into(),
            description: String::new(),
            url: String::new(),
            state: "open".into(),
            labels: vec![],
            updated_at: String::new(),
            column_id: 1,
        }],
        cursor: None,
        logs: vec![],
    };
    let prepared =
        prepare_output(&f.db, f.instance.clone(), "run".into(), &output(&f, "new")).unwrap();
    f.db.commit_changeset(&prepared).unwrap();
    let keys: Vec<String> =
        f.db.plugin_connection()
            .prepare("SELECT external_key FROM cardbe_plugin_sources ORDER BY external_key")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
    assert_eq!(keys, vec!["live", "new"]);
    // Live mappings are never pruned, so an oversized live set still fails without writing.
    let large = format!("\"{}\"", "x".repeat(140_000));
    let tx = f.db.plugin_connection().unchecked_transaction().unwrap();
    for task in 11..261 {
        insert(&f, &format!("live:{task}"), task, &large);
    }
    tx.commit().unwrap();
    let token = version(&f.db, board).unwrap();
    let prepared = prepare_output(
        &f.db,
        f.instance.clone(),
        "run".into(),
        &output(&f, "another"),
    )
    .unwrap();
    assert!(matches!(
        f.db.commit_changeset(&prepared),
        Err(DomainError::InvalidArgument)
    ));
    assert_eq!(version(&f.db, board).unwrap(), token);
    cleanup(f);
}

#[test]
fn changeset_uses_actual_pinned_order_and_host_allocated_task_ids() {
    let mut f = Fixture::new();
    let mut data = f.db.read_board_complete(f.instance.board_id).unwrap();
    data.columns[0].tasks[0].pinned = true;
    f.db.replace_board_as_local_edit(f.instance.board_id, &data)
        .unwrap();
    let p = f
        .prepare(json!([
            {"kind":"create_column","key":"new","name":"New","color":""},
            {"kind":"create_task","key":"task","column":"new","task":{"title":"New task","id":-1}},
            {"kind":"move_task","task_id":10,"column":"new","before_task_id":null}
        ]))
        .unwrap();
    let committed = f.db.commit_changeset(&p).unwrap();
    assert_eq!(committed.columns[1].tasks[0].id, 10);
    assert_ne!(committed.columns[1].tasks[1].id, -1);
    assert_eq!(committed.columns[1].tasks[1].title, "New task");
    cleanup(f);
}

#[test]
fn persistence_failure_rolls_back_board_crdt_and_run_and_allows_retry() {
    let mut f = Fixture::new();
    let p = f
        .prepare(json!([{"kind":"create_column","key":"new","name":"New","color":""}]))
        .unwrap();
    let token = version(&f.db, f.instance.board_id).unwrap();
    let crdt = f.db.iroh_loro_update(f.instance.board_id).unwrap();
    f.db.connection.execute_batch("CREATE TRIGGER refuse_changeset BEFORE UPDATE ON cardbe_plugin_data WHEN NEW.kind='run' BEGIN SELECT RAISE(ABORT,'isolated failure'); END;").unwrap();
    assert!(f.db.commit_changeset(&p).is_err());
    assert_eq!(version(&f.db, f.instance.board_id).unwrap(), token);
    assert_eq!(f.db.iroh_loro_update(f.instance.board_id).unwrap(), crdt);
    assert_eq!(
        super::storage::list::<PluginRun>(f.db.plugin_connection(), "run").unwrap()[0].status,
        "running"
    );
    f.db.connection
        .execute_batch("DROP TRIGGER refuse_changeset")
        .unwrap();
    let committed = f.db.commit_changeset(&p).unwrap();
    let update = f.db.iroh_loro_update(f.instance.board_id).unwrap().unwrap();
    assert_eq!(
        crate::loro_board::project(&update).unwrap().columns,
        committed.columns
    );
    cleanup(f);
}

#[test]
fn guest_column_updates_keep_sort_rules_that_match_the_legacy_sort() {
    let mut f = Fixture::new();
    let mut data = f.db.read_board_complete(f.instance.board_id).unwrap();
    let rules = vec![SortRule {
        field: "title".into(),
        direction: SortDirection::Desc,
    }];
    data.columns[0].set_sort_rules(rules.clone());
    f.db.replace_board_as_local_edit(f.instance.board_id, &data)
        .unwrap();

    // The guest sees `custom` and renames the column without changing it.
    let p = f
        .prepare(json!([{"kind":"update_column","column":1,"name":"Renamed","color":"","sort_order":"custom"}]))
        .unwrap();
    let committed = f.db.commit_changeset(&p).unwrap();
    assert_eq!(committed.columns[0].name, "Renamed");
    assert_eq!(committed.columns[0].sort_rules, rules);

    let p = f
        .prepare(json!([{"kind":"update_column","column":1,"name":"Renamed","color":"","sort_order":"due_date_desc"}]))
        .unwrap();
    let committed = f.db.commit_changeset(&p).unwrap();
    assert_eq!(committed.columns[0].sort_order, ColumnSort::DueDateDesc);
    assert_eq!(
        committed.columns[0].sort_rules,
        vec![SortRule {
            field: "due_time".into(),
            direction: SortDirection::Desc,
        }]
    );
    cleanup(f);
}
