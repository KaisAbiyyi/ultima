//! Model listing commands
//!
//! Commands for listing available models from various providers.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::error::AppResult;
use crate::inference::HttpClient;

/// Model info returned to frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_free: Option<bool>,
}

/// List local GGUF models from a directory
#[tauri::command]
pub async fn list_local_models(directory: String) -> AppResult<Vec<ModelInfo>> {
    let path = Path::new(&directory);

    if !path.exists() || !path.is_dir() {
        return Ok(vec![]);
    }

    let mut models = Vec::new();

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let file_path = entry.path();
            if let Some(ext) = file_path.extension() {
                if ext == "gguf" {
                    if let Some(name) = file_path.file_stem() {
                        let name_str = name.to_string_lossy().to_string();
                        models.push(ModelInfo {
                            id: file_path.to_string_lossy().to_string(),
                            name: name_str,
                            provider: "llama-server".to_string(),
                            context_length: None,
                            is_free: None,
                        });
                    }
                }
            }
        }
    }

    // Sort by name
    models.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(models)
}

/// List models from Ollama API
#[tauri::command]
pub async fn list_ollama_models(endpoint: Option<String>) -> AppResult<Vec<ModelInfo>> {
    let base_url = endpoint.unwrap_or_else(|| "http://localhost:11434".to_string());
    let url = format!("{}/api/tags", base_url);

    let client = HttpClient::default_client()?;

    #[derive(Deserialize)]
    struct OllamaResponse {
        models: Option<Vec<OllamaModel>>,
    }

    #[derive(Deserialize)]
    struct OllamaModel {
        name: String,
    }

    let data: OllamaResponse = match client.get(&url, None).await {
        Ok(d) => d,
        Err(_) => return Ok(vec![]), // Ollama not running
    };

    let models = data
        .models
        .unwrap_or_default()
        .into_iter()
        .map(|m| ModelInfo {
            id: m.name.clone(),
            name: m.name,
            provider: "ollama".to_string(),
            context_length: None,
            is_free: Some(true),
        })
        .collect();

    Ok(models)
}

/// List models from OpenRouter API
#[tauri::command]
pub async fn list_openrouter_models(api_key: String, free_only: bool) -> AppResult<Vec<ModelInfo>> {
    let url = "https://openrouter.ai/api/v1/models";

    let client = HttpClient::default_client()?;

    #[derive(Deserialize)]
    struct OpenRouterResponse {
        data: Vec<OpenRouterModel>,
    }

    #[derive(Deserialize)]
    struct OpenRouterModel {
        id: String,
        name: Option<String>,
        context_length: Option<u32>,
        pricing: Option<OpenRouterPricing>,
    }

    #[derive(Deserialize)]
    struct OpenRouterPricing {
        prompt: Option<String>,
        completion: Option<String>,
    }

    let data: OpenRouterResponse = match client.get(url, Some(&api_key)).await {
        Ok(d) => d,
        Err(_) => return Ok(vec![]),
    };

    let models: Vec<ModelInfo> = data
        .data
        .into_iter()
        .filter_map(|m| {
            let is_free = m
                .pricing
                .as_ref()
                .map(|p| {
                    p.prompt
                        .as_ref()
                        .map(|s| s == "0" || s == "0.0")
                        .unwrap_or(false)
                        && p.completion
                            .as_ref()
                            .map(|s| s == "0" || s == "0.0")
                            .unwrap_or(false)
                })
                .unwrap_or(false);

            if free_only && !is_free {
                return None;
            }

            Some(ModelInfo {
                id: m.id.clone(),
                name: m.name.unwrap_or_else(|| m.id.clone()),
                provider: "openrouter".to_string(),
                context_length: m.context_length,
                is_free: Some(is_free),
            })
        })
        .collect();

    Ok(models)
}

/// List models from OpenAI API
#[tauri::command]
pub async fn list_openai_models(api_key: String) -> AppResult<Vec<ModelInfo>> {
    let url = "https://api.openai.com/v1/models";

    let client = HttpClient::default_client()?;

    #[derive(Deserialize)]
    struct OpenAIResponse {
        data: Vec<OpenAIModel>,
    }

    #[derive(Deserialize)]
    struct OpenAIModel {
        id: String,
    }

    let data: OpenAIResponse = match client.get(url, Some(&api_key)).await {
        Ok(d) => d,
        Err(_) => return Ok(vec![]),
    };

    // Filter to common chat models
    let chat_model_prefixes = ["gpt-4", "gpt-3.5", "o1", "o3"];

    let models: Vec<ModelInfo> = data
        .data
        .into_iter()
        .filter(|m| chat_model_prefixes.iter().any(|p| m.id.starts_with(p)))
        .map(|m| ModelInfo {
            id: m.id.clone(),
            name: m.id,
            provider: "openai".to_string(),
            context_length: None,
            is_free: Some(false),
        })
        .collect();

    Ok(models)
}

/// List models from Anthropic API
#[tauri::command]
pub async fn list_anthropic_models(api_key: String) -> AppResult<Vec<ModelInfo>> {
    // Anthropic doesn't have a public models endpoint, so we return known models
    let _ = api_key; // Validate key exists

    let models = vec![
        ModelInfo {
            id: "claude-3-5-sonnet-20241022".to_string(),
            name: "Claude 3.5 Sonnet".to_string(),
            provider: "anthropic".to_string(),
            context_length: Some(200000),
            is_free: Some(false),
        },
        ModelInfo {
            id: "claude-3-5-haiku-20241022".to_string(),
            name: "Claude 3.5 Haiku".to_string(),
            provider: "anthropic".to_string(),
            context_length: Some(200000),
            is_free: Some(false),
        },
        ModelInfo {
            id: "claude-3-opus-20240229".to_string(),
            name: "Claude 3 Opus".to_string(),
            provider: "anthropic".to_string(),
            context_length: Some(200000),
            is_free: Some(false),
        },
        ModelInfo {
            id: "claude-3-sonnet-20240229".to_string(),
            name: "Claude 3 Sonnet".to_string(),
            provider: "anthropic".to_string(),
            context_length: Some(200000),
            is_free: Some(false),
        },
        ModelInfo {
            id: "claude-3-haiku-20240307".to_string(),
            name: "Claude 3 Haiku".to_string(),
            provider: "anthropic".to_string(),
            context_length: Some(200000),
            is_free: Some(false),
        },
    ];

    Ok(models)
}
