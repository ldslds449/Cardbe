//! Device-local MCP server settings and client grants. They live outside
//! `Settings`, board synchronization, backups and Undo; tokens are stored only
//! as SHA-256 digests here and as values in the OS keyring.
use crate::{errors::DomainError, operations::Grants};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const MAX_CLIENTS: usize = 32;
const MAX_NAME_CHARS: usize = 100;
const METADATA_KEY: &str = "mcp_server";
const KEYRING_SERVICE: &str = "cardbe.mcp";
const TOKEN_PREFIX: &str = "cardbe_mcp_";

pub const DEFAULT_PORT: u16 = 8765;

/// Development instances default to distinct ports, like their Vite ports.
pub fn default_port() -> u16 {
    #[cfg(debug_assertions)]
    if let Some(offset) = std::env::var("CARDBE_DEV_PORT")
        .ok()
        .and_then(|port| port.parse::<u16>().ok())
        .and_then(|port| port.checked_sub(1420))
    {
        return DEFAULT_PORT.saturating_add(offset);
    }
    DEFAULT_PORT
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct McpConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub clients: Vec<McpClient>,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_start: false,
            port: default_port(),
            clients: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct McpClient {
    pub id: String,
    pub name: String,
    /// Hex SHA-256 of the access token.
    pub token_hash: String,
    /// Includes boards created later. Older settings default to the
    /// selected boards only.
    #[serde(default)]
    pub all_boards: bool,
    #[serde(default)]
    pub boards: BTreeSet<i64>,
    #[serde(default)]
    pub write: bool,
    pub created_at: i64,
}

impl McpClient {
    pub fn grants(&self) -> Grants {
        Grants {
            all_boards: self.all_boards,
            boards: self.boards.clone(),
            write: self.write,
        }
    }
}

pub fn load(connection: &Connection) -> Result<McpConfig, DomainError> {
    let value: Option<String> = connection
        .query_row(
            "SELECT value FROM cardbe_global_metadata WHERE key=?1",
            [METADATA_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| DomainError::Internal(error.to_string()))?;
    match value {
        // Unreadable settings fall back to a stopped server rather than failing startup.
        Some(value) => Ok(serde_json::from_str(&value).unwrap_or_else(|error| {
            log::warn!(target: "mcp", "Ignoring unreadable MCP settings: {error}");
            McpConfig::default()
        })),
        None => Ok(McpConfig::default()),
    }
}

pub fn save(connection: &Connection, config: &McpConfig) -> Result<(), DomainError> {
    let value =
        serde_json::to_string(config).map_err(|error| DomainError::Internal(error.to_string()))?;
    connection
        .execute(
            "INSERT INTO cardbe_global_metadata(key,value) VALUES(?1,?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [METADATA_KEY, value.as_str()],
        )
        .map_err(|error| DomainError::Internal(error.to_string()))?;
    Ok(())
}

pub fn valid_port(port: u16) -> bool {
    port >= 1024
}

/// Trimmed display name, 1 to 100 characters.
pub fn client_name(name: &str) -> Result<String, DomainError> {
    let name = name.trim();
    if name.is_empty()
        || name.chars().count() > MAX_NAME_CHARS
        || name.chars().any(char::is_control)
    {
        return Err(DomainError::InvalidArgument);
    }
    Ok(name.into())
}

pub fn generate_token() -> String {
    format!(
        "{TOKEN_PREFIX}{}",
        URL_SAFE_NO_PAD.encode(iroh::SecretKey::generate().to_bytes())
    )
}

pub fn token_digest(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

pub fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

pub fn parse_hash(hash: &str) -> Option<[u8; 32]> {
    if hash.len() != 64 {
        return None;
    }
    let mut digest = [0u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hash.get(index * 2..index * 2 + 2)?, 16).ok()?;
    }
    Some(digest)
}

fn keyring_entry(namespace: &str, client_id: &str) -> Result<keyring::Entry, DomainError> {
    keyring::Entry::new(KEYRING_SERVICE, &format!("{namespace}:{client_id}"))
        .map_err(|_| "Secret storage unavailable".into())
}

/// Saves the token for later display; `false` when the keyring is unavailable,
/// so it can be shown only once.
pub fn store_token(namespace: &str, client_id: &str, token: &str) -> bool {
    keyring_entry(namespace, client_id)
        .and_then(|entry| {
            entry
                .set_password(token)
                .map_err(|_| "Secret storage unavailable".into())
        })
        .is_ok()
}

pub fn read_token(namespace: &str, client_id: &str) -> Option<String> {
    keyring_entry(namespace, client_id)
        .ok()?
        .get_password()
        .ok()
}

pub fn delete_token(namespace: &str, client_id: &str) {
    if let Ok(entry) = keyring_entry(namespace, client_id) {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(_) => log::warn!(target: "mcp", "Could not remove an MCP client token"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_random_and_only_their_digest_is_persisted() {
        let token = generate_token();
        assert!(token.starts_with(TOKEN_PREFIX));
        assert_eq!(token.len(), TOKEN_PREFIX.len() + 43);
        assert_ne!(token, generate_token());
        let hash = token_hash(&token);
        assert_eq!(parse_hash(&hash), Some(token_digest(&token)));
        assert!(!hash.contains(&token[TOKEN_PREFIX.len()..]));
        assert_eq!(parse_hash("zz"), None);
        assert_eq!(parse_hash(&"g".repeat(64)), None);
    }

    #[test]
    fn settings_default_to_a_stopped_loopback_server_and_round_trip() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE cardbe_global_metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL)",
            )
            .unwrap();
        let config = load(&connection).unwrap();
        assert!(!config.enabled && !config.auto_start);
        assert!(config.clients.is_empty());
        let saved = McpConfig {
            enabled: true,
            port: 9000,
            clients: vec![McpClient {
                id: "client".into(),
                name: "Claude 🤖".into(),
                token_hash: token_hash("token"),
                all_boards: true,
                boards: [1, 2].into(),
                write: true,
                created_at: 1,
            }],
            ..McpConfig::default()
        };
        save(&connection, &saved).unwrap();
        assert_eq!(load(&connection).unwrap(), saved);
        // Clients saved before board scopes keep only their selected boards.
        let legacy: McpClient = serde_json::from_str(
            r#"{"id":"c","name":"n","token_hash":"h","boards":[1],"created_at":1}"#,
        )
        .unwrap();
        assert!(!legacy.all_boards && legacy.grants().allows(1, crate::operations::Access::Read));
        assert!(!legacy.grants().allows(2, crate::operations::Access::Read));
        connection
            .execute(
                "UPDATE cardbe_global_metadata SET value='not json' WHERE key=?1",
                [METADATA_KEY],
            )
            .unwrap();
        assert_eq!(load(&connection).unwrap(), McpConfig::default());
    }

    #[test]
    fn names_and_ports_are_validated() {
        assert_eq!(client_name("  Claude Code ").unwrap(), "Claude Code");
        assert_eq!(client_name(&"字".repeat(100)).unwrap().chars().count(), 100);
        for name in ["", "   ", "a\nb"] {
            assert!(client_name(name).is_err());
        }
        assert!(client_name(&"字".repeat(101)).is_err());
        assert!(!valid_port(0) && !valid_port(80));
        assert!(valid_port(8765) && valid_port(65535));
    }
}
