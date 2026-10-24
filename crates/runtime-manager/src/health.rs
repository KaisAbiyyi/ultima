//! Bounded unauthenticated pre-activation smoke. No agent turns, tools or approvals.
use crate::Error;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::{timeout, Instant};
use ultima_app_server_supervisor::{EngineConfig, EngineSupervisor, OutputEvent};

const DEADLINE: Duration = Duration::from_secs(18);

async fn exchange(supervisor: &mut EngineSupervisor) -> Result<(), Error> {
    let session = supervisor.session_mut().ok_or(Error::Invalid("probe child"))?;
    session.send_json(&json!({"id":0,"method":"initialize",
        "params":{"clientInfo":{"name":"ultima","title":"Ultima","version":"0.1.0"},
        "capabilities":{"experimentalApi":false}}}))
        .await.map_err(|_| Error::Invalid("probe initialize send"))?;
    let initialized = next_response(supervisor, 0).await?;
    if !initialized["result"]["userAgent"].is_string() ||
       !initialized["result"]["platformFamily"].is_string() ||
       !initialized["result"]["platformOs"].is_string() {
        return Err(Error::Invalid("initialize shape"));
    }
    let session = supervisor.session_mut().ok_or(Error::Invalid("probe child"))?;
    session.send_json(&json!({"method":"initialized","params":{}}))
        .await.map_err(|_| Error::Invalid("probe acknowledge"))?;
    session.send_json(&json!({"id":1,"method":"model/list","params":{"limit":3}}))
        .await.map_err(|_| Error::Invalid("probe model list send"))?;
    let models = next_response(supervisor, 1).await?;
    if !models["result"]["data"].is_array() ||
        models["result"].as_object().is_none_or(|v| !v.contains_key("nextCursor")) {
        return Err(Error::Invalid("model list shape"));
    }
    Ok(())
}

async fn next_response(supervisor: &mut EngineSupervisor, id: u64) -> Result<Value, Error> {
    for _ in 0..64 {
        let line = supervisor.session_mut().ok_or(Error::Invalid("probe child"))?
            .next_event().await;
        let OutputEvent::Line(raw) = line.ok_or(Error::Invalid("probe disconnected"))? else {
            return Err(Error::Invalid("probe output invalid"));
        };
        let value: Value = serde_json::from_str(&raw)
            .map_err(|_| Error::Invalid("probe JSON"))?;
        if value.get("method").is_some() && value.get("id").is_some() {
            // Preflight is not an interactive session; any inbound request is fatal.
            return Err(Error::Invalid("probe server request"));
        }
        if value["id"] == id {
            if value.get("error").is_some() || value.get("result").is_none() {
                return Err(Error::Invalid("probe rpc error"));
            }
            return Ok(value);
        }
        if value.get("id").is_some() { return Err(Error::Invalid("probe unexpected id")); }
    }
    Err(Error::Invalid("probe event cap"))
}
pub async fn probe(config: EngineConfig) -> Result<(), Error> {
    let mut supervisor = EngineSupervisor::new(config);
    supervisor.start().map_err(|_| Error::Invalid("probe start"))?;
    let begin = Instant::now();
    let result = timeout(DEADLINE, exchange(&mut supervisor)).await;
    // No successful activation unless the entire process tree was reaped.
    let stopped = supervisor.stop().await.is_ok();
    if !stopped { return Err(Error::Invalid("probe teardown")); }
    if begin.elapsed() > DEADLINE { return Err(Error::Invalid("probe timeout")); }
    result.map_err(|_| Error::Invalid("probe timeout"))?
}
