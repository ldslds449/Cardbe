use super::*;

impl Database {
    pub fn get_link_preview(
        &self,
        url: &str,
        now: i64,
    ) -> Result<Option<crate::link_preview::LinkPreview>, DomainError> {
        crate::link_preview::cache::read(&self.connection, url, now)
    }

    pub fn cache_link_preview(
        &self,
        preview: &crate::link_preview::LinkPreview,
    ) -> Result<(), DomainError> {
        crate::link_preview::cache::write(&self.connection, preview)
    }

    pub fn get_notes(&self) -> StorageResult<Vec<Note>> {
        let mut statement = self.connection.prepare(
            "SELECT id, title, content, pinned, created_at, updated_at
             FROM notes ORDER BY pinned DESC, updated_at DESC, id DESC",
        )?;
        let notes = statement
            .query_map([], |row| {
                Ok(Note {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    content: row.get(2)?,
                    pinned: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(notes)
    }

    /// Note IDs ranked by fused trigram FTS (title and content) and fuzzy title matching.
    pub fn search_notes(
        &self,
        query: &str,
        options: crate::search::SearchOptions,
    ) -> StorageResult<Vec<i64>> {
        use crate::search::{fuse, title_priority, FuzzySearch};
        let query = query.trim();
        if !options.valid() {
            return Err(DomainError::InvalidArgument.into());
        }
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let lowercase_query = query.to_lowercase();
        let limit = i64::try_from(options.candidate_limit)?;
        let (sql, pattern) = if query.chars().count() < 3 {
            (
                "SELECT id,title FROM notes
                 WHERE instr(lower(title || ' ' || content),lower(?1))>0 ORDER BY id LIMIT ?2",
                query.to_owned(),
            )
        } else {
            (
                "SELECT n.id,n.title FROM cardbe_note_fts JOIN notes n ON n.id=cardbe_note_fts.rowid
                 WHERE cardbe_note_fts MATCH ?1 ORDER BY bm25(cardbe_note_fts),n.id LIMIT ?2",
                format!("\"{}\"", query.replace('"', "\"\"")),
            )
        };
        let fts = self
            .connection
            .prepare(sql)?
            .query_map(params![pattern, limit], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    title_priority(&row.get::<_, String>(1)?, &lowercase_query),
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut statement = self.connection.prepare("SELECT id,title FROM notes")?;
        let mut rows = statement.query([])?;
        let mut fuzzy = FuzzySearch::new(query, options.candidate_limit);
        while let Some(row) = rows.next()? {
            fuzzy.push(row.get(0)?, &row.get::<_, String>(1)?);
        }
        Ok(fuse(&fts, &fuzzy.finish(), options))
    }

    pub fn create_note(&mut self, now: i64) -> StorageResult<Note> {
        self.create_note_with_content(now, String::new(), String::new())
    }

    pub fn create_note_with_content(
        &mut self,
        now: i64,
        title: String,
        content: String,
    ) -> StorageResult<Note> {
        self.connection.execute(
            "INSERT INTO notes(title, content, pinned, created_at, updated_at)
             VALUES (?1, ?2, 0, ?3, ?3)",
            params![title, content, now],
        )?;
        Ok(Note {
            id: self.connection.last_insert_rowid(),
            title,
            content,
            pinned: false,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn update_note(&mut self, note: &Note) -> StorageResult<()> {
        let changed = self.connection.execute(
            "UPDATE notes SET title = ?2, content = ?3, pinned = ?4,
                 updated_at = ?5 WHERE id = ?1",
            params![
                note.id,
                note.title,
                note.content,
                note.pinned,
                note.updated_at
            ],
        )?;
        if changed == 0 {
            return Err(crate::errors::DomainError::NoteNotFound.into());
        }
        Ok(())
    }

    pub fn delete_note(&mut self, note_id: i64) -> StorageResult<()> {
        let changed = self
            .connection
            .execute("DELETE FROM notes WHERE id = ?1", [note_id])?;
        if changed == 0 {
            return Err(crate::errors::DomainError::NoteNotFound.into());
        }
        Ok(())
    }
}
