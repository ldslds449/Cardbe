//! Smart views are saved task queries across boards. They live on this device
//! only, outside board synchronization and Undo, and are included in full
//! backups.

use crate::errors::DomainError;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

const METADATA_KEY: &str = "smart_views";
pub const MAX_VIEWS: usize = 100;
const MAX_NAME_CHARS: usize = 100;
const MAX_QUERY_CHARS: usize = 500;
const MAX_BOARDS: usize = 1000;
const MAX_LABELS: usize = 100;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DueFilter {
    #[default]
    All,
    Overdue,
    Today,
    /// Overdue or due today.
    ByToday,
    /// Due today or in the following six days.
    Next7Days,
    None,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    #[default]
    Priority,
    Due,
    Created,
    Title,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    #[default]
    None,
    Board,
    Priority,
    Due,
}

/// Conditions that all have to match; a list matches any of its values.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct SmartViewFilter {
    pub query: String,
    /// `None` covers every board, including boards created later. Full
    /// backups store positions in the backup's board list instead of IDs.
    pub board_ids: Option<Vec<i64>>,
    /// `open`, `done` and `archived`; empty keeps all.
    pub completion: Vec<String>,
    /// Task priorities, with `none` for tasks without one; empty keeps all.
    pub priorities: Vec<String>,
    pub due: DueFilter,
    /// Any of these labels; empty keeps all.
    pub labels: Vec<String>,
    /// `waiting`, `not_waiting` or `waited_on`.
    pub waiting: Option<String>,
    pub recurring: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SmartView {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub filter: SmartViewFilter,
    #[serde(default)]
    pub sort: Sort,
    #[serde(default)]
    pub group: Group,
}

fn all_in(values: &[String], allowed: &[&str]) -> bool {
    let mut seen = std::collections::HashSet::new();
    values
        .iter()
        .all(|value| allowed.contains(&value.as_str()) && seen.insert(value))
}

fn valid_view(view: &SmartView) -> bool {
    let filter = &view.filter;
    let name = view.name.trim();
    !view.id.is_empty()
        && view.id.len() <= 64
        && !name.is_empty()
        && name.chars().count() <= MAX_NAME_CHARS
        && filter.query.chars().count() <= MAX_QUERY_CHARS
        && filter
            .board_ids
            .as_ref()
            .is_none_or(|ids| ids.len() <= MAX_BOARDS)
        && all_in(&filter.completion, &["open", "done", "archived"])
        && all_in(
            &filter.priorities,
            &["none", "low", "medium", "high", "urgent"],
        )
        && filter.labels.len() <= MAX_LABELS
        && filter
            .waiting
            .as_deref()
            .is_none_or(|waiting| ["waiting", "not_waiting", "waited_on"].contains(&waiting))
}

/// Checks a whole list, as saved or restored: valid views with unique IDs.
pub fn validate(views: &[SmartView]) -> Result<(), DomainError> {
    let mut ids = std::collections::HashSet::new();
    if views.len() > MAX_VIEWS
        || !views
            .iter()
            .all(|view| valid_view(view) && ids.insert(view.id.as_str()))
    {
        return Err(DomainError::InvalidArgument);
    }
    Ok(())
}

/// `None` until views were first saved, so the app can offer starter views.
pub fn load(connection: &Connection) -> Result<Option<Vec<SmartView>>, DomainError> {
    let value: Option<String> = connection
        .query_row(
            "SELECT value FROM cardbe_global_metadata WHERE key=?1",
            [METADATA_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| DomainError::Internal(error.to_string()))?;
    Ok(value.map(|value| {
        // Unreadable views are dropped rather than blocking the app.
        serde_json::from_str(&value).unwrap_or_else(|error| {
            log::warn!(target: "smart_views", "Ignoring unreadable smart views: {error}");
            Vec::new()
        })
    }))
}

pub fn save(connection: &Connection, views: &[SmartView]) -> Result<(), DomainError> {
    validate(views)?;
    let value =
        serde_json::to_string(views).map_err(|error| DomainError::Internal(error.to_string()))?;
    connection
        .execute(
            "INSERT INTO cardbe_global_metadata(key,value) VALUES(?1,?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [METADATA_KEY, value.as_str()],
        )
        .map_err(|error| DomainError::Internal(error.to_string()))?;
    Ok(())
}

/// Rewrites board filters from one numbering to another, such as board IDs to
/// backup positions. Boards without a mapping are dropped from the filter.
pub fn map_boards(views: &mut [SmartView], map: impl Fn(i64) -> Option<i64>) {
    for view in views {
        if let Some(ids) = &mut view.filter.board_ids {
            *ids = ids.iter().filter_map(|id| map(*id)).collect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(id: &str) -> SmartView {
        SmartView {
            id: id.into(),
            name: "Urgent work".into(),
            filter: SmartViewFilter {
                board_ids: Some(vec![3, 7]),
                completion: vec!["open".into()],
                priorities: vec!["urgent".into(), "none".into()],
                due: DueFilter::ByToday,
                labels: vec!["工作".into()],
                waiting: Some("waiting".into()),
                ..SmartViewFilter::default()
            },
            sort: Sort::Due,
            group: Group::Board,
        }
    }

    #[test]
    fn views_round_trip_and_start_unset() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute(
                "CREATE TABLE cardbe_global_metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL)",
                [],
            )
            .unwrap();
        assert_eq!(load(&connection).unwrap(), None);
        let views = vec![view("a"), view("b")];
        save(&connection, &views).unwrap();
        assert_eq!(load(&connection).unwrap(), Some(views));
        save(&connection, &[]).unwrap();
        assert_eq!(load(&connection).unwrap(), Some(Vec::new()));
    }

    #[test]
    fn invalid_views_are_rejected() {
        assert!(validate(&[view("a")]).is_ok());
        assert!(validate(&[view("a"), view("a")]).is_err());
        let invalid = [
            SmartView {
                name: "  ".into(),
                ..view("a")
            },
            SmartView {
                id: String::new(),
                ..view("a")
            },
        ];
        for view in invalid {
            assert!(validate(&[view]).is_err());
        }
        let mut filtered = view("a");
        filtered.filter.completion = vec!["open".into(), "open".into()];
        assert!(validate(&[filtered]).is_err());
        let mut filtered = view("a");
        filtered.filter.priorities = vec!["critical".into()];
        assert!(validate(&[filtered]).is_err());
        let mut filtered = view("a");
        filtered.filter.waiting = Some("blocked".into());
        assert!(validate(&[filtered]).is_err());
        assert!(serde_json::from_str::<SmartView>(r#"{"id":"a","name":"x","extra":1}"#).is_err());
    }

    #[test]
    fn board_filters_are_remapped_and_unknown_boards_dropped() {
        let mut views = vec![
            view("a"),
            SmartView {
                filter: SmartViewFilter::default(),
                ..view("b")
            },
        ];
        map_boards(&mut views, |id| (id == 7).then_some(0));
        assert_eq!(views[0].filter.board_ids, Some(vec![0]));
        assert_eq!(views[1].filter.board_ids, None);
    }
}
