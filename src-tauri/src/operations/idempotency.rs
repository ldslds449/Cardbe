//! Recent creation idempotency keys per caller and tool, so a retried
//! `task.create` or `column.create` returns what it created instead of a
//! duplicate. Kept in memory only: keys expire after a day and are forgotten
//! when Cardbe restarts.
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
    id: i64,
    fingerprint: [u8; 32],
    at: Instant,
}

pub enum Recorded<R> {
    New(R),
    Existing { board_id: i64, id: i64 },
}

/// Keys are scoped to the caller and the tool, so one key used with two tools
/// creates once for each.
#[derive(Default)]
pub struct Creations(Mutex<HashMap<(Caller, &'static str, String), Created>>);

/// Digest of the call's arguments, excluding the key itself.
pub fn fingerprint(arguments: &impl serde::Serialize) -> Result<[u8; 32], DomainError> {
    let bytes =
        serde_json::to_vec(arguments).map_err(|error| DomainError::Internal(error.to_string()))?;
    Ok(Sha256::digest(bytes).into())
}

impl Creations {
    /// Runs `create`, which returns its result and the new entity's ID, unless
    /// the caller already used `key` with `tool`. Reusing a key with other
    /// arguments is invalid. The lock is held across `create` so concurrent
    /// retries create once.
    pub fn once<R>(
        &self,
        caller: &Caller,
        tool: &'static str,
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
        let key = (caller.clone(), tool, key.to_owned());
        if let Some(entry) = created.get(&key) {
            if entry.fingerprint != fingerprint {
                return Err(DomainError::InvalidArgument);
            }
            return Ok(Recorded::Existing {
                board_id: entry.board_id,
                id: entry.id,
            });
        }
        let (result, id) = create()?;
        if created.len() >= MAX_KEYS {
            if let Some(oldest) = created
                .iter()
                .min_by_key(|(_, entry)| entry.at)
                .map(|(key, _)| key.clone())
            {
                created.remove(&oldest);
            }
        }
        created.insert(
            key,
            Created {
                board_id,
                id,
                fingerprint,
                at: now,
            },
        );
        Ok(Recorded::New(result))
    }
}
