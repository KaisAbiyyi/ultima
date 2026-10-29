//! Bounded, read-only projections of official App Server history. This is not a
//! parallel session store: after a process restart the same RPCs rehydrate it.
use serde_json::Value;
use std::path::Path;

pub const THREAD_PAGE_SIZE: u64 = 24;
pub const TURN_PAGE_SIZE: u64 = 12;
pub const MAX_THREADS: usize = 120;
pub const MAX_HISTORY_ROWS: usize = 512;
const MAX_ROW_BYTES: usize = 32 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadEntry {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRow {
    pub from_user: bool,
    pub preview: String,
}

fn bounded_id(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 256)
}

pub fn safe_cursor(value: Option<&Value>) -> Result<Option<String>, &'static str> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if !s.is_empty() && s.len() <= 2048 => Ok(Some(s.clone())),
        _ => Err("Codex returned an invalid history cursor."),
    }
}

fn trusted_cwd(cwd: Option<&str>, project: &Path) -> bool {
    let Some(cwd) = cwd else {
        return false;
    };
    project.canonicalize().ok() == Path::new(cwd).canonicalize().ok()
        && project.canonicalize().is_ok()
}

pub fn parse_thread_page(
    value: &Value,
    project: &Path,
) -> Result<(Vec<ThreadEntry>, Option<String>), &'static str> {
    let data = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or("Codex thread listing was invalid.")?;
    if data.len() > THREAD_PAGE_SIZE as usize {
        return Err("Codex thread listing exceeded the page limit.");
    }
    let mut entries = Vec::new();
    for thread in data {
        // Do not present an untrusted directory as an available project thread,
        // even if an incorrectly filtered thread/list response includes it.
        if !trusted_cwd(thread.get("cwd").and_then(Value::as_str), project) {
            continue;
        }
        let Some(id) = bounded_id(thread.get("id")) else {
            return Err("Codex listed a malformed thread ID.");
        };
        if thread.get("ephemeral").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let title = thread
            .get("name")
            .and_then(Value::as_str)
            .or_else(|| thread.get("preview").and_then(Value::as_str))
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("Untitled thread");
        let label = truncate_utf8(title, 96);
        if !entries.iter().any(|e: &ThreadEntry| e.id == id) {
            entries.push(ThreadEntry {
                id: id.to_owned(),
                label,
            });
        }
    }
    Ok((entries, safe_cursor(value.get("nextCursor"))?))
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    let mut end = value.len().min(max_bytes);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

pub fn verify_thread_identity(
    value: &Value,
    expected: &str,
    project: &Path,
) -> Result<(), &'static str> {
    let thread = value
        .get("thread")
        .ok_or("Codex thread metadata was missing.")?;
    if bounded_id(thread.get("id")) != Some(expected) {
        return Err("Codex thread identity changed unexpectedly.");
    }
    if !trusted_cwd(thread.get("cwd").and_then(Value::as_str), project) {
        return Err("Codex thread directory does not match the trusted project.");
    }
    Ok(())
}

fn user_item_text(item: &Value) -> Option<String> {
    let parts = item.get("content")?.as_array()?;
    let mut text = String::new();
    for part in parts {
        if part.get("type").and_then(Value::as_str) != Some("text") {
            continue;
        }
        if let Some(value) = part.get("text").and_then(Value::as_str) {
            if text.len().saturating_add(value.len()) > MAX_ROW_BYTES {
                return Some(truncate_utf8(&text, MAX_ROW_BYTES));
            }
            text.push_str(value);
        }
    }
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Resume adopts server-owned active work. It never replays a queued prompt.
pub fn active_turn_in_page(value: &Value) -> Result<Option<String>, &'static str> {
    let turns = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or("Codex active-turn history was malformed.")?;
    if turns.len() > TURN_PAGE_SIZE as usize {
        return Err("Codex active-turn page exceeded its bound.");
    }
    let mut active = None;
    for turn in turns {
        let status = turn
            .get("status")
            .and_then(Value::as_str)
            .ok_or("Codex turn status was missing.");
        let status = status?;
        if !matches!(
            status,
            "inProgress"
                | "in_progress"
                | "running"
                | "completed"
                | "interrupted"
                | "failed"
                | "cancelled"
        ) {
            return Err("Codex returned an unknown historical turn state.");
        }
        if matches!(status, "inProgress" | "in_progress" | "running") {
            let id = bounded_id(turn.get("id")).ok_or("Codex active-turn identity was invalid.")?;
            if active.is_some() {
                return Err("Codex reported multiple active turns.");
            }
            active = Some(id.to_owned());
        }
    }
    Ok(active)
}

