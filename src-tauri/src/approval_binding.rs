//! NC-07/NC-10E cross-renderer approval binding. Frontend snapshots are untrusted.
use serde_json::Value;
use std::path::{Component, Path};

use ultima_protocol_bridge::{Approval, ApprovalKind, LocalCommandEnvironment, NotificationKind};

const MAX_COMMAND: usize = 4096;
const MAX_OUTPUT: usize = 32 * 1024;
const MAX_DIFF: usize = 64 * 1024;
const MAX_PATH: usize = 512;
const MAX_CHANGE: usize = 32;
const MAX_APPROVAL_DESCRIPTOR_BYTES: usize = 8 * 1024;
const MAX_EXEC_POLICY_AMENDMENT_ENTRIES: usize = 32;
const MAX_EXEC_POLICY_AMENDMENT_ITEM_BYTES: usize = 512;
const MAX_EXEC_POLICY_AMENDMENT_BYTES: usize = 4 * 1024;
const MAX_AVAILABLE_DECISIONS: usize = 8;
const MAX_NETWORK_POLICY_HOST_BYTES: usize = 512;
const MAX_COMMAND_ACTIONS: usize = 32;

#[derive(Clone)]
struct Observed {
    thread: String,
    turn: String,
    id: String,
    item: Value,
    state: &'static str,
    changed_output: bool,
}

#[derive(Default)]
pub struct EvidenceLedger {
    items: Vec<Observed>,
}

fn ident<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)?
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 256)
}

fn command(value: &Value) -> Option<String> {
    let normalized = if let Some(text) = value.as_str() {
        text.to_owned()
    } else {
        let args = value.as_array()?;
        if args.is_empty()
            || args.iter().any(|arg| {
                !arg.as_str()
                    .is_some_and(|s| !s.is_empty() && !s.chars().any(char::is_whitespace))
            })
        {
            return None;
        }
        args.iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    };
    (!normalized.trim().is_empty() && normalized.len() <= MAX_COMMAND).then_some(normalized)
}

fn valid_exec_policy_amendment(value: &Value) -> bool {
    let Some(entries) = value.as_array() else {
        return false;
    };
    if entries.len() > MAX_EXEC_POLICY_AMENDMENT_ENTRIES {
        return false;
    }
    let mut total = 0usize;
    entries.iter().all(|entry| {
        let Some(token) = entry.as_str() else {
            return false;
        };
        if token.len() > MAX_EXEC_POLICY_AMENDMENT_ITEM_BYTES {
            return false;
        }
        total = total.saturating_add(token.len());
        total <= MAX_EXEC_POLICY_AMENDMENT_BYTES
    })
}

fn optional_action_string(
    action: &serde_json::Map<String, Value>,
    key: &str,
    limit: usize,
) -> bool {
    match action.get(key) {
        None | Some(Value::Null) => true,
        Some(Value::String(value)) => value.len() <= limit,
        _ => false,
    }
}

fn valid_command_action(value: &Value) -> bool {
    let Some(action) = value.as_object() else {
        return false;
    };
    let Some(kind) = action.get("type").and_then(Value::as_str) else {
        return false;
    };
    let Some(command) = action.get("command").and_then(Value::as_str) else {
        return false;
    };
    if command.len() > MAX_COMMAND {
        return false;
    }
    match kind {
        "read" => {
            action
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name.len() <= MAX_PATH)
                && action
                    .get("path")
                    .and_then(Value::as_str)
                    .is_some_and(|path| path.len() <= MAX_PATH)
        }
        "listFiles" => optional_action_string(action, "path", MAX_PATH),
        "search" => {
            optional_action_string(action, "path", MAX_PATH)
                && optional_action_string(action, "query", MAX_PATH)
        }
        "unknown" => true,
        _ => false,
    }
}

fn valid_command_actions(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => true,
        Some(Value::Array(actions)) => {
            actions.len() <= MAX_COMMAND_ACTIONS && actions.iter().all(valid_command_action)
        }
        _ => false,
    }
}

