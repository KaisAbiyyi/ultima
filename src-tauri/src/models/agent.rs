//! Agent model definition
//!
//! Represents an AI agent configuration with provider settings.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

// === Type Aliases ===

/// Strongly-typed Agent ID
pub type AgentId = String;

// === Enums ===

/// AI Provider types supported by the system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentProvider {
    /// Local llama-server inference (port 9991)
    #[serde(alias = "local")]
    LlamaServer,
    /// Ollama local inference
    Ollama,
    /// OpenRouter API
    OpenRouter,
    /// OpenAI API
    OpenAi,
    /// Anthropic API
    Anthropic,
    /// Custom OpenAI-compatible API
    Custom,
}

impl AgentProvider {
    /// Check if provider requires API key
    pub fn requires_api_key(&self) -> bool {
        matches!(
            self,
            AgentProvider::OpenRouter
                | AgentProvider::OpenAi
                | AgentProvider::Anthropic
                | AgentProvider::Custom
        )
    }

    /// Check if provider requires model path
    pub fn requires_model_path(&self) -> bool {
        matches!(self, AgentProvider::LlamaServer)
    }

    /// Get default API endpoint for provider
    pub fn default_endpoint(&self) -> Option<&'static str> {
        match self {
            AgentProvider::LlamaServer => Some("http://127.0.0.1:6661"),
            AgentProvider::Ollama => Some("http://localhost:11434"),
            AgentProvider::OpenRouter => Some("https://openrouter.ai/api/v1"),
            AgentProvider::OpenAi => Some("https://api.openai.com/v1"),
            AgentProvider::Anthropic => Some("https://api.anthropic.com/v1"),
            AgentProvider::Custom => None,
        }
    }
}

impl Default for AgentProvider {
    fn default() -> Self {
        AgentProvider::LlamaServer
    }
}

impl std::fmt::Display for AgentProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentProvider::LlamaServer => write!(f, "llama_server"),
            AgentProvider::Ollama => write!(f, "ollama"),
            AgentProvider::OpenRouter => write!(f, "open_router"),
            AgentProvider::OpenAi => write!(f, "open_ai"),
            AgentProvider::Anthropic => write!(f, "anthropic"),
            AgentProvider::Custom => write!(f, "custom"),
        }
    }
}

// === Main Model ===

/// Agent configuration entity
///
/// Represents a configured AI agent with all settings
/// required for inference operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    /// Unique identifier (UUID v4)
    pub id: AgentId,

    /// Display name for the agent
    pub name: String,

    /// System prompt / instruction for the agent
    pub system_prompt: String,

    /// AI provider type
    pub provider: AgentProvider,

    /// Path to local model file (for Local provider)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_path: Option<String>,

    /// Path to multimodal projector file (.mmproj) for vision models
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mmproj_path: Option<String>,

    /// Model identifier (for external providers)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,

    /// API key (for external providers)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Custom API endpoint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_endpoint: Option<String>,

    /// Context window size in tokens
    pub context_window: u32,

    /// Whether this agent aggregates other agents
    pub is_aggregator: bool,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
}

