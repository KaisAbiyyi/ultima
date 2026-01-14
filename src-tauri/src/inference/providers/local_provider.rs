//! Local inference provider
//!
//! Communicates with llama-server sidecar.

use async_trait::async_trait;
use reqwest::Response;

use crate::error::{AppError, AppResult};
use crate::inference::HttpClient;
use crate::models::{ChatCompletionRequest, ChatCompletionResponse, ChatMessage};

/// Default local server port
const DEFAULT_PORT: u16 = 6661;

/// Default local server host
const DEFAULT_HOST: &str = "127.0.0.1";

/// Local inference provider using llama-server
pub struct LocalProvider {
    /// HTTP client
    client: HttpClient,
    /// Server host
    host: String,
    /// Server port
    port: u16,
}

impl LocalProvider {
    /// Create new local provider
    pub fn new(client: HttpClient) -> Self {
        Self {
            client,
            host: DEFAULT_HOST.to_string(),
            port: DEFAULT_PORT,
        }
    }

    /// Create with custom host/port
    pub fn with_address(client: HttpClient, host: String, port: u16) -> Self {
        Self { client, host, port }
    }

    /// Get base URL
    fn base_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }

    /// Get chat completions endpoint
    fn completions_url(&self) -> String {
        format!("{}/v1/chat/completions", self.base_url())
    }

    /// Get health endpoint
    fn health_url(&self) -> String {
        format!("{}/health", self.base_url())
    }

    /// Send streaming request
    pub async fn chat_completion_stream(
        &self,
        request: ChatCompletionRequest,
    ) -> AppResult<Response> {
        let url = self.completions_url();

        // Ensure streaming is enabled
        let mut stream_request = request;
        stream_request.stream = true;

        self.client.post_stream(&url, &stream_request, None).await
    }

    /// Build request with model override (local doesn't need model name)
    fn prepare_request(&self, mut request: ChatCompletionRequest) -> ChatCompletionRequest {
        // Local server uses the loaded model, model field is optional
        if request.model.is_empty() {
            request.model = "local".to_string();
        }
        request
    }
}

#[async_trait]
impl super::InferenceProvider for LocalProvider {
    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> AppResult<ChatCompletionResponse> {
        let url = self.completions_url();
        let prepared = self.prepare_request(request);

        self.client.post_json(&url, &prepared, None).await
    }

    async fn health_check(&self) -> AppResult<bool> {
        self.client.health_check(&self.health_url()).await
    }

    fn name(&self) -> &str {
        "local"
    }
}

/// Builder for LocalProvider
pub struct LocalProviderBuilder {
    host: String,
    port: u16,
}

impl LocalProviderBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            host: DEFAULT_HOST.to_string(),
            port: DEFAULT_PORT,
        }
    }

    /// Set host
    pub fn host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    /// Set port
    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Build provider
    pub fn build(self) -> AppResult<LocalProvider> {
        let client = HttpClient::default_client()?;
        Ok(LocalProvider::with_address(client, self.host, self.port))
    }
}

impl Default for LocalProviderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urls() {
        let client = HttpClient::default_client().unwrap();
        let provider = LocalProvider::new(client);

        assert_eq!(provider.base_url(), "http://127.0.0.1:6661");
        assert!(provider.completions_url().contains("/v1/chat/completions"));
    }

    #[test]
    fn test_builder() {
        let provider = LocalProviderBuilder::new()
            .host("localhost")
            .port(9000)
            .build()
            .unwrap();

        assert_eq!(provider.base_url(), "http://localhost:9000");
    }
}
