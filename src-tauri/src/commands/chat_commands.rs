//! Chat-related Tauri commands
//!
//! CRUD operations for chat sessions exposed to frontend via IPC.

use tauri::State;

use crate::error::AppResult;
use crate::models::{
    Chat, ChatId,
    CreateChatRequest, UpdateChatRequest, CreateMessageRequest,
    ChatWithMessages, StoredChatMessage,
};
use crate::storage::StorageHandle;

/// Create a new chat session
#[tauri::command]
pub async fn create_chat(
    storage: State<'_, StorageHandle>,
    request: CreateChatRequest,
) -> AppResult<Chat> {
    log::debug!("create_chat: {:?}", request);
    storage.create_chat(request)
}

/// Get chat by ID
#[tauri::command]
pub async fn get_chat(
    storage: State<'_, StorageHandle>,
    id: ChatId,
) -> AppResult<Option<Chat>> {
    log::debug!("get_chat: {}", id);
    storage.get_chat(&id)
}

/// Get chat with all messages
#[tauri::command]
pub async fn get_chat_with_messages(
    storage: State<'_, StorageHandle>,
    id: ChatId,
) -> AppResult<ChatWithMessages> {
    log::debug!("get_chat_with_messages: {}", id);
    storage.get_chat_with_messages(&id)
}

/// Get all non-archived chats
#[tauri::command]
pub async fn get_all_chats(
    storage: State<'_, StorageHandle>,
) -> AppResult<Vec<Chat>> {
    log::debug!("get_all_chats");
    storage.get_all_chats()
}

/// Get chats by agent ID
#[tauri::command]
pub async fn get_chats_by_agent(
    storage: State<'_, StorageHandle>,
    agent_id: String,
) -> AppResult<Vec<Chat>> {
    log::debug!("get_chats_by_agent: {}", agent_id);
    storage.get_chats_by_agent(&agent_id)
}

/// Get chats by project ID
#[tauri::command]
pub async fn get_chats_by_project(
    storage: State<'_, StorageHandle>,
    project_id: String,
) -> AppResult<Vec<Chat>> {
    log::debug!("get_chats_by_project: {}", project_id);
    storage.get_chats_by_project(&project_id)
}

/// Update chat metadata (rename, move to project, pin/archive)
#[tauri::command]
pub async fn update_chat(
    storage: State<'_, StorageHandle>,
    id: ChatId,
    request: UpdateChatRequest,
) -> AppResult<Chat> {
    log::debug!("update_chat: {} {:?}", id, request);
    storage.update_chat(&id, request)
}

/// Delete a chat
#[tauri::command]
pub async fn delete_chat(
    storage: State<'_, StorageHandle>,
    id: ChatId,
) -> AppResult<bool> {
    log::debug!("delete_chat: {}", id);
    storage.delete_chat(&id)
}

/// Add a message to a chat
#[tauri::command]
pub async fn add_message(
    storage: State<'_, StorageHandle>,
    chat_id: ChatId,
    request: CreateMessageRequest,
) -> AppResult<StoredChatMessage> {
    log::debug!("add_message to chat: {}", chat_id);
    storage.add_message_to_chat(&chat_id, request)
}

/// Get all messages for a chat
#[tauri::command]
pub async fn get_chat_messages(
    storage: State<'_, StorageHandle>,
    chat_id: ChatId,
) -> AppResult<Vec<StoredChatMessage>> {
    log::debug!("get_chat_messages: {}", chat_id);
    storage.get_chat_messages(&chat_id)
}
