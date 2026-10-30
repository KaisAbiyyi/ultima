//! Opt-in official Codex App Server client for the native-renderer experiment.
//! Transport, JSONL protocol, process-tree teardown and approval policy are
//! owned by the production Rust crates. There are no automatic tool approvals.
use crate::native_tools::{ToolEvidence, ToolLedger};
use crate::session_history::{self, HistoryRow, ThreadEntry};
use eframe::egui;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};
use ultima_app_server_supervisor::{EngineSupervisor, OutputEvent};
use ultima_native_session::SessionGate;
use ultima_protocol_bridge::{
    Approval, ApprovalDecision, Broker, ClientMethod, Event, Limits, NotificationKind, Phase, RpcId,
};

const MAX_PROMPT_BYTES: usize = 8 * 1024;
const MAX_ASSISTANT_BYTES: usize = 256 * 1024;
const MAX_UI_EVENTS: usize = 128;
const MAX_UI_EVENT_BYTES: usize = 512 * 1024;
const MAX_COMMANDS: usize = 8;
const RPC_TIMEOUT: Duration = Duration::from_secs(30);
const CANCEL_TIMEOUT: Duration = Duration::from_secs(20);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);
const HISTORY_TIMEOUT: Duration = Duration::from_secs(30);
const RESUMED_TURN_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ProbeStatus {
    Connecting = 0,
    Ready = 1,
    Failed = 2,
    Stopped = 3,
}

/// Provenance is attached to every piece of assistant text, including the
/// canonical completed message (which can arrive without any deltas).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProbeEvent {
    ThreadReady {
        thread_id: String,
    },
    ThreadPage {
        entries: Vec<ThreadEntry>,
        append: bool,
        has_more: bool,
    },
    ThreadSelected {
        thread_id: Option<String>,
    },
    HistoryPage {
        thread_id: String,
        rows: Vec<HistoryRow>,
        prepend: bool,
        has_more: bool,
        active_turn: Option<String>,
    },
    HistoryCapabilityBlocked(&'static str),
    TurnCancelledBeforeStart,
    TurnStarted {
        thread_id: String,
        turn_id: String,
    },
    AssistantDelta {
        thread_id: String,
        turn_id: String,
        text: String,
    },
    AssistantFinal {
        thread_id: String,
        turn_id: String,
        text: String,
    },
    TurnCompleted {
        thread_id: String,
        turn_id: String,
        status: String,
    },
    ToolSnapshot(ToolEvidence),
    ApprovalPending(Approval),
    ApprovalResolved(RpcId),
    Error(String),
}

impl ProbeEvent {
    fn bytes(&self) -> usize {
        match self {
            Self::AssistantDelta { text, .. } | Self::AssistantFinal { text, .. } => {
                text.len() + 512
            }
            Self::Error(message) => message.len() + 512,
            Self::ToolSnapshot(item) => {
                item.command.len() + item.output.len() + item.diff.len() + 1024
            }
            Self::ThreadPage { entries, .. } => {
                entries
                    .iter()
                    .map(|e| e.label.len() + e.id.len() + 128)
                    .sum::<usize>()
                    + 512
            }
            Self::HistoryPage { rows, .. } => {
                rows.iter().map(|r| r.preview.len() + 128).sum::<usize>() + 512
            }
            _ => 512,
        }
    }
}

#[derive(Default)]
struct DisplayEvents {
    queue: VecDeque<TimedProbeEvent>,
    bytes: usize,
}

/// Timestamp at admission into the bounded native UI mailbox. Never carries
/// prompt content to the latency recorder; only the original UI event does.
#[derive(Clone, Debug)]
pub struct TimedProbeEvent {
    pub event: ProbeEvent,
    pub received_at: Instant,
    pub coalesced: u32,
}

impl DisplayEvents {
    fn push(&mut self, event: ProbeEvent) -> Result<(), &'static str> {
        self.push_at(event, Instant::now())
    }

    fn push_at(&mut self, event: ProbeEvent, received_at: Instant) -> Result<(), &'static str> {
        // A canonical final supersedes any still-undelivered stream for the
        // same turn. Avoid retaining two large copies of one assistant reply.
        if let ProbeEvent::AssistantFinal {
            thread_id, turn_id, ..
        } = &event
        {
            if matches!(self.queue.back().map(|stamped| &stamped.event), Some(ProbeEvent::AssistantDelta {
                thread_id: previous_thread, turn_id: previous_turn, ..
            }) if previous_thread == thread_id && previous_turn == turn_id)
            {
                if let Some(previous) = self.queue.pop_back() {
                    self.bytes -= previous.event.bytes();
                }
            }
        }
        // Coalesce consecutive stream frames, reducing UI churn and queue use.
        if let ProbeEvent::AssistantDelta {
            thread_id,
            turn_id,
            text,
        } = &event
        {
            if let Some(previous) = self.queue.back_mut() {
                if let ProbeEvent::AssistantDelta {
                    thread_id: previous_thread,
                    turn_id: previous_turn,
                    text: previous_text,
                } = &mut previous.event
                {
                    if previous_thread == thread_id && previous_turn == turn_id {
                        let bytes = text.len();
                        if self
                            .bytes
                            .checked_add(bytes)
                            .is_none_or(|total| total > MAX_UI_EVENT_BYTES)
                        {
                            return Err("Native stream display buffer exceeded its limit.");
                        }
                        previous_text.push_str(text);
                        self.bytes += bytes;
                        previous.coalesced = previous.coalesced.saturating_add(1);
                        return Ok(());
                    }
                }
            }
        }
        let bytes = event.bytes();
        if self.queue.len() >= MAX_UI_EVENTS
            || self
                .bytes
                .checked_add(bytes)
                .is_none_or(|total| total > MAX_UI_EVENT_BYTES)
        {
            return Err("Native stream display buffer exceeded its limit.");
        }
        self.bytes += bytes;
        self.queue.push_back(TimedProbeEvent {
            event,
            received_at,
            coalesced: 0,
        });
        Ok(())
    }

    fn fatal(&mut self, error: &'static str) {
        // Terminal errors are always visible even when the normal queue filled.
        self.queue.clear();
        self.bytes = 0;
        let _ = self.push(ProbeEvent::Error(error.to_owned()));
    }

    #[cfg(test)]
    fn take(&mut self) -> Vec<ProbeEvent> {
        self.take_timed()
            .into_iter()
            .map(|stamped| stamped.event)
            .collect()
    }

    fn take_timed(&mut self) -> Vec<TimedProbeEvent> {
        self.bytes = 0;
        self.queue.drain(..).collect()
    }
}

enum Command {
    Submit(String),
    Cancel,
    Decide(Approval, bool, Option<Box<ToolEvidence>>),
    NewThread,
    SelectThread(String),
    MoreThreads,
    MoreHistory,
}

pub struct EngineProbe {
    status: Arc<AtomicU8>,
    commands: mpsc::Sender<Command>,
    emergency_stop: Arc<AtomicBool>,
    events: Arc<Mutex<DisplayEvents>>,
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

fn notify(status: &AtomicU8, ctx: &egui::Context, state: ProbeStatus) {
    status.store(state as u8, Ordering::Release);
    ctx.request_repaint();
}

fn enqueue(
    events: &Mutex<DisplayEvents>,
    ctx: &egui::Context,
    event: ProbeEvent,
) -> Result<(), &'static str> {
    events
        .lock()
        .map_err(|_| "Native display state unavailable.")?
        .push(event)?;
    ctx.request_repaint();
    Ok(())
}

fn safe_id(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 256)
}

fn thread_id(value: &Value) -> Option<&str> {
    safe_id(
        value
            .pointer("/thread/id")
            .or_else(|| value.get("threadId")),
    )
}

fn turn_id(value: &Value) -> Option<&str> {
    safe_id(value.pointer("/turn/id").or_else(|| value.get("turnId")))
}

/// Codex 0.164.0 ThreadStartResponse includes the *effective* permissions.
/// Refuse to send any prompt if the server did not apply the requested policy.
fn accepted_read_only_thread(value: &Value, project: &Path) -> Result<String, &'static str> {
    if value.get("approvalPolicy").and_then(Value::as_str) != Some("never")
        || value.pointer("/sandbox/type").and_then(Value::as_str) != Some("readOnly")
        || value
            .pointer("/sandbox/networkAccess")
            .and_then(Value::as_bool)
            != Some(false)
    {
        return Err("Codex did not confirm read-only, offline, never-approve permissions.");
    }
    let cwd = value
        .get("cwd")
        .and_then(Value::as_str)
        .ok_or("Codex did not confirm the project directory.")?;
    let expected = project
        .canonicalize()
        .map_err(|_| "Native project directory is no longer available.")?;
    let actual = Path::new(cwd)
        .canonicalize()
        .map_err(|_| "Codex returned an invalid project directory.")?;
    if expected != actual {
        return Err("Codex switched to an unexpected project directory.");
    }
    let created = thread_id(value).ok_or("Codex returned an invalid thread identity.")?;
    if value
        .pointer("/thread/cwd")
        .and_then(Value::as_str)
        .and_then(|path| Path::new(path).canonicalize().ok())
        .as_ref()
        != Some(&expected)
    {
        return Err("Codex thread project does not match the selected project.");
    }
    Ok(created.to_owned())
}

fn accepted_thread(value: &Value, project: &Path, coding: bool) -> Result<String, &'static str> {
    if !coding {
        return accepted_read_only_thread(value, project);
    }
    if value.get("approvalPolicy").and_then(Value::as_str) != Some("on-request")
        || value.pointer("/sandbox/type").and_then(Value::as_str) != Some("workspaceWrite")
        || value
            .pointer("/sandbox/networkAccess")
            .and_then(Value::as_bool)
            != Some(false)
    {
        return Err("Codex did not confirm restricted, on-request coding permissions.");
    }
    let expected = project
        .canonicalize()
        .map_err(|_| "Native project is unavailable.")?;
    let actual = value
        .get("cwd")
        .and_then(Value::as_str)
        .and_then(|p| Path::new(p).canonicalize().ok())
        .ok_or("Codex did not confirm the project directory.")?;
    if expected != actual
        || value
            .pointer("/thread/cwd")
            .and_then(Value::as_str)
            .and_then(|p| Path::new(p).canonicalize().ok())
            .as_ref()
            != Some(&expected)
    {
        return Err("Codex switched to an unexpected project directory.");
    }
    Ok(thread_id(value)
        .ok_or("Invalid thread identity.")?
        .to_owned())
}

fn failed_turn_message(turn: &Value) -> &'static str {
    let info = turn.pointer("/error/codexErrorInfo");
    let kind = info
        .and_then(Value::as_str)
        .or_else(|| {
            info.and_then(Value::as_object)
                .and_then(|object| object.keys().next().map(String::as_str))
        })
        .unwrap_or("");
    match kind {
        "usageLimitExceeded" | "rateLimitExceeded" | "sessionBudgetExceeded" => {
            return "Codex reached its upstream usage or rate limit."
        }
        "unauthorized" => return "Codex authorization failed. Reconnect the account.",
        "httpConnectionFailed"
        | "responseStreamConnectionFailed"
        | "responseStreamDisconnected"
        | "responseTooManyFailedAttempts" => return "Codex lost its upstream connection.",
        _ => {}
    }
    // Some official providers return only a message. Classify its text but
    // never forward arbitrary upstream diagnostics or account details.
    let detail = turn
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("");
    let detail = detail
        .chars()
        .take(512)
        .collect::<String>()
        .to_ascii_lowercase();
    if [
        "usage limit",
        "rate limit",
        "quota exceeded",
        "reached your limit",
    ]
    .iter()
    .any(|label| detail.contains(label))
    {
        "Codex reached its upstream usage or rate limit."
    } else if detail.contains("unauthorized") || detail.contains("authentication required") {
        "Codex authorization failed. Reconnect the account."
    } else {
        "Codex reported a failed turn without a completed response."
    }
}

/// The UI has at most one turn in flight, bound to one official thread.
/// In-flight request IDs are matched against the Broker's typed replies;
/// notifications and content require matching thread AND turn IDs.
struct UnverifiedCompletion {
    turn_id: String,
    status: &'static str,
    error: Option<&'static str>,
}

impl UnverifiedCompletion {
    fn from_notification(params: &Value) -> Result<Self, &'static str> {
        let id = turn_id(params).ok_or("Codex turn completed without a valid turn ID.")?;
        let turn = params
            .get("turn")
            .ok_or("Codex turn completion omitted its turn.")?;
        let status = match turn.get("status").and_then(Value::as_str) {
            Some("completed") => "completed",
            Some("interrupted") => "interrupted",
            Some("failed") => "failed",
            _ => return Err("Codex returned an unknown final turn status."),
        };
        Ok(Self {
            turn_id: id.to_owned(),
            status,
            error: (status == "failed").then(|| failed_turn_message(turn)),
        })
    }
}

#[derive(Default)]
struct TurnLifecycle {
    thread: Option<String>,
    project: Option<PathBuf>,
    thread_request: Option<u64>,
    turn_request: Option<u64>,
    interrupt_request: Option<u64>,
    interrupt_sent: bool,
    turn: Option<String>,
    pending_prompt: Option<String>,
    busy: bool,
    cancel_requested: bool,
    text_bytes: usize,
    assistant_item: Option<String>,
    terminal_error: Option<&'static str>,
    rpc_started: Option<Instant>,
    cancel_started: Option<Instant>,
    // A terminal notification can overtake the start RPC reply and even
    // turn/started. Never expose it until a separate official start identity
    // confirms the exact same turn; this holds only one <=256-byte ID.
    unverified_completion: Option<UnverifiedCompletion>,
    coding_enabled: bool,
    tools: ToolLedger,
    resumed_turn_started: Option<Instant>,
    // A successful resume is not execution-safe until the FIRST official
    // turns page proves whether the resumed thread already has active work.
    // Persist this gate even if an RPC completes with an error.
    resumed_unverified: Option<String>,
}

