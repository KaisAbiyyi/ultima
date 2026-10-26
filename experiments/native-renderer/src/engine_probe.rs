//! Opt-in official Codex App Server client for the native-renderer experiment.
//! Transport, JSONL protocol, process-tree teardown and approval policy are
//! owned by the production Rust crates. There are no automatic tool approvals.
use eframe::egui;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};
use ultima_app_server_supervisor::{EngineConfig, EngineSupervisor, OutputEvent};
use ultima_native_session::SessionGate;
use ultima_protocol_bridge::{Broker, ClientMethod, Event, Limits, NotificationKind, Phase};

const MAX_PROMPT_BYTES: usize = 8 * 1024;
const MAX_ASSISTANT_BYTES: usize = 256 * 1024;
const MAX_UI_EVENTS: usize = 128;
const MAX_UI_EVENT_BYTES: usize = 512 * 1024;
const MAX_COMMANDS: usize = 8;
const RPC_TIMEOUT: Duration = Duration::from_secs(30);
const CANCEL_TIMEOUT: Duration = Duration::from_secs(20);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);

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
    Error(String),
}

impl ProbeEvent {
    fn bytes(&self) -> usize {
        match self {
            Self::AssistantDelta { text, .. } | Self::AssistantFinal { text, .. } => {
                text.len() + 512
            }
            Self::Error(message) => message.len() + 512,
            _ => 512,
        }
    }
}

#[derive(Default)]
struct DisplayEvents {
    queue: VecDeque<ProbeEvent>,
    bytes: usize,
}

