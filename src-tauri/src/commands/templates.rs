use crate::errors::{CommandError, DomainError};
use crate::{
    models::{StoredData, TaskTemplate},
    state::{lock_app, update_stored_for_board, SharedAppData},
};
use tauri::State;

fn find_task(data: &StoredData, task_id: i64) -> Result<crate::models::Task, DomainError> {
    data.columns
        .iter()
        .flat_map(|column| column.tasks.iter())
        .find(|task| task.id == task_id)
        .cloned()
        .ok_or(DomainError::TaskNotFound)
}

fn normalize_template_task(mut task: crate::models::Task) -> crate::models::Task {
    task.id = -1;
    task.start_time = 0;
    task.created_time = 0;
    task.due_time = None;
    task.recurrence = None;
    task.pinned = false;
    task.blocked_by.clear();
    for item in &mut task.items {
        item.completed = false;
    }
    task
}

fn template_name(name: String) -> Result<String, CommandError> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(CommandError::TemplateNameRequired);
    }
    Ok(name)
}

fn insert_template(
    data: &mut StoredData,
    name: String,
    task: crate::models::Task,
) -> Result<TaskTemplate, DomainError> {
    let template = TaskTemplate {
        id: data.allocate_template_id()?,
        name,
        task: normalize_template_task(task),
    };
    data.templates.push(template.clone());
    Ok(template)
}

#[tauri::command]
pub fn get_task_templates(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
) -> Result<Vec<TaskTemplate>, CommandError> {
    let guard = lock_app(&state)?;
    if guard.active_board_id != expected_board_id {
        return Err(CommandError::StaleBoardRequest);
    }
    Ok(guard.stored.templates.clone())
}

#[tauri::command]
pub fn save_task_template(
    state: State<'_, SharedAppData>,
    task_id: i64,
    name: String,
    expected_board_id: i64,
) -> Result<TaskTemplate, CommandError> {
    let name = template_name(name)?;
    update_stored_for_board(&state, expected_board_id, |data| {
        let task = find_task(data, task_id)?;
        insert_template(data, name, task)
    })
    .map_err(CommandError::from)
}

/// Creates a template from a card drafted in the template editor.
#[tauri::command]
pub fn create_task_template(
    state: State<'_, SharedAppData>,
    name: String,
    task: crate::models::Task,
    expected_board_id: i64,
) -> Result<TaskTemplate, CommandError> {
    let name = template_name(name)?;
    if task.title.trim().is_empty() {
        return Err(CommandError::TaskTitleRequired);
    }
    update_stored_for_board(&state, expected_board_id, |data| {
        insert_template(data, name, task)
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn update_task_template(
    state: State<'_, SharedAppData>,
    template_id: i64,
    name: String,
    task: crate::models::Task,
    expected_board_id: i64,
) -> Result<TaskTemplate, CommandError> {
    let name = template_name(name)?;
    if task.title.trim().is_empty() {
        return Err(CommandError::TaskTitleRequired);
    }

    update_stored_for_board(&state, expected_board_id, |data| {
        let index = data
            .templates
            .iter()
            .position(|template| template.id == template_id)
            .ok_or(DomainError::TemplateNotFound)?;
        let template = TaskTemplate {
            id: template_id,
            name,
            task: normalize_template_task(task),
        };
        data.templates[index] = template.clone();
        Ok(template)
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn delete_task_template(
    state: State<'_, SharedAppData>,
    template_id: i64,
    expected_board_id: i64,
) -> Result<(), CommandError> {
    update_stored_for_board(&state, expected_board_id, |data| {
        let index = data
            .templates
            .iter()
            .position(|template| template.id == template_id)
            .ok_or(DomainError::TemplateNotFound)?;
        data.templates.remove(index);
        Ok(())
    })
    .map_err(CommandError::from)
}

#[tauri::command]
pub fn copy_task_template_to_board(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
    template_id: i64,
    target_board_id: i64,
) -> Result<(), CommandError> {
    let mut guard = lock_app(&state)?;
    if guard.active_board_id != expected_board_id {
        return Err(CommandError::StaleBoardRequest);
    }
    guard
        .database
        .copy_task_template_to_board(expected_board_id, template_id, target_board_id)
        .map_err(CommandError::repository)?;
    guard.boards = guard.database.boards().map_err(CommandError::repository)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Task, TaskItem};

    #[test]
    fn template_names_are_trimmed_and_required() {
        assert_eq!(template_name("  Weekly  ".into()).unwrap(), "Weekly");
        assert!(matches!(
            template_name(" 	".into()),
            Err(CommandError::TemplateNameRequired)
        ));
    }

    #[test]
    fn inserted_templates_drop_dates_and_completion() {
        let mut data = StoredData::default();
        let task = Task {
            id: 42,
            title: "Review".into(),
            start_time: 1,
            created_time: 2,
            due_time: Some(3),
            pinned: true,
            blocked_by: vec![7],
            items: vec![TaskItem {
                id: "item".into(),
                text: "Check".into(),
                completed: true,
            }],
            ..Task::default()
        };

        let template = insert_template(&mut data, "Review".into(), task).unwrap();

        assert_eq!(data.templates, vec![template.clone()]);
        assert_eq!(template.task.id, -1);
        assert_eq!(template.task.title, "Review");
        assert_eq!(template.task.due_time, None);
        assert!(!template.task.pinned);
        assert!(template.task.blocked_by.is_empty());
        assert!(template.task.items.iter().all(|item| !item.completed));
        let second = insert_template(&mut data, "Other".into(), Task::default()).unwrap();
        assert_ne!(second.id, template.id);
    }
}