impl TurnLifecycle {
    fn submit(
        &mut self,
        broker: &mut Broker,
        project: &Path,
        prompt: String,
    ) -> Result<(), &'static str> {
        if self.resumed_unverified.is_some() {
            return Err("Resumed Codex turn state is not verified; reconnect required.");
        }
        if self.busy {
            return Err("Codex is already processing a turn.");
        }
        if prompt.trim().is_empty() || prompt.len() > MAX_PROMPT_BYTES {
            return Err("Prompt is empty or exceeds the 8 KiB limit.");
        }
        // Only advance the UI state after the Broker has queued a valid RPC.
        let policy = if self.coding_enabled {
            "on-request"
        } else {
            "never"
        };
        let sandbox = if self.coding_enabled {
            json!({"type":"workspaceWrite","networkAccess":false})
        } else {
            json!({"type":"readOnly","networkAccess":false})
        };
        let (thread_request, turn_request, pending_prompt) = if let Some(thread) =
            self.thread.as_deref()
        {
            let id = broker
                .call(
                    ClientMethod::TurnStart,
                    json!({"threadId":thread,"input":[{"type":"text","text":prompt}],
                            "approvalPolicy":policy,
                            "sandboxPolicy":sandbox}),
                )
                .map_err(|_| "Codex turn submission was rejected.")?;
            (None, Some(id), None)
        } else {
            let cwd = project.to_str().ok_or("Project path is not valid UTF-8.")?;
            // Explicit opt-in workspace-write + per-request approvals, or the
            // original read-only never-approve behavior. No default elevation.
            let id = broker
                    .call(
                        ClientMethod::ThreadStart,
                        json!({"cwd":cwd,"sandbox":if self.coding_enabled {"workspace-write"} else {"read-only"},"approvalPolicy":policy}),
                    )
                    .map_err(|_| "Codex thread creation was rejected.")?;
            (Some(id), None, Some(prompt))
        };
        self.busy = true;
        self.cancel_requested = false;
        self.text_bytes = 0;
        self.assistant_item = None;
        self.turn = None;
        self.thread_request = thread_request;
        self.turn_request = turn_request;
        self.interrupt_request = None;
        self.interrupt_sent = false;
        self.pending_prompt = pending_prompt;
        self.project = Some(project.to_path_buf());
        self.terminal_error = None;
        self.rpc_started = Some(Instant::now());
        self.cancel_started = None;
        self.unverified_completion = None;
        self.tools.clear();
        self.resumed_turn_started = None;
        Ok(())
    }

    fn start_turn(&mut self, broker: &mut Broker) -> Result<(), &'static str> {
        let thread_id = self
            .thread
            .as_deref()
            .ok_or("Codex thread is unavailable.")?;
        let prompt = self
            .pending_prompt
            .as_deref()
            .ok_or("Pending prompt was lost.")?;
        self.turn_request = Some(
            broker
                .call(
                    ClientMethod::TurnStart,
                    json!({
                        "threadId": thread_id,
                        "input": [{ "type": "text", "text": prompt }],
                        "approvalPolicy": if self.coding_enabled { "on-request" } else { "never" },
                        "sandboxPolicy": if self.coding_enabled { json!({"type":"workspaceWrite","networkAccess":false}) } else { json!({"type":"readOnly","networkAccess":false}) },
                    }),
                )
                .map_err(|_| "Codex turn submission was rejected.")?,
        );
        self.pending_prompt = None;
        self.rpc_started = Some(Instant::now());
        Ok(())
    }

    fn cancel(&mut self, broker: &mut Broker) -> Result<(), &'static str> {
        if !self.busy {
            // egui can enqueue Stop while the server's terminal notification
            // is still being drained. After an authoritative turn/completed,
            // the queued Stop must not append an Error and overwrite the
            // completed/interrupted status in the UI. It is simply too late.
            return Ok(());
        }
        if !self.cancel_requested {
            self.cancel_started = Some(Instant::now());
        }
        self.cancel_requested = true;
        self.tools.clear_pending();
        self.interrupt_if_known(broker)
    }

    fn interrupt_if_known(&mut self, broker: &mut Broker) -> Result<(), &'static str> {
        if !self.cancel_requested || self.interrupt_sent {
            return Ok(());
        }
        let (Some(thread), Some(turn)) = (&self.thread, &self.turn) else {
            // A request was sent, but the server has not yet issued a turn ID.
            // Do not invent an ID or claim that the server turn was cancelled.
            return Ok(());
        };
        broker
            .decline_approvals_for_turn(thread, turn)
            .map_err(|_| "Could not safely decline outstanding tool requests.")?;
        self.interrupt_request = Some(
            broker
                .interrupt_turn(thread, turn)
                .map_err(|_| "Codex turn interrupt was rejected.")?,
        );
        self.interrupt_sent = true;
        Ok(())
    }

    fn identify_turn(
        &mut self,
        broker: &mut Broker,
        id: &str,
    ) -> Result<Option<ProbeEvent>, &'static str> {
        if !self.busy || self.turn_request.is_none() {
            return Ok(None);
        }
        if self.turn.as_deref() == Some(id) {
            return Ok(None);
        }
        if self.turn.is_some() {
            return Err("Codex returned conflicting turn identities.");
        }
        // If completion reached us first, compare it with the start identity
        // from another official frame. Do not interrupt an already-ended turn.
        if let Some(early) = self.unverified_completion.take() {
            if early.turn_id == id {
                return Ok(Some(self.finish_turn(early)));
            }
            // A stale same-thread notification is not proof that this new
            // turn finished. Continue normally with the actual start ID.
        }
        self.turn = Some(id.to_owned());
        self.interrupt_if_known(broker)?;
        Ok(Some(ProbeEvent::TurnStarted {
            thread_id: self.thread.clone().ok_or("Codex thread is unavailable.")?,
            turn_id: id.to_owned(),
        }))
    }

    fn exact_turn(&self, params: &Value) -> bool {
        self.busy
            && self.thread.is_some()
            && self.turn.is_some()
            && self.thread.as_deref() == safe_id(params.get("threadId"))
            && self.turn.as_deref() == turn_id(params)
    }

    fn take_terminal_error(&mut self) -> Option<&'static str> {
        self.terminal_error.take()
    }

    fn finish_turn(&mut self, completed: UnverifiedCompletion) -> ProbeEvent {
        let event = ProbeEvent::TurnCompleted {
            thread_id: self.thread.clone().expect("official thread checked"),
            turn_id: completed.turn_id,
            status: completed.status.to_owned(),
        };
        self.terminal_error = completed.error;
        self.turn = None;
        self.turn_request = None;
        self.busy = false;
        self.cancel_requested = false;
        self.interrupt_request = None;
        self.interrupt_sent = false;
        self.rpc_started = None;
        self.cancel_started = None;
        self.assistant_item = None;
        self.unverified_completion = None;
        self.tools.clear_pending();
        self.resumed_turn_started = None;
        event
    }

    fn timeout_error(&self, now: Instant) -> Option<&'static str> {
        if self
            .cancel_started
            .is_some_and(|start| now.saturating_duration_since(start) >= CANCEL_TIMEOUT)
        {
            return Some("Codex did not acknowledge cancellation; owned process stopped.");
        }
        if self
            .resumed_turn_started
            .is_some_and(|start| now.saturating_duration_since(start) >= RESUMED_TURN_TIMEOUT)
        {
            return Some("Codex resumed active turn without a terminal; owned process stopped.");
        }
        if self
            .rpc_started
            .is_some_and(|start| now.saturating_duration_since(start) >= RPC_TIMEOUT)
        {
            return Some("Codex did not answer the thread/turn request; owned process stopped.");
        }
        None
    }

    fn decide(
        &mut self,
        broker: &mut Broker,
        expected: &Approval,
        accept: bool,
        reviewed: Option<&ToolEvidence>,
    ) -> Result<Option<ProbeEvent>, &'static str> {
        let project = self
            .project
            .as_deref()
            .ok_or("Codex project context unavailable.")?;
        if !self.busy
            || !self.coding_enabled
            || self.cancel_requested
            || self.thread.as_deref() != Some(&expected.thread_id)
            || self.turn.as_deref() != Some(&expected.turn_id)
        {
            return Err("Approval is stale, mismatched, or lacks complete review evidence.");
        }
        let admissible = if accept {
            reviewed
                .is_some_and(|snapshot| self.tools.consume_reviewed(expected, project, snapshot))
        } else {
            self.tools.take(expected)
        };
        if !admissible {
            return Err("Approval preview changed or request is no longer pending.");
        }
        broker
            .decide_exact_approval(
                expected,
                if accept {
                    ApprovalDecision::Accept
                } else {
                    ApprovalDecision::Decline
                },
                accept,
            )
            .map_err(|_| "Approval is stale or already resolved.")?;
        Ok(Some(ProbeEvent::ApprovalResolved(expected.id.clone())))
    }

    fn on_event(
        &mut self,
        broker: &mut Broker,
        event: Event,
    ) -> Result<Option<ProbeEvent>, &'static str> {
        match event {
            Event::Approval(approval) => {
                let admissible = self.coding_enabled
                    && self.busy
                    && !self.cancel_requested
                    && self.thread.as_deref() == Some(&approval.thread_id)
                    && self.turn.as_deref() == Some(&approval.turn_id)
                    && self
                        .project
                        .as_deref()
                        .is_some_and(|path| self.tools.offer(approval.clone(), path));
                if admissible {
                    return Ok(Some(ProbeEvent::ApprovalPending(approval)));
                }
                // Missing/incomplete evidence, unexpected identity, overflow or
                // read-only mode: broker must synchronously decline or stop.
                broker
                    .decline_approval(&approval.id)
                    .map_err(|_| "Could not safely decline a tool request.")?;
                Ok(None)
            }
            Event::Reply {
                id,
                method,
                outcome,
            } => {
                match method {
                    ClientMethod::Initialize => {
                        if outcome.is_err() {
                            return Err("Codex rejected the initial handshake.");
                        }
                        Ok(None)
                    }
                    ClientMethod::ThreadStart if self.thread_request == Some(id) => {
                        self.thread_request = None;
                        self.rpc_started = None;
                        let value = outcome.map_err(|_| "Codex rejected thread creation.")?;
                        // Never start a turn based only on the optimistic
                        // client-side policy request.
                        let project = self
                            .project
                            .as_deref()
                            .ok_or("Codex project provenance was lost.")?;
                        let created = accepted_thread(&value, project, self.coding_enabled)?;
                        self.thread = Some(created.clone());
                        if self.cancel_requested {
                            self.pending_prompt = None;
                            self.busy = false;
                            self.cancel_started = None;
                            self.cancel_requested = false;
                            // Only thread/start was sent. The pending prompt
                            // never became a model turn and needs an explicit
                            // UI acknowledgement so the composer can unlock.
                            return Ok(Some(ProbeEvent::TurnCancelledBeforeStart));
                        } else {
                            self.start_turn(broker)?;
                        }
                        Ok(Some(ProbeEvent::ThreadReady { thread_id: created }))
                    }
                    ClientMethod::TurnStart if self.turn_request == Some(id) => {
                        self.rpc_started = None;
                        let value = outcome.map_err(|_| "Codex rejected the turn request.")?;
                        let id = turn_id(&value)
                            .ok_or("Codex turn response lacked an exact turn ID.")?;
                        self.identify_turn(broker, id)
                    }
                    ClientMethod::TurnInterrupt if self.interrupt_request == Some(id) => {
                        self.interrupt_request = None;
                        // A rejected interrupt can race an already-terminal
                        // turn. Neither an ACK nor a rejection proves how the
                        // turn ended. Keep cancellation pending until a
                        // matched turn/completed arrives; the cancel watchdog
                        // kills the owned process if that never happens.
                        let _ = outcome;
                        Ok(None)
                    }
                    _ => Ok(None),
                }
            }
            Event::Notification { kind, params } => {
                match kind {
                    NotificationKind::TurnStarted => {
                        if !self.busy || self.thread.as_deref() != safe_id(params.get("threadId")) {
                            return Ok(None);
                        }
                        let id =
                            turn_id(&params).ok_or("Codex turn started without a valid ID.")?;
                        self.identify_turn(broker, id)
                    }
                    NotificationKind::AgentMessageDelta if self.exact_turn(&params) => {
                        let item_id = safe_id(params.get("itemId"))
                            .ok_or("Codex assistant stream lacked an item identity.")?;
                        // A turn may contain several assistant messages. The
                        // current minimal UI has one streaming text target:
                        // never concatenate distinct item streams. A later
                        // item's canonical final can still replace it.
                        if self
                            .assistant_item
                            .as_deref()
                            .is_some_and(|id| id != item_id)
                        {
                            return Ok(None);
                        }
                        if self.assistant_item.is_none() {
                            self.assistant_item = Some(item_id.to_owned());
                        }
                        let delta = params
                            .get("delta")
                            .and_then(Value::as_str)
                            .ok_or("Codex assistant delta was malformed.")?;
                        self.text_bytes = self
                            .text_bytes
                            .checked_add(delta.len())
                            .filter(|length| *length <= MAX_ASSISTANT_BYTES)
                            .ok_or("Codex assistant text exceeded the 256 KiB limit.")?;
                        Ok(Some(ProbeEvent::AssistantDelta {
                            thread_id: self.thread.clone().expect("checked"),
                            turn_id: self.turn.clone().expect("checked"),
                            text: delta.to_owned(),
                        }))
                    }
                    NotificationKind::ItemCompleted
                        if self.exact_turn(&params)
                            && params.pointer("/item/type").and_then(Value::as_str)
                                == Some("agentMessage") =>
                    {
                        let item = params.get("item");
                        if item.and_then(|v| v.get("type")).and_then(Value::as_str)
                            != Some("agentMessage")
                        {
                            return Ok(None);
                        }
                        safe_id(item.and_then(|value| value.get("id")))
                            .ok_or("Codex final assistant item lacked an identity.")?;
                        let Some(final_text) =
                            item.and_then(|v| v.get("text")).and_then(Value::as_str)
                        else {
                            return Ok(None);
                        };
                        if final_text.len() > MAX_ASSISTANT_BYTES {
                            return Err("Codex final assistant text exceeded the 256 KiB limit.");
                        }
                        Ok(Some(ProbeEvent::AssistantFinal {
                            thread_id: self.thread.clone().expect("checked"),
                            turn_id: self.turn.clone().expect("checked"),
                            text: final_text.to_owned(),
                        }))
                    }
                    NotificationKind::ServerRequestResolved => Ok(self
                        .tools
                        .resolved(&params)
                        .map(ProbeEvent::ApprovalResolved)),
                    NotificationKind::ItemStarted
                    | NotificationKind::ItemCompleted
                    | NotificationKind::CommandOutputDelta
                        if self.exact_turn(&params) =>
                    {
                        let project = self
                            .project
                            .as_deref()
                            .ok_or("Codex project context unavailable.")?;
                        Ok(self
                            .tools
                            .capture(kind, &params, project)
                            .map(ProbeEvent::ToolSnapshot))
                    }
                    NotificationKind::TurnCompleted
                        if self.busy
                            && self.thread.as_deref() == safe_id(params.get("threadId")) =>
                    {
                        if self.turn.is_some() && !self.exact_turn(&params) {
                            return Ok(None);
                        }
                        let completion = UnverifiedCompletion::from_notification(&params)?;
                        if self.turn.is_some() {
                            if let (Some(thread), Some(turn)) = (&self.thread, &self.turn) {
                                broker
                                    .decline_approvals_for_turn(thread, turn)
                                    .map_err(|_| {
                                        "Could not safely settle outstanding approvals."
                                    })?;
                            }
                            return Ok(Some(self.finish_turn(completion)));
                        }
                        if self.turn_request.is_none() {
                            // No turn/start has been submitted. An unrelated
                            // thread's terminal event must not be actionable.
                            return Ok(None);
                        }
                        if let Some(previous) = &self.unverified_completion {
                            if previous.turn_id != completion.turn_id
                                || previous.status != completion.status
                            {
                                return Err(
                                    "Codex reported conflicting unverified turn completions.",
                                );
                            }
                        } else {
                            self.unverified_completion = Some(completion);
                        }
                        Ok(None)
                    }
                    _ => Ok(None), // Unknown, unproven or cross-turn content is not shown.
                }
            }
        }
    }
}

