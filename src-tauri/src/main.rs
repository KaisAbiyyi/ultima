#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::path::Path;
use std::sync::Arc;
use tauri::{Emitter, Manager, State};
use tokio::sync::{mpsc, oneshot, Mutex};
use ultima_app_server_supervisor::{EngineConfig, EngineSupervisor, OutputEvent};
use ultima_protocol_bridge::{Broker, ClientMethod, Event, Limits, NotificationKind, Phase};
use serde_json::{json, Value};

#[derive(Default)]
struct Shared {
    actor: Arc<Mutex<Option<mpsc::Sender<Action>>>>,
}
enum Action {
    Call(ClientMethod, Value, oneshot::Sender<Result<u64, String>>),
    Stop,
}
fn safe_error(reason: &str) -> String {
    // Never forward arbitrary child stderr or protocol messages containing credentials.
    reason.to_owned()
}
fn emit(app: &tauri::AppHandle, value: Value) {
    let _ = app.emit("ultima:engine", value);
}
async fn flush(
    broker: &mut Broker,
    supervisor: &mut EngineSupervisor,
) -> Result<(), String> {
    while let Some(line) = broker.take_outbound() {
        let value: Value = serde_json::from_slice(&line)
            .map_err(|_| safe_error("Invalid outbound frame"))?;
        supervisor.session_mut().ok_or("Engine closed")?
            .send_json(&value).await.map_err(|_| safe_error("Engine transport failed"))?;
    }
    Ok(())
}
fn emit_broker_event(app: &tauri::AppHandle, event: Event) -> Option<Value> {
    match event {
        Event::Reply { id, method, outcome } => {
            let (ok, result) = match outcome {
                Ok(value) => (true, value),
                Err(_) => (false, Value::Null), // Do not leak upstream error message.
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
        Event::Approval(approval) => {
            emit(app, json!({"type":"approvalBlocked","kind":format!("{:?}",approval.kind)}));
            None // The caller MUST explicitly decline below; NC-07 provides real approvals.
        }
    }
}
async fn drive(
    app: tauri::AppHandle,
    mut supervisor: EngineSupervisor,
    mut broker: Broker,
    mut inbox: mpsc::Receiver<Action>,
    owned_sender: mpsc::Sender<Action>,
) {
    let result: Result<(), String> = async {
        broker.begin_initialize().map_err(|_| safe_error("Handshake initialization failed"))?;
        flush(&mut broker, &mut supervisor).await?;
        emit(&app, json!({"type":"status","state":"connecting"}));
        loop {
            tokio::select! {
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
                                emit(&app, json!({"type":"status","state":"ready"}));
                            }
                            while let Some(event) = broker.take_event() {
                                match event {
                                    Event::Approval(ref approval) => {
                                        broker.decline_approval(&approval.id)
                                            .map_err(|_| safe_error("Permission denial failed"))?;
                                        emit(&app, json!({"type":"approvalBlocked","kind":format!("{:?}",approval.kind)}));
                                    }
                                    other => {
                                        if let Some(value) = emit_broker_event(&app, other) { emit(&app, value); }
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
        emit(&app, json!({"type":"status","state":"error","message":reason}));
    }
    let _ = supervisor.stop().await;
    // Cleared by the actor only after it exits; never erase a newer session.
    let actor = app.state::<Shared>().actor.clone();
    let mut guard = actor.lock().await;
    let was_current = guard.as_ref().is_some_and(|sender| sender.same_channel(&owned_sender));
    if was_current {
        *guard = None;
    }
    drop(guard);
    if was_current {
        emit(&app, json!({"type":"status","state":"disconnected"}));
    }
}
#[tauri::command]
async fn ultima_connect(
    app: tauri::AppHandle, state: State<'_, Shared>,
    executable: String, project: String,
) -> Result<(), String> {
    // User must explicitly select each executable and project directory; no PATH search,
    // shell invocation, credential harvesting, or ambient directory traversal.
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
    let mut guard = state.actor.lock().await;
    if guard.is_some() { return Err(safe_error("Disconnect current engine first")); }
    let mut supervisor = EngineSupervisor::new(config);
    supervisor.start().map_err(|_| safe_error("Failed to launch trusted Codex"))?;
    let broker = Broker::new(Limits::default())
        .map_err(|_| safe_error("Bridge initialization failed"))?;
    let (sender, receiver) = mpsc::channel(16);
    *guard = Some(sender.clone());
    drop(guard);
    tauri::async_runtime::spawn(drive(app, supervisor, broker, receiver, sender));
    Ok(())
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
    let sender = state.actor.lock().await.clone().ok_or("Not connected")?;
    let (tx, rx) = oneshot::channel();
    sender.send(Action::Call(allowed, params, tx)).await
        .map_err(|_| safe_error("Engine closed"))?;
    rx.await.map_err(|_| safe_error("Engine command unavailable"))?
}
#[tauri::command]
async fn ultima_disconnect(state: State<'_, Shared>) -> Result<(), String> {
    let sender = state.actor.lock().await.take();
    if let Some(sender) = sender {
        sender.send(Action::Stop).await.map_err(|_| safe_error("Engine closed"))?;
    }
    Ok(())
}
fn main() {
    tauri::Builder::default()
        .manage(Shared::default())
        .invoke_handler(tauri::generate_handler![ultima_connect, ultima_call, ultima_disconnect])
        .run(tauri::generate_context!())
        .expect("Ultima desktop could not start");
}
