use super::types::*;
use crate::errors::DomainError;
use rusqlite::{params, Connection, OptionalExtension};
pub fn revision(c: &Connection, plugin: &str) -> Result<Option<String>, DomainError> {
    let payload: Option<String> = c
        .query_row(
            "SELECT payload FROM cardbe_plugin_data WHERE kind='revision' AND id=?1",
            [plugin],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let revision: Option<String> = payload
        .map(|s| serde_json::from_str(&s))
        .transpose()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    if revision.as_ref().is_some_and(|s| {
        s.len() != 64
            || !s
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    }) {
        return Err(DomainError::InvalidArgument);
    }
    Ok(revision)
}
pub fn initialize(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS cardbe_plugin_data(kind TEXT NOT NULL,id TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(kind,id)); CREATE TABLE IF NOT EXISTS cardbe_plugin_sources(instance_id TEXT NOT NULL,external_key TEXT NOT NULL,board_id INTEGER NOT NULL,task_id INTEGER NOT NULL,last_import TEXT NOT NULL,PRIMARY KEY(instance_id,board_id,external_key)); CREATE INDEX IF NOT EXISTS cardbe_plugin_sources_by_board_key ON cardbe_plugin_sources(board_id,external_key);")
}
pub fn list<T: serde::de::DeserializeOwned>(
    c: &Connection,
    kind: &str,
) -> Result<Vec<T>, DomainError> {
    let mut q = c
        .prepare("SELECT payload FROM cardbe_plugin_data WHERE kind=?1 ORDER BY id")
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let rows = q
        .query_map([kind], |r| r.get::<_, String>(0))
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    rows.map(|s| {
        serde_json::from_str(&s.map_err(|e| DomainError::Internal(e.to_string()))?)
            .map_err(|e| DomainError::Internal(e.to_string()))
    })
    .collect()
}
pub fn put<T: serde::Serialize>(
    c: &Connection,
    kind: &str,
    id: &str,
    value: &T,
) -> Result<(), DomainError> {
    c.execute("INSERT INTO cardbe_plugin_data(kind,id,payload) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO UPDATE SET payload=excluded.payload",params![kind,id,serde_json::to_string(value).map_err(|e| DomainError::Internal(e.to_string()))?]).map_err(|e| DomainError::Internal(e.to_string()))?;
    Ok(())
}
pub fn remove(c: &Connection, kind: &str, id: &str) -> Result<(), DomainError> {
    c.execute(
        "DELETE FROM cardbe_plugin_data WHERE kind=?1 AND id=?2",
        params![kind, id],
    )
    .map_err(|e| DomainError::Internal(e.to_string()))?;
    Ok(())
}
pub fn retain_runs(c: &Connection, instance: &str) -> Result<(), DomainError> {
    let mut runs: Vec<PluginRun> = list(c, "run")?;
    runs.retain(|r| r.instance_id == instance);
    runs.sort_by_key(|r| std::cmp::Reverse(r.started_at));
    for run in runs.into_iter().skip(50) {
        remove(c, "run", &run.id)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revision_paths_are_generated_ids_and_legacy_packages_use_the_original_component() {
        let c = Connection::open_in_memory().unwrap();
        initialize(&c).unwrap();
        assert!(revision(&c, "example").unwrap().is_none());
        for invalid in ["../outside", "a/b", "a\\b", "", "A", "component.wasm"] {
            put(&c, "revision", "example", &invalid).unwrap();
            assert!(revision(&c, "example").is_err());
        }
        let generated = id();
        put(&c, "revision", "example", &generated).unwrap();
        assert_eq!(revision(&c, "example").unwrap(), Some(generated));
    }
}