// Single in-flight authoritative history request. No local session database,
// replay queue, or automatic post-crash turn resubmission exists.
#[derive(Clone, Debug)]
enum HistoryRequest {
    List { append: bool },
    Read { thread: String },
    Resume { thread: String },
    Turns { thread: String, prepend: bool },
}

#[derive(Default)]
struct HistoryFlow {
    pending: Option<(u64, HistoryRequest, Instant)>,
    known: Vec<ThreadEntry>,
    selected: Option<String>,
    next_threads: Option<String>,
    next_turns: Option<String>,
}

impl HistoryFlow {
    fn timeout_error(&self, now: Instant) -> Option<&'static str> {
        self.pending.as_ref().and_then(|(_, _, start)| {
            (now.saturating_duration_since(*start) >= HISTORY_TIMEOUT)
                .then_some("Codex history RPC timed out; owned process stopped.")
        })
    }

    fn request(
        &mut self,
        broker: &mut Broker,
        method: ClientMethod,
        params: Value,
        operation: HistoryRequest,
    ) -> Result<(), &'static str> {
        if self.pending.is_some() {
            return Err("Codex history request already in progress.");
        }
        let id = broker
            .call(method, params)
            .map_err(|_| "Codex history request failed validation.")?;
        self.pending = Some((id, operation, Instant::now()));
        Ok(())
    }

    fn list(
        &mut self,
        broker: &mut Broker,
        project: &Path,
        append: bool,
    ) -> Result<(), &'static str> {
        if !append {
            self.known.clear();
            self.next_threads = None;
        }
        if append
            && (self.known.len() >= session_history::MAX_THREADS || self.next_threads.is_none())
        {
            return Ok(());
        }
        let cwd = project.to_str().ok_or("Project path is not valid UTF-8.")?;
        let mut params = json!({"cwd":cwd,"limit":session_history::THREAD_PAGE_SIZE,
            "sourceKinds":["cli","vscode","appServer"]});
        if append {
            params["cursor"] = Value::String(self.next_threads.clone().expect("checked"));
        }
        self.request(
            broker,
            ClientMethod::ThreadList,
            params,
            HistoryRequest::List { append },
        )
    }

    fn select(&mut self, broker: &mut Broker, id: String) -> Result<(), &'static str> {
        if !self.known.iter().any(|e| e.id == id) {
            return Err("Selected thread was not in the trusted project listing.");
        }
        self.request(
            broker,
            ClientMethod::ThreadRead,
            json!({"threadId":id,"includeTurns":false}),
            HistoryRequest::Read { thread: id },
        )
    }

    fn turns(
        &mut self,
        broker: &mut Broker,
        thread: &str,
        prepend: bool,
    ) -> Result<(), &'static str> {
        if self.selected.as_deref() != Some(thread) {
            return Err("Codex history thread selection changed.");
        }
        let mut params = json!({"threadId":thread,"limit":session_history::TURN_PAGE_SIZE,
            "sortDirection":"desc","itemsView":"full"});
        if prepend {
            let Some(cursor) = self.next_turns.clone() else {
                return Ok(());
            };
            params["cursor"] = Value::String(cursor);
        } else {
            self.next_turns = None;
        }
        self.request(
            broker,
            ClientMethod::ThreadTurnsList,
            params,
            HistoryRequest::Turns {
                thread: thread.to_owned(),
                prepend,
            },
        )
    }

    fn handle_reply(
        &mut self,
        broker: &mut Broker,
        event: &Event,
        lifecycle: &mut TurnLifecycle,
        project: &Path,
    ) -> Result<Option<Option<ProbeEvent>>, &'static str> {
        let Event::Reply { id, outcome, .. } = event else {
            return Ok(None);
        };
        let Some((pending_id, _, _)) = &self.pending else {
            return Ok(None);
        };
        if id != pending_id {
            return Ok(None);
        }
        let (_, request, _) = self.pending.take().expect("checked");
        let Ok(result) = outcome else {
            // Once a resumed thread has been adopted, a rejected FIRST turns
            // page is a fatal verification failure, not a recoverable UI state.
            // The worker propagates this error into owned-process shutdown.
            if lifecycle.resumed_unverified.is_some() {
                return Err("Codex resumed turn verification failed; owned process stopped.");
            }
            // Explicit unsupported capability, no silent fallback to unbounded
            // thread/read(includeTurns=true), replay, or new thread execution.
            return Ok(Some(Some(ProbeEvent::HistoryCapabilityBlocked(
                "Official thread paging/read/resume is unsupported or rejected by this Codex App Server."
            ))));
        };
        match request {
            HistoryRequest::List { append } => {
                let (entries, next) = session_history::parse_thread_page(result, project)?;
                if !append {
                    self.known.clear();
                }
                let mut new_entries = Vec::new();
                for entry in entries {
                    if self.known.len() == session_history::MAX_THREADS {
                        break;
                    }
                    if !self.known.iter().any(|known| known.id == entry.id) {
                        self.known.push(entry.clone());
                        new_entries.push(entry);
                    }
                }
                self.next_threads = if self.known.len() >= session_history::MAX_THREADS {
                    None
                } else {
                    next
                };
                Ok(Some(Some(ProbeEvent::ThreadPage {
                    entries: new_entries,
                    append,
                    has_more: self.next_threads.is_some(),
                })))
            }
            HistoryRequest::Read { thread } => {
                session_history::verify_thread_identity(result, &thread, project)?;
                let cwd = project.to_str().ok_or("Project path is not valid UTF-8.")?;
                self.request(
                    broker,
                    ClientMethod::ThreadResume,
                    json!({"threadId":thread,"cwd":cwd,"approvalPolicy":"never",
                        "sandbox":"read-only","excludeTurns":true}),
                    HistoryRequest::Resume { thread },
                )?;
                Ok(Some(None))
            }
            HistoryRequest::Resume { thread } => {
                // A resume can change effective authorization. A metadata
                // read is not proof that the resumed session is read-only.
                let resumed_id = accepted_read_only_thread(result, project)?;
                if resumed_id != thread {
                    return Err("Codex resumed a different thread identity.");
                }
                lifecycle.resumed_unverified = Some(thread.clone());
                lifecycle.thread = Some(thread.clone());
                lifecycle.project = Some(project.to_path_buf());
                self.selected = Some(thread.clone());
                self.next_turns = None;
                self.turns(broker, &thread, false)?;
                Ok(Some(Some(ProbeEvent::ThreadSelected {
                    thread_id: Some(thread),
                })))
            }
            HistoryRequest::Turns { thread, prepend } => {
                if self.selected.as_deref() != Some(thread.as_str()) {
                    return Err("Codex turns page changed selected thread identity.");
                }
                let first_resume_page =
                    !prepend && lifecycle.resumed_unverified.as_deref() == Some(thread.as_str());
                if !prepend && !first_resume_page {
                    return Err("Codex turns page lacked a matching resume verification gate.");
                }
                // Some protocol versions echo threadId in this response. If
                // present it MUST match the broker-bound request identity.
                if result.get("threadId").is_some()
                    && safe_id(result.get("threadId")) != Some(thread.as_str())
                {
                    return Err("Codex turns page reported a foreign thread identity.");
                }
                let active_turn = if prepend {
                    None
                } else {
                    session_history::active_turn_in_page(result)?
                };
                let (rows, next) = session_history::parse_turn_page(result)?;
                if let Some(id) = &active_turn {
                    if lifecycle.busy || lifecycle.turn_request.is_some() {
                        return Err("Codex resume returned conflicting active-turn state.");
                    }
                    lifecycle.turn = Some(id.clone());
                    lifecycle.busy = true;
                    lifecycle.resumed_turn_started = Some(Instant::now());
                }
                self.next_turns = next;
                // Release only after complete validation and active-turn
                // adoption. Any error above leaves the gate locked and the
                // worker destroys the owned session (never prompt replay).
                if first_resume_page {
                    lifecycle.resumed_unverified = None;
                }
                Ok(Some(Some(ProbeEvent::HistoryPage {
                    thread_id: thread,
                    rows,
                    prepend,
                    has_more: self.next_turns.is_some(),
                    active_turn,
                })))
            }
        }
    }
}

async fn flush(broker: &mut Broker, supervisor: &mut EngineSupervisor) -> Result<(), &'static str> {
    while let Some(frame) = broker.take_outbound() {
        let payload: Value = serde_json::from_slice(&frame)
            .map_err(|_| "Native protocol outbound frame was invalid.")?;
        supervisor
            .session_mut()
            .ok_or("Codex session closed.")?
            .send_json(&payload)
            .await
            .map_err(|_| "Codex transport write failed.")?;
    }
    Ok(())
}

