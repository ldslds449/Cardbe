use crate::errors::DomainError;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// A line between two tasks. Arrows are only visual; Cardbe gives them no meaning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "RelationshipInput")]
pub struct Relationship {
    pub source: i64,
    pub target: i64,
    pub source_arrow: bool,
    pub target_arrow: bool,
}

// Older backups typed lines as `related_to` or `depends_on` (an arrow at the target).
#[derive(Deserialize)]
struct RelationshipInput {
    source: i64,
    target: i64,
    #[serde(default)]
    source_arrow: bool,
    #[serde(default)]
    target_arrow: bool,
    #[serde(default)]
    kind: Option<String>,
}

impl From<RelationshipInput> for Relationship {
    fn from(input: RelationshipInput) -> Self {
        Self {
            source: input.source,
            target: input.target,
            source_arrow: input.source_arrow,
            target_arrow: input.target_arrow || input.kind.as_deref() == Some("depends_on"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowPosition {
    pub task_id: i64,
    pub x: f64,
    pub y: f64,
    /// Card size relative to the default; omitted at the default size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowViewport {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FlowLayout {
    pub nodes: Vec<FlowPosition>,
    pub viewport: Option<FlowViewport>,
    #[serde(default, deserialize_with = "connections")]
    pub connections: HashMap<String, FlowConnection>,
    #[serde(default)]
    pub annotations: Vec<FlowAnnotation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowAnnotation {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    /// Content size relative to the default; omitted at the default size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    #[serde(flatten)]
    pub content: AnnotationContent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnnotationContent {
    Text { text: String },
    Note { note_id: i64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowConnection {
    pub source_handle: String,
    pub target_handle: String,
}

/// Rewrites legacy `kind:source:target` handle keys to `low:high`, swapping handles as needed.
fn connections<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<HashMap<String, FlowConnection>, D::Error> {
    let stored = HashMap::<String, FlowConnection>::deserialize(deserializer)?;
    let mut result = HashMap::new();
    for (key, mut connection) in stored {
        let parts: Vec<_> = key.split(':').collect();
        let pair = match parts.as_slice() {
            ["related_to" | "depends_on", a, b] => a.parse::<i64>().ok().zip(b.parse().ok()),
            _ => None,
        };
        let key = match pair {
            Some((a, b)) => {
                if a > b {
                    std::mem::swap(&mut connection.source_handle, &mut connection.target_handle);
                }
                format!("{}:{}", a.min(b), a.max(b))
            }
            None => key,
        };
        result.entry(key).or_insert(connection);
    }
    Ok(result)
}

/// Boards without stored tabs expose this implicit tab, which holds legacy layouts.
pub const DEFAULT_TAB_ID: &str = "default";
const MAX_TABS: usize = 50;
const MAX_TAB_NAME_CHARS: usize = 60;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowTab {
    pub id: String,
    // Empty names display the localized default canvas name.
    pub name: String,
    // Present only in backups; commands return tab metadata without layouts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<FlowLayout>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FlowData {
    // shortcut: Device-local data, extend sharing snapshots/CRDTs before syncing Flow.
    #[serde(default)]
    pub tab_id: String,
    pub layout: FlowLayout,
    pub relationships: Vec<Relationship>,
    #[serde(default)]
    pub tabs: Vec<FlowTab>,
}

type FlowResult<T> = Result<T, Box<dyn std::error::Error>>;

pub fn initialize(tx: &Transaction<'_>) -> FlowResult<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS cardbe_flow_layouts(
            board_id INTEGER NOT NULL REFERENCES cardbe_boards(id) ON DELETE CASCADE,
            layout_id TEXT NOT NULL, payload TEXT NOT NULL,
            name TEXT NOT NULL DEFAULT '', position INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY(board_id,layout_id));
         CREATE TABLE IF NOT EXISTS cardbe_task_links(
            board_id INTEGER NOT NULL REFERENCES cardbe_boards(id) ON DELETE CASCADE,
            source INTEGER NOT NULL, target INTEGER NOT NULL,
            source_arrow INTEGER NOT NULL DEFAULT 0, target_arrow INTEGER NOT NULL DEFAULT 0,
            CHECK(source < target), PRIMARY KEY(board_id,source,target));",
    )?;
    let legacy: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='cardbe_task_relationships')",
        [],
        |row| row.get(0),
    )?;
    if legacy {
        // A dependency pointed from its source to its target; merge both kinds per pair.
        tx.execute_batch(
            "INSERT INTO cardbe_task_links(board_id,source,target,source_arrow,target_arrow)
             SELECT board_id, MIN(source,target), MAX(source,target),
                kind='depends_on' AND target<source, kind='depends_on' AND target>source
             FROM cardbe_task_relationships WHERE true
             ON CONFLICT(board_id,source,target) DO UPDATE SET
                source_arrow=source_arrow OR excluded.source_arrow,
                target_arrow=target_arrow OR excluded.target_arrow;
             DROP TABLE cardbe_task_relationships;",
        )?;
    }
    for sql in [
        "ALTER TABLE cardbe_flow_layouts ADD COLUMN name TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE cardbe_flow_layouts ADD COLUMN position INTEGER NOT NULL DEFAULT 0",
    ] {
        let _ = tx.execute(sql, []);
    }
    Ok(())
}

pub fn tabs(connection: &Connection, board_id: i64) -> FlowResult<Vec<FlowTab>> {
    let mut statement = connection.prepare(
        "SELECT layout_id,name FROM cardbe_flow_layouts WHERE board_id=?1 ORDER BY position,layout_id",
    )?;
    let mut tabs = statement
        .query_map([board_id], |row| {
            Ok(FlowTab {
                id: row.get(0)?,
                name: row.get(1)?,
                layout: None,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if tabs.is_empty() {
        tabs.push(FlowTab {
            id: DEFAULT_TAB_ID.into(),
            name: String::new(),
            layout: None,
        });
    }
    Ok(tabs)
}

fn read_layout(connection: &Connection, board_id: i64, tab_id: &str) -> FlowResult<FlowLayout> {
    let payload: Option<String> = connection
        .query_row(
            "SELECT payload FROM cardbe_flow_layouts WHERE board_id=?1 AND layout_id=?2",
            params![board_id, tab_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(payload
        .map(|p| serde_json::from_str(&p))
        .transpose()?
        .unwrap_or_default())
}

fn layouts(connection: &Connection, board_id: i64) -> FlowResult<Vec<(String, FlowLayout)>> {
    let mut statement = connection
        .prepare("SELECT layout_id,payload FROM cardbe_flow_layouts WHERE board_id=?1")?;
    let rows = statement
        .query_map([board_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(id, payload)| Ok((id, serde_json::from_str(&payload)?)))
        .collect()
}

/// Reads one tab, falling back to the first tab when the requested tab no longer exists.
pub fn read(connection: &Connection, board_id: i64, tab_id: Option<&str>) -> FlowResult<FlowData> {
    let tabs = tabs(connection, board_id)?;
    let tab_id = tab_id
        .and_then(|id| tabs.iter().find(|tab| tab.id == id))
        .unwrap_or(&tabs[0])
        .id
        .clone();
    Ok(FlowData {
        layout: read_layout(connection, board_id, &tab_id)?,
        relationships: relationships(connection, board_id)?,
        tab_id,
        tabs,
    })
}

/// Reads every tab layout for backups; `layout` repeats the first tab for older readers.
pub fn read_backup(connection: &Connection, board_id: i64) -> FlowResult<FlowData> {
    let mut data = read(connection, board_id, None)?;
    for tab in &mut data.tabs {
        tab.layout = Some(read_layout(connection, board_id, &tab.id)?);
    }
    Ok(data)
}

fn relationships(connection: &Connection, board_id: i64) -> FlowResult<Vec<Relationship>> {
    let mut statement = connection.prepare(
        "SELECT source,target,source_arrow,target_arrow FROM cardbe_task_links WHERE board_id=?1 ORDER BY source,target",
    )?;
    let relationships = statement
        .query_map([board_id], |row| {
            Ok(Relationship {
                source: row.get(0)?,
                target: row.get(1)?,
                source_arrow: row.get(2)?,
                target_arrow: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(relationships)
}

fn valid_tab_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
}

pub fn tab_name(name: &str) -> Result<String, DomainError> {
    let name = name.trim();
    if name.chars().count() > MAX_TAB_NAME_CHARS || name.chars().any(char::is_control) {
        return Err(DomainError::InvalidArgument);
    }
    Ok(name.into())
}

pub fn validate_tabs(tabs: &[FlowTab]) -> Result<(), DomainError> {
    let mut ids = HashSet::new();
    if tabs.len() > MAX_TABS
        || tabs.iter().any(|tab| {
            !valid_tab_id(&tab.id) || !ids.insert(tab.id.as_str()) || tab_name(&tab.name).is_err()
        })
    {
        return Err(DomainError::InvalidArgument);
    }
    Ok(())
}

// Stores the implicit legacy tab before tab metadata changes.
fn materialize_tabs(tx: &Transaction<'_>, board_id: i64) -> FlowResult<Vec<FlowTab>> {
    let tabs = tabs(tx, board_id)?;
    tx.execute(
        "INSERT INTO cardbe_flow_layouts(board_id,layout_id,payload) VALUES(?1,?2,?3) ON CONFLICT(board_id,layout_id) DO NOTHING",
        params![board_id, tabs[0].id, serde_json::to_string(&FlowLayout::default())?],
    )?;
    Ok(tabs)
}

pub fn create_tab(tx: &Transaction<'_>, board_id: i64, id: &str, name: &str) -> FlowResult<()> {
    let name = tab_name(name)?;
    let tabs = materialize_tabs(tx, board_id)?;
    if !valid_tab_id(id) || tabs.len() >= MAX_TABS || tabs.iter().any(|tab| tab.id == id) {
        return Err(DomainError::InvalidArgument.into());
    }
    tx.execute(
        "INSERT INTO cardbe_flow_layouts(board_id,layout_id,payload,name,position)
         VALUES(?1,?2,?3,?4,(SELECT COALESCE(MAX(position),0)+1 FROM cardbe_flow_layouts WHERE board_id=?1))",
        params![board_id, id, serde_json::to_string(&FlowLayout::default())?, name],
    )?;
    Ok(())
}

pub fn rename_tab(tx: &Transaction<'_>, board_id: i64, id: &str, name: &str) -> FlowResult<()> {
    let name = tab_name(name)?;
    if !materialize_tabs(tx, board_id)?
        .iter()
        .any(|tab| tab.id == id)
    {
        return Err(DomainError::FlowTabNotFound.into());
    }
    tx.execute(
        "UPDATE cardbe_flow_layouts SET name=?3 WHERE board_id=?1 AND layout_id=?2",
        params![board_id, id, name],
    )?;
    Ok(())
}

pub fn delete_tab(tx: &Transaction<'_>, board_id: i64, id: &str) -> FlowResult<()> {
    let tabs = tabs(tx, board_id)?;
    if !tabs.iter().any(|tab| tab.id == id) {
        return Err(DomainError::FlowTabNotFound.into());
    }
    if tabs.len() == 1 {
        return Err(DomainError::LastFlowTabRequired.into());
    }
    tx.execute(
        "DELETE FROM cardbe_flow_layouts WHERE board_id=?1 AND layout_id=?2",
        params![board_id, id],
    )?;
    Ok(())
}

pub fn reorder_tabs(tx: &Transaction<'_>, board_id: i64, ids: &[String]) -> FlowResult<()> {
    let current = materialize_tabs(tx, board_id)?;
    let requested: HashSet<&str> = ids.iter().map(String::as_str).collect();
    if ids.len() != current.len()
        || requested.len() != ids.len()
        || current
            .iter()
            .any(|tab| !requested.contains(tab.id.as_str()))
    {
        return Err(DomainError::InvalidArgument.into());
    }
    for (position, id) in ids.iter().enumerate() {
        tx.execute(
            "UPDATE cardbe_flow_layouts SET position=?3 WHERE board_id=?1 AND layout_id=?2",
            params![board_id, id, position as i64],
        )?;
    }
    Ok(())
}

fn invalid_scale(scale: Option<f64>) -> bool {
    scale.is_some_and(|scale| !scale.is_finite() || !(0.5..=2.0).contains(&scale))
}

pub fn validate_layout(layout: &FlowLayout, task_ids: &HashSet<i64>) -> Result<(), DomainError> {
    let mut seen = HashSet::new();
    let mut annotation_ids = HashSet::new();
    if layout.nodes.len() > 10_000
        || layout.annotations.len() > 1_000
        || layout.annotations.iter().any(|annotation| {
            annotation.id.is_empty()
                || annotation.id.len() > 64
                || !annotation
                    .id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
                || !annotation_ids.insert(&annotation.id)
                || !annotation.x.is_finite()
                || !annotation.y.is_finite()
                || annotation.x.abs() > 1e7
                || annotation.y.abs() > 1e7
                || !annotation.width.is_finite()
                || !(160.0..=1200.0).contains(&annotation.width)
                || invalid_scale(annotation.scale)
                || match &annotation.content {
                    AnnotationContent::Text { text } => text.len() > 16_000,
                    AnnotationContent::Note { note_id } => *note_id < 0,
                }
        })
        || layout.connections.len() > 100_000
        || layout.connections.iter().any(|(id, connection)| {
            id.len() > 64
                || !id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b':')
                || !["left", "right", "top", "bottom"].contains(&connection.source_handle.as_str())
                || !["left", "right", "top", "bottom"].contains(&connection.target_handle.as_str())
        })
        || layout.nodes.iter().any(|node| {
            !node.x.is_finite()
                || !node.y.is_finite()
                || node.x.abs() > 1e7
                || node.y.abs() > 1e7
                || invalid_scale(node.scale)
                || !seen.insert(node.task_id)
        })
        || layout.viewport.as_ref().is_some_and(|v| {
            !v.x.is_finite()
                || !v.y.is_finite()
                || v.x.abs() > 1e7
                || v.y.abs() > 1e7
                || !v.zoom.is_finite()
                || !(0.1..=4.0).contains(&v.zoom)
        })
    {
        return Err(DomainError::InvalidArgument);
    }
    if layout
        .nodes
        .iter()
        .any(|node| !task_ids.contains(&node.task_id))
    {
        return Err(DomainError::TaskNotFound);
    }
    Ok(())
}

pub fn save_layout(
    tx: &Transaction<'_>,
    board_id: i64,
    tab_id: &str,
    layout: &FlowLayout,
) -> FlowResult<()> {
    tx.execute("INSERT INTO cardbe_flow_layouts(board_id,layout_id,payload) VALUES(?1,?2,?3) ON CONFLICT(board_id,layout_id) DO UPDATE SET payload=excluded.payload", params![board_id, tab_id, serde_json::to_string(layout)?])?;
    Ok(())
}

/// Stores each pair once in ascending task order; arrows move with their tasks.
pub fn normalize(mut relationship: Relationship) -> Relationship {
    if relationship.source > relationship.target {
        std::mem::swap(&mut relationship.source, &mut relationship.target);
        std::mem::swap(
            &mut relationship.source_arrow,
            &mut relationship.target_arrow,
        );
    }
    relationship
}

fn same_pair(a: &Relationship, b: &Relationship) -> bool {
    a.source == b.source && a.target == b.target
}

/// Expects normalized relationships.
pub fn validate_relationship(
    existing: &[Relationship],
    relationship: &Relationship,
    task_ids: &HashSet<i64>,
) -> Result<(), DomainError> {
    if relationship.source == relationship.target {
        return Err(DomainError::RelationshipSelf);
    }
    if !task_ids.contains(&relationship.source) || !task_ids.contains(&relationship.target) {
        return Err(DomainError::TaskNotFound);
    }
    if existing.iter().any(|edge| same_pair(edge, relationship)) {
        return Err(DomainError::RelationshipDuplicate);
    }
    Ok(())
}

pub fn insert_relationship(
    tx: &Transaction<'_>,
    board_id: i64,
    relationship: &Relationship,
) -> FlowResult<()> {
    tx.execute(
        "INSERT INTO cardbe_task_links(board_id,source,target,source_arrow,target_arrow) VALUES(?1,?2,?3,?4,?5)",
        params![
            board_id,
            relationship.source,
            relationship.target,
            relationship.source_arrow,
            relationship.target_arrow
        ],
    )?;
    Ok(())
}

pub fn delete_relationship(
    tx: &Transaction<'_>,
    board_id: i64,
    relationship: &Relationship,
) -> FlowResult<()> {
    tx.execute(
        "DELETE FROM cardbe_task_links WHERE board_id=?1 AND source=?2 AND target=?3",
        params![board_id, relationship.source, relationship.target],
    )?;
    Ok(())
}

/// Changes the arrows of an existing line.
pub fn update_relationship(
    tx: &Transaction<'_>,
    board_id: i64,
    relationship: &Relationship,
) -> FlowResult<()> {
    let changed = tx.execute(
        "UPDATE cardbe_task_links SET source_arrow=?4, target_arrow=?5 WHERE board_id=?1 AND source=?2 AND target=?3",
        params![
            board_id,
            relationship.source,
            relationship.target,
            relationship.source_arrow,
            relationship.target_arrow
        ],
    )?;
    if changed == 0 {
        return Err(DomainError::RelationshipNotFound.into());
    }
    Ok(())
}

pub fn task_ids(connection: &Connection, board_id: i64) -> FlowResult<HashSet<i64>> {
    let mut statement = connection.prepare("SELECT id FROM cardbe_tasks WHERE board_id=?1 UNION SELECT task_id FROM cardbe_archives WHERE board_id=?1")?;
    let ids = statement
        .query_map([board_id], |row| row.get::<_, i64>(0))?
        .collect::<Result<HashSet<_>, _>>()?;
    Ok(ids)
}

// Legacy backups contain only `layout`; tab backups may omit the selected tab's layout.
fn backup_tabs(data: &FlowData) -> Vec<(String, String, FlowLayout)> {
    if data.tabs.is_empty() {
        let id = if data.tab_id.is_empty() {
            DEFAULT_TAB_ID
        } else {
            data.tab_id.as_str()
        };
        return vec![(id.into(), String::new(), data.layout.clone())];
    }
    data.tabs
        .iter()
        .map(|tab| {
            let layout = tab.layout.clone().unwrap_or_else(|| {
                if tab.id == data.tab_id {
                    data.layout.clone()
                } else {
                    FlowLayout::default()
                }
            });
            (tab.id.clone(), tab.name.clone(), layout)
        })
        .collect()
}

fn validate_backup(data: &FlowData, ids: &HashSet<i64>) -> Result<(), DomainError> {
    validate_tabs(&data.tabs)?;
    if !data.tab_id.is_empty() && !valid_tab_id(&data.tab_id) {
        return Err(DomainError::InvalidArgument);
    }
    validate_layout(&data.layout, ids)?;
    for tab in &data.tabs {
        if let Some(layout) = &tab.layout {
            validate_layout(layout, ids)?;
        }
    }
    Ok(())
}

pub fn validate_data(
    data: &FlowData,
    tasks: &crate::models::ExportData,
) -> Result<(), DomainError> {
    let ids = tasks
        .columns
        .iter()
        .flat_map(|column| column.tasks.iter().map(|task| task.id))
        .chain(tasks.archives.iter().map(|archive| archive.task.id))
        .collect();
    validate_backup(data, &ids)?;
    let mut validated = Vec::new();
    if data.relationships.len() > 100_000 {
        return Err(DomainError::InvalidArgument);
    }
    for edge in &data.relationships {
        let edge = normalize(edge.clone());
        validate_relationship(&validated, &edge, &ids)?;
        validated.push(edge);
    }
    Ok(())
}

pub fn restore(tx: &Transaction<'_>, board_id: i64, data: &FlowData) -> FlowResult<()> {
    let ids = task_ids(tx, board_id)?;
    validate_backup(data, &ids)?;
    for (position, (id, name, layout)) in backup_tabs(data).into_iter().enumerate() {
        tx.execute(
            "INSERT INTO cardbe_flow_layouts(board_id,layout_id,payload,name,position) VALUES(?1,?2,?3,?4,?5)",
            params![board_id, id, serde_json::to_string(&layout)?, name, position as i64],
        )?;
    }
    let mut validated = Vec::new();
    for edge in &data.relationships {
        let edge = normalize(edge.clone());
        validate_relationship(&validated, &edge, &ids)?;
        insert_relationship(tx, board_id, &edge)?;
        validated.push(edge);
    }
    Ok(())
}

// Prune after the complete task/archive transaction, including received snapshots.
pub fn prune(tx: &Transaction<'_>, board_id: i64) -> FlowResult<()> {
    let relationships = relationships(tx, board_id)?;
    let layouts = layouts(tx, board_id)?;
    if relationships.is_empty()
        && layouts
            .iter()
            .all(|(_, layout)| layout.nodes.is_empty() && layout.connections.is_empty())
    {
        return Ok(());
    }
    let ids = task_ids(tx, board_id)?;
    let mut surviving = HashSet::new();
    for edge in relationships {
        if !ids.contains(&edge.source) || !ids.contains(&edge.target) {
            delete_relationship(tx, board_id, &edge)?;
        } else {
            surviving.insert(connection_key(&edge));
        }
    }
    for (tab_id, mut layout) in layouts {
        let old_len = layout.nodes.len();
        let old_connections = layout.connections.len();
        layout.connections.retain(|key, _| surviving.contains(key));
        layout.nodes.retain(|node| ids.contains(&node.task_id));
        if old_len != layout.nodes.len() || old_connections != layout.connections.len() {
            save_layout(tx, board_id, &tab_id, &layout)?;
        }
    }
    Ok(())
}

// Removing a relationship drops its handle overrides from every tab.
pub fn remove_connection(
    tx: &Transaction<'_>,
    board_id: i64,
    relationship: &Relationship,
) -> FlowResult<()> {
    let key = connection_key(relationship);
    for (tab_id, mut layout) in layouts(tx, board_id)? {
        if layout.connections.remove(&key).is_some() {
            save_layout(tx, board_id, &tab_id, &layout)?;
        }
    }
    Ok(())
}

fn connection_key(relationship: &Relationship) -> String {
    format!("{}:{}", relationship.source, relationship.target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Archive, Column, StoredData, Task};

    #[test]
    fn annotations_validate_bounds_identity_and_legacy_layouts() {
        let ids = HashSet::new();
        let mut layout: FlowLayout =
            serde_json::from_str(r#"{"nodes":[],"viewport":null}"#).unwrap();
        assert!(layout.annotations.is_empty());
        let annotation = FlowAnnotation {
            id: "text-1".into(),
            x: -42.0,
            y: 18.0,
            width: 280.0,
            scale: None,
            content: AnnotationContent::Text {
                text: "說明 🐈\n第二行".into(),
            },
        };
        layout.annotations.push(annotation.clone());
        assert!(validate_layout(&layout, &ids).is_ok());
        assert_eq!(
            serde_json::from_str::<FlowLayout>(&serde_json::to_string(&layout).unwrap()).unwrap(),
            layout
        );
        layout.annotations.push(annotation.clone());
        assert!(matches!(
            validate_layout(&layout, &ids),
            Err(DomainError::InvalidArgument)
        ));
        layout.annotations.pop();
        for width in [f64::NAN, f64::INFINITY, 159.0, 1201.0] {
            layout.annotations[0].width = width;
            assert!(matches!(
                validate_layout(&layout, &ids),
                Err(DomainError::InvalidArgument)
            ));
        }
        layout.annotations[0] = annotation.clone();
        layout.annotations[0].scale = Some(0.1);
        assert!(validate_layout(&layout, &ids).is_err());
        layout.annotations[0] = annotation.clone();
        layout.annotations[0].x = f64::NAN;
        assert!(validate_layout(&layout, &ids).is_err());
        layout.annotations[0] = annotation.clone();
        layout.annotations[0].id = "annotation_invalid".into();
        assert!(validate_layout(&layout, &ids).is_err());
        layout.annotations[0] = annotation.clone();
        layout.annotations[0].content = AnnotationContent::Text {
            text: "x".repeat(16_001),
        };
        assert!(validate_layout(&layout, &ids).is_err());
        layout.annotations[0].content = AnnotationContent::Note { note_id: -1 };
        assert!(validate_layout(&layout, &ids).is_err());
        // Deleted notes leave removable references rather than breaking the entire canvas.
        layout.annotations[0].content = AnnotationContent::Note { note_id: 42 };
        assert!(validate_layout(&layout, &ids).is_ok());
        layout.annotations = (0..1001)
            .map(|id| FlowAnnotation {
                id: id.to_string(),
                ..annotation.clone()
            })
            .collect();
        assert!(validate_layout(&layout, &ids).is_err());
    }

    const LINE: (bool, bool) = (false, false);
    const ARROW: (bool, bool) = (false, true);

    fn edge(source: i64, target: i64, (source_arrow, target_arrow): (bool, bool)) -> Relationship {
        Relationship {
            source,
            target,
            source_arrow,
            target_arrow,
        }
    }

    #[test]
    fn relationship_validation_handles_direction_duplicates_and_missing_tasks() {
        let ids = HashSet::from([1, 2, 3]);
        let existing = vec![edge(1, 2, ARROW), edge(2, 3, ARROW)];
        // Arrows carry no meaning, so cycles are allowed.
        assert!(validate_relationship(&existing, &edge(1, 3, ARROW), &ids).is_ok());
        assert!(matches!(
            validate_relationship(&existing, &edge(1, 1, LINE), &ids),
            Err(DomainError::RelationshipSelf)
        ));
        assert!(matches!(
            validate_relationship(&existing, &normalize(edge(2, 1, LINE)), &ids),
            Err(DomainError::RelationshipDuplicate)
        ));
        assert!(matches!(
            validate_relationship(&existing, &edge(1, 4, LINE), &ids),
            Err(DomainError::TaskNotFound)
        ));
        // Arrows follow their tasks when a pair is stored in ascending order.
        assert_eq!(normalize(edge(2, 1, ARROW)), edge(1, 2, (true, false)));
        let legacy: Vec<Relationship> = serde_json::from_str(
            r#"[{"source":2,"target":1,"kind":"depends_on"},{"source":1,"target":3,"kind":"related_to"}]"#,
        )
        .unwrap();
        assert_eq!(
            legacy.into_iter().map(normalize).collect::<Vec<_>>(),
            vec![edge(1, 2, (true, false)), edge(1, 3, LINE)]
        );
        let layout: FlowLayout = serde_json::from_str(
            r#"{"nodes":[],"viewport":null,"connections":{
                "depends_on:2:1":{"source_handle":"top","target_handle":"left"},
                "related_to:1:3":{"source_handle":"right","target_handle":"bottom"},
                "1:3":{"source_handle":"left","target_handle":"left"}}}"#,
        )
        .unwrap();
        assert_eq!(
            layout.connections["1:2"],
            FlowConnection {
                source_handle: "left".into(),
                target_handle: "top".into()
            }
        );
        assert_eq!(layout.connections.len(), 2);
    }

    #[test]
    fn layout_validation_rejects_duplicates_invalid_numbers_and_missing_tasks() {
        let ids = HashSet::from([1]);
        let mut layout = FlowLayout {
            annotations: Vec::new(),
            connections: HashMap::new(),
            nodes: vec![FlowPosition {
                task_id: 1,
                x: -42.0,
                y: 18.0,
                scale: None,
            }],
            viewport: Some(FlowViewport {
                x: 0.0,
                y: 0.0,
                zoom: 1.0,
            }),
        };
        assert!(validate_layout(&layout, &ids).is_ok());
        layout.connections.insert(
            "1:2".into(),
            FlowConnection {
                source_handle: "right".into(),
                target_handle: "bottom".into(),
            },
        );
        assert!(validate_layout(&layout, &ids).is_ok());
        layout.connections.get_mut("1:2").unwrap().target_handle = "invalid".into();
        assert!(matches!(
            validate_layout(&layout, &ids),
            Err(DomainError::InvalidArgument)
        ));
        layout.connections.clear();
        let legacy: FlowLayout = serde_json::from_str(r#"{"nodes":[],"viewport":null}"#).unwrap();
        assert!(legacy.connections.is_empty());
        layout.nodes.push(layout.nodes[0].clone());
        assert!(matches!(
            validate_layout(&layout, &ids),
            Err(DomainError::InvalidArgument)
        ));
        layout.nodes.pop();
        layout.nodes[0].x = f64::NAN;
        assert!(matches!(
            validate_layout(&layout, &ids),
            Err(DomainError::InvalidArgument)
        ));
        layout.nodes[0].x = 0.0;
        for scale in [f64::NAN, 0.4, 2.1] {
            layout.nodes[0].scale = Some(scale);
            assert!(validate_layout(&layout, &ids).is_err());
        }
        layout.nodes[0].scale = Some(2.0);
        assert!(validate_layout(&layout, &ids).is_ok());
        layout.nodes[0].scale = None;
        layout.viewport.as_mut().unwrap().zoom = 0.0;
        assert!(matches!(
            validate_layout(&layout, &ids),
            Err(DomainError::InvalidArgument)
        ));
        layout.viewport = None;
        layout.nodes[0].task_id = 99;
        assert!(matches!(
            validate_layout(&layout, &ids),
            Err(DomainError::TaskNotFound)
        ));
    }

    #[test]
    fn sqlite_flow_survives_restart_and_backup_without_changing_tasks_and_prunes_deletions() {
        let dir = std::env::temp_dir().join(format!(
            "cardbe-flow-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut loaded = crate::storage::load(&dir).unwrap();
        let db = &mut loaded.database;
        let mut tasks = StoredData::default();
        tasks.columns = vec![Column {
            id: 1,
            name: "Tasks".into(),
            color: String::new(),
            sort_order: Default::default(),
            tasks: (1..=3)
                .map(|id| Task {
                    id,
                    title: format!("Task {id}"),
                    ..Default::default()
                })
                .collect(),
        }];
        db.replace_board_as_local_edit(db.active_board_id(), &tasks)
            .unwrap();
        let board_id = db.active_board_id();
        let original = db.read_board_complete(board_id).unwrap();
        let note = db
            .create_note_with_content(1, "Original note".into(), "Content".into())
            .unwrap();
        let layout = FlowLayout {
            annotations: vec![
                FlowAnnotation {
                    id: "text-1".into(),
                    x: 10.0,
                    y: 20.0,
                    width: 240.0,
                    scale: Some(1.25),
                    content: AnnotationContent::Text {
                        text: "註記 🐈".into(),
                    },
                },
                FlowAnnotation {
                    id: "note-1".into(),
                    x: 20.0,
                    y: 40.0,
                    width: 280.0,
                    scale: None,
                    content: AnnotationContent::Note { note_id: note.id },
                },
            ],
            connections: HashMap::from([(
                "1:2".into(),
                FlowConnection {
                    source_handle: "right".into(),
                    target_handle: "right".into(),
                },
            )]),
            nodes: vec![
                FlowPosition {
                    task_id: 1,
                    x: 42.0,
                    y: -13.0,
                    scale: Some(0.75),
                },
                FlowPosition {
                    task_id: 2,
                    x: 240.0,
                    y: 18.0,
                    scale: None,
                },
            ],
            viewport: Some(FlowViewport {
                x: 17.0,
                y: 29.0,
                zoom: 1.5,
            }),
        };
        db.save_flow_layout(board_id, DEFAULT_TAB_ID, &layout)
            .unwrap();
        let relationship = edge(1, 2, ARROW);
        db.change_relationship(board_id, &relationship, false)
            .unwrap();
        assert!(db
            .change_relationship(board_id, &relationship, false)
            .is_err());
        assert!(db
            .change_relationship(board_id, &edge(2, 1, ARROW), false)
            .is_err());
        assert!(db
            .change_relationship(board_id, &edge(3, 3, ARROW), false)
            .is_err());
        // A reversed pair is the same line, whatever its arrows.
        assert!(db
            .change_relationship(board_id, &edge(2, 1, LINE), false)
            .is_err());
        assert_eq!(
            db.read_board_complete(board_id).unwrap().columns,
            original.columns
        );
        let saved = db.read_flow(board_id, None).unwrap();
        drop(loaded);
        let mut loaded = crate::storage::load(&dir).unwrap();
        let db = &mut loaded.database;
        assert_eq!(db.read_flow(board_id, None).unwrap(), saved);
        // Removing a canvas node preserves both domain task and relationship.
        db.save_flow_layout(board_id, DEFAULT_TAB_ID, &FlowLayout::default())
            .unwrap();
        assert_eq!(db.get_notes().unwrap(), vec![note.clone()]);
        assert_eq!(
            db.read_flow(board_id, None).unwrap().relationships,
            saved.relationships
        );
        assert_eq!(
            db.read_board_complete(board_id).unwrap().columns,
            original.columns
        );
        db.change_relationship(board_id, &relationship, true)
            .unwrap();
        assert!(db
            .read_flow(board_id, None)
            .unwrap()
            .relationships
            .is_empty());
        let other = db.create_board_with_data("Other", &tasks).unwrap();
        let empty = db.read_flow(other.id, None).unwrap();
        assert_eq!(empty.tab_id, DEFAULT_TAB_ID);
        assert_eq!(empty.layout, FlowLayout::default());
        assert!(empty.relationships.is_empty());
        assert_eq!(
            empty.tabs,
            vec![FlowTab {
                id: DEFAULT_TAB_ID.into(),
                name: String::new(),
                layout: None
            }]
        );
        let (boards, _) = db
            .replace_all_boards(
                &[("Restored".into(), original.clone(), saved.clone())],
                0,
                std::slice::from_ref(&note),
                &Default::default(),
            )
            .unwrap();
        let id = boards[0].id;
        assert_eq!(db.read_flow(id, None).unwrap(), saved);
        let mut invalid = saved.clone();
        invalid.relationships.push(edge(2, 1, ARROW));
        assert!(db
            .replace_all_boards(
                &[("Invalid".into(), original.clone(), invalid)],
                0,
                &[],
                &Default::default()
            )
            .is_err());
        assert_eq!(db.read_flow(id, None).unwrap(), saved);
        assert_eq!(
            db.read_board_complete(id).unwrap().columns,
            original.columns
        );
        db.load_board(id).unwrap();
        let mut archived = original.clone();
        let task = archived.columns[0].tasks.remove(0);
        archived.archives.push(Archive { time: 1, task });
        db.persist_diff(&original, &archived).unwrap();
        let archived_task = archived.archives[0].task.clone();
        assert_eq!(
            db.load_archive_tasks(id, &[archived_task.id, 999, archived_task.id])
                .unwrap(),
            vec![archived_task]
        );
        assert_eq!(db.read_flow(id, None).unwrap(), saved);
        let mut deleted = archived.clone();
        deleted.archives.clear();
        db.persist_diff(&archived, &deleted).unwrap();
        let pruned = db.read_flow(id, None).unwrap();
        assert_eq!(pruned.layout.nodes.len(), 1);
        assert_eq!(pruned.layout.nodes[0].task_id, 2);
        assert!(pruned.relationships.is_empty());
        assert!(pruned.layout.connections.is_empty());
        assert_eq!(pruned.layout.annotations, saved.layout.annotations);
        assert!(db.save_flow_layout(id, DEFAULT_TAB_ID, &layout).is_err());
        // Authoritative snapshots keep surviving positions but prune missing IDs.
        deleted.columns[0].tasks.clear();
        db.apply_iroh_snapshot(
            id,
            "Received",
            crate::models::IrohPermission::Viewer,
            1,
            &deleted,
            None,
        )
        .unwrap();
        assert!(db.read_flow(id, None).unwrap().layout.nodes.is_empty());
        assert_eq!(
            db.read_flow(id, None).unwrap().layout.annotations,
            saved.layout.annotations
        );
        drop(loaded);
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn domain_error(error: Box<dyn std::error::Error>) -> DomainError {
        *error.downcast::<DomainError>().unwrap()
    }

    fn position(task_id: i64) -> FlowPosition {
        FlowPosition {
            task_id,
            x: task_id as f64 * 100.0,
            y: 0.0,
            scale: None,
        }
    }

    #[test]
    fn tabs_keep_independent_layouts_share_relationships_and_survive_backups() {
        let dir = std::env::temp_dir().join(format!(
            "cardbe-flow-tabs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut loaded = crate::storage::load(&dir).unwrap();
        let db = &mut loaded.database;
        let mut tasks = StoredData::default();
        tasks.columns = vec![Column {
            id: 1,
            name: "Tasks".into(),
            color: String::new(),
            sort_order: Default::default(),
            tasks: (1..=3)
                .map(|id| Task {
                    id,
                    title: format!("Task {id}"),
                    ..Default::default()
                })
                .collect(),
        }];
        db.replace_board_as_local_edit(db.active_board_id(), &tasks)
            .unwrap();
        let board_id = db.active_board_id();
        let original = db.read_board_complete(board_id).unwrap();
        let connection = || FlowConnection {
            source_handle: "bottom".into(),
            target_handle: "top".into(),
        };
        db.save_flow_layout(
            board_id,
            DEFAULT_TAB_ID,
            &FlowLayout {
                nodes: vec![position(1), position(2)],
                connections: HashMap::from([("1:2".into(), connection())]),
                ..Default::default()
            },
        )
        .unwrap();
        let tabs = db
            .change_flow_tabs(board_id, |tx| {
                create_tab(tx, board_id, "plan-1", " 規劃 🐈 ")
            })
            .unwrap();
        assert_eq!(
            tabs.iter()
                .map(|tab| (tab.id.as_str(), tab.name.as_str()))
                .collect::<Vec<_>>(),
            vec![(DEFAULT_TAB_ID, ""), ("plan-1", "規劃 🐈")]
        );
        for (id, name) in [
            ("plan-1", "Duplicate"),
            ("bad_id", "Invalid ID"),
            ("long", &"長".repeat(61)),
            ("control", "a\nb"),
        ] {
            assert!(matches!(
                domain_error(
                    db.change_flow_tabs(board_id, |tx| create_tab(tx, board_id, id, name))
                        .unwrap_err()
                ),
                DomainError::InvalidArgument
            ));
        }
        let plan = FlowLayout {
            nodes: vec![position(2), position(3)],
            connections: HashMap::from([("1:2".into(), connection())]),
            ..Default::default()
        };
        db.save_flow_layout(board_id, "plan-1", &plan).unwrap();
        assert!(matches!(
            domain_error(db.save_flow_layout(board_id, "missing", &plan).unwrap_err()),
            DomainError::FlowTabNotFound
        ));
        let relationship = edge(1, 2, ARROW);
        db.change_relationship(board_id, &relationship, false)
            .unwrap();
        let read = db.read_flow(board_id, Some("plan-1")).unwrap();
        assert_eq!(read.tab_id, "plan-1");
        assert_eq!(read.layout, plan);
        assert_eq!(read.relationships, vec![relationship.clone()]);
        // Deleted or unknown tabs fall back to the first tab.
        let fallback = db.read_flow(board_id, Some("missing")).unwrap();
        assert_eq!(fallback.tab_id, DEFAULT_TAB_ID);
        assert_eq!(fallback.layout.nodes, vec![position(1), position(2)]);

        let tabs = db
            .change_flow_tabs(board_id, |tx| {
                reorder_tabs(tx, board_id, &["plan-1".into(), DEFAULT_TAB_ID.into()])?;
                rename_tab(tx, board_id, DEFAULT_TAB_ID, "總覽")
            })
            .unwrap();
        assert_eq!(
            tabs.iter()
                .map(|tab| (tab.id.as_str(), tab.name.as_str()))
                .collect::<Vec<_>>(),
            vec![("plan-1", "規劃 🐈"), (DEFAULT_TAB_ID, "總覽")]
        );
        assert_eq!(db.read_flow(board_id, None).unwrap().tab_id, "plan-1");
        for ids in [
            vec!["plan-1".to_string()],
            vec!["plan-1".into(), "plan-1".into()],
            vec!["plan-1".into(), "missing".into()],
        ] {
            assert!(db
                .change_flow_tabs(board_id, |tx| reorder_tabs(tx, board_id, &ids))
                .is_err());
        }
        assert!(matches!(
            domain_error(
                db.change_flow_tabs(board_id, |tx| rename_tab(tx, board_id, "missing", "x"))
                    .unwrap_err()
            ),
            DomainError::FlowTabNotFound
        ));

        // Backups include every tab, and legacy backups restore one default tab.
        let backup = db.read_flow_backup(board_id).unwrap();
        assert!(backup.tabs.iter().all(|tab| tab.layout.is_some()));
        let legacy: FlowData = serde_json::from_str(
            r#"{"layout":{"nodes":[{"task_id":1,"x":0,"y":0}],"viewport":null},"relationships":[]}"#,
        )
        .unwrap();
        let (boards, _) = db
            .replace_all_boards(
                &[
                    ("Tabs".into(), original.clone(), backup.clone()),
                    ("Legacy".into(), original.clone(), legacy),
                ],
                0,
                &[],
                &Default::default(),
            )
            .unwrap();
        assert_eq!(db.read_flow_backup(boards[0].id).unwrap(), backup);
        let legacy = db.read_flow(boards[1].id, None).unwrap();
        assert_eq!(legacy.tabs.len(), 1);
        assert_eq!(legacy.tab_id, DEFAULT_TAB_ID);
        assert_eq!(legacy.layout.nodes.len(), 1);
        let mut duplicate = backup.clone();
        duplicate.tabs.push(duplicate.tabs[0].clone());
        assert!(validate_data(
            &duplicate,
            &crate::models::ExportData {
                columns: original.columns.clone(),
                archives: vec![],
                templates: vec![],
                labels: vec![],
                label_recency: vec![],
            }
        )
        .is_err());

        // Relationship removal and task deletion prune every tab.
        let board_id = boards[0].id;
        db.load_board(board_id).unwrap();
        db.change_relationship(board_id, &relationship, true)
            .unwrap();
        let backup = db.read_flow_backup(board_id).unwrap();
        assert!(backup
            .tabs
            .iter()
            .all(|tab| tab.layout.as_ref().unwrap().connections.is_empty()));
        let mut deleted = original.clone();
        deleted.columns[0].tasks.retain(|task| task.id != 2);
        db.persist_diff(&original, &deleted).unwrap();
        for tab in db.read_flow_backup(board_id).unwrap().tabs {
            assert!(tab
                .layout
                .unwrap()
                .nodes
                .iter()
                .all(|node| node.task_id != 2));
        }

        let tabs = db
            .change_flow_tabs(board_id, |tx| delete_tab(tx, board_id, "plan-1"))
            .unwrap();
        assert_eq!(tabs.len(), 1);
        assert!(matches!(
            domain_error(
                db.change_flow_tabs(board_id, |tx| delete_tab(tx, board_id, DEFAULT_TAB_ID))
                    .unwrap_err()
            ),
            DomainError::LastFlowTabRequired
        ));
        assert!(matches!(
            domain_error(
                db.change_flow_tabs(board_id, |tx| delete_tab(tx, board_id, "plan-1"))
                    .unwrap_err()
            ),
            DomainError::FlowTabNotFound
        ));
        drop(loaded);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn arrows_change_without_moving_handles_and_legacy_tables_migrate() {
        let dir = std::env::temp_dir().join(format!(
            "cardbe-flow-arrows-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut loaded = crate::storage::load(&dir).unwrap();
        let db = &mut loaded.database;
        let mut tasks = StoredData::default();
        tasks.columns = vec![Column {
            id: 1,
            name: "Tasks".into(),
            color: String::new(),
            sort_order: Default::default(),
            tasks: (1..=3)
                .map(|id| Task {
                    id,
                    title: format!("Task {id}"),
                    ..Default::default()
                })
                .collect(),
        }];
        db.replace_board_as_local_edit(db.active_board_id(), &tasks)
            .unwrap();
        let board_id = db.active_board_id();
        let handles = FlowConnection {
            source_handle: "bottom".into(),
            target_handle: "top".into(),
        };
        db.change_flow_tabs(board_id, |tx| create_tab(tx, board_id, "plan", ""))
            .unwrap();
        for tab in [DEFAULT_TAB_ID, "plan"] {
            db.save_flow_layout(
                board_id,
                tab,
                &FlowLayout {
                    nodes: vec![position(1), position(2), position(3)],
                    connections: HashMap::from([("1:2".into(), handles.clone())]),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        db.change_relationship(board_id, &edge(1, 2, LINE), false)
            .unwrap();
        // Arrows are set per end, in either task order.
        db.update_relationship(board_id, &edge(2, 1, (true, true)))
            .unwrap();
        db.update_relationship(board_id, &edge(2, 1, ARROW))
            .unwrap();
        for tab in [DEFAULT_TAB_ID, "plan"] {
            let read = db.read_flow(board_id, Some(tab)).unwrap();
            assert_eq!(read.relationships, vec![edge(1, 2, (true, false))]);
            assert_eq!(
                read.layout.connections,
                HashMap::from([("1:2".into(), handles.clone())])
            );
        }
        assert!(matches!(
            domain_error(
                db.update_relationship(board_id, &edge(1, 3, ARROW))
                    .unwrap_err()
            ),
            DomainError::RelationshipNotFound
        ));
        // Databases created before arrows typed each line instead.
        let tx = db.connection.transaction().unwrap();
        tx.execute_batch(
            "CREATE TABLE cardbe_task_relationships(board_id INTEGER, source INTEGER, target INTEGER, kind TEXT);
             DELETE FROM cardbe_task_links;",
        )
        .unwrap();
        tx.execute(
            "INSERT INTO cardbe_task_relationships VALUES(?1,2,1,'depends_on'),(?1,1,2,'related_to'),(?1,1,3,'related_to'),(?1,2,3,'depends_on')",
            [board_id],
        )
        .unwrap();
        initialize(&tx).unwrap();
        tx.commit().unwrap();
        assert_eq!(
            db.read_flow(board_id, None).unwrap().relationships,
            vec![
                edge(1, 2, (true, false)),
                edge(1, 3, LINE),
                edge(2, 3, ARROW)
            ]
        );
        drop(loaded);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
