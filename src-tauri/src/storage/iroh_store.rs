use super::*;

impl Database {
    /// Read what an Iroh host response needs for one board. Authorization,
    /// revision, Loro document, and board data come from one read transaction,
    /// so a concurrent revocation, local edit, or editor push cannot mix
    /// states, and content committed after a revocation is never returned.
    /// `plan` chooses what to read once permission and revision are known.
    pub fn read_iroh_share(
        &self,
        board_id: i64,
        invite_id: &str,
        secret: &str,
        node_id: &str,
        plan: impl FnOnce(IrohPermission, &Board) -> IrohShareRead,
    ) -> StorageResult<Result<IrohShareContents, IrohShareDenied>> {
        self.read_consistent(|db| -> StorageResult<Result<_, IrohShareDenied>> {
            let approved = db
                .connection
                .query_row(
                    "SELECT d.status FROM cardbe_iroh_devices d JOIN cardbe_iroh_invites i ON i.invite_id=d.invite_id WHERE d.invite_id=?1 AND d.node_id=?2 AND i.enabled=1 AND i.secret=?3",
                    params![invite_id, node_id, secret],
                    |row| row.get::<_, IrohDeviceStatus>(0),
                )
                .optional()?
                == Some(IrohDeviceStatus::Approved);
            if !approved {
                return Ok(Err(IrohShareDenied::DeviceNotApproved));
            }
            let Some(permission) = db
                .connection
                .query_row(
                    "SELECT permission FROM cardbe_iroh_invites WHERE invite_id=?1 AND board_id=?2 AND secret=?3 AND enabled=1",
                    params![invite_id, board_id, secret],
                    |row| row.get::<_, IrohPermission>(0),
                )
                .optional()?
            else {
                return Ok(Err(IrohShareDenied::InvitationUnavailable));
            };
            let Some(board) = db
                .boards()?
                .into_iter()
                .find(|board| board.id == board_id && board.shared_role == BoardRole::Owner)
            else {
                return Ok(Err(IrohShareDenied::BoardDeleted));
            };
            let read = plan(permission, &board);
            let loro_document = if read.loro_document {
                Some(
                    db.iroh_loro_update(board_id)?
                        .ok_or("Editable board is missing its Loro document")?,
                )
            } else {
                None
            };
            let data = if read.data {
                Some(db.load_board_data(board_id, true)?)
            } else {
                None
            };
            Ok(Ok(IrohShareContents {
                board,
                permission,
                loro_document,
                data,
            }))
        })?
    }

    /// Iroh capabilities are deliberately database-only metadata.  The JSON
    /// backup format remains portable and must never contain bearer secrets.
    pub fn save_iroh_invite(
        &mut self,
        invite_id: &str,
        board_id: i64,
        secret: &str,
        permission: IrohPermission,
    ) -> StorageResult<()> {
        self.connection.execute("INSERT INTO cardbe_iroh_invites(invite_id,board_id,secret,permission,enabled,created_at) VALUES(?1,?2,?3,?4,1,datetime('now')) ON CONFLICT(invite_id) DO UPDATE SET board_id=excluded.board_id,secret=excluded.secret,permission=excluded.permission", params![invite_id, board_id, secret, permission])?; // gitleaks:allow
        Ok(())
    }

    pub fn iroh_device_access(
        &mut self,
        invite_id: &str,
        secret: &str,
        node_id: &str,
        request_approval: bool,
    ) -> StorageResult<Option<IrohDeviceStatus>> {
        self.connection.execute(
            "INSERT INTO cardbe_iroh_devices(invite_id,node_id,status) SELECT invite_id,?3,0 FROM cardbe_iroh_invites WHERE invite_id=?1 AND secret=?2 AND enabled=1 ON CONFLICT(invite_id,node_id) DO NOTHING",
            params![invite_id, secret, node_id],
        )?;
        if request_approval {
            self.connection.execute(
                "UPDATE cardbe_iroh_devices SET status=?3 WHERE invite_id=?1 AND node_id=?2 AND status=?4 AND EXISTS(SELECT 1 FROM cardbe_iroh_invites WHERE invite_id=?1 AND secret=?5 AND enabled=1)",
                params![invite_id, node_id, IrohDeviceStatus::Pending, IrohDeviceStatus::Revoked, secret],
            )?;
        }
        Ok(self.connection.query_row(
            "SELECT d.status FROM cardbe_iroh_devices d JOIN cardbe_iroh_invites i ON i.invite_id=d.invite_id WHERE d.invite_id=?1 AND d.node_id=?2 AND i.enabled=1 AND i.secret=?3",
            params![invite_id, node_id, secret], |r| r.get::<_, IrohDeviceStatus>(0),
        ).optional()?)
    }

