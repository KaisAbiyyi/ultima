//! NC-07/NC-10E cross-renderer approval binding. Frontend snapshots are untrusted.
use serde_json::Value;
use std::path::{Component, Path};

use ultima_protocol_bridge::{Approval, ApprovalKind, NotificationKind};

const MAX_COMMAND: usize = 4096;
const MAX_OUTPUT: usize = 32 * 1024;
const MAX_DIFF: usize = 64 * 1024;
const MAX_PATH: usize = 512;
const MAX_CHANGE: usize = 32;

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
    value.get(name)?.as_str().filter(|s| !s.is_empty() && s.len() <= 256)
}

fn command(value: &Value) -> Option<String> {
    let normalized = if let Some(text) = value.as_str() {
        text.to_owned()
    } else {
        let args = value.as_array()?;
        if args.is_empty()
            || args.iter().any(|arg| {
                !arg.as_str().is_some_and(|s| {
                    !s.is_empty() && !s.chars().any(char::is_whitespace)
                })
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

fn scope(request: &Value, kind: ApprovalKind, project: &Path) -> bool {
    let Some(map) = request.as_object() else {
        return false;
    };
    if map.keys().any(|k| {
        !matches!(
            k.as_str(),
            "threadId" | "turnId" | "itemId" | "startedAtMs" | "command"
                | "cwd" | "grantRoot" | "availableDecisions" | "reason"
                | "additionalPermissions" | "networkApprovalContext"
                | "proposedExecpolicyAmendment" | "proposedNetworkPolicyAmendments"
        )
    }) {
        return false;
    }
    if [
        "additionalPermissions",
        "networkApprovalContext",
        "proposedExecpolicyAmendment",
        "proposedNetworkPolicyAmendments",
    ]
    .iter()
    .any(|key| request.get(*key).is_some_and(|v| !v.is_null()))
    {
        return false;
    }
    if let Some(reason) = request.get("reason") {
        if !reason.as_str().is_some_and(|s| s.len() <= MAX_COMMAND) {
            return false;
        }
    }
    if let Some(decisions) = request.get("availableDecisions") {
        let Some(list) = decisions.as_array() else {
            return false;
        };
        if !list.iter().any(|v| v.as_str() == Some("accept"))
            || list.iter().any(|v| {
                !matches!(v.as_str(), Some("accept" | "decline" | "cancel"))
            })
        {
            return false;
        }
    }
    match kind {
        ApprovalKind::Command => {
            request.get("grantRoot").is_none()
                && request.get("command").and_then(command).is_some()
                && request.get("cwd").and_then(Value::as_str)
                    == project.to_str()
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
        || path.split('/').any(|part| part.is_empty() || part == "." || part == "..")
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
        || ["totalChanges", "changeCount"]
            .iter()
            .any(|k| item.get(*k).is_some_and(|v| v.as_u64() != Some(changes.len() as u64)))
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
        let segment = format!("{}{}\n{}", if full.is_empty() { "" } else { "\n\n" }, path, diff);
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
        self.items.retain(|old| {
            old.thread != thread || old.turn != turn || old.id != id
        });
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
            || approval.request.get("threadId").and_then(Value::as_str) != Some(current.thread.as_str())
            || approval.request.get("turnId").and_then(Value::as_str) != Some(current.turn.as_str())
            || approval.request.get("itemId").and_then(Value::as_str) != Some(current.id.as_str())
            || approval.request.get("startedAtMs").and_then(Value::as_u64) != Some(approval.started_at_ms)
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
                    && current.item.get("cwd").and_then(Value::as_str) == Some(cwd)
                    && reviewed.get("cwd").and_then(Value::as_str) == Some(cwd)
                    && Path::new(cwd).canonicalize().ok().as_ref() == Some(&root)
                    && reviewed.get("command").and_then(Value::as_str) == Some(actual_text.as_str())
                    && reviewed.get("commandDescriptor") == Some(actual)
                    && current.item.get("aggregatedOutput").is_none_or(|v| {
                        v.as_str().is_some_and(|s| s.len() <= MAX_OUTPUT
                            && reviewed.get("output").and_then(Value::as_str) == Some(s))
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
                let item_cwd = current.item.get("cwd").and_then(Value::as_str).unwrap_or("");
                if (!item_cwd.is_empty() && (item_cwd != project.to_str().unwrap_or("")
                    || Path::new(item_cwd).canonicalize().ok() != project.canonicalize().ok()))
                    || reviewed.get("cwd").and_then(Value::as_str) != Some(item_cwd)
                {
                    return false;
                }
                let Some((paths, diff)) = full_diff(&current.item, project) else {
                    return false;
                };
                reviewed.get("diff").and_then(Value::as_str) == Some(diff.as_str())
                    && reviewed.get("paths").and_then(Value::as_array).is_some_and(|array| {
                        array.len() == paths.len()
                            && array.iter().zip(paths.iter()).all(|(v, p)| v.as_str() == Some(p))
                    })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use ultima_protocol_bridge::{RpcId, ApprovalKind, Approval};
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
                "startedAtMs":1,"command":cmd,"cwd":cwd,
                "availableDecisions":["accept","decline"]}),
        }
    }
    fn observe_cmd(ledger: &mut EvidenceLedger, cmd: &str, cwd: &str) {
        ledger.observe(NotificationKind::ItemStarted, &json!({
            "threadId":"t","turnId":"u",
            "item":{"id":"i","type":"commandExecution","command":cmd,"cwd":cwd}
        }));
    }
    fn reviewed(cmd: &str, cwd: &str) -> Value {
        json!({"id":"i","threadId":"t","turnId":"u","kind":"command",
            "command":cmd,"commandDescriptor":cmd,"cwd":cwd,
            "output":"","state":"running","previewComplete":true,"truncated":false})
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
        elevated.request["availableDecisions"] = json!(["accept","acceptForSession"]);
        assert!(!ledger.matches(&elevated, &snapshot, &root));
        elevated = original.clone();
        elevated.request["permissionScope"] = json!("workspace");
        assert!(!ledger.matches(&elevated, &snapshot, &root));
        let mut preview_changed = snapshot.clone();
        preview_changed["command"] = json!("echo evil");
        assert!(!ledger.matches(&original, &preview_changed, &root));
        observe_cmd(&mut ledger, "echo evil", cwd);
        assert!(!ledger.matches(&original, &snapshot, &root));
    }
    #[test]
    fn file_change_full_diff_unsafe_path_and_output_race_denied() {
        let root = project();
        let cwd = root.to_str().unwrap();
        let mut ledger = EvidenceLedger::default();
        observe_cmd(&mut ledger, "echo safe", cwd);
        let original = req("echo safe", cwd);
        let snapshot = reviewed("echo safe", cwd);
        ledger.observe(NotificationKind::CommandOutputDelta,
            &json!({"threadId":"t","turnId":"u","itemId":"i","delta":"changed"}));
        assert!(!ledger.matches(&original, &snapshot, &root));
        let mut file = original.clone();
        file.kind = ApprovalKind::FileChange;
        file.request = json!({"threadId":"t","turnId":"u","itemId":"i","startedAtMs":1});
        let good = json!({"threadId":"t","turnId":"u","item":{"id":"i","type":"fileChange",
            "changes":[{"path":"safe.txt","diff":"+ok"}]}});
        ledger.observe(NotificationKind::ItemStarted, &good);
        let preview = json!({"id":"i","threadId":"t","turnId":"u","kind":"fileChange",
            "state":"running","previewComplete":true,"truncated":false,
            "paths":["safe.txt"],"diff":"safe.txt\n+ok"});
        assert!(ledger.matches(&file, &preview, &root));
        let mut truncated = preview.clone();
        truncated["diff"] = json!("safe.txt\n+");
        assert!(!ledger.matches(&file, &truncated, &root));
        let unsafe_change = json!({"threadId":"t","turnId":"u","item":{"id":"i",
            "type":"fileChange","changes":[{"path":"../escape","diff":"+evil"}]}});
        ledger.observe(NotificationKind::ItemStarted, &unsafe_change);
        assert!(!ledger.matches(&file, &preview, &root));
    }
}