async fn drive(
    paths: (PathBuf, PathBuf, Option<PathBuf>),
    shutdown: oneshot::Receiver<()>,
    mut commands: mpsc::Receiver<Command>,
    emergency_stop: Arc<AtomicBool>,
    events: Arc<Mutex<DisplayEvents>>,
    status: Arc<AtomicU8>,
    ui_context: (egui::Context, bool),
) {
    let (ctx, coding_enabled) = ui_context;
    let (executable, project, runtime_store) = paths;
    let prepared = (|| {
        let selection =
            crate::native_runtime::resolve(&executable, &project, runtime_store.as_deref())?;
        let broker = Broker::new(Limits::default())
            .map_err(|_| "Codex protocol broker failed initialization.")?;
        Ok::<_, &'static str>((selection, broker))
    })();
    let (mut selection, mut broker) = match prepared {
        Ok(v) => v,
        Err(error) => {
            if let Ok(mut queue) = events.lock() {
                queue.fatal(error);
            }
            notify(&status, &ctx, ProbeStatus::Failed);
            return;
        }
    };
    let mut supervisor = EngineSupervisor::new(selection.config.clone());
    let gate = SessionGate::<u64>::default();
    if gate
        .start(1, || {
            supervisor.start().map_err(|_| ())?;
            Ok::<_, ()>((1_u64, ()))
        })
        .await
        .is_err()
    {
        // A failed managed spawn must not be silently retried. Roll back
        // only if the supervisor confirms that no owned engine remains.
        let reaped = supervisor.stop().await.is_ok();
        if !reaped {
            selection.retain_lock_after_uncertain_cleanup();
        }
        let recovered = reaped
            && selection.is_managed()
            && selection.rollback_after_confirmed_failure().is_ok();
        if let Ok(mut queue) = events.lock() {
            queue.fatal(if recovered {
                "Codex process failed to start; runtime fallback requires explicit reconnect."
            } else {
                "Codex process startup/rollback could not be verified."
            });
        }
        notify(&status, &ctx, ProbeStatus::Failed);
        return;
    }
    let mut shutdown = shutdown;
    let mut lifecycle = TurnLifecycle {
        coding_enabled,
        ..Default::default()
    };
    let mut history = HistoryFlow::default();
    let mut watchdog = tokio::time::interval(Duration::from_secs(1));
    let handshake_started = Instant::now();
    let result: Result<(), &'static str> = async {
        broker.begin_initialize().map_err(|_| "Codex handshake could not start.")?;
        flush(&mut broker, &mut supervisor).await?;
        loop {
            tokio::select! {
                biased;
                _ = &mut shutdown => return Ok(()),
                _ = watchdog.tick() => {
                    if emergency_stop.load(Ordering::Acquire) {
                        return Err("Codex cancellation queue unavailable; owned process stopped.");
                    }
                    if broker.phase() != Phase::Ready &&
                        handshake_started.elapsed() >= HANDSHAKE_TIMEOUT {
                        return Err("Codex initial handshake timed out.");
                    }
                    if let Some(reason) = lifecycle.timeout_error(Instant::now()) {
                        return Err(reason);
                    }
                    if let Some(reason) = history.timeout_error(Instant::now()) {
                        return Err(reason);
                    }
                }
                command = commands.recv() => {
                    if emergency_stop.load(Ordering::Acquire) {
                        return Err("Codex cancellation queue unavailable; owned process stopped.");
                    }
                    let Some(command) = command else { return Ok(()); };
                    if broker.phase() != Phase::Ready {
                        enqueue(&events, &ctx, ProbeEvent::Error("Codex handshake is not ready.".to_owned()))?;
                        continue;
                    }
                    let outcome = match command {
                        Command::Submit(text) => {
                            if history.pending.is_some() || lifecycle.resumed_unverified.is_some() {
                                Err("Resumed turn verification is in progress; submission blocked.")
                            } else {
                                lifecycle.submit(&mut broker, &project, text)
                            }
                        }
                        Command::Cancel => lifecycle.cancel(&mut broker),
                        Command::Decide(request, accept, reviewed) => {
                            match lifecycle.decide(&mut broker, &request, accept, reviewed.as_deref()) {
                                Ok(Some(event)) => { enqueue(&events, &ctx, event)?; Ok(()) },
                                Ok(None) => Ok(()),
                                Err(reason) => return Err(reason), // no ambiguous pending grant
                            }
                        },
                        Command::NewThread if !lifecycle.busy && history.pending.is_none()
                            && lifecycle.resumed_unverified.is_none() => {
                            lifecycle.thread = None;
                            history.selected = None;
                            history.next_turns = None;
                            enqueue(&events, &ctx,
                                ProbeEvent::ThreadSelected {thread_id:None})?;
                            Ok(())
                        }
                        Command::SelectThread(id) if !lifecycle.busy
                            && lifecycle.resumed_unverified.is_none()
                            && !lifecycle.coding_enabled => {
                            // NC-10F verifies resumed threads only as read-only;
                            // never adopt one into an opt-in coding session.
                            history.select(&mut broker, id)
                        }
                        Command::MoreThreads if !lifecycle.busy
                            && lifecycle.resumed_unverified.is_none() => {
                            history.list(&mut broker, &project, true)
                        }
                        Command::MoreHistory if !lifecycle.busy
                            && lifecycle.resumed_unverified.is_none() => {
                            if let Some(thread) = history.selected.clone() {
                                history.turns(&mut broker, &thread, true)
                            } else { Err("No stored thread selected.") }
                        }
                        _ => Err("Cannot switch or page threads during an active turn."),
                    };
                    if let Err(message) = outcome {
                        if lifecycle.cancel_requested {
                            // Failed interrupt/decline: destroy the owned
                            // process rather than let it continue unconfirmed.
                            return Err(message);
                        }
                        enqueue(&events, &ctx, ProbeEvent::Error(message.to_owned()))?;
                    }
                    flush(&mut broker, &mut supervisor).await?;
                }
                event = async {
                    supervisor.session_mut().ok_or("Codex session closed.")?
                        .next_event().await.ok_or("Codex session disconnected.")
                } => {
                    match event? {
                        OutputEvent::Line(line) => {
                            broker.ingest_line(line.as_bytes())
                                .map_err(|_| "Codex protocol input failed validation.")?;
                            if broker.phase() == Phase::Acknowledgeable {
                                broker.acknowledge_initialized()
                                    .map_err(|_| "Codex handshake acknowledgement failed.")?;
                                flush(&mut broker, &mut supervisor).await?;
                                notify(&status, &ctx, ProbeStatus::Ready);
                                // Authoritative persistence lives only in
                                // Codex; never scan or synthesize JSONL files.
                                history.list(&mut broker, &project, false)?;
                            }
                            while let Some(event) = broker.take_event() {
                                let translated = match history.handle_reply(
                                    &mut broker, &event, &mut lifecycle, &project
                                )? {
                                    Some(projection) => projection,
                                    None => lifecycle.on_event(&mut broker, event)?,
                                };
                                if let Some(ProbeEvent::ThreadReady {thread_id}) = &translated {
                                    history.selected = Some(thread_id.clone());
                                    if !history.known.iter().any(|e| e.id == *thread_id) {
                                        let entry = ThreadEntry {
                                            id: thread_id.clone(),
                                            label: "New Codex thread".to_owned(),
                                        };
                                        history.known.insert(0, entry.clone());
                                        enqueue(&events, &ctx, ProbeEvent::ThreadPage {
                                            entries:vec![entry], append:true,
                                            has_more:history.next_threads.is_some()
                                        })?;
                                    }
                                }
                                if let Some(event) = translated {
                                    enqueue(&events, &ctx, event)?;
                                }
                                // Deliver an independently verified terminal
                                // before its diagnostic error so the UI can
                                // settle a turn that completed ahead of its
                                // start RPC response.
                                if let Some(message) = lifecycle.take_terminal_error() {
                                    enqueue(&events, &ctx, ProbeEvent::Error(message.to_owned()))?;
                                }
                            }
                            flush(&mut broker, &mut supervisor).await?;
                        }
                        OutputEvent::Closed => return Err("Codex closed its connection."),
                        OutputEvent::LineTooLong => return Err("Codex sent an oversized protocol line."),
                        OutputEvent::InvalidUtf8 => return Err("Codex sent invalid UTF-8."),
                        OutputEvent::ReadFailed => return Err("Codex output could not be read."),
                    }
                }
            }
        }
    }.await;
    let stopping = result.is_ok() && matches!(gate.begin_stop().await, Ok(Some((1, _))));
    // The owned process tree must stop before the session gate can be released.
    let clean = supervisor.stop().await.is_ok();
    let finalized = if result.is_ok() && stopping {
        gate.finish_stop(1, clean).await
    } else {
        gate.finish_unexpected(1, clean && result.is_err()).await
    };
    if !clean || !finalized {
        selection.retain_lock_after_uncertain_cleanup();
    }
    // Protocol/history/transport/user Stop and ordinary session failures are
    // not proof that the selected managed executable is defective. Never
    // mutate the shared host journal for these unrelated failures.
    // Only a confirmed managed spawn failure above can request rollback.
    if result.is_err() || !clean || !finalized {
        let error = result
            .err()
            .unwrap_or("Codex process shutdown could not be verified.");
        if let Ok(mut queue) = events.lock() {
            queue.fatal(error);
        }
        notify(&status, &ctx, ProbeStatus::Failed);
    } else {
        notify(&status, &ctx, ProbeStatus::Stopped);
    }
}

impl EngineProbe {
    pub fn start(
        executable: PathBuf,
        project: PathBuf,
        ctx: egui::Context,
        coding_enabled: bool,
        runtime_store: Option<PathBuf>,
    ) -> Self {
        let (shutdown, receiver) = oneshot::channel();
        let (commands, inbox) = mpsc::channel(MAX_COMMANDS);
        let status = Arc::new(AtomicU8::new(ProbeStatus::Connecting as u8));
        let emergency_stop = Arc::new(AtomicBool::new(false));
        let events = Arc::new(Mutex::new(DisplayEvents::default()));
        let thread_status = status.clone();
        let thread_emergency_stop = emergency_stop.clone();
        let thread_events = events.clone();
        let thread = std::thread::spawn(move || {
            match tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime.block_on(drive(
                    (executable, project, runtime_store),
                    receiver,
                    inbox,
                    thread_emergency_stop,
                    thread_events,
                    thread_status,
                    (ctx, coding_enabled),
                )),
                Err(_) => {
                    if let Ok(mut queue) = thread_events.lock() {
                        queue.fatal("Native asynchronous runtime failed to start.");
                    }
                    notify(&thread_status, &ctx, ProbeStatus::Failed);
                }
            }
        });
        Self {
            status,
            commands,
            emergency_stop,
            events,
            shutdown: Some(shutdown),
            thread: Some(thread),
        }
    }

    pub fn status(&self) -> ProbeStatus {
        match self.status.load(Ordering::Acquire) {
            0 => ProbeStatus::Connecting,
            1 => ProbeStatus::Ready,
            2 => ProbeStatus::Failed,
            _ => ProbeStatus::Stopped,
        }
    }

    pub fn submit_prompt(&self, prompt: String) -> Result<(), &'static str> {
        if self.status() != ProbeStatus::Ready {
            return Err("Codex handshake is not ready.");
        }
        if prompt.trim().is_empty() || prompt.len() > MAX_PROMPT_BYTES {
            return Err("Prompt is empty or exceeds the 8 KiB limit.");
        }
        self.commands
            .try_send(Command::Submit(prompt))
            .map_err(|_| "Codex command queue is full or disconnected.")
    }

    pub fn decide(
        &self,
        request: Approval,
        accept: bool,
        reviewed: Option<ToolEvidence>,
    ) -> Result<(), &'static str> {
        if self.status() != ProbeStatus::Ready {
            return Err("Codex handshake is not ready.");
        }
        if self
            .commands
            .try_send(Command::Decide(request, accept, reviewed.map(Box::new)))
            .is_err()
        {
            self.emergency_stop.store(true, Ordering::Release);
            return Err("Approval queue unavailable; stopping the owned process.");
        }
        Ok(())
    }

    pub fn new_thread(&self) -> Result<(), &'static str> {
        self.send_history_command(Command::NewThread)
    }

    pub fn select_thread(&self, id: String) -> Result<(), &'static str> {
        self.send_history_command(Command::SelectThread(id))
    }

    pub fn load_more_threads(&self) -> Result<(), &'static str> {
        self.send_history_command(Command::MoreThreads)
    }

    pub fn load_more_history(&self) -> Result<(), &'static str> {
        self.send_history_command(Command::MoreHistory)
    }

    fn send_history_command(&self, command: Command) -> Result<(), &'static str> {
        if self.status() != ProbeStatus::Ready {
            return Err("Codex handshake is not ready.");
        }
        self.commands
            .try_send(command)
            .map_err(|_| "Codex history command queue is unavailable.")
    }

    pub fn cancel(&self) -> Result<(), &'static str> {
        if self.status() != ProbeStatus::Ready {
            return Err("Codex handshake is not ready.");
        }
        if self.commands.try_send(Command::Cancel).is_err() {
            // A full command queue must never leave a billed turn running
            // after the owner asks to cancel. The worker checks this atomic
            // flag before processing further work and destroys the process.
            self.emergency_stop.store(true, Ordering::Release);
            return Err("Codex cancellation queue unavailable; stopping its process.");
        }
        Ok(())
    }

    pub fn take_timed_events(&self) -> Vec<TimedProbeEvent> {
        match self.events.lock() {
            Ok(mut queue) => queue.take_timed(),
            Err(_) => vec![TimedProbeEvent {
                event: ProbeEvent::Error("Native display state unavailable.".to_owned()),
                received_at: Instant::now(),
                coalesced: 0,
            }],
        }
    }
}

impl Drop for EngineProbe {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ultima_protocol_bridge::{ApprovalKind, NotificationKind};

