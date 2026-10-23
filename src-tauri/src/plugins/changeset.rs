use super::{runtime::ExternalTask, storage, types::*};
use crate::{
    errors::DomainError,
    models::{StoredData, Task, TaskPriority},
};
use rusqlite::{params, OptionalExtension};
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceLink {
    pub task_id: i64,
    pub imported: ExternalTask,
    pub managed_labels: Vec<String>,
    pub ignored_labels: Vec<String>,
    pub title_owned: bool,
    pub description_owned: bool,
    pub column_owned: bool,
    #[serde(default = "owned")]
    pub priority_owned: bool,
}
fn owned() -> bool {
    true
}
/// Applies a reported source priority while the user has not changed it since the last report.
pub fn merge_priority(
    data: &mut StoredData,
    link: &mut SourceLink,
    priority: Option<TaskPriority>,
) -> bool {
    let Some(task) = data
        .columns
        .iter_mut()
        .flat_map(|c| c.tasks.iter_mut())
        .find(|t| t.id == link.task_id)
    else {
        return false;
    };
    link.priority_owned &= task.priority == link.imported.priority;
    link.imported.priority = priority;
    if !link.priority_owned || task.priority == priority {
        return false;
    }
    task.priority = priority;
    true
}
pub fn merge(
    data: &mut StoredData,
    incoming: &ExternalTask,
    link: Option<&SourceLink>,
) -> Result<(SourceLink, bool, bool), DomainError> {
    if incoming.external_key.is_empty()
        || incoming.external_key.len() > 512
        || incoming.title.trim().is_empty()
        || incoming.title.len() > 4096
        || incoming.description.len() > 262144
        || incoming.url.len() > 4096
        || incoming.state.len() > 64
        || incoming.updated_at.len() > 128
        || incoming.labels.len() > 100
        || incoming
            .labels
            .iter()
            .any(|l| l.trim().is_empty() || l.len() > 256)
    {
        return Err(DomainError::InvalidArgument);
    }
    let destination = data
        .columns
        .iter()
        .position(|c| c.id == incoming.column_id)
        .ok_or(DomainError::ColumnNotFound)?;
    if let Some(link) = link {
        let position = data.columns.iter().enumerate().find_map(|(ci, c)| {
            c.tasks
                .iter()
                .position(|t| t.id == link.task_id)
                .map(|ti| (ci, ti))
        });
        // Tombstones also cover archives and cross-board moves; never recreate a deliberately removed task.
        if let Some((ci, ti)) = position {
            let mut next = link.clone();
            let mut task = data.columns[ci].tasks.remove(ti);
            let old = task.clone();
            next.title_owned &= task.title == link.imported.title;
            next.description_owned &= task.description == link.imported.description;
            next.column_owned &= data.columns[ci].id == link.imported.column_id;
            if next.title_owned {
                task.title = incoming.title.clone();
            }
            if next.description_owned {
                task.description = incoming.description.clone();
            }
            for label in &link.managed_labels {
                if !task.labels.contains(label) && !next.ignored_labels.contains(label) {
                    next.ignored_labels.push(label.clone());
                }
            }
            task.labels
                .retain(|l| !link.managed_labels.contains(l) || incoming.labels.contains(l));
            next.managed_labels
                .retain(|l| incoming.labels.contains(l) && task.labels.contains(l));
            for label in &incoming.labels {
                if !task.labels.contains(label) && !next.ignored_labels.contains(label) {
                    task.labels.push(label.clone());
                    next.managed_labels.push(label.clone());
                }
            }
            let target = if next.column_owned { destination } else { ci };
            let changed = old != task || target != ci;
            if target == ci {
                data.columns[ci].tasks.insert(ti, task);
            } else {
                data.columns[target].tasks.push(task);
            }
            if next.ignored_labels.len() > 1000 {
                return Err(DomainError::InvalidArgument);
            }
            next.imported = ExternalTask {
                priority: link.imported.priority,
                ..incoming.clone()
            };
            return Ok((next, false, changed));
        }
        return Ok((link.clone(), false, false));
    }
    let id = data.allocate_task_id().map_err(DomainError::Internal)?;
    data.columns[destination].tasks.push(Task {
        id,
        title: incoming.title.clone(),
        description: incoming.description.clone(),
        labels: incoming.labels.clone(),
        created_time: now() as u128,
        ..Task::default()
    });
    Ok((
        SourceLink {
            task_id: id,
            imported: incoming.clone(),
            managed_labels: incoming.labels.clone(),
            ignored_labels: Vec::new(),
            title_owned: true,
            description_owned: true,
            column_owned: true,
            priority_owned: true,
        },
        true,
        false,
    ))
}
impl crate::storage::Database {
    pub(crate) fn plugin_connection(&self) -> &rusqlite::Connection {
        &self.connection
    }
    pub(crate) fn initialize_plugins(&self) -> Result<(), DomainError> {
        storage::initialize(&self.connection).map_err(|e| DomainError::Internal(e.to_string()))
    }
}

