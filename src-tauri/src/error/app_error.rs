//! Application error types
//!
//! Defines all error variants used throughout the application.
//! Uses thiserror for ergonomic error definitions.

use serde::Serialize;
use thiserror::Error;

/// Result type alias using AppError
pub type AppResult<T> = Result<T, AppError>;

/// Main application error enum
///
/// Each variant represents a distinct error category
/// with appropriate context information.
#[derive(Error, Debug)]
pub enum AppError {
    // === Database Errors ===

    #[error("Database error: {0}")]
    Database(String),

    #[error("Record not found: {entity} with id {id}")]
    NotFound { entity: String, id: String },

    #[error("Duplicate record: {0}")]
    Duplicate(String),

    // === Inference Errors ===

    #[error("Inference error: {0}")]
    Inference(String),

    #[error("Model not found at path: {0}")]
    ModelNotFound(String),

    #[error("Provider error: {provider} - {message}")]
    Provider { provider: String, message: String },

    #[error("Sidecar process error: {0}")]
    Sidecar(String),

    // === HTTP Errors ===

    #[error("HTTP request failed: {0}")]
    Http(String),

    #[error("API key missing for provider: {0}")]
    ApiKeyMissing(String),

    #[error("Invalid API response: {0}")]
    InvalidResponse(String),

    // === Streaming Errors ===

    #[error("Stream error: {0}")]
    Stream(String),

    #[error("SSE parse error: {0}")]
    SseParse(String),

    // === Validation Errors ===

    #[error("Validation error: {field} - {message}")]
    Validation { field: String, message: String },

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    // === System Errors ===

    #[error("IO error: {0}")]
    Io(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Serializable error for Tauri frontend
///
/// Converts AppError to a frontend-friendly format
#[derive(Serialize, Debug)]
pub struct ErrorResponse {
    pub code: String,
    pub message: String,
    pub details: Option<String>,
}

impl From<AppError> for ErrorResponse {
    fn from(error: AppError) -> Self {
        let code = match &error {
            AppError::Database(_) => "DATABASE_ERROR",
            AppError::NotFound { .. } => "NOT_FOUND",
            AppError::Duplicate(_) => "DUPLICATE",
            AppError::Inference(_) => "INFERENCE_ERROR",
            AppError::ModelNotFound(_) => "MODEL_NOT_FOUND",
            AppError::Provider { .. } => "PROVIDER_ERROR",
            AppError::Sidecar(_) => "SIDECAR_ERROR",
            AppError::Http(_) => "HTTP_ERROR",
            AppError::ApiKeyMissing(_) => "API_KEY_MISSING",
            AppError::InvalidResponse(_) => "INVALID_RESPONSE",
            AppError::Stream(_) => "STREAM_ERROR",
            AppError::SseParse(_) => "SSE_PARSE_ERROR",
            AppError::Validation { .. } => "VALIDATION_ERROR",
            AppError::InvalidConfig(_) => "INVALID_CONFIG",
            AppError::Io(_) => "IO_ERROR",
            AppError::Serialization(_) => "SERIALIZATION_ERROR",
            AppError::Internal(_) => "INTERNAL_ERROR",
        };

        ErrorResponse {
            code: code.to_string(),
            message: error.to_string(),
            details: None,
        }
    }
}

// === Conversion Implementations ===

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::Serialization(err.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        AppError::Http(err.to_string())
    }
}

impl From<tauri::Error> for AppError {
    fn from(err: tauri::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

// Implement Serialize for Tauri command returns
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let response = ErrorResponse::from(self.clone());
        response.serialize(serializer)
    }
}

impl Clone for AppError {
    fn clone(&self) -> Self {
        match self {
            AppError::Database(s) => AppError::Database(s.clone()),
            AppError::NotFound { entity, id } => AppError::NotFound {
                entity: entity.clone(),
                id: id.clone(),
            },
            AppError::Duplicate(s) => AppError::Duplicate(s.clone()),
            AppError::Inference(s) => AppError::Inference(s.clone()),
            AppError::ModelNotFound(s) => AppError::ModelNotFound(s.clone()),
            AppError::Provider { provider, message } => AppError::Provider {
                provider: provider.clone(),
                message: message.clone(),
            },
            AppError::Sidecar(s) => AppError::Sidecar(s.clone()),
            AppError::Http(s) => AppError::Http(s.clone()),
            AppError::ApiKeyMissing(s) => AppError::ApiKeyMissing(s.clone()),
            AppError::InvalidResponse(s) => AppError::InvalidResponse(s.clone()),
            AppError::Stream(s) => AppError::Stream(s.clone()),
            AppError::SseParse(s) => AppError::SseParse(s.clone()),
            AppError::Validation { field, message } => AppError::Validation {
                field: field.clone(),
                message: message.clone(),
            },
            AppError::InvalidConfig(s) => AppError::InvalidConfig(s.clone()),
            AppError::Io(s) => AppError::Io(s.clone()),
            AppError::Serialization(s) => AppError::Serialization(s.clone()),
            AppError::Internal(s) => AppError::Internal(s.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = AppError::NotFound {
            entity: "Agent".to_string(),
            id: "123".to_string(),
        };
        assert_eq!(err.to_string(), "Record not found: Agent with id 123");
    }

    #[test]
    fn test_error_response_conversion() {
        let err = AppError::Database("connection failed".to_string());
        let response: ErrorResponse = err.into();
        assert_eq!(response.code, "DATABASE_ERROR");
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(
            ErrorResponse::from(AppError::NotFound {
                entity: "".to_string(),
                id: "".to_string()
            })
            .code,
            "NOT_FOUND"
        );
        assert_eq!(
            ErrorResponse::from(AppError::Database("".to_string())).code,
            "DATABASE_ERROR"
        );
        assert_eq!(
            ErrorResponse::from(AppError::Inference("".to_string())).code,
            "INFERENCE_ERROR"
        );
    }

    #[test]
    fn test_validation_error() {
        let err = AppError::Validation {
            field: "name".to_string(),
            message: "is required".to_string(),
        };
        let response: ErrorResponse = err.into();
        assert_eq!(response.code, "VALIDATION_ERROR");
        assert!(response.message.contains("name"));
        assert!(response.message.contains("is required"));
    }

    #[test]
    fn test_error_clone() {
        let err1 = AppError::Database("test".to_string());
        let err2 = err1.clone();
        assert_eq!(err1.to_string(), err2.to_string());
    }
}
