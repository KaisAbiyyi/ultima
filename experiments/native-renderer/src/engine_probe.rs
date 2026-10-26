//! Opt-in official Codex App Server client for the native-renderer experiment.
//! Transport, JSONL protocol, process-tree teardown and approval policy are
//! owned by the production Rust crates. There are no automatic tool approvals.
use eframe::egui;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use tokio::sync::{mpsc, oneshot};
use ultima_app_server_supervisor::{EngineConfig, EngineSupervisor, OutputEvent};
use ultima_native_session::SessionGate;
use ultima_protocol_bridge::{Broker, ClientMethod, Event, Limits, NotificationKind, Phase};

const MAX_PROMPT_BYTES: usize = 8 * 1024;
const MAX_ASSISTANT_BYTES: usize = 256 * 1024;
const MAX_UI_EVENTS: usize = 128;
const MAX_UI_EVENT_BYTES: usize = 512 * 1024;
const MAX_COMMANDS: usize = 8;

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

/// The UI has at most one turn in flight, bound to one official thread.
/// In-flight request IDs are matched against the Broker's typed replies;
/// notifications and content require matching thread AND turn IDs.
#[derive(Default)]
struct TurnLifecycle {
    thread: Option<String>,
    thread_request: Option<u64>,
    turn_request: Option<u64>,
    interrupt_request: Option<u64>,
    turn: Option<String>,
    pending_prompt: Option<String>,
    busy: bool,
    cancel_requested: bool,
    text_bytes: usize,
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
                        json!({"threadId":thread,"input":[{"type":"text","text":prompt}]}),
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
        self.turn = None;
        self.thread_request = thread_request;
        self.turn_request = turn_request;
        self.interrupt_request = None;
        self.pending_prompt = pending_prompt;
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
                    }),
                )
                .map_err(|_| "Codex turn submission was rejected.")?,
        );
        self.pending_prompt = None;
        Ok(())
    }

    fn cancel(&mut self, broker: &mut Broker) -> Result<(), &'static str> {
        if !self.busy {
            return Err("There is no active turn to cancel.");
        }
        self.cancel_requested = true;
        self.interrupt_if_known(broker)
    }

    fn interrupt_if_known(&mut self, broker: &mut Broker) -> Result<(), &'static str> {
        if !self.cancel_requested || self.interrupt_request.is_some() {
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
                        let value = outcome.map_err(|_| "Codex rejected thread creation.")?;
                        let created = thread_id(&value)
                            .ok_or("Codex returned an invalid thread identity.")?;
                        self.thread = Some(created.to_owned());
                        if self.cancel_requested {
                            self.pending_prompt = None;
                            self.busy = false;
                        } else {
                            self.start_turn(broker)?;
                        }
                        Ok(Some(ProbeEvent::ThreadReady {
                            thread_id: created.to_owned(),
                        }))
                    }
                    ClientMethod::TurnStart if self.turn_request == Some(id) => {
                        let value = outcome.map_err(|_| "Codex rejected the turn request.")?;
                        if let Some(id) = turn_id(&value) {
                            self.identify_turn(broker, id)
                        } else {
                            // A turn/started notification may carry the ID.
                            Ok(None)
                        }
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
                    NotificationKind::TurnCompleted if self.exact_turn(&params) => {
                        let status = params
                            .pointer("/turn/status")
                            .and_then(Value::as_str)
                            .ok_or("Codex turn completed without a final status.")?;
                        if !matches!(status, "completed" | "interrupted" | "failed") {
                            return Err("Codex returned an unknown final turn status.");
                        }
                        let event = ProbeEvent::TurnCompleted {
                            thread_id: self.thread.clone().expect("checked"),
                            turn_id: self.turn.clone().expect("checked"),
                            status: status.to_owned(),
                        };
                        self.turn = None;
                        self.turn_request = None;
                        self.busy = false;
                        self.cancel_requested = false;
                        self.interrupt_request = None;
                        Ok(Some(event))
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
    executable: PathBuf,
    project: PathBuf,
    shutdown: oneshot::Receiver<()>,
    mut commands: mpsc::Receiver<Command>,
    events: Arc<Mutex<DisplayEvents>>,
    status: Arc<AtomicU8>,
    ctx: egui::Context,
) {
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
    let result: Result<(), &'static str> = async {
        broker.begin_initialize().map_err(|_| "Codex handshake could not start.")?;
        flush(&mut broker, &mut supervisor).await?;
        loop {
            tokio::select! {
                biased;
                _ = &mut shutdown => return Ok(()),
                command = commands.recv() => {
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
                                if let Some(event) = lifecycle.on_event(&mut broker, event)? {
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
        let events = Arc::new(Mutex::new(DisplayEvents::default()));
        let thread_status = status.clone();
        let thread_events = events.clone();
        let thread = std::thread::spawn(move || {
            match tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime.block_on(drive(
                    executable,
                    project,
                    receiver,
                    inbox,
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
        self.commands
            .try_send(Command::Cancel)
            .map_err(|_| "Codex command queue is full or disconnected.")
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

    #[test]
    fn thread_and_turn_use_exact_ids_and_read_only_policy() {
        let mut b = ready();
        let mut f = TurnLifecycle::default();
        f.submit(&mut b, Path::new("C:/project"), "explain this code".into())
            .unwrap();
        let start: Value = serde_json::from_slice(&b.take_outbound().unwrap()).unwrap();
        assert_eq!(start["method"], "thread/start");
        assert_eq!(start["params"]["sandbox"], "read-only");
        assert_eq!(start["params"]["approvalPolicy"], "never");
        let response = reply(&mut b, 1, json!({"thread":{"id":"thread-a"}}));
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
                    params: json!({"threadId":"thread-other","turnId":"turn-a","delta":"wrong"}),
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
                    params: json!({"threadId":"thread-a","turnId":"turn-a","delta":"right"}),
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
        assert!(b.take_outbound().is_none());
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
    }
}
