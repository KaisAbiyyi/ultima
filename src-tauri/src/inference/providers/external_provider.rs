//! External inference provider
//!
//! Supports OpenAI-compatible APIs.

use async_trait::async_trait;
use reqwest::Response;

use crate::error::{AppError, AppResult};
use crate::inference::HttpClient;
use crate::models::{AgentProvider, ChatCompletionRequest, ChatCompletionResponse};

/// External API provider
pub struct ExternalProvider {
    /// HTTP client
    client: HttpClient,
    /// API endpoint URL
    endpoint: String,
    /// API key for authentication
    api_key: String,
    /// Provider type
    provider_type: AgentProvider,
    /// Model identifier
    model: String,
}

impl ExternalProvider {
    /// Create new external provider
    pub fn new(
        client: HttpClient,
        endpoint: String,
        api_key: String,
        provider_type: AgentProvider,
        model: String,
    ) -> Self {
        Self {
            client,
            endpoint,
            api_key,
            provider_type,
            model,
        }
    }

    /// Create from agent provider type with defaults
    pub fn from_provider(
        provider_type: AgentProvider,
        api_key: String,
        model: String,
    ) -> AppResult<Self> {
        let endpoint = provider_type
            .default_endpoint()
            .ok_or_else(|| AppError::InvalidConfig("No default endpoint for provider".to_string()))?
            .to_string();

        let client = HttpClient::default_client()?;

        Ok(Self::new(client, endpoint, api_key, provider_type, model))
    }

    /// Get completions URL
    fn completions_url(&self) -> String {
        format!("{}/chat/completions", self.endpoint.trim_end_matches('/'))
    }

    /// Prepare request with model
    fn prepare_request(&self, mut request: ChatCompletionRequest) -> ChatCompletionRequest {
        request.model = self.model.clone();
        request
    }

    /// Send streaming request
    pub async fn chat_completion_stream(
        &self,
        request: ChatCompletionRequest,
    ) -> AppResult<Response> {
        let url = self.completions_url();
        let mut prepared = self.prepare_request(request);
        prepared.stream = true;

        self.client
            .post_stream(&url, &prepared, Some(&self.api_key))
            .await
    }

    /// Get API key (masked for logging)
    pub fn masked_api_key(&self) -> String {
        if self.api_key.len() > 8 {
            format!(
                "{}...{}",
                &self.api_key[..4],
                &self.api_key[self.api_key.len() - 4..]
            )
        } else {
            "****".to_string()
        }
    }
}

#[async_trait]
impl super::InferenceProvider for ExternalProvider {
    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> AppResult<ChatCompletionResponse> {
        let url = self.completions_url();
        let prepared = self.prepare_request(request);

        self.client
            .post_json(&url, &prepared, Some(&self.api_key))
            .await
    }

    async fn health_check(&self) -> AppResult<bool> {
        // External APIs don't have health endpoints typically
        // Just verify we have credentials
        Ok(!self.api_key.is_empty() && !self.endpoint.is_empty())
    }

    fn name(&self) -> &str {
        match self.provider_type {
            AgentProvider::OpenAi => "openai",
            AgentProvider::OpenRouter => "openrouter",
            AgentProvider::Anthropic => "anthropic",
            AgentProvider::Custom => "custom",
            AgentProvider::LlamaServer => "llama-server",
            AgentProvider::Ollama => "ollama",
        }
    }
}

/// Builder for ExternalProvider
pub struct ExternalProviderBuilder {
    endpoint: Option<String>,
    api_key: Option<String>,
    provider_type: AgentProvider,
    model: Option<String>,
}

impl ExternalProviderBuilder {
    /// Create new builder
    pub fn new(provider_type: AgentProvider) -> Self {
        Self {
            endpoint: None,
            api_key: None,
            provider_type,
            model: None,
        }
    }

    /// Set endpoint (overrides default)
    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// Set API key
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Set model
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Build provider
    pub fn build(self) -> AppResult<ExternalProvider> {
        let api_key = self
            .api_key
            .ok_or_else(|| AppError::ApiKeyMissing(self.provider_type.to_string()))?;

        let model = self
            .model
            .ok_or_else(|| AppError::InvalidConfig("Model not specified".to_string()))?;

        let endpoint = self
            .endpoint
            .or_else(|| self.provider_type.default_endpoint().map(String::from))
            .ok_or_else(|| AppError::InvalidConfig("No endpoint configured".to_string()))?;

        let client = HttpClient::default_client()?;

        Ok(ExternalProvider::new(
            client,
            endpoint,
            api_key,
            self.provider_type,
            model,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urls() {
        let client = HttpClient::default_client().unwrap();
        let provider = ExternalProvider::new(
            client,
            "https://api.openai.com/v1".to_string(),
            "sk-test".to_string(),
            AgentProvider::OpenAi,
            "gpt-4".to_string(),
        );

        assert!(provider.completions_url().contains("/chat/completions"));
    }

    #[test]
    fn test_masked_key() {
        let client = HttpClient::default_client().unwrap();
        let provider = ExternalProvider::new(
            client,
            "https://api.example.com".to_string(),
            "sk-1234567890abcdef".to_string(),
            AgentProvider::OpenAi,
            "gpt-4".to_string(),
        );

        let masked = provider.masked_api_key();
        assert!(masked.contains("..."));
        assert!(!masked.contains("1234567890"));
    }

    #[test]
    fn test_builder() {
        let result = ExternalProviderBuilder::new(AgentProvider::OpenAi)
            .api_key("sk-test")
            .model("gpt-4")
            .build();

        assert!(result.is_ok());
    }
}
