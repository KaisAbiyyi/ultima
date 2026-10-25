/** Renderer-only NC-10 feasibility measurement; never grants native acceptance. */
import { spawn, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { once } from 'node:events';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { closeOwned } from '../../scripts/nc-10-processes.mjs';

const root = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(root, '../..');
const exe = path.join(root, 'target', 'release', 'ultima-native-renderer-spike.exe');
const mode = process.argv[2] ?? 'empty';
if (!['empty', 'history', 'connected', 'connected-history', 'cli'].includes(mode)) {
  throw Error('Usage: node benchmark.mjs empty|history|connected|connected-history|cli [--full-idle]');
}
const cliOnly = mode === 'cli';
const requiresCodex = mode.startsWith('connected') || cliOnly;
const fixture = requiresCodex
  ? JSON.parse(await readFile(path.join(repo, 'artifacts/nc10/project-smoke-plan.json'), 'utf8'))
  : null;
const repetitions = 3;
const durationMs = process.argv.includes('--full-idle') ? 120_000 : 45_000;
const warmupMs = durationMs === 120_000 ? 60_000 : 15_000;
const sourceSha = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: repo, encoding: 'utf8' }).trim();
const executableSha256 = createHash('sha256').update(await readFile(exe)).digest('hex');
const dir = path.join(root, 'results', mode + '-' + sourceSha.slice(0, 8) + '-' + Date.now());
await mkdir(dir, { recursive: true });
function percentile(values, p) {
  const sorted = [...values].sort((a, b) => a - b);
  const rank = (sorted.length - 1) * p;
  return sorted[Math.floor(rank)] + (sorted[Math.ceil(rank)] - sorted[Math.floor(rank)]) * (rank % 1);
}
const sourceDirty = execFileSync('git', ['status', '--porcelain=v1', '--untracked-files=all', '--', 'experiments/native-renderer'],
  { cwd: repo, encoding: 'utf8' }).trim().length > 0;
const report = {
  kind: 'NC10_RENDERER_ONLY_DIAGNOSTIC_NOT_PRODUCT_ACCEPTANCE',
  sourceSha, sourceDirty, binarySha256: executableSha256, mode, durationMs,
  warmupMs, samples: [],
};

