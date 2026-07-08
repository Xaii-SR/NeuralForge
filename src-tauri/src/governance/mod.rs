pub mod requirements;
pub mod validator;

use crate::core::errors::AppResult;
use crate::database::{with_conn, DbState};
use requirements::{RequirementContract, RequirementHistoryEntry};
use tauri::State;

/// Sprint 1 scope note: "created_by" is a fixed local identity for now -
/// NeuralForge has no user accounts. The column exists so multi-author
/// attribution is a data migration away, not a schema change.
const LOCAL_USER: &str = "local";

#[tauri::command]
pub fn create_requirement(db: State<DbState>, title: String, intent: String, acceptance_criteria: Vec<String>) -> AppResult<RequirementContract> {
    with_conn(&db, |conn| requirements::create(conn, &title, &intent, acceptance_criteria, LOCAL_USER))
}

#[tauri::command]
pub fn update_requirement(db: State<DbState>, id: String, title: String, intent: String, acceptance_criteria: Vec<String>) -> AppResult<RequirementContract> {
    with_conn(&db, |conn| requirements::update(conn, &id, &title, &intent, acceptance_criteria))
}

#[tauri::command]
pub fn set_requirement_status(db: State<DbState>, id: String, status: String) -> AppResult<RequirementContract> {
    with_conn(&db, |conn| requirements::set_status(conn, &id, &status))
}

#[tauri::command]
pub fn get_requirement(db: State<DbState>, id: String) -> AppResult<RequirementContract> {
    with_conn(&db, |conn| requirements::get(conn, &id))
}

#[tauri::command]
pub fn list_requirements(db: State<DbState>) -> AppResult<Vec<RequirementContract>> {
    with_conn(&db, requirements::list)
}

#[tauri::command]
pub fn get_requirement_history(db: State<DbState>, id: String) -> AppResult<Vec<RequirementHistoryEntry>> {
    with_conn(&db, |conn| requirements::history(conn, &id))
}
