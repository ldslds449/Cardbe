//! Writes to any board outside the open board's Undo history, committed and
//! installed the same way as plugin changesets.
use super::OperationHost;
use crate::{
    errors::DomainError,
    models::{BoardRole, StoredData},
    state::{lock_app, AppData},
};

fn internal(error: impl std::fmt::Display) -> DomainError {
    DomainError::Internal(error.to_string())
}

/// Reads the board fresh from SQLite under the application lock, applies
/// `change`, and notifies listeners when anything was saved.
pub(super) fn commit<R>(
    host: &dyn OperationHost,
    board_id: i64,
    change: impl FnOnce(&mut StoredData) -> Result<R, DomainError>,
) -> Result<R, DomainError> {
    let (result, changed) = {
        let mut guard = lock_app(host.app_data()?)?;
        commit_locked(&mut guard, board_id, change)?
    };
    if changed {
        host.board_changed(board_id);
    }
    Ok(result)
}

fn commit_locked<R>(
    guard: &mut AppData,
    board_id: i64,
    change: impl FnOnce(&mut StoredData) -> Result<R, DomainError>,
) -> Result<(R, bool), DomainError> {
    let role = guard
        .database
        .board_role(board_id)
        .map_err(DomainError::repository)?;
    if role == BoardRole::Viewer {
        return Err(DomainError::PermissionDenied);
    }
    let before = guard
        .database
        .read_board_complete(board_id)
        .map_err(DomainError::repository)?;
    let mut after = before.clone();
    let result = change(&mut after)?;
    let now = crate::commands::archive::current_time_millis()?;
    crate::completion::reconcile(&before, &mut after, now)?;
    after.keep_pinned_tasks_first();
    if serde_json::to_vec(&after.columns).map_err(internal)?.len() > 8 * 1024 * 1024 {
        return Err(DomainError::InvalidArgument);
    }
    if before == after {
        return Ok((result, false));
    }
    let database = &mut guard.database;
    let mut positions = database
        .load_board_positions(board_id)
        .map_err(DomainError::repository)?;
    let tx = database
        .connection
        .unchecked_transaction()
        .map_err(internal)?;
    crate::storage::persist_local_board_change(
        &tx,
        board_id,
        role,
        &before,
        &after,
        &mut positions,
        database.loro_peers.entry(board_id).or_default(),
        true,
        &crate::task_events::EventSource::local(),
    )
    .map_err(DomainError::repository)?;
    tx.commit().map_err(internal)?;
    if database.active_board_id() == board_id {
        database.positions = positions;
    }
    crate::plugins::manager::install_committed(guard, board_id, after, true);
    if let Ok(boards) = guard.database.boards() {
        guard.boards = boards;
    }
    Ok((result, true))
}
