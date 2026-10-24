//! NC-02: bounded, non-interactive process-tree baseline sampler.
//! All observations are local. No command output, prompts, or environment are recorded.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{Pid, ProcessesToUpdate, Signal, System};

const MAX_EVENTS: usize = 10_000;
const MAX_LINE_BYTES: usize = 1024 * 1024;
const MAX_TOTAL_SAMPLES: u64 = 240_000;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    schema_version: u32,
    scenario: String,
    duration_ms: u64,
    sample_interval_ms: u64,
    repetitions: u32,
    variants: Vec<Variant>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Variant {
    id: String,
    argv: Vec<String>,
    cwd: String,
    #[serde(default)]
    allow_early_exit: bool,
}

#[derive(Clone, Debug, Serialize)]
struct Sample {
    elapsed_ms: u64,
    process_count: usize,
    tree_rss_bytes: u64,
    tree_cpu_percent_one_core: f64,
}

#[derive(Debug, Default)]
struct StreamStats {
    first_byte_ms: Option<u64>,
    line_timestamps_ms: Vec<u64>,
    complete_lines: u64,
    truncated_lines: u64,
    total_bytes: u64,
}

#[derive(Debug, Serialize)]
struct Run {
    variant: String,
    repetition: u32,
    command_fingerprint_sha256: String,
    executable: String,
    cwd: String,
    start_epoch_ms: u128,
    spawn_latency_ms: u64,
    duration_observed_ms: u64,
    samples: Vec<Sample>,
    stdout_first_byte_ms: Option<u64>,
    stdout_first_line_ms: Option<u64>,
    stdout_complete_lines: u64,
    stdout_truncated_lines: u64,
    stdout_bytes: u64,
    stdout_line_timestamps_ms: Vec<u64>,
    stdout_interline_p95_ms: Option<f64>,
    stderr_bytes: u64,
    exit_code: Option<i32>,
    outcome: String,
    valid: bool,
}

#[derive(Debug, Serialize)]
struct Summary {
    variant: String,
    successful_repetitions: usize,
    total_repetitions: usize,
    rss_peak_bytes_median: Option<f64>,
    rss_peak_bytes_p95: Option<f64>,
    cpu_mean_percent_one_core_median: Option<f64>,
    cpu_mean_percent_one_core_p95: Option<f64>,
    first_stdout_line_ms_median: Option<f64>,
    first_stdout_line_ms_p95: Option<f64>,
}

#[derive(Serialize)]
struct Report<'a> {
    schema_version: u32,
    status: &'static str,
    scenario: &'a str,
    timestamp_epoch_ms: u128,
    platform_os: &'static str,
    platform_arch: &'static str,
    logical_cpus: usize,
    duration_ms: u64,
    sample_interval_ms: u64,
    repetitions: u32,
    measurement_notes: Vec<&'static str>,
    runs: Vec<Run>,
    summary: Vec<Summary>,
    native_windows_gate: &'static str,
}

fn millis(t: Instant, start: Instant) -> u64 {
    t.duration_since(start).as_millis() as u64
}

fn epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn percentile(values: &[f64], percentile: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut ordered = values.to_vec();
    ordered.sort_by(f64::total_cmp);
    let rank = (ordered.len() as f64 - 1.0) * percentile;
    let low = rank.floor() as usize;
    let high = rank.ceil() as usize;
    Some(ordered[low] + (ordered[high] - ordered[low]) * (rank - low as f64))
}

fn summarize(variant: &str, runs: &[Run]) -> Summary {
    let selected: Vec<&Run> = runs
        .iter()
        .filter(|r| r.variant == variant && r.valid)
        .collect();
    let peaks: Vec<f64> = selected
        .iter()
        .filter_map(|r| {
            r.samples
                .iter()
                .map(|s| s.tree_rss_bytes)
                .max()
                .map(|v| v as f64)
        })
        .collect();
    let cpus: Vec<f64> = selected
        .iter()
        .filter_map(|r| {
            if r.samples.len() < 2 {
                None
            } else {
                Some(
                    r.samples[1..]
                        .iter()
                        .map(|s| s.tree_cpu_percent_one_core)
                        .sum::<f64>()
                        / (r.samples.len() - 1) as f64,
                )
            }
        })
        .collect();
    let first: Vec<f64> = selected
        .iter()
        .filter_map(|r| r.stdout_first_line_ms.map(|v| v as f64))
        .collect();
    Summary {
        variant: variant.into(),
        successful_repetitions: selected.len(),
        total_repetitions: runs.iter().filter(|r| r.variant == variant).count(),
        rss_peak_bytes_median: percentile(&peaks, 0.5),
        rss_peak_bytes_p95: percentile(&peaks, 0.95),
        cpu_mean_percent_one_core_median: percentile(&cpus, 0.5),
        cpu_mean_percent_one_core_p95: percentile(&cpus, 0.95),
        first_stdout_line_ms_median: percentile(&first, 0.5),
        first_stdout_line_ms_p95: percentile(&first, 0.95),
    }
}

