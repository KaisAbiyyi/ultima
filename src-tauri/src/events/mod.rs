//! Tauri event definitions
//!
//! Defines all events emitted to the frontend.

#![allow(dead_code)]
#![allow(unused_imports)]

mod inference_events;

pub use inference_events::InferenceEvent;
pub use inference_events::InferenceEventPayload;
pub use inference_events::StreamTokenPayload;
pub use inference_events::StreamCompletePayload;
pub use inference_events::StreamErrorPayload;
pub use inference_events::SidecarStatusPayload;
pub use inference_events::SidecarStatus;
pub use inference_events::InferenceStartedPayload;
pub use inference_events::InferenceCancelledPayload;
