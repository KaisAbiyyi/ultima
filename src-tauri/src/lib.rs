//! Ultimate AI - Rust Backend
//!
//! High-performance local AI platform with multi-provider support.

use tauri::Manager;

// Module declarations
pub mod commands;
pub mod config;
pub mod error;
pub mod events;
pub mod inference;
pub mod models;
pub mod storage;

// Re-exports for convenience
pub use error::{AppError, AppResult};

// Tauri command handlers
use commands::{agent_commands, chat_commands, inference_commands, project_commands};
use inference::sidecar_manager::init_sidecar;
use storage::init_storage;

/// Configure and run the Tauri application.
///
/// This is the main entry point called from main.rs.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Initialize sidecar manager
            let sidecar = init_sidecar(6661);

            // Initialize storage
            let storage = init_storage(&app.handle())?;

            // Manage state
            app.manage(sidecar);
            app.manage(storage);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Agent commands
            agent_commands::create_agent,
            agent_commands::get_agents,
            agent_commands::get_agent,
            agent_commands::update_agent,
            agent_commands::delete_agent,
            agent_commands::search_agents,
            agent_commands::get_agents_by_provider,
            agent_commands::get_aggregator_agents,
            agent_commands::count_agents,
            agent_commands::agent_exists,
            // Chat commands
            chat_commands::create_chat,
            chat_commands::get_chat,
            chat_commands::get_chat_with_messages,
            chat_commands::get_all_chats,
            chat_commands::get_chats_by_agent,
            chat_commands::get_chats_by_project,
            chat_commands::update_chat,
            chat_commands::delete_chat,
            chat_commands::add_message,
            chat_commands::get_chat_messages,
            // Project commands
            project_commands::create_project,
            project_commands::get_project,
            project_commands::get_all_projects,
            project_commands::get_root_projects,
            project_commands::get_child_projects,
            project_commands::update_project,
            project_commands::delete_project,
            // Inference commands
            inference_commands::start_local_server,
            inference_commands::stop_local_server,
            inference_commands::get_server_status,
            inference_commands::chat_completion,
            inference_commands::chat_completion_stream,
            inference_commands::quick_chat,
            // Model listing commands
            commands::model_commands::list_local_models,
            commands::model_commands::list_ollama_models,
            commands::model_commands::list_openrouter_models,
            commands::model_commands::list_openai_models,
            commands::model_commands::list_anthropic_models,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
