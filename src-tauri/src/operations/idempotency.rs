//! Recent `task.create` idempotency keys per caller, so a retried call returns
//! the task it created instead of a duplicate. Kept in memory only: keys expire
//! after a day and are forgotten when Cardbe restarts.
use super::Caller;
use crate::errors::DomainError;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

const TTL: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_KEYS: usize = 10_000;
const MAX_KEY_BYTES: usize = 128;

struct Created {
    board_id: i64,
    task_id: i64,
    fingerprint: [u8; 32],
    at: Instant,
}

pub enum Recorded<R> {
    New(R),
    Existing { board_id: i64, task_id: i64 },
}

#[derive(Default)]
pub struct CreatedTasks(Mutex<HashMap<(Caller, String), Created>>);

/// Digest of the call's arguments, excluding the key itself.
pub fn fingerprint(arguments: &impl serde::Serialize) -> Result<[u8; 32], DomainError> {
    let bytes =
        serde_json::to_vec(arguments).map_err(|error| DomainError::Internal(error.to_string()))?;
    Ok(Sha256::digest(bytes).into())
}

impl CreatedTasks {
    /// Runs `create`, which returns its result and the new task ID, unless the
    /// caller already used `key`. Reusing a key with other arguments is invalid.
    /// The lock is held across `create` so concurrent retries create one task.
    pub fn once<R>(
        &self,
        caller: &Caller,
        key: &str,
        board_id: i64,
        fingerprint: [u8; 32],
        create: impl FnOnce() -> Result<(R, i64), DomainError>,
    ) -> Result<Recorded<R>, DomainError> {
        if key.is_empty() || key.len() > MAX_KEY_BYTES || key.chars().any(char::is_control) {
            return Err(DomainError::InvalidArgument);
        }
        let mut created = self
            .0
            .lock()
            .map_err(|_| DomainError::Internal("Idempotency keys are poisoned".into()))?;
        let now = Instant::now();
        created.retain(|_, entry| now.duration_since(entry.at) < TTL);
        let id = (caller.clone(), key.to_owned());
        if let Some(entry) = created.get(&id) {
            if entry.fingerprint != fingerprint {
                return Err(DomainError::InvalidArgument);
            }
            return Ok(Recorded::Existing {
                board_id: entry.board_id,
                task_id: entry.task_id,
            });
        }
        let (result, task_id) = create()?;
        if created.len() >= MAX_KEYS {
            if let Some(oldest) = created
                .iter()
                .min_by_key(|(_, entry)| entry.at)
                .map(|(id, _)| id.clone())
            {
                created.remove(&oldest);
            }
        }
        created.insert(
            id,
            Created {
                board_id,
                task_id,
                fingerprint,
                at: now,
            },
        );
        Ok(Recorded::New(result))
    }
}
