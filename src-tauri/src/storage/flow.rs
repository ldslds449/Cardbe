use super::*;

impl Database {
    pub fn read_flow(
        &self,
        board_id: i64,
        tab_id: Option<&str>,
    ) -> StorageResult<crate::flow::FlowData> {
        crate::flow::read(&self.connection, board_id, tab_id)
    }

    pub fn read_flow_backup(&self, board_id: i64) -> StorageResult<crate::flow::FlowData> {
        crate::flow::read_backup(&self.connection, board_id)
    }

    pub fn save_flow_layout(
        &mut self,
        board_id: i64,
        tab_id: &str,
        layout: &crate::flow::FlowLayout,
    ) -> StorageResult<()> {
        let tx = self.connection.transaction()?;
        if !crate::flow::tabs(&tx, board_id)?
            .iter()
            .any(|tab| tab.id == tab_id)
        {
            return Err(DomainError::FlowTabNotFound.into());
        }
        crate::flow::validate_layout(layout, &crate::flow::task_ids(&tx, board_id)?)?;
        crate::flow::save_layout(&tx, board_id, tab_id, layout)?;
        tx.commit()?;
        Ok(())
    }

    pub fn change_flow_tabs(
        &mut self,
        board_id: i64,
        change: impl FnOnce(&Transaction<'_>) -> StorageResult<()>,
    ) -> StorageResult<Vec<crate::flow::FlowTab>> {
        let tx = self.connection.transaction()?;
        change(&tx)?;
        let tabs = crate::flow::tabs(&tx, board_id)?;
        tx.commit()?;
        Ok(tabs)
    }

    pub fn change_relationship(
        &mut self,
        board_id: i64,
        relationship: &crate::flow::Relationship,
        remove: bool,
    ) -> StorageResult<()> {
        let relationship = &crate::flow::normalize(relationship.clone());
        let tx = self.connection.transaction()?;
        if remove {
            crate::flow::delete_relationship(&tx, board_id, relationship)?;
            crate::flow::remove_connection(&tx, board_id, relationship)?;
        } else {
            let data = crate::flow::read(&tx, board_id, None)?;
            crate::flow::validate_relationship(
                &data.relationships,
                relationship,
                &crate::flow::task_ids(&tx, board_id)?,
            )?;
            crate::flow::insert_relationship(&tx, board_id, relationship)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn update_relationship(
        &mut self,
        board_id: i64,
        relationship: &crate::flow::Relationship,
    ) -> StorageResult<()> {
        let tx = self.connection.transaction()?;
        crate::flow::update_relationship(
            &tx,
            board_id,
            &crate::flow::normalize(relationship.clone()),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Due times before this instant (milliseconds since the Unix epoch) were
    /// already checked for notifications on this device.
    pub fn notifications_checked_until(&self) -> StorageResult<Option<u128>> {
        Ok(
            namespaced_metadata_value(&self.connection, "notifications_checked_until")?
                .and_then(|value| value.parse().ok()),
        )
    }

    pub fn set_notifications_checked_until(&mut self, time: u128) -> StorageResult<()> {
        self.connection.execute(
            "INSERT INTO cardbe_global_metadata(key,value) VALUES('notifications_checked_until',?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [time.to_string()],
        )?;
        Ok(())
    }

    pub fn task_events(
        &self,
        board_id: i64,
        task_id: Option<i64>,
    ) -> StorageResult<Vec<task_events::TaskEvent>> {
        task_events::list(&self.connection, board_id, task_id, None)
    }

    /// Board events recorded after `after_id`, for refreshing a loaded log.
    pub fn task_events_after(
        &self,
        board_id: i64,
        after_id: Option<i64>,
    ) -> StorageResult<Vec<task_events::TaskEvent>> {
        task_events::list(&self.connection, board_id, None, after_id)
    }

    pub fn load_task(&self, board_id: i64, task_id: i64) -> StorageResult<Option<Task>> {
        self.connection
            .query_row(
                "SELECT payload FROM cardbe_tasks WHERE board_id=?1 AND id=?2",
                params![board_id, task_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|payload| serde_json::from_str(&payload).map_err(Into::into))
            .transpose()
    }
}
