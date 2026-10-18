//! Typed component boundary; storage and mutation services keep their native models.
use super::{
    board,
    runtime::{bindings::cardbe::plugin::types as wit, ExternalTask},
};
use crate::{errors::DomainError, models as model, storage::TaskExplorerQuery};

macro_rules! enum_mapping {
    ($native:ty, $guest:ty, $($case:ident),+) => {
        impl From<$native> for $guest {
            fn from(value: $native) -> Self { match value { $(<$native>::$case => Self::$case),+ } }
        }
        impl From<$guest> for $native {
            fn from(value: $guest) -> Self { match value { $(<$guest>::$case => Self::$case),+ } }
        }
    };
}
enum_mapping!(model::BoardRole, wit::BoardRole, Owner, Editor, Viewer);
enum_mapping!(
    model::SyncStatus,
    wit::SyncStatus,
    Local,
    Pending,
    Synced,
    Conflict
);
enum_mapping!(
    model::ColumnSort,
    wit::ColumnSort,
    Custom,
    DueDateAsc,
    DueDateDesc
);
enum_mapping!(
    model::RecurrenceFrequency,
    wit::RecurrenceFrequency,
    Daily,
    Weekly,
    Monthly
);

impl From<model::Board> for wit::BoardInfo {
    fn from(b: model::Board) -> Self {
        Self {
            id: b.id,
            name: b.name,
            task_count: b.task_count,
            shared_role: b.shared_role.into(),
            is_shared: b.is_shared,
            sync_status: b.sync_status.into(),
            sync_revision: b.sync_revision,
        }
    }
}
impl From<board::ColumnInfo> for wit::ColumnInfo {
    fn from(c: board::ColumnInfo) -> Self {
        Self {
            id: c.id,
            board_id: c.board_id,
            name: c.name,
            color: c.color,
            sort_order: c.sort_order.into(),
            position: c.position,
        }
    }
}
impl TryFrom<model::Task> for wit::Task {
    type Error = DomainError;
    fn try_from(t: model::Task) -> Result<Self, Self::Error> {
        Ok(Self {
            id: t.id,
            title: t.title,
            description: t.description,
            pinned: t.pinned,
            color: t.color,
            start_time: t
                .start_time
                .try_into()
                .map_err(|_| DomainError::InvalidArgument)?,
            due_time: t
                .due_time
                .map(u64::try_from)
                .transpose()
                .map_err(|_| DomainError::InvalidArgument)?,
            labels: t.labels,
            items: t
                .items
                .into_iter()
                .map(|i| wit::TaskItem {
                    id: i.id,
                    text: i.text,
                    completed: i.completed,
                })
                .collect(),
            recurrence: t.recurrence.map(|r| wit::Recurrence {
                frequency: r.frequency.into(),
                interval: r.interval,
            }),
        })
    }
}
impl From<wit::Task> for model::Task {
    fn from(t: wit::Task) -> Self {
        Self {
            id: t.id,
            title: t.title,
            description: t.description,
            pinned: t.pinned,
            color: t.color,
            start_time: t.start_time.into(),
            due_time: t.due_time.map(u128::from),
            created_time: 0,
            labels: t.labels,
            items: t
                .items
                .into_iter()
                .map(|i| model::TaskItem {
                    id: i.id,
                    text: i.text,
                    completed: i.completed,
                })
                .collect(),
            recurrence: t.recurrence.map(|r| model::Recurrence {
                frequency: r.frequency.into(),
                interval: r.interval,
            }),
        }
    }
}
impl TryFrom<board::TaskInfo> for wit::TaskInfo {
    type Error = DomainError;
    fn try_from(t: board::TaskInfo) -> Result<Self, Self::Error> {
        Ok(Self {
            board_id: t.board_id,
            column_id: t.column_id,
            position: t.position,
            archived: t.archived,
            task: t.task.try_into()?,
        })
    }
}
impl From<wit::TaskQuery> for TaskExplorerQuery {
    fn from(q: wit::TaskQuery) -> Self {
        Self {
            query: q.query,
            column_id: q.column_id,
            status: match q.status {
                wit::TaskStatus::All => "all",
                wit::TaskStatus::Active => "active",
                wit::TaskStatus::Archived => "archived",
                wit::TaskStatus::Overdue => "overdue",
                wit::TaskStatus::Recurring => "recurring",
            }
            .into(),
            sort: match q.sort {
                wit::TaskSort::Title => "title",
                wit::TaskSort::Due => "due",
                wit::TaskSort::Column => "column",
                wit::TaskSort::Archived => "archived",
            }
            .into(),
            due: match q.due {
                wit::DueFilter::All => "",
                wit::DueFilter::None => "none",
                wit::DueFilter::Due => "due",
            }
            .into(),
            due_start: q.due_start,
            due_end: q.due_end,
            archive_start: q.archive_start,
            archive_end: q.archive_end,
            search_options: q.search_options.map(|o| crate::search::SearchOptions {
                candidate_limit: o.candidate_limit as usize,
                result_limit: o.result_limit as usize,
                rrf_k: o.rrf_k,
            }),
            ..Self::default()
        }
    }
}
impl From<wit::ColumnReference> for board::Reference {
    fn from(r: wit::ColumnReference) -> Self {
        match r {
            wit::ColumnReference::Id(id) => Self::Id(id),
            wit::ColumnReference::Temporary(key) => Self::Temporary(key),
        }
    }
}
impl From<wit::ExternalTask> for ExternalTask {
    fn from(t: wit::ExternalTask) -> Self {
        Self {
            external_key: t.external_key,
            title: t.title,
            description: t.description,
            url: t.url,
            state: t.state,
            labels: t.labels,
            updated_at: t.updated_at,
            column_id: t.column_id,
        }
    }
}
impl From<wit::Operation> for board::Operation {
    fn from(op: wit::Operation) -> Self {
        match op {
            wit::Operation::CreateColumn(o) => Self::CreateColumn {
                key: o.key,
                name: o.name,
                color: o.color,
            },
            wit::Operation::UpdateColumn(o) => Self::UpdateColumn {
                column: o.column.into(),
                name: o.name,
                color: o.color,
                sort_order: o.sort_order.into(),
            },
            wit::Operation::MoveColumn(o) => Self::MoveColumn {
                column: o.column.into(),
                before: o.before.map(Into::into),
            },
            wit::Operation::CreateTask(o) => Self::CreateTask {
                key: o.key,
                column: o.column.into(),
                task: o.task.into(),
            },
            wit::Operation::UpdateTask(o) => Self::UpdateTask {
                task_id: o.task_id,
                task: o.task.into(),
            },
            wit::Operation::MoveTask(o) => Self::MoveTask {
                task_id: o.task_id,
                column: o.column.into(),
                before_task_id: o.before_task_id,
            },
            wit::Operation::UpsertExternalTask(o) => Self::UpsertExternalTask {
                task: o.task.into(),
                column: o.column.map(Into::into),
            },
        }
    }
}
impl From<wit::Changeset> for board::ChangeSet {
    fn from(c: wit::Changeset) -> Self {
        Self {
            version: c.version,
            operations: c.operations.into_iter().map(Into::into).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_fields_survive_the_component_boundary_without_date_truncation() {
        let task = model::Task {
            id: 123,
            title: "工作 🦀".into(),
            description: "Details".into(),
            pinned: true,
            color: "#123456".into(),
            start_time: 1_800_000_000_000,
            due_time: Some(1_800_000_100_000),
            created_time: 0,
            labels: vec!["Label".into()],
            items: vec![model::TaskItem {
                id: "item".into(),
                text: "Check".into(),
                completed: true,
            }],
            recurrence: Some(model::Recurrence {
                frequency: model::RecurrenceFrequency::Monthly,
                interval: 3,
            }),
        };
        let guest = wit::Task::try_from(task.clone()).unwrap();
        assert_eq!(model::Task::from(guest), task);
        assert!(wit::Task::try_from(model::Task {
            start_time: u128::MAX,
            ..task.clone()
        })
        .is_err());
        assert!(wit::Task::try_from(model::Task {
            due_time: Some(u128::MAX),
            ..task
        })
        .is_err());
    }

    #[test]
    fn typed_filter_preserves_bounds_and_host_controls_board_and_time() {
        let query = wit::TaskQuery {
            query: "Find".into(),
            column_id: Some(42),
            status: wit::TaskStatus::Archived,
            sort: wit::TaskSort::Due,
            due: wit::DueFilter::None,
            due_start: Some(10),
            due_end: Some(20),
            archive_start: Some(30),
            archive_end: Some(40),
            search_options: Some(wit::SearchOptions {
                candidate_limit: 50,
                result_limit: 60,
                rrf_k: 70,
            }),
            offset: 100,
            limit: 50,
        };
        let native: TaskExplorerQuery = query.into();
        assert_eq!(
            (
                native.query.as_str(),
                native.status.as_str(),
                native.sort.as_str(),
                native.due.as_str()
            ),
            ("Find", "archived", "due", "none")
        );
        assert_eq!(
            (
                native.column_id,
                native.due_start,
                native.due_end,
                native.archive_start,
                native.archive_end
            ),
            (Some(42), Some(10), Some(20), Some(30), Some(40))
        );
        assert!(native.board_ids.is_none());
        assert_eq!(native.now, 0);
        let options = native.search_options.unwrap();
        assert_eq!(
            (options.candidate_limit, options.result_limit, options.rrf_k),
            (50, 60, 70)
        );
    }
}
