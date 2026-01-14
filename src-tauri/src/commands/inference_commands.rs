//! Inference-related Tauri commands
//!
//! Commands for chat completion and sidecar control.

use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::error::AppResult;
use crate::inference::providers::{ExternalProvider, InferenceProvider, LocalProvider};
use crate::inference::sidecar_manager::{get_sidecar, SidecarHandle};
use crate::inference::streaming::StreamProcessor;
use crate::inference::HttpClient;
use crate::models::{
    Agent, AgentProvider, ChatCompletionRequest, ChatCompletionResponse, ChatMessage,
};
use crate::storage::StorageHandle;

/// Start local inference server with model
#[tauri::command]
pub async fn start_local_server(
    app_handle: AppHandle,
    sidecar: State<'_, SidecarHandle>,
    model_path: String,
    port: Option<u16>,
    context_window: Option<u32>,
    mmproj_path: Option<String>,
) -> AppResult<()> {
    let mut manager = sidecar.lock().await;
    manager
        .start(&app_handle, &model_path, port, context_window, mmproj_path.as_deref())
        .await
}

/// Stop local inference server
#[tauri::command]
pub async fn stop_local_server(
    app_handle: AppHandle,
    sidecar: State<'_, SidecarHandle>,
) -> AppResult<()> {
    let mut manager = sidecar.lock().await;
    manager.stop(&app_handle).await
}

/// Get local server status
#[tauri::command]
pub async fn get_server_status(sidecar: State<'_, SidecarHandle>) -> AppResult<ServerStatus> {
    let manager = sidecar.lock().await;
    Ok(ServerStatus {
        running: manager.is_running(),
        model_path: manager.model_path().map(String::from),
        port: manager.port(),
    })
}

/// Server status response
#[derive(serde::Serialize)]
pub struct ServerStatus {
    pub running: bool,
    pub model_path: Option<String>,
    pub port: u16,
}

/// Send chat completion (non-streaming)
#[tauri::command]
pub async fn chat_completion(
    app_handle: AppHandle,
    storage: State<'_, StorageHandle>,
    sidecar: State<'_, SidecarHandle>,
    agent_id: String,
    messages: Vec<ChatMessage>,
) -> AppResult<ChatCompletionResponse> {
    let agent = storage.get_agent_or_error(&agent_id)?;

    // Auto-start sidecar if needed
    if agent.provider == AgentProvider::LlamaServer {
        if let Some(model_path) = &agent.model_path {
            let mut manager = sidecar.lock().await;
            manager
                .ensure_running(
                    &app_handle,
                    model_path,
                    Some(agent.context_window),
                    agent.mmproj_path.as_deref(),
                )
                .await?;
        }
    }

    let provider = create_provider(&agent)?;
    let request = build_request(&agent, messages, false);

    provider.chat_completion(request).await
}

/// Send streaming chat completion
#[tauri::command]
pub async fn chat_completion_stream(
    app_handle: AppHandle,
    storage: State<'_, StorageHandle>,
    sidecar: State<'_, SidecarHandle>,
    agent_id: String,
    messages: Vec<ChatMessage>,
) -> AppResult<String> {
    let agent = storage.get_agent_or_error(&agent_id)?;

    let request_id = Uuid::new_v4().to_string();
    let request = build_request(&agent, messages, true);

    match agent.provider {
        AgentProvider::LlamaServer => {
            // Auto-start sidecar
            if let Some(model_path) = &agent.model_path {
                let mut manager = sidecar.lock().await;
                manager
                    .ensure_running(
                        &app_handle,
                        model_path,
                        Some(agent.context_window),
                        agent.mmproj_path.as_deref(),
                    )
                    .await?;
            }

            let client = HttpClient::default_client()?;
            let provider = LocalProvider::new(client);
            let response = provider.chat_completion_stream(request).await?;

            StreamProcessor::process(response, &request_id, &agent_id, &app_handle).await
        }
        _ => {
            let provider = create_external_provider(&agent)?;
            let response = provider.chat_completion_stream(request).await?;

            StreamProcessor::process(response, &request_id, &agent_id, &app_handle).await
        }
    }
}

/// Quick chat (single message, non-streaming)
#[tauri::command]
pub async fn quick_chat(
    app_handle: AppHandle,
    storage: State<'_, StorageHandle>,
    sidecar: State<'_, SidecarHandle>,
    agent_id: String,
    message: String,
) -> AppResult<String> {
    let messages = vec![ChatMessage::user(message)];
    let response = chat_completion(app_handle, storage, sidecar, agent_id, messages).await?;

    Ok(response.content().unwrap_or_default().to_string())
}

// === Helper Functions ===

fn create_provider(agent: &Agent) -> AppResult<Box<dyn InferenceProvider>> {
    match agent.provider {
        AgentProvider::LlamaServer => {
            let client = HttpClient::default_client()?;
            Ok(Box::new(LocalProvider::new(client)))
        }
        _ => {
            let provider = create_external_provider(agent)?;
            Ok(Box::new(provider))
        }
    }
}

fn create_external_provider(agent: &Agent) -> AppResult<ExternalProvider> {
    let api_key = agent
        .api_key
        .clone()
        .ok_or_else(|| crate::error::AppError::ApiKeyMissing(agent.provider.to_string()))?;

    let model = agent.model_id.clone().unwrap_or_default();

    let endpoint = agent
        .api_endpoint
        .clone()
        .or_else(|| agent.provider.default_endpoint().map(String::from))
        .ok_or_else(|| {
            crate::error::AppError::InvalidConfig("No endpoint configured".to_string())
        })?;

    let client = HttpClient::default_client()?;
    Ok(ExternalProvider::new(
        client,
        endpoint,
        api_key,
        agent.provider.clone(),
        model,
    ))
}

fn build_request(agent: &Agent, messages: Vec<ChatMessage>, stream: bool) -> ChatCompletionRequest {
    let mut all_messages = Vec::new();

    // Add system prompt if present
    if !agent.system_prompt.is_empty() {
        all_messages.push(ChatMessage::system(&agent.system_prompt));
    }

    all_messages.extend(messages);

    let model = agent
        .model_id
        .clone()
        .unwrap_or_else(|| "local".to_string());

    ChatCompletionRequest {
        model,
        messages: all_messages,
        temperature: Some(0.7),
        top_p: None,
        max_tokens: Some(2048),
        stream,
        stop: None,
        frequency_penalty: None,
        presence_penalty: None,
    }
}
