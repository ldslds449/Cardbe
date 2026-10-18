use crate::{
    errors::DomainError,
    models::{Column, ColumnSort, StoredData, Task},
};

pub(crate) fn column_index(data: &StoredData, column_id: i64) -> Result<usize, DomainError> {
    data.columns
        .iter()
        .position(|column| column.id == column_id)
        .ok_or(DomainError::ColumnNotFound)
}

pub(crate) fn task_position(
    data: &StoredData,
    task_id: i64,
) -> Result<(usize, usize), DomainError> {
    data.columns
        .iter()
        .enumerate()
        .find_map(|(column_idx, column)| {
            column
                .tasks
                .iter()
                .position(|task| task.id == task_id)
                .map(|task_idx| (column_idx, task_idx))
        })
        .ok_or(DomainError::TaskNotFound)
}

pub(crate) fn add_column(
    data: &mut StoredData,
    name: String,
    color: String,
) -> Result<i64, DomainError> {
    let id = data.allocate_column_id()?;
    data.columns.push(Column {
        id,
        name,
        color,
        sort_order: ColumnSort::Custom,
        tasks: Vec::new(),
    });
    Ok(id)
}

pub(crate) fn update_column(
    data: &mut StoredData,
    column_id: i64,
    name: String,
    color: String,
    sort_order: ColumnSort,
) -> Result<(), DomainError> {
    let index = column_index(data, column_id)?;
    data.columns[index].name = name;
    data.columns[index].color = color;
    data.columns[index].sort_order = sort_order;
    Ok(())
}

pub(crate) fn move_column(
    data: &mut StoredData,
    column_id: i64,
    before_column_id: Option<i64>,
) -> Result<(), DomainError> {
    if before_column_id == Some(column_id) {
        return Ok(());
    }
    let from = column_index(data, column_id)?;
    let column = data.columns.remove(from);
    let destination = match before_column_id {
        Some(id) => column_index(data, id)?,
        None => data.columns.len(),
    };
    data.columns.insert(destination, column);
    Ok(())
}

pub(crate) fn move_task(
    data: &mut StoredData,
    task_id: i64,
    to_column_id: i64,
    before_task_id: Option<i64>,
) -> Result<(), DomainError> {
    if before_task_id == Some(task_id) {
        return Ok(());
    }
    let (from_column, from_task) = task_position(data, task_id)?;
    let task = data.columns[from_column].tasks.remove(from_task);
    let to_column = column_index(data, to_column_id)?;
    let destination = match before_task_id {
        Some(id) => data.columns[to_column]
            .tasks
            .iter()
            .position(|candidate| candidate.id == id)
            .ok_or(DomainError::TaskNotFound)?,
        None => data.columns[to_column].tasks.len(),
    };
    data.columns[to_column].tasks.insert(destination, task);
    Ok(())
}

pub(crate) fn add_task(
    data: &mut StoredData,
    column_id: i64,
    mut task: Task,
    after_task_id: Option<i64>,
) -> Result<i64, DomainError> {
    let column = column_index(data, column_id)?;
    let destination = match after_task_id {
        Some(id) => {
            data.columns[column]
                .tasks
                .iter()
                .position(|candidate| candidate.id == id)
                .ok_or(DomainError::TaskNotFound)?
                + 1
        }
        None => data.columns[column].tasks.len(),
    };
    let id = data.allocate_task_id()?;
    task.id = id;
    for label in &task.labels {
        data.touch_label(label.clone());
    }
    data.columns[column].tasks.insert(destination, task);
    Ok(id)
}

pub(crate) fn update_task(
    data: &mut StoredData,
    task_id: i64,
    mut task: Task,
) -> Result<(), DomainError> {
    let (column, index) = task_position(data, task_id)?;
    let previous = &data.columns[column].tasks[index];
    let previous_labels = previous.labels.clone();
    // Creation time is immutable once known; legacy tasks may receive one here.
    // Legacy tasks stored their creation time in start_time, so callers that
    // omit it (plugins) must not leave a new start time read as the creation.
    if previous.created_time != 0 {
        task.created_time = previous.created_time;
    } else if task.created_time == 0 {
        task.created_time = previous.start_time;
    }
    task.id = task_id;
    for label in &task.labels {
        if !previous_labels.contains(label) {
            data.touch_label(label.clone());
        }
    }
    data.columns[column].tasks[index] = task;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(task: Task) -> StoredData {
        let mut data = StoredData::default();
        data.columns.push(Column {
            id: 1,
            name: "Todo".into(),
            color: String::new(),
            sort_order: ColumnSort::Custom,
            tasks: vec![Task { id: 2, ..task }],
        });
        data
    }

    #[test]
    fn update_keeps_known_creation_time_and_resolves_legacy_creation() {
        let mut data = board(Task {
            created_time: 1_700_000_000_000,
            ..Task::default()
        });
        let edited = Task {
            start_time: 1_800_000_000_000,
            created_time: 5,
            ..Task::default()
        };
        update_task(&mut data, 2, edited.clone()).unwrap();
        assert_eq!(data.columns[0].tasks[0].created_time, 1_700_000_000_000);

        // A legacy start_time was the creation time; a caller without
        // created_time must not turn its new start into the creation time.
        let mut legacy = board(Task {
            start_time: 1_600_000_000_000,
            ..Task::default()
        });
        update_task(
            &mut legacy,
            2,
            Task {
                created_time: 0,
                ..edited
            },
        )
        .unwrap();
        let task = &legacy.columns[0].tasks[0];
        assert_eq!(
            (task.start_time, task.created_time),
            (1_800_000_000_000, 1_600_000_000_000)
        );
    }
}
