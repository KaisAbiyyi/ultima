/** Strict NC-10 evidence evaluation. No synthetic/native or proxy/paint promotion. */
import { readFile, writeFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';

export function statistics(values) {
  if (!Array.isArray(values) || values.length > 240000 || values.some(v => !Number.isFinite(v) || v < 0)) {
    throw Error('Invalid numeric observations');
  }
  if (!values.length) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const percentile = p => {
    const rank = (sorted.length - 1) * p;
    return sorted[Math.floor(rank)] + (sorted[Math.ceil(rank)] - sorted[Math.floor(rank)]) * (rank % 1);
  };
  const mean = values.reduce((a, b) => a + b, 0) / values.length;
  return { count: values.length, p50: percentile(0.5), p95: percentile(0.95),
    variance: values.length > 1 ? values.reduce((a, b) => a + (b - mean) ** 2, 0) / (values.length - 1) : 0 };
}
function validIdentity(id) {
  return id?.platform === 'win32' && id.physicalWindows === true && id.dirty === false && id.artifactUnchanged === true &&
    id.codexProvenance === true && id.build?.clean === true && id.build.sourceSha === id.sourceSha &&
    id.build.artifactSha256 === id.artifactSha256 &&
    ['reviewedBy', 'manufacturer', 'model'].every(key => typeof id.deviceReceipt?.[key] === 'string' && id.deviceReceipt[key].trim()) &&
    /^[a-f0-9]{64}$/.test(id.deviceReceipt?.evidenceSha256 ?? '') &&
    /^[a-f0-9]{40}$/.test(id.sourceSha ?? '') &&
    ['artifactSha256', 'codexSha256', 'fixtureSha256'].every(key => /^[a-f0-9]{64}$/.test(id[key] ?? '')) &&
    id.workload === 'official-codex' &&
    ['codexVersion', 'hardware', 'powerPlan', 'graphics', 'authState', 'cacheState'].every(key => typeof id[key] === 'string' && id[key].trim());
}
function samplesValid(samples, durationMs, maxGap = 5000) {
  if (!Number.isFinite(durationMs) || durationMs <= 0 || durationMs > 3900000) return false;
  if (!Array.isArray(samples) || samples.length < 2 || samples.length > 240000) return false;
  if (samples.some(s => !['elapsedMs', 'rssBytes', 'cpuPercent', 'processCount'].every(k => Number.isFinite(s[k]) && s[k] >= 0) || s.processCount < 1)) return false;
  if (samples[0].elapsedMs > maxGap || samples.at(-1).elapsedMs < durationMs - maxGap) return false;
  return samples.every((s, i) => i === 0 || (s.elapsedMs > samples[i - 1].elapsedMs && s.elapsedMs - samples[i - 1].elapsedMs <= maxGap));
}
function enginePresent(sample) {
  // A connected desktop with no attributable official engine is not the
  // same workload as its headless CLI pair. The runner must refuse this data,
  // even if WebView2 and the parent desktop process remain alive.
  return Array.isArray(sample.tree) && sample.tree.some(p =>
    p && typeof p.name === 'string' && p.name.toLowerCase() === 'codex' &&
    Number.isSafeInteger(p.pid) && p.pid > 0);
}
function idleValues(runs) {
  // S0 settles for 60 s, then observes at least another 60 s. Repetitions are
  // kept independent rather than treating adjacent ticks as independent trials.
  if (!Array.isArray(runs) || runs.length < 3 || runs.length > 20) return null;
  return runs.map(run => {
    if (!run.valid || run.durationMs < 120000 || !samplesValid(run.samples, run.durationMs)) return null;
    const settled = run.samples.filter(s => s.elapsedMs >= 60000 && s.elapsedMs <= run.durationMs);
    if (settled.length < 2 || settled[0].elapsedMs > 65000 || settled.at(-1).elapsedMs < run.durationMs - 5000 ||
        settled.some(sample => !enginePresent(sample)) ||
        settled.some(s => !['elapsedMs', 'rssBytes', 'cpuPercent', 'processCount'].every(k => Number.isFinite(s[k]) && s[k] >= 0) || s.processCount < 1)) return null;
    return { ram: statistics(settled.map(s => s.rssBytes)), cpu: settled.reduce((n, s) => n + s.cpuPercent, 0) / settled.length };
  });
}
export function evaluateQualification(report) {
  const eligible = validIdentity(report?.identity);
  const gates = [];
  const gate = (id, status, reason, metrics = null) => gates.push({ id, status: eligible ? status : 'NOT_RUN',
    reason: eligible ? reason : 'Missing clean exact-SHA physical Windows / official-engine artifact identity', metrics });
  const idle = report?.idle;
  const cli = idleValues(idle?.cli);
  const desktop = idleValues(idle?.desktop);
  if (!idle?.matched || !cli || !desktop) {
    gate('idle-ram', 'NOT_RUN', 'Need at least three matched official CLI and connected desktop runs');
    gate('idle-cpu', 'NOT_RUN', 'Need 60 s settlement and a 60 s idle measurement per repetition');
  } else if ([...cli, ...desktop].some(v => !v)) {
    gate('idle-ram', 'FAIL', 'Invalid/early-exit, missing official Codex process, or incomplete settled idle run');
    gate('idle-cpu', 'FAIL', 'Invalid/early-exit, missing official Codex process, or incomplete settled idle run');
  } else {
    const desktopRam = statistics(desktop.map(v => v.ram.p95));
    const cliRam = statistics(cli.map(v => v.ram.p95));
    const overhead = desktopRam.p95 - cliRam.p95;
    gate('idle-ram', overhead <= 150 * 1024 ** 2 ? 'PASS' : 'FAIL', 'Desktop minus same-engine CLI p95 RSS; shared pages can be double-counted',
      { desktopRam, cliRam, incrementalBytes: overhead, budgetBytes: 150 * 1024 ** 2 });
    const cpu = statistics(desktop.map(v => v.cpu));
    gate('idle-cpu', cpu.p95 <= 1 ? 'PASS' : 'FAIL', 'Settled tree average CPU per run, one-core normalized', cpu);
  }
  for (const [id, values, budget, minimum, boundary] of [
    ['cold-start', report?.coldStart?.values, 1500, 3, report?.coldStart?.boundary === 'process-launch-to-responsive-composer' && report.coldStart.cache === 'cold'],
    ['input', report?.latency?.input, 50, 30, report?.latency?.boundary === 'event-to-presented-frame'],
    ['stream', report?.latency?.stream, 50, 30, report?.latency?.boundary === 'event-to-presented-frame']
  ]) {
    let summary = null;
    try { summary = statistics(values ?? []); } catch { /* invalid observations fail closed */ }
    gate(id, boundary && summary?.count >= minimum ? (summary.p95 <= budget ? 'PASS' : 'FAIL') : 'NOT_RUN',
      `Need ${minimum}+ observations at the actual required boundary; frame scheduling/stdout proxies are diagnostic only`, summary);
  }
  const history = report?.history;
  const historyMeasured = history?.sourceMessages >= 10000 && history?.renderedRows > 0 && history.renderedRows <= 100 &&
    history?.noLockup === true && history?.boundary === 'event-to-presented-frame';
  let historyLatency = null;
  try { historyLatency = statistics(history?.latencies ?? []); } catch { /* fail closed */ }
  gate('history', historyMeasured && historyLatency?.count >= 30 ? (historyLatency.p95 <= 50 ? 'PASS' : 'FAIL') : 'NOT_RUN',
    'Need real 10k-history restore, bounded DOM, 30+ wheel/input presentation timings and no lockup', historyLatency);
  const stress = report?.stress;
  if (!stress || stress.durationMs < 3600000 || stress.turns < 2 || stress.projectSwitches < 3 ||
      stress.pauseResume !== true || stress.restartRecovery !== true || !samplesValid(stress.samples, stress.durationMs)) {
    gate('endurance', 'NOT_RUN', 'Need complete 60 min repeated turns, three project switches, pause/resume, restart/recovery and continuous tree samples');
  } else {
    // Least-squares trend after the first five minutes. A >20 MiB/h increase
    // requires investigation; it must never be silently labeled a leak PASS.
    const settled = stress.samples.filter(s => s.elapsedMs >= 300000);
    const xMean = settled.reduce((n, s) => n + s.elapsedMs, 0) / settled.length;
    const yMean = settled.reduce((n, s) => n + s.rssBytes, 0) / settled.length;
    const slope = settled.reduce((n, s) => n + (s.elapsedMs - xMean) * (s.rssBytes - yMean), 0) /
      settled.reduce((n, s) => n + (s.elapsedMs - xMean) ** 2, 0) * 3600000;
    gate('endurance', stress.valid === true && slope <= 20 * 1024 ** 2 ? 'PASS' : 'FAIL',
      'Trend above 20 MiB/hour or failed workload requires investigation', { rss: statistics(settled.map(s => s.rssBytes)), bytesPerHour: slope });
  }
  for (const id of ['approval-cancel', 'crash-resume', 'windows-dpi-ime-screen-reader', 'webview-lifecycle', 'packaged-candidate']) {
    // These are explicitly reviewed receipts, not arbitrary JSON booleans.
    gate(id, 'NOT_RUN', 'Separate native/owner receipt required; collector cannot self-approve this gate');
  }
  return { schemaVersion: 1, status: report?.collectionFailure || gates.some(g => g.status === 'FAIL') ? 'FAIL' : gates.every(g => g.status === 'PASS') ? 'PASS' : 'PARTIAL',
    eligibleIdentity: !!eligible, gates, ownerAcceptance: 'NOT_RUN', cloudCI: 'NOT_RUN' };
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [input, output] = process.argv.slice(2);
  if (!input || !output) throw Error('Usage: node scripts/nc-10-evidence.mjs RAW.json RECEIPT.json');
  const result = evaluateQualification(JSON.parse(await readFile(input, 'utf8')));
  await writeFile(output, JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ status: result.status, output }));
  if (result.status !== 'PASS') process.exitCode = 2;
}