fn valid_available_decision(value: &Value) -> bool {
    match value.as_str() {
        Some("accept" | "acceptForSession" | "decline" | "cancel") => true,
        Some(_) => false,
        None => {
            let Some(choice) = value.as_object() else {
                return false;
            };
            if choice.len() != 1 {
                return false;
            }
            if let Some(payload) = choice.get("acceptWithExecpolicyAmendment") {
                let Some(payload) = payload.as_object() else {
                    return false;
                };
                payload.len() == 1
                    && payload
                        .get("execpolicy_amendment")
                        .is_some_and(valid_exec_policy_amendment)
            } else if let Some(payload) = choice.get("applyNetworkPolicyAmendment") {
                let Some(payload) = payload.as_object() else {
                    return false;
                };
                let Some(amendment) = payload
                    .get("network_policy_amendment")
                    .and_then(Value::as_object)
                else {
                    return false;
                };
                payload.len() == 1
                    && amendment.len() == 2
                    && amendment
                        .get("host")
                        .and_then(Value::as_str)
                        .is_some_and(|host| host.len() <= MAX_NETWORK_POLICY_HOST_BYTES)
                    && matches!(
                        amendment.get("action").and_then(Value::as_str),
                        Some("allow" | "deny")
                    )
            } else {
                false
            }
        }
    }
}

fn one_shot_accept_offered(request: &Value) -> bool {
    match request.get("availableDecisions") {
        None | Some(Value::Null) => true,
        Some(Value::Array(decisions)) => {
            decisions.len() <= MAX_AVAILABLE_DECISIONS
                && decisions.iter().all(valid_available_decision)
                && decisions
                    .iter()
                    .any(|decision| decision.as_str() == Some("accept"))
        }
        _ => false,
    }
}

fn scope(request: &Value, kind: ApprovalKind, project: &Path) -> bool {
    let Some(map) = request.as_object() else {
        return false;
    };
    if map.keys().any(|k| {
        !matches!(
            k.as_str(),
            "threadId"
                | "turnId"
                | "itemId"
                | "startedAtMs"
                | "kind"
                | "environmentId"
                | "approvalId"
                | "commandActions"
                | "command"
                | "cwd"
                | "grantRoot"
                | "availableDecisions"
                | "reason"
                | "additionalPermissions"
                | "networkApprovalContext"
                | "proposedExecpolicyAmendment"
                | "proposedNetworkPolicyAmendments"
        )
    }) {
        return false;
    }
    if !serde_json::to_vec(request)
        .is_ok_and(|serialized| serialized.len() <= MAX_APPROVAL_DESCRIPTOR_BYTES)
        || [
            "additionalPermissions",
            "networkApprovalContext",
            "proposedNetworkPolicyAmendments",
        ]
        .iter()
        .any(|key| request.get(*key).is_some_and(|v| !v.is_null()))
    {
        return false;
    }
    if let Some(proposal) = request
        .get("proposedExecpolicyAmendment")
        .filter(|value| !value.is_null())
    {
        if kind != ApprovalKind::Command || !valid_exec_policy_amendment(proposal) {
            return false;
        }
    }
    if !one_shot_accept_offered(request) {
        return false;
    }
    if kind == ApprovalKind::Command {
        if request
            .get("kind")
            .is_some_and(|value| value.as_str() != Some("command"))
            || LocalCommandEnvironment::parse(request.get("environmentId")).is_none()
            || request
                .get("approvalId")
                .is_some_and(|value| !value.is_null())
            || !valid_command_actions(request.get("commandActions"))
        {
            return false;
        }
    } else if ["kind", "environmentId", "approvalId", "commandActions"]
        .iter()
        .any(|key| request.get(*key).is_some())
    {
        return false;
    }
    if let Some(reason) = request.get("reason").filter(|value| !value.is_null()) {
        if reason.as_str().is_none_or(|s| s.len() > MAX_COMMAND) {
            return false;
        }
    }
    match kind {
        ApprovalKind::Command => {
            request.get("grantRoot").is_none()
                && request.get("command").and_then(command).is_some()
                && request.get("cwd").and_then(Value::as_str) == project.to_str()
        }
        ApprovalKind::FileChange => {
            request.get("command").is_none()
                && request.get("cwd").is_none()
                && request
                    .get("grantRoot")
                    .is_none_or(|root| root.as_str() == project.to_str())
        }
    }
}

