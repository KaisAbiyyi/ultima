//! Chat and ChatMessage models for persistent storage
//!
//! Represents chat conversations stored in SQLite database.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

// === Type Aliases ===

/// Strongly-typed Chat ID
pub type ChatId = String;

/// Strongly-typed Message ID
pub type MessageId = String;

// === Main Models ===

/// Chat conversation entity
///
/// Represents a complete chat session with messages,
/// associated with an agent and optionally a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chat {
    /// Unique identifier (UUID v4)
    pub id: ChatId,

    /// Chat title (auto-generated or user-set)
    pub title: String,

    /// Associated agent ID
    pub agent_id: String,

    /// Optional project/folder ID for organization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,

    /// Whether this chat is pinned to top
    #[serde(default)]
    pub is_pinned: bool,

    /// Whether this chat is archived
    #[serde(default)]
    pub archived: bool,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
}

impl Chat {
    /// Create a new Chat with generated ID and timestamps
    pub fn new(title: String, agent_id: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            title,
            agent_id,
            project_id: None,
            is_pinned: false,
            archived: false,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new Chat with project association
    pub fn with_project(title: String, agent_id: String, project_id: String) -> Self {
        let mut chat = Self::new(title, agent_id);
        chat.project_id = Some(project_id);
        chat
    }

    /// Validate chat configuration
    pub fn validate(&self) -> AppResult<()> {
        // Title validation
        if self.title.trim().is_empty() {
            return Err(AppError::Validation {
                field: "title".to_string(),
                message: "Title cannot be empty".to_string(),
            });
        }

        if self.title.len() > 200 {
            return Err(AppError::Validation {
                field: "title".to_string(),
                message: "Title cannot exceed 200 characters".to_string(),
            });
        }

        // Agent ID validation
        if self.agent_id.trim().is_empty() {
            return Err(AppError::Validation {
                field: "agent_id".to_string(),
                message: "Agent ID cannot be empty".to_string(),
            });
        }

        Ok(())
    }

    /// Update the title
    pub fn with_title(mut self, title: String) -> Self {
        self.title = title;
        self.updated_at = Utc::now();
        self
    }

    /// Set pinned status
    pub fn with_pinned(mut self, pinned: bool) -> Self {
        self.is_pinned = pinned;
        self.updated_at = Utc::now();
        self
    }

    /// Set archived status
    pub fn with_archived(mut self, archived: bool) -> Self {
        self.archived = archived;
        self.updated_at = Utc::now();
        self
    }

    /// Move to a different project
    pub fn with_project_id(mut self, project_id: Option<String>) -> Self {
        self.project_id = project_id;
        self.updated_at = Utc::now();
        self
    }
}

/// Chat message entity for persistent storage
///
/// Individual messages within a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Unique identifier (UUID v4)
    pub id: MessageId,

    /// Parent chat ID
    pub chat_id: ChatId,

    /// Message role
    pub role: MessageRole,

    /// Message content (text or markdown)
    pub content: String,

    /// Optional attached image paths (for multimodal)
    #[serde(default)]
    pub images: Vec<String>,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Token usage for this message (assistant only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<MessageUsage>,
}

impl ChatMessage {
    /// Create a new user message
    pub fn user(chat_id: ChatId, content: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            chat_id,
            role: MessageRole::User,
            content,
            images: Vec::new(),
            created_at: Utc::now(),
            usage: None,
        }
    }

    /// Create a new user message with images
    pub fn user_with_images(chat_id: ChatId, content: String, images: Vec<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            chat_id,
            role: MessageRole::User,
            content,
            images,
            created_at: Utc::now(),
            usage: None,
        }
    }

    /// Create a new assistant message
    pub fn assistant(chat_id: ChatId, content: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            chat_id,
            role: MessageRole::Assistant,
            content,
            images: Vec::new(),
            created_at: Utc::now(),
            usage: None,
        }
    }

    /// Create a new system message
    pub fn system(chat_id: ChatId, content: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            chat_id,
            role: MessageRole::System,
            content,
            images: Vec::new(),
            created_at: Utc::now(),
            usage: None,
        }
    }

    /// Add token usage to an assistant message
    pub fn with_usage(mut self, usage: MessageUsage) -> Self {
        self.usage = Some(usage);
        self
    }

    /// Validate message content
    pub fn validate(&self) -> AppResult<()> {
        if self.content.is_empty() && self.images.is_empty() {
            return Err(AppError::Validation {
                field: "content".to_string(),
                message: "Message must have content or images".to_string(),
            });
        }

        if self.content.len() > 100_000 {
            return Err(AppError::Validation {
                field: "content".to_string(),
                message: "Content exceeds maximum length".to_string(),
            });
        }

        Ok(())
    }

    /// Check if message has images
    pub fn has_images(&self) -> bool {
        !self.images.is_empty()
    }
}

