//! Application configuration
//!
//! Centralized configuration constants and utilities.

/// Default port for the local llama-server
pub const DEFAULT_LLAMA_SERVER_PORT: u16 = 6661;

/// Default context window size in tokens
pub const DEFAULT_CONTEXT_WINDOW: u32 = 4096;

/// Server health check timeout in seconds
pub const SERVER_HEALTH_TIMEOUT_SECS: u64 = 60;

/// Server health check interval in milliseconds
pub const SERVER_HEALTH_INTERVAL_MS: u64 = 500;

/// Additional delay after health check passes (in seconds)
pub const POST_HEALTH_DELAY_SECS: u64 = 2;

/// Windows-specific binary name for llama-server
#[cfg(target_os = "windows")]
pub const LLAMA_SERVER_BINARY: &str = "llama-server-x86_64-pc-windows-msvc.exe";

/// Linux-specific binary name for llama-server
#[cfg(target_os = "linux")]
pub const LLAMA_SERVER_BINARY: &str = "llama-server-x86_64-unknown-linux-gnu";

/// macOS-specific binary name for llama-server
#[cfg(target_os = "macos")]
pub const LLAMA_SERVER_BINARY: &str = "llama-server-aarch64-apple-darwin";

/// Default fallback binary name (Windows)
#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
pub const LLAMA_SERVER_BINARY: &str = "llama-server";

/// Local server host address
pub const LOCAL_SERVER_HOST: &str = "127.0.0.1";

/// Get the local server base URL for a given port
pub fn local_server_url(port: u16) -> String {
    format!("http://{}:{}", LOCAL_SERVER_HOST, port)
}

/// Get the health check URL for a given port
pub fn health_check_url(port: u16) -> String {
    format!("{}/health", local_server_url(port))
}

/// Get the chat completions URL for a given port
pub fn chat_completions_url(port: u16) -> String {
    format!("{}/v1/chat/completions", local_server_url(port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_server_url() {
        assert_eq!(local_server_url(6661), "http://127.0.0.1:6661");
    }

    #[test]
    fn test_health_check_url() {
        assert_eq!(health_check_url(6661), "http://127.0.0.1:6661/health");
    }

    #[test]
    fn test_chat_completions_url() {
        assert_eq!(
            chat_completions_url(6661),
            "http://127.0.0.1:6661/v1/chat/completions"
        );
    }
}
