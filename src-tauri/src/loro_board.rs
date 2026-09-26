//! Loro owns editable board state; SQLite is its queryable projection.
use crate::models::{Archive, Column, StoredData, Task, TaskTemplate};
use loro::{
    ExportMode, LoroDoc, LoroMap, LoroText, LoroTree, ToJson, TreeID, UpdateOptions, VersionVector,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Serialize, Deserialize)]
struct Positioned<T> {
    position: usize,
    value: T,
}

fn doc_from_update(update: Option<&[u8]>) -> Result<LoroDoc, String> {
    // A fresh peer ID on each load prevents two restarted replicas from issuing
    // operations under the same peer clock.
    let doc = LoroDoc::new();
    if let Some(bytes) = update.filter(|bytes| !bytes.is_empty()) {
        doc.import(bytes).map_err(|e| e.to_string())?;
    }
    Ok(doc)
}

pub fn state_vector(update: Option<&[u8]>) -> Result<Vec<u8>, String> {
    Ok(doc_from_update(update)?.oplog_vv().encode())
}

pub fn diff(update: Option<&[u8]>, remote_state_vector: &[u8]) -> Result<Vec<u8>, String> {
    let remote = VersionVector::decode(remote_state_vector).map_err(|e| e.to_string())?;
    doc_from_update(update)?
        .export(ExportMode::updates(&remote))
        .map_err(|e| e.to_string())
}

pub fn merge(update: Option<&[u8]>, incoming: &[u8]) -> Result<Vec<u8>, String> {
    let doc = doc_from_update(update)?;
    if !incoming.is_empty() {
        let status = doc.import(incoming).map_err(|e| e.to_string())?;
        if status.pending.is_some() {
            return Err("Loro update has missing dependencies".into());
        }
    }
    doc.export(ExportMode::Snapshot).map_err(|e| e.to_string())
}

/// Keep the invitation document compact while retaining sync lineage.
/// Loro may retain deleted values in its binary history; this is not erasure.
pub fn share_snapshot(update: &[u8]) -> Result<Vec<u8>, String> {
    let doc = doc_from_update(Some(update))?;
    doc.export(ExportMode::shallow_snapshot_owned(doc.oplog_frontiers()))
        .map_err(|e| e.to_string())
}

fn json(map: &LoroMap) -> serde_json::Value {
    map.get_deep_value().to_json_value()
}

fn meta(tree: &LoroTree, id: TreeID) -> Result<serde_json::Value, String> {
    Ok(json(&tree.get_meta(id).map_err(|e| e.to_string())?))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LoroNodeKind {
    Archives,
    Column,
    Task,
}

impl LoroNodeKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Archives => "archives",
            Self::Column => "column",
            Self::Task => "task",
        }
    }
}

fn node_kind(tree: &LoroTree, id: TreeID) -> Result<LoroNodeKind, String> {
    let metadata = meta(tree, id)?;
    let value = metadata
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or("Loro tree node has no kind")?;
    match value {
        "archives" => Ok(LoroNodeKind::Archives),
        "column" => Ok(LoroNodeKind::Column),
        "task" => Ok(LoroNodeKind::Task),
        _ => Err("Invalid Loro tree node kind".into()),
    }
}

fn node_id(tree: &LoroTree, id: TreeID) -> Result<i64, String> {
    meta(tree, id)?
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("Loro tree node has no ID")?
        .parse()
        .map_err(|_| "Invalid Loro tree node ID".into())
}

fn active_nodes(tree: &LoroTree, kind: LoroNodeKind) -> Result<HashMap<i64, TreeID>, String> {
    // ponytail: Scan the board per save; add a persisted ID index if large boards make saves slow.
    let mut result = HashMap::new();
    for node in tree.get_nodes(false) {
        if node_kind(tree, node.id)? == kind {
            if result.insert(node_id(tree, node.id)?, node.id).is_some() {
                return Err("Duplicate Loro tree node ID".into());
            }
        }
    }
    Ok(result)
}

