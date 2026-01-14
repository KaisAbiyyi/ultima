//! Inference providers
//!
//! Implementations for different AI backends.

#![allow(dead_code)]
#![allow(unused_imports)]

mod local_provider;
mod external_provider;

pub use local_provider::LocalProvider;
pub use external_provider::ExternalProvider;
pub use local_provider::LocalProviderBuilder;

use async_trait::async_trait;
use crate::error::AppResult;
use crate::models::{ChatCompletionRequest, ChatCompletionResponse};

/// Trait for inference providers
#[async_trait]
pub trait InferenceProvider: Send + Sync {
    /// Send chat completion request
    async fn chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> AppResult<ChatCompletionResponse>;

    /// Check if provider is available
    async fn health_check(&self) -> AppResult<bool>;

    /// Get provider name
    fn name(&self) -> &str;
}
