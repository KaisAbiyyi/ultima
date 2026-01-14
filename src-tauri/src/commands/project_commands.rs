//! Project-related Tauri commands
//!
//! CRUD operations for projects/folders exposed to frontend via IPC.

use tauri::State;

use crate::error::AppResult;
use crate::models::{
    Project, ProjectId,
    CreateProjectRequest, UpdateProjectRequest,
};
use crate::storage::StorageHandle;

/// Create a new project
#[tauri::command]
pub async fn create_project(
    storage: State<'_, StorageHandle>,
    request: CreateProjectRequest,
) -> AppResult<Project> {
    log::debug!("create_project: {:?}", request);
    storage.create_project(request)
}

/// Get project by ID
#[tauri::command]
pub async fn get_project(
    storage: State<'_, StorageHandle>,
    id: ProjectId,
) -> AppResult<Option<Project>> {
    log::debug!("get_project: {}", id);
    storage.get_project(&id)
}

/// Get all projects
#[tauri::command]
pub async fn get_all_projects(
    storage: State<'_, StorageHandle>,
) -> AppResult<Vec<Project>> {
    log::debug!("get_all_projects");
    storage.get_all_projects()
}

/// Get root-level projects (no parent)
#[tauri::command]
pub async fn get_root_projects(
    storage: State<'_, StorageHandle>,
) -> AppResult<Vec<Project>> {
    log::debug!("get_root_projects");
    storage.get_root_projects()
}

/// Get child projects of a parent
#[tauri::command]
pub async fn get_child_projects(
    storage: State<'_, StorageHandle>,
    parent_id: ProjectId,
) -> AppResult<Vec<Project>> {
    log::debug!("get_child_projects: {}", parent_id);
    storage.get_child_projects(&parent_id)
}

/// Update a project
#[tauri::command]
pub async fn update_project(
    storage: State<'_, StorageHandle>,
    id: ProjectId,
    request: UpdateProjectRequest,
) -> AppResult<Project> {
    log::debug!("update_project: {} {:?}", id, request);
    storage.update_project(&id, request)
}

/// Delete a project
#[tauri::command]
pub async fn delete_project(
    storage: State<'_, StorageHandle>,
    id: ProjectId,
) -> AppResult<bool> {
    log::debug!("delete_project: {}", id);
    storage.delete_project(&id)
}
