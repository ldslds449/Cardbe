//! A task can wait on other tasks of the same board. It is blocked while any
//! of them is still open; references to tasks that are done, archived, deleted
//! or moved to another board no longer block it, so nothing has to clean them
//! up when those tasks change.

use crate::{errors::DomainError, models::StoredData};
use std::collections::{HashMap, HashSet};

pub const MAX_BLOCKERS: usize = 50;

/// Checks the tasks a local edit makes `task_id` wait on. Newly added ones
/// must be active tasks of this board other than itself, without duplicates,
/// and must not wait on `task_id` themselves, directly or through others.
/// References kept from `previous` may point to archived or removed tasks.
pub fn validate(
    data: &StoredData,
    task_id: i64,
    previous: &[i64],
    next: &[i64],
) -> Result<(), DomainError> {
    if next.len() > MAX_BLOCKERS || next.contains(&task_id) {
        return Err(DomainError::InvalidArgument);
    }
    let mut seen = HashSet::new();
    if !next.iter().all(|id| seen.insert(*id)) {
        return Err(DomainError::InvalidArgument);
    }
    let added: Vec<i64> = next
        .iter()
        .copied()
        .filter(|id| !previous.contains(id))
        .collect();
    if added.is_empty() {
        return Ok(());
    }
    let waits: HashMap<i64, &[i64]> = data
        .columns
        .iter()
        .flat_map(|column| &column.tasks)
        .map(|task| (task.id, task.blocked_by.as_slice()))
        .collect();
    if added.iter().any(|id| !waits.contains_key(id)) {
        return Err(DomainError::InvalidArgument);
    }
    let mut visited = HashSet::new();
    let mut pending = added;
    while let Some(id) = pending.pop() {
        if id == task_id {
            return Err(DomainError::InvalidArgument);
        }
        if visited.insert(id) {
            pending.extend(waits.get(&id).copied().unwrap_or_default());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate, MAX_BLOCKERS};
    use crate::models::{Column, StoredData, Task};

    fn board(tasks: &[(i64, &[i64])]) -> StoredData {
        StoredData {
            columns: vec![Column {
                id: 1,
                name: String::new(),
                color: String::new(),
                sort_order: Default::default(),
                sort_rules: Vec::new(),
                done: false,
                tasks: tasks
                    .iter()
                    .map(|(id, blocked_by)| Task {
                        id: *id,
                        blocked_by: blocked_by.to_vec(),
                        ..Task::default()
                    })
                    .collect(),
            }],
            ..StoredData::default()
        }
    }

    #[test]
    fn accepts_waiting_on_open_tasks_of_the_same_board() {
        let data = board(&[(1, &[]), (2, &[]), (3, &[2])]);
        assert!(validate(&data, 1, &[], &[2, 3]).is_ok());
        assert!(validate(&data, 1, &[], &[]).is_ok());
    }

    #[test]
    fn rejects_itself_duplicates_unknown_tasks_and_too_many() {
        let data = board(&[(1, &[]), (2, &[])]);
        assert!(validate(&data, 1, &[], &[1]).is_err());
        assert!(validate(&data, 1, &[], &[2, 2]).is_err());
        assert!(validate(&data, 1, &[], &[9]).is_err());
        let many: Vec<i64> = (100..=100 + MAX_BLOCKERS as i64).collect();
        assert!(validate(&data, 1, &[], &many).is_err());
    }

    #[test]
    fn keeps_existing_references_to_tasks_that_left_the_board() {
        let data = board(&[(1, &[9]), (2, &[])]);
        assert!(validate(&data, 1, &[9], &[9, 2]).is_ok());
    }

    #[test]
    fn rejects_cycles_through_other_tasks() {
        let data = board(&[(1, &[]), (2, &[3]), (3, &[1])]);
        assert!(validate(&data, 1, &[], &[3]).is_err());
        assert!(validate(&data, 1, &[], &[2]).is_err());
    }
}
