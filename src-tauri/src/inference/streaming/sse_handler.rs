//! SSE handler for streaming responses
//!
//! Parses Server-Sent Events from AI providers.

use futures::StreamExt;
use reqwest::Response;
use tauri::{AppHandle, Emitter};

use crate::error::{AppError, AppResult};
use crate::events::{
    InferenceEvent,
    StreamCompletePayload,
    StreamErrorPayload,
    StreamTokenPayload,
};
use crate::models::StreamChunk;

/// SSE handler for processing stream responses
pub struct SseHandler {
    /// Request ID for correlation
    request_id: String,
    /// Agent ID
    agent_id: String,
}

impl SseHandler {
    /// Create new SSE handler
    pub fn new(request_id: impl Into<String>, agent_id: impl Into<String>) -> Self {
        Self {
            request_id: request_id.into(),
            agent_id: agent_id.into(),
        }
    }

    /// Process streaming response
    pub async fn process_stream(
        &self,
        response: Response,
        app_handle: &AppHandle,
    ) -> AppResult<String> {
        let mut stream = response.bytes_stream();
        let mut full_content = String::new();
        let mut token_index: u32 = 0;
        let mut buffer = String::new();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| {
                AppError::Stream(format!("Stream read error: {}", e))
            })?;

            // Append to buffer
            buffer.push_str(&String::from_utf8_lossy(&bytes));

            // Process complete SSE events in buffer
            while let Some(event_end) = buffer.find("\n\n") {
                let event_data = buffer[..event_end].to_string();
                buffer = buffer[event_end + 2..].to_string();

                // Parse and emit each event
                if let Some(content) = self.parse_sse_event(&event_data)? {
                    if content == "[DONE]" {
                        continue;
                    }

                    full_content.push_str(&content);

                    // Emit token event
                    let payload = StreamTokenPayload::new(
                        &self.request_id,
                        &self.agent_id,
                        &content,
                        token_index,
                    );

                    app_handle
                        .emit(InferenceEvent::STREAM_TOKEN, &payload)
                        .map_err(|e| AppError::Internal(format!(
                            "Failed to emit event: {}", e
                        )))?;

                    token_index += 1;
                }
            }
        }

        // Emit completion
        let complete_payload = StreamCompletePayload::new(
            &self.request_id,
            &self.agent_id,
            &full_content,
        );

        app_handle
            .emit(InferenceEvent::STREAM_COMPLETE, &complete_payload)
            .map_err(|e| AppError::Internal(format!(
                "Failed to emit completion: {}", e
            )))?;

        Ok(full_content)
    }

    /// Parse a single SSE event
    fn parse_sse_event(&self, event: &str) -> AppResult<Option<String>> {
        for line in event.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                let data = data.trim();

                // Check for stream end
                if data == "[DONE]" {
                    return Ok(Some("[DONE]".to_string()));
                }

                // Parse JSON chunk
                match serde_json::from_str::<StreamChunk>(data) {
                    Ok(chunk) => {
                        if let Some(content) = chunk.content() {
                            return Ok(Some(content.to_string()));
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to parse chunk: {} - {}", data, e);
                    }
                }
            }
        }

        Ok(None)
    }

    /// Emit error event
    pub fn emit_error(
        &self,
        app_handle: &AppHandle,
        code: &str,
        message: &str,
    ) -> AppResult<()> {
        let payload = StreamErrorPayload::new(
            &self.request_id,
            &self.agent_id,
            code,
            message,
        );

        app_handle
            .emit(InferenceEvent::STREAM_ERROR, &payload)
            .map_err(|e| AppError::Internal(format!(
                "Failed to emit error: {}", e
            )))
    }
}

/// High-level stream processor
pub struct StreamProcessor;

impl StreamProcessor {
    /// Process stream with automatic error handling
    pub async fn process(
        response: Response,
        request_id: &str,
        agent_id: &str,
        app_handle: &AppHandle,
    ) -> AppResult<String> {
        let handler = SseHandler::new(request_id, agent_id);

        match handler.process_stream(response, app_handle).await {
            Ok(content) => Ok(content),
            Err(e) => {
                handler.emit_error(
                    app_handle,
                    "STREAM_ERROR",
                    &e.to_string(),
                )?;
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handler_creation() {
        let handler = SseHandler::new("req-123", "agent-456");
        assert_eq!(handler.request_id, "req-123");
        assert_eq!(handler.agent_id, "agent-456");
    }

    #[test]
    fn test_parse_done() {
        let handler = SseHandler::new("req", "agent");
        let result = handler.parse_sse_event("data: [DONE]").unwrap();
        assert_eq!(result, Some("[DONE]".to_string()));
    }
}