/// Message role enum
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

impl Default for MessageRole {
    fn default() -> Self {
        MessageRole::User
    }
}

impl std::fmt::Display for MessageRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageRole::System => write!(f, "system"),
            MessageRole::User => write!(f, "user"),
            MessageRole::Assistant => write!(f, "assistant"),
        }
    }
}

/// Token usage for a single message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

// === DTOs ===

/// Request to create a new chat
#[derive(Debug, Clone, Deserialize)]
pub struct CreateChatRequest {
    pub title: String,
    pub agent_id: String,
    #[serde(default)]
    pub project_id: Option<String>,
}

impl CreateChatRequest {
    /// Convert request to Chat entity
    pub fn into_chat(self) -> Chat {
        if let Some(project_id) = self.project_id {
            Chat::with_project(self.title, self.agent_id, project_id)
        } else {
            Chat::new(self.title, self.agent_id)
        }
    }
}

/// Request to update an existing chat
#[derive(Debug, Clone, Deserialize, Default)]
pub struct UpdateChatRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub project_id: Option<Option<String>>,
    #[serde(default)]
    pub is_pinned: Option<bool>,
    #[serde(default)]
    pub archived: Option<bool>,
}

impl UpdateChatRequest {
    /// Apply updates to an existing chat
    pub fn apply_to(self, chat: &mut Chat) {
        if let Some(title) = self.title {
            chat.title = title;
        }
        if let Some(project_id) = self.project_id {
            chat.project_id = project_id;
        }
        if let Some(pinned) = self.is_pinned {
            chat.is_pinned = pinned;
        }
        if let Some(archived) = self.archived {
            chat.archived = archived;
        }
        chat.updated_at = Utc::now();
    }
}

/// Request to add a message to a chat
#[derive(Debug, Clone, Deserialize)]
pub struct CreateMessageRequest {
    pub chat_id: ChatId,
    pub role: MessageRole,
    pub content: String,
    #[serde(default)]
    pub images: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<MessageUsage>,
}

impl CreateMessageRequest {
    /// Convert request to ChatMessage entity
    pub fn into_message(self) -> ChatMessage {
        let id = Uuid::new_v4().to_string();
        let created_at = Utc::now();

        ChatMessage {
            id,
            chat_id: self.chat_id,
            role: self.role,
            content: self.content,
            images: self.images,
            created_at,
            usage: self.usage,
        }
    }
}

/// Chat summary for list views
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSummary {
    pub id: ChatId,
    pub title: String,
    pub agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default)]
    pub is_pinned: bool,
    #[serde(default)]
    pub archived: bool,
    pub message_count: usize,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Chat> for ChatSummary {
    fn from(chat: Chat) -> Self {
        Self {
            id: chat.id,
            title: chat.title,
            agent_id: chat.agent_id,
            project_id: chat.project_id,
            is_pinned: chat.is_pinned,
            archived: chat.archived,
            message_count: 0, // Will be populated by storage layer
            created_at: chat.created_at,
            updated_at: chat.updated_at,
        }
    }
}