async function initializeOfficialAppServer(app) {
  await new Promise((resolve, reject) => {
    let buffered = '';
    const timeout = setTimeout(() => reject(Error('Official app-server initialize timed out')), 15_000);
    const onData = bytes => {
      buffered += bytes.toString('utf8');
      if (buffered.length > 1024 * 1024) {
        clearTimeout(timeout);
        reject(Error('Oversized official initialization output'));
        return;
      }
      let split;
      while ((split = buffered.indexOf('\n')) >= 0) {
        const line = buffered.slice(0, split);
        buffered = buffered.slice(split + 1);
        let event;
        try { event = JSON.parse(line); } catch { continue; }
        if (event.id !== 1) continue;
        clearTimeout(timeout);
        app.stdout.off('data', onData);
        if (event.error) reject(Error('Official app-server initialize returned an error'));
        else {
          app.stdin.write(JSON.stringify({ method: 'initialized', params: {} }) + '\n');
          // Keep draining stdout with bounded memory, never persist protocol
          // payloads, conversation contents, account identifiers or stderr.
          app.stdout.resume();
          resolve();
        }
        return;
      }
    };
    app.stdout.on('data', onData);
    app.stdin.write(JSON.stringify({
      id: 1, method: 'initialize',
      params: { clientInfo: { name: 'ultima', title: 'Ultima', version: '0.1.0' }, capabilities: { experimentalApi: false } },
    }) + '\n');
  });
}
for (let run = 1; run <= repetitions; run++) {
  const out = path.join(dir, 'run-' + run);
  await mkdir(out);
  const output = path.join(out, 'tree.jsonl');
  const stop = path.join(out, 'stop');
  const flags = mode.endsWith('history') ? ['--fixture-history=10000'] : [];
  if (fixture) {
    flags.push('--codex-exe=' + fixture.codex, '--project=' + fixture.projects[0]);
  }
  const app = cliOnly
    ? spawn(fixture.codex, ['app-server'], {
      cwd: fixture.projects[0], windowsHide: true, stdio: ['pipe', 'pipe', 'ignore'],
    })
    : spawn(exe, flags, { cwd: root, windowsHide: true, stdio: 'ignore' });
  let sampler;
  try {
    await once(app, 'spawn');
    if (cliOnly) {
      await initializeOfficialAppServer(app);
    } else if (requiresCodex) {
      let ready = false;
      for (let attempt = 0; attempt < 25; attempt++) {
        await new Promise(resolve => setTimeout(resolve, 400));
        let title = '';
        try {
          title = execFileSync('powershell.exe', [
            '-NoProfile', '-NonInteractive', '-Command',
            '(Get-Process -Id ' + String(app.pid) + ' -ErrorAction Stop).MainWindowTitle',
          ], { encoding: 'utf8', windowsHide: true, timeout: 5000 }).trim();
        } catch { /* window may not be created yet */ }
        if (title.includes('APP SERVER FAILED')) throw Error('Native Codex handshake failed');
        if (title.includes('APP SERVER READY')) { ready = true; break; }
      }
      if (!ready) throw Error('Native engine connection not acknowledged by window');
    }
    sampler = spawn('powershell.exe', [
      '-NoProfile', '-NonInteractive', '-File', path.join(repo, 'scripts', 'nc-10-sample.ps1'),
      '-RootProcessId', String(app.pid), '-OutputFile', output,
      '-StopFile', stop, '-DurationMs', String(durationMs + 2000),
    ], { cwd: repo, windowsHide: true, stdio: ['ignore', 'ignore', 'pipe'] });
    let stderr = '';
    sampler.stderr.on('data', chunk => { stderr = (stderr + chunk.toString('utf8')).slice(-2000); });
    const finished = once(sampler, 'exit');
    await new Promise(resolve => setTimeout(resolve, durationMs));
    await writeFile(stop, 'stop\n');
    const [exitCode] = await finished;
    if (exitCode !== 0) throw Error('Sampler failed: ' + stderr.replace(/[a-zA-Z]:\\[^\s]+/g, '[redacted path]'));
    const samples = [];
    for await (const line of createInterface({ input: createReadStream(output) })) {
      if (line.trim()) samples.push(JSON.parse(line));
    }
    const settled = samples.filter(sample => sample.elapsedMs >= warmupMs && sample.elapsedMs <= durationMs);
    if (settled.length < 10 || settled.some(sample => sample.processCount < 1)) {
      throw Error('Insufficient living-window sample coverage');
    }
    if (requiresCodex && settled.some(sample =>
      !sample.tree.some(child => child.name.toLowerCase() === 'codex'))) {
      throw Error('Official engine did not remain present for all settled samples');
    }
    const avgCpu = settled.reduce((sum, sample) => sum + sample.cpuPercent, 0) / settled.length;
    const entry = {
      run, count: settled.length,
      rssMiB: {
        median: percentile(settled.map(s => s.rssBytes), 0.5) / 1048576,
        p95: percentile(settled.map(s => s.rssBytes), 0.95) / 1048576,
      },
      privateCommitMiB: {
        median: percentile(settled.map(s => s.privateCommitBytes), 0.5) / 1048576,
        p95: percentile(settled.map(s => s.privateCommitBytes), 0.95) / 1048576,
      },
      cpuPercentMean: avgCpu,
      processCount: settled.at(-1).processCount,
      processNames: [...new Set(settled.at(-1).tree.map(p => p.name))],
      attributableCodexSamples: settled.filter(s => s.tree.some(p => p.name.toLowerCase() === 'codex')).length,
    };
    report.samples.push(entry);
    console.log(JSON.stringify(entry));
  } finally {
    await closeOwned(app);
    if (sampler?.exitCode === null) sampler.kill();
  }
}
report.summary = {
  count: report.samples.length,
  rssMiBP95AcrossRuns: percentile(report.samples.map(r => r.rssMiB.p95), 0.95),
  privateCommitMiBP95AcrossRuns: percentile(report.samples.map(r => r.privateCommitMiB.p95), 0.95),
  cpuPercentMean: report.samples.reduce((sum, sample) => sum + sample.cpuPercentMean, 0) / report.samples.length,
};
await writeFile(path.join(dir, 'report.json'), JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify({ report: path.join(dir, 'report.json'), summary: report.summary }));
