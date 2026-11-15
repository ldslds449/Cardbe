use super::*;

impl Database {
    pub fn search_tasks(&self, board_id: i64, query: &str) -> StorageResult<Vec<i64>> {
        self.search_tasks_with_options(board_id, query, crate::search::SearchOptions::default())
    }

    pub fn search_tasks_with_options(
        &self,
        board_id: i64,
        query: &str,
        options: crate::search::SearchOptions,
    ) -> StorageResult<Vec<i64>> {
        let filter = TaskExplorerQuery {
            query: query.to_owned(),
            board_ids: Some(vec![board_id]),
            ..Default::default()
        };
        let rowids = hybrid_search(&self.connection, &filter, options)?;
        let mut statement = self.connection.prepare(
            "SELECT s.task_id FROM json_each(?1) ranked
                 JOIN cardbe_task_search s ON s.rowid=ranked.value
                 ORDER BY CAST(ranked.key AS INTEGER)",
        )?;
        let rows = statement.query_map([serde_json::to_string(&rowids)?], |row| {
            row.get::<_, i64>(0)
        })?;
        let mut ids = Vec::with_capacity(rowids.len());
        let mut seen = std::collections::HashSet::new();
        for id in rows {
            let id = id?;
            if seen.insert(id) {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    pub(super) fn initialize_task_search_index(&self) -> StorageResult<()> {
        let index_exists = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='cardbe_task_search')",
            [],
            |row| row.get::<_, bool>(0),
        )?;
        self.connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS cardbe_task_search(
                 rowid INTEGER PRIMARY KEY,
                 board_id INTEGER NOT NULL,
                 task_id INTEGER NOT NULL,
                 archived INTEGER NOT NULL,
                 body TEXT NOT NULL,
                 UNIQUE(board_id,task_id,archived)
             );
             CREATE VIRTUAL TABLE IF NOT EXISTS cardbe_task_fts USING fts5(
                 body,content='cardbe_task_search',content_rowid='rowid',tokenize='trigram'
             );
             CREATE TRIGGER IF NOT EXISTS cardbe_task_search_ai AFTER INSERT ON cardbe_task_search BEGIN
                 INSERT INTO cardbe_task_fts(rowid,body) VALUES(new.rowid,new.body);
             END;
             CREATE TRIGGER IF NOT EXISTS cardbe_task_search_ad AFTER DELETE ON cardbe_task_search BEGIN
                 INSERT INTO cardbe_task_fts(cardbe_task_fts,rowid,body) VALUES('delete',old.rowid,old.body);
             END;
             CREATE TRIGGER IF NOT EXISTS cardbe_task_search_au AFTER UPDATE ON cardbe_task_search BEGIN
                 INSERT INTO cardbe_task_fts(cardbe_task_fts,rowid,body) VALUES('delete',old.rowid,old.body);
                 INSERT INTO cardbe_task_fts(rowid,body) VALUES(new.rowid,new.body);
             END;
             CREATE VIEW IF NOT EXISTS cardbe_task_search_payloads AS
                 SELECT board_id,id AS task_id,0 AS archived,payload FROM cardbe_tasks
                 UNION ALL
                 SELECT board_id,task_id,1 AS archived,payload FROM cardbe_archives;
             CREATE VIEW IF NOT EXISTS cardbe_task_search_content AS
                 SELECT board_id,task_id,archived,
                     COALESCE(json_extract(payload,'$.title'),'') || ' ' ||
                     COALESCE(json_extract(payload,'$.description'),'') || ' ' ||
                     COALESCE((SELECT group_concat(value,' ') FROM json_each(payload,'$.labels')),'') || ' ' ||
                     COALESCE((SELECT group_concat(json_extract(value,'$.text'),' ') FROM json_each(payload,'$.items')),'') AS body
                 FROM cardbe_task_search_payloads;
             CREATE TRIGGER IF NOT EXISTS cardbe_tasks_ai AFTER INSERT ON cardbe_tasks BEGIN
                 INSERT INTO cardbe_task_search(board_id,task_id,archived,body)
                 SELECT board_id,task_id,archived,body FROM cardbe_task_search_content
                 WHERE board_id=new.board_id AND task_id=new.id AND archived=0
                 ON CONFLICT(board_id,task_id,archived) DO UPDATE SET body=excluded.body;
             END;
             CREATE TRIGGER IF NOT EXISTS cardbe_tasks_ad AFTER DELETE ON cardbe_tasks BEGIN
                 DELETE FROM cardbe_task_search WHERE board_id=old.board_id AND task_id=old.id AND archived=0;
             END;
             CREATE TRIGGER IF NOT EXISTS cardbe_tasks_au AFTER UPDATE OF board_id,id,payload ON cardbe_tasks BEGIN
                 DELETE FROM cardbe_task_search WHERE board_id=old.board_id AND task_id=old.id AND archived=0;
                 INSERT INTO cardbe_task_search(board_id,task_id,archived,body)
                 SELECT board_id,task_id,archived,body FROM cardbe_task_search_content
                 WHERE board_id=new.board_id AND task_id=new.id AND archived=0
                 ON CONFLICT(board_id,task_id,archived) DO UPDATE SET body=excluded.body;
             END;
             CREATE TRIGGER IF NOT EXISTS cardbe_archives_ai AFTER INSERT ON cardbe_archives BEGIN
                 INSERT INTO cardbe_task_search(board_id,task_id,archived,body)
                 SELECT board_id,task_id,archived,body FROM cardbe_task_search_content
                 WHERE board_id=new.board_id AND task_id=new.task_id AND archived=1
                 ON CONFLICT(board_id,task_id,archived) DO UPDATE SET body=excluded.body;
             END;
             CREATE TRIGGER IF NOT EXISTS cardbe_archives_ad AFTER DELETE ON cardbe_archives BEGIN
                 DELETE FROM cardbe_task_search WHERE board_id=old.board_id AND task_id=old.task_id AND archived=1;
             END;
             CREATE TRIGGER IF NOT EXISTS cardbe_archives_au AFTER UPDATE OF board_id,task_id,payload ON cardbe_archives BEGIN
                 DELETE FROM cardbe_task_search WHERE board_id=old.board_id AND task_id=old.task_id AND archived=1;
                 INSERT INTO cardbe_task_search(board_id,task_id,archived,body)
                 SELECT board_id,task_id,archived,body FROM cardbe_task_search_content
                 WHERE board_id=new.board_id AND task_id=new.task_id AND archived=1
                 ON CONFLICT(board_id,task_id,archived) DO UPDATE SET body=excluded.body;
             END;",
        )?;
        if !index_exists {
            self.connection.execute_batch(
                "INSERT INTO cardbe_task_search(board_id,task_id,archived,body)
                 SELECT board_id,task_id,archived,body FROM cardbe_task_search_content WHERE 1;",
            )?;
        }
        Ok(())
    }