pub fn parse_turn_page(value: &Value) -> Result<(Vec<HistoryRow>, Option<String>), &'static str> {
    let data = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or("Codex paginated turn history is unavailable.")?;
    if data.len() > TURN_PAGE_SIZE as usize {
        return Err("Codex history exceeded the turn page limit.");
    }
    let mut rows = Vec::new();
    // The official desc page is newest-first, while each turn's items are
    // chronological. Reverse turns, never reverse items inside a turn.
    for turn in data.iter().rev() {
        bounded_id(turn.get("id")).ok_or("Codex history had an invalid turn ID.")?;
        let items = turn
            .get("items")
            .and_then(Value::as_array)
            .ok_or("Codex did not return full paginated turn items.")?;
        if items.len() > 128 {
            return Err("Codex turn exceeded the supported item limit.");
        }
        for item in items {
            let parsed = match item.get("type").and_then(Value::as_str) {
                Some("userMessage") => user_item_text(item).map(|preview| HistoryRow {
                    from_user: true,
                    preview,
                }),
                Some("agentMessage") => {
                    item.get("text")
                        .and_then(Value::as_str)
                        .map(|value| HistoryRow {
                            from_user: false,
                            preview: truncate_utf8(value, MAX_ROW_BYTES),
                        })
                }
                _ => None, // Never render tool commands, secrets or arbitrary items.
            };
            if let Some(row) = parsed {
                if rows.len() >= MAX_HISTORY_ROWS {
                    return Err("Codex history exceeded the row limit.");
                }
                rows.push(row);
            }
        }
        // Persist terminal state even if no assistant item exists. Never
        // interpret interrupted/failed turns as requests to execute again.
        if let Some(status) = turn.get("status").and_then(Value::as_str) {
            if matches!(status, "interrupted" | "failed" | "cancelled") {
                rows.push(HistoryRow {
                    from_user: false,
                    preview: format!("Turn ended: {status}"),
                });
            }
        }
    }
    Ok((rows, safe_cursor(value.get("nextCursor"))?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_cross_project_and_oversized_thread_pages() {
        let cwd = std::env::current_dir().unwrap();
        let project = cwd.to_string_lossy();
        let page = json!({"data":[
            {"id":"mine","cwd":project,"preview":"Useful","ephemeral":false},
            {"id":"other","cwd":"/does-not-exist","preview":"Do not show"},
            {"id":"mine","cwd":project,"preview":"duplicate"}
        ],"nextCursor":"more"});
        let (items, cursor) = parse_thread_page(&page, &cwd).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(cursor.as_deref(), Some("more"));
        let massive = json!({"data":vec![json!({"id":"x","cwd":project});25]});
        assert!(parse_thread_page(&massive, &cwd).is_err());
    }

    #[test]
    fn strict_read_identity_and_cwd() {
        let cwd = std::env::current_dir().unwrap();
        let good = json!({"thread":{"id":"a","cwd":cwd.to_str().unwrap()}});
        assert!(verify_thread_identity(&good, "a", &cwd).is_ok());
        assert!(verify_thread_identity(&good, "b", &cwd).is_err());
        assert!(verify_thread_identity(
            &json!({"thread":{"id":"a","cwd":"/tmp/foreign"}}),
            "a",
            &cwd
        )
        .is_err());
    }

    #[test]
    fn paginated_history_preserves_order_and_terminal_status() {
        let page = json!({"data":[
            {"id":"new","status":"failed","items":[
                {"type":"userMessage","content":[{"type":"text","text":"new prompt"}]},
                {"type":"commandExecution","command":"NEVER DISPLAY"}
            ]},
            {"id":"old","status":"completed","items":[
                {"type":"userMessage","content":[{"type":"text","text":"old prompt"}]},
                {"type":"agentMessage","text":"old answer"}
            ]}
        ],"nextCursor":"older"});
        let (rows, cursor) = parse_turn_page(&page).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.preview.as_str()).collect::<Vec<_>>(),
            [
                "old prompt",
                "old answer",
                "new prompt",
                "Turn ended: failed"
            ]
        );
        assert_eq!(cursor.as_deref(), Some("older"));
    }

    #[test]
    fn missing_full_items_and_unsafe_cursor_are_explicit_failures() {
        assert!(parse_turn_page(&json!({"data":[{"id":"x"}]})).is_err());
        assert!(parse_turn_page(&json!({"data":[],"nextCursor":"x".repeat(2049)})).is_err());
    }

    #[test]
    fn active_turn_state_is_single_and_authoritative() {
        assert_eq!(
            active_turn_in_page(&json!({"data":[{"id":"t","status":"inProgress"}]})),
            Ok(Some("t".into()))
        );
        assert_eq!(
            active_turn_in_page(&json!({"data":[{"id":"t","status":"completed"}]})),
            Ok(None)
        );
        assert!(active_turn_in_page(&json!({"data":[
            {"id":"a","status":"inProgress"},{"id":"b","status":"inProgress"}
        ]}))
        .is_err());
        assert!(active_turn_in_page(&json!({"data":[{"id":"a"}]})).is_err());
        assert!(active_turn_in_page(&json!({"data":[{"id":"a","status":"unknown"}]})).is_err());
    }

    #[test]
    fn utf8_limits_are_safe() {
        let text = "é".repeat(20_000);
        let page = json!({"data":[{"id":"x","items":[{"type":"agentMessage","text":text}]}]});
        let (rows, _) = parse_turn_page(&page).unwrap();
        assert!(rows[0].preview.len() <= MAX_ROW_BYTES);
        assert!(rows[0].preview.is_char_boundary(rows[0].preview.len()));
    }
}
