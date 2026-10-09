use super::{LinkPreview, LinkPreviewStatus};
use crate::errors::DomainError;
use rusqlite::{params, Connection, OptionalExtension};

pub const TTL: i64 = 24 * 60 * 60;
pub const UNAVAILABLE_TTL: i64 = 5 * 60;

pub fn initialize(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch("CREATE TABLE IF NOT EXISTS link_preview_cache(url TEXT PRIMARY KEY, payload TEXT NOT NULL, expires_at INTEGER NOT NULL);")
}

pub fn read(
    connection: &Connection,
    url: &str,
    now: i64,
) -> Result<Option<LinkPreview>, DomainError> {
    let payload: Option<String> = connection
        .query_row(
            "SELECT payload FROM link_preview_cache WHERE url=?1 AND expires_at>?2",
            params![url, now],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| DomainError::Internal(error.to_string()))?;
    payload
        .map(|payload| {
            serde_json::from_str(&payload).map_err(|error| DomainError::Internal(error.to_string()))
        })
        .transpose()
}

pub fn write(connection: &Connection, preview: &LinkPreview) -> Result<(), DomainError> {
    let ttl = if preview.status == LinkPreviewStatus::Unavailable {
        UNAVAILABLE_TTL
    } else {
        TTL
    };
    let payload =
        serde_json::to_string(preview).map_err(|error| DomainError::Internal(error.to_string()))?;
    connection
        .execute(
            "DELETE FROM link_preview_cache WHERE expires_at<=?1",
            [preview.fetched_at],
        )
        .map_err(|error| DomainError::Internal(error.to_string()))?;
    connection
        .execute(
            "INSERT OR REPLACE INTO link_preview_cache(url,payload,expires_at) VALUES(?1,?2,?3)",
            params![preview.url, payload, preview.fetched_at + ttl],
        )
        .map_err(|error| DomainError::Internal(error.to_string()))?;
    // ponytail: cap metadata at 1000 entries; use LRU if cache churn matters.
    connection.execute("DELETE FROM link_preview_cache WHERE url IN (SELECT url FROM link_preview_cache ORDER BY expires_at DESC LIMIT -1 OFFSET 1000)", []).map_err(|error| DomainError::Internal(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_results_expire_including_unavailable() {
        let connection = Connection::open_in_memory().unwrap();
        initialize(&connection).unwrap();
        let mut preview = LinkPreview::unavailable("https://example.com/");
        preview.fetched_at = 100;
        write(&connection, &preview).unwrap();
        assert_eq!(
            read(&connection, &preview.url, 101)
                .unwrap()
                .unwrap()
                .status,
            LinkPreviewStatus::Unavailable
        );
        assert!(read(&connection, &preview.url, 100 + UNAVAILABLE_TTL)
            .unwrap()
            .is_none());
        preview.status = LinkPreviewStatus::Partial;
        write(&connection, &preview).unwrap();
        assert!(read(&connection, &preview.url, 100 + UNAVAILABLE_TTL)
            .unwrap()
            .is_some());
        assert!(read(&connection, &preview.url, 100 + TTL)
            .unwrap()
            .is_none());
    }
}
