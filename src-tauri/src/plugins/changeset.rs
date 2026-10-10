use super::{
    runtime::{ExternalTask, RuntimeOutput},
    storage,
    types::*,
};
use crate::{
    errors::DomainError,
    models::{StoredData, Task},
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
            next.imported = incoming.clone();
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
        start_time: now() as u128,
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
    pub(crate) fn commit_plugin_output(
        &mut self,
        instance: &PluginInstance,
        output: &RuntimeOutput,
        may_commit: impl Fn() -> bool,
    ) -> Result<(usize, usize, StoredData), DomainError> {
        if !may_commit() {
            return Err(DomainError::PermissionDenied);
        }
        if output.tasks.len() > 2000
            || output.cursor.as_ref().is_some_and(|c| c.len() > 4096)
            || serde_json::to_vec(&output.tasks)
                .map_err(|e| DomainError::Internal(e.to_string()))?
                .len()
                > 8 * 1024 * 1024
        {
            return Err(DomainError::InvalidArgument);
        }
        self.board_role(instance.board_id)
            .map_err(DomainError::repository)?;
        let before = self
            .read_board_complete(instance.board_id)
            .map_err(DomainError::repository)?;
        let mut after = before.clone();
        let mut positions = self
            .load_board_positions(instance.board_id)
            .map_err(DomainError::repository)?;
        let tx = self
            .connection
            .transaction()
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let role: crate::models::BoardRole = tx
            .query_row(
                "SELECT shared_role FROM cardbe_boards WHERE id=?1",
                [instance.board_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| DomainError::Internal(e.to_string()))?
            .ok_or(DomainError::BoardNotFound)?;
        if role == crate::models::BoardRole::Viewer {
            return Err(DomainError::PermissionDenied);
        }
        let mut created = 0;
        let mut updated = 0;
        let mut seen = std::collections::HashSet::new();
        for task in &output.tasks {
            if !seen.insert(&task.external_key) {
                continue;
            }
            let collision:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM cardbe_plugin_sources s JOIN cardbe_plugin_data i ON i.kind='instance' AND i.id=s.instance_id WHERE s.board_id=?1 AND s.external_key=?2 AND s.instance_id<>?3 AND json_extract(i.payload,'$.plugin_id')=?4)",params![instance.board_id,task.external_key,instance.id,instance.plugin_id],|r|r.get(0)).map_err(|e|DomainError::Internal(e.to_string()))?;
            if collision {
                return Err(DomainError::InvalidArgument);
            }
            let raw:Option<String>=tx.query_row("SELECT last_import FROM cardbe_plugin_sources WHERE instance_id=?1 AND board_id=?2 AND external_key=?3",params![instance.id,instance.board_id,task.external_key],|r|r.get(0)).optional().map_err(|e|DomainError::Internal(e.to_string()))?;
            let link: Option<SourceLink> = raw
                .map(|s| serde_json::from_str(&s))
                .transpose()
                .map_err(|e| DomainError::Internal(e.to_string()))?;
            let (link, new, changed) = merge(&mut after, task, link.as_ref())?;
            created += usize::from(new);
            updated += usize::from(changed);
            tx.execute("INSERT INTO cardbe_plugin_sources(instance_id,external_key,board_id,task_id,last_import) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(instance_id,board_id,external_key) DO UPDATE SET last_import=excluded.last_import",params![instance.id,task.external_key,instance.board_id,link.task_id,serde_json::to_string(&link).map_err(|e|DomainError::Internal(e.to_string()))?]).map_err(|e|DomainError::Internal(e.to_string()))?;
        }
        let source_count: i64 = tx
            .query_row(
                "SELECT count(*) FROM cardbe_plugin_sources WHERE instance_id=?1",
                [&instance.id],
                |r| r.get(0),
            )
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let source_bytes:i64=tx.query_row("SELECT coalesce(sum(length(CAST(last_import AS BLOB))),0) FROM cardbe_plugin_sources WHERE instance_id=?1",[&instance.id],|r|r.get(0)).map_err(|e|DomainError::Internal(e.to_string()))?;
        if source_count > 20000 || source_bytes > 32 * 1024 * 1024 {
            return Err(DomainError::InvalidArgument);
        }
        after.sort_column_tasks();
        crate::storage::persist_board_diff_transaction(
            &tx,
            instance.board_id,
            &before,
            &after,
            &mut positions,
            true,
        )
        .map_err(DomainError::repository)?;
        if before != after {
            crate::storage::mark_local_board_change(&tx, instance.board_id, role)
                .map_err(DomainError::repository)?;
            crate::storage::persist_loro_local_delta(
                &tx,
                instance.board_id,
                &before,
                &after,
                self.loro_peers.entry(instance.board_id).or_default(),
            )
            .map_err(DomainError::repository)?;
        }
        storage::put(&tx, "cursor", &instance.id, &output.cursor)?;
        if !may_commit() {
            return Err(DomainError::PermissionDenied);
        }
        tx.commit()
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        if self.active_board_id() == instance.board_id {
            self.positions = positions;
        }
        Ok((created, updated, after))
    }
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
                    tasks: vec![],
                },
                Column {
                    id: 2,
                    name: "Done".into(),
                    color: String::new(),
                    sort_order: ColumnSort::Custom,
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
            enabled: true,
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
    fn output() -> RuntimeOutput {
        RuntimeOutput {
            tasks: vec![ExternalTask {
                external_key: "remote:1".into(),
                title: "Imported".into(),
                description: "Body".into(),
                url: String::new(),
                state: "open".into(),
                labels: vec![],
                updated_at: String::new(),
                column_id: 1,
            }],
            cursor: Some("cursor-a".into()),
            logs: vec![],
        }
    }
    #[test]
    fn commits_are_idempotent_and_transaction_failure_does_not_advance_cursor() {
        let (dir, mut db, i) = fixture();
        let mut output = output();
        let (created, updated, _) = db.commit_plugin_output(&i, &output, || true).unwrap();
        assert_eq!((created, updated), (1, 0));
        let (created, updated, _) = db.commit_plugin_output(&i, &output, || true).unwrap();
        assert_eq!((created, updated), (0, 0));
        let before = db.read_board_complete(i.board_id).unwrap();
        db.plugin_connection().execute_batch("CREATE TRIGGER fail_plugin_task BEFORE UPDATE OF payload ON cardbe_tasks BEGIN SELECT RAISE(ABORT,'test rollback'); END;").unwrap();
        output.tasks[0].title = "Changed".into();
        output.cursor = Some("cursor-b".into());
        assert!(db.commit_plugin_output(&i, &output, || true).is_err());
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
        assert!(db.commit_plugin_output(&i, &output, || true).is_err());
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
        assert!(db
            .commit_plugin_output(&i, &output, || {
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
        let (created, updated, _) = db.commit_plugin_output(&i, &output, || true).unwrap();
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
            db.commit_plugin_output(&i, &out, || true),
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
        db.commit_plugin_output(&i, &out, || true).unwrap();
        let mut other = i.clone();
        other.id = "instance-b".into();
        storage::put(db.plugin_connection(), "instance", &other.id, &other).unwrap();
        assert!(db.commit_plugin_output(&other, &out, || true).is_err());
        db.plugin_connection()
            .execute(
                "UPDATE cardbe_boards SET shared_role='viewer' WHERE id=?1",
                [i.board_id],
            )
            .unwrap();
        assert!(matches!(
            db.commit_plugin_output(&i, &out, || true),
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
        db.commit_plugin_output(&i, &out, || true).unwrap();
        let mut data = db.read_board_complete(i.board_id).unwrap();
        data.columns[0].tasks.clear();
        db.replace_board_as_local_edit(i.board_id, &data).unwrap();
        let (created, updated, _) = db.commit_plugin_output(&i, &out, || true).unwrap();
        assert_eq!((created, updated), (0, 0));
        let second = db.create_board_with_data("Second", &data).unwrap();
        i.board_id = second.id;
        let (created, updated, _) = db.commit_plugin_output(&i, &out, || true).unwrap();
        assert_eq!((created, updated), (1, 0));
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
