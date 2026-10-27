//! NC-05: bounded, typed App Server JSONL broker. No process, network or credential ownership.
//! Caller MUST close the owned App Server session after any framing/backpressure error.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};

pub const PINNED_APP_SERVER: &str = "0.164.0";
pub const MAX_JSONL_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeError {
    InvalidState,
    InvalidRequest,
    InvalidEnvelope,
    InvalidJson,
    InvalidUtf8,
    LineTooLarge,
    TruncatedLine,
    Backpressure,
    IdExhausted,
    UnknownApproval,
    ExplicitApprovalRequired,
}
impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for BridgeError {}
pub type Result<T> = std::result::Result<T, BridgeError>;

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub line_bytes: usize,
    pub pending: usize,
    pub approvals: usize,
    pub events: usize,
    pub event_bytes: usize,
    pub outbound: usize,
    pub outbound_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            line_bytes: MAX_JSONL_BYTES,
            pending: 128,
            approvals: 32,
            events: 128,
            event_bytes: 4 * 1024 * 1024,
            outbound: 128,
            outbound_bytes: 4 * 1024 * 1024,
        }
    }
}
impl Limits {
    fn valid(self) -> bool {
        self.line_bytes > 0
            && self.line_bytes <= MAX_JSONL_BYTES
            && self.pending > 0
            && self.approvals > 0
            && self.events > 0
            && self.event_bytes > 0
            && self.outbound > 0
            && self.outbound_bytes > 0
    }
}

