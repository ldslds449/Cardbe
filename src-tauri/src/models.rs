use serde::{Deserialize, Serialize};

pub const CURRENT_SCHEMA_VERSION: u32 = 8;

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Board {
    pub id: i64,
    pub name: String,
    pub task_count: i64,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub id: i64,
    pub title: String,
    pub content: String,
    pub pinned: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

fn default_id() -> i64 {
    -1
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct TaskItem {
    pub id: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub completed: bool,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RecurrenceFrequency {
    Daily,
    Weekly,
    Monthly,
}

fn default_recurrence_interval() -> u32 {
    1
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Recurrence {
    pub frequency: RecurrenceFrequency,
    #[serde(default = "default_recurrence_interval")]
    pub interval: u32,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Task {
    #[serde(default = "default_id")]
    pub id: i64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub start_time: u128,
    #[serde(default)]
    pub due_time: Option<u128>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub items: Vec<TaskItem>,
    #[serde(default)]
    pub recurrence: Option<Recurrence>,
}

impl Default for Task {
    fn default() -> Self {
        Self {
            id: -1,
            title: String::new(),
            description: String::new(),
            color: String::new(),
            start_time: 0,
            due_time: None,
            labels: Vec::new(),
            items: Vec::new(),
            recurrence: None,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct TaskTemplate {
    #[serde(default = "default_id")]
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub task: Task,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ColumnSort {
    Custom,
    DueDateAsc,
    DueDateDesc,
}

impl Default for ColumnSort {
    fn default() -> Self {
        Self::Custom
    }
}

impl ColumnSort {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Custom => "custom",
            Self::DueDateAsc => "due_date_asc",
            Self::DueDateDesc => "due_date_desc",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "due_date_asc" => Self::DueDateAsc,
            "due_date_desc" => Self::DueDateDesc,
            _ => Self::Custom,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Column {
    #[serde(default = "default_id")]
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub sort_order: ColumnSort,
    #[serde(default)]
    pub tasks: Vec<Task>,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Archive {
    #[serde(default)]
    pub time: u128,
    #[serde(default)]
    pub task: Task,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    #[serde(default)]
    pub notify_enabled: bool,
    #[serde(default = "default_true")]
    pub global_shortcuts_enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            notify_enabled: false,
            global_shortcuts_enabled: true,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct StoredData {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub columns: Vec<Column>,
    #[serde(default)]
    pub archives: Vec<Archive>,
    #[serde(default)]
    pub templates: Vec<TaskTemplate>,
    #[serde(default)]
    pub settings: Settings,
    /// Labels in most-recently-used order. Labels themselves are still derived
    /// from active tasks, so removing a label from every task removes it here.
    #[serde(default)]
    pub label_recency: Vec<String>,
    #[serde(default)]
    pub next_column_id: i64,
    #[serde(default)]
    pub next_task_id: i64,
    #[serde(default)]
    pub next_template_id: i64,
}

impl Default for StoredData {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            columns: Vec::new(),
            archives: Vec::new(),
            templates: Vec::new(),
            settings: Settings::default(),
            label_recency: Vec::new(),
            next_column_id: 0,
            next_task_id: 0,
            next_template_id: 0,
        }
    }
}

impl StoredData {
    pub fn sort_column_tasks(&mut self) {
        for column in &mut self.columns {
            let direction = match column.sort_order {
                ColumnSort::Custom => continue,
                ColumnSort::DueDateAsc => std::cmp::Ordering::Less,
                ColumnSort::DueDateDesc => std::cmp::Ordering::Greater,
            };
            column
                .tasks
                .sort_by(|left, right| match (left.due_time, right.due_time) {
                    (Some(left), Some(right)) => {
                        if direction == std::cmp::Ordering::Less {
                            left.cmp(&right)
                        } else {
                            right.cmp(&left)
                        }
                    }
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                });
        }
    }

    pub fn migrate(mut self) -> Result<(Self, bool), String> {
        if self.schema_version > CURRENT_SCHEMA_VERSION {
            return Err(format!(
                "Data schema version {} is newer than this application supports ({}).",
                self.schema_version, CURRENT_SCHEMA_VERSION
            ));
        }

        let version_changed = self.schema_version != CURRENT_SCHEMA_VERSION;
        let column_ids_changed = self.repair_column_ids();
        let task_ids_changed = self.repair_task_ids();
        let template_ids_changed = self.repair_template_ids();
        let label_recency_changed = self.normalize_label_recency();
        let changed = version_changed
            || column_ids_changed
            || task_ids_changed
            || template_ids_changed
            || label_recency_changed;
        self.schema_version = CURRENT_SCHEMA_VERSION;
        Ok((self, changed))
    }

    pub fn touch_label(&mut self, label: String) {
        self.label_recency.retain(|candidate| candidate != &label);
        self.label_recency.insert(0, label);
    }

    fn normalize_label_recency(&mut self) -> bool {
        let previous = self.label_recency.clone();
        let active_labels = self
            .columns
            .iter()
            .flat_map(|column| column.tasks.iter())
            .flat_map(|task| task.labels.iter())
            .collect::<std::collections::HashSet<_>>();
        let mut seen = std::collections::HashSet::new();
        self.label_recency
            .retain(|label| active_labels.contains(label) && seen.insert(label.clone()));
        self.label_recency != previous
    }

    pub fn repair_column_ids(&mut self) -> bool {
        use std::collections::HashSet;

        let existing_ids = self
            .columns
            .iter()
            .filter_map(|column| (column.id >= 0).then_some(column.id))
            .collect::<HashSet<_>>();
        let mut next_id = existing_ids
            .iter()
            .max()
            .map_or(0, |id| id.saturating_add(1))
            .max(self.next_column_id.max(0));
        let mut seen = HashSet::new();
        let mut changed = false;

        for column in &mut self.columns {
            if column.id >= 0 && seen.insert(column.id) {
                continue;
            }
            while existing_ids.contains(&next_id) || seen.contains(&next_id) {
                next_id = next_id.saturating_add(1);
            }
            column.id = next_id;
            seen.insert(next_id);
            next_id = next_id.saturating_add(1);
            changed = true;
        }

        if self.next_column_id != next_id {
            self.next_column_id = next_id;
            changed = true;
        }
        changed
    }

    pub fn repair_task_ids(&mut self) -> bool {
        use std::collections::HashSet;

        let existing_ids = self
            .columns
            .iter()
            .flat_map(|column| column.tasks.iter())
            .chain(self.archives.iter().map(|archive| &archive.task))
            .filter_map(|task| (task.id >= 0).then_some(task.id))
            .collect::<HashSet<_>>();
        let mut next_id = existing_ids
            .iter()
            .max()
            .map_or(0, |id| id.saturating_add(1))
            .max(self.next_task_id.max(0));
        let mut seen = HashSet::new();
        let mut changed = false;

        for task in self
            .columns
            .iter_mut()
            .flat_map(|column| column.tasks.iter_mut())
            .chain(self.archives.iter_mut().map(|archive| &mut archive.task))
        {
            if task.id >= 0 && seen.insert(task.id) {
                continue;
            }

            while existing_ids.contains(&next_id) || seen.contains(&next_id) {
                next_id = next_id.saturating_add(1);
            }
            task.id = next_id;
            seen.insert(next_id);
            next_id = next_id.saturating_add(1);
            changed = true;
        }

        if self.next_task_id != next_id {
            self.next_task_id = next_id;
            changed = true;
        }
        changed
    }

    pub fn allocate_task_id(&mut self) -> Result<i64, String> {
        let id = self.next_task_id;
        self.next_task_id = self
            .next_task_id
            .checked_add(1)
            .ok_or_else(|| "Task ID range exhausted".to_string())?;
        Ok(id)
    }

    pub fn allocate_column_id(&mut self) -> Result<i64, String> {
        let id = self.next_column_id;
        self.next_column_id = self
            .next_column_id
            .checked_add(1)
            .ok_or_else(|| "Column ID range exhausted".to_string())?;
        Ok(id)
    }

    pub fn repair_template_ids(&mut self) -> bool {
        use std::collections::HashSet;

        let existing_ids = self
            .templates
            .iter()
            .filter_map(|template| (template.id >= 0).then_some(template.id))
            .collect::<HashSet<_>>();
        let mut next_id = existing_ids
            .iter()
            .max()
            .map_or(0, |id| id.saturating_add(1))
            .max(self.next_template_id.max(0));
        let mut seen = HashSet::new();
        let mut changed = false;

        for template in &mut self.templates {
            if template.id >= 0 && seen.insert(template.id) {
                continue;
            }
            while existing_ids.contains(&next_id) || seen.contains(&next_id) {
                next_id = next_id.saturating_add(1);
            }
            template.id = next_id;
            seen.insert(next_id);
            next_id = next_id.saturating_add(1);
            changed = true;
        }

        if self.next_template_id != next_id {
            self.next_template_id = next_id;
            changed = true;
        }
        changed
    }

    pub fn allocate_template_id(&mut self) -> Result<i64, String> {
        let id = self.next_template_id;
        self.next_template_id = self
            .next_template_id
            .checked_add(1)
            .ok_or_else(|| "Template ID range exhausted".to_string())?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_settings_keep_global_shortcuts_enabled() {
        let settings: Settings = serde_json::from_str(r#"{"notify_enabled":true}"#).unwrap();

        assert!(settings.notify_enabled);
        assert!(settings.global_shortcuts_enabled);
    }

    #[test]
    fn old_document_defaults_settings_and_preserves_valid_ids() {
        let json = r#"{
            "columns": [{"name":"Todo","color":"","tasks":[
                {"id":-1,"title":"New"},
                {"id":5,"title":"Existing"}
            ]}],
            "archives": []
        }"#;
        let old: StoredData = serde_json::from_str(json).unwrap();
        let (migrated, changed) = old.migrate().unwrap();

        assert!(changed);
        assert!(!migrated.settings.notify_enabled);
        assert!(migrated.settings.global_shortcuts_enabled);
        assert_eq!(migrated.columns[0].id, 0);
        assert_eq!(migrated.columns[0].tasks[1].id, 5);
        assert!(migrated.columns[0].tasks[0].id > 5);
        assert_eq!(migrated.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(migrated.columns[0].tasks[0].items.is_empty());
        assert!(migrated.columns[0].tasks[0].recurrence.is_none());
    }

    #[test]
    fn due_date_sort_keeps_undated_tasks_last_in_both_directions() {
        let tasks = vec![
            Task {
                id: 1,
                title: "Undated".into(),
                ..Task::default()
            },
            Task {
                id: 2,
                title: "Later".into(),
                due_time: Some(20),
                ..Task::default()
            },
            Task {
                id: 3,
                title: "Earlier".into(),
                due_time: Some(10),
                ..Task::default()
            },
        ];
        let mut stored = StoredData::default();
        stored.columns.push(Column {
            id: 1,
            name: "Todo".into(),
            color: String::new(),
            sort_order: ColumnSort::DueDateAsc,
            tasks,
        });

        stored.sort_column_tasks();
        assert_eq!(
            stored.columns[0]
                .tasks
                .iter()
                .map(|task| task.id)
                .collect::<Vec<_>>(),
            vec![3, 2, 1]
        );

        stored.columns[0].sort_order = ColumnSort::DueDateDesc;
        stored.sort_column_tasks();
        assert_eq!(
            stored.columns[0]
                .tasks
                .iter()
                .map(|task| task.id)
                .collect::<Vec<_>>(),
            vec![2, 3, 1]
        );
    }
}

#[derive(Serialize, Deserialize)]
pub struct ExportData {
    pub columns: Vec<Column>,
    pub archives: Vec<Archive>,
    #[serde(default)]
    pub templates: Vec<TaskTemplate>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub label_recency: Vec<String>,
}

/// Complete, portable backup. Unlike `ExportData`, this contains every board
/// plus global data and is deliberately versioned for future migrations.
#[derive(Serialize, Deserialize)]
pub struct AllBoardsExport {
    pub schema_version: u32,
    pub boards: Vec<AllBoardsExportBoard>,
    pub active_board_index: usize,
    pub settings: Settings,
    pub notes: Vec<Note>,
}

#[derive(Serialize, Deserialize)]
pub struct AllBoardsExportBoard {
    pub name: String,
    pub data: ExportData,
}