fn validate(plan: &Plan) -> Result<(), String> {
    if plan.schema_version != 1 {
        return Err("unsupported schema_version".into());
    }
    if plan.scenario.trim().is_empty() || plan.scenario.len() > 100 {
        return Err("invalid scenario".into());
    }
    if !(200..=3_600_000).contains(&plan.duration_ms) {
        return Err("duration_ms must be 200..=3600000".into());
    }
    if !(100..=5000).contains(&plan.sample_interval_ms)
        || plan.sample_interval_ms > plan.duration_ms / 2
    {
        return Err("sample_interval_ms must be 100..=5000 and allow at least two samples".into());
    }
    if !(3..=20).contains(&plan.repetitions) {
        return Err("repetitions must be 3..=20".into());
    }
    if plan.variants.is_empty() || plan.variants.len() > 8 {
        return Err("variants must be 1..=8".into());
    }
    // Worst-case report size is determined before launching any command.
    // A 60-minute endurance test is still allowed at a suitable sample rate.
    let samples_per_run = plan.duration_ms.div_ceil(plan.sample_interval_ms);
    let total_samples = samples_per_run * u64::from(plan.repetitions) * plan.variants.len() as u64;
    if total_samples > MAX_TOTAL_SAMPLES {
        return Err(format!(
            "plan may collect {total_samples} samples; maximum is {MAX_TOTAL_SAMPLES}. Increase sample_interval_ms or reduce repetitions/variants"
        ));
    }
    let mut ids = HashSet::new();
    for variant in &plan.variants {
        if variant.id.trim().is_empty() || !ids.insert(&variant.id) || variant.id.len() > 64 {
            return Err("variant ids must be unique, nonempty and <=64 chars".into());
        }
        if variant.argv.is_empty() || variant.argv[0].trim().is_empty() || variant.argv.len() > 100
        {
            return Err(format!("invalid argv for {}", variant.id));
        }
        if variant.cwd.trim().is_empty() {
            return Err(format!("invalid cwd for {}", variant.id));
        }
    }
    Ok(())
}

// A process belongs to the benchmark only when its ancestry reaches the launched root.
// This prevents adding unrelated OS processes to measured total.
fn descendants(root: Pid, processes: &HashMap<Pid, sysinfo::Process>) -> Vec<Pid> {
    let mut owned: HashSet<Pid> = HashSet::from([root]);
    loop {
        let before = owned.len();
        for (&pid, p) in processes {
            if p.parent().is_some_and(|parent| owned.contains(&parent)) {
                owned.insert(pid);
            }
        }
        if owned.len() == before {
            break;
        }
    }
    owned
        .into_iter()
        .filter(|p| processes.contains_key(p))
        .collect()
}

fn read_stream<R: Read + Send + 'static>(
    mut stream: R,
    begin: Instant,
    count_lines: bool,
) -> Receiver<StreamStats> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut stats = StreamStats::default();
        let mut buf = [0u8; 8192];
        let mut pending = 0usize;
        loop {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let elapsed = millis(Instant::now(), begin);
                    if stats.first_byte_ms.is_none() {
                        stats.first_byte_ms = Some(elapsed);
                    }
                    stats.total_bytes = stats.total_bytes.saturating_add(n as u64);
                    if !count_lines {
                        continue;
                    }
                    for byte in &buf[..n] {
                        if *byte == b'\n' {
                            stats.complete_lines += 1;
                            if stats.line_timestamps_ms.len() < MAX_EVENTS {
                                stats.line_timestamps_ms.push(elapsed);
                            }
                            pending = 0;
                        } else if pending < MAX_LINE_BYTES {
                            pending += 1;
                        } else if pending == MAX_LINE_BYTES {
                            stats.truncated_lines += 1;
                            pending += 1; // saturating marker until newline
                        }
                    }
                }
                Err(_) => break,
            }
        }
        let _ = sender.send(stats);
    });
    receiver
}

