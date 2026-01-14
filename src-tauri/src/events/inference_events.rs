//! Inference-related event payloads
//!
//! Structures for streaming and inference status events.

use serde::{Deserialize, Serialize};

use crate::models::TokenUsage;

// === Event Names ===

/// Event name constants for type safety
pub struct InferenceEvent;

impl InferenceEvent {
    /// New token received in stream
    pub const STREAM_TOKEN: &'static str = "inference:stream-token";

    /// Stream completed successfully
    pub const STREAM_COMPLETE: &'static str = "inference:stream-complete";

    /// Stream error occurred
    pub const STREAM_ERROR: &'static str = "inference:stream-error";

    /// Inference started
    pub const STARTED: &'static str = "inference:started";

    /// Inference cancelled
    pub const CANCELLED: &'static str = "inference:cancelled";

    /// Sidecar status changed
    pub const SIDECAR_STATUS: &'static str = "inference:sidecar-status";
}

// === Payload Types ===

/// Wrapper for all inference events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum InferenceEventPayload {
    /// New token in stream
    StreamToken(StreamTokenPayload),

    /// Stream completed
    StreamComplete(StreamCompletePayload),

    /// Stream error
    StreamError(StreamErrorPayload),

    /// Inference started
    Started(InferenceStartedPayload),

    /// Inference cancelled
    Cancelled(InferenceCancelledPayload),

    /// Sidecar status
    SidecarStatus(SidecarStatusPayload),
}

/// Payload for stream token events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamTokenPayload {
    /// Request ID for correlation
    pub request_id: String,

    /// Agent ID processing the request
    pub agent_id: String,

    /// Token content
    pub token: String,

    /// Token index in stream
    pub index: u32,
}

impl StreamTokenPayload {
    /// Create new stream token payload
    pub fn new(
        request_id: impl Into<String>,
        agent_id: impl Into<String>,
        token: impl Into<String>,
        index: u32,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            agent_id: agent_id.into(),
            token: token.into(),
            index,
        }
    }
}

/// Payload for stream completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamCompletePayload {
    /// Request ID
    pub request_id: String,

    /// Agent ID
    pub agent_id: String,

    /// Full response content
    pub content: String,

    /// Finish reason (stop, length, etc.)
    pub finish_reason: Option<String>,

    /// Token usage statistics
    pub usage: Option<TokenUsage>,
}

impl StreamCompletePayload {
    /// Create completion payload
    pub fn new(
        request_id: impl Into<String>,
        agent_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            agent_id: agent_id.into(),
            content: content.into(),
            finish_reason: None,
            usage: None,
        }
    }

    /// Set finish reason
    pub fn with_finish_reason(mut self, reason: impl Into<String>) -> Self {
        self.finish_reason = Some(reason.into());
        self
    }

    /// Set usage statistics
    pub fn with_usage(mut self, usage: TokenUsage) -> Self {
        self.usage = Some(usage);
        self
    }
}

/// Payload for stream errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamErrorPayload {
    /// Request ID
    pub request_id: String,

    /// Agent ID
    pub agent_id: String,

    /// Error code
    pub code: String,

    /// Error message
    pub message: String,

    /// Whether request can be retried
    pub retryable: bool,
}

impl StreamErrorPayload {
    /// Create error payload
    pub fn new(
        request_id: impl Into<String>,
        agent_id: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            agent_id: agent_id.into(),
            code: code.into(),
            message: message.into(),
            retryable: false,
        }
    }

    /// Mark as retryable
    pub fn retryable(mut self) -> Self {
        self.retryable = true;
        self
    }
}

/// Payload for inference started event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceStartedPayload {
    pub request_id: String,
    pub agent_id: String,
    pub model: String,
}

/// Payload for inference cancelled event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceCancelledPayload {
    pub request_id: String,
    pub agent_id: String,
    pub reason: String,
}

// === Sidecar Events ===

/// Sidecar process status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SidecarStatus {
    /// Starting up
    Starting,
    /// Running and ready
    Running,
    /// Stopped
    Stopped,
    /// Error state
    Error,
}

/// Payload for sidecar status changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarStatusPayload {
    /// Current status
    pub status: SidecarStatus,

    /// Model path being served (if running)
    pub model_path: Option<String>,

    /// Process ID (if running)
    pub pid: Option<u32>,

    /// Error message (if error)
    pub error: Option<String>,

    /// Port number
    pub port: u16,
}

impl SidecarStatusPayload {
    /// Create starting status
    pub fn starting(port: u16) -> Self {
        Self {
            status: SidecarStatus::Starting,
            model_path: None,
            pid: None,
            error: None,
            port,
        }
    }

    /// Create running status
    pub fn running(model_path: String, pid: u32, port: u16) -> Self {
        Self {
            status: SidecarStatus::Running,
            model_path: Some(model_path),
            pid: Some(pid),
            error: None,
            port,
        }
    }

    /// Create stopped status
    pub fn stopped(port: u16) -> Self {
        Self {
            status: SidecarStatus::Stopped,
            model_path: None,
            pid: None,
            error: None,
            port,
        }
    }

    /// Create error status
    pub fn error(message: String, port: u16) -> Self {
        Self {
            status: SidecarStatus::Error,
            model_path: None,
            pid: None,
            error: Some(message),
            port,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_token_payload() {
        let payload = StreamTokenPayload::new("req-1", "agent-1", "Hello", 0);
        assert_eq!(payload.token, "Hello");
        assert_eq!(payload.index, 0);
    }

    #[test]
    fn test_sidecar_status() {
        let running = SidecarStatusPayload::running("/path/model.gguf".to_string(), 1234, 6661);
        assert_eq!(running.status, SidecarStatus::Running);
        assert_eq!(running.pid, Some(1234));
    }
}