    pub fn iroh_devices(&self) -> StorageResult<Vec<(String, String, IrohDeviceStatus)>> {
        Ok(self.connection.prepare("SELECT invite_id,node_id,status FROM cardbe_iroh_devices WHERE status!=?1 ORDER BY status,invite_id,node_id")?
            .query_map([IrohDeviceStatus::Revoked], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn set_iroh_device_approved(
        &mut self,
        invite_id: &str,
        node_id: &str,
        approved: bool,
    ) -> StorageResult<()> {
        let status = if approved {
            IrohDeviceStatus::Approved
        } else {
            IrohDeviceStatus::Revoked
        };
        if self.connection.execute(
            "UPDATE cardbe_iroh_devices SET status=?3 WHERE invite_id=?1 AND node_id=?2",
            params![invite_id, node_id, status],
        )? == 0
        {
            return Err(crate::errors::DomainError::DeviceRequestNotFound.into());
        }
        Ok(())
    }

    pub fn iroh_remote(&self, board_id: i64) -> StorageResult<Option<String>> {
        Ok(self
            .connection
            .query_row(
                "SELECT invitation FROM cardbe_iroh_remotes WHERE board_id=?1",
                [board_id],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn update_iroh_remote(&self, board_id: i64, invitation: &str) -> StorageResult<()> {
        if self.connection.execute(
            "UPDATE cardbe_iroh_remotes SET invitation=?2 WHERE board_id=?1",
            params![board_id, invitation],
        )? == 0
        {
            return Err(DomainError::InviteNotFound.into());
        }
        Ok(())
    }

    pub fn iroh_loro_update(&self, board_id: i64) -> StorageResult<Option<Vec<u8>>> {
        Ok(self
            .connection
            .query_row(
                "SELECT payload FROM cardbe_loro_docs WHERE board_id=?1",
                [board_id],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn ensure_iroh_loro_doc(&mut self, board_id: i64) -> StorageResult<Vec<u8>> {
        if let Some(update) = self.iroh_loro_update(board_id)? {
            return Ok(update);
        }
        let data = self.read_board_complete(board_id)?;
        let update = crate::loro_board::apply_local_delta_with_peer(
            None,
            &StoredData::default(),
            &data,
            self.loro_peers.entry(board_id).or_default(),
        )
        .map_err(|e| format!("Could not seed shared-board CRDT: {e}"))?;
        self.connection.execute(
            "INSERT INTO cardbe_loro_docs(board_id,payload) VALUES(?1,?2)",
            params![board_id, update],
        )?;
        self.iroh_loro_update(board_id)?
            .ok_or_else(|| "Could not load seeded shared-board CRDT".into())
    }

    /// Applies an editor's Loro update atomically with the SQL projection.
    #[cfg(test)]
    pub fn apply_iroh_loro_update(
        &mut self,
        board_id: i64,
        invite_id: &str,
        secret: &str,
        node_id: &str,
        update: &[u8],
    ) -> StorageResult<(bool, i64, StoredData)> {
        loop {
            let prepared =
                self.prepare_iroh_loro_update(board_id, invite_id, secret, node_id, update)?;
            if let Some(result) =
                self.commit_iroh_loro_update(prepared, invite_id, secret, node_id)?
            {
                return Ok(result);
            }
        }
    }

    /// Performs the CPU-bound half of an editor push: merge, projection, and
    /// size validation. It writes nothing, so callers can run it without
    /// holding application state and commit it afterwards.
    pub fn prepare_iroh_loro_update(
        &self,
        board_id: i64,
        invite_id: &str,
        secret: &str,
        node_id: &str,
        update: &[u8],
    ) -> StorageResult<PreparedLoroUpdate> {
        ensure_iroh_editor(&self.connection, board_id, invite_id, secret, node_id)?;
        let base = editable_loro_doc(&self.connection, board_id)?;
        let merged = crate::loro_board::merge(Some(&base), update)
            .map_err(|e| format!("Invalid Loro update: {e}"))?;
        // Snapshot bytes can differ after re-export even when the Loro
        // operation history is unchanged. Use its version vector for retries.
        if crate::loro_board::same_state_vector(&base, &merged)
            .map_err(|e| format!("Invalid Loro version vector: {e}"))?
        {
            return Ok(PreparedLoroUpdate {
                board_id,
                base,
                merged: None,
            });
        }
        let projected = crate::loro_board::project(&merged)
            .map_err(|e| format!("Invalid Loro projection: {e}"))?;
        validate_remote_board_size(Some(&self.read_board_complete(board_id)?), &projected)?;
        Ok(PreparedLoroUpdate {
            board_id,
            base,
            merged: Some((merged, projected)),
        })
    }

    /// Commits a prepared editor push. Returns `None` without writing when
    /// the board's Loro document changed after preparation, so the caller can
    /// prepare again from the current document. The capability is checked
    /// inside this transaction so revocation wins even when it races an
    /// already accepted Iroh connection.
    pub fn commit_iroh_loro_update(
        &mut self,
        prepared: PreparedLoroUpdate,
        invite_id: &str,
        secret: &str,
        node_id: &str,
    ) -> StorageResult<Option<(bool, i64, StoredData)>> {
        let board_id = prepared.board_id;
        let previous = self.read_board_complete(board_id)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ensure_iroh_editor(&tx, board_id, invite_id, secret, node_id)?;
        if editable_loro_doc(&tx, board_id)? != prepared.base {
            return Ok(None);
        }
        let revision: i64 = tx.query_row(
            "SELECT sync_revision FROM cardbe_boards WHERE id=?1 AND shared_role=?2",
            params![board_id, BoardRole::Owner],
            |row| row.get(0),
        )?;
        let Some((merged, projected)) = prepared.merged else {
            tx.commit()?;
            return Ok(Some((false, revision, previous)));
        };
        let mut data = previous.clone();
        data.columns = projected.columns;
        data.archives = projected.archives;
        data.templates = projected.templates;
        write_board_transaction(&tx, board_id, &data)?;
        task_events::record(&tx, board_id, &previous, &data, &EventSource::remote())?;
        tx.execute("INSERT INTO cardbe_loro_docs(board_id,payload) VALUES(?1,?2) ON CONFLICT(board_id) DO UPDATE SET payload=excluded.payload", params![board_id, merged])?;
        tx.execute(
            "UPDATE cardbe_boards SET sync_revision=sync_revision+1,sync_status=?2 WHERE id=?1",
            params![board_id, SyncStatus::Synced],
        )?;
        tx.commit()?;
        Ok(Some((true, revision + 1, data)))
    }

    /// Client-side counterpart: merge an owner-approved diff into the local
    /// document and atomically refresh only the CRDT-owned projection.
    pub fn merge_iroh_loro_diff(
        &mut self,
        board_id: i64,
        update: &[u8],
        name: &str,
        role: IrohPermission,
        revision: i64,
        sent: &[u8],
    ) -> StorageResult<bool> {
        let previous = self.read_board_complete(board_id)?;
        let current = self
            .iroh_loro_update(board_id)?
            .ok_or("Editable board is missing its Loro document")?;
        let changed_during_sync = current != sent;
        let merged = crate::loro_board::merge(Some(&current), update)
            .map_err(|e| format!("Invalid Loro update: {e}"))?;
        let projected = crate::loro_board::project(&merged)
            .map_err(|e| format!("Invalid Loro projection: {e}"))?;
        validate_remote_board_size(Some(&previous), &projected)?;
        let content_changed = previous.columns != projected.columns
            || previous.archives != projected.archives
            || previous.templates != projected.templates
            || self.board_role(board_id)? != BoardRole::from(role);
        let mut data = previous.clone();
        data.columns = projected.columns;
        data.archives = projected.archives;
        data.templates = projected.templates;
        let tx = self.connection.transaction()?;
        if content_changed {
            write_board_transaction(&tx, board_id, &data)?;
            task_events::record(&tx, board_id, &previous, &data, &EventSource::remote())?;
        }
        if !crate::loro_board::same_state_vector(&current, &merged)? {
            tx.execute("INSERT INTO cardbe_loro_docs(board_id,payload) VALUES(?1,?2) ON CONFLICT(board_id) DO UPDATE SET payload=excluded.payload", params![board_id, merged])?;
        }
        tx.execute("UPDATE cardbe_boards SET name=?2,shared_role=?3,sync_revision=?4,sync_status=?5 WHERE id=?1", params![board_id, name, role, revision, if changed_during_sync { SyncStatus::Pending } else { SyncStatus::Synced }])?;
        tx.commit()?;
        if self.active_board_id == board_id {
            self.positions = self.load_board_positions(board_id)?;
        }
        Ok(content_changed)
    }

    pub fn save_iroh_conflict(
        &mut self,
        board_id: i64,
        revision: i64,
        name: &str,
        permission: IrohPermission,
        data: &StoredData,
        loro_update: Option<&[u8]>,
    ) -> StorageResult<()> {
        validate_iroh_snapshot(data, permission, loro_update)?;
        let tx = self.connection.transaction()?;
        tx.execute("INSERT INTO cardbe_iroh_conflicts(board_id,revision,name,permission,payload,loro_update) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(board_id) DO UPDATE SET revision=excluded.revision,name=excluded.name,permission=excluded.permission,payload=excluded.payload,loro_update=excluded.loro_update", params![board_id, revision, name, permission, serde_json::to_string(data)?, loro_update])?;
        tx.execute(
            "UPDATE cardbe_boards SET sync_status=?2,shared_role=?3 WHERE id=?1",
            params![board_id, SyncStatus::Conflict, BoardRole::from(permission)],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn iroh_conflict(&self, board_id: i64) -> StorageResult<Option<IrohConflict>> {
        let raw = self.connection.query_row(
            "SELECT revision,name,permission,payload,loro_update FROM cardbe_iroh_conflicts WHERE board_id=?1",
            [board_id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, IrohPermission>(2)?, r.get::<_, String>(3)?, r.get::<_, Option<Vec<u8>>>(4)?)),
        ).optional()?;
        raw.map(|(revision, name, permission, payload, loro_update)| {
            Ok(IrohConflict {
                revision,
                name,
                permission,
                data: serde_json::from_str(&payload)?,
                loro_update,
            })
        })
        .transpose()
    }

    pub fn keep_iroh_conflict_local(&mut self, board_id: i64) -> StorageResult<()> {
        let conflict = self
            .iroh_conflict(board_id)?
            .ok_or("No saved sync conflict")?;
        if conflict.permission != IrohPermission::Editor {
            return Err("The invitation is read-only; save your changes as a copy instead".into());
        }
        validate_iroh_snapshot(
            &conflict.data,
            conflict.permission,
            conflict.loro_update.as_deref(),
        )?;
        let local = self.read_board_complete(board_id)?;
        let mut peer = crate::loro_board::LocalPeer::default();
        let rebased = crate::loro_board::apply_local_delta_with_peer(
            conflict.loro_update.as_deref(),
            &conflict.data,
            &local,
            &mut peer,
        )
        .map_err(|error| format!("Could not rebase local changes: {error}"))?;
        let tx = self.connection.transaction()?;
        tx.execute("INSERT INTO cardbe_loro_docs(board_id,payload) VALUES(?1,?2) ON CONFLICT(board_id) DO UPDATE SET payload=excluded.payload", params![board_id, rebased])?;
        tx.execute(
            "UPDATE cardbe_boards SET shared_role=?2,sync_status=?3,sync_revision=?4 WHERE id=?1",
            params![
                board_id,
                BoardRole::Editor,
                SyncStatus::Pending,
                conflict.revision
            ],
        )?;
        tx.execute(
            "DELETE FROM cardbe_iroh_conflicts WHERE board_id=?1",
            [board_id],
        )?;
        tx.commit()?;
        self.loro_peers.insert(board_id, peer);
        Ok(())
    }

    pub fn iroh_remote_tickets(&self) -> StorageResult<Vec<String>> {
        Ok(self
            .connection
            .prepare("SELECT invitation FROM cardbe_iroh_remotes")?
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn iroh_invites(&self) -> StorageResult<Vec<IrohInviteRecord>> {
        Ok(self
            .connection
            .prepare(
                "SELECT invite_id,board_id,secret,permission,enabled FROM cardbe_iroh_invites",
            )?
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get::<_, i64>(4)? != 0,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn iroh_invite(&self, invite_id: &str) -> StorageResult<Option<IrohInviteRecord>> {
        Ok(self
            .connection
            .query_row(
                "SELECT invite_id,board_id,secret,permission,enabled FROM cardbe_iroh_invites WHERE invite_id=?1",
                [invite_id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get::<_, i64>(4)? != 0,
                    ))
                },
            )
            .optional()?)
    }

    pub fn iroh_invite_summaries(&self) -> StorageResult<Vec<IrohInviteSummaryRecord>> {
        Ok(self.connection.prepare("SELECT invite_id,board_id,permission,enabled,created_at,secret FROM cardbe_iroh_invites ORDER BY created_at DESC")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)? != 0, r.get(4)?, r.get(5)?)))?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn update_iroh_invite(
        &mut self,
        invite_id: &str,
        permission: Option<IrohPermission>,
        enabled: Option<bool>,
    ) -> StorageResult<()> {
        if self.connection.execute(
            "UPDATE cardbe_iroh_invites SET permission=COALESCE(?2,permission),enabled=COALESCE(?3,enabled) WHERE invite_id=?1",
            params![invite_id, permission, enabled.map(i64::from)],
        )? == 0 {
            return Err(crate::errors::DomainError::InviteNotFound.into());
        }
        Ok(())
    }

    pub fn delete_iroh_invite(&mut self, invite_id: &str) -> StorageResult<()> {
        if self.connection.execute(
            "DELETE FROM cardbe_iroh_invites WHERE invite_id=?1",
            [invite_id],
        )? == 0
        {
            return Err(crate::errors::DomainError::InviteNotFound.into());
        }
        Ok(())
    }

    /// Deleted invitations still need an endpoint to report revocation to offline peers.
    // ponytail: retain the endpoint indefinitely; retire it only with peer acknowledgements.
    pub fn iroh_host_required(&self) -> StorageResult<bool> {
        Ok(self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM cardbe_iroh_invites WHERE enabled=1) OR EXISTS(SELECT 1 FROM cardbe_global_metadata WHERE key='iroh_has_revocations')",
            [],
            |row| row.get(0),
        )?)
    }

    pub fn iroh_endpoint_seed(&mut self) -> StorageResult<String> {
        if let Some(seed) = self
            .connection
            .query_row(
                "SELECT value FROM cardbe_global_metadata WHERE key='iroh_endpoint_seed'",
                [],
                |r| r.get(0),
            )
            .optional()?
        {
            return Ok(seed);
        }
        let seed = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(iroh::SecretKey::generate().to_bytes());
        self.connection.execute(
            "INSERT INTO cardbe_global_metadata(key,value) VALUES('iroh_endpoint_seed',?1)",
            [&seed],
        )?;
        Ok(seed)
    }

    /// The last automatically selected UDP port for this device's sharing
    /// endpoint, so peers' saved invitation addresses stay valid after restart.
    pub fn iroh_automatic_port(&self) -> StorageResult<Option<u16>> {
        Ok(
            namespaced_metadata_value(&self.connection, "iroh_automatic_port")?
                .and_then(|value| value.parse().ok())
                .filter(|port| *port != 0),
        )
    }

    pub fn set_iroh_automatic_port(&mut self, port: u16) -> StorageResult<()> {
        self.connection.execute(
            "INSERT INTO cardbe_global_metadata(key,value) VALUES('iroh_automatic_port',?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [port.to_string()],
        )?;
        Ok(())
    }

    pub fn apply_iroh_snapshot(
        &mut self,
        board_id: i64,
        name: &str,
        role: IrohPermission,
        revision: i64,
        data: &StoredData,
        loro_update: Option<&[u8]>,
    ) -> StorageResult<()> {
        validate_iroh_snapshot(data, role, loro_update)?;
        let previous = self.read_board_complete(board_id)?;
        let tx = self.connection.transaction()?;
        if tx
            .query_row("SELECT 1 FROM cardbe_boards WHERE id=?1", [board_id], |r| {
                r.get::<_, i64>(0)
            })
            .optional()?
            .is_none()
        {
            return Err(DomainError::BoardNotFound.into());
        }
        write_board_transaction(&tx, board_id, data)?;
        task_events::record(&tx, board_id, &previous, data, &EventSource::remote())?;
        if let Some(update) = loro_update.filter(|bytes| !bytes.is_empty()) {
            tx.execute("INSERT INTO cardbe_loro_docs(board_id,payload) VALUES(?1,?2) ON CONFLICT(board_id) DO UPDATE SET payload=excluded.payload", params![board_id, update])?;
        } else {
            tx.execute("DELETE FROM cardbe_loro_docs WHERE board_id=?1", [board_id])?;
        }
        tx.execute("UPDATE cardbe_boards SET name=?2,shared_role=?3,sync_status=?4,sync_revision=?5 WHERE id=?1", params![board_id, name, role, SyncStatus::Synced, revision])?;
        tx.execute(
            "DELETE FROM cardbe_iroh_conflicts WHERE board_id=?1",
            [board_id],
        )?;
        tx.commit()?;
        self.loro_peers.remove(&board_id);
        if self.active_board_id == board_id {
            self.positions = self.load_board_positions(board_id)?;
        }
        Ok(())
    }

    pub fn use_iroh_conflict_remote(
        &mut self,
        board_id: i64,
        local_name: &str,
        local: &StoredData,
    ) -> StorageResult<Board> {
        let conflict = self
            .iroh_conflict(board_id)?
            .ok_or("No saved sync conflict")?;
        validate_iroh_snapshot(
            &conflict.data,
            conflict.permission,
            conflict.loro_update.as_deref(),
        )?;
        let previous = self.read_board_complete(board_id)?;
        let tx = self.connection.transaction()?;
        let copy_name = format!("{local_name} (conflict copy)");
        tx.execute("INSERT INTO cardbe_boards(name) VALUES(?1)", [&copy_name])?;
        let copy_id = tx.last_insert_rowid();
        write_board_transaction(&tx, copy_id, local)?;
        write_board_transaction(&tx, board_id, &conflict.data)?;
        task_events::record(
            &tx,
            board_id,
            &previous,
            &conflict.data,
            &EventSource::remote(),
        )?;
        if let Some(update) = conflict
            .loro_update
            .as_deref()
            .filter(|bytes| !bytes.is_empty())
        {
            tx.execute("INSERT INTO cardbe_loro_docs(board_id,payload) VALUES(?1,?2) ON CONFLICT(board_id) DO UPDATE SET payload=excluded.payload", params![board_id, update])?;
        } else {
            tx.execute("DELETE FROM cardbe_loro_docs WHERE board_id=?1", [board_id])?;
        }
        tx.execute("UPDATE cardbe_boards SET name=?2,shared_role=?3,sync_status=?4,sync_revision=?5 WHERE id=?1", params![board_id, conflict.name, conflict.permission, SyncStatus::Synced, conflict.revision])?;
        tx.execute(
            "DELETE FROM cardbe_iroh_conflicts WHERE board_id=?1",
            [board_id],
        )?;
        tx.commit()?;
        self.loro_peers.remove(&board_id);
        if self.active_board_id == board_id {
            self.positions = self.load_board_positions(board_id)?;
        }
        Ok(Board {
            id: copy_id,
            name: copy_name,
            task_count: local
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
}