    /// Unarchived tasks with a due time in `[from, to)`, for due notifications.
    pub fn tasks_due_between(&self, from: u128, to: u128) -> StorageResult<Vec<DueTask>> {
        let bound = |time: u128| i64::try_from(time).unwrap_or(i64::MAX);
        let mut stmt = self.connection.prepare(
            // Tasks in a done column are completed and need no reminder.
            "SELECT t.board_id,b.name,t.column_id,t.payload
             FROM cardbe_tasks t JOIN cardbe_boards b ON b.id=t.board_id
             JOIN cardbe_columns c ON c.board_id=t.board_id AND c.id=t.column_id
             WHERE c.done=0 AND json_extract(t.payload,'$.due_time')>=?1
               AND json_extract(t.payload,'$.due_time')<?2
             ORDER BY t.board_id,t.column_id,t.position,t.id",
        )?;
        let rows = stmt.query_map([bound(from), bound(to)], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        let mut tasks = Vec::new();
        for row in rows {
            let (board_id, board_name, column_id, payload) = row?;
            tasks.push(DueTask {
                board_id,
                board_name,
                column_id,
                task: serde_json::from_str(&payload)?,
            });
        }
        Ok(tasks)
    }

    pub fn list_all_task_page(
        &self,
        filter: &TaskExplorerQuery,
        offset: i64,
        limit: usize,
    ) -> StorageResult<Vec<AllTaskRow>> {
        if filter
            .search_options
            .is_some_and(|options| !options.valid())
        {
            return Err(DomainError::InvalidArgument.into());
        }
        // shortcut: OFFSET slows deep pages; use keyset cursors if profiling justifies it.
        let matches = if filter.query.trim().is_empty() {
            None
        } else {
            Some(serde_json::to_string(&hybrid_search(
                &self.connection,
                filter,
                filter.search_options.unwrap_or_default(),
            )?)?)
        };
        let sql = format!(
            "{TASK_SEARCH_CTE}
             , hits AS MATERIALIZED (
               SELECT CAST(value AS INTEGER) AS search_id, CAST(key AS INTEGER) AS rank
               FROM json_each(?1))
             SELECT e.board_id, e.board_name, e.column_name, e.archived_at, e.payload,
                    e.column_id, e.completion='done', e.waiting_count
             FROM eligible e
             LEFT JOIN hits h ON h.search_id=e.search_id
             WHERE (?1 IS NULL OR h.search_id IS NOT NULL)
             ORDER BY
               CASE WHEN ?14='relevance' THEN h.rank END,
               CASE WHEN ?14='priority' THEN CASE priority
                 WHEN 'urgent' THEN 4 WHEN 'high' THEN 3 WHEN 'medium' THEN 2
                 WHEN 'low' THEN 1 ELSE 0 END END DESC,
               CASE WHEN ?14 IN ('due','priority') THEN due_time IS NULL END,
               CASE WHEN ?14 IN ('due','priority') THEN due_time END,
               CASE WHEN ?14='archived' THEN CAST(archived_at AS INTEGER) END DESC,
               CASE WHEN ?14='column' THEN column_name END COLLATE NOCASE,
               CASE WHEN ?14='created' THEN
                 CAST(json_extract(payload, '$.created_time') AS INTEGER) END DESC,
               title COLLATE NOCASE, board_id, archived, task_id
             LIMIT ?15 OFFSET ?16",
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement
            .query_map(
                params![
                    matches,
                    filter
                        .board_ids
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()?,
                    filter.column_id,
                    filter.status,
                    filter.now,
                    filter.due,
                    filter.due_start,
                    filter.due_end,
                    filter.archive_start,
                    filter.archive_end,
                    crate::task_dates::local_day_start(filter.now),
                    filter
                        .priorities
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()?,
                    filter.extra_filters()?,
                    filter.sort,
                    i64::try_from(limit)?,
                    offset,
                ],
                |row| {
                    let board_id = row.get(0)?;
                    let board_name = row.get(1)?;
                    let column_name = row.get(2)?;
                    let archived_at = row
                        .get::<_, Option<String>>(3)?
                        .map(|value| value.parse::<u128>())
                        .transpose()
                        .map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                3,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?;
                    let payload = row.get::<_, String>(4)?;
                    let task: Task = serde_json::from_str(&payload).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
                    Ok(AllTaskRow {
                        board_id,
                        board_name,
                        column_id: row.get(5)?,
                        column_name,
                        archived_at,
                        done: row.get(6)?,
                        waiting_count: row.get(7)?,
                        excerpt: crate::search::task_excerpt(&task, &filter.query),
                        task: TaskSummary::from(&task),
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Counts `filter`'s tasks under every status at once, ignoring its own
    /// status and sort. The column filter does not apply to archived tasks,
    /// as the explorer drops it there. Searches count one pool fused across
    /// all statuses, so each count is capped by the same search limits.
    pub fn count_all_tasks(&self, filter: &TaskExplorerQuery) -> StorageResult<TaskStatusCounts> {
        let every_status = TaskExplorerQuery {
            status: "all".into(),
            column_id: None,
            ..filter.clone()
        };
        let options = filter.search_options.unwrap_or_default();
        if !options.valid() {
            return Err(DomainError::InvalidArgument.into());
        }
        let mut values = search_params(&every_status)?;
        if !filter.query.trim().is_empty() {
            let matches = hybrid_search(&self.connection, &every_status, options)?;
            values[0] = rusqlite::types::Value::Text(serde_json::to_string(&matches)?);
        }
        values.push(
            filter
                .column_id
                .map(rusqlite::types::Value::Integer)
                .unwrap_or(rusqlite::types::Value::Null),
        );
        let sql = format!(
            "{TASK_SEARCH_CTE}
             , counted AS (SELECT *, archived=0 AND (?14 IS NULL OR column_id=?14) AS open_match
               FROM eligible
               WHERE (?1 IS NULL OR search_id IN (SELECT value FROM json_each(?1))))
             SELECT COALESCE(SUM(?14 IS NULL OR column_id=?14), 0),
                    COALESCE(SUM(open_match), 0),
                    COALESCE(SUM(open_match AND due_time<?5
                                 AND (due_time<>?11 OR due_at_midnight)), 0),
                    COALESCE(SUM(open_match AND recurrence IS NOT NULL), 0),
                    COALESCE(SUM(archived=1), 0)
             FROM counted",
        );
        Ok(self
            .connection
            .query_row(&sql, rusqlite::params_from_iter(&values), |row| {
                Ok(TaskStatusCounts {
                    all: row.get(0)?,
                    active: row.get(1)?,
                    overdue: row.get(2)?,
                    recurring: row.get(3)?,
                    archived: row.get(4)?,
                })
            })?)
    }
}
