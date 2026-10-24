//! NC-12 synthetic protocol fixture, NEVER production Codex.
use serde_json::{json, Value};
use std::{fs, io::{self, BufRead, Write}};
fn main() {
    let stdin = io::stdin();
    let mut out = io::stdout().lock();
    let cwd = std::env::current_dir().expect("cwd");
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break; };
        let Ok(data) = serde_json::from_str::<Value>(&line) else { break; };
        if data["id"] == 0 && data["method"] == "initialize" {
            if fs::metadata(cwd.join("probe-fail-init")).is_ok() {
                let _ = writeln!(out, "{}", json!({"id":0,"error":{"code":-1,"message":"error"}}));
            } else {
                let _ = writeln!(out, "{}", json!({"id":0,"result":{"userAgent":"fixture","platformFamily":"test","platformOs":"fixture"}}));
            }
            let _ = out.flush();
        } else if data["method"] == "model/list" {
            if fs::metadata(cwd.join("probe-fail-model")).is_ok() {
                let _ = writeln!(out, "{}", json!({"id":1,"error":{"code":-1,"message":"error"}}));
            } else {
                let _ = writeln!(out, "{}", json!({"id":1,"result":{"data":[],"nextCursor":null}}));
            }
            let _ = out.flush();
        }
    }
}