impl DisplayEvents {
    fn push(&mut self, event: ProbeEvent) -> Result<(), &'static str> {
        // A canonical final supersedes any still-undelivered stream for the
        // same turn. Avoid retaining two large copies of one assistant reply.
        if let ProbeEvent::AssistantFinal {
            thread_id, turn_id, ..
        } = &event
        {
            if matches!(self.queue.back(), Some(ProbeEvent::AssistantDelta {
                thread_id: previous_thread, turn_id: previous_turn, ..
            }) if previous_thread == thread_id && previous_turn == turn_id)
            {
                if let Some(previous) = self.queue.pop_back() {
                    self.bytes -= previous.bytes();
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
            if let Some(ProbeEvent::AssistantDelta {
                thread_id: previous_thread,
                turn_id: previous_turn,
                text: previous_text,
            }) = self.queue.back_mut()
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
                    return Ok(());
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
        self.queue.push_back(event);
        Ok(())
    }

    fn fatal(&mut self, error: &'static str) {
        // Terminal errors are always visible even when the normal queue filled.
        self.queue.clear();
        self.bytes = 0;
        let _ = self.push(ProbeEvent::Error(error.to_owned()));
    }

    fn take(&mut self) -> Vec<ProbeEvent> {
        self.bytes = 0;
        self.queue.drain(..).collect()
    }
}

enum Command {
    Submit(String),
    Cancel,
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
}

impl TurnLifecycle {
    fn submit(
        &mut self,
        broker: &mut Broker,
        project: &Path,
        prompt: String,
    ) -> Result<(), &'static str> {
        if self.busy {
            return Err("Codex is already processing a turn.");
        }
        if prompt.trim().is_empty() || prompt.len() > MAX_PROMPT_BYTES {
            return Err("Prompt is empty or exceeds the 8 KiB limit.");
        }
        // Only advance the UI state after the Broker has queued a valid RPC.
        let (thread_request, turn_request, pending_prompt) =
            if let Some(thread) = self.thread.as_deref() {
                let id = broker
                    .call(
                        ClientMethod::TurnStart,
                        json!({"threadId":thread,"input":[{"type":"text","text":prompt}],
                            "approvalPolicy":"never",
                            "sandboxPolicy":{"type":"readOnly","networkAccess":false}}),
                    )
                    .map_err(|_| "Codex turn submission was rejected.")?;
                (None, Some(id), None)
            } else {
                let cwd = project.to_str().ok_or("Project path is not valid UTF-8.")?;
                // Explicit read-only sandbox and never-approve policy. The Broker
                // will separately decline every actual tool approval request.
                let id = broker
                    .call(
                        ClientMethod::ThreadStart,
                        json!({"cwd":cwd,"sandbox":"read-only","approvalPolicy":"never"}),
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
                        "approvalPolicy": "never",
                        "sandboxPolicy": {"type":"readOnly","networkAccess":false},
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
            .rpc_started
            .is_some_and(|start| now.saturating_duration_since(start) >= RPC_TIMEOUT)
        {
            return Some("Codex did not answer the thread/turn request; owned process stopped.");
        }
        None
    }

    fn on_event(
        &mut self,
        broker: &mut Broker,
        event: Event,
    ) -> Result<Option<ProbeEvent>, &'static str> {
        match event {
            Event::Approval(approval) => {
                // Never accept an approval, including one with unexpected
                // thread/turn provenance. Failed decline is fatal to the session.
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
                        let created = accepted_read_only_thread(&value, project)?;
                        self.thread = Some(created.clone());
                        if self.cancel_requested {
                            self.pending_prompt = None;
                            self.busy = false;
                            self.cancel_started = None;
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
                        outcome.map_err(|_| "Codex rejected the turn interrupt.")?;
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
                    NotificationKind::ItemCompleted if self.exact_turn(&params) => {
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
                    NotificationKind::TurnCompleted
                        if self.busy
                            && self.thread.as_deref() == safe_id(params.get("threadId")) =>
                    {
                        if self.turn.is_some() && !self.exact_turn(&params) {
                            return Ok(None);
                        }
                        let completion = UnverifiedCompletion::from_notification(&params)?;
                        if self.turn.is_some() {
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
    paths: (PathBuf, PathBuf),
    shutdown: oneshot::Receiver<()>,
    mut commands: mpsc::Receiver<Command>,
    emergency_stop: Arc<AtomicBool>,
    events: Arc<Mutex<DisplayEvents>>,
    status: Arc<AtomicU8>,
    ctx: egui::Context,
) {
    let (executable, project) = paths;
    let prepared = (|| {
        if !executable.is_absolute() || !project.is_absolute() {
            return Err("Codex executable and project require explicit absolute paths.");
        }
        let exe_root = executable
            .parent()
            .ok_or("Invalid Codex executable path.")?;
        let config = EngineConfig::new(&executable, exe_root, &project, &project)
            .map_err(|_| "Codex executable or project failed path validation.")?;
        let broker = Broker::new(Limits::default())
            .map_err(|_| "Codex protocol broker failed initialization.")?;
        Ok::<_, &'static str>((config, broker))
    })();
    let (config, mut broker) = match prepared {
        Ok(v) => v,
        Err(error) => {
            if let Ok(mut queue) = events.lock() {
                queue.fatal(error);
            }
            notify(&status, &ctx, ProbeStatus::Failed);
            return;
        }
    };
    let mut supervisor = EngineSupervisor::new(config);
    let gate = SessionGate::<u64>::default();
    if gate
        .start(1, || {
            supervisor.start().map_err(|_| ())?;
            Ok::<_, ()>((1_u64, ()))
        })
        .await
        .is_err()
    {
        if let Ok(mut queue) = events.lock() {
            queue.fatal("Codex process failed to start.");
        }
        notify(&status, &ctx, ProbeStatus::Failed);
        return;
    }
    let mut shutdown = shutdown;
    let mut lifecycle = TurnLifecycle::default();
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
                        Command::Submit(text) => lifecycle.submit(&mut broker, &project, text),
                        Command::Cancel => lifecycle.cancel(&mut broker),
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
                            }
                            while let Some(event) = broker.take_event() {
                                let translated = lifecycle.on_event(&mut broker, event)?;
                                if let Some(message) = lifecycle.take_terminal_error() {
                                    enqueue(&events, &ctx, ProbeEvent::Error(message.to_owned()))?;
                                }
                                if let Some(event) = translated {
                                    enqueue(&events, &ctx, event)?;
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
    pub fn start(executable: PathBuf, project: PathBuf, ctx: egui::Context) -> Self {
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
                    (executable, project),
                    receiver,
                    inbox,
                    thread_emergency_stop,
                    thread_events,
                    thread_status,
                    ctx,
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

    pub fn take_events(&self) -> Vec<ProbeEvent> {
        match self.events.lock() {
            Ok(mut queue) => queue.take(),
            Err(_) => vec![ProbeEvent::Error(
                "Native display state unavailable.".to_owned(),
            )],
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
    use ultima_protocol_bridge::NotificationKind;

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
}