fn stop_tree(child: &mut Child, system: &mut System) {
    system.refresh_processes(ProcessesToUpdate::All, true);
    let root = Pid::from_u32(child.id());
    let own = descendants(root, system.processes());
    for pid in &own {
        if *pid != root {
            if let Some(p) = system.process(*pid) {
                if p.kill_with(Signal::Term).is_none() {
                    p.kill();
                }
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn execute(
    variant: &Variant,
    plan: &Plan,
    plan_parent: &Path,
    repetition: u32,
) -> Result<Run, String> {
    let cwd = plan_parent
        .join(&variant.cwd)
        .canonicalize()
        .map_err(|e| format!("invalid cwd for {}: {e}", variant.id))?;
    if !cwd.is_dir() {
        return Err("cwd must be a directory".into());
    }
    let argv: Vec<String> = variant
        .argv
        .iter()
        .map(|v| {
            if v == "<SELF>" {
                std::env::current_exe()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            } else {
                v.clone()
            }
        })
        .collect();
    let mut hash = Sha256::new();
    for item in &argv {
        hash.update((item.len() as u64).to_le_bytes());
        hash.update(item.as_bytes());
    }
    let fingerprint = format!("{:x}", hash.finalize());
    let begin = Instant::now();
    let started_at = epoch_ms();
    // Keep the pipe open while sampling; the official App Server may exit on stdin EOF.
    // No requests, credentials, or approvals are sent by the generic S0 sampler.
    let mut child = Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot launch variant {}: {e}", variant.id))?;
    let spawn_ms = millis(Instant::now(), begin);
    let stdout_reader = read_stream(child.stdout.take().unwrap(), begin, true);
    let stderr_reader = read_stream(child.stderr.take().unwrap(), begin, false);
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::All, true);
    let root = Pid::from_u32(child.id());
    let mut samples = Vec::new();
    let duration = Duration::from_millis(plan.duration_ms);
    let sample_interval = Duration::from_millis(plan.sample_interval_ms);
    let mut next = begin;
    let mut outcome = "completed".to_string();
    let mut exit_code = None;
    loop {
        let now = Instant::now();
        if now >= begin + duration {
            break;
        }
        if let Some(exit) = child.try_wait().map_err(|e| e.to_string())? {
            exit_code = exit.code();
            outcome = "early_exit".into();
            break;
        }
        system.refresh_processes(ProcessesToUpdate::All, true);
        let pids = descendants(root, system.processes());
        let mut rss = 0u64;
        let mut cpu = 0.0f64;
        for pid in &pids {
            if let Some(p) = system.process(*pid) {
                rss = rss.saturating_add(p.memory());
                cpu += f64::from(p.cpu_usage());
            }
        }
        samples.push(Sample {
            elapsed_ms: millis(Instant::now(), begin),
            process_count: pids.len(),
            tree_rss_bytes: rss,
            tree_cpu_percent_one_core: cpu,
        });
        next += sample_interval;
        if let Some(remaining) = next.checked_duration_since(Instant::now()) {
            thread::sleep(remaining);
        } else {
            next = Instant::now();
        }
    }
    let observed = millis(Instant::now(), begin);
    if outcome == "completed" {
        stop_tree(&mut child, &mut system);
    } else {
        let _ = child.wait();
    }
    let out = stdout_reader
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| "stdout collector did not finish within 5 seconds")?;
    let err = stderr_reader
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| "stderr collector did not finish within 5 seconds")?;
    let intervals: Vec<f64> = out
        .line_timestamps_ms
        .windows(2)
        .map(|pair| (pair[1] - pair[0]) as f64)
        .collect();
    let valid = (outcome == "completed" || (variant.allow_early_exit && exit_code == Some(0)))
        && samples.len() >= 2
        && samples.iter().any(|s| s.process_count > 0);
    Ok(Run {
        variant: variant.id.clone(),
        repetition,
        command_fingerprint_sha256: fingerprint,
        executable: Path::new(&argv[0])
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        cwd: cwd.to_string_lossy().into_owned(),
        start_epoch_ms: started_at,
        spawn_latency_ms: spawn_ms,
        duration_observed_ms: observed,
        samples,
        stdout_first_byte_ms: out.first_byte_ms,
        stdout_first_line_ms: out.line_timestamps_ms.first().copied(),
        stdout_complete_lines: out.complete_lines,
        stdout_truncated_lines: out.truncated_lines,
        stdout_bytes: out.total_bytes,
        stdout_line_timestamps_ms: out.line_timestamps_ms,
        stdout_interline_p95_ms: percentile(&intervals, 0.95),
        stderr_bytes: err.total_bytes,
        exit_code,
        outcome,
        valid,
    })
}

fn run(plan_path: &Path, output_path: &Path) -> Result<bool, String> {
    let raw = fs::read_to_string(plan_path).map_err(|e| e.to_string())?;
    let plan: Plan = serde_json::from_str(&raw).map_err(|e| format!("invalid plan JSON: {e}"))?;
    validate(&plan)?;
    let parent = plan_path
        .canonicalize()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("plan has no parent")?
        .to_path_buf();
    let mut runs = Vec::new();
    for repetition in 1..=plan.repetitions {
        for variant in &plan.variants {
            eprintln!(
                "NC-02 {} repetition {}/{}",
                variant.id, repetition, plan.repetitions
            );
            runs.push(execute(variant, &plan, &parent, repetition)?);
        }
    }
    let ok = runs.iter().all(|r| r.valid);
    let summary = plan
        .variants
        .iter()
        .map(|v| summarize(&v.id, &runs))
        .collect();
    let report = Report {
        schema_version: 1,
        status: if ok { "MEASURED" } else { "INCOMPLETE" },
        scenario: &plan.scenario,
        timestamp_epoch_ms: epoch_ms(),
        platform_os: std::env::consts::OS,
        platform_arch: std::env::consts::ARCH,
        logical_cpus: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
        duration_ms: plan.duration_ms,
        sample_interval_ms: plan.sample_interval_ms,
        repetitions: plan.repetitions,
        measurement_notes: vec![
            "RSS is sum of per-process resident memory and can double-count shared pages.",
            "CPU percentage uses 100% per fully occupied logical core, summed across the owned tree.",
            "Stdout timestamps are pipe line-observation proxies, not UI event-to-paint or input latency.",
            "CLOUD CI synthetic smoke does not satisfy native Windows product qualification.",
            "Spawned descendants are sampled by parent PID and terminated on normal timeout; detached or reparented children may escape this harness."
        ],
        runs,
        summary,
        native_windows_gate: "NOT_RUN",
    };
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(
        output_path,
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(ok)
}

fn fixture(which: &str) {
    match which {
        "idle" => thread::sleep(Duration::from_secs(5)),
        "stream" => {
            for n in 0..10 {
                println!("synthetic-event-{n}");
                thread::sleep(Duration::from_millis(70));
            }
            thread::sleep(Duration::from_secs(5));
        }
        "short" => println!("synthetic-exit"),
        _ => std::process::exit(2),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "fixture" {
        fixture(&args[2]);
        return;
    }
    if args.len() != 5 || args[1] != "--plan" || args[3] != "--out" {
        eprintln!("Usage: ultima-nc02 --plan <plan.json> --out <report.json>");
        std::process::exit(2);
    }
    match run(Path::new(&args[2]), Path::new(&args[4])) {
        Ok(true) => println!("NC-02 report written: {}", &args[4]),
        Ok(false) => {
            eprintln!("NC-02 incomplete: at least one run invalid");
            std::process::exit(1)
        }
        Err(err) => {
            eprintln!("NC-02 error: {err}");
            std::process::exit(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn percentile_interpolates_and_handles_missing() {
        assert_eq!(percentile(&[], 0.5), None);
        assert_eq!(percentile(&[10.0, 30.0], 0.5), Some(20.0));
        assert_eq!(percentile(&[10.0, 30.0], 0.95), Some(29.0));
    }
    #[test]
    fn streamed_lines_are_counted_without_saving_payload() {
        let input = Cursor::new(b"private-content\nprivate-content\n".to_vec());
        let stats = read_stream(input, Instant::now(), true)
            .recv_timeout(Duration::from_secs(1))
            .expect("collector completes");
        assert_eq!(stats.complete_lines, 2);
        assert_eq!(stats.line_timestamps_ms.len(), 2);
        assert_eq!(stats.total_bytes, 32);
        assert_eq!(stats.truncated_lines, 0);
    }
    #[test]
    fn rejects_unbounded_and_ambiguous_plans() {
        let mut plan = Plan {
            schema_version: 1,
            scenario: "S0".into(),
            duration_ms: 1000,
            sample_interval_ms: 100,
            repetitions: 3,
            variants: vec![Variant {
                id: "cli".into(),
                argv: vec!["codex".into()],
                cwd: ".".into(),
                allow_early_exit: false,
            }],
        };
        assert!(validate(&plan).is_ok());
        plan.repetitions = 1;
        assert!(validate(&plan).is_err());
        plan.repetitions = 3;
        plan.variants.push(Variant {
            id: "cli".into(),
            argv: vec!["x".into()],
            cwd: ".".into(),
            allow_early_exit: false,
        });
        assert!(validate(&plan).is_err());
    }
    #[test]
    fn bounds_total_telemetry_samples_before_launch() {
        let mut plan = Plan {
            schema_version: 1,
            scenario: "S4".into(),
            duration_ms: 3_600_000,
            sample_interval_ms: 100,
            repetitions: 20,
            variants: vec![Variant {
                id: "cli".into(),
                argv: vec!["codex".into()],
                cwd: ".".into(),
                allow_early_exit: false,
            }],
        };
        assert!(validate(&plan).unwrap_err().contains("maximum is"));
        plan.sample_interval_ms = 500;
        assert!(validate(&plan).is_ok());
    }
}
