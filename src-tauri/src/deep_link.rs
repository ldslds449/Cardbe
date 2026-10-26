use crate::errors::{CommandError, DomainError};
use crate::state::{lock_app, SharedAppData};
use crate::storage::{Database, StorageResult};
use rusqlite::OptionalExtension;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Mutex;
use tauri::{Emitter, Manager, State};

const MAX_URL: usize = 32_768;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Link {
    Invite {
        ticket: String,
    },
    Board {
        reference: String,
        task_id: Option<i64>,
    },
    Invalid,
}

pub fn valid_reference(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn parse(value: &str) -> Link {
    if value.len() > MAX_URL || value.bytes().any(|b| b.is_ascii_control()) {
        return Link::Invalid;
    }
    if let Some(payload) = value.strip_prefix("cardbe://share/") {
        if !payload.is_empty()
            && payload
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Link::Invite {
                ticket: value.into(),
            };
        }
    }
    if let Some(path) = value.strip_prefix("cardbe://board/") {
        let parts: Vec<_> = path.split('/').collect();
        if valid_reference(parts[0]) {
            if parts.len() == 1 {
                return Link::Board {
                    reference: parts[0].into(),
                    task_id: None,
                };
            }
            if parts.len() == 3
                && parts[1] == "task"
                && parts[2].bytes().all(|b| b.is_ascii_digit())
            {
                if let Ok(id @ 0..=9_007_199_254_740_991) = parts[2].parse::<i64>() {
                    return Link::Board {
                        reference: parts[0].into(),
                        task_id: Some(id),
                    };
                }
            }
        }
    }
    Link::Invalid
}

#[derive(Default)]
pub struct Pending(Mutex<VecDeque<Link>>);

pub fn receive(app: &tauri::AppHandle, urls: impl IntoIterator<Item = String>) {
    let state = app.state::<Pending>();
    if let Ok(mut queue) = state.0.lock() {
        for url in urls {
            let link = parse(&url);
            if !queue.contains(&link) && queue.len() < 32 {
                queue.push_back(link);
            }
        }
    }
    #[cfg(desktop)]
    crate::desktop::show_main_window(app);
    // The event only wakes the main window; invitation secrets stay in memory.
    let _ = app.emit_to("main", "cardbe:deep-link", ());
}

#[tauri::command]
pub fn take_deep_links(
    window: tauri::WebviewWindow,
    state: State<'_, Pending>,
) -> Result<Vec<Link>, CommandError> {
    if window.label() != "main" {
        return Err(CommandError::PermissionDenied);
    }
    Ok(state
        .0
        .lock()
        .map_err(|_| CommandError::internal("Deep link queue unavailable"))?
        .drain(..)
        .collect())
}

impl Database {
    pub fn board_link_reference(&self, board_id: i64) -> StorageResult<String> {
        if !self.board_exists(board_id)? {
            return Err(DomainError::BoardNotFound.into());
        }
        if let Some(value) = self.connection.query_row("SELECT value FROM cardbe_board_metadata WHERE board_id=?1 AND key='link_reference'", [board_id], |row| row.get::<_, String>(0)).optional()? {
            return Ok(value);
        }
        // Receivers learn the owner's identity from an authenticated snapshot.
        if self.board_role(board_id)? != crate::models::BoardRole::Owner {
            return Err(DomainError::BoardLinkUnavailable.into());
        }
        let reference: String = iroh::SecretKey::generate()
            .to_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        self.connection.execute("INSERT INTO cardbe_board_metadata(board_id,key,value) VALUES(?1,'link_reference',?2) ON CONFLICT(board_id,key) DO NOTHING", rusqlite::params![board_id, reference])?;
        self.board_link_reference(board_id)
    }

