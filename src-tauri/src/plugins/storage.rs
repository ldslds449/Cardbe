use super::types::*;
use crate::errors::DomainError;
use rusqlite::{params, Connection};
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