fn create_node(
    tree: &LoroTree,
    parent: Option<TreeID>,
    kind: LoroNodeKind,
    id: i64,
) -> Result<TreeID, String> {
    let node = tree.create(parent).map_err(|e| e.to_string())?;
    let meta = tree.get_meta(node).map_err(|e| e.to_string())?;
    meta.insert("kind", kind.as_str()).map_err(|e| e.to_string())?;
    meta.insert("id", id.to_string())
        .map_err(|e| e.to_string())?;
    Ok(node)
}

fn sync_order(tree: &LoroTree, parent: Option<TreeID>, wanted: &[TreeID]) -> Result<(), String> {
    for (index, node) in wanted.iter().copied().enumerate() {
        let current = tree.children(parent).unwrap_or_default();
        if current.get(index) != Some(&node) {
            tree.mov_to(node, parent, index)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn set_field<T: Serialize>(map: &LoroMap, key: &str, value: &T) -> Result<(), String> {
    map.insert(
        key,
        serde_json::to_string(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn write_task(map: &LoroMap, task: &Task, archive_time: Option<u128>) -> Result<(), String> {
    set_field(map, "title", &task.title)?;
    let text: LoroText = map
        .ensure_mergeable_text("description")
        .map_err(|e| e.to_string())?;
    text.update(&task.description, UpdateOptions::default())
        .map_err(|e| e.to_string())?;
    set_field(map, "color", &task.color)?;
    set_field(map, "start_time", &task.start_time)?;
    set_field(map, "due_time", &task.due_time)?;
    set_field(map, "labels", &task.labels)?;
    set_field(map, "items", &task.items)?;
    set_field(map, "recurrence", &task.recurrence)?;
    set_field(map, "archive_time", &archive_time)
}

pub fn apply_local_delta(
    update: Option<&[u8]>,
    before: &StoredData,
    after: &StoredData,
) -> Result<Vec<u8>, String> {
    let doc = doc_from_update(update)?;
    let tree = doc.get_tree("cardbe.tree");
    tree.enable_fractional_index(0);
    let columns = doc.get_map("cardbe.columns");
    let tasks = doc.get_map("cardbe.tasks");
    let templates = doc.get_map("cardbe.templates");

    let archive_root = match tree
        .roots()
        .into_iter()
        .find(|id| node_kind(&tree, *id).ok() == Some(LoroNodeKind::Archives))
    {
        Some(id) => id,
        None => {
            let id = tree.create(None).map_err(|e| e.to_string())?;
            tree.get_meta(id)
                .map_err(|e| e.to_string())?
                .insert("kind", LoroNodeKind::Archives.as_str())
                .map_err(|e| e.to_string())?;
            id
        }
    };
    let mut column_nodes = active_nodes(&tree, LoroNodeKind::Column)?;
    let mut task_nodes = active_nodes(&tree, LoroNodeKind::Task)?;
    let wanted_columns = after.columns.iter().map(|c| c.id).collect::<HashSet<_>>();
    let wanted_tasks = after
        .columns
        .iter()
        .flat_map(|c| c.tasks.iter())
        .map(|t| t.id)
        .chain(after.archives.iter().map(|a| a.task.id))
        .collect::<HashSet<_>>();

    for id in before
        .columns
        .iter()
        .map(|c| c.id)
        .filter(|id| !wanted_columns.contains(id))
    {
        if let Some(node) = column_nodes.remove(&id) {
            tree.delete(node).map_err(|e| e.to_string())?;
        }
        columns.delete(&id.to_string()).map_err(|e| e.to_string())?;
    }
    for id in before
        .columns
        .iter()
        .flat_map(|c| c.tasks.iter())
        .map(|t| t.id)
        .chain(before.archives.iter().map(|a| a.task.id))
        .filter(|id| !wanted_tasks.contains(id))
    {
        if let Some(node) = task_nodes.remove(&id) {
            tree.delete(node).map_err(|e| e.to_string())?;
        }
        tasks.delete(&id.to_string()).map_err(|e| e.to_string())?;
    }

    let mut root_order = vec![archive_root];
    for column in &after.columns {
        let node = match column_nodes.get(&column.id) {
            Some(node) => *node,
            None => create_node(&tree, None, LoroNodeKind::Column, column.id)?,
        };
        root_order.push(node);
        let map = columns
            .ensure_mergeable_map(&column.id.to_string())
            .map_err(|e| e.to_string())?;
        set_field(&map, "name", &column.name)?;
        set_field(&map, "color", &column.color)?;
        set_field(&map, "sort_order", &column.sort_order)?;
        let mut order = Vec::new();
        for task in &column.tasks {
            let task_node = match task_nodes.get(&task.id) {
                Some(node) => *node,
                None => create_node(&tree, Some(node), LoroNodeKind::Task, task.id)?,
            };
            order.push(task_node);
            let map = tasks
                .ensure_mergeable_map(&task.id.to_string())
                .map_err(|e| e.to_string())?;
            write_task(&map, task, None)?;
        }
        sync_order(&tree, Some(node), &order)?;
    }
    sync_order(&tree, None, &root_order)?;
    let mut archive_order = Vec::new();
    for archive in &after.archives {
        let node = match task_nodes.get(&archive.task.id) {
            Some(node) => *node,
            None => create_node(&tree, Some(archive_root), LoroNodeKind::Task, archive.task.id)?,
        };
        archive_order.push(node);
        let map = tasks
            .ensure_mergeable_map(&archive.task.id.to_string())
            .map_err(|e| e.to_string())?;
        write_task(&map, &archive.task, Some(archive.time))?;
    }
    sync_order(&tree, Some(archive_root), &archive_order)?;

    let wanted_templates = after.templates.iter().map(|t| t.id).collect::<HashSet<_>>();
    for id in before
        .templates
        .iter()
        .map(|t| t.id)
        .filter(|id| !wanted_templates.contains(id))
    {
        templates
            .delete(&id.to_string())
            .map_err(|e| e.to_string())?;
    }
    for (position, template) in after.templates.iter().enumerate() {
        templates
            .insert(
                &template.id.to_string(),
                serde_json::to_string(&Positioned {
                    position,
                    value: template,
                })
                .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
    }
    doc.export(ExportMode::Snapshot).map_err(|e| e.to_string())
}

fn field<T: for<'a> Deserialize<'a>>(value: &serde_json::Value, key: &str) -> Result<T, String> {
    let text = value
        .get(key)
        .and_then(|value| value.as_str())
        .ok_or_else(|| format!("Missing Loro field: {key}"))?;
    serde_json::from_str(text).map_err(|e| e.to_string())
}

fn task_from_map(id: i64, map: &serde_json::Value) -> Result<Task, String> {
    Ok(Task {
        id,
        title: field(map, "title")?,
        description: map
            .get("description")
            .and_then(|v| v.as_str())
            .ok_or("Missing Loro description")?
            .to_string(),
        color: field(map, "color")?,
        start_time: field(map, "start_time")?,
        due_time: field(map, "due_time")?,
        labels: field(map, "labels")?,
        items: field(map, "items")?,
        recurrence: field(map, "recurrence")?,
    })
}

pub fn project(update: &[u8]) -> Result<StoredData, String> {
    let doc = doc_from_update(Some(update))?;
    let tree = doc.get_tree("cardbe.tree");
    let columns = json(&doc.get_map("cardbe.columns"));
    let tasks = json(&doc.get_map("cardbe.tasks"));
    let templates = json(&doc.get_map("cardbe.templates"));
    let mut result = StoredData::default();
    let mut seen = HashSet::new();
    for root in tree.roots() {
        match node_kind(&tree, root)? {
            LoroNodeKind::Archives => {
                for node in tree.children(root).unwrap_or_default() {
                    if node_kind(&tree, node)? != LoroNodeKind::Task {
                        return Err("Invalid Loro archive child".into());
                    }
                    let id = node_id(&tree, node)?;
                    if !seen.insert(id) {
                        return Err("Duplicate Loro task ID".into());
                    }
                    let map = tasks.get(id.to_string()).ok_or("Missing Loro task")?;
                    result.archives.push(Archive {
                        time: field(map, "archive_time")?,
                        task: task_from_map(id, map)?,
                    });
                }
            }
            LoroNodeKind::Column => {
                let id = node_id(&tree, root)?;
                let map = columns.get(id.to_string()).ok_or("Missing Loro column")?;
                let mut column = Column {
                    id,
                    name: field(map, "name")?,
                    color: field(map, "color")?,
                    sort_order: field(map, "sort_order")?,
                    tasks: Vec::new(),
                };
                for node in tree.children(root).unwrap_or_default() {
                    if node_kind(&tree, node)? != LoroNodeKind::Task {
                        return Err("Invalid Loro column child".into());
                    }
                    let id = node_id(&tree, node)?;
                    if !seen.insert(id) {
                        return Err("Duplicate Loro task ID".into());
                    }
                    let map = tasks.get(id.to_string()).ok_or("Missing Loro task")?;
                    column.tasks.push(task_from_map(id, map)?);
                }
                result.columns.push(column);
            }
            _ => return Err("Invalid Loro root node".into()),
        }
    }
    let mut ordered = std::collections::BTreeMap::new();
    for (id, value) in templates.as_object().into_iter().flat_map(|map| map.iter()) {
        let text = value.as_str().ok_or("Invalid Loro template")?;
        let entry: Positioned<TaskTemplate> =
            serde_json::from_str(text).map_err(|e| e.to_string())?;
        ordered.insert((entry.position, id.clone()), entry.value);
    }
    result.templates = ordered.into_values().collect();
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ColumnSort;
    #[test]
    fn concurrent_cards_merge_without_a_board_snapshot() {
        let base = apply_local_delta(None, &StoredData::default(), &StoredData::default()).unwrap();
        let mut a = StoredData::default();
        a.columns.push(Column {
            id: 1,
            name: "A".into(),
            color: String::new(),
            sort_order: ColumnSort::Custom,
            tasks: vec![Task {
                id: 11,
                title: "left".into(),
                ..Task::default()
            }],
        });
        let mut b = StoredData::default();
        b.columns.push(Column {
            id: 2,
            name: "B".into(),
            color: String::new(),
            sort_order: ColumnSort::Custom,
            tasks: vec![Task {
                id: 22,
                title: "right".into(),
                ..Task::default()
            }],
        });
        let left = apply_local_delta(Some(&base), &StoredData::default(), &a).unwrap();
        let right = apply_local_delta(Some(&base), &StoredData::default(), &b).unwrap();
        let merged = merge(Some(&left), &right).unwrap();
        let projected = project(&merged).unwrap();
        assert_eq!(
            projected
                .columns
                .iter()
                .flat_map(|c| c.tasks.iter())
                .count(),
            2
        );
    }

    #[test]
    fn two_editors_can_add_cards_to_the_same_column() {
        let mut base = StoredData::default();
        base.columns.push(Column {
            id: 1,
            name: "Todo".into(),
            color: String::new(),
            sort_order: ColumnSort::Custom,
            tasks: Vec::new(),
        });
        let initial = apply_local_delta(None, &StoredData::default(), &base).unwrap();
        let mut left = base.clone();
        left.columns[0].tasks.push(Task {
            id: 11,
            title: "B".into(),
            ..Task::default()
        });
        let mut right = base.clone();
        right.columns[0].tasks.push(Task {
            id: 22,
            title: "C".into(),
            ..Task::default()
        });
        let left = apply_local_delta(Some(&initial), &base, &left).unwrap();
        let right = apply_local_delta(Some(&initial), &base, &right).unwrap();
        let merged = project(&merge(Some(&left), &right).unwrap()).unwrap();
        let ids = merged.columns[0]
            .tasks
            .iter()
            .map(|task| task.id)
            .collect::<HashSet<_>>();
        assert_eq!(ids, HashSet::from([11, 22]));
    }

    #[test]
    fn concurrent_fields_on_one_card_merge() {
        let mut base = StoredData::default();
        base.columns.push(Column {
            id: 1,
            name: "Todo".into(),
            color: String::new(),
            sort_order: ColumnSort::Custom,
            tasks: vec![Task {
                id: 2,
                title: "Before".into(),
                ..Task::default()
            }],
        });
        let initial = apply_local_delta(None, &StoredData::default(), &base).unwrap();
        let mut title = base.clone();
        title.columns[0].tasks[0].title = "After".into();
        let mut color = base.clone();
        color.columns[0].tasks[0].color = "blue".into();
        let left = apply_local_delta(Some(&initial), &base, &title).unwrap();
        let right = apply_local_delta(Some(&initial), &base, &color).unwrap();
        let merged = merge(Some(&left), &right).unwrap();
        let card = &project(&merged).unwrap().columns[0].tasks[0];
        assert_eq!(card.title, "After");
        assert_eq!(card.color, "blue");
    }

    #[test]
    fn invited_shallow_document_merges_later_offline_edits() {
        let mut base = StoredData::default();
        base.columns.push(Column {
            id: 1,
            name: "Todo".into(),
            color: String::new(),
            sort_order: ColumnSort::Custom,
            tasks: vec![Task {
                id: 2,
                title: "Before".into(),
                ..Task::default()
            }],
        });
        let owner = apply_local_delta(None, &StoredData::default(), &base).unwrap();
        let invited = share_snapshot(&owner).unwrap();
        let mut owner_data = base.clone();
        owner_data.columns[0].tasks[0].title = "After".into();
        let owner = apply_local_delta(Some(&owner), &base, &owner_data).unwrap();
        let mut editor_data = base.clone();
        editor_data.columns[0].tasks[0].color = "blue".into();
        let editor = apply_local_delta(Some(&invited), &base, &editor_data).unwrap();
        let merged = merge(Some(&owner), &editor).unwrap();
        let card = &project(&merged).unwrap().columns[0].tasks[0];
        assert_eq!(card.title, "After");
        assert_eq!(card.color, "blue");
        let delta = diff(Some(&merged), &state_vector(Some(&editor)).unwrap()).unwrap();
        let editor = merge(Some(&editor), &delta).unwrap();
        let card = &project(&editor).unwrap().columns[0].tasks[0];
        assert_eq!(card.title, "After");
        assert_eq!(card.color, "blue");
    }

    #[test]
    fn tree_reorders_columns_and_moves_cards() {
        let mut before = StoredData::default();
        before.columns = vec![
            Column {
                id: 1,
                name: "A".into(),
                color: String::new(),
                sort_order: ColumnSort::Custom,
                tasks: vec![
                    Task {
                        id: 11,
                        ..Task::default()
                    },
                    Task {
                        id: 12,
                        ..Task::default()
                    },
                ],
            },
            Column {
                id: 2,
                name: "B".into(),
                color: String::new(),
                sort_order: ColumnSort::Custom,
                tasks: vec![Task {
                    id: 21,
                    ..Task::default()
                }],
            },
        ];
        let initial = apply_local_delta(None, &StoredData::default(), &before).unwrap();
        let mut after = before.clone();
        after.columns.swap(0, 1);
        let moved = after.columns[1].tasks.remove(0);
        after.columns[0].tasks.insert(0, moved);
        let update = apply_local_delta(Some(&initial), &before, &after).unwrap();
        let projected = project(&update).unwrap();
        assert_eq!(
            projected.columns.iter().map(|c| c.id).collect::<Vec<_>>(),
            vec![2, 1]
        );
        assert_eq!(
            projected.columns[0]
                .tasks
                .iter()
                .map(|t| t.id)
                .collect::<Vec<_>>(),
            vec![11, 21]
        );
        assert_eq!(
            projected.columns[1]
                .tasks
                .iter()
                .map(|t| t.id)
                .collect::<Vec<_>>(),
            vec![12]
        );
    }

    #[test]
    fn concurrent_nonoverlapping_description_edits_merge() {
        let mut base = StoredData::default();
        base.columns.push(Column {
            id: 1,
            name: "Todo".into(),
            color: String::new(),
            sort_order: ColumnSort::Custom,
            tasks: vec![Task {
                id: 2,
                description: "hello world".into(),
                ..Task::default()
            }],
        });
        let initial = apply_local_delta(None, &StoredData::default(), &base).unwrap();
        let mut first = base.clone();
        first.columns[0].tasks[0].description = "hello brave world".into();
        let mut second = base.clone();
        second.columns[0].tasks[0].description = "hello world!".into();
        let left = apply_local_delta(Some(&initial), &base, &first).unwrap();
        let right = apply_local_delta(Some(&initial), &base, &second).unwrap();
        let merged = merge(Some(&left), &right).unwrap();
        assert_eq!(
            project(&merged).unwrap().columns[0].tasks[0].description,
            "hello brave world!"
        );
    }

    #[test]
    fn archiving_while_another_peer_edits_keeps_one_card() {
        let mut base = StoredData::default();
        base.columns.push(Column {
            id: 1,
            name: "Todo".into(),
            color: String::new(),
            sort_order: ColumnSort::Custom,
            tasks: vec![Task {
                id: 2,
                title: "Before".into(),
                ..Task::default()
            }],
        });
        let initial = apply_local_delta(None, &StoredData::default(), &base).unwrap();
        let mut archived = base.clone();
        let task = archived.columns[0].tasks.remove(0);
        archived.archives.push(Archive { time: 1, task });
        let mut edited = base.clone();
        edited.columns[0].tasks[0].title = "After".into();
        let left = apply_local_delta(Some(&initial), &base, &archived).unwrap();
        let right = apply_local_delta(Some(&initial), &base, &edited).unwrap();
        let merged = project(&merge(Some(&left), &right).unwrap()).unwrap();
        assert!(merged.columns[0].tasks.is_empty());
        assert_eq!(merged.archives.len(), 1);
        assert_eq!(merged.archives[0].task.title, "After");
    }

    #[test]
    fn archives_and_templates_keep_their_order() {
        let mut board = StoredData::default();
        board.archives = vec![
            Archive {
                time: 1,
                task: Task {
                    id: 20,
                    ..Task::default()
                },
            },
            Archive {
                time: 2,
                task: Task {
                    id: 10,
                    ..Task::default()
                },
            },
        ];
        board.templates = vec![
            TaskTemplate {
                id: 20,
                name: "First".into(),
                task: Task::default(),
            },
            TaskTemplate {
                id: 10,
                name: "Second".into(),
                task: Task::default(),
            },
        ];
        let update = apply_local_delta(None, &StoredData::default(), &board).unwrap();
        let projected = project(&update).unwrap();
        assert_eq!(
            projected
                .archives
                .iter()
                .map(|a| a.task.id)
                .collect::<Vec<_>>(),
            vec![20, 10]
        );
        assert_eq!(
            projected.templates.iter().map(|t| t.id).collect::<Vec<_>>(),
            vec![20, 10]
        );
    }

    #[test]
    fn deleting_a_card_is_absent_from_shared_projection() {
        let mut before = StoredData::default();
        before.columns.push(Column {
            id: 1,
            name: "Todo".into(),
            color: String::new(),
            sort_order: ColumnSort::Custom,
            tasks: vec![Task {
                id: 2,
                title: "private-token-ccae8f7c".into(),
                ..Task::default()
            }],
        });
        let first = apply_local_delta(None, &StoredData::default(), &before).unwrap();
        let mut after = before.clone();
        after.columns[0].tasks.clear();
        let update = apply_local_delta(Some(&first), &before, &after).unwrap();
        assert!(project(&update).unwrap().columns[0].tasks.is_empty());
        let shared = share_snapshot(&update).unwrap();
        assert!(project(&shared).unwrap().columns[0].tasks.is_empty());
    }
}