    pub fn save_board_link_reference(
        &self,
        board_id: i64,
        reference: Option<&str>,
    ) -> StorageResult<()> {
        if let Some(reference) = reference {
            if !valid_reference(reference) {
                return Err(DomainError::InvalidArgument.into());
            }
            // A remote owner must not take over another local board's links.
            let used_elsewhere = self
                .connection
                .query_row(
                    "SELECT 1 FROM cardbe_board_metadata WHERE key='link_reference' AND value=?1 AND board_id<>?2",
                    rusqlite::params![reference, board_id],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if used_elsewhere {
                log::warn!(target: "iroh", "Ignored a board link reference already used by another board");
                return Ok(());
            }
            self.connection.execute("INSERT INTO cardbe_board_metadata(board_id,key,value) VALUES(?1,'link_reference',?2) ON CONFLICT(board_id,key) DO UPDATE SET value=excluded.value", rusqlite::params![board_id, reference])?;
        }
        Ok(())
    }
}

#[tauri::command]
pub fn create_board_link(
    state: State<'_, SharedAppData>,
    board_id: i64,
    task_id: Option<i64>,
) -> Result<String, CommandError> {
    let guard = lock_app(&state)?;
    let reference = guard
        .database
        .board_link_reference(board_id)
        .map_err(CommandError::repository)?;
    match task_id {
        Some(id @ 0..=9_007_199_254_740_991) => Ok(format!("cardbe://board/{reference}/task/{id}")),
        None => Ok(format!("cardbe://board/{reference}")),
        _ => Err(DomainError::InvalidArgument.into()),
    }
}

#[tauri::command]
pub fn resolve_board_link(
    state: State<'_, SharedAppData>,
    reference: String,
) -> Result<i64, CommandError> {
    if !valid_reference(&reference) {
        return Err(DomainError::InvalidArgument.into());
    }
    let guard = lock_app(&state)?;
    guard.database.connection.query_row("SELECT board_id FROM cardbe_board_metadata WHERE key='link_reference' AND value=?1 ORDER BY board_id LIMIT 1", [&reference], |row| row.get(0)).optional().map_err(|error| CommandError::repository(Box::new(error)))?.ok_or(DomainError::BoardNotFound.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identities_survive_replacements_and_map_across_local_ids() {
        let dir = std::env::temp_dir().join(format!(
            "cardbe-links-{}",
            iroh::SecretKey::generate().public()
        ));
        let mut loaded = crate::storage::load(&dir).unwrap();
        let owner = loaded.database.active_board_id();
        let reference = loaded.database.board_link_reference(owner).unwrap();
        assert!(valid_reference(&reference));
        let other = loaded.database.create_board("Other").unwrap();
        assert_ne!(
            loaded.database.board_link_reference(other.id).unwrap(),
            reference
        );
        loaded
            .database
            .replace_board_as_local_edit(owner, &Default::default())
            .unwrap();
        assert_eq!(
            loaded.database.board_link_reference(owner).unwrap(),
            reference
        );
        let received = loaded
            .database
            .create_received_board(
                "Received",
                &Default::default(),
                crate::models::IrohPermission::Viewer,
                1,
                "fixture",
                None,
            )
            .unwrap();
        assert!(matches!(
            CommandError::from(DomainError::repository(
                loaded
                    .database
                    .board_link_reference(received.id)
                    .unwrap_err()
            )),
            CommandError::BoardLinkUnavailable
        ));
        assert!(loaded
            .database
            .save_board_link_reference(received.id, Some("bad"))
            .is_err());
        // A reference owned by another local board is ignored.
        loaded
            .database
            .save_board_link_reference(received.id, Some(&reference))
            .unwrap();
        assert!(loaded.database.board_link_reference(received.id).is_err());
        let remote = "b".repeat(64);
        loaded
            .database
            .save_board_link_reference(received.id, Some(&remote))
            .unwrap();
        assert_eq!(
            loaded.database.board_link_reference(received.id).unwrap(),
            remote
        );
        assert!(!loaded.stored.settings.iroh_network.enabled);
        drop(loaded);
        let loaded = crate::storage::load(&dir).unwrap();
        assert_eq!(
            loaded.database.board_link_reference(owner).unwrap(),
            reference
        );
        assert_eq!(
            loaded.database.board_link_reference(received.id).unwrap(),
            remote
        );
        drop(loaded);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn strict_routes() {
        let reference = "a".repeat(64);
        assert!(matches!(
            parse(&format!("cardbe://board/{reference}")),
            Link::Board { task_id: None, .. }
        ));
        assert!(matches!(
            parse(&format!("cardbe://board/{reference}/task/42")),
            Link::Board {
                task_id: Some(42),
                ..
            }
        ));
        assert!(matches!(
            parse(&format!("cardbe://board/{reference}/task/0")),
            Link::Board {
                task_id: Some(0),
                ..
            }
        ));
        assert!(matches!(
            parse("cardbe://share/abc-_123"),
            Link::Invite { .. }
        ));
        for value in [
            "cardbe+iroh://abc".into(),
            "cardbe://share/abc?x=1".into(),
            format!("cardbe://board/{reference}/task/-1"),
            format!("cardbe://board/{reference}/task/9007199254740992"),
            format!("cardbe://board/{reference}/extra"),
            "cardbe://share/".into(),
            "x".repeat(MAX_URL + 1),
        ] {
            assert_eq!(parse(&value), Link::Invalid);
        }
    }
}
