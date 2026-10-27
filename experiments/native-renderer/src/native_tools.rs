//! NC-10E: bounded, typed, untrusted Codex tool evidence. Evidence is never authority.
use serde_json::Value;
use std::path::{Component, Path};
use ultima_protocol_bridge::{Approval, ApprovalKind, NotificationKind, RpcId};

pub const MAX_ITEMS: usize = 16;
const MAX_COMMAND: usize = 4 * 1024;
const MAX_OUTPUT: usize = 32 * 1024;
const MAX_DIFF: usize = 64 * 1024;
const MAX_CHANGES: usize = 32;
const MAX_PATH: usize = 512;
const MAX_PENDING: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolEvidence {
    pub thread: String,
    pub turn: String,
    pub id: String,
    pub kind: ApprovalKind,
    pub state: String,
    pub command: String,
    pub cwd: String,
    pub output: String,
    pub diff: String,
    pub paths: Vec<String>,
    pub complete: bool,
    pub truncated: bool,
}

fn identifier(v: Option<&Value>) -> Option<&str> {
    v.and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 256)
}
fn text(v: Option<&Value>, limit: usize) -> (String, bool) {
    let raw = v.and_then(Value::as_str).unwrap_or("");
    let mut end = raw.len().min(limit);
    while !raw.is_char_boundary(end) {
        end -= 1;
    }
    (raw[..end].to_owned(), raw.len() > end)
}
fn safe_path(path: &str, project: &Path) -> bool {
    if path.is_empty()
        || path.len() > MAX_PATH
        || path.contains('\\')
        || path.contains(':')
        || path.contains('%')
        || path.chars().any(char::is_control)
        || path.starts_with('/')
        || path.ends_with('/')
    {
        return false;
    }
    let target = Path::new(path);
    if !target
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
        || path
            .split('/')
            .any(|part| part == "." || part == ".." || part.is_empty())
    {
        return false;
    }
    let Ok(root) = project.canonicalize() else {
        return false;
    };
    // Follow every existing ancestor, including symlinks; reject escape even for a new file.
    let mut ancestor = project.to_path_buf();
    for component in target.components() {
        ancestor.push(component.as_os_str());
        if ancestor.symlink_metadata().is_ok() {
            let Ok(real) = ancestor.canonicalize() else {
                return false;
            };
            if !real.starts_with(&root) {
                return false;
            }
        }
    }
    true
}
fn file_preview(raw: &Value, project: &Path) -> (Vec<String>, String, bool) {
    let Some(changes) = raw.get("changes").and_then(Value::as_array) else {
        return (Vec::new(), String::new(), false);
    };
    if changes.is_empty()
        || changes.len() > MAX_CHANGES
        || ["truncated", "isTruncated", "partial", "hasMore"]
            .iter()
            .any(|k| raw.get(k).and_then(Value::as_bool) == Some(true))
        || ["totalChanges", "changeCount"].iter().any(|k| {
            raw.get(k)
                .is_some_and(|v| v.as_u64() != Some(changes.len() as u64))
        })
    {
        return (Vec::new(), String::new(), false);
    }
    let mut paths = Vec::new();
    let mut diff = String::new();
    for change in changes {
        let Some(path) = identifier(change.get("path")) else {
            return (paths, diff, false);
        };
        let Some(chunk) = change.get("diff").and_then(Value::as_str) else {
            return (paths, diff, false);
        };
        if !safe_path(path, project)
            || chunk.trim().is_empty()
            || paths.iter().any(|p| p == path)
            || ["truncated", "isTruncated", "partial", "hasMore"]
                .iter()
                .any(|k| change.get(k).and_then(Value::as_bool) == Some(true))
        {
            return (paths, diff, false);
        }
        let segment = format!(
            "{}{}\n{}",
            if diff.is_empty() { "" } else { "\n\n" },
            path,
            chunk
        );
        if segment.len() > MAX_DIFF.saturating_sub(diff.len()) {
            return (paths, diff, false);
        }
        diff.push_str(&segment);
        paths.push(path.to_owned());
    }
    (paths, diff, true)
}

#[derive(Default)]
pub struct ToolLedger {
    pub items: Vec<ToolEvidence>,
    pending: Vec<Approval>,
}

