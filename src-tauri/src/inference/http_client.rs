//! HTTP client for AI inference
//!
//! Unified client for local and external API calls.

use std::time::Duration;

use reqwest::{Client, Response, header};
use serde::{de::DeserializeOwned, Serialize};

use crate::error::{AppError, AppResult};

/// HTTP client configuration
#[derive(Debug, Clone)]
pub struct HttpClientConfig {
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Max retries on failure
    pub max_retries: u32,
    /// User agent string
    pub user_agent: String,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 120,
            max_retries: 3,
            user_agent: "UltimateAI/1.0".to_string(),
        }
    }
}

/// HTTP client wrapper for inference
#[derive(Debug, Clone)]
pub struct HttpClient {
    client: Client,
    config: HttpClientConfig,
}

impl HttpClient {
    /// Create new HTTP client with config
    pub fn new(config: HttpClientConfig) -> AppResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .user_agent(&config.user_agent)
            .build()
            .map_err(|e| AppError::Http(format!(
                "Failed to create HTTP client: {}", e
            )))?;

        Ok(Self { client, config })
    }

    /// Create with default config
    pub fn default_client() -> AppResult<Self> {
        Self::new(HttpClientConfig::default())
    }

    /// POST JSON request
    pub async fn post_json<T, R>(
        &self,
        url: &str,
        body: &T,
        api_key: Option<&str>,
    ) -> AppResult<R>
    where
        T: Serialize,
        R: DeserializeOwned,
    {
        let mut request = self.client
            .post(url)
            .header(header::CONTENT_TYPE, "application/json");

        // Add authorization if API key provided
        if let Some(key) = api_key {
            request = request.header(header::AUTHORIZATION, format!("Bearer {}", key));
        }

        let response = request
            .json(body)
            .send()
            .await
            .map_err(|e| AppError::Http(format!("Request failed: {}", e)))?;

        self.handle_response(response).await
    }

    /// POST request returning raw response (for streaming)
    pub async fn post_stream<T: Serialize>(
        &self,
        url: &str,
        body: &T,
        api_key: Option<&str>,
    ) -> AppResult<Response> {
        let mut request = self.client
            .post(url)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "text/event-stream");

        if let Some(key) = api_key {
            request = request.header(header::AUTHORIZATION, format!("Bearer {}", key));
        }

        let response = request
            .json(body)
            .send()
            .await
            .map_err(|e| AppError::Http(format!("Stream request failed: {}", e)))?;

        // Check status before returning
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::Http(format!(
                "Request failed with status {}: {}", status, body
            )));
        }

        Ok(response)
    }

    /// GET request
    pub async fn get<R: DeserializeOwned>(
        &self,
        url: &str,
        api_key: Option<&str>,
    ) -> AppResult<R> {
        let mut request = self.client.get(url);

        if let Some(key) = api_key {
            request = request.header(header::AUTHORIZATION, format!("Bearer {}", key));
        }

        let response = request
            .send()
            .await
            .map_err(|e| AppError::Http(format!("GET failed: {}", e)))?;

        self.handle_response(response).await
    }

    /// Health check endpoint
    pub async fn health_check(&self, url: &str) -> AppResult<bool> {
        let response = self.client
            .get(url)
            .timeout(Duration::from_secs(5))
            .send()
            .await;

        Ok(response.is_ok())
    }

    /// Handle response and parse JSON
    async fn handle_response<R: DeserializeOwned>(
        &self,
        response: Response,
    ) -> AppResult<R> {
        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::Http(format!(
                "Request failed with status {}: {}", status, body
            )));
        }

        response
            .json()
            .await
            .map_err(|e| AppError::InvalidResponse(format!(
                "Failed to parse response: {}", e
            )))
    }

    /// Get underlying reqwest client
    pub fn inner(&self) -> &Client {
        &self.client
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = HttpClientConfig::default();
        assert_eq!(config.timeout_secs, 120);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_client_creation() {
        let client = HttpClient::default_client();
        assert!(client.is_ok());
    }
}