    fn ready() -> Broker {
        let mut b = Broker::new(Limits::default()).unwrap();
        b.begin_initialize().unwrap();
        b.take_outbound().unwrap();
        b.ingest_line(br#"{"id":0,"result":{"userAgent":"codex","platformFamily":"windows","platformOs":"windows"}}"#).unwrap();
        b.take_event().unwrap();
        b.acknowledge_initialized().unwrap();
        b.take_outbound().unwrap();
        b
    }

    fn reply(b: &mut Broker, id: u64, result: Value) -> Event {
        b.ingest_line(json!({"id":id,"result":result}).to_string().as_bytes())
            .unwrap();
        b.take_event().unwrap()
    }

    fn created(project: &Path) -> Value {
        let cwd = project.to_str().unwrap();
        // Matches Codex 0.164.0 generated ThreadStartResponse contract.
        json!({"thread":{"id":"thread-a","cwd":cwd},"cwd":cwd,
            "sandbox":{"type":"readOnly","networkAccess":false},
            "approvalPolicy":"never"})
    }

    #[test]
    fn completed_tool_items_are_routed_before_generic_completion() {
        let mut f = TurnLifecycle {
            busy: true,
            thread: Some("thread-a".into()),
            turn: Some("turn-a".into()),
            project: Some(std::env::current_dir().unwrap()),
            ..Default::default()
        };
        let mut broker = ready();
        for (kind, id, item) in [
            (
                ApprovalKind::Command,
                "cmd",
                json!({"id":"cmd","type":"commandExecution",
                "command":"echo ok","aggregatedOutput":"done","exitCode":0}),
            ),
            (
                ApprovalKind::FileChange,
                "file",
                json!({"id":"file","type":"fileChange",
                "changes":[{"path":"example.txt","diff":"@@ -0,0 +1 @@\\n+new"}]}),
            ),
        ] {
            let event = f
                .on_event(
                    &mut broker,
                    Event::Notification {
                        kind: NotificationKind::ItemCompleted,
                        params: json!({"threadId":"thread-a","turnId":"turn-a","item":item}),
                    },
                )
                .unwrap();
            let Some(ProbeEvent::ToolSnapshot(snapshot)) = event else {
                panic!("item completion was dropped");
            };
            assert_eq!(snapshot.id, id);
            assert_eq!(snapshot.kind, kind);
            assert_eq!(snapshot.state, "completed");
            if kind == ApprovalKind::Command {
                assert_eq!(snapshot.output, "done");
            } else {
                assert!(snapshot.diff.contains("example.txt"));
            }
        }
    }

    #[test]
    fn lost_history_reply_expires_without_retry_or_turn_execution() {
        let mut broker = ready();
        let mut history = HistoryFlow::default();
        history
            .list(&mut broker, &std::env::current_dir().unwrap(), false)
            .unwrap();
        assert_eq!(history.timeout_error(Instant::now()), None);
        history.pending.as_mut().unwrap().2 =
            Instant::now() - HISTORY_TIMEOUT - Duration::from_secs(1);
        assert_eq!(
            history.timeout_error(Instant::now()),
            Some("Codex history RPC timed out; owned process stopped.")
        );
        let rpc: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        assert_eq!(rpc["method"], "thread/list");
        assert!(broker.take_outbound().is_none());
    }

    #[test]
    fn thread_and_turn_use_exact_ids_and_read_only_policy() {
        let mut b = ready();
        let mut f = TurnLifecycle::default();
        let project = std::env::current_dir().unwrap();
        f.submit(&mut b, &project, "explain this code".into())
            .unwrap();
        let start: Value = serde_json::from_slice(&b.take_outbound().unwrap()).unwrap();
        assert_eq!(start["method"], "thread/start");
        assert_eq!(start["params"]["sandbox"], "read-only");
        assert_eq!(start["params"]["approvalPolicy"], "never");
        let response = reply(&mut b, 1, created(&project));
        assert_eq!(
            f.on_event(&mut b, response),
            Ok(Some(ProbeEvent::ThreadReady {
                thread_id: "thread-a".into()
            }))
        );
        let turn: Value = serde_json::from_slice(&b.take_outbound().unwrap()).unwrap();
        assert_eq!(turn["method"], "turn/start");
        assert_eq!(turn["params"]["threadId"], "thread-a");
        assert_eq!(turn["params"]["input"][0]["text"], "explain this code");
        assert_eq!(turn["params"]["approvalPolicy"], "never");
        assert_eq!(turn["params"]["sandboxPolicy"]["type"], "readOnly");
        assert_eq!(turn["params"]["sandboxPolicy"]["networkAccess"], false);
        assert_eq!(
            f.on_event(
                &mut b,
                Event::Notification {
                    kind: NotificationKind::TurnStarted,
                    params: json!({"threadId":"thread-a","turn":{"id":"turn-a"}}),
                }
            )
            .unwrap(),
            Some(ProbeEvent::TurnStarted {
                thread_id: "thread-a".into(),
                turn_id: "turn-a".into()
            })
        );
        assert_eq!(
            f.on_event(
                &mut b,
                Event::Notification {
                    kind: NotificationKind::AgentMessageDelta,
                    params: json!({"threadId":"thread-other","turnId":"turn-a","itemId":"msg-a","delta":"wrong"}),
                }
            )
            .unwrap(),
            None
        );
        assert_eq!(
            f.on_event(
                &mut b,
                Event::Notification {
                    kind: NotificationKind::AgentMessageDelta,
                    params: json!({"threadId":"thread-a","turnId":"turn-a","itemId":"msg-a","delta":"right"}),
                }
            )
            .unwrap(),
            Some(ProbeEvent::AssistantDelta {
                thread_id: "thread-a".into(),
                turn_id: "turn-a".into(),
                text: "right".into()
            })
        );
        assert_eq!(f.on_event(&mut b, Event::Notification {
            kind: NotificationKind::AgentMessageDelta,
            params: json!({"threadId":"thread-a","turnId":"turn-a","itemId":"msg-b","delta":"separate"}),
        }).unwrap(), None, "different assistant items must not concatenate");
        assert_eq!(f.on_event(&mut b, Event::Notification {
            kind: NotificationKind::TurnCompleted,
            params: json!({"threadId":"thread-a","turn":{"id":"wrong","status":"completed"}}),
        }).unwrap(), None);
        assert!(f.busy);
        assert_eq!(f.on_event(&mut b, Event::Notification {
            kind: NotificationKind::TurnCompleted,
            params: json!({"threadId":"thread-a","turn":{"id":"turn-a","status":"completed"}}),
        }).unwrap(), Some(ProbeEvent::TurnCompleted {thread_id:"thread-a".into(),turn_id:"turn-a".into(),status:"completed".into()}));
        assert!(!f.busy);
    }

    #[test]
    fn cancelling_during_thread_creation_never_submits_a_model_turn() {
        let mut broker = ready();
        let mut lifecycle = TurnLifecycle::default();
        let project = std::env::current_dir().unwrap();
        lifecycle
            .submit(&mut broker, &project, "do not send".into())
            .unwrap();
        let start: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        assert_eq!(start["method"], "thread/start");
        lifecycle.cancel(&mut broker).unwrap();
        assert!(broker.take_outbound().is_none());

        let confirmation = reply(
            &mut broker,
            start["id"].as_u64().unwrap(),
            created(&project),
        );
        assert_eq!(
            lifecycle.on_event(&mut broker, confirmation),
            Ok(Some(ProbeEvent::TurnCancelledBeforeStart))
        );
        assert!(!lifecycle.busy);
        assert!(!lifecycle.cancel_requested);
        assert!(lifecycle.pending_prompt.is_none());
        assert!(
            broker.take_outbound().is_none(),
            "must not issue turn/start"
        );

        lifecycle
            .submit(&mut broker, &project, "subsequent prompt".into())
            .unwrap();
        let next: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        assert_eq!(next["method"], "turn/start");
        assert_eq!(next["params"]["input"][0]["text"], "subsequent prompt");
    }

    #[test]
    fn cancel_waits_for_real_turn_id_then_declines_and_interrupts() {
        let mut b = ready();
        let mut f = TurnLifecycle {
            thread: Some("thread-a".into()),
            ..Default::default()
        };
        f.submit(&mut b, Path::new("C:/project"), "hi".into())
            .unwrap();
        b.take_outbound().unwrap();
        f.cancel(&mut b).unwrap();
        assert!(b.take_outbound().is_none());
        // An approval can arrive before the turn ID is attached to the UI.
        // Cancellation must decline it before sending the real interrupt.
        b.ingest_line(br#"{"id":"pending-tool","method":"item/commandExecution/requestApproval","params":{"threadId":"thread-a","turnId":"turn-a","itemId":"item-a","startedAtMs":1}}"#).unwrap();
        assert!(matches!(b.take_event(), Some(Event::Approval(_))));
        let _ = f
            .on_event(
                &mut b,
                Event::Notification {
                    kind: NotificationKind::TurnStarted,
                    params: json!({"threadId":"thread-a","turnId":"turn-a"}),
                },
            )
            .unwrap();
        let decline: Value = serde_json::from_slice(&b.take_outbound().unwrap()).unwrap();
        assert_eq!(decline["id"], "pending-tool");
        assert_eq!(decline["result"]["decision"], "decline");
        let interrupt: Value = serde_json::from_slice(&b.take_outbound().unwrap()).unwrap();
        assert_eq!(interrupt["method"], "turn/interrupt");
        assert_eq!(interrupt["params"]["threadId"], "thread-a");
        assert_eq!(interrupt["params"]["turnId"], "turn-a");
        let answer = reply(&mut b, interrupt["id"].as_u64().unwrap(), json!({}));
        assert_eq!(f.on_event(&mut b, answer).unwrap(), None);
        // Interrupt ACK is not turn completion, and never authorizes a replay.
        f.cancel(&mut b).unwrap();
        assert!(b.take_outbound().is_none());
    }

    #[test]
    fn official_interrupt_ack_alone_never_reports_interrupted_completion() {
        // Replay real 0.164.0 envelope shapes through Broker, not fabricated
        // Event variants: TurnInterruptResponse is {}, TurnCompletedNotification
        // carries the independent authoritative terminal status.
        let mut b = ready();
        let mut f = TurnLifecycle {
            thread: Some("thread-a".into()),
            ..Default::default()
        };
        f.submit(&mut b, Path::new("C:/project"), "synthetic".into())
            .unwrap();
        let request: Value = serde_json::from_slice(&b.take_outbound().unwrap()).unwrap();
        let start_id = request["id"].as_u64().unwrap();
        b.ingest_line(
            json!({"method":"turn/started","params":{"threadId":"thread-a",
            "turn":{"id":"turn-a","status":"inProgress"}}})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
        let started = b.take_event().unwrap();
        assert_eq!(
            f.on_event(&mut b, started).unwrap(),
            Some(ProbeEvent::TurnStarted {
                thread_id: "thread-a".into(),
                turn_id: "turn-a".into()
            })
        );
        f.cancel(&mut b).unwrap();
        let interrupt: Value = serde_json::from_slice(&b.take_outbound().unwrap()).unwrap();
        assert_eq!(interrupt["method"], "turn/interrupt");
        assert_eq!(
            interrupt["params"],
            json!({"threadId":"thread-a","turnId":"turn-a"})
        );
        let ack = reply(&mut b, interrupt["id"].as_u64().unwrap(), json!({}));
        assert_eq!(f.on_event(&mut b, ack).unwrap(), None);
        assert!(f.busy, "an interrupt ACK alone cannot prove termination");
        assert!(f.cancel_requested);
        assert!(f.cancel_started.is_some());
        assert!(b.take_outbound().is_none());
        b.ingest_line(
            json!({"method":"turn/completed","params":{
            "threadId":"thread-a","turn":{"id":"turn-wrong","status":"interrupted"}}})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
        let forged = b.take_event().unwrap();
        assert_eq!(f.on_event(&mut b, forged).unwrap(), None);
        assert!(f.busy);
        b.ingest_line(
            json!({"method":"turn/completed","params":{
            "threadId":"thread-a","turn":{"id":"turn-a","status":"interrupted"}}})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
        let terminal = b.take_event().unwrap();
        let expected = ProbeEvent::TurnCompleted {
            thread_id: "thread-a".into(),
            turn_id: "turn-a".into(),
            status: "interrupted".into(),
        };
        assert_eq!(
            f.on_event(&mut b, terminal).unwrap(),
            Some(expected.clone())
        );
        assert!(!f.busy);
        let late_start_reply = reply(&mut b, start_id, json!({"turn":{"id":"turn-a"}}));
        assert_eq!(f.on_event(&mut b, late_start_reply).unwrap(), None);
        assert_eq!(f.on_event(&mut b, Event::Notification {
            kind:NotificationKind::TurnCompleted,
            params:json!({"threadId":"thread-a","turn":{"id":"turn-a","status":"interrupted"}}),
        }).unwrap(), None, "duplicate terminal must not be re-emitted");
        let mut display = DisplayEvents::default();
        display.push(expected.clone()).unwrap();
        assert_eq!(
            display.take(),
            vec![expected],
            "UI queue preserves a verified interrupted completion"
        );
    }

    #[test]
    fn rejected_interrupt_waits_for_official_terminal_or_bounded_shutdown() {
        let mut broker = ready();
        let mut lifecycle = TurnLifecycle {
            thread: Some("thread-a".into()),
            ..Default::default()
        };
        lifecycle
            .submit(&mut broker, Path::new("C:/project"), "synthetic".into())
            .unwrap();
        broker.take_outbound().unwrap();
        let started = Event::Notification {
            kind: NotificationKind::TurnStarted,
            params: json!({"threadId":"thread-a","turn":{"id":"turn-a"}}),
        };
        assert!(matches!(
            lifecycle.on_event(&mut broker, started),
            Ok(Some(ProbeEvent::TurnStarted { .. }))
        ));
        lifecycle.cancel(&mut broker).unwrap();
        let interrupt: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        broker
            .ingest_line(
                json!({"id":interrupt["id"],"error":{"code":-32602,"message":"no active turn"}})
                    .to_string()
                    .as_bytes(),
            )
            .unwrap();
        let rejected = broker.take_event().unwrap();
        assert_eq!(lifecycle.on_event(&mut broker, rejected), Ok(None));
        assert!(lifecycle.busy);
        assert!(lifecycle.cancel_requested);
        assert!(lifecycle.cancel_started.is_some());
        assert!(broker.take_outbound().is_none(), "no second interrupt");

        // A rejection never fabricates the status 'interrupted'.
        let completed = Event::Notification {
            kind: NotificationKind::TurnCompleted,
            params: json!({"threadId":"thread-a","turn":{"id":"turn-a","status":"completed"}}),
        };
        assert_eq!(
            lifecycle.on_event(&mut broker, completed),
            Ok(Some(ProbeEvent::TurnCompleted {
                thread_id: "thread-a".into(),
                turn_id: "turn-a".into(),
                status: "completed".into(),
            }))
        );
        assert!(!lifecycle.busy);

        // If the server never emits a terminal, the already-installed 20s
        // watchdog still forces the process-tree shutdown.
        lifecycle
            .submit(&mut broker, Path::new("C:/project"), "another".into())
            .unwrap();
        broker.take_outbound().unwrap();
        lifecycle.cancel(&mut broker).unwrap();
        lifecycle.cancel_started = Some(Instant::now() - CANCEL_TIMEOUT - Duration::from_secs(1));
        assert_eq!(
            lifecycle.timeout_error(Instant::now()),
            Some("Codex did not acknowledge cancellation; owned process stopped.")
        );
    }

    #[test]
    fn terminal_before_start_reply_is_deferred_until_matching_identity() {
        let mut b = ready();
        let mut f = TurnLifecycle {
            thread: Some("thread-a".into()),
            ..Default::default()
        };
        f.submit(&mut b, Path::new("C:/project"), "synthetic".into())
            .unwrap();
        let start: Value = serde_json::from_slice(&b.take_outbound().unwrap()).unwrap();
        f.cancel(&mut b).unwrap();
        let early = Event::Notification {
            kind: NotificationKind::TurnCompleted,
            params: json!({"threadId":"thread-a","turn":{"id":"turn-a","status":"interrupted"}}),
        };
        assert_eq!(f.on_event(&mut b, early).unwrap(), None);
        assert!(f.busy);
        assert!(
            b.take_outbound().is_none(),
            "no ID means no interrupt request"
        );
        let reply = reply(
            &mut b,
            start["id"].as_u64().unwrap(),
            json!({"turn":{"id":"turn-a"}}),
        );
        assert_eq!(
            f.on_event(&mut b, reply).unwrap(),
            Some(ProbeEvent::TurnCompleted {
                thread_id: "thread-a".into(),
                turn_id: "turn-a".into(),
                status: "interrupted".into(),
            })
        );
        assert!(!f.busy);
        assert!(
            b.take_outbound().is_none(),
            "must not interrupt a turn already reported finished"
        );
    }

    #[test]
    fn broker_queue_and_ui_recover_from_terminal_before_turn_start_reply() {
        let mut broker = ready();
        let mut lifecycle = TurnLifecycle {
            thread: Some("thread-a".into()),
            ..Default::default()
        };
        let mut display = DisplayEvents::default();
        let mut app = crate::RendererSpike::new(0, false, None, None);
        app.awaiting_turn_start = true;
        app.push_live_row(true, "synthetic prompt".into());

        lifecycle
            .submit(
                &mut broker,
                Path::new("C:/project"),
                "synthetic prompt".into(),
            )
            .unwrap();
        let request: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        broker
            .ingest_line(
                json!({"method":"turn/completed","params":{
                    "threadId":"thread-a","turn":{"id":"turn-a","status":"failed",
                    "error":{"codexErrorInfo":"usageLimitExceeded", "message":"private details"}}}})
                .to_string()
                .as_bytes(),
            )
            .unwrap();
        let early = broker.take_event().unwrap();
        assert_eq!(lifecycle.on_event(&mut broker, early).unwrap(), None);
        assert!(display.take().is_empty(), "unverified terminal is withheld");

        let start_reply = reply(
            &mut broker,
            request["id"].as_u64().unwrap(),
            json!({"turn":{"id":"turn-a"}}),
        );
        let verified = lifecycle
            .on_event(&mut broker, start_reply)
            .unwrap()
            .unwrap();
        display.push(verified).unwrap();
        if let Some(diagnostic) = lifecycle.take_terminal_error() {
            display
                .push(ProbeEvent::Error(diagnostic.to_owned()))
                .unwrap();
        }
        for event in display.take() {
            app.apply_probe_event(event);
        }
        assert!(!app.awaiting_turn_start, "verified terminal unlocks input");
        assert!(app.active_turn.is_none());
        assert_eq!(app.last_turn_status.as_deref(), Some("failed"));
        assert_eq!(
            app.live_error.as_deref(),
            Some("Codex reached its upstream usage or rate limit.")
        );
        assert_eq!(app.live_rows.len(), 2);
        assert!(!app.live_rows[1].preview.contains("private details"));
        assert!(!lifecycle.busy);
    }

    #[test]
    fn stale_early_terminal_does_not_falsely_complete_or_interrupt_another_turn() {
        let mut b = ready();
        let mut f = TurnLifecycle {
            thread: Some("thread-a".into()),
            ..Default::default()
        };
        f.submit(&mut b, Path::new("C:/project"), "synthetic".into())
            .unwrap();
        let start: Value = serde_json::from_slice(&b.take_outbound().unwrap()).unwrap();
        f.cancel(&mut b).unwrap();
        let stale = Event::Notification {
            kind: NotificationKind::TurnCompleted,
            params: json!({"threadId":"thread-a","turn":{"id":"old-turn","status":"interrupted"}}),
        };
        assert_eq!(f.on_event(&mut b, stale).unwrap(), None);
        let started = reply(
            &mut b,
            start["id"].as_u64().unwrap(),
            json!({"turn":{"id":"new-turn"}}),
        );
        assert_eq!(
            f.on_event(&mut b, started).unwrap(),
            Some(ProbeEvent::TurnStarted {
                thread_id: "thread-a".into(),
                turn_id: "new-turn".into(),
            })
        );
        let interrupt: Value = serde_json::from_slice(&b.take_outbound().unwrap()).unwrap();
        assert_eq!(interrupt["method"], "turn/interrupt");
        assert_eq!(interrupt["params"]["turnId"], "new-turn");
        assert!(f.busy);
    }

    #[test]
    fn early_terminal_confirmed_by_turn_started_does_not_send_late_interrupt() {
        let mut b = ready();
        let mut f = TurnLifecycle {
            thread: Some("thread-a".into()),
            ..Default::default()
        };
        f.submit(&mut b, Path::new("C:/project"), "synthetic".into())
            .unwrap();
        let start: Value = serde_json::from_slice(&b.take_outbound().unwrap()).unwrap();
        f.cancel(&mut b).unwrap();
        assert_eq!(f.on_event(&mut b, Event::Notification {
            kind:NotificationKind::TurnCompleted,
            params:json!({"threadId":"thread-a","turn":{"id":"turn-a","status":"completed"}}),
        }).unwrap(), None);
        assert_eq!(f.on_event(&mut b, Event::Notification {
            kind:NotificationKind::TurnStarted,
            params:json!({"threadId":"thread-a","turn":{"id":"turn-a","status":"inProgress"}}),
        }).unwrap(), Some(ProbeEvent::TurnCompleted {
            thread_id:"thread-a".into(),turn_id:"turn-a".into(),status:"completed".into(),
        }), "Stop is a request, not proof upstream returned interrupted");
        assert!(b.take_outbound().is_none());
        assert!(!f.busy);
        let late_reply = reply(
            &mut b,
            start["id"].as_u64().unwrap(),
            json!({"turn":{"id":"turn-a"}}),
        );
        assert_eq!(f.on_event(&mut b, late_reply).unwrap(), None);
    }

    #[test]
    fn late_ui_stop_after_authoritative_completion_never_overwrites_terminal() {
        let mut b = ready();
        let mut f = TurnLifecycle::default();
        let mut display = DisplayEvents::default();
        f.thread = Some("thread-a".into());
        f.submit(&mut b, Path::new("C:/project"), "synthetic".into())
            .unwrap();
        let start: Value = serde_json::from_slice(&b.take_outbound().unwrap()).unwrap();
        let response = reply(
            &mut b,
            start["id"].as_u64().unwrap(),
            json!({"turn":{"id":"turn-a"}}),
        );
        let started = f.on_event(&mut b, response).unwrap().unwrap();
        display.push(started).unwrap();
        // The server has already completed by the time the user's Stop
        // command is dispatched from the bounded UI mpsc queue.
        let terminal = f.on_event(&mut b, Event::Notification {
            kind:NotificationKind::TurnCompleted,
            params:json!({"threadId":"thread-a","turn":{"id":"turn-a","status":"completed"}}),
        }).unwrap().unwrap();
        display.push(terminal.clone()).unwrap();
        assert_eq!(f.cancel(&mut b), Ok(()));
        assert!(!f.cancel_requested);
        assert!(
            b.take_outbound().is_none(),
            "too-late Stop sends no turn/interrupt"
        );
        assert_eq!(display.take().last(), Some(&terminal));
        // Duplicate Stop does not put a misleading error into the UI queue.
        assert_eq!(f.cancel(&mut b), Ok(()));
        assert!(display.take().is_empty());
    }

    #[test]
    fn server_permission_or_project_downgrade_fails_before_any_turn() {
        let project = std::env::current_dir().unwrap();
        for change in [
            "approval",
            "sandbox",
            "network",
            "cwd",
            "thread-cwd",
            "missing",
        ] {
            let mut b = ready();
            let mut f = TurnLifecycle::default();
            f.submit(&mut b, &project, "do not run".into()).unwrap();
            b.take_outbound().unwrap();
            let mut response = created(&project);
            match change {
                "approval" => response["approvalPolicy"] = json!("on-request"),
                "sandbox" => response["sandbox"]["type"] = json!("workspaceWrite"),
                "network" => response["sandbox"]["networkAccess"] = json!(true),
                "cwd" => response["cwd"] = json!("C:/nonexistent-project"),
                "thread-cwd" => response["thread"]["cwd"] = json!("C:/nonexistent-project"),
                "missing" => {
                    response.as_object_mut().unwrap().remove("sandbox");
                }
                _ => unreachable!(),
            }
            let rpc = reply(&mut b, 1, response);
            assert!(f.on_event(&mut b, rpc).is_err(), "{change}");
            assert!(
                b.take_outbound().is_none(),
                "{change}: must not send a turn"
            );
        }
    }

    #[test]
    fn malformed_turn_response_is_fatal_even_if_notification_arrived() {
        let mut b = ready();
        let mut f = TurnLifecycle {
            thread: Some("thread-a".into()),
            ..Default::default()
        };
        f.submit(&mut b, Path::new("C:/project"), "hi".into())
            .unwrap();
        b.take_outbound().unwrap();
        let started = Event::Notification {
            kind: NotificationKind::TurnStarted,
            params: json!({"threadId":"thread-a","turn":{"id":"turn-a"}}),
        };
        assert!(f.on_event(&mut b, started).unwrap().is_some());
        let answered = reply(&mut b, 1, json!({"turn":{"id":"turn-other"}}));
        assert_eq!(
            f.on_event(&mut b, answered),
            Err("Codex returned conflicting turn identities.")
        );
        assert!(f.busy);
    }

    #[test]
    fn final_item_and_failure_report_are_strict_and_redacted() {
        let mut b = ready();
        let mut f = TurnLifecycle {
            thread: Some("thread-a".into()),
            ..Default::default()
        };
        f.submit(&mut b, Path::new("C:/project"), "hi".into())
            .unwrap();
        b.take_outbound().unwrap();
        f.on_event(
            &mut b,
            Event::Notification {
                kind: NotificationKind::TurnStarted,
                params: json!({"threadId":"thread-a","turn":{"id":"turn-a"}}),
            },
        )
        .unwrap();
        let wrong_item = Event::Notification {
            kind: NotificationKind::ItemCompleted,
            params: json!({"threadId":"thread-a","turnId":"wrong","item":{"type":"agentMessage","id":"msg-x","text":"untrusted"}}),
        };
        assert_eq!(f.on_event(&mut b, wrong_item).unwrap(), None);
        let final_item = Event::Notification {
            kind: NotificationKind::ItemCompleted,
            params: json!({"threadId":"thread-a","turnId":"turn-a","item":{"type":"agentMessage","id":"msg-a","text":"official"}}),
        };
        assert_eq!(
            f.on_event(&mut b, final_item).unwrap(),
            Some(ProbeEvent::AssistantFinal {
                thread_id: "thread-a".into(),
                turn_id: "turn-a".into(),
                text: "official".into()
            })
        );
        let completed = Event::Notification {
            kind: NotificationKind::TurnCompleted,
            params: json!({"threadId":"thread-a","turn":{"id":"turn-a","status":"failed",
                "error":{"codexErrorInfo":"usageLimitExceeded","message":"secret-account-id-1234"}}}),
        };
        assert_eq!(
            f.on_event(&mut b, completed).unwrap(),
            Some(ProbeEvent::TurnCompleted {
                thread_id: "thread-a".into(),
                turn_id: "turn-a".into(),
                status: "failed".into()
            })
        );
        assert_eq!(
            f.take_terminal_error(),
            Some("Codex reached its upstream usage or rate limit.")
        );
        assert_eq!(f.take_terminal_error(), None);
        assert!(!f.busy);
    }

    #[test]
    fn request_and_cancellation_watchdogs_expire_without_live_codex() {
        let mut b = ready();
        let mut f = TurnLifecycle::default();
        let now = Instant::now();
        f.submit(&mut b, Path::new("C:/project"), "test".into())
            .unwrap();
        assert_eq!(f.timeout_error(now), None);
        f.rpc_started = Some(now - RPC_TIMEOUT - Duration::from_secs(1));
        assert_eq!(
            f.timeout_error(now),
            Some("Codex did not answer the thread/turn request; owned process stopped.")
        );

        f.rpc_started = None;
        f.cancel(&mut b).unwrap();
        f.cancel_started = Some(now - CANCEL_TIMEOUT - Duration::from_secs(1));
        assert_eq!(
            f.timeout_error(now),
            Some("Codex did not acknowledge cancellation; owned process stopped.")
        );
        // Repeated cancel never refreshes its clock or queues an unknown ID.
        f.cancel(&mut b).unwrap();
        assert!(b.take_outbound().is_some()); // Original thread creation RPC only.
        assert!(b.take_outbound().is_none());
    }

    #[test]
    fn a_full_ui_queue_makes_cancel_request_shutdown() {
        let (commands, _receiver) = mpsc::channel(1);
        commands
            .try_send(Command::Submit("existing".into()))
            .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let probe = EngineProbe {
            status: Arc::new(AtomicU8::new(ProbeStatus::Ready as u8)),
            commands,
            emergency_stop: stop.clone(),
            events: Arc::new(Mutex::new(DisplayEvents::default())),
            shutdown: None,
            thread: None,
        };
        assert_eq!(
            probe.cancel(),
            Err("Codex cancellation queue unavailable; stopping its process.")
        );
        assert!(stop.load(Ordering::Acquire));
    }

    #[test]
    fn stream_queue_is_bounded_and_invalid_events_never_contaminate() {
        let mut mailbox = DisplayEvents::default();
        let event = || ProbeEvent::AssistantDelta {
            thread_id: "thread-a".into(),
            turn_id: "turn-a".into(),
            text: "x".repeat(4096),
        };
        for _ in 0..60 {
            mailbox.push(event()).unwrap();
        }
        assert!(mailbox.bytes < MAX_UI_EVENT_BYTES);
        assert_eq!(mailbox.take().len(), 1);
        for _ in 0..MAX_UI_EVENTS {
            mailbox.push(ProbeEvent::Error("error".into())).unwrap();
        }
        assert!(mailbox.push(ProbeEvent::Error("overflow".into())).is_err());
        mailbox.fatal("Native stream display buffer exceeded its limit.");
        assert_eq!(
            mailbox.take(),
            vec![ProbeEvent::Error(
                "Native stream display buffer exceeded its limit.".into()
            )]
        );
        // A full-size canonical reply can supersede an undelivered full-size
        // delta without doubling the bounded UI memory requirement.
        let big = "z".repeat(MAX_ASSISTANT_BYTES);
        mailbox
            .push(ProbeEvent::AssistantDelta {
                thread_id: "thread-a".into(),
                turn_id: "turn-a".into(),
                text: big.clone(),
            })
            .unwrap();
        mailbox
            .push(ProbeEvent::AssistantFinal {
                thread_id: "thread-a".into(),
                turn_id: "turn-a".into(),
                text: big,
            })
            .unwrap();
        assert_eq!(mailbox.take().len(), 1);
    }

    #[test]
    fn coalesced_stream_retains_oldest_mailbox_timestamp_and_count() {
        let mut mailbox = DisplayEvents::default();
        let before = Instant::now() - Duration::from_millis(125);
        let after = Instant::now();
        let delta = |text: &str| ProbeEvent::AssistantDelta {
            thread_id: "t".into(),
            turn_id: "turn".into(),
            text: text.into(),
        };
        mailbox.push_at(delta("hello"), before).unwrap();
        mailbox.push_at(delta(" world"), after).unwrap();
        let results = mailbox.take_timed();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].received_at, before);
        assert_eq!(results[0].coalesced, 1);
        assert!(
            matches!(&results[0].event, ProbeEvent::AssistantDelta { text, .. } if text == "hello world")
        );
    }
    #[test]
    fn coding_policy_requires_exact_effective_permissions() {
        let project = std::env::current_dir().unwrap();
        let mut response = created(&project);
        assert!(accepted_thread(&response, &project, true).is_err());
        response["approvalPolicy"] = json!("on-request");
        response["sandbox"] = json!({"type":"workspaceWrite","networkAccess":false});
        assert_eq!(
            accepted_thread(&response, &project, true),
            Ok("thread-a".to_owned())
        );
        response["sandbox"]["networkAccess"] = json!(true);
        assert!(accepted_thread(&response, &project, true).is_err());
        response["sandbox"]["networkAccess"] = json!(false);
        response["thread"]["cwd"] = json!("/foreign/path");
        assert!(accepted_thread(&response, &project, true).is_err());
    }

    #[test]
    fn coding_approval_requires_live_tool_evidence_and_single_exact_rpc() {
        let project = std::env::current_dir().unwrap();
        let mut broker = ready();
        let mut lifecycle = TurnLifecycle {
            thread: Some("thread-a".to_owned()),
            turn: Some("turn-a".to_owned()),
            project: Some(project.clone()),
            busy: true,
            coding_enabled: true,
            ..Default::default()
        };
        let tool = lifecycle
            .on_event(
                &mut broker,
                Event::Notification {
                    kind: NotificationKind::ItemStarted,
                    params: json!({
                        "threadId":"thread-a","turnId":"turn-a",
                        "item":{"id":"item-a","type":"commandExecution","command":"echo reviewed","cwd":project.to_str().unwrap()}
                    }),
                },
            )
            .unwrap();
        let Some(ProbeEvent::ToolSnapshot(reviewed)) = tool else {
            panic!("Missing actual reviewed tool evidence");
        };
        broker
            .ingest_line(
                json!({"id":"request-a","method":"item/commandExecution/requestApproval",
            "params":{"threadId":"thread-a","turnId":"turn-a","itemId":"item-a",
                "startedAtMs":1,"command":"echo reviewed","cwd":project.to_str().unwrap()}})
                .to_string()
                .as_bytes(),
            )
            .unwrap();
        let event = broker.take_event().unwrap();
        let pending = lifecycle.on_event(&mut broker, event).unwrap();
        let Some(ProbeEvent::ApprovalPending(approval)) = pending else {
            panic!("Expected one exact pending approval");
        };
        let mut forged = approval.clone();
        forged.item_id = "not-the-reviewed-tool".to_owned();
        assert!(lifecycle
            .decide(&mut broker, &forged, true, Some(&reviewed))
            .is_err());
        assert!(broker.take_outbound().is_none());
        assert!(matches!(
            lifecycle.decide(&mut broker, &approval, true, Some(&reviewed)),
            Ok(Some(ProbeEvent::ApprovalResolved(_)))
        ));
        let outbound: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        assert_eq!(outbound["id"], "request-a");
        assert_eq!(outbound["result"]["decision"], "accept");
        assert!(lifecycle
            .decide(&mut broker, &approval, true, Some(&reviewed))
            .is_err());
        assert!(broker.take_outbound().is_none());
    }

    #[test]
    fn official_local_fixture_dispatches_accept_once_and_denies_environment_substitution() {
        let project = std::env::current_dir().unwrap();
        let mut fixture: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/nc10g-local-approval.json"
        ))
        .unwrap();
        fixture["params"]["cwd"] = json!(project.to_str().unwrap());
        let mut broker = ready();
        let mut lifecycle = TurnLifecycle {
            thread: Some("t".into()),
            turn: Some("u".into()),
            project: Some(project.clone()),
            busy: true,
            coding_enabled: true,
            ..Default::default()
        };
        let Some(ProbeEvent::ToolSnapshot(reviewed)) = lifecycle.on_event(&mut broker,
            Event::Notification { kind: NotificationKind::ItemStarted,
                params: json!({"threadId":"t","turnId":"u","item":{"id":"i",
                    "type":"commandExecution","command":"echo safe","cwd":project.to_str().unwrap()}})
            }).unwrap() else { panic!("tool"); };
        broker
            .ingest_line(&serde_json::to_vec(&fixture).unwrap())
            .unwrap();
        let event = broker.take_event().unwrap();
        let Some(ProbeEvent::ApprovalPending(approval)) =
            lifecycle.on_event(&mut broker, event).unwrap()
        else {
            panic!("approval");
        };
        for env in [
            json!("remote"),
            Value::Null,
            json!(""),
            json!(true),
            json!("x".repeat(8193)),
        ] {
            let mut changed = approval.clone();
            changed.request["environmentId"] = env;
            assert!(lifecycle
                .decide(&mut broker, &changed, true, Some(&reviewed))
                .is_err());
            assert!(broker.take_outbound().is_none());
        }
        for (field, value) in [
            ("threadId", json!("foreign")),
            ("turnId", json!("foreign")),
            ("itemId", json!("foreign")),
            ("startedAtMs", json!(2)),
            ("unexpectedField", json!(true)),
            ("additionalPermissions", json!({"network":true})),
            ("proposedExecpolicyAmendment", json!([1])),
            ("availableDecisions", json!(["acceptForSession"])),
        ] {
            let mut changed = approval.clone();
            changed.request[field] = value;
            assert!(lifecycle
                .decide(&mut broker, &changed, true, Some(&reviewed))
                .is_err());
            assert!(broker.take_outbound().is_none());
        }
        assert!(lifecycle
            .decide(&mut broker, &approval, true, Some(&reviewed))
            .is_ok());
        let wire: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        assert_eq!(
            wire,
            json!({"id":"local-once","result":{"decision":"accept"}})
        );
        assert!(lifecycle
            .decide(&mut broker, &approval, true, Some(&reviewed))
            .is_err());
        assert!(broker.take_outbound().is_none());
        // A new lifecycle/session has neither pending consent nor evidence.
        let mut new_broker = ready();
        let mut new_lifecycle = TurnLifecycle {
            thread: Some("t".into()),
            turn: Some("u".into()),
            project: Some(project),
            busy: true,
            coding_enabled: true,
            ..Default::default()
        };
        assert!(new_lifecycle
            .decide(&mut new_broker, &approval, true, Some(&reviewed))
            .is_err());
        assert!(new_broker.take_outbound().is_none());
    }

    #[test]
    fn resolved_request_cannot_be_approved_after_provider_revoke() {
        let project = std::env::current_dir().unwrap();
        let mut broker = ready();
        let mut lifecycle = TurnLifecycle {
            thread: Some("thread-a".to_owned()),
            turn: Some("turn-a".to_owned()),
            project: Some(project.clone()),
            busy: true,
            coding_enabled: true,
            ..Default::default()
        };
        let tool = lifecycle
            .on_event(
                &mut broker,
                Event::Notification {
                    kind: NotificationKind::ItemStarted,
                    params: json!({
                        "threadId":"thread-a","turnId":"turn-a",
                        "item":{"id":"item-a","type":"commandExecution","command":"echo reviewed"}
                    }),
                },
            )
            .unwrap();
        let Some(ProbeEvent::ToolSnapshot(reviewed)) = tool else {
            panic!("Missing actual reviewed tool evidence");
        };
        broker.ingest_line(br#"{"id":"request-a","method":"item/commandExecution/requestApproval","params":{"threadId":"thread-a","turnId":"turn-a","itemId":"item-a","startedAtMs":1}}"#).unwrap();
        let arrival = broker.take_event().unwrap();
        let Some(ProbeEvent::ApprovalPending(pending)) =
            lifecycle.on_event(&mut broker, arrival).unwrap()
        else {
            panic!("Expected pending approval");
        };
        broker
            .ingest_line(
                br#"{"method":"serverRequest/resolved","params":{"requestId":"request-a"}}"#,
            )
            .unwrap();
        let event = broker.take_event().unwrap();
        assert!(matches!(
            lifecycle.on_event(&mut broker, event),
            Ok(Some(ProbeEvent::ApprovalResolved(_)))
        ));
        assert!(lifecycle
            .decide(&mut broker, &pending, true, Some(&reviewed))
            .is_err());
        assert!(broker.take_outbound().is_none());
    }
    #[test]
    fn official_history_restart_resume_switch_and_page_do_not_start_tools() {
        let project = std::env::current_dir().unwrap();
        let cwd = project.to_str().unwrap();
        let mut broker = ready();
        let mut flow = HistoryFlow::default();
        let mut lifecycle = TurnLifecycle::default();

        // First launch: only an official read-only thread/list RPC.
        flow.list(&mut broker, &project, false).unwrap();
        let request: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        assert_eq!(request["method"], "thread/list");
        assert_eq!(request["params"]["cwd"], cwd);
        assert_eq!(request["params"]["limit"], 24);
        let event = reply(
            &mut broker,
            request["id"].as_u64().unwrap(),
            json!({
                "data":[
                    {"id":"a","cwd":cwd,"preview":"First","ephemeral":false},
                    {"id":"b","cwd":cwd,"preview":"Second","ephemeral":false}
                ],
                "nextCursor":null
            }),
        );
        assert!(matches!(
            flow.handle_reply(&mut broker, &event, &mut lifecycle, &project).unwrap(),
            Some(Some(ProbeEvent::ThreadPage {entries, ..})) if entries.len() == 2
        ));

        // Exact-ID metadata read must precede resume, and resume MUST exclude
        // full-history hydration. No automatic turn/start is allowed.
        flow.select(&mut broker, "a".into()).unwrap();
        let read: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        assert_eq!(read["method"], "thread/read");
        assert_eq!(read["params"]["includeTurns"], false);
        let event = reply(
            &mut broker,
            read["id"].as_u64().unwrap(),
            json!({"thread":{"id":"a","cwd":cwd}}),
        );
        assert!(matches!(
            flow.handle_reply(&mut broker, &event, &mut lifecycle, &project),
            Ok(Some(None))
        ));
        let resume: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        assert_eq!(resume["method"], "thread/resume");
        assert_eq!(resume["params"]["excludeTurns"], true);
        assert_eq!(resume["params"]["sandbox"], "read-only");
        assert_eq!(resume["params"]["approvalPolicy"], "never");
        let event = reply(
            &mut broker,
            resume["id"].as_u64().unwrap(),
            json!({"thread":{"id":"a","cwd":cwd},"cwd":cwd,"approvalPolicy":"never","sandbox":{"type":"readOnly","networkAccess":false}}),
        );
        assert!(matches!(
            flow.handle_reply(&mut broker, &event, &mut lifecycle, &project),
            Ok(Some(Some(ProbeEvent::ThreadSelected {
                thread_id: Some(_)
            })))
        ));
        assert_eq!(lifecycle.thread.as_deref(), Some("a"));
        let turns: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        assert_eq!(turns["method"], "thread/turns/list");
        assert_eq!(turns["params"]["itemsView"], "full");
        let event = reply(
            &mut broker,
            turns["id"].as_u64().unwrap(),
            json!({"data":[
            {"id":"t1","status":"interrupted","items":[
                {"type":"userMessage","content":[{"type":"text","text":"original prompt"}]}
            ]}
        ],"nextCursor":"older"}),
        );
        assert!(matches!(
            flow.handle_reply(&mut broker, &event, &mut lifecycle, &project),
            Ok(Some(Some(ProbeEvent::HistoryPage { has_more: true, .. })))
        ));
        assert_eq!(broker.pending_count(), 0);
        assert!(broker.take_outbound().is_none()); // no replay of interrupted turn

        flow.turns(&mut broker, "a", true).unwrap();
        let older: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        assert_eq!(older["params"]["cursor"], "older");
        let event = reply(
            &mut broker,
            older["id"].as_u64().unwrap(),
            json!({"data":[],"nextCursor":null}),
        );
        flow.handle_reply(&mut broker, &event, &mut lifecycle, &project)
            .unwrap();

        // Switch: only known IDs from the verified current-project listing.
        assert!(flow.select(&mut broker, "foreign".into()).is_err());
        flow.select(&mut broker, "b".into()).unwrap();
        let next: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        assert_eq!(next["method"], "thread/read");
        let event = reply(
            &mut broker,
            next["id"].as_u64().unwrap(),
            json!({"thread":{"id":"b","cwd":cwd}}),
        );
        flow.handle_reply(&mut broker, &event, &mut lifecycle, &project)
            .unwrap();
        let resumed: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        let event = reply(
            &mut broker,
            resumed["id"].as_u64().unwrap(),
            json!({"thread":{"id":"b","cwd":cwd},"cwd":cwd,"approvalPolicy":"never","sandbox":{"type":"readOnly","networkAccess":false}}),
        );
        flow.handle_reply(&mut broker, &event, &mut lifecycle, &project)
            .unwrap();
        assert_eq!(lifecycle.thread.as_deref(), Some("b"));

        // Restart/new process creates a fresh broker+flow, rehydrating from
        // official thread/list; it does not persist or execute another prompt.
        let mut after_restart = HistoryFlow::default();
        let mut fresh_broker = ready();
        after_restart
            .list(&mut fresh_broker, &project, false)
            .unwrap();
        let list: Value = serde_json::from_slice(&fresh_broker.take_outbound().unwrap()).unwrap();
        assert_eq!(list["method"], "thread/list");
    }

    #[test]
    fn resumed_active_turn_blocks_duplicate_execution_until_exact_terminal() {
        let project = std::env::current_dir().unwrap();
        let cwd = project.to_str().unwrap();
        let mut broker = ready();
        let mut flow = HistoryFlow::default();
        let mut lifecycle = TurnLifecycle::default();
        flow.known.push(ThreadEntry {
            id: "owned".into(),
            label: "Trusted".into(),
        });

        flow.select(&mut broker, "owned".into()).unwrap();
        let read: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        let event = reply(
            &mut broker,
            read["id"].as_u64().unwrap(),
            json!({"thread":{"id":"owned","cwd":cwd}}),
        );
        flow.handle_reply(&mut broker, &event, &mut lifecycle, &project)
            .unwrap();
        let resume: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        let event = reply(
            &mut broker,
            resume["id"].as_u64().unwrap(),
            json!({"thread":{"id":"owned","cwd":cwd},"cwd":cwd,
                "approvalPolicy":"never",
                "sandbox":{"type":"readOnly","networkAccess":false}}),
        );
        flow.handle_reply(&mut broker, &event, &mut lifecycle, &project)
            .unwrap();

        let turns: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        assert_eq!(turns["method"], "thread/turns/list");
        let event = reply(
            &mut broker,
            turns["id"].as_u64().unwrap(),
            json!({"data":[{"id":"running-turn","status":"inProgress","items":[]}],
                "nextCursor":null}),
        );
        assert!(matches!(
            flow.handle_reply(&mut broker, &event, &mut lifecycle, &project),
            Ok(Some(Some(ProbeEvent::HistoryPage {
                active_turn: Some(_),
                ..
            })))
        ));
        assert!(lifecycle.busy);
        assert_eq!(lifecycle.turn.as_deref(), Some("running-turn"));
        assert!(lifecycle.resumed_turn_started.is_some());
        assert_eq!(
            lifecycle.submit(&mut broker, &project, "never replay".into()),
            Err("Codex is already processing a turn.")
        );
        assert!(broker.take_outbound().is_none());

        let unrelated = lifecycle
            .on_event(
                &mut broker,
                Event::Notification {
                    kind: NotificationKind::TurnCompleted,
                    params: json!({"threadId":"owned",
                        "turn":{"id":"other","status":"completed"}}),
                },
            )
            .unwrap();
        assert!(unrelated.is_none());
        assert!(lifecycle.busy);
        assert!(broker.take_outbound().is_none());

        assert_eq!(
            lifecycle
                .on_event(
                    &mut broker,
                    Event::Notification {
                        kind: NotificationKind::TurnCompleted,
                        params: json!({"threadId":"owned",
                            "turn":{"id":"running-turn","status":"completed"}}),
                    },
                )
                .unwrap(),
            Some(ProbeEvent::TurnCompleted {
                thread_id: "owned".into(),
                turn_id: "running-turn".into(),
                status: "completed".into()
            })
        );
        assert!(!lifecycle.busy);
        assert!(lifecycle.resumed_turn_started.is_none());
        lifecycle
            .submit(&mut broker, &project, "new explicit prompt".into())
            .unwrap();
        let started: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        assert_eq!(started["method"], "turn/start");
        assert_eq!(started["params"]["threadId"], "owned");
        assert_eq!(started["params"]["input"][0]["text"], "new explicit prompt");
        assert!(broker.take_outbound().is_none());
    }

    // Model the real HistoryFlow -> Broker -> TurnLifecycle sequence. Every
    // fixture consumes the first thread/turns/list outbound frame so any
    // subsequently emitted turn/start is visible rather than hidden in a queue.
    fn resume_waiting_for_first_turn_page() -> (Broker, HistoryFlow, TurnLifecycle, PathBuf, u64) {
        let project = std::env::current_dir().unwrap();
        let cwd = project.to_str().unwrap();
        let mut broker = ready();
        let mut flow = HistoryFlow::default();
        let mut lifecycle = TurnLifecycle::default();
        flow.known.push(ThreadEntry {
            id: "owned".to_owned(),
            label: "Trusted".to_owned(),
        });
        flow.select(&mut broker, "owned".to_owned()).unwrap();
        let read: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        let event = reply(
            &mut broker,
            read["id"].as_u64().unwrap(),
            json!({"thread":{"id":"owned","cwd":cwd}}),
        );
        flow.handle_reply(&mut broker, &event, &mut lifecycle, &project)
            .unwrap();
        let resume: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        let event = reply(
            &mut broker,
            resume["id"].as_u64().unwrap(),
            json!({"thread":{"id":"owned","cwd":cwd},"cwd":cwd,
                "approvalPolicy":"never",
                "sandbox":{"type":"readOnly","networkAccess":false}}),
        );
        flow.handle_reply(&mut broker, &event, &mut lifecycle, &project)
            .unwrap();
        let turns: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        assert_eq!(turns["method"], "thread/turns/list");
        assert_eq!(turns["params"]["threadId"], "owned");
        assert_eq!(
            flow.pending.as_ref().unwrap().0,
            turns["id"].as_u64().unwrap()
        );
        assert_eq!(lifecycle.thread.as_deref(), Some("owned"));
        assert_eq!(lifecycle.resumed_unverified.as_deref(), Some("owned"));
        assert!(!lifecycle.busy); // busy=false MUST NOT authorize submit.
        assert!(broker.take_outbound().is_none());
        (
            broker,
            flow,
            lifecycle,
            project,
            turns["id"].as_u64().unwrap(),
        )
    }

    fn assert_resume_submit_blocked(
        broker: &mut Broker,
        lifecycle: &mut TurnLifecycle,
        project: &Path,
    ) {
        assert_eq!(
            lifecycle.submit(broker, project, "do not duplicate".into()),
            Err("Resumed Codex turn state is not verified; reconnect required.")
        );
        // The pending RPC has already been drained by the fixture. Zero
        // outbound turn/start frames is the authoritative negative proof.
        assert!(broker.take_outbound().is_none());
    }

    fn failed_first_page(
        broker: &mut Broker,
        flow: &mut HistoryFlow,
        lifecycle: &mut TurnLifecycle,
        project: &Path,
        id: u64,
        code: i64,
    ) -> Event {
        broker
            .ingest_line(
                json!({"id":id,"error":{"code":code,"message":"rejected"}})
                    .to_string()
                    .as_bytes(),
            )
            .unwrap();
        let event = broker.take_event().unwrap();
        assert_eq!(
            flow.handle_reply(broker, &event, lifecycle, project),
            Err("Codex resumed turn verification failed; owned process stopped.")
        );
        assert!(flow.pending.is_none());
        assert_eq!(lifecycle.resumed_unverified.as_deref(), Some("owned"));
        assert_resume_submit_blocked(broker, lifecycle, project);
        event
    }

    #[test]
    fn resume_first_page_json_rpc_error_never_emits_turn_start() {
        let (mut broker, mut flow, mut lifecycle, project, id) =
            resume_waiting_for_first_turn_page();
        assert_resume_submit_blocked(&mut broker, &mut lifecycle, &project);
        failed_first_page(&mut broker, &mut flow, &mut lifecycle, &project, id, -32000);
    }

    #[test]
    fn resume_first_page_unsupported_capability_cannot_unlock_submit() {
        let (mut broker, mut flow, mut lifecycle, project, id) =
            resume_waiting_for_first_turn_page();
        failed_first_page(&mut broker, &mut flow, &mut lifecycle, &project, id, -32601);
    }

    #[test]
    fn resume_first_page_malformed_incomplete_or_foreign_fails_closed() {
        let bad_pages = [
            json!({"data":[{"id":"turn","items":[]}],"nextCursor":null}),
            json!({"data":[{"id":"turn","status":"completed"}],"nextCursor":null}),
            json!({"data":[{"id":"turn","status":"unrecognized","items":[]}],"nextCursor":null}),
            json!({"threadId":"foreign","data":[],"nextCursor":null}),
            json!({"data":[],"nextCursor":false}),
            json!({"data":[{"id":"turn","status":"inProgress","items":[]}],
                "nextCursor":"x".repeat(2049)}),
        ];
        for page in bad_pages {
            let (mut broker, mut flow, mut lifecycle, project, id) =
                resume_waiting_for_first_turn_page();
            let event = reply(&mut broker, id, page);
            assert!(flow
                .handle_reply(&mut broker, &event, &mut lifecycle, &project)
                .is_err());
            assert_eq!(lifecycle.resumed_unverified.as_deref(), Some("owned"));
            assert_resume_submit_blocked(&mut broker, &mut lifecycle, &project);
        }
    }

    #[test]
    fn resume_missing_first_page_retains_gate_until_thirty_second_shutdown() {
        let (mut broker, mut flow, mut lifecycle, project, _) =
            resume_waiting_for_first_turn_page();
        assert_eq!(flow.timeout_error(Instant::now()), None);
        flow.pending.as_mut().unwrap().2 =
            Instant::now() - HISTORY_TIMEOUT - Duration::from_secs(1);
        assert_eq!(
            flow.timeout_error(Instant::now()),
            Some("Codex history RPC timed out; owned process stopped.")
        );
        assert_resume_submit_blocked(&mut broker, &mut lifecycle, &project);
    }

    #[test]
    fn stale_failed_resume_reply_cannot_reactivate_adopted_thread() {
        let (mut broker, mut flow, mut lifecycle, project, id) =
            resume_waiting_for_first_turn_page();
        failed_first_page(&mut broker, &mut flow, &mut lifecycle, &project, id, -32601);
        // A delayed SUCCESS for the same first page cannot revive a failed
        // session after its pending request has been invalidated.
        let late_success = Event::Reply {
            id,
            method: ClientMethod::ThreadTurnsList,
            outcome: Ok(json!({"data":[],"nextCursor":null})),
        };
        assert!(matches!(
            flow.handle_reply(&mut broker, &late_success, &mut lifecycle, &project),
            Ok(None)
        ));
        assert_eq!(lifecycle.resumed_unverified.as_deref(), Some("owned"));
        assert_resume_submit_blocked(&mut broker, &mut lifecycle, &project);
    }

    #[test]
    fn resumed_active_first_page_stays_busy_without_duplicate_execution() {
        let (mut broker, mut flow, mut lifecycle, project, id) =
            resume_waiting_for_first_turn_page();
        let event = reply(
            &mut broker,
            id,
            json!({"data":[{"id":"running","status":"inProgress","items":[]}],
                "nextCursor":null}),
        );
        assert!(matches!(
            flow.handle_reply(&mut broker, &event, &mut lifecycle, &project),
            Ok(Some(Some(ProbeEvent::HistoryPage {
                active_turn: Some(_),
                ..
            })))
        ));
        assert!(lifecycle.resumed_unverified.is_none());
        assert!(lifecycle.busy);
        assert_eq!(lifecycle.turn.as_deref(), Some("running"));
        assert!(lifecycle.resumed_turn_started.is_some());
        assert_eq!(
            lifecycle.submit(&mut broker, &project, "duplicate".into()),
            Err("Codex is already processing a turn.")
        );
        assert!(broker.take_outbound().is_none());
    }

    #[test]
    fn resumed_no_active_first_page_allows_new_explicit_prompt_only() {
        let (mut broker, mut flow, mut lifecycle, project, id) =
            resume_waiting_for_first_turn_page();
        let event = reply(
            &mut broker,
            id,
            json!({"data":[{"id":"previous","status":"completed","items":[]}],
                "nextCursor":null}),
        );
        assert!(matches!(
            flow.handle_reply(&mut broker, &event, &mut lifecycle, &project),
            Ok(Some(Some(ProbeEvent::HistoryPage {
                active_turn: None,
                ..
            })))
        ));
        assert!(lifecycle.resumed_unverified.is_none());
        assert!(!lifecycle.busy);
        assert!(broker.take_outbound().is_none()); // no automatic replay
        lifecycle
            .submit(&mut broker, &project, "explicit new prompt".into())
            .unwrap();
        let sent: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        assert_eq!(sent["method"], "turn/start");
        assert_eq!(sent["params"]["threadId"], "owned");
        assert_eq!(sent["params"]["input"][0]["text"], "explicit new prompt");
        assert!(broker.take_outbound().is_none());
    }

    #[test]
    fn explicit_reconnect_uses_fresh_broker_without_prompt_replay() {
        let (mut broker, mut flow, mut lifecycle, project, id) =
            resume_waiting_for_first_turn_page();
        failed_first_page(&mut broker, &mut flow, &mut lifecycle, &project, id, -32601);
        // EngineProbe::start creates new worker-owned broker/flow/lifecycle;
        // the failed session does not reuse its request IDs or prompt queue.
        let mut new_broker = ready();
        let mut new_flow = HistoryFlow::default();
        let mut new_lifecycle = TurnLifecycle::default();
        assert!(new_lifecycle.thread.is_none());
        assert!(new_lifecycle.resumed_unverified.is_none());
        new_flow.list(&mut new_broker, &project, false).unwrap();
        let sent: Value = serde_json::from_slice(&new_broker.take_outbound().unwrap()).unwrap();
        assert_eq!(sent["method"], "thread/list");
        assert!(new_broker.take_outbound().is_none());
        let event = reply(
            &mut new_broker,
            sent["id"].as_u64().unwrap(),
            json!({"data":[{"id":"owned","cwd":project.to_str().unwrap(),
                "ephemeral":false}],"nextCursor":null}),
        );
        new_flow
            .handle_reply(&mut new_broker, &event, &mut new_lifecycle, &project)
            .unwrap();
        assert!(new_broker.take_outbound().is_none());
        assert!(!new_lifecycle.busy);
    }

    #[test]
    fn resume_without_verified_sandbox_never_binds_a_thread() {
        let project = std::env::current_dir().unwrap();
        let cwd = project.to_str().unwrap();
        let mut broker = ready();
        let mut flow = HistoryFlow::default();
        flow.known.push(ThreadEntry {
            id: "owned".to_owned(),
            label: "Known".to_owned(),
        });
        let mut lifecycle = TurnLifecycle::default();
        flow.select(&mut broker, "owned".to_owned()).unwrap();
        let read: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        let event = reply(
            &mut broker,
            read["id"].as_u64().unwrap(),
            json!({"thread":{"id":"owned","cwd":cwd}}),
        );
        flow.handle_reply(&mut broker, &event, &mut lifecycle, &project)
            .unwrap();
        let resume: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        let event = reply(
            &mut broker,
            resume["id"].as_u64().unwrap(),
            json!({"thread":{"id":"owned","cwd":cwd},"cwd":cwd,
                "approvalPolicy":"on-request","sandbox":{"type":"workspaceWrite","networkAccess":true}}),
        );
        assert!(flow
            .handle_reply(&mut broker, &event, &mut lifecycle, &project)
            .is_err());
        assert!(lifecycle.thread.is_none());
        assert!(broker.take_outbound().is_none());
    }

    #[test]
    fn malformed_or_unsupported_official_history_fails_closed() {
        let project = std::env::current_dir().unwrap();
        let mut broker = ready();
        let mut flow = HistoryFlow::default();
        let mut lifecycle = TurnLifecycle::default();
        flow.list(&mut broker, &project, false).unwrap();
        let request: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        broker
            .ingest_line(
                json!({"id":request["id"],"error":{"code":-32601,"message":"Method not found"}})
                    .to_string()
                    .as_bytes(),
            )
            .unwrap();
        let event = broker.take_event().unwrap();
        assert!(matches!(
            flow.handle_reply(&mut broker, &event, &mut lifecycle, &project),
            Ok(Some(Some(ProbeEvent::HistoryCapabilityBlocked(_))))
        ));
        assert!(flow.known.is_empty());
        assert!(broker.take_outbound().is_none());
    }
}