impl ToolLedger {
    pub fn clear(&mut self) {
        self.items.clear();
        self.pending.clear();
    }
    pub fn clear_pending(&mut self) {
        self.pending.clear();
    }
    #[cfg(test)]
    pub fn pending(&self) -> &[Approval] {
        &self.pending
    }
    pub fn resolved(&mut self, params: &Value) -> Option<RpcId> {
        let id = ["requestId", "serverRequestId", "id"]
            .iter()
            .find_map(|k| serde_json::from_value::<RpcId>(params.get(k)?.clone()).ok())?;
        self.pending.retain(|p| p.id != id);
        Some(id)
    }
    pub fn capture(
        &mut self,
        kind: NotificationKind,
        params: &Value,
        project: &Path,
    ) -> Option<ToolEvidence> {
        let thread = identifier(params.get("threadId"))?;
        let turn = identifier(params.get("turnId"))?;
        let id = if kind == NotificationKind::CommandOutputDelta {
            identifier(params.get("itemId"))?
        } else {
            identifier(params.get("item").and_then(|v| v.get("id")))?
        };
        let pos = self
            .items
            .iter()
            .position(|item| item.thread == thread && item.turn == turn && item.id == id);
        if kind == NotificationKind::CommandOutputDelta {
            let index = pos?;
            let item = &mut self.items[index];
            if item.kind != ApprovalKind::Command {
                return None;
            }
            let (delta, clipped) = text(params.get("delta"), MAX_OUTPUT);
            let available = MAX_OUTPUT.saturating_sub(item.output.len());
            let mut end = delta.len().min(available);
            while !delta.is_char_boundary(end) {
                end -= 1;
            }
            item.output.push_str(&delta[..end]);
            item.truncated |= clipped || end < delta.len();
            return Some(item.clone());
        }
        let data = params.get("item")?;
        let completed = kind == NotificationKind::ItemCompleted;
        let kind = match data.get("type").and_then(Value::as_str) {
            Some("commandExecution") => ApprovalKind::Command,
            Some("fileChange") => ApprovalKind::FileChange,
            _ => return None,
        };
        let mut item = pos
            .and_then(|i| self.items.get(i).cloned())
            .unwrap_or(ToolEvidence {
                thread: thread.into(),
                turn: turn.into(),
                id: id.into(),
                kind,
                state: "running".into(),
                command: String::new(),
                cwd: String::new(),
                output: String::new(),
                diff: String::new(),
                paths: Vec::new(),
                complete: false,
                truncated: false,
            });
        if item.kind != kind {
            return None;
        }
        item.state = if completed { "completed" } else { "running" }.into();
        if let Some(cwd) = data.get("cwd").and_then(Value::as_str) {
            item.cwd = cwd.to_owned();
        }
        if item.kind == ApprovalKind::Command {
            let command = match data.get("command") {
                Some(Value::String(v)) => v.clone(),
                Some(Value::Array(v)) if v.iter().all(Value::is_string) => v
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" "),
                _ => String::new(),
            };
            let (view, clipped) = text(Some(&Value::String(command.clone())), MAX_COMMAND);
            item.command = view;
            item.truncated |= clipped;
            item.complete = !clipped && !command.trim().is_empty();
            if let Some(raw) = data.get("aggregatedOutput") {
                let (full, clipped) = text(Some(raw), MAX_OUTPUT);
                item.output = full;
                item.truncated |= clipped;
            }
        } else {
            let (paths, diff, valid) = file_preview(data, project);
            item.paths = paths;
            item.diff = diff;
            item.complete = valid;
            item.truncated |= !valid;
        }
        if let Some(index) = pos {
            self.items.remove(index);
        }
        self.items.push(item.clone());
        if self.items.len() > MAX_ITEMS {
            self.items.remove(0);
        }
        Some(item)
    }
    pub fn matches_evidence(&self, approval: &Approval, project: &Path) -> bool {
        self.items.iter().any(|item| {
            item.thread == approval.thread_id
                && item.turn == approval.turn_id
                && item.id == approval.item_id
                && item.kind == approval.kind
                && item.complete
                && !item.truncated
                && match item.kind {
                    ApprovalKind::Command => !item.command.trim().is_empty(),
                    ApprovalKind::FileChange => {
                        !item.diff.is_empty()
                            && !item.paths.is_empty()
                            && item.paths.iter().all(|path| safe_path(path, project))
                    }
                }
        })
    }
    pub fn offer(&mut self, approval: Approval, project: &Path) -> bool {
        if !self.matches_evidence(&approval, project)
            || self.pending.len() >= MAX_PENDING
            || self.pending.iter().any(|p| p.id == approval.id)
        {
            return false;
        }
        self.pending.push(approval);
        true
    }
    pub fn take(&mut self, approval: &Approval) -> bool {
        let Some(index) = self.pending.iter().position(|p| p == approval) else {
            return false;
        };
        self.pending.remove(index);
        true
    }
    /// Compare the immutable RPC approval descriptor to the exact tool item
    /// actually rendered. Evidence alone (even same itemId) is not authority.
    fn request_matches_review(
        approval: &Approval,
        project: &Path,
        reviewed: &ToolEvidence,
    ) -> bool {
        let req = &approval.request;
        if !req.is_object()
            || [
                "additionalPermissions",
                "networkApprovalContext",
                "proposedExecpolicyAmendment",
                "proposedNetworkPolicyAmendments",
            ]
            .iter()
            .any(|key| req.get(*key).is_some_and(|v| !v.is_null()))
        {
            // No implicit privilege expansion or session-wide scope.
            return false;
        }
        if let Some(decisions) = req.get("availableDecisions") {
            if !decisions
                .as_array()
                .is_some_and(|v| v.iter().any(|d| d.as_str() == Some("accept")))
            {
                return false;
            }
        }
        let Ok(expected_root) = project.canonicalize() else {
            return false;
        };
        match approval.kind {
            ApprovalKind::Command => {
                let Some(command) = req.get("command").and_then(Value::as_str) else {
                    return false;
                };
                let Some(cwd) = req.get("cwd").and_then(Value::as_str) else {
                    return false;
                };
                // The request and item cwd must BOTH resolve to the scoped
                // project; a same-item different command/cwd cannot be granted.
                command == reviewed.command
                    && !command.trim().is_empty()
                    && Path::new(cwd).canonicalize().ok().as_ref() == Some(&expected_root)
                    && Path::new(&reviewed.cwd).canonicalize().ok().as_ref() == Some(&expected_root)
                    && cwd == reviewed.cwd
            }
            ApprovalKind::FileChange => {
                // grantRoot is optional on official fileChange approval;
                // explicitly deny any broader grant outside the project.
                req.get("grantRoot").is_none_or(|value| {
                    value.as_str().is_some_and(|path| {
                        Path::new(path).canonicalize().ok().as_ref() == Some(&expected_root)
                    })
                })
            }
        }
    }

    pub fn consume_reviewed(
        &mut self,
        approval: &Approval,
        project: &Path,
        reviewed: &ToolEvidence,
    ) -> bool {
        let same_snapshot = self.items.iter().any(|item| {
            item.thread == approval.thread_id
                && item.turn == approval.turn_id
                && item.id == approval.item_id
                && item == reviewed
        });
        if !same_snapshot
            || !self.matches_evidence(approval, project)
            || !Self::request_matches_review(approval, project, reviewed)
        {
            return false;
        }
        self.take(approval)
    }

    #[cfg(test)]
    pub fn consume(&mut self, approval: &Approval, project: &Path) -> bool {
        if !self.matches_evidence(approval, project) {
            return false;
        }
        self.take(approval)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn project() -> std::path::PathBuf {
        std::env::current_dir().unwrap()
    }
    fn command(typ: &str, value: &str) -> Value {
        json!({"threadId":"t","turnId":"u","item":{"id":"i","type":typ,"command":value}})
    }
    fn approval() -> Approval {
        Approval {
            id: RpcId::String("r".into()),
            kind: ApprovalKind::Command,
            thread_id: "t".into(),
            turn_id: "u".into(),
            item_id: "i".into(),
            started_at_ms: 1,
            request: json!({}),
        }
    }
    #[test]
    fn adversarial_same_item_different_command_and_cwd_is_denied() {
        let mut ledger = ToolLedger::default();
        let cwd = project().to_str().unwrap().to_owned();
        let item = json!({"threadId":"t","turnId":"u","item":{"id":"i",
            "type":"commandExecution","command":"echo safe","cwd":cwd}});
        let snapshot = ledger
            .capture(NotificationKind::ItemStarted, &item, &project())
            .unwrap();
        let mut a = approval();
        a.request = json!({"threadId":"t","turnId":"u","itemId":"i",
            "command":"echo DIFFERENT","cwd":cwd,"startedAtMs":1});
        assert!(ledger.offer(a.clone(), &project()));
        assert!(!ledger.consume_reviewed(&a, &project(), &snapshot));
        assert_eq!(ledger.pending().len(), 1);
        a.request["command"] = Value::String("echo safe".into());
        a.request["cwd"] = Value::String(project().parent().unwrap().to_str().unwrap().to_owned());
        ledger.clear_pending();
        assert!(ledger.offer(a.clone(), &project()));
        assert!(!ledger.consume_reviewed(&a, &project(), &snapshot));
        a.request["cwd"] = Value::String(cwd.clone());
        a.request["additionalPermissions"] = json!({"network":{"enabled":true}});
        ledger.clear_pending();
        assert!(ledger.offer(a.clone(), &project()));
        assert!(!ledger.consume_reviewed(&a, &project(), &snapshot));
        a.request
            .as_object_mut()
            .unwrap()
            .remove("additionalPermissions");
        ledger.clear_pending();
        assert!(ledger.offer(a.clone(), &project()));
        assert!(ledger.consume_reviewed(&a, &project(), &snapshot));
        assert!(ledger.pending().is_empty());
    }

    #[test]
    fn exact_request_one_shot_and_cancel_clear() {
        let mut ledger = ToolLedger::default();
        ledger.capture(
            NotificationKind::ItemStarted,
            &command("commandExecution", "echo hello"),
            &project(),
        );
        assert!(ledger.offer(approval(), &project()));
        assert!(!ledger.offer(approval(), &project()));
        assert!(ledger.consume(&approval(), &project()));
        assert!(!ledger.consume(&approval(), &project()));
        assert!(ledger.offer(approval(), &project()));
        ledger.clear_pending();
        assert!(!ledger.consume(&approval(), &project()));
    }
    #[test]
    fn wrong_identity_and_missing_preview_denied() {
        let mut ledger = ToolLedger::default();
        assert!(!ledger.offer(approval(), &project()));
        ledger.capture(
            NotificationKind::ItemStarted,
            &command("commandExecution", "echo ok"),
            &project(),
        );
        for mut v in [approval(), approval(), approval()] {
            if v.id == RpcId::String("r".into()) {
                v.thread_id = "foreign".into();
            }
            assert!(!ledger.offer(v, &project()));
        }
        let mut v = approval();
        v.item_id = "other".into();
        assert!(!ledger.offer(v, &project()));
    }
    #[test]
    fn unsafe_paths_and_partial_diffs_are_unapprovable() {
        for path in [
            "../escape",
            "/abs",
            "C:/Windows/x",
            "folder\\evil",
            "a/./b",
            "a//b",
            "a%2fb",
        ] {
            assert!(!safe_path(path, &project()), "{path}");
        }
        let mut ledger = ToolLedger::default();
        let item = json!({"threadId":"t","turnId":"u","item":{"id":"f","type":"fileChange",
            "changes":[{"path":"src/lib.rs","diff":"@@ -1 +1 @@\n-a\n+b"}],"partial":true}});
        let snapshot = ledger
            .capture(NotificationKind::ItemStarted, &item, &project())
            .unwrap();
        assert!(!snapshot.complete);
        let mut a = approval();
        a.kind = ApprovalKind::FileChange;
        a.item_id = "f".into();
        assert!(!ledger.offer(a, &project()));
    }
    #[test]
    fn bounded_output_and_truncated_command_disallow_accept() {
        let mut ledger = ToolLedger::default();
        let oversized = command("commandExecution", &"a".repeat(MAX_COMMAND + 1));
        ledger.capture(NotificationKind::ItemStarted, &oversized, &project());
        assert!(!ledger.offer(approval(), &project()));
        ledger.clear();
        ledger.capture(
            NotificationKind::ItemStarted,
            &command("commandExecution", "echo ok"),
            &project(),
        );
        let delta =
            json!({"threadId":"t","turnId":"u","itemId":"i","delta":"a".repeat(MAX_OUTPUT+1)});
        let value = ledger
            .capture(NotificationKind::CommandOutputDelta, &delta, &project())
            .unwrap();
        assert!(value.truncated && value.output.len() <= MAX_OUTPUT);
        assert!(!ledger.offer(approval(), &project()));
    }
    #[test]
    fn broker_resolution_removes_stale_request() {
        let mut ledger = ToolLedger::default();
        ledger.capture(
            NotificationKind::ItemStarted,
            &command("commandExecution", "echo ok"),
            &project(),
        );
        assert!(ledger.offer(approval(), &project()));
        assert_eq!(
            ledger.resolved(&json!({"requestId":"r"})),
            Some(RpcId::String("r".into()))
        );
        assert!(ledger.pending().is_empty());
        assert!(!ledger.consume(&approval(), &project()));
    }
    #[test]
    fn deny_requires_exact_pending_identity_but_not_valid_preview() {
        let mut ledger = ToolLedger::default();
        ledger.capture(
            NotificationKind::ItemStarted,
            &command("commandExecution", "echo ok"),
            &project(),
        );
        let expected = approval();
        assert!(ledger.offer(expected.clone(), &project()));
        ledger.capture(
            NotificationKind::ItemCompleted,
            &command("commandExecution", &"x".repeat(MAX_COMMAND + 1)),
            &project(),
        );
        assert!(!ledger.consume(&expected, &project()));
        let mut wrong = expected.clone();
        wrong.turn_id = "foreign".into();
        assert!(!ledger.take(&wrong));
        assert!(ledger.take(&expected));
        assert!(!ledger.take(&expected));
    }
}
