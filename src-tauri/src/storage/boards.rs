use super::*;

impl Database {
    pub fn active_board_id(&self) -> i64 {
        self.active_board_id
    }

    pub fn boards(&self) -> StorageResult<Vec<Board>> {
        Ok(self
            .connection
            .prepare(
                "SELECT b.id, b.name, COUNT(t.id), b.shared_role, b.sync_status, b.sync_revision,
                        b.shared_role <> 'owner' OR EXISTS(SELECT 1 FROM cardbe_iroh_invites i WHERE i.board_id=b.id)
                 FROM cardbe_boards b
                 LEFT JOIN cardbe_tasks t ON t.board_id = b.id
                 GROUP BY b.id, b.name, b.shared_role, b.sync_status, b.sync_revision
                 ORDER BY b.id",
            )?
            .query_map([], |row| {
                Ok(Board {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    task_count: row.get(2)?,
                    shared_role: row.get(3)?,
                    sync_status: row.get(4)?,
                    sync_revision: row.get(5)?,
                    is_shared: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?)
    }

    /// Every board's columns in board order, with their task counts.
    pub fn column_overviews(&self) -> StorageResult<Vec<ColumnOverview>> {
        Ok(self
            .connection
            .prepare(
                "SELECT c.board_id, c.id, c.name, c.done, COUNT(t.id)
                 FROM cardbe_columns c
                 LEFT JOIN cardbe_tasks t ON t.board_id = c.board_id AND t.column_id = c.id
                 GROUP BY c.board_id, c.id
                 ORDER BY c.board_id, c.position",
            )?
            .query_map([], |row| {
                Ok(ColumnOverview {
                    board_id: row.get(0)?,
                    id: row.get(1)?,
                    name: row.get(2)?,
                    done: row.get(3)?,
                    task_count: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn initialize_boards(
        &mut self,
        legacy: &StoredData,
    ) -> StorageResult<(Vec<Board>, StoredData)> {
        self.migrate_multiboard_schema(legacy)?;
        let boards = self.boards()?;
        if let Some(board) = boards.first() {
            let requested_active_board_id =
                namespaced_metadata_parse::<i64>(&self.connection, "active_board_id")?;
            let id = requested_active_board_id
                .filter(|id| boards.iter().any(|board| board.id == *id))
                .unwrap_or(board.id);
            let stored = self.load_board(id)?;
            if requested_active_board_id != Some(id) {
                log::warn!(
                    target: "storage.database",
                    "Stored active board {:?} was not selected; using active_board_id={} board_count={}",
                    requested_active_board_id,
                    id,
                    boards.len(),
                );
            }
            log::debug!(
                target: "storage.database",
                "Boards initialized board_count={} active_board_id={} schema_version={} current_schema_version={}",
                boards.len(),
                id,
                stored.schema_version,
                CURRENT_SCHEMA_VERSION,
            );
            return Ok((boards, stored));
        }
        self.connection
            .execute("INSERT INTO cardbe_boards(name) VALUES ('My board')", [])?;
        let id = self.connection.last_insert_rowid();
        self.write_board(id, legacy)?;
        self.set_active_board_id(id)?;
        Ok((
            vec![Board {
                id,
                name: "My board".into(),
                task_count: legacy
                    .columns
                    .iter()
                    .map(|column| column.tasks.len() as i64)
                    .sum(),
                shared_role: BoardRole::Owner,
                is_shared: false,
                sync_status: SyncStatus::Local,
                sync_revision: 0,
            }],
            legacy.clone(),
        ))
    }

    pub fn load_board(&mut self, id: i64) -> StorageResult<StoredData> {
        let stored = self.load_board_data(id, true)?;
        self.positions = self.load_board_positions(id)?;
        self.set_active_board_id(id)?;
        Ok(stored)
    }

    /// Read a board without changing the active-board metadata or cached positions.
    pub fn read_board(&self, id: i64) -> StorageResult<StoredData> {
        self.load_board_data(id, false)
    }

    /// Read every board-owned collection without changing active state.
    pub fn read_board_complete(&self, id: i64) -> StorageResult<StoredData> {
        self.load_board_data(id, true)
    }

    /// Run `read` inside one read transaction so its queries observe a single
    /// committed state. Under WAL this does not block writers.
    pub fn read_consistent<R>(&self, read: impl FnOnce(&Self) -> R) -> StorageResult<R> {
        let _transaction = self.connection.unchecked_transaction()?;
        Ok(read(self))
    }

    pub fn load_board_archives_for_export(&self, id: i64) -> StorageResult<Vec<Archive>> {
        self.load_board_archives(id)
    }

    pub fn replace_all_boards(
        &mut self,
        boards: &[(String, StoredData, crate::flow::FlowData)],
        active_index: usize,
        notes: &[Note],
        settings: &crate::models::Settings,
        focuses: Option<&[crate::focuses::Focus]>,
    ) -> StorageResult<(Vec<Board>, StoredData)> {
        if boards.is_empty() || active_index >= boards.len() {
            return Err("Backup must contain an active board".into());
        }
        let tx = self.connection.transaction()?;
        tx.execute("DELETE FROM cardbe_boards", [])?;
        tx.execute("DELETE FROM notes", [])?;
        let mut created = Vec::with_capacity(boards.len());
        for (name, data, flow) in boards {
            tx.execute("INSERT INTO cardbe_boards(name) VALUES(?1)", [name])?;
            let id = tx.last_insert_rowid();
            write_board_transaction(&tx, id, data)?;
            crate::flow::restore(&tx, id, flow)?;
            created.push(Board {
                id,
                name: name.clone(),
                task_count: data
                    .columns
                    .iter()
                    .map(|column| column.tasks.len() as i64)
                    .sum(),
                shared_role: BoardRole::Owner,
                is_shared: false,
                sync_status: SyncStatus::Local,
                sync_revision: 0,
            });
        }
        for note in notes {
            tx.execute("INSERT INTO notes(id,title,content,pinned,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6)", params![note.id, note.title, note.content, note.pinned, note.created_at, note.updated_at])?;
        }
        let active_id = created[active_index].id;
        tx.execute("INSERT INTO cardbe_global_metadata(key,value) VALUES('active_board_id',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [active_id.to_string()])?;
        tx.execute("INSERT INTO cardbe_global_metadata(key,value) VALUES('settings',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [serde_json::to_string(settings)?])?;
        if let Some(views) = focuses {
            // Backups refer to boards by position; restored boards get new IDs.
            let mut views = views.to_vec();
            crate::focuses::map_boards(&mut views, |index| {
                usize::try_from(index)
                    .ok()
                    .and_then(|index| created.get(index))
                    .map(|board| board.id)
            });
            crate::focuses::save(&tx, &views)?;
        }
        tx.commit()?;
        self.active_board_id = active_id;
        self.loro_peers.clear();
        self.positions = self.load_board_positions(active_id)?;
        let active = self.load_board_data(active_id, true)?;
        Ok((created, active))
    }

    pub fn create_board(&mut self, name: &str) -> StorageResult<Board> {
        // Board creation writes both the row and its scoped metadata. Keep
        // those writes in the same transaction so a storage error cannot
        // leave a selectable, half-initialized board behind.
        self.create_board_with_data(name, &StoredData::default())
    }

    pub fn board_exists(&self, id: i64) -> StorageResult<bool> {
        Ok(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM cardbe_boards WHERE id=?1)",
            [id],
            |row| row.get(0),
        )?)
    }

    pub fn board_role(&self, id: i64) -> StorageResult<BoardRole> {
        self.connection
            .query_row(
                "SELECT shared_role FROM cardbe_boards WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| DomainError::BoardNotFound.into())
    }

    #[cfg(test)]
    pub fn set_shared_board(
        &mut self,
        id: i64,
        role: BoardRole,
        status: SyncStatus,
        revision: i64,
    ) -> StorageResult<()> {
        if self.connection.execute(
            "UPDATE cardbe_boards SET shared_role=?2,sync_status=?3,sync_revision=?4 WHERE id=?1",
            params![id, role, status, revision],
        )? == 0
        {
            return Err(DomainError::BoardNotFound.into());
        }
        Ok(())
    }

    pub fn board_sync_state(&self, id: i64) -> StorageResult<(i64, SyncStatus)> {
        Ok(self.connection.query_row(
            "SELECT sync_revision,sync_status FROM cardbe_boards WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    }

    /// Every existing column as `(board ID, column ID)`.
    pub fn column_ids(&self) -> StorageResult<std::collections::HashSet<(i64, i64)>> {
        let mut stmt = self
            .connection
            .prepare("SELECT board_id,id FROM cardbe_columns")?;
        let ids = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        Ok(ids)
    }

    pub fn copy_task_template_to_board(
        &mut self,
        source_id: i64,
        template_id: i64,
        target_id: i64,
    ) -> StorageResult<()> {
        if !self.board_exists(source_id)? || !self.board_exists(target_id)? {
            return Err(DomainError::BoardNotFound.into());
        }
        if source_id == target_id {
            return Err(DomainError::InvalidArgument.into());
        }
        let source = self.read_board_complete(source_id)?;
        let mut template = source
            .templates
            .into_iter()
            .find(|template| template.id == template_id)
            .ok_or(DomainError::TemplateNotFound)?;
        let mut target = self.read_board_complete(target_id)?;
        template.id = target.allocate_template_id()?;
        target.templates.push(template);
        self.replace_board_as_local_edit(target_id, &target)?;
        Ok(())
    }

    /// Moves the tasks in source board order; any missing task aborts the move.
    pub fn move_tasks_to_board(
        &mut self,
        source_id: i64,
        task_ids: &[i64],
        target_id: i64,
        column_id: i64,
    ) -> StorageResult<StoredData> {
        if !self.board_exists(source_id)? || !self.board_exists(target_id)? {
            return Err(DomainError::BoardNotFound.into());
        }
        if source_id == target_id {
            return Err("Choose a different board".into());
        }
        let before_source = self.read_board_complete(source_id)?;
        let before_target = self.read_board_complete(target_id)?;
        let mut source = before_source.clone();
        let mut target = before_target.clone();
        let column = target
            .columns
            .iter()
            .position(|c| c.id == column_id)
            .ok_or(DomainError::ColumnNotFound)?;
        let selected: std::collections::HashSet<i64> = task_ids.iter().copied().collect();
        let mut moved = Vec::with_capacity(selected.len());
        for source_column in &mut source.columns {
            let (tasks, kept): (Vec<Task>, Vec<Task>) = std::mem::take(&mut source_column.tasks)
                .into_iter()
                .partition(|task| selected.contains(&task.id));
            source_column.tasks = kept;
            moved.extend(tasks);
        }
        if moved.len() != selected.len() {
            return Err(DomainError::TaskNotFound.into());
        }
        for mut task in moved {
            task.id = target.allocate_task_id()?;
            // Tasks only wait on tasks of their own board.
            task.blocked_by.clear();
            for label in &task.labels {
                target.touch_label(label.clone());
            }
            target.columns[column].tasks.push(task);
        }
        let now = crate::commands::archive::current_time_millis()?;
        crate::completion::reconcile(&before_target, &mut target, now)?;
        target.keep_pinned_tasks_first();
        let tx = self.connection.transaction()?;
        for (id, before, after) in [
            (source_id, &before_source, &source),
            (target_id, &before_target, &target),
        ] {
            let role: BoardRole = tx.query_row(
                "SELECT shared_role FROM cardbe_boards WHERE id=?1",
                [id],
                |r| r.get(0),
            )?;
            if role == BoardRole::Viewer {
                return Err(DomainError::PermissionDenied.into());
            }
            write_board_transaction(&tx, id, after)?;
            mark_local_board_change(&tx, id, role)?;
            persist_loro_local_delta(
                &tx,
                id,
                before,
                after,
                self.loro_peers.entry(id).or_default(),
            )?;
            task_events::record_transfer(&tx, id, before, after)?;
        }
        // Moved tasks leave their plugin sources, which recreate them on the source board.
        if table_exists(&tx, "cardbe_plugin_sources")? {
            for task_id in task_ids {
                tx.execute(
                    "DELETE FROM cardbe_plugin_sources WHERE board_id=?1 AND task_id=?2",
                    params![source_id, task_id],
                )?;
            }
        }
        tx.commit()?;
        self.positions = self.load_board_positions(self.active_board_id)?;
        Ok(source)
    }

    pub fn replace_board_as_local_edit(
        &mut self,
        board_id: i64,
        data: &StoredData,
    ) -> StorageResult<(i64, SyncStatus)> {
        Ok(self.replace_board_as_local_edit_recorded(board_id, data)?.0)
    }

    /// Like [`Self::replace_board_as_local_edit`], also returning the task
    /// event operation it recorded.
    pub fn replace_board_as_local_edit_recorded(
        &mut self,
        board_id: i64,
        data: &StoredData,
    ) -> StorageResult<((i64, SyncStatus), Option<i64>)> {
        // The Loro delta must compare with the persisted board, not an empty
        // value: otherwise removals are absent from the CRDT and reappear on
        // the next editor sync.
        let before = self.read_board_complete(board_id)?;
        let tx = self.connection.transaction()?;
        let role: BoardRole = tx.query_row(
            "SELECT shared_role FROM cardbe_boards WHERE id=?1",
            [board_id],
            |r| r.get(0),
        )?;
        if role == BoardRole::Viewer {
            return Err(DomainError::PermissionDenied.into());
        }
        write_board_transaction(&tx, board_id, data)?;
        mark_local_board_change(&tx, board_id, role)?;
        persist_loro_local_delta(
            &tx,
            board_id,
            &before,
            data,
            self.loro_peers.entry(board_id).or_default(),
        )?;
        let event_op = task_events::record(&tx, board_id, &before, data, &EventSource::local())?;
        tx.commit()?;
        if self.active_board_id == board_id {
            self.positions = self.load_board_positions(board_id)?;
        }
        Ok((self.board_sync_state(board_id)?, event_op))
    }

    /// Create and populate a board atomically.
    pub fn create_board_with_data(
        &mut self,
        name: &str,
        data: &StoredData,
    ) -> StorageResult<Board> {
        let tx = self.connection.transaction()?;
        tx.execute("INSERT INTO cardbe_boards(name) VALUES (?1)", [name])?;
        let id = tx.last_insert_rowid();
        write_board_transaction(&tx, id, data)?;
        persist_loro_local_delta(
            &tx,
            id,
            &StoredData::default(),
            data,
            self.loro_peers.entry(id).or_default(),
        )?;
        tx.commit()?;
        Ok(Board {
            id,
            name: name.into(),
            task_count: data
                .columns
                .iter()
                .map(|column| column.tasks.len() as i64)
                .sum(),
            shared_role: BoardRole::Owner,
            is_shared: false,
            sync_status: SyncStatus::Local,
            sync_revision: 0,
        })
    }

    pub fn rename_board(&mut self, id: i64, name: &str) -> StorageResult<()> {
        let tx = self.connection.transaction()?;
        let role: BoardRole = tx.query_row(
            "SELECT shared_role FROM cardbe_boards WHERE id=?1",
            [id],
            |r| r.get(0),
        )?;
        if role != BoardRole::Owner {
            return Err(DomainError::PermissionDenied.into());
        }
        tx.execute(
            "UPDATE cardbe_boards SET name=?2 WHERE id=?1",
            params![id, name],
        )?;
        mark_local_board_change(&tx, id, role)?;
        tx.commit()?;
        Ok(())
    }

    pub fn create_received_board(
        &mut self,
        name: &str,
        data: &StoredData,
        role: IrohPermission,
        revision: i64,
        invitation: &str,
        loro_update: Option<&[u8]>,
    ) -> StorageResult<Board> {
        validate_iroh_snapshot(data, role, loro_update)?;
        let tx = self.connection.transaction()?;
        tx.execute("INSERT INTO cardbe_boards(name,shared_role,sync_status,sync_revision) VALUES(?1,?2,?3,?4)", params![name, role, SyncStatus::Synced, revision])?;
        let id = tx.last_insert_rowid();
        write_board_transaction(&tx, id, data)?;
        if let Some(update) = loro_update.filter(|update| !update.is_empty()) {
            tx.execute(
                "INSERT INTO cardbe_loro_docs(board_id,payload) VALUES(?1,?2)",
                params![id, update],
            )?;
        }
        tx.execute(
            "INSERT INTO cardbe_iroh_remotes(board_id,invitation) VALUES(?1,?2)",
            params![id, invitation],
        )?;
        tx.commit()?;
        Ok(Board {
            id,
            name: name.into(),
            task_count: data
                .columns
                .iter()
                .map(|column| column.tasks.len() as i64)
                .sum(),
            shared_role: role.into(),
            is_shared: true,
            sync_status: SyncStatus::Synced,
            sync_revision: revision,
        })
    }

    pub fn delete_board(&mut self, id: i64) -> StorageResult<()> {
        let tx = self.connection.transaction()?;
        if tx.execute("DELETE FROM cardbe_boards WHERE id = ?1", [id])? == 0 {
            return Err(DomainError::BoardNotFound.into());
        }
        // Plugin source mappings have no foreign key and the table is created lazily.
        if table_exists(&tx, "cardbe_plugin_sources")? {
            tx.execute("DELETE FROM cardbe_plugin_sources WHERE board_id=?1", [id])?;
        }
        tx.commit()?;
        self.loro_peers.remove(&id);
        Ok(())
    }

    pub fn archived_task_count(&self, board_id: i64) -> StorageResult<i64> {
        Ok(self.connection.query_row(
            "SELECT COUNT(*) FROM cardbe_archives WHERE board_id=?1",
            [board_id],
            |row| row.get(0),
        )?)
    }

    pub(super) fn set_active_board_id(&mut self, id: i64) -> StorageResult<()> {
        self.connection.execute(
            "INSERT INTO cardbe_global_metadata(key, value) VALUES ('active_board_id', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [id.to_string()],
        )?;
        self.active_board_id = id;
        Ok(())
    }

    pub(super) fn load_board_data(
        &self,
        board_id: i64,
        include_archives: bool,
    ) -> StorageResult<StoredData> {
        let mut stored = StoredData::default();
        let mut stmt=self.connection.prepare("SELECT id,name,color,sort_order,sort_rules,done FROM cardbe_columns WHERE board_id=?1 ORDER BY position,id")?;
        stored.columns = stmt
            .query_map([board_id], column_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        let indexes = stored
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| (c.id, i))
            .collect::<HashMap<_, _>>();
        let mut stmt=self.connection.prepare("SELECT column_id,payload FROM cardbe_tasks WHERE board_id=?1 ORDER BY column_id,position,id")?;
        for row in stmt.query_map([board_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })? {
            let (column_id, payload) = row?;
            let task: Task = serde_json::from_str(&payload)?;
            let index = indexes.get(&column_id).ok_or_else(|| {
                format!(
                    "Board {board_id} task {} refers to missing column {column_id}",
                    task.id
                )
            })?;
            stored.columns[*index].tasks.push(task);
        }
        if include_archives {
            stored.archives = self.load_board_archives(board_id)?;
        }
        let mut stmt = self.connection.prepare(
            "SELECT id,name,payload FROM cardbe_templates WHERE board_id=?1 ORDER BY position,id",
        )?;
        stored.templates = stmt
            .query_map([board_id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .map(|row| {
                let (id, name, payload) = row?;
                Ok(TaskTemplate {
                    id,
                    name,
                    task: serde_json::from_str(&payload).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            2,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                })
            })
            .collect::<Result<Vec<_>, rusqlite::Error>>()?;
        stored.schema_version = board_metadata_parse(&self.connection, board_id, "schema_version")?
            .unwrap_or(CURRENT_SCHEMA_VERSION);
        stored.next_column_id =
            board_metadata_parse(&self.connection, board_id, "next_column_id")?.unwrap_or_default();
        stored.next_task_id =
            board_metadata_parse(&self.connection, board_id, "next_task_id")?.unwrap_or_default();
        stored.next_template_id =
            board_metadata_parse(&self.connection, board_id, "next_template_id")?
                .unwrap_or_default();
        stored.label_recency =
            board_metadata_json(&self.connection, board_id, "label_recency")?.unwrap_or_default();
        stored.settings =
            namespaced_metadata_json(&self.connection, "settings")?.unwrap_or_default();
        Ok(stored)
    }

    pub(super) fn load_board_archives(&self, board_id: i64) -> StorageResult<Vec<Archive>> {
        let mut stmt=self.connection.prepare("SELECT archived_at,payload FROM cardbe_archives WHERE board_id=?1 ORDER BY position,task_id")?;
        let result = stmt
            .query_map([board_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .map(|row| {
                let (time, payload) = row?;
                Ok(Archive {
                    time: time.parse()?,
                    task: serde_json::from_str(&payload)?,
                })
            })
            .collect();
        result
    }

    pub fn load_archive_task(&self, board_id: i64, task_id: i64) -> StorageResult<Option<Task>> {
        self.connection
            .query_row(
                "SELECT payload FROM cardbe_archives WHERE board_id=?1 AND task_id=?2",
                params![board_id, task_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|payload| serde_json::from_str(&payload).map_err(Into::into))
            .transpose()
    }

    /// Loads archived tasks in request order, skipping duplicates and missing IDs.
    pub fn load_archive_tasks(&self, board_id: i64, task_ids: &[i64]) -> StorageResult<Vec<Task>> {
        let mut statement = self
            .connection
            .prepare("SELECT payload FROM cardbe_archives WHERE board_id=?1 AND task_id=?2")?;
        let mut seen = std::collections::HashSet::new();
        let mut tasks = Vec::new();
        for task_id in task_ids {
            if !seen.insert(*task_id) {
                continue;
            }
            if let Some(payload) = statement
                .query_row(params![board_id, task_id], |row| row.get::<_, String>(0))
                .optional()?
            {
                tasks.push(serde_json::from_str(&payload)?);
            }
        }
        Ok(tasks)
    }
}