fn safe_path(path: &str, project: &Path) -> bool {
    if path.is_empty()
        || path.len() > MAX_PATH
        || (path.contains('\\') || path.contains(':') || path.contains('%'))
        || path.chars().any(char::is_control)
        || path.starts_with('/')
        || path.ends_with('/')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return false;
    }
    let Ok(root) = project.canonicalize() else {
        return false;
    };
    let mut current = project.to_path_buf();
    for component in Path::new(path).components() {
        current.push(component.as_os_str());
        if current.symlink_metadata().is_ok()
            && !current.canonicalize().is_ok_and(|p| p.starts_with(&root))
        {
            return false;
        }
    }
    true
}

fn full_diff(item: &Value, project: &Path) -> Option<(Vec<String>, String)> {
    let changes = item.get("changes")?.as_array()?;
    if changes.is_empty()
        || changes.len() > MAX_CHANGE
        || ["truncated", "isTruncated", "partial", "hasMore"]
            .iter()
            .any(|k| item.get(*k).and_then(Value::as_bool) == Some(true))
        || ["totalChanges", "changeCount"].iter().any(|k| {
            item.get(*k)
                .is_some_and(|v| v.as_u64() != Some(changes.len() as u64))
        })
    {
        return None;
    }
    let mut paths = Vec::new();
    let mut full = String::new();
    for change in changes {
        let path = change.get("path")?.as_str()?;
        let diff = change.get("diff")?.as_str()?;
        if !safe_path(path, project)
            || diff.trim().is_empty()
            || paths.iter().any(|p| p == path)
            || ["truncated", "isTruncated", "partial", "hasMore"]
                .iter()
                .any(|k| change.get(*k).and_then(Value::as_bool) == Some(true))
        {
            return None;
        }
        let segment = format!(
            "{}{}\n{}",
            if full.is_empty() { "" } else { "\n\n" },
            path,
            diff
        );
        if segment.len() > MAX_DIFF.saturating_sub(full.len()) {
            return None;
        }
        full.push_str(&segment);
        paths.push(path.to_owned());
    }
    Some((paths, full))
}

impl EvidenceLedger {
    pub fn observe(&mut self, kind: NotificationKind, params: &Value) {
        let (Some(thread), Some(turn)) = (ident(params, "threadId"), ident(params, "turnId"))
        else {
            return;
        };
        if kind == NotificationKind::CommandOutputDelta {
            if let Some(id) = ident(params, "itemId") {
                if let Some(entry) = self
                    .items
                    .iter_mut()
                    .find(|entry| entry.thread == thread && entry.turn == turn && entry.id == id)
                {
                    // Output deltas make the previously rendered item snapshot non-atomic.
                    entry.changed_output = true;
                }
            }
            return;
        }
        if kind != NotificationKind::ItemStarted && kind != NotificationKind::ItemCompleted {
            return;
        }
        let Some(item) = params.get("item") else {
            return;
        };
        let Some(id) = ident(item, "id") else {
            return;
        };
        let state = if kind == NotificationKind::ItemCompleted {
            "complete"
        } else {
            "running"
        };
        let oversized = serde_json::to_vec(item).map_or(true, |s| s.len() > 128 * 1024);
        let entry = Observed {
            thread: thread.to_owned(),
            turn: turn.to_owned(),
            id: id.to_owned(),
            item: if oversized { Value::Null } else { item.clone() },
            state,
            changed_output: oversized,
        };
        self.items
            .retain(|old| old.thread != thread || old.turn != turn || old.id != id);
        self.items.push(entry);
        if self.items.len() > 16 {
            self.items.remove(0);
        }
    }

