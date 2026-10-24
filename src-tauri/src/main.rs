#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod lifecycle;

use std::path::Path;
use std::sync::{atomic::{AtomicU64, Ordering}, Arc};
use tauri::{Emitter, State};
use tokio::sync::{mpsc, oneshot, watch};
use ultima_app_server_supervisor::{EngineConfig, EngineSupervisor, OutputEvent};
use ultima_protocol_bridge::{Approval, ApprovalDecision, ApprovalKind, Broker, ClientMethod, Event, Limits, NotificationKind, Phase, RpcId};
use serde_json::{json, Value};
use lifecycle::{SessionGate, GateError, StartError};

#[derive(Clone)]
struct EngineHandle {
    tx: mpsc::Sender<Action>,
    closed: watch::Receiver<Option<bool>>,
}
struct Shared {
    gate: Arc<SessionGate<EngineHandle>>,
    next_session: AtomicU64,
}
impl Default for Shared {
    fn default() -> Self {
        Self { gate: Arc::new(SessionGate::default()), next_session: AtomicU64::new(0) }
    }
}
enum Action {
    Call(ClientMethod, Value, oneshot::Sender<Result<u64, String>>),
    Decision(Approval, ApprovalDecision, oneshot::Sender<Result<(), String>>),
    Stop,
}
/// A decision must match the complete immutable server request (not merely its id).
fn parse_approval(value: &Value) -> Result<Approval, String> {
    let id: RpcId = serde_json::from_value(value.get("id").cloned().ok_or("Approval ID missing")?)
        .map_err(|_| safe_error("Invalid approval ID"))?;
    let id_valid = match &id { RpcId::Number(_) => true, RpcId::String(s) => !s.is_empty() && s.len() <= 128 };
    if !id_valid { return Err(safe_error("Invalid approval ID")); }
    let field = |name| -> Result<String, String> {
        value.get(name).and_then(Value::as_str)
            .filter(|s| !s.is_empty() && s.len() <= 256)
            .map(str::to_owned).ok_or_else(|| safe_error("Invalid approval identity"))
    };
    let kind = match value.get("kind").and_then(Value::as_str) {
        Some("command") => ApprovalKind::Command,
        Some("fileChange") => ApprovalKind::FileChange,
        _ => return Err(safe_error("Invalid approval kind")),
    };
    Ok(Approval { id, kind, thread_id: field("threadId")?, turn_id: field("turnId")?,
        item_id: field("itemId")?, started_at_ms: value.get("startedAtMs")
            .and_then(Value::as_u64).ok_or("Invalid approval timestamp")? })
}
fn safe_error(reason: &str) -> String {
    // Never forward arbitrary child stderr or protocol messages containing credentials.
    reason.to_owned()
}
fn emit(app: &tauri::AppHandle, session: u64, mut value: Value) {
    if let Some(obj) = value.as_object_mut() {
        obj.insert("session".to_owned(), json!(session));
    }
    let _ = app.emit("ultima:engine", value);
}
async fn flush(broker: &mut Broker, supervisor: &mut EngineSupervisor) -> Result<(), String> {
    while let Some(line) = broker.take_outbound() {
        let value: Value = serde_json::from_slice(&line)
            .map_err(|_| safe_error("Invalid outbound frame"))?;
        supervisor.session_mut().ok_or("Engine closed")?
            .send_json(&value).await.map_err(|_| safe_error("Engine transport failed"))?;
    }
    Ok(())
}
fn broker_event(event: Event) -> Option<Value> {
    match event {
        Event::Reply { id, method, outcome } => {
            let (ok, result) = match outcome {
                Ok(value) => (true, value),
                Err(_) => (false, Value::Null),
            };
            Some(json!({"type":"reply", "id":id, "method":format!("{method:?}"), "ok":ok, "result":result}))
        }
        Event::Notification { kind, params } => {
            let name = match kind {
                NotificationKind::ThreadStarted => "threadStarted",
                NotificationKind::TurnStarted => "turnStarted",
                NotificationKind::TurnCompleted => "turnCompleted",
                NotificationKind::ItemStarted => "itemStarted",
                NotificationKind::ItemCompleted => "itemCompleted",
                NotificationKind::AgentMessageDelta => "delta",
                NotificationKind::CommandOutputDelta => "commandOutput",
                NotificationKind::ServerRequestResolved => "requestResolved",
            };
            Some(json!({"type":name,"params":params}))
        }
        Event::Approval(_) => None, // Caller always explicitly declines before forwarding.
    }
}
async fn drive(
    app: tauri::AppHandle,
    gate: Arc<SessionGate<EngineHandle>>,
    session: u64,
    mut supervisor: EngineSupervisor,
    mut broker: Broker,
    mut inbox: mpsc::Receiver<Action>,
    closed: watch::Sender<Option<bool>>,
) {
    let result: Result<(), String> = async {
        broker.begin_initialize().map_err(|_| safe_error("Handshake initialization failed"))?;
        flush(&mut broker, &mut supervisor).await?;
        emit(&app, session, json!({"type":"status","state":"connecting"}));
        loop {
            tokio::select! {
                biased;
                action = inbox.recv() => {
                    match action {
                        Some(Action::Call(method, params, reply)) => {
                            if method == ClientMethod::TurnInterrupt {
                                let thread = params.get("threadId").and_then(Value::as_str).unwrap_or("");
                                let turn = params.get("turnId").and_then(Value::as_str).unwrap_or("");
                                broker.decline_approvals_for_turn(thread, turn)
                                    .map_err(|_| safe_error("Cannot safely cancel pending approvals"))?;
                            }
                            let answer = broker.call(method, params)
                                .map_err(|_| safe_error("Invalid or unavailable engine command"));
                            if answer.is_ok() {
                                if let Err(reason) = flush(&mut broker, &mut supervisor).await {
                                    let _ = reply.send(Err(reason.clone()));
                                    return Err(reason);
                                }
                            }
                            let _ = reply.send(answer);
                        }
                        Some(Action::Decision(approval, decision, reply)) => {
                            let answer = broker.decide_exact_approval(&approval, decision, decision == ApprovalDecision::Accept)
                                .map_err(|_| safe_error("Approval stale, invalid or already resolved"));
                            if answer.is_ok() {
                                if let Err(reason) = flush(&mut broker, &mut supervisor).await {
                                    let _ = reply.send(Err(reason.clone()));
                                    return Err(reason);
                                }
                            }
                            let _ = reply.send(answer);
                        }
                        Some(Action::Stop) | None => break,
                    }
                }
                incoming = async {
                    supervisor.session_mut().expect("Session started").next_event().await
                } => {
                    match incoming {
                        Some(OutputEvent::Line(line)) => {
                            broker.ingest_line(line.as_bytes())
                                .map_err(|_| safe_error("App Server protocol framing failed"))?;
                            if broker.phase() == Phase::Acknowledgeable {
                                broker.acknowledge_initialized()
                                    .map_err(|_| safe_error("Handshake failed"))?;
                                flush(&mut broker, &mut supervisor).await?;
                                emit(&app, session, json!({"type":"status","state":"ready"}));
                            }
                            while let Some(event) = broker.take_event() {
                                match event {
                                    Event::Approval(approval) => {
                                        emit(&app, session, json!({"type":"approvalPending","request":{
                                            "id":approval.id,
                                            "kind":if approval.kind == ApprovalKind::Command { "command" } else { "fileChange" },
                                            "threadId":approval.thread_id,
                                            "turnId":approval.turn_id,
                                            "itemId":approval.item_id,
                                            "startedAtMs":approval.started_at_ms,
                                        }}));
                                    }
                                    other => {
                                        if let Some(value) = broker_event(other) { emit(&app, session, value); }
                                    }
                                }
                            }
                            flush(&mut broker, &mut supervisor).await?;
                        }
                        Some(OutputEvent::Closed) | None => return Err(safe_error("Codex disconnected")),
                        Some(_) => return Err(safe_error("App Server output failed validation")),
                    }
                }
            }
        }
        Ok(())
    }.await;

    if let Err(reason) = result {
        emit(&app, session, json!({"type":"status","state":"error","message":reason}));
    }
    // The stop signal is NOT completion. Notify observers only after supervisor.stop().
    let clean = supervisor.stop().await.is_ok();
    let _ = closed.send(Some(clean));
    // A disconnect owns the Stopping sentinel; spontaneous EOF owns Running.
    if gate.finish_unexpected(session, clean).await {
        emit(&app, session, if clean {
            json!({"type":"status","state":"disconnected"})
        } else {
            json!({"type":"status","state":"error","message":"Engine shutdown could not be confirmed; restart Ultima before reconnecting."})
        });
    }
}
#[tauri::command]
async fn ultima_connect(
    app: tauri::AppHandle, state: State<'_, Shared>,
    executable: String, project: String,
) -> Result<u64, String> {
    if executable.len() > 4096 || project.len() > 4096 {
        return Err(safe_error("Path too long"));
    }
    let exe = Path::new(&executable);
    let cwd = Path::new(&project);
    if !exe.is_absolute() || !cwd.is_absolute() {
        return Err(safe_error("Absolute paths are required"));
    }
    let exe_parent = exe.parent().ok_or("Missing executable parent")?;
    let config = EngineConfig::new(exe, exe_parent, cwd, cwd)
        .map_err(|_| safe_error("Executable or project not accessible"))?;
    let broker = Broker::new(Limits::default())
        .map_err(|_| safe_error("Bridge initialization failed"))?;
    let session = state.next_session.fetch_add(1, Ordering::Relaxed)
        .checked_add(1).ok_or("Session identifiers exhausted")?;
    let (supervisor, receiver, shutdown_tx) = state.gate.start(session, || {
        let mut supervisor = EngineSupervisor::new(config);
        supervisor.start().map_err(|_| safe_error("Failed to launch trusted Codex"))?;
        let (tx, rx) = mpsc::channel(16);
        let (shutdown_tx, shutdown_rx) = watch::channel(None);
        Ok::<_, String>((EngineHandle { tx, closed: shutdown_rx }, (supervisor, rx, shutdown_tx)))
    }).await.map_err(|error| match error {
        StartError::Gate(GateError::Busy) => safe_error("Engine is still running or shutting down"),
        StartError::Gate(GateError::Poisoned) => safe_error("Previous engine shutdown unconfirmed; restart Ultima"),
        StartError::Start(reason) => reason,
    })?;
    tauri::async_runtime::spawn(drive(app, state.gate.clone(), session, supervisor, broker, receiver, shutdown_tx));
    Ok(session)
}
#[tauri::command]
async fn ultima_call(
    state: State<'_, Shared>, method: String, params: Value,
) -> Result<u64, String> {
    let allowed = match method.as_str() {
        "model/list" => ClientMethod::ModelList,
        "thread/start" => ClientMethod::ThreadStart,
        "thread/resume" => ClientMethod::ThreadResume,
        "turn/start" => ClientMethod::TurnStart,
        "turn/interrupt" => ClientMethod::TurnInterrupt,
        _ => return Err(safe_error("Unsupported method")),
    };
    let (_, handle) = state.gate.current().await.ok_or("Engine is not ready")?;
    let (tx, rx) = oneshot::channel();
    handle.tx.send(Action::Call(allowed, params, tx)).await
        .map_err(|_| safe_error("Engine closed"))?;
    rx.await.map_err(|_| safe_error("Engine command unavailable"))?
}
#[tauri::command]
async fn ultima_approval(state: State<'_, Shared>, session: u64, request: Value, decision: String) -> Result<(), String> {
    if decision != "accept" && decision != "decline" { return Err(safe_error("Unsupported decision")); }
    let approval = parse_approval(&request)?;
    let (current, handle) = state.gate.current().await.ok_or("Engine is not ready")?;
    if current != session { return Err(safe_error("Stale engine session")); }
    let (tx, rx) = oneshot::channel();
    let decision = if decision == "accept" { ApprovalDecision::Accept } else { ApprovalDecision::Decline };
    handle.tx.try_send(Action::Decision(approval, decision, tx))
        .map_err(|_| safe_error("Engine decision queue unavailable"))?;
    rx.await.map_err(|_| safe_error("Engine decision dropped"))?
}
#[tauri::command]
async fn ultima_disconnect(app: tauri::AppHandle, state: State<'_, Shared>) -> Result<(), String> {
    let (session, handle) = match state.gate.begin_stop().await {
        Ok(Some(v)) => v,
        Ok(None) => return Ok(()),
        Err(GateError::Busy) => return Err(safe_error("Engine shutdown already in progress")),
        Err(GateError::Poisoned) => return Err(safe_error("Previous shutdown unconfirmed; restart Ultima")),
    };
    // Detached teardown guarantees the gate is eventually updated even if IPC caller cancels.
    let gate = state.gate.clone();
    let (reply, completion) = oneshot::channel();
    tauri::async_runtime::spawn(async move {
        let _ = handle.tx.send(Action::Stop).await;
        let mut monitor = handle.closed;
        let clean = loop {
            let observed = { *monitor.borrow_and_update() };
            if let Some(ok) = observed { break ok; }
            if monitor.changed().await.is_err() { break false; }
        };
        if gate.finish_stop(session, clean).await {
            emit(&app, session, if clean {
                json!({"type":"status","state":"disconnected"})
            } else {
                json!({"type":"status","state":"error","message":"Engine shutdown could not be confirmed; restart Ultima before reconnecting."})
            });
        }
        let _ = reply.send(clean);
    });
    match completion.await {
        Ok(true) => Ok(()),
        _ => Err(safe_error("Engine shutdown unconfirmed; reconnect blocked")),
    }
}
fn main() {
    tauri::Builder::default()
        .manage(Shared::default())
        .invoke_handler(tauri::generate_handler![ultima_connect, ultima_call, ultima_approval, ultima_disconnect])
        .run(tauri::generate_context!())
        .expect("Ultima desktop could not start");
}
