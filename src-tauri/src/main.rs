#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod lifecycle;
mod runtime;

use std::path::Path;
use std::sync::{atomic::{AtomicU64, Ordering}, Arc};
use tauri::{Emitter, State};
use tokio::sync::{mpsc, oneshot, watch, Mutex};
use ultima_app_server_supervisor::{EngineConfig, EngineSupervisor, OutputEvent};
use ultima_protocol_bridge::{Broker, ClientMethod, Event, Limits, NotificationKind, Phase};
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
    updates: Arc<Mutex<()>>,
}
impl Default for Shared {
    fn default() -> Self {
        Self { gate: Arc::new(SessionGate::default()), next_session: AtomicU64::new(0), updates: Arc::new(Mutex::new(())) }
    }
}
enum Action {
    Call(ClientMethod, Value, oneshot::Sender<Result<u64, String>>),
    Stop,
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
    updates: Arc<Mutex<()>>,
    managed: bool,
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
                                    Event::Approval(ref approval) => {
                                        broker.decline_approval(&approval.id)
                                            .map_err(|_| safe_error("Permission denial failed"))?;
                                        emit(&app, session, json!({"type":"approvalBlocked","kind":format!("{:?}",approval.kind)}));
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
    let failed = result.is_err();
    let clean = supervisor.stop().await.is_ok();
    // Reserve the update mutex through rollback + gate completion so a new
    // connect cannot race a failed managed session and inherit its selection.
    let _update = updates.lock().await;
    if failed && managed && clean {
        // Never replay or approve old in-flight protocol work.
        let _ = runtime::manager(&app).and_then(|manager| manager.rollback().map(|_| ())
            .map_err(|_| "Managed rollback failed".into()));
        emit(&app, session, json!({"type":"status","state":"error","message":"Managed Codex failed; previous safe runtime restored. Reconnect explicitly."}));
    }
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
    // Serialize connect with updates and rollback. SessionGate remains the
    // authoritative owner for one live process tree.
    let _update = state.updates.lock().await;
    let exe = Path::new(&executable);
    let cwd = Path::new(&project);
    if !exe.is_absolute() || !cwd.is_absolute() {
        return Err(safe_error("Absolute paths are required"));
    }
    let exe_parent = exe.parent().ok_or("Missing executable parent")?;
    let manager = runtime::manager(&app)?;
    let managed = matches!(manager.selected(), ultima_runtime_manager::Choice::Managed(_));
    let config = manager.engine_config(exe, exe_parent, cwd, cwd)
        .map_err(|_| safe_error("Executable or project not accessible"))?;
    let broker = Broker::new(Limits::default())
        .map_err(|_| safe_error("Bridge initialization failed"))?;
    let session = state.next_session.fetch_add(1, Ordering::Relaxed)
        .checked_add(1).ok_or("Session identifiers exhausted")?;
    let launch = state.gate.start(session, || {
        let mut supervisor = EngineSupervisor::new(config);
        supervisor.start().map_err(|_| safe_error("Failed to launch trusted Codex"))?;
        let (tx, rx) = mpsc::channel(16);
        let (shutdown_tx, shutdown_rx) = watch::channel(None);
        Ok::<_, String>((EngineHandle { tx, closed: shutdown_rx }, (supervisor, rx, shutdown_tx)))
    }).await;
    let (supervisor, receiver, shutdown_tx) = match launch {
        Ok(value) => value,
        Err(StartError::Gate(GateError::Busy)) => return Err(safe_error("Engine is still running or shutting down")),
        Err(StartError::Gate(GateError::Poisoned)) => return Err(safe_error("Previous engine shutdown unconfirmed; restart Ultima")),
        Err(StartError::Start(reason)) => {
            if managed { let _ = manager.rollback(); }
            return Err(reason);
        }
    };
    tauri::async_runtime::spawn(drive(app, state.gate.clone(), session, supervisor, broker, receiver, shutdown_tx, state.updates.clone(), managed));
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
// Update operations are opt-in, never invoked during automatic desktop startup.
// In particular, Stopping/Poisoned SessionGate is not an idle gate.
#[tauri::command]
async fn ultima_runtime_status(app: tauri::AppHandle) -> Result<Value, String> {
    runtime::status(&app)
}
#[tauri::command]
async fn ultima_runtime_stage(app: tauri::AppHandle, state: State<'_, Shared>, source: String) -> Result<(), String> {
    let _updates = state.updates.lock().await;
    state.gate.require_idle().await.map_err(|_| safe_error("Disconnect Codex and confirm shutdown before updating"))?;
    runtime::stage(&app, &source)
}
#[tauri::command]
async fn ultima_runtime_activate(app: tauri::AppHandle, state: State<'_, Shared>, project: String) -> Result<(), String> {
    let _updates = state.updates.lock().await;
    state.gate.require_idle().await.map_err(|_| safe_error("Disconnect Codex and confirm shutdown before updating"))?;
    runtime::activate(&app, &project).await
}
#[tauri::command]
async fn ultima_runtime_system(app: tauri::AppHandle, state: State<'_, Shared>) -> Result<(), String> {
    let _updates = state.updates.lock().await;
    state.gate.require_idle().await.map_err(|_| safe_error("Disconnect Codex and confirm shutdown before updating"))?;
    runtime::restore_system(&app)
}
#[tauri::command]
async fn ultima_runtime_rollback(app: tauri::AppHandle, state: State<'_, Shared>) -> Result<(), String> {
    let _updates = state.updates.lock().await;
    state.gate.require_idle().await.map_err(|_| safe_error("Disconnect Codex and confirm shutdown before updating"))?;
    runtime::rollback(&app)
}
fn main() {
    tauri::Builder::default()
        .manage(Shared::default())
        .invoke_handler(tauri::generate_handler![ultima_connect, ultima_call, ultima_disconnect, ultima_runtime_status, ultima_runtime_stage, ultima_runtime_activate, ultima_runtime_system, ultima_runtime_rollback])
        .run(tauri::generate_context!())
        .expect("Ultima desktop could not start");
}
