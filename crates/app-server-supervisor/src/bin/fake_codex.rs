// Synthetic App Server only. No official Codex auth, tools or network.
use std::{
    io::{self, BufRead, Write},
    process,
};
fn main() {
    if std::env::args().nth(1).as_deref() == Some("fixture/child") {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    }
    if std::env::args().nth(1).as_deref() != Some("app-server") {
        process::exit(2);
    }
    let stdin = io::stdin();
    for input in stdin.lock().lines() {
        let Ok(input) = input else { break };
        eprintln!("FAKE_TEST_SECRET=never-print-in-app-log");
        if input.contains("fixture/spawnChild") {
            let mut child = process::Command::new(std::env::current_exe().unwrap())
                .arg("fixture/child")
                .spawn()
                .unwrap();
            println!("{{\"childPid\":{}}}", child.id());
            let _ = io::stdout().flush();
            std::thread::spawn(move || { let _ = child.wait(); });
            continue;
        }
        if input.contains("fixture/crash") {
            process::exit(12);
        }
        if input.contains("fixture/oversize") {
            println!("{}", "x".repeat(1_048_577));
            let _ = io::stdout().flush();
            continue;
        }
        println!("{input}");
        let _ = io::stdout().flush();
    }
}
