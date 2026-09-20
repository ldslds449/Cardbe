use crate::{
    models::{StoredData, TaskTemplate},
    state::{update_stored_for_board, SharedAppData},
};
use tauri::State;

fn find_task(data: &StoredData, task_id: i64) -> Result<crate::models::Task, String> {
    data.columns
        .iter()
        .flat_map(|column| column.tasks.iter())
        .find(|task| task.id == task_id)
        .cloned()
        .ok_or_else(|| format!("Task not found: {task_id}"))
}

fn normalize_template_task(mut task: crate::models::Task) -> crate::models::Task {
    task.id = -1;
    task.start_time = 0;
    task.due_time = None;
    task.recurrence = None;
    for item in &mut task.items {
        item.completed = false;
    }
    task
}

#[tauri::command]
pub fn get_task_templates(
    state: State<'_, SharedAppData>,
    expected_board_id: i64,
) -> Result<Vec<TaskTemplate>, String> {
    let guard = state
        .lock()
        .map_err(|_| "Application state lock is poisoned".to_string())?;
    if guard.active_board_id != expected_board_id {
        return Err("Stale board request".into());
    }
    Ok(guard.stored.templates.clone())
}

#[tauri::command]
pub fn save_task_template(
    state: State<'_, SharedAppData>,
    task_id: i64,
    name: String,
    expected_board_id: i64,
) -> Result<TaskTemplate, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Template name cannot be empty".to_string());
    }

    update_stored_for_board(&state, expected_board_id, |data| {
        let task = normalize_template_task(find_task(data, task_id)?);

        let template = TaskTemplate {
            id: data.allocate_template_id()?,
            name,
            task,
        };
        data.templates.push(template.clone());
        Ok(template)
    })
}

#[tauri::command]
pub fn update_task_template(
    state: State<'_, SharedAppData>,
    template_id: i64,
    name: String,
    task: crate::models::Task,
    expected_board_id: i64,
) -> Result<TaskTemplate, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Template name cannot be empty".to_string());
    }
    if task.title.trim().is_empty() {
        return Err("Template task title cannot be empty".to_string());
    }

    update_stored_for_board(&state, expected_board_id, |data| {
        let index = data
            .templates
            .iter()
            .position(|template| template.id == template_id)
            .ok_or_else(|| format!("Template not found: {template_id}"))?;
        let template = TaskTemplate {
            id: template_id,
            name,
            task: normalize_template_task(task),
        };
        data.templates[index] = template.clone();
        Ok(template)
    })
}

#[tauri::command]
pub fn delete_task_template(
    state: State<'_, SharedAppData>,
    template_id: i64,
    expected_board_id: i64,
) -> Result<(), String> {
    update_stored_for_board(&state, expected_board_id, |data| {
        let index = data
            .templates
            .iter()
            .position(|template| template.id == template_id)
            .ok_or_else(|| format!("Template not found: {template_id}"))?;
        data.templates.remove(index);
        Ok(())
    })
}