    pub fn matches(&self, approval: &Approval, reviewed: &Value, project: &Path) -> bool {
        let Some(current) = self.items.iter().find(|item| {
            item.thread == approval.thread_id
                && item.turn == approval.turn_id
                && item.id == approval.item_id
        }) else {
            return false;
        };
        if current.changed_output
            || !scope(&approval.request, approval.kind, project)
            || reviewed.get("threadId").and_then(Value::as_str) != Some(current.thread.as_str())
            || reviewed.get("turnId").and_then(Value::as_str) != Some(current.turn.as_str())
            || reviewed.get("id").and_then(Value::as_str) != Some(current.id.as_str())
            || reviewed.get("state").and_then(Value::as_str) != Some(current.state)
            || reviewed.get("previewComplete").and_then(Value::as_bool) != Some(true)
            || reviewed.get("truncated").and_then(Value::as_bool) != Some(false)
            || approval.request.get("threadId").and_then(Value::as_str)
                != Some(current.thread.as_str())
            || approval.request.get("turnId").and_then(Value::as_str) != Some(current.turn.as_str())
            || approval.request.get("itemId").and_then(Value::as_str) != Some(current.id.as_str())
            || approval.request.get("startedAtMs").and_then(Value::as_u64)
                != Some(approval.started_at_ms)
        {
            return false;
        }
        match approval.kind {
            ApprovalKind::Command => {
                if current.item.get("type").and_then(Value::as_str) != Some("commandExecution")
                    || reviewed.get("kind").and_then(Value::as_str) != Some("command")
                {
                    return false;
                }
                let Some(actual) = current.item.get("command") else {
                    return false;
                };
                let Some(actual_text) = command(actual) else {
                    return false;
                };
                let Some(request_text) = approval.request.get("command").and_then(command) else {
                    return false;
                };
                let Some(cwd) = approval.request.get("cwd").and_then(Value::as_str) else {
                    return false;
                };
                let Ok(root) = project.canonicalize() else {
                    return false;
                };
                actual_text == request_text
                    && approval.request.get("command") == Some(actual)
                    && current.item.get("cwd").and_then(Value::as_str) == Some(cwd)
                    && reviewed.get("cwd").and_then(Value::as_str) == Some(cwd)
                    && Path::new(cwd).canonicalize().ok().as_ref() == Some(&root)
                    && reviewed.get("command").and_then(Value::as_str) == Some(actual_text.as_str())
                    && reviewed.get("commandDescriptor") == Some(actual)
                    && current.item.get("aggregatedOutput").is_none_or(|v| {
                        v.as_str().is_some_and(|s| {
                            s.len() <= MAX_OUTPUT
                                && reviewed.get("output").and_then(Value::as_str) == Some(s)
                        })
                    })
                    && (current.item.get("aggregatedOutput").is_some()
                        || reviewed.get("output").and_then(Value::as_str) == Some(""))
            }
            ApprovalKind::FileChange => {
                if current.item.get("type").and_then(Value::as_str) != Some("fileChange")
                    || reviewed.get("kind").and_then(Value::as_str) != Some("fileChange")
                {
                    return false;
                }
                if current.item.get("cwd").is_some_and(|v| !v.is_string()) {
                    return false;
                }
                let item_cwd = current
                    .item
                    .get("cwd")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if (!item_cwd.is_empty()
                    && (item_cwd != project.to_str().unwrap_or("")
                        || Path::new(item_cwd).canonicalize().ok() != project.canonicalize().ok()))
                    || reviewed.get("cwd").and_then(Value::as_str) != Some(item_cwd)
                {
                    return false;
                }
                let Some((paths, diff)) = full_diff(&current.item, project) else {
                    return false;
                };
                reviewed.get("diff").and_then(Value::as_str) == Some(diff.as_str())
                    && reviewed
                        .get("paths")
                        .and_then(Value::as_array)
                        .is_some_and(|array| {
                            array.len() == paths.len()
                                && array
                                    .iter()
                                    .zip(paths.iter())
                                    .all(|(v, p)| v.as_str() == Some(p))
                        })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use ultima_protocol_bridge::{Approval, ApprovalKind, RpcId};
    fn project() -> std::path::PathBuf {
        std::env::current_dir().unwrap()
    }
    fn req(cmd: &str, cwd: &str) -> Approval {
        Approval {
            id: RpcId::String("req".into()),
            kind: ApprovalKind::Command,
            thread_id: "t".into(),
            turn_id: "u".into(),
            item_id: "i".into(),
            started_at_ms: 1,
            request: json!({"threadId":"t","turnId":"u","itemId":"i",
                "startedAtMs":1,"kind":"command","environmentId":null,"approvalId":null,
                "commandActions":[{"type":"unknown","command":cmd}],
                "command":cmd,"cwd":cwd,"availableDecisions":["accept","decline"]}),
        }
    }
    fn observe_cmd(ledger: &mut EvidenceLedger, cmd: &str, cwd: &str) {
        ledger.observe(
            NotificationKind::ItemStarted,
            &json!({
                "threadId":"t","turnId":"u",
                "item":{"id":"i","type":"commandExecution","command":cmd,"cwd":cwd}
            }),
        );
    }
    fn reviewed(cmd: &str, cwd: &str) -> Value {
        json!({"id":"i","threadId":"t","turnId":"u","kind":"command",
            "command":cmd,"commandDescriptor":cmd,"cwd":cwd,
            "output":"","state":"running","previewComplete":true,"truncated":false})
    }
    #[test]
    fn official_local_environment_fixture_and_unsupported_identifiers() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/nc10g-local-approval.json"
        ))
        .unwrap();
        let cases: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/nc10g-environments.json"))
                .unwrap();
        let root = project();
        let cwd = root.to_str().unwrap();
        let mut a = req("echo safe", cwd);
        a.request = fixture["params"].clone();
        a.request["cwd"] = json!(cwd);
        let mut ledger = EvidenceLedger::default();
        observe_cmd(&mut ledger, "echo safe", cwd);
        let snapshot = reviewed("echo safe", cwd);
        assert!(ledger.matches(&a, &snapshot, &root));
        for case in cases.as_array().unwrap() {
            let mut changed = a.clone();
            changed.request["environmentId"] = case["value"].clone();
            assert_eq!(
                ledger.matches(&changed, &snapshot, &root),
                case["supported"].as_bool().unwrap(),
                "{}",
                case["value"]
            );
        }
        // No identity can compensate for missing evidence or a foreign project.
        assert!(!ledger.matches(&a, &Value::Null, &root));
        assert!(!ledger.matches(&a, &snapshot, root.parent().unwrap()));
    }
    #[test]
    fn host_evidence_cannot_replace_broker_environment_binding() {
        use ultima_protocol_bridge::{ApprovalDecision, Broker, Event, Limits};
        let root = project();
        let cwd = root.to_str().unwrap();
        let mut fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/nc10g-local-approval.json"
        ))
        .unwrap();
        fixture["params"]["cwd"] = json!(cwd);
        let mut broker = Broker::new(Limits::default()).unwrap();
        broker.begin_initialize().unwrap();
        broker.take_outbound().unwrap();
        broker.ingest_line(br#"{"id":0,"result":{"userAgent":"fixture","platformFamily":"windows","platformOs":"windows"}}"#).unwrap();
        broker.acknowledge_initialized().unwrap();
        broker.take_outbound().unwrap();
        broker.take_event().unwrap();
        broker
            .ingest_line(&serde_json::to_vec(&fixture).unwrap())
            .unwrap();
        let Event::Approval(original) = broker.take_event().unwrap() else {
            panic!("approval");
        };
        let mut ledger = EvidenceLedger::default();
        observe_cmd(&mut ledger, "echo safe", cwd);
        let snapshot = reviewed("echo safe", cwd);
        for env in [json!("remote"), Value::Null] {
            let mut changed = original.clone();
            changed.request["environmentId"] = env;
            // Same command/cwd evidence is insufficient, including for legacy local.
            let dispatched = ledger.matches(&changed, &snapshot, &root)
                && broker
                    .decide_exact_approval(&changed, ApprovalDecision::Accept, true)
                    .is_ok();
            assert!(!dispatched);
            assert!(broker.take_outbound().is_none());
        }
        assert!(ledger.matches(&original, &snapshot, &root));
        broker
            .decide_exact_approval(&original, ApprovalDecision::Accept, true)
            .unwrap();
        let wire: Value = serde_json::from_slice(&broker.take_outbound().unwrap()).unwrap();
        assert_eq!(
            wire,
            json!({"id":"local-once","result":{"decision":"accept"}})
        );
        assert!(broker
            .decide_exact_approval(&original, ApprovalDecision::Accept, true)
            .is_err());
    }

    #[test]
    fn cross_renderer_command_scope_and_mutations_fail_closed() {
        let root = project();
        let cwd = root.to_str().unwrap();
        let mut ledger = EvidenceLedger::default();
        observe_cmd(&mut ledger, "echo safe", cwd);
        let original = req("echo safe", cwd);
        let snapshot = reviewed("echo safe", cwd);
        assert!(ledger.matches(&original, &snapshot, &root));
        assert!(!ledger.matches(&req("echo evil", cwd), &snapshot, &root));
        assert!(!ledger.matches(&req("echo safe", "/foreign"), &snapshot, &root));
        let mut elevated = original.clone();
        elevated.request["additionalPermissions"] = json!({"network":true});
        assert!(!ledger.matches(&elevated, &snapshot, &root));
        elevated = original.clone();
        elevated.request["availableDecisions"] = json!(["accept", "acceptForSession"]);
        assert!(ledger.matches(&elevated, &snapshot, &root));
        elevated.request["availableDecisions"] = json!(["acceptForSession"]);
        assert!(!ledger.matches(&elevated, &snapshot, &root));
        elevated.request["availableDecisions"] = json!(["accept", 7]);
        assert!(!ledger.matches(&elevated, &snapshot, &root));
        for key in [
            "permissionScope",
            "unexpectedField",
            "permissionGrant",
            "requestedPermissions",
            "permissionProfile",
        ] {
            elevated = original.clone();
            elevated.request[key] = json!({"enabled":true});
            assert!(!ledger.matches(&elevated, &snapshot, &root));
        }
        let mut preview_changed = snapshot.clone();
        preview_changed["command"] = json!("echo evil");
        assert!(!ledger.matches(&original, &preview_changed, &root));
        observe_cmd(&mut ledger, "echo evil", cwd);
        assert!(!ledger.matches(&original, &snapshot, &root));
        ledger.observe(
            NotificationKind::ItemStarted,
            &json!({"threadId":"t","turnId":"u",
                "item":{"id":"i","type":"commandExecution","command":["echo","safe"],"cwd":cwd}}),
        );
        let mut request_vector = original.clone();
        request_vector.request["command"] = json!(["echo", "safe"]);
        let mut reviewed_vector = reviewed("echo safe", cwd);
        reviewed_vector["commandDescriptor"] = json!(["echo", "safe"]);
        assert!(!ledger.matches(&original, &reviewed_vector, &root));
        assert!(ledger.matches(&request_vector, &reviewed_vector, &root));
    }
    #[test]
    fn file_change_full_diff_unsafe_path_and_output_race_denied() {
        let root = project();
        let cwd = root.to_str().unwrap();
        let mut ledger = EvidenceLedger::default();
        observe_cmd(&mut ledger, "echo safe", cwd);
        let original = req("echo safe", cwd);
        let snapshot = reviewed("echo safe", cwd);
        ledger.observe(
            NotificationKind::CommandOutputDelta,
            &json!({"threadId":"t","turnId":"u","itemId":"i","delta":"changed"}),
        );
        assert!(!ledger.matches(&original, &snapshot, &root));
        let mut file = original.clone();
        file.kind = ApprovalKind::FileChange;
        file.request = json!({"threadId":"t","turnId":"u","itemId":"i","startedAtMs":1});
        let good = json!({"threadId":"t","turnId":"u","item":{"id":"i","type":"fileChange",
            "changes":[{"path":"safe.txt","diff":"+ok"}]}});
        ledger.observe(NotificationKind::ItemStarted, &good);
        let preview = json!({"id":"i","threadId":"t","turnId":"u","kind":"fileChange",
            "state":"running","previewComplete":true,"truncated":false,
            "cwd":"","paths":["safe.txt"],"diff":"safe.txt\n+ok"});
        assert!(ledger.matches(&file, &preview, &root));
        for env in [Value::Null, json!("local")] {
            let mut changed = file.clone();
            changed.request["environmentId"] = env;
            assert!(!ledger.matches(&changed, &preview, &root));
        }
        let mut missing_cwd = preview.clone();
        missing_cwd.as_object_mut().unwrap().remove("cwd");
        assert!(!ledger.matches(&file, &missing_cwd, &root));
        let mut truncated = preview.clone();
        truncated["diff"] = json!("safe.txt\n+");
        assert!(!ledger.matches(&file, &truncated, &root));
        let unsafe_change = json!({"threadId":"t","turnId":"u","item":{"id":"i",
            "type":"fileChange","changes":[{"path":"../escape","diff":"+evil"}]}});
        ledger.observe(NotificationKind::ItemStarted, &unsafe_change);
        assert!(!ledger.matches(&file, &preview, &root));
    }

    #[test]
    fn bounded_execpolicy_proposal_is_metadata_and_persistent_choices_are_not_granted() {
        let root = project();
        let cwd = root.to_str().unwrap();
        let mut ledger = EvidenceLedger::default();
        observe_cmd(&mut ledger, "echo safe", cwd);
        let snapshot = reviewed("echo safe", cwd);
        let mut proposal = req("echo safe", cwd);
        proposal.request["proposedExecpolicyAmendment"] = json!(["echo", "safe"]);
        proposal.request["availableDecisions"] = json!([
            "accept",
            "acceptForSession",
            {"acceptWithExecpolicyAmendment":{"execpolicy_amendment":["echo","safe"]}},
            {"applyNetworkPolicyAmendment":{"network_policy_amendment":{"action":"allow","host":"example.com"}}},
            "decline",
            "cancel"
        ]);
        let original = proposal.request.clone();
        assert!(ledger.matches(&proposal, &snapshot, &root));
        assert_eq!(proposal.request, original);

        let mut null_proposal = req("echo safe", cwd);
        null_proposal.request["proposedExecpolicyAmendment"] = Value::Null;
        null_proposal.request["reason"] = Value::Null;
        assert!(ledger.matches(&null_proposal, &snapshot, &root));

        let mut wrong_kind = proposal.clone();
        wrong_kind.request["kind"] = json!("writeStdin");
        assert!(!ledger.matches(&wrong_kind, &snapshot, &root));
        let mut foreign_environment = proposal.clone();
        foreign_environment.request["environmentId"] = json!("remote");
        assert!(!ledger.matches(&foreign_environment, &snapshot, &root));
        let mut callback_scoped = proposal.clone();
        callback_scoped.request["approvalId"] = json!("callback-1");
        assert!(!ledger.matches(&callback_scoped, &snapshot, &root));
        let mut malformed_actions = proposal.clone();
        malformed_actions.request["commandActions"] =
            json!([{"type":"read","command":"cat","name":"file"}]);
        assert!(!ledger.matches(&malformed_actions, &snapshot, &root));

        let mut malformed = vec![json!("echo"), json!({"command":"echo"}), json!([1])];
        malformed.push(json!(
            ["x".repeat(MAX_EXEC_POLICY_AMENDMENT_ITEM_BYTES + 1)]
        ));
        malformed.push(json!((0..=MAX_EXEC_POLICY_AMENDMENT_ENTRIES)
            .map(|_| "x")
            .collect::<Vec<_>>()));
        for value in malformed {
            let mut candidate = req("echo safe", cwd);
            candidate.request["proposedExecpolicyAmendment"] = value;
            assert!(!ledger.matches(&candidate, &snapshot, &root));
        }

        for key in [
            "additionalPermissions",
            "networkApprovalContext",
            "proposedNetworkPolicyAmendments",
        ] {
            let mut candidate = req("echo safe", cwd);
            candidate.request[key] = json!({"enabled":true});
            assert!(!ledger.matches(&candidate, &snapshot, &root));
        }

        for invalid_choice in [
            json!({"acceptWithExecpolicyAmendment":{
                "execpolicy_amendment":["echo","safe"],"source":"server"
            }}),
            json!({"applyNetworkPolicyAmendment":{
                "network_policy_amendment":{"action":"allow","host":"example.com","source":"server"}
            }}),
            json!({"applyNetworkPolicyAmendment":{
                "network_policy_amendment":{"action":"allow","host":"example.com"},
                "source":"server"
            }}),
            json!({
                "acceptWithExecpolicyAmendment":{"execpolicy_amendment":["echo","safe"]},
                "decline":null
            }),
        ] {
            let mut candidate = req("echo safe", cwd);
            candidate.request["availableDecisions"] = json!(["accept", invalid_choice]);
            assert!(!ledger.matches(&candidate, &snapshot, &root));
        }

        let mut no_accept = req("echo safe", cwd);
        no_accept.request["availableDecisions"] = json!([
            {"acceptWithExecpolicyAmendment":{"execpolicy_amendment":["echo","safe"]}}
        ]);
        assert!(!ledger.matches(&no_accept, &snapshot, &root));
        let mut null_decisions = req("echo safe", cwd);
        null_decisions.request["availableDecisions"] = Value::Null;
        assert!(ledger.matches(&null_decisions, &snapshot, &root));
        let mut stale = proposal.clone();
        stale.request["startedAtMs"] = json!(2);
        assert!(!ledger.matches(&stale, &snapshot, &root));

        let long_command = "x".repeat(MAX_COMMAND);
        let mut oversized = req(&long_command, cwd);
        oversized.request["reason"] = json!("r".repeat(MAX_COMMAND));
        let mut long_ledger = EvidenceLedger::default();
        observe_cmd(&mut long_ledger, &long_command, cwd);
        let long_snapshot = reviewed(&long_command, cwd);
        assert!(!long_ledger.matches(&oversized, &long_snapshot, &root));
    }
}
