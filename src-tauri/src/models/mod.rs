//! Domain models for Ultimate AI
//!
//! Contains all core business entities and value objects.

#![allow(dead_code)]
#![allow(unused_imports)]

mod agent;
mod chat;
mod inference;
mod project;

// Agent exports
pub use agent::Agent;
pub use agent::AgentProvider;
pub use agent::CreateAgentRequest;
pub use agent::UpdateAgentRequest;
pub use agent::AgentId;

// Chat exports (for persistent storage)
pub use chat::Chat;
pub use chat::ChatId;
pub use chat::ChatMessage as StoredChatMessage;
pub use chat::MessageId;
pub use chat::MessageRole as StorageMessageRole;
pub use chat::MessageUsage;
pub use chat::CreateChatRequest;
pub use chat::UpdateChatRequest;
pub use chat::CreateMessageRequest;
pub use chat::ChatSummary;
pub use chat::ChatWithMessages;

// Project exports
pub use project::Project;
pub use project::ProjectId;
pub use project::CreateProjectRequest;
pub use project::UpdateProjectRequest;

// Inference exports (for API request/response)
pub use inference::ChatMessage;
pub use inference::MessageRole;
pub use inference::ChatCompletionRequest;
pub use inference::ChatCompletionResponse;
pub use inference::ChatCompletionChoice;
pub use inference::StreamChunk;
pub use inference::StreamChoice;
pub use inference::StreamDelta;
pub use inference::TokenUsage;
pub use inference::InferenceRequest;
pub use inference::InferenceResult;
