use crate::errors::DomainError;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKind {
    RelatedTo,
    DependsOn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relationship {
    pub source: i64,
    pub target: i64,
    pub kind: RelationshipKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowPosition {
    pub task_id: i64,
    pub x: f64,
    pub y: f64,
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
    #[serde(default)]
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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FlowData {
    // shortcut: Device-local data, extend sharing snapshots/CRDTs before syncing Flow.
    pub layout: FlowLayout,
    pub relationships: Vec<Relationship>,
}

type FlowResult<T> = Result<T, Box<dyn std::error::Error>>;

pub fn initialize(tx: &Transaction<'_>) -> FlowResult<()> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS cardbe_flow_layouts(
            board_id INTEGER NOT NULL REFERENCES cardbe_boards(id) ON DELETE CASCADE,
            layout_id TEXT NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(board_id,layout_id));
         CREATE TABLE IF NOT EXISTS cardbe_task_relationships(
            board_id INTEGER NOT NULL REFERENCES cardbe_boards(id) ON DELETE CASCADE,
            source INTEGER NOT NULL, target INTEGER NOT NULL,
            kind TEXT NOT NULL CHECK(kind IN ('related_to','depends_on')),
            CHECK(source != target), PRIMARY KEY(board_id,source,target,kind));",
    )?;
    Ok(())
}

pub fn read(connection: &Connection, board_id: i64) -> FlowResult<FlowData> {
    let payload: Option<String> = connection
        .query_row(
            "SELECT payload FROM cardbe_flow_layouts WHERE board_id=?1 AND layout_id='default'",
            [board_id],
            |row| row.get(0),
        )
        .optional()?;
    let mut statement = connection.prepare(
        "SELECT source,target,kind FROM cardbe_task_relationships WHERE board_id=?1 ORDER BY source,target,kind",
    )?;
    let relationships = statement
        .query_map([board_id], |row| {
            let kind: String = row.get(2)?;
            Ok(Relationship {
                source: row.get(0)?,
                target: row.get(1)?,
                kind: if kind == "depends_on" {
                    RelationshipKind::DependsOn
                } else {
                    RelationshipKind::RelatedTo
                },
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(FlowData {
        layout: payload
            .map(|p| serde_json::from_str(&p))
            .transpose()?
            .unwrap_or_default(),
        relationships,
    })
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

pub fn save_layout(tx: &Transaction<'_>, board_id: i64, layout: &FlowLayout) -> FlowResult<()> {
    tx.execute("INSERT INTO cardbe_flow_layouts(board_id,layout_id,payload) VALUES(?1,'default',?2) ON CONFLICT(board_id,layout_id) DO UPDATE SET payload=excluded.payload", params![board_id, serde_json::to_string(layout)?])?;
    Ok(())
}

pub fn normalize(mut relationship: Relationship) -> Relationship {
    if relationship.kind == RelationshipKind::RelatedTo && relationship.source > relationship.target
    {
        std::mem::swap(&mut relationship.source, &mut relationship.target);
    }
    relationship
}

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
    if existing.contains(relationship) {
        return Err(DomainError::RelationshipDuplicate);
    }
    if relationship.kind == RelationshipKind::DependsOn {
        let mut adjacency: HashMap<i64, Vec<i64>> = HashMap::new();
        for edge in existing
            .iter()
            .filter(|edge| edge.kind == RelationshipKind::DependsOn)
        {
            adjacency.entry(edge.source).or_default().push(edge.target);
        }
        let mut pending = vec![relationship.target];
        let mut visited = HashSet::new();
        while let Some(id) = pending.pop() {
            if id == relationship.source {
                return Err(DomainError::RelationshipCycle);
            }
            if visited.insert(id) {
                pending.extend(adjacency.get(&id).into_iter().flatten());
            }
        }
    }
    Ok(())
}

pub fn insert_relationship(
    tx: &Transaction<'_>,
    board_id: i64,
    relationship: &Relationship,
) -> FlowResult<()> {
    tx.execute(
        "INSERT INTO cardbe_task_relationships(board_id,source,target,kind) VALUES(?1,?2,?3,?4)",
        params![
            board_id,
            relationship.source,
            relationship.target,
            kind_name(relationship.kind)
        ],
    )?;
    Ok(())
}

pub fn delete_relationship(
    tx: &Transaction<'_>,
    board_id: i64,
    relationship: &Relationship,
) -> FlowResult<()> {
    tx.execute("DELETE FROM cardbe_task_relationships WHERE board_id=?1 AND source=?2 AND target=?3 AND kind=?4", params![board_id, relationship.source, relationship.target, kind_name(relationship.kind)])?;
    Ok(())
}

fn kind_name(kind: RelationshipKind) -> &'static str {
    match kind {
        RelationshipKind::RelatedTo => "related_to",
        RelationshipKind::DependsOn => "depends_on",
    }
}

pub fn task_ids(connection: &Connection, board_id: i64) -> FlowResult<HashSet<i64>> {
    let mut statement = connection.prepare("SELECT id FROM cardbe_tasks WHERE board_id=?1 UNION SELECT task_id FROM cardbe_archives WHERE board_id=?1")?;
    let ids = statement
        .query_map([board_id], |row| row.get::<_, i64>(0))?
        .collect::<Result<HashSet<_>, _>>()?;
    Ok(ids)
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
    validate_layout(&data.layout, &ids)?;
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
    validate_layout(&data.layout, &ids)?;
    save_layout(tx, board_id, &data.layout)?;
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
    let data = read(tx, board_id)?;
    if data.layout.nodes.is_empty()
        && data.relationships.is_empty()
        && data.layout.connections.is_empty()
    {
        return Ok(());
    }
    let ids = task_ids(tx, board_id)?;
    let mut surviving = HashSet::new();
    for edge in data.relationships {
        if !ids.contains(&edge.source) || !ids.contains(&edge.target) {
            delete_relationship(tx, board_id, &edge)?;
        } else {
            surviving.insert(format!(
                "{}:{}:{}",
                kind_name(edge.kind),
                edge.source,
                edge.target
            ));
        }
    }
    let mut layout = data.layout;
    let old_len = layout.nodes.len();
    let old_connections = layout.connections.len();
    layout.connections.retain(|key, _| surviving.contains(key));
    layout.nodes.retain(|node| ids.contains(&node.task_id));
    if old_len != layout.nodes.len() || old_connections != layout.connections.len() {
        save_layout(tx, board_id, &layout)?;
    }
    Ok(())
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

    fn edge(source: i64, target: i64, kind: RelationshipKind) -> Relationship {
        Relationship {
            source,
            target,
            kind,
        }
    }

    #[test]
    fn relationship_validation_handles_direction_duplicates_cycles_and_missing_tasks() {
        let ids = HashSet::from([1, 2, 3]);
        let depends = RelationshipKind::DependsOn;
        let related = RelationshipKind::RelatedTo;
        let existing = vec![edge(1, 2, depends), edge(2, 3, depends)];
        assert!(matches!(
            validate_relationship(&existing, &edge(3, 1, depends), &ids),
            Err(DomainError::RelationshipCycle)
        ));
        assert!(matches!(
            validate_relationship(&existing, &edge(1, 1, depends), &ids),
            Err(DomainError::RelationshipSelf)
        ));
        assert!(matches!(
            validate_relationship(&existing, &edge(1, 2, depends), &ids),
            Err(DomainError::RelationshipDuplicate)
        ));
        assert!(matches!(
            validate_relationship(&existing, &edge(1, 4, depends), &ids),
            Err(DomainError::TaskNotFound)
        ));
        assert!(validate_relationship(&existing, &edge(3, 1, related), &ids).is_ok());
        let related_edge = normalize(edge(2, 1, related));
        assert_eq!(related_edge, edge(1, 2, related));
        assert!(matches!(
            validate_relationship(&[related_edge], &normalize(edge(1, 2, related)), &ids),
            Err(DomainError::RelationshipDuplicate)
        ));
        assert!(validate_relationship(&[], &edge(2, 1, depends), &ids).is_ok());
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
            }],
            viewport: Some(FlowViewport {
                x: 0.0,
                y: 0.0,
                zoom: 1.0,
            }),
        };
        assert!(validate_layout(&layout, &ids).is_ok());
        layout.connections.insert(
            "related_to:1:2".into(),
            FlowConnection {
                source_handle: "right".into(),
                target_handle: "bottom".into(),
            },
        );
        assert!(validate_layout(&layout, &ids).is_ok());
        layout
            .connections
            .get_mut("related_to:1:2")
            .unwrap()
            .target_handle = "invalid".into();
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
                    content: AnnotationContent::Text {
                        text: "註記 🐈".into(),
                    },
                },
                FlowAnnotation {
                    id: "note-1".into(),
                    x: 20.0,
                    y: 40.0,
                    width: 280.0,
                    content: AnnotationContent::Note { note_id: note.id },
                },
            ],
            connections: HashMap::from([(
                "depends_on:1:2".into(),
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
                },
                FlowPosition {
                    task_id: 2,
                    x: 240.0,
                    y: 18.0,
                },
            ],
            viewport: Some(FlowViewport {
                x: 17.0,
                y: 29.0,
                zoom: 1.5,
            }),
        };
        db.save_flow_layout(board_id, &layout).unwrap();
        let relationship = edge(1, 2, RelationshipKind::DependsOn);
        db.change_relationship(board_id, &relationship, false)
            .unwrap();
        assert!(db
            .change_relationship(board_id, &relationship, false)
            .is_err());
        assert!(db
            .change_relationship(board_id, &edge(2, 1, RelationshipKind::DependsOn), false)
            .is_err());
        assert!(db
            .change_relationship(board_id, &edge(3, 3, RelationshipKind::DependsOn), false)
            .is_err());
        db.change_relationship(board_id, &edge(2, 1, RelationshipKind::RelatedTo), false)
            .unwrap();
        assert!(db
            .change_relationship(board_id, &edge(1, 2, RelationshipKind::RelatedTo), false)
            .is_err());
        db.change_relationship(board_id, &edge(1, 2, RelationshipKind::RelatedTo), true)
            .unwrap();
        assert_eq!(
            db.read_board_complete(board_id).unwrap().columns,
            original.columns
        );
        let saved = db.read_flow(board_id).unwrap();
        drop(loaded);
        let mut loaded = crate::storage::load(&dir).unwrap();
        let db = &mut loaded.database;
        assert_eq!(db.read_flow(board_id).unwrap(), saved);
        // Removing a canvas node preserves both domain task and relationship.
        db.save_flow_layout(board_id, &FlowLayout::default())
            .unwrap();
        assert_eq!(db.get_notes().unwrap(), vec![note.clone()]);
        assert_eq!(
            db.read_flow(board_id).unwrap().relationships,
            saved.relationships
        );
        assert_eq!(
            db.read_board_complete(board_id).unwrap().columns,
            original.columns
        );
        db.change_relationship(board_id, &relationship, true)
            .unwrap();
        assert!(db.read_flow(board_id).unwrap().relationships.is_empty());
        let other = db.create_board_with_data("Other", &tasks).unwrap();
        assert_eq!(db.read_flow(other.id).unwrap(), FlowData::default());
        let (boards, _) = db
            .replace_all_boards(
                &[("Restored".into(), original.clone(), saved.clone())],
                0,
                std::slice::from_ref(&note),
                &Default::default(),
            )
            .unwrap();
        let id = boards[0].id;
        assert_eq!(db.read_flow(id).unwrap(), saved);
        let mut invalid = saved.clone();
        invalid
            .relationships
            .push(edge(2, 1, RelationshipKind::DependsOn));
        assert!(db
            .replace_all_boards(
                &[("Invalid".into(), original.clone(), invalid)],
                0,
                &[],
                &Default::default()
            )
            .is_err());
        assert_eq!(db.read_flow(id).unwrap(), saved);
        assert_eq!(
            db.read_board_complete(id).unwrap().columns,
            original.columns
        );
        db.load_board(id).unwrap();
        let mut archived = original.clone();
        let task = archived.columns[0].tasks.remove(0);
        archived.archives.push(Archive { time: 1, task });
        db.persist_diff(&original, &archived).unwrap();
        assert_eq!(db.read_flow(id).unwrap(), saved);
        let mut deleted = archived.clone();
        deleted.archives.clear();
        db.persist_diff(&archived, &deleted).unwrap();
        let pruned = db.read_flow(id).unwrap();
        assert_eq!(pruned.layout.nodes.len(), 1);
        assert_eq!(pruned.layout.nodes[0].task_id, 2);
        assert!(pruned.relationships.is_empty());
        assert!(pruned.layout.connections.is_empty());
        assert_eq!(pruned.layout.annotations, saved.layout.annotations);
        assert!(db.save_flow_layout(id, &layout).is_err());
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
        assert!(db.read_flow(id).unwrap().layout.nodes.is_empty());
        assert_eq!(
            db.read_flow(id).unwrap().layout.annotations,
            saved.layout.annotations
        );
        drop(loaded);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