pub(crate) struct SourceChange {
    pub link: SourceLink,
    pub expected: Option<String>,
}
fn collision(
    connection: &rusqlite::Connection,
    instance: &PluginInstance,
    key: &str,
) -> Result<bool, DomainError> {
    connection.query_row("SELECT EXISTS(SELECT 1 FROM cardbe_plugin_sources s JOIN cardbe_plugin_data i ON i.kind='instance' AND i.id=s.instance_id WHERE s.board_id=?1 AND s.external_key=?2 AND s.instance_id<>?3 AND json_extract(i.payload,'$.plugin_id')=?4)", params![instance.board_id,key,instance.id,instance.plugin_id], |r| r.get(0)).map_err(|e| DomainError::repository(Box::new(e)))
}
fn source_raw(
    connection: &rusqlite::Connection,
    instance: &PluginInstance,
    key: &str,
) -> Result<Option<String>, DomainError> {
    connection.query_row("SELECT last_import FROM cardbe_plugin_sources WHERE instance_id=?1 AND board_id=?2 AND external_key=?3", params![instance.id,instance.board_id,key], |r|r.get(0)).optional().map_err(|e| DomainError::repository(Box::new(e)))
}
pub(crate) fn prepare_source(
    connection: &rusqlite::Connection,
    instance: &PluginInstance,
    data: &mut StoredData,
    task: &ExternalTask,
    pending: &std::collections::HashMap<String, SourceChange>,
) -> Result<(SourceChange, bool, bool), DomainError> {
    if collision(connection, instance, &task.external_key)? {
        return Err(DomainError::InvalidArgument);
    }
    let expected = match pending.get(&task.external_key) {
        Some(change) => change.expected.clone(),
        None => source_raw(connection, instance, &task.external_key)?,
    };
    let stored: Option<SourceLink> = expected
        .as_ref()
        .map(|raw| serde_json::from_str(raw))
        .transpose()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let previous = pending
        .get(&task.external_key)
        .map(|c| &c.link)
        .or(stored.as_ref());
    let (link, created, updated) = merge(data, task, previous)?;
    Ok((SourceChange { link, expected }, created, updated))
}
pub(crate) fn persist_sources(
    connection: &rusqlite::Connection,
    instance: &PluginInstance,
    changes: &std::collections::HashMap<String, SourceChange>,
) -> Result<(), DomainError> {
    if changes.is_empty() {
        return Ok(());
    }
    for (key, change) in changes {
        if collision(connection, instance, key)? {
            return Err(DomainError::InvalidArgument);
        }
        if source_raw(connection, instance, key)? != change.expected {
            return Err(DomainError::StaleBoardRequest);
        }
        connection.execute("INSERT INTO cardbe_plugin_sources(instance_id,external_key,board_id,task_id,last_import) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(instance_id,board_id,external_key) DO UPDATE SET last_import=excluded.last_import",params![instance.id,key,instance.board_id,change.link.task_id,serde_json::to_string(&change.link).map_err(|e| DomainError::Internal(e.to_string()))?]).map_err(|e| DomainError::repository(Box::new(e)))?;
    }
    if !within_source_limits(connection, instance)? {
        prune_tombstones(connection, instance, changes)?;
        if !within_source_limits(connection, instance)? {
            return Err(DomainError::InvalidArgument);
        }
    }
    Ok(())
}
fn within_source_limits(
    connection: &rusqlite::Connection,
    instance: &PluginInstance,
) -> Result<bool, DomainError> {
    let (count, bytes): (i64,i64) = connection.query_row("SELECT count(*),coalesce(sum(length(CAST(last_import AS BLOB))),0) FROM cardbe_plugin_sources WHERE instance_id=?1", [&instance.id], |r| Ok((r.get(0)?,r.get(1)?))).map_err(|e| DomainError::repository(Box::new(e)))?;
    Ok(count <= 20000 && bytes <= 32 * 1024 * 1024)
}
/// At the limit, forgets mappings whose task is neither on its board nor archived there.
/// The source may then recreate those deleted tasks; live and archived tasks keep their links.
fn prune_tombstones(
    connection: &rusqlite::Connection,
    instance: &PluginInstance,
    changes: &std::collections::HashMap<String, SourceChange>,
) -> Result<(), DomainError> {
    let repository = |e: rusqlite::Error| DomainError::repository(Box::new(e));
    // Board rows are written after sources, so this run's mappings are skipped by key.
    let tombstones = connection
        .prepare("SELECT s.board_id,s.external_key FROM cardbe_plugin_sources s WHERE s.instance_id=?1 AND NOT EXISTS(SELECT 1 FROM cardbe_tasks t WHERE t.board_id=s.board_id AND t.id=s.task_id) AND NOT EXISTS(SELECT 1 FROM cardbe_archives a WHERE a.board_id=s.board_id AND a.task_id=s.task_id)")
        .map_err(repository)?
        .query_map([&instance.id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(repository)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(repository)?;
    for (board_id, key) in tombstones {
        if board_id == instance.board_id && changes.contains_key(&key) {
            continue;
        }
        connection
            .execute(
                "DELETE FROM cardbe_plugin_sources WHERE instance_id=?1 AND board_id=?2 AND external_key=?3",
                params![instance.id, board_id, key],
            )
            .map_err(repository)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Column, ColumnSort};
    fn data() -> StoredData {
        StoredData {
            columns: vec![
                Column {
                    id: 1,
                    name: "Todo".into(),
                    color: String::new(),
                    sort_order: ColumnSort::Custom,
                    sort_rules: Vec::new(),
                    tasks: vec![],
                },
                Column {
                    id: 2,
                    name: "Done".into(),
                    color: String::new(),
                    sort_order: ColumnSort::Custom,
                    sort_rules: Vec::new(),
                    tasks: vec![],
                },
            ],
            ..StoredData::default()
        }
    }
    fn task() -> ExternalTask {
        ExternalTask {
            external_key: "source:1".into(),
            title: "Original".into(),
            description: "Body".into(),
            url: String::new(),
            state: "open".into(),
            labels: vec!["imported".into()],
            updated_at: String::new(),
            column_id: 1,
            priority: None,
        }
    }
    #[test]
    fn merge_is_idempotent_and_preserves_user_overrides() {
        let mut d = data();
        let mut t = task();
        let (l, _, _) = merge(&mut d, &t, None).unwrap();
        assert!(!merge(&mut d, &t, Some(&l)).unwrap().1);
        d.columns[0].tasks[0].title = "User title".into();
        d.columns[0].tasks[0].labels = vec!["personal".into()];
        let user = d.columns[0].tasks.remove(0);
        d.columns[1].tasks.push(user);
        t.title = "Changed".into();
        t.labels.push("new".into());
        let (l, _, _) = merge(&mut d, &t, Some(&l)).unwrap();
        assert_eq!(d.columns[1].tasks[0].title, "User title");
        assert_eq!(d.columns[1].tasks[0].labels, vec!["personal", "new"]);
        t.title = "User title".into();
        let (l, _, _) = merge(&mut d, &t, Some(&l)).unwrap();
        t.title = "Reclaim".into();
        merge(&mut d, &t, Some(&l)).unwrap();
        assert_eq!(d.columns[1].tasks[0].title, "User title");
    }
    #[test]
    fn source_priority_is_owned_until_the_user_changes_it() {
        use TaskPriority::{High, Low, Urgent};
        let mut d = data();
        let t = task();
        let (mut l, _, _) = merge(&mut d, &t, None).unwrap();
        assert!(merge_priority(&mut d, &mut l, Some(High)));
        assert_eq!(d.columns[0].tasks[0].priority, Some(High));
        // An upsert without a report keeps both the task and the imported value.
        l = merge(&mut d, &t, Some(&l)).unwrap().0;
        assert_eq!(l.imported.priority, Some(High));
        assert_eq!(d.columns[0].tasks[0].priority, Some(High));
        assert!(merge_priority(&mut d, &mut l, None));
        assert_eq!(d.columns[0].tasks[0].priority, None);

        d.columns[0].tasks[0].priority = Some(Low);
        assert!(!merge_priority(&mut d, &mut l, Some(Urgent)));
        assert!(!l.priority_owned);
        assert_eq!(d.columns[0].tasks[0].priority, Some(Low));
        // Matching values again never reclaims ownership.
        merge_priority(&mut d, &mut l, Some(Low));
        assert!(!merge_priority(&mut d, &mut l, Some(High)));
        assert_eq!(d.columns[0].tasks[0].priority, Some(Low));
    }
    #[test]
    fn legacy_links_own_priority_only_while_it_is_unset() {
        let mut d = data();
        let (l, _, _) = merge(&mut d, &task(), None).unwrap();
        let mut raw = serde_json::to_value(&l).unwrap();
        raw.as_object_mut().unwrap().remove("priority_owned");
        assert!(raw["imported"].get("priority").is_none());
        let legacy: SourceLink = serde_json::from_value(raw).unwrap();
        assert!(legacy.priority_owned);

        let mut unset = legacy.clone();
        assert!(merge_priority(&mut d, &mut unset, Some(TaskPriority::High)));
        d.columns[0].tasks[0].priority = Some(TaskPriority::Low);
        let mut user_set = legacy;
        assert!(!merge_priority(
            &mut d,
            &mut user_set,
            Some(TaskPriority::High)
        ));
        assert_eq!(d.columns[0].tasks[0].priority, Some(TaskPriority::Low));
    }
    #[test]
    fn removed_task_is_not_recreated() {
        let mut d = data();
        let t = task();
        let (l, _, _) = merge(&mut d, &t, None).unwrap();
        d.columns[0].tasks.clear();
        let (_, new, changed) = merge(&mut d, &t, Some(&l)).unwrap();
        assert!(!new && !changed);
        assert!(d.columns[0].tasks.is_empty());
    }
    #[test]
    fn preexisting_user_label_is_never_claimed() {
        let mut d = data();
        let mut t = task();
        let (mut l, _, _) = merge(&mut d, &t, None).unwrap();
        d.columns[0].tasks[0].labels.push("personal".into());
        t.labels.push("personal".into());
        l = merge(&mut d, &t, Some(&l)).unwrap().0;
        t.labels.clear();
        merge(&mut d, &t, Some(&l)).unwrap();
        assert_eq!(d.columns[0].tasks[0].labels, vec!["personal"]);
    }
}
#[cfg(test)]
mod persistence_tests {
    use super::*;
    fn fixture() -> (std::path::PathBuf, crate::storage::Database, PluginInstance) {
        let dir = std::env::temp_dir().join(format!("cardbe-plugin-test-{}", id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut loaded = crate::storage::load(&dir).unwrap();
        loaded.database.initialize_plugins().unwrap();
        let board = loaded.database.active_board_id();
        let data = StoredData {
            columns: vec![crate::models::Column {
                id: 1,
                name: "Todo".into(),
                color: String::new(),
                sort_order: Default::default(),
                sort_rules: Vec::new(),
                tasks: vec![],
            }],
            ..Default::default()
        };
        loaded
            .database
            .replace_board_as_local_edit(board, &data)
            .unwrap();
        let instance = PluginInstance {
            id: "instance-a".into(),
            plugin_id: "example".into(),
            name: "test".into(),
            board_id: board,
            config: Default::default(),
            secret_fields: vec![],
            allowed_domains: vec![],
            private_network_domains: vec![],
            pending_domain: None,
            enabled: true,
            needs_review: false,
            credentials_need_review: false,
            interval_seconds: 60,
            last_run_at: None,
            last_run_status: None,
            next_run_at: None,
            failures: 0,
            last_error: None,
            running: false,
        };
        storage::put(
            loaded.database.plugin_connection(),
            "instance",
            &instance.id,
            &instance,
        )
        .unwrap();
        (dir, loaded.database, instance)
    }
    struct TestOutput {
        tasks: Vec<ExternalTask>,
        cursor: Option<String>,
    }
    fn commit(
        db: &mut crate::storage::Database,
        instance: &PluginInstance,
        output: &TestOutput,
        may_commit: impl Fn() -> bool,
    ) -> Result<(usize, usize, StoredData), DomainError> {
        let run_id = id();
        let run: PluginRun = serde_json::from_value(serde_json::json!({"id":run_id,"instance_id":instance.id,"trigger":"manual","started_at":0,"status":"running","logs":[],"created":0,"updated":0})).unwrap();
        storage::put(db.plugin_connection(), "instance", &instance.id, instance)?;
        storage::put(db.plugin_connection(), "run", &run_id, &run)?;
        let output = super::super::runtime::RuntimeOutput {
            changeset: None,
            snapshot_version: super::super::board::version(db, instance.board_id)?,
            tasks: output.tasks.clone(),
            cursor: output.cursor.clone(),
            logs: vec![],
        };
        let changeset = super::super::board::prepare_output(db, instance.clone(), run_id, &output)?;
        let data = db.commit_changeset_if(&changeset, may_commit)?;
        Ok((
            changeset.created,
            changeset.updated,
            data.expect("source operations have board projection"),
        ))
    }
    fn output() -> TestOutput {
        TestOutput {
            tasks: vec![ExternalTask {
                external_key: "remote:1".into(),
                title: "Imported".into(),
                description: "Body".into(),
                url: String::new(),
                state: "open".into(),
                labels: vec![],
                updated_at: String::new(),
                column_id: 1,
                priority: None,
            }],
            cursor: Some("cursor-a".into()),
        }
    }
    #[test]
    fn commits_are_idempotent_and_transaction_failure_does_not_advance_cursor() {
        let (dir, mut db, i) = fixture();
        let mut output = output();
        let (created, updated, _) = commit(&mut db, &i, &output, || true).unwrap();
        assert_eq!((created, updated), (1, 0));
        let (created, updated, _) = commit(&mut db, &i, &output, || true).unwrap();
        assert_eq!((created, updated), (0, 0));
        let before = db.read_board_complete(i.board_id).unwrap();
        db.plugin_connection().execute_batch("CREATE TRIGGER fail_plugin_task BEFORE UPDATE OF payload ON cardbe_tasks BEGIN SELECT RAISE(ABORT,'test rollback'); END;").unwrap();
        output.tasks[0].title = "Changed".into();
        output.cursor = Some("cursor-b".into());
        assert!(commit(&mut db, &i, &output, || true).is_err());
        assert_eq!(db.read_board_complete(i.board_id).unwrap(), before);
        let cursors: Vec<Option<String>> = storage::list(db.plugin_connection(), "cursor").unwrap();
        assert_eq!(cursors, vec![Some("cursor-a".into())]);
        let raw: String = db
            .plugin_connection()
            .query_row("SELECT last_import FROM cardbe_plugin_sources", [], |r| {
                r.get(0)
            })
            .unwrap();
        let link: SourceLink = serde_json::from_str(&raw).unwrap();
        assert_eq!(link.imported.title, "Imported");
        db.plugin_connection().execute_batch("DROP TRIGGER fail_plugin_task; CREATE TRIGGER fail_plugin_cursor BEFORE UPDATE ON cardbe_plugin_data WHEN NEW.kind='cursor' BEGIN SELECT RAISE(ABORT,'cursor rollback'); END;").unwrap();
        let loro_before: Vec<u8> = db
            .plugin_connection()
            .query_row(
                "SELECT payload FROM cardbe_loro_docs WHERE board_id=?1",
                [i.board_id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(commit(&mut db, &i, &output, || true).is_err());
        assert_eq!(db.read_board_complete(i.board_id).unwrap(), before);
        let loro_after: Vec<u8> = db
            .plugin_connection()
            .query_row(
                "SELECT payload FROM cardbe_loro_docs WHERE board_id=?1",
                [i.board_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(loro_before, loro_after);
        let raw_after: String = db
            .plugin_connection()
            .query_row("SELECT last_import FROM cardbe_plugin_sources", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(raw, raw_after);
        let cursors: Vec<Option<String>> = storage::list(db.plugin_connection(), "cursor").unwrap();
        assert_eq!(cursors, vec![Some("cursor-a".into())]);
        db.plugin_connection()
            .execute_batch("DROP TRIGGER fail_plugin_cursor;")
            .unwrap();
        let checks = std::cell::Cell::new(0);
        assert!(commit(&mut db, &i, &output, || {
            checks.set(checks.get() + 1);
            checks.get() < 2
        })
        .is_err());
        assert_eq!(db.read_board_complete(i.board_id).unwrap(), before);
        let unchanged: Vec<u8> = db
            .plugin_connection()
            .query_row(
                "SELECT payload FROM cardbe_loro_docs WHERE board_id=?1",
                [i.board_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(loro_before, unchanged);
        let cursors: Vec<Option<String>> = storage::list(db.plugin_connection(), "cursor").unwrap();
        assert_eq!(cursors, vec![Some("cursor-a".into())]);
        let (created, updated, _) = commit(&mut db, &i, &output, || true).unwrap();
        assert_eq!((created, updated), (0, 1));
        assert_eq!(
            db.read_board_complete(i.board_id).unwrap().columns[0].tasks[0].title,
            "Changed"
        );
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn invalid_later_task_rolls_back_earlier_source_and_cursor() {
        let (dir, mut db, i) = fixture();
        let mut out = output();
        let mut invalid = out.tasks[0].clone();
        invalid.external_key = "remote:2".into();
        invalid.title = " ".into();
        out.tasks.push(invalid);
        assert!(matches!(
            commit(&mut db, &i, &out, || true),
            Err(DomainError::InvalidArgument)
        ));
        assert!(db.read_board_complete(i.board_id).unwrap().columns[0]
            .tasks
            .is_empty());
        let count: i64 = db
            .plugin_connection()
            .query_row("SELECT count(*) FROM cardbe_plugin_sources", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
        assert!(
            storage::list::<Option<String>>(db.plugin_connection(), "cursor")
                .unwrap()
                .is_empty()
        );
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn instance_collision_and_viewer_permission_fail_before_cursor_commit() {
        let (dir, mut db, i) = fixture();
        let out = output();
        commit(&mut db, &i, &out, || true).unwrap();
        let mut other = i.clone();
        other.id = "instance-b".into();
        storage::put(db.plugin_connection(), "instance", &other.id, &other).unwrap();
        assert!(commit(&mut db, &other, &out, || true).is_err());
        db.plugin_connection()
            .execute(
                "UPDATE cardbe_boards SET shared_role='viewer' WHERE id=?1",
                [i.board_id],
            )
            .unwrap();
        assert!(matches!(
            commit(&mut db, &i, &out, || true),
            Err(DomainError::PermissionDenied)
        ));
        let count: i64 = db
            .plugin_connection()
            .query_row("SELECT count(*) FROM cardbe_plugin_sources", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn changed_board_uses_separate_source_mapping_and_preserves_tombstones() {
        let (dir, mut db, mut i) = fixture();
        let out = output();
        commit(&mut db, &i, &out, || true).unwrap();
        let mut data = db.read_board_complete(i.board_id).unwrap();
        data.columns[0].tasks.clear();
        db.replace_board_as_local_edit(i.board_id, &data).unwrap();
        let (created, updated, _) = commit(&mut db, &i, &out, || true).unwrap();
        assert_eq!((created, updated), (0, 0));
        let second = db.create_board_with_data("Second", &data).unwrap();
        i.board_id = second.id;
        let (created, updated, _) = commit(&mut db, &i, &out, || true).unwrap();
        assert_eq!((created, updated), (1, 0));
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
