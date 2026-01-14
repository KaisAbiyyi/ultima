//! Inference module for Ultimate AI
//!
//! Handles AI inference via local and external providers.

#![allow(dead_code)]
#![allow(unused_imports)]

mod http_client;

pub mod sidecar_manager;
pub mod providers;
pub mod streaming;

pub use http_client::HttpClient;
pub use http_client::HttpClientConfig;
pub use sidecar_manager::{SidecarHandle, SidecarManager, get_sidecar, init_sidecar};
