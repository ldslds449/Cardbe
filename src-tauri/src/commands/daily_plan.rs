use crate::{
    errors::{CommandError, DomainError},
    models::{Task, TaskSummary},
    state::{lock_app, SharedAppData},
};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

const KEY: &str = "daily_plan";
const MAX_TASKS: usize = 100;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub struct PlannedTask {
    pub board_id: i64,
    pub task_id: i64,
}

#[derive(Serialize)]
pub struct PlanItem {
    pub board_id: i64,
    pub board_name: String,
    pub column_name: String,
    pub done: bool,
    pub task: TaskSummary,
}

fn internal(error: impl std::fmt::Display) -> DomainError {
    DomainError::Internal(error.to_string())
}

fn load(connection: &Connection) -> Result<Vec<PlannedTask>, DomainError> {
    let value: Option<String> = connection
        .query_row(
            "SELECT value FROM cardbe_global_metadata WHERE key=?1",
            [KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(internal)?;
    value.map_or(Ok(Vec::new()), |value| {
        serde_json::from_str(&value).map_err(internal)
    })
}

fn save(connection: &Connection, tasks: &[PlannedTask]) -> Result<(), DomainError> {
    let mut seen = std::collections::HashSet::new();
    if tasks.len() > MAX_TASKS
        || tasks
            .iter()
            .any(|task| task.board_id <= 0 || task.task_id <= 0 || !seen.insert(task))
    {
        return Err(DomainError::InvalidArgument);
    }
    let value = serde_json::to_string(tasks).map_err(internal)?;
    connection.execute(
        "INSERT INTO cardbe_global_metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [KEY, value.as_str()],
    ).map_err(internal)?;
    Ok(())
}

// Device-local ordered references; task content remains owned by its board.
#[tauri::command]
pub fn get_daily_plan(state: State<'_, SharedAppData>) -> Result<Vec<PlanItem>, CommandError> {
    let guard = lock_app(&state)?;
    read_items(&guard.database.connection).map_err(CommandError::from)
}

fn read_items(connection: &Connection) -> Result<Vec<PlanItem>, DomainError> {
    let mut query = connection.prepare(
        "SELECT b.name,c.name,(c.id=(SELECT d.id FROM cardbe_columns d WHERE d.board_id=t.board_id AND d.done=1 ORDER BY d.position,d.id LIMIT 1)),t.payload FROM cardbe_tasks t JOIN cardbe_boards b ON b.id=t.board_id JOIN cardbe_columns c ON c.board_id=t.board_id AND c.id=t.column_id WHERE t.board_id=?1 AND t.id=?2",
    ).map_err(internal)?;
    let mut items = Vec::new();
    for reference in load(connection)? {
        let row: Option<(String, String, bool, String)> = query
            .query_row([reference.board_id, reference.task_id], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get::<_, Option<bool>>(2)?.unwrap_or(false),
                    row.get(3)?,
                ))
            })
            .optional()
            .map_err(internal)?;
        // Deleted, archived and transferred tasks no longer belong to this plan.
        if let Some((board_name, column_name, done, payload)) = row {
            items.push(PlanItem {
                board_id: reference.board_id,
                board_name,
                column_name,
                done,
                task: TaskSummary::from(&serde_json::from_str::<Task>(&payload).map_err(internal)?),
            });
        }
    }
    Ok(items)
}

#[tauri::command]
pub fn set_daily_plan(
    app: tauri::AppHandle,
    state: State<'_, SharedAppData>,
    tasks: Vec<PlannedTask>,
) -> Result<(), CommandError> {
    {
        let guard = lock_app(&state)?;
        save(&guard.database.connection, &tasks).map_err(CommandError::from)?;
    }
    if let Err(error) = app.emit("cardbe:daily-plan-changed", ()) {
        log::warn!(target: "daily_plan", "Could not announce daily plan: {error}");
    }
    Ok(())
}

/// Appends tasks of one board after the current plan, skipping planned ones,
/// and returns how many were added.
fn append(connection: &Connection, board_id: i64, task_ids: &[i64]) -> Result<usize, DomainError> {
    let mut tasks = load(connection)?;
    let before = tasks.len();
    let mut exists = connection
        .prepare("SELECT 1 FROM cardbe_tasks WHERE board_id=?1 AND id=?2")
        .map_err(internal)?;
    for &task_id in task_ids {
        let task = PlannedTask { board_id, task_id };
        if tasks.contains(&task) {
            continue;
        }
        if !exists.exists([board_id, task_id]).map_err(internal)? {
            return Err(DomainError::TaskNotFound);
        }
        tasks.push(task);
    }
    if tasks.len() > MAX_TASKS {
        return Err(DomainError::DailyPlanFull);
    }
    if tasks.len() > before {
        save(connection, &tasks)?;
    }
    Ok(tasks.len() - before)
}

