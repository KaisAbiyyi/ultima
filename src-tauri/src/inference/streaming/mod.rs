//! Streaming module
//!
//! Handles SSE parsing and stream processing.

#![allow(dead_code)]
#![allow(unused_imports)]

mod sse_handler;

pub use sse_handler::SseHandler;
pub use sse_handler::StreamProcessor;
