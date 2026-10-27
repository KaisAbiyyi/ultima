//! NC-10B: bounded, content-free timing at the native OpenGL paint callback.
//! This callback precedes buffer swap/compositor scanout. Independent Windows
//! pixel/UIA evidence is still required for displayed-frame acceptance.
use serde_json::{json, Value};
use std::collections::hash_map::RandomState;
use std::collections::VecDeque;
use std::hash::{BuildHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const MAX_SAMPLES: usize = 4096;

#[derive(Clone, Copy)]
pub enum Kind {
    Stream,
    Keyboard,
    KeyboardDuringTurn,
    Startup,
}

#[derive(Default)]
struct Series {
    values_ms: VecDeque<f64>,
    rejected: u64,
}

impl Series {
    fn record(&mut self, elapsed: Duration) {
        if self.values_ms.len() < MAX_SAMPLES {
            self.values_ms.push_back(elapsed.as_secs_f64() * 1000.0);
        } else {
            // A truncated series is not eligible for an acceptance verdict.
            self.rejected = self.rejected.saturating_add(1);
        }
    }

    fn json(&self) -> Value {
        let mut sorted: Vec<f64> = self.values_ms.iter().copied().collect();
        sorted.sort_by(f64::total_cmp);
        let percentile = |p: f64| {
            let rank = ((sorted.len() as f64 * p).ceil() as usize).saturating_sub(1);
            sorted.get(rank).copied()
        };
        json!({
            "samples": sorted.len(),
            "overflow": self.rejected,
            "p50Ms": percentile(0.50),
            "p95Ms": percentile(0.95),
            "maxMs": sorted.last(),
        })
    }
}

#[derive(Default)]
struct Samples {
    stream: Series,
    keyboard: Series,
    keyboard_during_turn: Series,
    startup: Series,
    coalesced_stream_events: u64,
}

#[derive(Clone)]
pub struct LatencyRecorder {
    samples: Arc<Mutex<Samples>>,
    started_at: Instant,
    session_nonce: String,
    receipt: Option<PathBuf>,
}

// A fresh, opaque per-process token prevents accidentally counting a copied
// receipt twice as two independent repetitions. It contains no prompt, path,
// user, or process identifier, and it is not a cryptographic attestation.
fn session_nonce() -> String {
    let nonce_part = || {
        let mut hasher = RandomState::new().build_hasher();
        Instant::now().hash(&mut hasher);
        std::process::id().hash(&mut hasher);
        hasher.finish()
    };
    format!("{:016x}{:016x}", nonce_part(), nonce_part())
}

impl LatencyRecorder {
    pub fn new(started_at: Instant, receipt: Option<PathBuf>) -> Self {
        Self {
            samples: Arc::new(Mutex::new(Samples::default())),
            started_at,
            session_nonce: session_nonce(),
            receipt,
        }
    }

    pub fn started_at(&self) -> Instant {
        self.started_at
    }

    pub fn record_paint(&self, kind: Kind, started: Instant, coalesced: u32) {
        if let Ok(mut samples) = self.samples.lock() {
            let elapsed = Instant::now().saturating_duration_since(started);
            match kind {
                Kind::Stream => {
                    samples.stream.record(elapsed);
                    samples.coalesced_stream_events = samples
                        .coalesced_stream_events
                        .saturating_add(u64::from(coalesced));
                }
                Kind::Keyboard => samples.keyboard.record(elapsed),
                Kind::KeyboardDuringTurn => samples.keyboard_during_turn.record(elapsed),
                Kind::Startup => samples.startup.record(elapsed),
            }
        }
    }

    fn report(&self) -> Value {
        let Ok(samples) = self.samples.lock() else {
            return json!({"kind":"NC10B_GLOW_PAINT_DIAGNOSTIC","error":"telemetry lock unavailable"});
        };
        json!({
            "kind": "NC10B_GLOW_PAINT_DIAGNOSTIC",
            "version": 1,
            "sessionNonce": self.session_nonce,
            // Embedded at compile time by the SHA-bound exact-build helper.
            // Ordinary cargo builds remain UNBOUND and cannot qualify.
            "sourceSha": option_env!("NC10B_BUILD_SOURCE_SHA").unwrap_or("UNBOUND"),
            "stage": "OpenGL callback before buffer swap; not compositor presentation",
            "presentationVerified": false,
            "clock": "monotonic Instant",
            "limitPerSeries": MAX_SAMPLES,
            "streamCoalescedEvents": samples.coalesced_stream_events,
            "streamReceivedToPaintCallback": samples.stream.json(),
            "rawKeyToComposerPaintCallback": samples.keyboard.json(),
            "rawKeyDuringActiveTurnToPaintCallback": samples.keyboard_during_turn.json(),
            "processMainToFirstComposerPaintCallback": samples.startup.json(),
            "acceptance": "NOT_RUN: independent Windows screen-presentation receipt required",
        })
    }

    pub fn save_on_exit(&self) {
        let Some(path) = &self.receipt else {
            return;
        };
        if let Err(error) = write_atomic(path, &self.report()) {
            eprintln!("NC-10B receipt unavailable: {error}");
        }
    }
}

fn write_atomic(path: &Path, value: &Value) -> std::io::Result<()> {
    let tmp = path.with_extension("nc10b-partial");
    // Do not create directories or silently overwrite an earlier receipt.
    let mut data = serde_json::to_vec_pretty(value)?;
    data.push(b'\n');
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)?;
    file.write_all(&data)?;
    file.sync_all()?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_nearest_rank_and_overflow_are_fail_closed() {
        let mut series = Series::default();
        for value in [10, 20, 30, 40, 50, 60, 70, 80, 90, 100] {
            series.record(Duration::from_millis(value));
        }
        assert_eq!(series.json()["p50Ms"], 50.0);
        assert_eq!(series.json()["p95Ms"], 100.0);
        for _ in 0..MAX_SAMPLES {
            series.record(Duration::ZERO);
        }
        assert!(series.rejected > 0);
    }

    #[test]
    fn absent_visual_proof_never_claims_pass() {
        let recorder = LatencyRecorder::new(Instant::now(), None);
        let receipt = recorder.report();
        assert_eq!(receipt["presentationVerified"], false);
        assert_eq!(receipt["streamReceivedToPaintCallback"]["samples"], 0);
        assert!(receipt["acceptance"].as_str().unwrap().contains("NOT_RUN"));
        recorder.record_paint(Kind::Stream, Instant::now(), 4);
        let receipt = recorder.report();
        assert_eq!(receipt["streamReceivedToPaintCallback"]["samples"], 1);
        assert_eq!(receipt["streamCoalescedEvents"], 4);
        assert_eq!(receipt["presentationVerified"], false);
    }

    #[test]
    fn idle_keyboard_input_cannot_populate_active_turn_samples() {
        let recorder = LatencyRecorder::new(Instant::now(), None);
        recorder.record_paint(Kind::Keyboard, Instant::now(), 0);
        assert_eq!(
            recorder.report()["rawKeyToComposerPaintCallback"]["samples"],
            1
        );
        assert_eq!(
            recorder.report()["rawKeyDuringActiveTurnToPaintCallback"]["samples"],
            0
        );
        recorder.record_paint(Kind::KeyboardDuringTurn, Instant::now(), 0);
        assert_eq!(
            recorder.report()["rawKeyDuringActiveTurnToPaintCallback"]["samples"],
            1
        );
    }

    #[test]
    fn independent_recorder_instances_have_distinct_opaque_session_nonces() {
        let first = LatencyRecorder::new(Instant::now(), None);
        let second = LatencyRecorder::new(Instant::now(), None);
        let a = first.report()["sessionNonce"].as_str().unwrap().to_owned();
        let b = second.report()["sessionNonce"].as_str().unwrap().to_owned();
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