impl Agent {
    /// Create a new Agent with generated ID and timestamps
    pub fn new(name: String, system_prompt: String, provider: AgentProvider) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            system_prompt,
            provider,
            model_path: None,
            mmproj_path: None,
            model_id: None,
            api_key: None,
            api_endpoint: None,
            context_window: 4096,
            is_aggregator: false,
            created_at: now,
            updated_at: now,
        }
    }

    /// Validate agent configuration
    pub fn validate(&self) -> AppResult<()> {
        // Name validation
        if self.name.trim().is_empty() {
            return Err(AppError::Validation {
                field: "name".to_string(),
                message: "Name cannot be empty".to_string(),
            });
        }

        if self.name.len() > 100 {
            return Err(AppError::Validation {
                field: "name".to_string(),
                message: "Name cannot exceed 100 characters".to_string(),
            });
        }

        // Provider-specific validation
        if self.provider.requires_model_path() && self.model_path.is_none() {
            return Err(AppError::Validation {
                field: "model_path".to_string(),
                message: "Model path required for local provider".to_string(),
            });
        }

        if self.provider.requires_api_key() && self.api_key.is_none() {
            return Err(AppError::Validation {
                field: "api_key".to_string(),
                message: format!("API key required for {} provider", self.provider),
            });
        }

        // Context window validation
        if self.context_window < 128 {
            return Err(AppError::Validation {
                field: "context_window".to_string(),
                message: "Context window must be at least 128".to_string(),
            });
        }

        Ok(())
    }

    /// Get the effective API endpoint
    pub fn get_endpoint(&self) -> String {
        self.api_endpoint
            .clone()
            .or_else(|| self.provider.default_endpoint().map(String::from))
            .unwrap_or_else(|| "http://127.0.0.1:6661".to_string())
    }
}

// === DTOs ===

/// Request to create a new agent
#[derive(Debug, Clone, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub system_prompt: String,
    pub provider: AgentProvider,
    #[serde(default)]
    pub model_path: Option<String>,
    #[serde(default)]
    pub mmproj_path: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_endpoint: Option<String>,
    #[serde(default = "default_context_window")]
    pub context_window: u32,
    #[serde(default)]
    pub is_aggregator: bool,
}

fn default_context_window() -> u32 {
    4096
}

impl CreateAgentRequest {
    /// Convert request to Agent entity
    pub fn into_agent(self) -> Agent {
        let mut agent = Agent::new(self.name, self.system_prompt, self.provider);
        agent.model_path = self.model_path;
        agent.mmproj_path = self.mmproj_path;
        agent.model_id = self.model_id;
        agent.api_key = self.api_key;
        agent.api_endpoint = self.api_endpoint;
        agent.context_window = self.context_window;
        agent.is_aggregator = self.is_aggregator;
        agent
    }
}

/// Request to update an existing agent
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateAgentRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub provider: Option<AgentProvider>,
    #[serde(default)]
    pub model_path: Option<String>,
    #[serde(default)]
    pub mmproj_path: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_endpoint: Option<String>,
    #[serde(default)]
    pub context_window: Option<u32>,
    #[serde(default)]
    pub is_aggregator: Option<bool>,
}