/// Feed bytes in bounded memory, emitting one complete line at a time instead
/// of buffering an unbounded number of lines from one large read().
pub struct LineDecoder {
    buffer: Vec<u8>,
    maximum: usize,
}
impl LineDecoder {
    pub fn new(maximum: usize) -> Result<Self> {
        if maximum == 0 || maximum > MAX_JSONL_BYTES {
            return Err(BridgeError::InvalidRequest);
        }
        Ok(Self {
            buffer: Vec::new(),
            maximum,
        })
    }
    pub fn feed(
        &mut self,
        bytes: &[u8],
        mut on_line: impl FnMut(&[u8]) -> Result<()>,
    ) -> Result<()> {
        for &byte in bytes {
            if byte == b'\n' {
                if self.buffer.last() == Some(&b'\r') {
                    self.buffer.pop();
                }
                on_line(&self.buffer)?;
                self.buffer.clear();
            } else {
                if self.buffer.len() >= self.maximum {
                    return Err(BridgeError::LineTooLarge);
                }
                self.buffer.push(byte);
            }
        }
        Ok(())
    }
    pub fn finish(&self) -> Result<()> {
        if self.buffer.is_empty() {
            Ok(())
        } else {
            Err(BridgeError::TruncatedLine)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Fresh,
    Initializing,
    Acknowledgeable,
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientMethod {
    Initialize,
    ModelList,
    ThreadStart,
    ThreadList,
    ThreadRead,
    ThreadTurnsList,
    ThreadResume,
    TurnStart,
    TurnInterrupt,
}
impl ClientMethod {
    fn name(self) -> &'static str {
        match self {
            Self::Initialize => "initialize",
            Self::ModelList => "model/list",
            Self::ThreadStart => "thread/start",
            Self::ThreadList => "thread/list",
            Self::ThreadRead => "thread/read",
            Self::ThreadTurnsList => "thread/turns/list",
            Self::ThreadResume => "thread/resume",
            Self::TurnStart => "turn/start",
            Self::TurnInterrupt => "turn/interrupt",
        }
    }
    fn valid_params(self, p: &Value) -> bool {
        let Some(p) = p.as_object() else {
            return false;
        };
        let non_empty = |key: &str| {
            p.get(key)
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty())
        };
        match self {
            Self::Initialize => false, // only begin_initialize can send this
            Self::ModelList => {
                p.get("cursor").is_none_or(|v| v.is_null() || v.is_string())
                    && p.get("limit")
                        .is_none_or(|v| v.is_null() || v.as_u64().is_some_and(|n| n > 0))
                    && p.get("includeHidden")
                        .is_none_or(|v| v.is_null() || v.is_boolean())
            }
            Self::ThreadStart => {
                p.get("cwd")
                    .is_none_or(|v| v.as_str().is_some_and(|s| !s.is_empty()))
                    && p.get("model")
                        .is_none_or(|v| v.as_str().is_some_and(|s| !s.is_empty()))
                    && !(p.get("sandbox").is_some_and(|v| !v.is_null())
                        && p.get("permissions").is_some_and(|v| !v.is_null()))
            }
            Self::ThreadList => {
                p.get("cwd").and_then(Value::as_str).is_some_and(|s| !s.is_empty())
                    && p.get("limit").and_then(Value::as_u64).is_some_and(|n| (1..=24).contains(&n))
                    && p.get("cursor").is_none_or(|v| v.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 2048))
            }
            Self::ThreadRead => {
                non_empty("threadId") && p.get("includeTurns") == Some(&Value::Bool(false))
            }
            Self::ThreadTurnsList => {
                non_empty("threadId")
                    && p.get("limit").and_then(Value::as_u64).is_some_and(|n| (1..=12).contains(&n))
                    && p.get("sortDirection").and_then(Value::as_str) == Some("desc")
                    && p.get("itemsView").and_then(Value::as_str) == Some("full")
                    && p.get("cursor").is_none_or(|v| v.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 2048))
            }
            Self::ThreadResume => {
                non_empty("threadId")
                    && p.get("excludeTurns")
                        .is_none_or(|v| v.is_null() || v.is_boolean())
            }
            Self::TurnStart => {
                non_empty("threadId")
                    && p.get("input").and_then(Value::as_array).is_some_and(|a| {
                        a.iter().all(|v| {
                            v.get("type").and_then(Value::as_str) == Some("text")
                                && v.get("text").is_some_and(Value::is_string)
                        })
                    })
            }
            Self::TurnInterrupt => non_empty("threadId") && non_empty("turnId"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RpcId {
    Number(u64),
    String(String),
}
impl RpcId {
    fn parse(v: &Value) -> Option<Self> {
        match v {
            Value::Number(n) => n.as_u64().map(Self::Number),
            Value::String(s) if !s.is_empty() && s.len() <= 128 => Some(Self::String(s.clone())),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationKind {
    ThreadStarted,
    TurnStarted,
    TurnCompleted,
    ItemStarted,
    ItemCompleted,
    AgentMessageDelta,
    CommandOutputDelta,
    ServerRequestResolved,
}
impl NotificationKind {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "thread/started" => Some(Self::ThreadStarted),
            "turn/started" => Some(Self::TurnStarted),
            "turn/completed" => Some(Self::TurnCompleted),
            "item/started" => Some(Self::ItemStarted),
            "item/completed" => Some(Self::ItemCompleted),
            "item/agentMessage/delta" => Some(Self::AgentMessageDelta),
            "item/commandExecution/outputDelta" => Some(Self::CommandOutputDelta),
            "serverRequest/resolved" => Some(Self::ServerRequestResolved),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalKind {
    Command,
    FileChange,
}
impl ApprovalKind {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "item/commandExecution/requestApproval" => Some(Self::Command),
            "item/fileChange/requestApproval" => Some(Self::FileChange),
            _ => None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approval {
    pub id: RpcId,
    pub kind: ApprovalKind,
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub started_at_ms: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcFailure {
    pub code: i64,
    pub message: String,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Reply {
        id: u64,
        method: ClientMethod,
        outcome: std::result::Result<Value, RpcFailure>,
    },
    Notification {
        kind: NotificationKind,
        params: Value,
    },
    Approval(Approval),
}
struct Outbound {
    id: Option<u64>,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cancelled {
    Unsent,
    Detached,
}

/// Fully bounded per-session broker. Stdio ownership and process termination belong to NC-04.
/// All incoming errors are fatal to this session; do not keep reading after an error.
pub struct Broker {
    limits: Limits,
    phase: Phase,
    next_id: u64,
    pending: BTreeMap<u64, ClientMethod>,
    approvals: BTreeMap<RpcId, Approval>,
    outbound: VecDeque<Outbound>,
    outbound_bytes: usize,
    events: VecDeque<(Event, usize)>,
    event_bytes: usize,
}
impl Broker {
    pub fn new(limits: Limits) -> Result<Self> {
        if !limits.valid() {
            return Err(BridgeError::InvalidRequest);
        }
        Ok(Self {
            limits,
            phase: Phase::Fresh,
            next_id: 0,
            pending: BTreeMap::new(),
            approvals: BTreeMap::new(),
            outbound: VecDeque::new(),
            outbound_bytes: 0,
            events: VecDeque::new(),
            event_bytes: 0,
        })
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
    pub fn event_count(&self) -> usize {
        self.events.len()
    }
    pub fn event_bytes(&self) -> usize {
        self.event_bytes
    }
    pub fn outbound_bytes(&self) -> usize {
        self.outbound_bytes
    }
    pub fn approval_count(&self) -> usize {
        self.approvals.len()
    }
    /// A request identity is bound to the original pending approval, not merely its RPC id.
    pub fn approval(&self, id: &RpcId) -> Option<&Approval> {
        self.approvals.get(id)
    }
    /// Interrupting a turn must decline all still-pending requests, never replay them.
    pub fn decline_approvals_for_turn(&mut self, thread_id: &str, turn_id: &str) -> Result<usize> {
        let ids: Vec<_> = self
            .approvals
            .values()
            .filter(|a| a.thread_id == thread_id && a.turn_id == turn_id)
            .map(|a| a.id.clone())
            .collect();
        for id in &ids {
            self.decline_approval(id)?;
        }
        Ok(ids.len())
    }

    fn encode(&self, value: &Value) -> Result<Vec<u8>> {
        let mut bytes = serde_json::to_vec(value).map_err(|_| BridgeError::InvalidJson)?;
        if bytes.len() > self.limits.line_bytes {
            return Err(BridgeError::LineTooLarge);
        }
        bytes.push(b'\n');
        Ok(bytes)
    }
    fn queue(&mut self, id: Option<u64>, message: Value) -> Result<()> {
        let bytes = self.encode(&message)?;
        if self.outbound.len() >= self.limits.outbound
            || self
                .outbound_bytes
                .checked_add(bytes.len())
                .is_none_or(|n| n > self.limits.outbound_bytes)
        {
            return Err(BridgeError::Backpressure);
        }
        self.outbound_bytes += bytes.len();
        self.outbound.push_back(Outbound { id, bytes });
        Ok(())
    }
    fn push_event(&mut self, event: Event, size: usize) -> Result<()> {
        if self.events.len() >= self.limits.events
            || self
                .event_bytes
                .checked_add(size)
                .is_none_or(|n| n > self.limits.event_bytes)
        {
            return Err(BridgeError::Backpressure);
        }
        self.event_bytes += size;
        self.events.push_back((event, size));
        Ok(())
    }
    fn call_internal(&mut self, method: ClientMethod, params: Value) -> Result<u64> {
        if self.pending.len() >= self.limits.pending {
            return Err(BridgeError::Backpressure);
        }
        let id = self.next_id;
        let following = id.checked_add(1).ok_or(BridgeError::IdExhausted)?;
        self.queue(
            Some(id),
            json!({"id": id, "method": method.name(), "params": params}),
        )?;
        self.pending.insert(id, method);
        self.next_id = following;
        Ok(id)
    }
    pub fn begin_initialize(&mut self) -> Result<u64> {
        if self.phase != Phase::Fresh {
            return Err(BridgeError::InvalidState);
        }
        let id = self.call_internal(ClientMethod::Initialize,
            json!({"clientInfo":{"name":"ultima","title":"Ultima","version":"0.1.0"},"capabilities":{"experimentalApi":false}}))?;
        self.phase = Phase::Initializing;
        Ok(id)
    }
    pub fn acknowledge_initialized(&mut self) -> Result<()> {
        if self.phase != Phase::Acknowledgeable {
            return Err(BridgeError::InvalidState);
        }
        self.queue(None, json!({"method":"initialized","params":{}}))?;
        self.phase = Phase::Ready;
        Ok(())
    }
    pub fn call(&mut self, method: ClientMethod, params: Value) -> Result<u64> {
        if self.phase != Phase::Ready {
            return Err(BridgeError::InvalidState);
        }
        if !method.valid_params(&params) {
            return Err(BridgeError::InvalidRequest);
        }
        self.call_internal(method, params)
    }
    /// This sends the real upstream interrupt RPC, rather than just suppressing a local response.
    pub fn interrupt_turn(&mut self, thread_id: &str, turn_id: &str) -> Result<u64> {
        self.call(
            ClientMethod::TurnInterrupt,
            json!({"threadId": thread_id, "turnId": turn_id}),
        )
    }
    /// Drop UI interest. If unsent, remove the outgoing command too. If already sent,
    /// callers must separately issue turn/interrupt to stop server-side work.
    pub fn cancel_local(&mut self, id: u64) -> Result<Cancelled> {
        if self.pending.get(&id) == Some(&ClientMethod::Initialize) {
            return Err(BridgeError::InvalidState);
        }
        if self.pending.remove(&id).is_none() {
            return Err(BridgeError::InvalidRequest);
        }
        if let Some(pos) = self.outbound.iter().position(|o| o.id == Some(id)) {
            let old = self.outbound.remove(pos).expect("position validated");
            self.outbound_bytes -= old.bytes.len();
            Ok(Cancelled::Unsent)
        } else {
            Ok(Cancelled::Detached)
        }
    }
    pub fn take_outbound(&mut self) -> Option<Vec<u8>> {
        let output = self.outbound.pop_front()?;
        self.outbound_bytes -= output.bytes.len();
        Some(output.bytes)
    }
    pub fn take_event(&mut self) -> Option<Event> {
        let (event, size) = self.events.pop_front()?;
        self.event_bytes -= size;
        Some(event)
    }
    pub fn reply_approval(
        &mut self,
        id: &RpcId,
        decision: ApprovalDecision,
        explicit_user_approval: bool,
    ) -> Result<()> {
        if !self.approvals.contains_key(id) {
            return Err(BridgeError::UnknownApproval);
        }
        if decision == ApprovalDecision::Accept && !explicit_user_approval {
            return Err(BridgeError::ExplicitApprovalRequired);
        }
        self.queue(
            None,
            json!({"id": id, "result": {"decision": decision.as_str()}}),
        )?;
        self.approvals.remove(id);
        Ok(())
    }
    /// Never auto-accept a tool request. The host should decline on rejection or
    /// terminate this session if its outbound queue cannot accept the decline.
    pub fn decide_exact_approval(
        &mut self,
        expected: &Approval,
        decision: ApprovalDecision,
        explicit_user_approval: bool,
    ) -> Result<()> {
        if self.approvals.get(&expected.id) != Some(expected) {
            return Err(BridgeError::UnknownApproval);
        }
        self.reply_approval(&expected.id, decision, explicit_user_approval)
    }
    pub fn decline_approval(&mut self, id: &RpcId) -> Result<()> {
        self.reply_approval(id, ApprovalDecision::Decline, false)
    }
    fn error_reply(&mut self, id: RpcId, code: i64, message: &str) -> Result<()> {
        self.queue(
            None,
            json!({"id": id, "error": {"code": code, "message": message}}),
        )
    }
    pub fn ingest_line(&mut self, line: &[u8]) -> Result<()> {
        if line.len() > self.limits.line_bytes {
            return Err(BridgeError::LineTooLarge);
        }
        let line = std::str::from_utf8(line).map_err(|_| BridgeError::InvalidUtf8)?;
        let value: Value = serde_json::from_str(line).map_err(|_| BridgeError::InvalidJson)?;
        let obj = value.as_object().ok_or(BridgeError::InvalidEnvelope)?;
        if let Some(method_value) = obj.get("method") {
            let method = method_value
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or(BridgeError::InvalidEnvelope)?;
            if obj.contains_key("result") || obj.contains_key("error") {
                return Err(BridgeError::InvalidEnvelope);
            }
            if let Some(raw_id) = obj.get("id") {
                let id = RpcId::parse(raw_id).ok_or(BridgeError::InvalidEnvelope)?;
                if let Some(kind) = ApprovalKind::parse(method) {
                    let p = obj.get("params").and_then(Value::as_object);
                    let field = |name: &str| {
                        p.and_then(|v| v.get(name))
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                    };
                    let started = p.and_then(|v| v.get("startedAtMs")).and_then(Value::as_u64);
                    if p.and_then(|v| v.get("approvalId"))
                        .is_some_and(|v| !v.is_null() && !v.as_str().is_some_and(|s| !s.is_empty()))
                        || ["threadId", "turnId", "itemId"]
                            .iter()
                            .any(|name| field(name).is_none_or(|s| s.len() > 256))
                        || started.is_none()
                    {
                        return self.error_reply(id, -32602, "Invalid approval request");
                    }
                    if self.approvals.len() >= self.limits.approvals {
                        return Err(BridgeError::Backpressure);
                    }
                    if self.approvals.contains_key(&id) {
                        return Err(BridgeError::InvalidEnvelope);
                    }
                    let approval = Approval {
                        id: id.clone(),
                        kind,
                        thread_id: field("threadId").expect("validated"),
                        turn_id: field("turnId").expect("validated"),
                        item_id: field("itemId").expect("validated"),
                        started_at_ms: started.expect("validated"),
                    };
                    self.push_event(Event::Approval(approval.clone()), line.len())?;
                    self.approvals.insert(id, approval);
                } else {
                    self.error_reply(id, -32601, "Unsupported server request")?;
                }
            } else if let Some(kind) = NotificationKind::parse(method) {
                let params = obj
                    .get("params")
                    .filter(|v| v.is_object())
                    .ok_or(BridgeError::InvalidEnvelope)?;
                if kind == NotificationKind::ServerRequestResolved {
                    let id = ["requestId", "serverRequestId", "id"]
                        .iter()
                        .find_map(|key| params.get(key).and_then(RpcId::parse));
                    if let Some(id) = id {
                        self.approvals.remove(&id);
                    }
                }
                self.push_event(
                    Event::Notification {
                        kind,
                        params: params.clone(),
                    },
                    line.len(),
                )?;
            } // Unknown notifications are intentionally not queued.
            return Ok(());
        }
        let id = obj
            .get("id")
            .and_then(Value::as_u64)
            .ok_or(BridgeError::InvalidEnvelope)?;
        let has_result = obj.contains_key("result");
        let has_error = obj.contains_key("error");
        if has_result == has_error {
            return Err(BridgeError::InvalidEnvelope);
        }
        let failure = if has_error {
            let err = obj
                .get("error")
                .and_then(Value::as_object)
                .ok_or(BridgeError::InvalidEnvelope)?;
            let code = err
                .get("code")
                .and_then(Value::as_i64)
                .ok_or(BridgeError::InvalidEnvelope)?;
            let message = err
                .get("message")
                .and_then(Value::as_str)
                .ok_or(BridgeError::InvalidEnvelope)?;
            Some(RpcFailure {
                code,
                message: message.to_string(),
            })
        } else {
            None
        };
        let Some(method) = self.pending.get(&id).copied() else {
            return Ok(());
        }; // late/duplicate response
        let outcome = if let Some(failure) = failure {
            Err(failure)
        } else {
            Ok(obj.get("result").expect("checked").clone())
        };
        if method == ClientMethod::Initialize && outcome.is_ok() {
            let valid = outcome
                .as_ref()
                .ok()
                .and_then(Value::as_object)
                .is_some_and(|o| {
                    ["userAgent", "platformFamily", "platformOs"]
                        .iter()
                        .all(|k| {
                            o.get(*k)
                                .and_then(Value::as_str)
                                .is_some_and(|s| !s.is_empty())
                        })
                });
            if !valid {
                return Err(BridgeError::InvalidEnvelope);
            }
        }
        self.push_event(
            Event::Reply {
                id,
                method,
                outcome: outcome.clone(),
            },
            line.len(),
        )?;
        self.pending.remove(&id);
        if method == ClientMethod::Initialize {
            self.phase = if outcome.is_ok() {
                Phase::Acknowledgeable
            } else {
                Phase::Fresh
            };
        }
        Ok(())
    }
    /// Only safe after the prior process/reader/writer have been stopped and joined.
    pub fn reset_for_new_process(&mut self) {
        self.phase = Phase::Fresh;
        self.pending.clear();
        self.approvals.clear();
        self.outbound.clear();
        self.events.clear();
        self.outbound_bytes = 0;
        self.event_bytes = 0;
        // Keep ID monotonic across restarts; never accidentally route late replies.
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    Accept,
    Decline,
    Cancel,
}
impl ApprovalDecision {
    fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Decline => "decline",
            Self::Cancel => "cancel",
        }
    }
}