#[tauri::command]
pub fn add_to_daily_plan(
    app: tauri::AppHandle,
    state: State<'_, SharedAppData>,
    board_id: i64,
    task_ids: Vec<i64>,
) -> Result<usize, CommandError> {
    let added = {
        let guard = lock_app(&state)?;
        append(&guard.database.connection, board_id, &task_ids).map_err(CommandError::from)?
    };
    if added > 0 {
        if let Err(error) = app.emit("cardbe:daily-plan-changed", ()) {
            log::warn!(target: "daily_plan", "Could not announce daily plan: {error}");
        }
    }
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_current_tasks_by_board_and_ignores_missing_references() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(r#"
            CREATE TABLE cardbe_global_metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE cardbe_boards(id INTEGER PRIMARY KEY,name TEXT);
            CREATE TABLE cardbe_columns(board_id INTEGER,id INTEGER,name TEXT,done INTEGER,position INTEGER);
            CREATE TABLE cardbe_tasks(board_id INTEGER,id INTEGER,column_id INTEGER,payload TEXT);
            INSERT INTO cardbe_boards VALUES(1,'One'),(2,'Two');
            INSERT INTO cardbe_columns VALUES(1,1,'Todo',0,0),(1,2,'Done',1,1),(2,1,'Todo',0,0);
            INSERT INTO cardbe_tasks VALUES(1,1,1,'{"id":1,"title":"First"}'),(2,1,1,'{"id":1,"title":"Second"}');
        "#).unwrap();
        let tasks = vec![
            PlannedTask {
                board_id: 2,
                task_id: 1,
            },
            PlannedTask {
                board_id: 1,
                task_id: 1,
            },
            PlannedTask {
                board_id: 1,
                task_id: 99,
            },
        ];
        save(&db, &tasks).unwrap();
        let items = read_items(&db).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].task.title, "Second");
        assert_eq!(items[1].task.title, "First");
        assert!(!items[1].done);
        db.execute("UPDATE cardbe_tasks SET column_id=2 WHERE board_id=1", [])
            .unwrap();
        assert!(read_items(&db).unwrap()[1].done);
        db.execute("DELETE FROM cardbe_tasks WHERE board_id=2", [])
            .unwrap();
        let items = read_items(&db).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].board_id, 1);
        // A missing task must not make a later read discard other saved choices.
        assert_eq!(load(&db).unwrap(), tasks);
    }

    #[test]
    fn plan_persists_order_and_validates_identity_without_changing_tasks() {
        let path = std::env::temp_dir().join(format!(
            "cardbe-daily-plan-test-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE cardbe_global_metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);",
        )
        .unwrap();
        assert!(load(&db).unwrap().is_empty());
        let tasks = vec![
            PlannedTask {
                board_id: 2,
                task_id: 1,
            },
            PlannedTask {
                board_id: 1,
                task_id: 1,
            },
        ];
        save(&db, &tasks).unwrap();
        drop(db);
        let db = Connection::open(&path).unwrap();
        assert_eq!(load(&db).unwrap(), tasks);
        assert!(save(&db, &[tasks[0].clone(), tasks[0].clone()]).is_err());
        assert!(save(
            &db,
            &[PlannedTask {
                board_id: 0,
                task_id: 1
            }]
        )
        .is_err());
        assert!(save(&db, &vec![tasks[0].clone(); MAX_TASKS + 1]).is_err());
        assert_eq!(load(&db).unwrap(), tasks);
        save(&db, &[]).unwrap();
        assert!(load(&db).unwrap().is_empty());
        drop(db);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn append_skips_planned_tasks_and_rejects_missing_or_overflowing_ones() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            r#"
            CREATE TABLE cardbe_global_metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
            CREATE TABLE cardbe_tasks(board_id INTEGER,id INTEGER,column_id INTEGER,payload TEXT);
            WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<=100)
            INSERT INTO cardbe_tasks SELECT 1,i,1,'{}' FROM n;
            INSERT INTO cardbe_tasks VALUES(2,1,1,'{}');
        "#,
        )
        .unwrap();
        save(
            &db,
            &[PlannedTask {
                board_id: 2,
                task_id: 1,
            }],
        )
        .unwrap();
        assert_eq!(append(&db, 1, &[2, 1, 2]).unwrap(), 2);
        assert_eq!(append(&db, 2, &[1]).unwrap(), 0);
        let planned = |board_id, task_id| PlannedTask { board_id, task_id };
        assert_eq!(
            load(&db).unwrap(),
            vec![planned(2, 1), planned(1, 2), planned(1, 1)]
        );
        assert!(matches!(
            append(&db, 2, &[99]),
            Err(DomainError::TaskNotFound)
        ));
        assert!(matches!(
            append(&db, 1, &(3..=100).collect::<Vec<_>>()),
            Err(DomainError::DailyPlanFull)
        ));
        assert_eq!(load(&db).unwrap().len(), 3);
        assert_eq!(append(&db, 1, &(3..=99).collect::<Vec<_>>()).unwrap(), 97);
        assert_eq!(load(&db).unwrap().len(), MAX_TASKS);
    }
}
