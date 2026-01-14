//! Agent-related Tauri commands
//!
//! CRUD operations exposed to frontend via IPC.

use tauri::State;

use crate::error::AppResult;
use crate::models::{Agent, AgentId, CreateAgentRequest, UpdateAgentRequest};
use crate::storage::StorageHandle;

/// Create a new agent
#[tauri::command]
pub async fn create_agent(
    storage: State<'_, StorageHandle>,
    request: CreateAgentRequest,
) -> AppResult<Agent> {
    storage.create_agent(request)
}

/// Get all agents
#[tauri::command]
pub async fn get_agents(storage: State<'_, StorageHandle>) -> AppResult<Vec<Agent>> {
    storage.get_all_agents()
}

/// Get a single agent by ID
#[tauri::command]
pub async fn get_agent(storage: State<'_, StorageHandle>, id: AgentId) -> AppResult<Option<Agent>> {
    storage.get_agent(&id)
}

/// Update an existing agent
#[tauri::command]
pub async fn update_agent(
    storage: State<'_, StorageHandle>,
    id: AgentId,
    request: UpdateAgentRequest,
) -> AppResult<Agent> {
    storage.update_agent(&id, request)
}

/// Delete an agent
#[tauri::command]
pub async fn delete_agent(storage: State<'_, StorageHandle>, id: AgentId) -> AppResult<bool> {
    storage.delete_agent(&id)
}

/// Search agents by name
#[tauri::command]
pub async fn search_agents(
    storage: State<'_, StorageHandle>,
    query: String,
) -> AppResult<Vec<Agent>> {
    storage.search_agents(&query)
}

/// Get agents by provider type
#[tauri::command]
pub async fn get_agents_by_provider(
    storage: State<'_, StorageHandle>,
    provider: String,
) -> AppResult<Vec<Agent>> {
    storage.get_agents_by_provider(&provider)
}

/// Get aggregator agents only
#[tauri::command]
pub async fn get_aggregator_agents(storage: State<'_, StorageHandle>) -> AppResult<Vec<Agent>> {
    storage.get_aggregator_agents()
}

/// Count total agents
#[tauri::command]
pub async fn count_agents(storage: State<'_, StorageHandle>) -> AppResult<usize> {
    storage.count_agents()
}

/// Check if agent exists
#[tauri::command]
pub async fn agent_exists(storage: State<'_, StorageHandle>, id: AgentId) -> AppResult<bool> {
    storage.agent_exists(&id)
}
