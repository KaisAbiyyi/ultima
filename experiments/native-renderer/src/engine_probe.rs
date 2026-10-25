//! A minimal read-only connection probe to the *official* Codex app-server.
//! All runtime/ownership and protocol validation reuse the production Rust crates.
//! No model turns, tool execution or user approval can be initiated by this spike.
use eframe::egui;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use tokio::sync::oneshot;
use ultima_app_server_supervisor::{EngineConfig, EngineSupervisor, OutputEvent};
use ultima_protocol_bridge::{Broker, Event, Limits, Phase};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ProbeStatus {
    Connecting = 0,
    Ready = 1,
    Failed = 2,
    Stopped = 3,
}

pub struct EngineProbe {
    status: Arc<AtomicU8>,
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

fn notify(status: &AtomicU8, ctx: &egui::Context, state: ProbeStatus) {
    status.store(state as u8, Ordering::Release);
    ctx.request_repaint();
}

async fn flush(broker: &mut Broker, supervisor: &mut EngineSupervisor) -> Result<(), ()> {
    while let Some(frame) = broker.take_outbound() {
        let payload: Value = serde_json::from_slice(&frame).map_err(|_| ())?;
        supervisor
            .session_mut()
            .ok_or(())?
            .send_json(&payload)
            .await
            .map_err(|_| ())?;
    }
    Ok(())
}

async fn drive(
    executable: PathBuf,
    project: PathBuf,
    shutdown: oneshot::Receiver<()>,
    status: Arc<AtomicU8>,
    ctx: egui::Context,
) {
    let prepared = (|| {
        if !executable.is_absolute() || !project.is_absolute() {
            return Err(());
        }
        let exe_root = executable.parent().ok_or(())?;
        let config =
            EngineConfig::new(&executable, exe_root, &project, &project).map_err(|_| ())?;
        let broker = Broker::new(Limits::default()).map_err(|_| ())?;
        Ok::<_, ()>((config, broker))
    })();
    let Ok((config, mut broker)) = prepared else {
        notify(&status, &ctx, ProbeStatus::Failed);
        return;
    };
    let mut supervisor = EngineSupervisor::new(config);
    if supervisor.start().is_err() {
        notify(&status, &ctx, ProbeStatus::Failed);
        return;
    }
    let mut shutdown = shutdown;
    let result: Result<(), ()> = async {
        broker.begin_initialize().map_err(|_| ())?;
        flush(&mut broker, &mut supervisor).await?;
        loop {
            tokio::select! {
                biased;
                _ = &mut shutdown => return Ok(()),
                event = async {
                    supervisor.session_mut().ok_or(())?.next_event().await.ok_or(())
                } => {
                    match event? {
                        OutputEvent::Line(line) => {
                            broker.ingest_line(line.as_bytes()).map_err(|_| ())?;
                            if broker.phase() == Phase::Acknowledgeable {
                                broker.acknowledge_initialized().map_err(|_| ())?;
                                flush(&mut broker, &mut supervisor).await?;
                                notify(&status, &ctx, ProbeStatus::Ready);
                            }
                            while let Some(event) = broker.take_event() {
                                if let Event::Approval(approval) = event {
                                    // Fail-closed: the spike never exposes an approval control.
                                    broker.decline_approval(&approval.id).map_err(|_| ())?;
                                }
                            }
                            flush(&mut broker, &mut supervisor).await?;
                        }
                        OutputEvent::Closed | OutputEvent::LineTooLong |
                        OutputEvent::InvalidUtf8 | OutputEvent::ReadFailed => return Err(()),
                    }
                }
            }
        }
    }
    .await;
    let clean = supervisor.stop().await.is_ok();
    if result.is_err() || !clean {
        notify(&status, &ctx, ProbeStatus::Failed);
    } else {
        notify(&status, &ctx, ProbeStatus::Stopped);
    }
}

impl EngineProbe {
    pub fn start(executable: PathBuf, project: PathBuf, ctx: egui::Context) -> Self {
        let (shutdown, receiver) = oneshot::channel();
        let status = Arc::new(AtomicU8::new(ProbeStatus::Connecting as u8));
        let thread_status = status.clone();
        let thread = std::thread::spawn(move || {
            match tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
            {
                Ok(runtime) => {
                    runtime.block_on(drive(executable, project, receiver, thread_status, ctx))
                }
                Err(_) => notify(&thread_status, &ctx, ProbeStatus::Failed),
            }
        });
        Self {
            status,
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