impl UpdateAgentRequest {
    /// Apply updates to an existing agent
    pub fn apply_to(self, agent: &mut Agent) {
        if let Some(name) = self.name {
            agent.name = name;
        }
        if let Some(prompt) = self.system_prompt {
            agent.system_prompt = prompt;
        }
        if let Some(provider) = self.provider {
            agent.provider = provider;
        }
        if self.model_path.is_some() {
            agent.model_path = self.model_path;
        }
        if self.mmproj_path.is_some() {
            agent.mmproj_path = self.mmproj_path;
        }
        if self.model_id.is_some() {
            agent.model_id = self.model_id;
        }
        if self.api_key.is_some() {
            agent.api_key = self.api_key;
        }
        if self.api_endpoint.is_some() {
            agent.api_endpoint = self.api_endpoint;
        }
        if let Some(ctx) = self.context_window {
            agent.context_window = ctx;
        }
        if let Some(agg) = self.is_aggregator {
            agent.is_aggregator = agg;
        }
        agent.updated_at = Utc::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> CreateAgentRequest {
        CreateAgentRequest {
            name: "Test Agent".to_string(),
            system_prompt: "You are helpful.".to_string(),
            provider: AgentProvider::LlamaServer,
            model_path: Some("/path/to/model.gguf".to_string()),
            model_id: None,
            mmproj_path: None,
            api_key: None,
            api_endpoint: None,
            context_window: 4096,
            is_aggregator: false,
        }
    }

    #[test]
    fn test_agent_creation() {
        let agent = Agent::new(
            "Test Agent".to_string(),
            "You are helpful".to_string(),
            AgentProvider::LlamaServer,
        );

        assert!(!agent.id.is_empty());
        assert_eq!(agent.name, "Test Agent");
        assert_eq!(agent.context_window, 4096);
        assert_eq!(agent.get_endpoint(), "http://127.0.0.1:6661");
    }

    #[test]
    fn test_agent_from_request() {
        let request = sample_request();
        let agent = request.into_agent();

        assert_eq!(agent.name, "Test Agent");
        assert_eq!(agent.provider, AgentProvider::LlamaServer);
        assert_eq!(agent.context_window, 4096);
        assert!(!agent.id.is_empty());
    }

    #[test]
    fn test_agent_apply_update() {
        let request = sample_request();
        let mut agent = request.into_agent();
        let original_id = agent.id.clone();

        let update = UpdateAgentRequest {
            name: Some("Updated Name".to_string()),
            ..Default::default()
        };

        update.apply_to(&mut agent);

        assert_eq!(agent.id, original_id);
        assert_eq!(agent.name, "Updated Name");
    }

    #[test]
    fn test_provider_serialization() {
        assert_eq!(
            serde_json::to_string(&AgentProvider::OpenRouter).unwrap(),
            r#""open_router""#
        );
    }

    #[test]
    fn test_provider_validation() {
        let mut agent = Agent::new(
            "Test".to_string(),
            "Prompt".to_string(),
            AgentProvider::OpenAi,
        );

        // Should fail - no API key
        assert!(agent.validate().is_err());

        // Should pass after adding key
        agent.api_key = Some("sk-test".to_string());
        assert!(agent.validate().is_ok());
    }

    #[test]
    fn test_local_provider_port() {
        let provider = AgentProvider::LlamaServer;
        assert_eq!(provider.default_endpoint(), Some("http://127.0.0.1:6661"));

        let agent = Agent::new(
            "Local Agent".to_string(),
            "Helpful assistant".to_string(),
            AgentProvider::LlamaServer,
        );
        assert_eq!(agent.get_endpoint(), "http://127.0.0.1:6661");
    }

    #[test]
    fn test_requires_model_path() {
        assert!(AgentProvider::LlamaServer.requires_model_path());
        assert!(!AgentProvider::OpenAi.requires_model_path());
    }

    #[test]
    fn test_requires_api_key() {
        assert!(!AgentProvider::LlamaServer.requires_api_key());
        assert!(AgentProvider::OpenAi.requires_api_key());
        assert!(AgentProvider::OpenRouter.requires_api_key());
    }

    #[test]
    fn test_provider_display() {
        assert_eq!(AgentProvider::LlamaServer.to_string(), "llama_server");
        assert_eq!(AgentProvider::OpenRouter.to_string(), "open_router");
        assert_eq!(AgentProvider::OpenAi.to_string(), "open_ai");
        assert_eq!(AgentProvider::Anthropic.to_string(), "anthropic");
        assert_eq!(AgentProvider::Custom.to_string(), "custom");
    }

    #[test]
    fn test_validation_empty_name() {
        let agent = Agent::new(
            "".to_string(),
            "Prompt".to_string(),
            AgentProvider::LlamaServer,
        );
        assert!(agent.validate().is_err());
    }

    #[test]
    fn test_validation_context_window_too_small() {
        let mut agent = Agent::new(
            "Test".to_string(),
            "Prompt".to_string(),
            AgentProvider::LlamaServer,
        );
        agent.context_window = 64;
        assert!(agent.validate().is_err());
    }

    #[test]
    fn test_default_context_window() {
        assert_eq!(default_context_window(), 4096);
    }
}

impl Default for UpdateAgentRequest {
    fn default() -> Self {
        Self {
            name: None,
            system_prompt: None,
            provider: None,
            model_path: None,
            mmproj_path: None,
            model_id: None,
            api_key: None,
            api_endpoint: None,
            context_window: None,
            is_aggregator: None,
        }
    }
}
