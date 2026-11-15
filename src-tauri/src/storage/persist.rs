use super::*;

impl Database {
    pub fn persist_diff(
        &mut self,
        before: &StoredData,
        after: &StoredData,
    ) -> StorageResult<PersistStats> {
        self.persist_diff_with_events(
            before,
            after,
            true,
            EventWrite::Record(EventSource::local()),
        )
    }

    #[cfg(test)]
    pub fn persist_diff_without_archives(
        &mut self,
        before: &StoredData,
        after: &StoredData,
    ) -> StorageResult<PersistStats> {
        self.persist_diff_with_events(
            before,
            after,
            false,
            EventWrite::Record(EventSource::local()),
        )
    }

    /// Persists the active board. Without loaded archives, `before` and
    /// `after` hold none, so archiving and restoring cannot occur.
    pub fn persist_diff_with_events(
        &mut self,
        before: &StoredData,
        after: &StoredData,
        include_archives: bool,
        events: EventWrite,
    ) -> StorageResult<PersistStats> {
        let mut positions = self.positions.clone();
        let board_changed = before.columns != after.columns
            || before.archives != after.archives
            || before.templates != after.templates
            || before.label_recency != after.label_recency
            || before.next_column_id != after.next_column_id
            || before.next_task_id != after.next_task_id
            || before.next_template_id != after.next_template_id
            || before.schema_version != after.schema_version;
        // Only a missing document needs the complete archive projection.
        let needs_archive_seed = !include_archives
            && board_changed
            && !self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM cardbe_loro_docs WHERE board_id=?1)",
                [self.active_board_id],
                |row| row.get::<_, bool>(0),
            )?;
        let archives = if needs_archive_seed {
            Some(self.load_board_archives(self.active_board_id)?)
        } else {
            None
        };
        let transaction = self.connection.transaction()?;
        if self.active_board_id == 0 {
            return Err("No active board is selected".into());
        }
        let role: BoardRole = transaction.query_row(
            "SELECT shared_role FROM cardbe_boards WHERE id=?1",
            [self.active_board_id],
            |r| r.get(0),
        )?;
        if board_changed && role == BoardRole::Viewer {
            return Err(DomainError::PermissionDenied.into());
        }
        let mut stats = persist_board_diff_transaction(
            &transaction,
            self.active_board_id,
            before,
            after,
            &mut positions,
            include_archives,
        )?;
        if board_changed {
            stats.event_op = match &events {
                EventWrite::Record(source) => {
                    task_events::record(&transaction, self.active_board_id, before, after, source)?
                }
                EventWrite::Reset(source) => {
                    task_events::record_reset(&transaction, self.active_board_id, source)?
                }
            };
        }
        // Settings are global application preferences, not board content.
        transaction.execute(
            "INSERT INTO cardbe_global_metadata(key,value) VALUES ('settings',?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [serde_json::to_string(&after.settings)?],
        )?;
        if board_changed {
            mark_local_board_change(&transaction, self.active_board_id, role)?;
            if matches!(role, BoardRole::Owner | BoardRole::Editor) {
                if let Some(archives) = archives {
                    let mut complete_before = before.clone();
                    let mut complete_after = after.clone();
                    complete_before.archives = archives.clone();
                    complete_after.archives = archives;
                    persist_loro_local_delta(
                        &transaction,
                        self.active_board_id,
                        &complete_before,
                        &complete_after,
                        self.loro_peers.entry(self.active_board_id).or_default(),
                    )?;
                } else {
                    persist_loro_local_delta(
                        &transaction,
                        self.active_board_id,
                        before,
                        after,
                        self.loro_peers.entry(self.active_board_id).or_default(),
                    )?;
                }
            }
        }
        transaction.commit()?;
        self.positions = positions;
        Ok(stats)
    }

    pub(super) fn replace_all(&mut self, data: &StoredData) -> StorageResult<()> {
        let mut positions = DatabasePositions::default();
        let transaction = self.connection.transaction()?;
        transaction.execute_batch(
            "DELETE FROM tasks;
             DELETE FROM columns;
             DELETE FROM archives;
             DELETE FROM templates;
             DELETE FROM metadata;",
        )?;
        persist_diff_transaction(
            &transaction,
            &StoredData::default(),
            data,
            &mut positions,
            true,
        )?;
        transaction.execute(
            "INSERT INTO metadata(key, value) VALUES
                ('storage_initialized', '1'),
                ('lazy_archives_ready', '1')",
            [],
        )?;
        transaction.commit()?;
        self.positions = positions;
        if self.active_board_id != 0 {
            let board_id = self.active_board_id;
            let tx = self.connection.transaction()?;
            tx.execute("DELETE FROM cardbe_columns WHERE board_id=?1", [board_id])?;
            tx.execute("DELETE FROM cardbe_archives WHERE board_id=?1", [board_id])?;
            tx.execute("DELETE FROM cardbe_templates WHERE board_id=?1", [board_id])?;
            tx.execute(
                "DELETE FROM cardbe_board_metadata WHERE board_id=?1",
                [board_id],
            )?;
            write_board_transaction(&tx, board_id, data)?;
            tx.commit()?;
            self.positions = self.load_board_positions(board_id)?;
        }
        Ok(())
    }

    pub(super) fn is_initialized(&self) -> StorageResult<bool> {
        Ok(metadata_value(&self.connection, "storage_initialized")?.as_deref() == Some("1"))
    }

    pub(super) fn load(&self) -> StorageResult<StoredData> {
        if self.active_board_id != 0 {
            return self.load_board_data(self.active_board_id, true);
        }
        self.load_internal(true)
    }

    pub(super) fn load_internal(&self, include_archives: bool) -> StorageResult<StoredData> {
        let mut stored = StoredData::default();

        let mut columns = self.connection.prepare(
            // Legacy single-board columns predate the done flag.
            "SELECT id, name, color, sort_order, sort_rules, 0 FROM columns ORDER BY position, id",
        )?;
        stored.columns = columns
            .query_map([], column_from_row)?
            .collect::<Result<Vec<_>, _>>()?;

        let column_indices = stored
            .columns
            .iter()
            .enumerate()
            .map(|(index, column)| (column.id, index))
            .collect::<HashMap<_, _>>();
        let mut tasks = self
            .connection
            .prepare("SELECT column_id, payload FROM tasks ORDER BY column_id, position, id")?;
        let task_rows = tasks
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (column_id, payload) in task_rows {
            let task: Task = serde_json::from_str(&payload)?;
            let index = column_indices
                .get(&column_id)
                .ok_or_else(|| format!("Task {} refers to missing column {column_id}", task.id))?;
            stored.columns[*index].tasks.push(task);
        }

        if include_archives {
            stored.archives = self.load_archives()?;
        }

        let mut templates = self
            .connection
            .prepare("SELECT id, name, payload FROM templates ORDER BY position, id")?;
        let template_rows = templates
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        stored.templates = template_rows
            .into_iter()
            .map(|(id, name, payload)| {
                Ok(TaskTemplate {
                    id,
                    name,
                    task: serde_json::from_str(&payload)?,
                })
            })
            .collect::<StorageResult<Vec<_>>>()?;

        stored.schema_version =
            metadata_parse(&self.connection, "schema_version")?.unwrap_or(stored.schema_version);
        stored.next_column_id =
            metadata_parse(&self.connection, "next_column_id")?.unwrap_or_default();
        stored.next_task_id = metadata_parse(&self.connection, "next_task_id")?.unwrap_or_default();
        stored.next_template_id =
            metadata_parse(&self.connection, "next_template_id")?.unwrap_or_default();
        stored.settings = metadata_json(&self.connection, "settings")?.unwrap_or_default();
        stored.label_recency =
            metadata_json(&self.connection, "label_recency")?.unwrap_or_default();
        Ok(stored)
    }

    pub fn load_archives(&self) -> StorageResult<Vec<Archive>> {
        if self.active_board_id != 0 {
            return self.load_board_archives(self.active_board_id);
        }
        let mut archives = self
            .connection
            .prepare("SELECT archived_at, payload FROM archives ORDER BY position, task_id")?;
        let archive_rows = archives
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        archive_rows
            .into_iter()
            .map(|(time, payload)| {
                Ok(Archive {
                    time: time.parse()?,
                    task: serde_json::from_str(&payload)?,
                })
            })
            .collect()
    }

    pub(super) fn lazy_archives_ready(&self) -> StorageResult<bool> {
        Ok(metadata_value(&self.connection, "lazy_archives_ready")?.as_deref() == Some("1"))
    }

    pub(super) fn mark_lazy_archives_ready(&self) -> StorageResult<()> {
        self.connection.execute(
            "INSERT INTO metadata(key, value) VALUES ('lazy_archives_ready', '1')
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [],
        )?;
        Ok(())
    }

    pub(super) fn migrate_multiboard_schema(&mut self, legacy: &StoredData) -> StorageResult<()> {
        let tx = self.connection.transaction()?;
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS cardbe_global_metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS cardbe_boards(id INTEGER PRIMARY KEY AUTOINCREMENT,name TEXT NOT NULL,shared_role TEXT NOT NULL DEFAULT 'owner',sync_status TEXT NOT NULL DEFAULT 'local',sync_revision INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE IF NOT EXISTS cardbe_columns(board_id INTEGER NOT NULL REFERENCES cardbe_boards(id) ON DELETE CASCADE,id INTEGER NOT NULL,name TEXT NOT NULL,color TEXT NOT NULL,sort_order TEXT NOT NULL,position INTEGER NOT NULL,sort_rules TEXT NOT NULL DEFAULT '[]',done INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(board_id,id));
             CREATE TABLE IF NOT EXISTS cardbe_tasks(board_id INTEGER NOT NULL,id INTEGER NOT NULL,column_id INTEGER NOT NULL,position INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(board_id,id),FOREIGN KEY(board_id,column_id) REFERENCES cardbe_columns(board_id,id) ON DELETE CASCADE);
             CREATE INDEX IF NOT EXISTS cardbe_tasks_by_board_column_position ON cardbe_tasks(board_id,column_id,position);
             CREATE TABLE IF NOT EXISTS cardbe_archives(board_id INTEGER NOT NULL REFERENCES cardbe_boards(id) ON DELETE CASCADE,task_id INTEGER NOT NULL,archived_at TEXT NOT NULL,position INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(board_id,task_id));
             CREATE INDEX IF NOT EXISTS cardbe_archives_by_board_time ON cardbe_archives(board_id,CAST(archived_at AS INTEGER) DESC,task_id DESC);
             CREATE TABLE IF NOT EXISTS cardbe_templates(board_id INTEGER NOT NULL REFERENCES cardbe_boards(id) ON DELETE CASCADE,id INTEGER NOT NULL,name TEXT NOT NULL,position INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(board_id,id));
             CREATE TABLE IF NOT EXISTS cardbe_board_metadata(board_id INTEGER NOT NULL REFERENCES cardbe_boards(id) ON DELETE CASCADE,key TEXT NOT NULL,value TEXT NOT NULL,PRIMARY KEY(board_id,key));"
        )?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS cardbe_iroh_invites(invite_id TEXT PRIMARY KEY,board_id INTEGER NOT NULL REFERENCES cardbe_boards(id) ON DELETE CASCADE,secret TEXT NOT NULL,permission TEXT NOT NULL,enabled INTEGER NOT NULL DEFAULT 1,created_at TEXT NOT NULL DEFAULT (datetime('now')));
            CREATE TABLE IF NOT EXISTS cardbe_iroh_devices(invite_id TEXT NOT NULL REFERENCES cardbe_iroh_invites(invite_id) ON DELETE CASCADE,node_id TEXT NOT NULL,status INTEGER NOT NULL DEFAULT 0 CHECK(status IN (0,1,2)),PRIMARY KEY(invite_id,node_id));
            CREATE TABLE IF NOT EXISTS cardbe_iroh_remotes(board_id INTEGER PRIMARY KEY REFERENCES cardbe_boards(id) ON DELETE CASCADE,invitation TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS cardbe_iroh_conflicts(board_id INTEGER PRIMARY KEY REFERENCES cardbe_boards(id) ON DELETE CASCADE,revision INTEGER NOT NULL,name TEXT NOT NULL,permission TEXT NOT NULL,payload TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS cardbe_loro_docs(board_id INTEGER PRIMARY KEY REFERENCES cardbe_boards(id) ON DELETE CASCADE,payload BLOB NOT NULL);")?;
        // Persist only an activation marker, never deleted invitation secrets.
        // The trigger also covers board cascades and backup restoration.
        tx.execute_batch("CREATE TRIGGER IF NOT EXISTS cardbe_iroh_record_revocation AFTER DELETE ON cardbe_iroh_invites BEGIN
            INSERT INTO cardbe_global_metadata(key,value) VALUES('iroh_has_revocations','1') ON CONFLICT(key) DO NOTHING;
            END;")?;
        for sql in [
            "ALTER TABLE cardbe_boards ADD COLUMN shared_role TEXT NOT NULL DEFAULT 'owner'",
            "ALTER TABLE cardbe_boards ADD COLUMN sync_status TEXT NOT NULL DEFAULT 'local'",
            "ALTER TABLE cardbe_boards ADD COLUMN sync_revision INTEGER NOT NULL DEFAULT 0",
            "ALTER TABLE cardbe_iroh_invites ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1",
            "ALTER TABLE cardbe_iroh_invites ADD COLUMN created_at TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE cardbe_iroh_conflicts ADD COLUMN loro_update BLOB",
            "ALTER TABLE cardbe_columns ADD COLUMN sort_rules TEXT NOT NULL DEFAULT '[]'",
            "ALTER TABLE cardbe_columns ADD COLUMN done INTEGER NOT NULL DEFAULT 0",
        ] {
            let _ = tx.execute(sql, []);
        }
        crate::flow::initialize(&tx)?;
        task_events::initialize(&tx)?;
        validate_cardbe_schema(&tx)?;
        let migrated: Option<String> = tx
            .query_row(
                "SELECT value FROM cardbe_global_metadata WHERE key='multiboard_schema_version'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if migrated.as_deref() != Some("1") {
            let experimental = experimental_snapshots(&tx)?;
            if experimental.is_empty() {
                tx.execute("INSERT INTO cardbe_boards(name) VALUES ('My board')", [])?;
                let id = tx.last_insert_rowid();
                write_board_transaction(&tx, id, legacy)?;
                tx.execute(
                    "INSERT INTO cardbe_global_metadata(key,value) VALUES ('active_board_id',?1)",
                    [id.to_string()],
                )?;
            } else {
                let mut first = None;
                for (old_id, name, data) in experimental {
                    tx.execute(
                        "INSERT INTO cardbe_boards(id,name) VALUES (?1,?2)",
                        params![old_id, name],
                    )?;
                    write_board_transaction(&tx, old_id, &data)?;
                    first.get_or_insert(old_id);
                }
                let preferred = legacy_metadata_parse::<i64>(&tx, "active_board_id")?
                    .filter(|id| {
                        tx.query_row(
                            "SELECT EXISTS(SELECT 1 FROM cardbe_boards WHERE id=?1)",
                            [id],
                            |r| r.get::<_, bool>(0),
                        )
                        .unwrap_or(false)
                    })
                    .or(first)
                    .ok_or("Experimental board migration contained no boards")?;
                tx.execute(
                    "INSERT INTO cardbe_global_metadata(key,value) VALUES ('active_board_id',?1)",
                    [preferred.to_string()],
                )?;
            }
            tx.execute("INSERT INTO cardbe_global_metadata(key,value) VALUES ('settings',?1) ON CONFLICT(key) DO NOTHING", [serde_json::to_string(&legacy.settings)?])?;
            tx.execute("INSERT INTO cardbe_global_metadata(key,value) VALUES ('multiboard_schema_version','1')", [])?;
        }
        if let Some(value) = namespaced_metadata_json::<serde_json::Value>(&tx, "settings")? {
            if value.pointer("/iroh_network/enabled").is_none() {
                let mut settings: crate::models::Settings = serde_json::from_value(value)?;
                settings.iroh_network.enabled = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM cardbe_iroh_invites)
                     OR EXISTS(SELECT 1 FROM cardbe_iroh_remotes)
                     OR EXISTS(SELECT 1 FROM cardbe_global_metadata WHERE key='iroh_has_revocations')",
                    [],
                    |row| row.get(0),
                )?;
                tx.execute(
                    "UPDATE cardbe_global_metadata SET value=?1 WHERE key='settings'",
                    [serde_json::to_string(&settings)?],
                )?;
            }
        }
        tx.commit()?;
        self.initialize_task_search_index()?;
        Ok(())
    }

    pub(super) fn write_board(&mut self, board_id: i64, data: &StoredData) -> StorageResult<()> {
        let tx = self.connection.transaction()?;
        write_board_transaction(&tx, board_id, data)?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn load_board_positions(&self, board_id: i64) -> StorageResult<DatabasePositions> {
        Ok(DatabasePositions {
            columns: load_board_positions(&self.connection, "cardbe_columns", board_id)?,
            tasks: load_board_positions(&self.connection, "cardbe_tasks", board_id)?,
        })
    }
}