/// Chat with its messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatWithMessages {
    pub chat: Chat,
    pub messages: Vec<ChatMessage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_chat() -> Chat {
        Chat::new("Test Chat".to_string(), "agent-123".to_string())
    }

    #[test]
    fn test_chat_creation() {
        let chat = sample_chat();

        assert!(!chat.id.is_empty());
        assert_eq!(chat.title, "Test Chat");
        assert_eq!(chat.agent_id, "agent-123");
        assert!(!chat.archived);
        assert!(chat.project_id.is_none());
    }

    #[test]
    fn test_chat_with_project() {
        let chat = Chat::with_project(
            "Test Chat".to_string(),
            "agent-123".to_string(),
            "project-456".to_string(),
        );

        assert_eq!(chat.project_id, Some("project-456".to_string()));
    }

    #[test]
    fn test_chat_validation_empty_title() {
        let chat = Chat::new("".to_string(), "agent-123".to_string());
        assert!(chat.validate().is_err());
    }

    #[test]
    fn test_chat_validation_title_too_long() {
        let mut chat = sample_chat();
        chat.title = "a".repeat(201);
        assert!(chat.validate().is_err());
    }

    #[test]
    fn test_chat_validation_empty_agent_id() {
        let chat = Chat::new("Title".to_string(), "".to_string());
        assert!(chat.validate().is_err());
    }

    #[test]
    fn test_chat_with_title() {
        let chat = sample_chat().with_title("New Title".to_string());
        assert_eq!(chat.title, "New Title");
    }

    #[test]
    fn test_chat_with_archived() {
        let chat = sample_chat().with_archived(true);
        assert!(chat.archived);
    }

    #[test]
    fn test_chat_with_project_id() {
        let chat = sample_chat().with_project_id(Some("proj-123".to_string()));
        assert_eq!(chat.project_id, Some("proj-123".to_string()));
    }

    #[test]
    fn test_message_creation_user() {
        let msg = ChatMessage::user("chat-123".to_string(), "Hello".to_string());
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello");
        assert!(!msg.has_images());
    }

    #[test]
    fn test_message_creation_user_with_images() {
        let msg = ChatMessage::user_with_images(
            "chat-123".to_string(),
            "Look at this".to_string(),
            vec!["/path/image.png".to_string()],
        );
        assert_eq!(msg.role, MessageRole::User);
        assert!(msg.has_images());
        assert_eq!(msg.images.len(), 1);
    }

    #[test]
    fn test_message_creation_assistant() {
        let msg = ChatMessage::assistant("chat-123".to_string(), "Hi there!".to_string());
        assert_eq!(msg.role, MessageRole::Assistant);
        assert_eq!(msg.content, "Hi there!");
    }

    #[test]
    fn test_message_creation_system() {
        let msg = ChatMessage::system("chat-123".to_string(), "Be helpful".to_string());
        assert_eq!(msg.role, MessageRole::System);
        assert_eq!(msg.content, "Be helpful");
    }

    #[test]
    fn test_message_validation_empty() {
        let msg = ChatMessage::user("chat-123".to_string(), "".to_string());
        assert!(msg.validate().is_err());
    }

    #[test]
    fn test_message_validation_too_long() {
        let mut msg = ChatMessage::user("chat-123".to_string(), "x".repeat(100_001));
        assert!(msg.validate().is_err());
    }

    #[test]
    fn test_message_with_usage() {
        let usage = MessageUsage {
            prompt_tokens: 10,
            completion_tokens: 20,
            total_tokens: 30,
        };
        let msg = ChatMessage::assistant("chat-123".to_string(), "Response".to_string())
            .with_usage(usage.clone());
        assert_eq!(msg.usage, Some(usage));
    }

    #[test]
    fn test_message_role_display() {
        assert_eq!(MessageRole::System.to_string(), "system");
        assert_eq!(MessageRole::User.to_string(), "user");
        assert_eq!(MessageRole::Assistant.to_string(), "assistant");
    }

    #[test]
    fn test_create_chat_request() {
        let req = CreateChatRequest {
            title: "New Chat".to_string(),
            agent_id: "agent-123".to_string(),
            project_id: Some("proj-456".to_string()),
        };

        let chat = req.into_chat();
        assert_eq!(chat.title, "New Chat");
        assert_eq!(chat.project_id, Some("proj-456".to_string()));
    }

    #[test]
    fn test_update_chat_request() {
        let mut chat = sample_chat();
        let req = UpdateChatRequest {
            title: Some("Updated".to_string()),
            archived: Some(true),
            ..Default::default()
        };

        req.apply_to(&mut chat);
        assert_eq!(chat.title, "Updated");
        assert!(chat.archived);
    }

    #[test]
    fn test_create_message_request() {
        let req = CreateMessageRequest {
            chat_id: "chat-123".to_string(),
            role: MessageRole::User,
            content: "Hello".to_string(),
            images: vec!["/img.png".to_string()],
            usage: None,
        };

        let msg = req.into_message();
        assert_eq!(msg.chat_id, "chat-123");
        assert_eq!(msg.content, "Hello");
        assert!(msg.has_images());
    }

    #[test]
    fn test_chat_summary_from_chat() {
        let chat = sample_chat();
        let summary = ChatSummary::from(chat.clone());

        assert_eq!(summary.id, chat.id);
        assert_eq!(summary.title, chat.title);
        assert_eq!(summary.message_count, 0);
    }
}
