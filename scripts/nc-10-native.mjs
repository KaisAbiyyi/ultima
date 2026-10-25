/** Local-only physical Windows runner. Never reads credentials or approves tools. */
import { spawn, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { mkdir, open, readFile, realpath, stat, writeFile } from 'node:fs/promises';
import { createInterface } from 'node:readline';
import { once } from 'node:events';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { chromium } from '@playwright/test';
import { evaluateQualification, statistics } from './nc-10-evidence.mjs';
import { createOutputDirectory, requireStressProjects, fingerprintFixtures } from './nc-10-support.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const ownedChildren = new Set();
const trackedSpawn = (...args) => {
  const child = spawn(...args); ownedChildren.add(child);
  child.once('exit', () => ownedChildren.delete(child));
  return child;
};
for (const signal of ['SIGINT', 'SIGTERM']) process.once(signal, () => {
  // Abrupt interruption is not a qualification receipt. Preserve streamed tree
  // files, terminate only still-live handles owned by this driver, retain lock
  // for explicit recovery and never label an interrupted run complete.
  for (const child of ownedChildren) void closeOwned(child);
  process.exit(130);
});
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const json = async (file, value) => writeFile(file, JSON.stringify(value, null, 2) + '\n');
async function digest(file) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  return hash.digest('hex');
}
function ps(script, args = []) {
  return execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script, ...args], { encoding: 'utf8', windowsHide: true }).trim();
}
async function trustedExe(file) {
  if (typeof file !== 'string' || !path.isAbsolute(file) || path.extname(file).toLowerCase() !== '.exe') throw Error('Use an absolute trusted .exe path');
  const resolved = await realpath(file);
  if (!(await stat(resolved)).isFile()) throw Error('Executable is not a file');
  return resolved;
}
async function waitFor(check, timeoutMs = 30000) {
  const end = Date.now() + timeoutMs;
  while (Date.now() < end) {
    const value = await check();
    if (value) return value;
    await delay(100);
  }
  throw Error('Native operation timed out');
}
function rpcClient(child) {
  const requests = new Map();
  let id = 0;
  let buffered = '';
  child.stdout.setEncoding('utf8');
  child.stdout.on('data', chunk => {
    buffered += chunk;
    if (buffered.length > 1024 * 1024) { child.kill(); return; }
    let newline;
    while ((newline = buffered.indexOf('\n')) >= 0) {
      const line = buffered.slice(0, newline); buffered = buffered.slice(newline + 1);
      let frame; try { frame = JSON.parse(line); } catch { continue; }
      if (frame.id !== undefined && requests.has(frame.id)) {
        const request = requests.get(frame.id); requests.delete(frame.id);
        frame.error ? request.reject(Error('Official engine RPC failed')) : request.resolve(frame.result);
      } else if (frame.id !== undefined && typeof frame.method === 'string') {
        // No allow path exists in this noninteractive qualification driver.
        const approval = /requestApproval$/.test(frame.method);
        child.stdin.write(JSON.stringify(approval ? { id: frame.id, result: { decision: 'decline' } } :
          { id: frame.id, error: { code: -32601, message: 'Unsupported qualification request' } }) + '\n');
      }
    }
  });
  child.stderr.resume(); // Drain without retaining content or secrets.
  child.on('exit', () => { for (const req of requests.values()) req.reject(Error('Engine exited')); requests.clear(); });
  return {
    async call(method, params) {
      if (requests.size >= 16) throw Error('RPC request limit');
      const requestId = ++id;
      let timer;
      try {
        return await new Promise((resolve, reject) => {
          timer = setTimeout(() => { requests.delete(requestId); reject(Error('RPC timed out')); }, 30000);
          requests.set(requestId, { resolve, reject });
          child.stdin.write(JSON.stringify({ id: requestId, method, params }) + '\n');
        });
      } finally { clearTimeout(timer); }
    },
    notify(method) { child.stdin.write(JSON.stringify({ method }) + '\n'); }
  };
}
async function sampler(child, folder, durationMs) {
  const output = path.join(folder, 'tree.jsonl');
  const stop = path.join(folder, 'sampler.stop');
  const worker = trackedSpawn('powershell.exe', ['-NoProfile', '-NonInteractive', '-File', path.join(root, 'scripts/nc-10-sample.ps1'),
    '-RootProcessId', String(child.pid), '-OutputFile', output, '-StopFile', stop,
    '-DurationMs', String(Math.min(durationMs + 30000, 3900000))], { windowsHide: true, stdio: ['ignore', 'ignore', 'pipe'] });
  let failed = false;
  worker.stderr.on('data', () => { failed = true; });
  const completion = once(worker, 'exit').catch(() => [1]);
  return async () => {
    await writeFile(stop, 'stop');
    const [code] = await completion;
    const samples = [];
    try {
      for await (const line of createInterface({ input: createReadStream(output), crlfDelay: Infinity })) {
        if (samples.length >= 240000) throw Error('Sample limit exceeded');
        samples.push(JSON.parse(line));
      }
    } catch { failed = true; }
    return { samples, valid: !failed && code === 0 && samples.length >= 2 };
  };
}
async function closeOwned(child) {
  if (!child.pid || child.exitCode !== null || child.signalCode !== null) return;
  // This PID is a live child handle created by this runner; never enumerate and
  // kill by executable name, user data directory or arbitrary PID from a plan.
  try { execFileSync('taskkill.exe', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore', windowsHide: true }); }
  catch { if (child.exitCode === null && child.signalCode === null) throw Error('Owned process shutdown unconfirmed'); }
}
async function desktop(config, folder, report) {
  // Isolated profile: never modify the owner's normal WebView storage.
  const child = trackedSpawn(config.exe, [], { cwd: config.projects[0], windowsHide: false, stdio: 'ignore', env: { ...process.env,
    WEBVIEW2_USER_DATA_FOLDER: path.join(folder, 'webview-profile'),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-address=127.0.0.1 --remote-debugging-port=0' } });
  let browser, page;
  try {
    report.collectionStage = 'native-launch-and-cdp';
    await once(child, 'spawn');
    const portFile = path.join(folder, 'webview-profile', 'EBWebView', 'DevToolsActivePort');
    const port = await waitFor(async () => {
      if (child.exitCode !== null) throw Error('Desktop exited before WebView startup');
      try { const port = Number((await readFile(portFile, 'utf8')).split('\n')[0]); return port > 0 ? port : null; } catch { return null; }
    }, 45000);
    browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
    page = await waitFor(async () => browser.contexts().flatMap(context => context.pages()).find(p => /tauri\.localhost|^tauri:/.test(p.url())));
    report.collectionStage = 'desktop-shell';
    page.setDefaultTimeout(30000);
    await page.waitForFunction(() => !!window.__ultimaPerformance);
    await page.evaluate(() => window.__ultimaPerformance.start());
    const shellReady = await page.evaluate(() => performance.getEntriesByName('ultima:shell-ready')[0]?.startTime ?? null);
    const folderInput = page.getByRole('textbox', { name: 'Absolute project folder' });
    for (const project of config.projects) {
      await folderInput.fill(project);
      await page.getByRole('button', { name: 'Add project', exact: true }).click();
    }
    await folderInput.fill(config.projects[0]);
    // Existing project buttons select by their title, which is the absolute path.
    await page.locator('button.project-item').filter({ hasText: path.basename(config.projects[0]) }).first().click();
    await page.locator('#nc06-executable').fill(config.codex);
    report.collectionStage = 'system-runtime-selection';
    await page.waitForFunction(() => document.querySelector('[data-testid="nc12-runtime-state"]')?.textContent?.includes('Selection: system'));
    await page.getByRole('button', { name: 'Connect to Codex', exact: true }).click();
    report.collectionStage = 'official-engine-connect';
    await page.waitForFunction(() => document.querySelector('.nc06-status')?.textContent === 'ready');
    return { child, browser, page, shellReady, async close() { await browser.close(); await closeOwned(child); } };
  } catch (error) {
    if (page) await page.screenshot({ path: path.join(folder, 'failure.png') }).catch(() => {});
    await browser?.close(); await closeOwned(child); throw error;
  }
}
async function inputProbe(page) {
  const composer = page.getByRole('textbox', { name: 'Message to Codex' });
  await composer.focus();
  for (let i = 0; i < 40; i++) { await composer.press('x'); await delay(60); }
  await composer.fill('');
  await delay(100);
}
async function turn(page) {
  const composer = page.getByRole('textbox', { name: 'Message to Codex' });
  await composer.fill('Reply with exactly OK. Do not use any tools, read files, run commands, or change files.');
  await page.getByRole('button', { name: 'Send message to Codex', exact: true }).click();
  // Require both a rendered assistant answer and end-of-turn acknowledgement.
  await page.waitForFunction(() => [...document.querySelectorAll('.message-row:not(.user-message) .message-content p')]
    .some(row => row.textContent?.trim() === 'OK'), undefined, { timeout: 120000 });
  await composer.waitFor({ state: 'visible' });
  await waitFor(async () => await composer.isEnabled(), 120000);
  if (await page.getByRole('alert').count()) throw Error('Desktop reported a turn error');
}
async function resumeStoredThread(page) {
  const thread = page.locator('button.history-link').first();
  if (await thread.count() === 0) return false;
  await thread.click();
  await page.waitForFunction(() => [...document.querySelectorAll('.message-row:not(.user-message) .message-content p')]
    .some(row => row.textContent?.trim() === 'OK'));
  if (await page.getByRole('alert').count()) throw Error('Durable thread resume failed');
  return true;
}
async function recoverOwnedRuntime(app, config) {
  await app.page.getByRole('button', { name: 'Disconnect engine', exact: true }).click();
  await app.page.getByRole('button', { name: 'Connect to Codex', exact: true }).click();
  await app.page.waitForFunction(() => document.querySelector('.nc06-status')?.textContent === 'ready');
  const pauseResume = await resumeStoredThread(app.page);
  execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-File', path.join(root, 'scripts/nc-10-owned-crash.ps1'),
    '-RootProcessId', String(app.child.pid), '-ExpectedExecutable', config.codex], { windowsHide: true, stdio: 'pipe' });
  await app.page.waitForFunction(() => document.querySelector('.nc06-status')?.textContent === 'disconnected');
  await app.page.getByRole('button', { name: 'Connect to Codex', exact: true }).click();
  await app.page.waitForFunction(() => document.querySelector('.nc06-status')?.textContent === 'ready');
  const restartRecovery = await resumeStoredThread(app.page);
  return { pauseResume, restartRecovery, boundary: 'clean-disconnect-resume-and-owned-engine-crash-resume' };
}
async function switchProject(page, project) {
  const disconnect = page.getByRole('button', { name: 'Disconnect engine', exact: true });
  if (await disconnect.count()) {
    await disconnect.click();
    // The project button stays clickable while teardown is pending, but the
    // application correctly rejects that premature selection. Await the real
    // host acknowledgement before asking it to change the working directory.
    await page.waitForFunction(() => document.querySelector('.nc06-status')?.textContent === 'disconnected');
  }
  await page.locator('button.project-item').filter({ hasText: path.basename(project) }).first().click();
  await page.waitForFunction(expected => document.querySelector('button.project-item[aria-current="page"]')?.getAttribute('title') === expected, project);
  await page.getByRole('button', { name: 'Connect to Codex', exact: true }).click();
  await page.waitForFunction(() => document.querySelector('.nc06-status')?.textContent === 'ready');
}
async function collect(config, output, report) {
  const mode = config.mode ?? 'smoke';
  if (!['smoke', 'idle', 'stress', 'project-smoke'].includes(mode)) throw Error('Unsupported native mode');
  if (mode === 'stress' && config.allowModelTurns !== true) throw Error('Stress requires explicit allowModelTurns in the private plan');
  if (mode === 'stress' || mode === 'project-smoke') requireStressProjects(config.projects);
  if (mode === 'idle') {
    report.idle = { matched: true, cli: [], desktop: [] };
    for (let repetition = 0; repetition < 3; repetition++) {
      for (const variant of ['cli', 'desktop']) {
        const folder = path.join(output, `${variant}-${repetition + 1}`); await mkdir(folder);
        let child, app, finish;
        try {
          if (variant === 'cli') {
            child = trackedSpawn(config.codex, ['app-server'], { cwd: config.projects[0], windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
            await once(child, 'spawn');
            const client = rpcClient(child);
            await client.call('initialize', { clientInfo: { name: 'ultima', title: 'Ultima', version: '0.1.0' }, capabilities: { experimentalApi: false } });
            client.notify('initialized'); await client.call('model/list', {});
          } else { app = await desktop(config, folder, report); child = app.child; }
          finish = await sampler(child, folder, 120000);
          await delay(120000);
          const sampled = await finish(); finish = null;
          report.idle[variant].push({ ...sampled, durationMs: 120000, valid: sampled.valid && child.exitCode === null });
        } finally { if (finish) await finish(); if (app) await app.close(); else if (child) await closeOwned(child); }
        await json(path.join(output, 'raw.json'), report);
        console.log(`${variant} idle repetition ${repetition + 1}/3 recorded`);
      }
    }
    return;
  }
  const folder = path.join(output, mode); await mkdir(folder);
  const app = await desktop(config, folder, report);
  let finish;
  try {
    const durationMs = mode === 'stress' ? 3600000 : 15000;
    finish = await sampler(app.child, folder, durationMs);
    report.collectionStage = 'input-probe';
    await inputProbe(app.page);
    report.shellReadyProxyMs = app.shellReady;
    await app.page.screenshot({ path: path.join(output, 'native-connected.png') });
    if (mode === 'project-smoke') {
      report.projectSwitchSmoke = { verifiedSwitches: 0, diagnosticOnly: true };
      for (let i = 1; i <= 3; i++) {
        if (config.allowModelTurns === true) await turn(app.page);
        report.collectionStage = 'project-switch-smoke';
        await switchProject(app.page, config.projects[i % 3]);
        report.projectSwitchSmoke.verifiedSwitches++;
      }
      report.smoke = await finish(); finish = null;
    } else if (mode === 'stress') {
      const start = Date.now();
      let turns = 0, switches = 0;
      let recovery = { pauseResume: false, restartRecovery: false };
      while (Date.now() - start < durationMs && turns < 64) {
        report.collectionStage = 'endurance-model-turn';
        await turn(app.page); turns++;
        if (turns === 1) { report.collectionStage = 'endurance-owned-recovery'; recovery = await recoverOwnedRuntime(app, config); }
        // Explicit disconnect/reconnect exercises the real SessionGate. No
        // injected fixtures and no change to approvals or engine permissions.
        if (turns % 3 === 0) {
          const project = config.projects[(switches + 1) % config.projects.length];
          report.collectionStage = 'endurance-project-switch';
          await switchProject(app.page, project);
          switches++;
        }
        report.progress = { elapsedMs: Date.now() - start, turns, projectSwitches: switches };
        await json(path.join(output, 'progress.json'), report.progress);
        console.log(`Endurance ${Math.round((Date.now() - start) / 60000)} min; turns=${turns}; switches=${switches}`);
        await delay(Math.min(60000, Math.max(0, durationMs - (Date.now() - start))));
      }
      report.stress = { durationMs: Date.now() - start, turns, projectSwitches: switches,
        ...recovery, ...(await finish()) }; finish = null;
    } else {
      if (config.allowModelTurns === true) {
        report.collectionStage = 'official-model-turn'; await turn(app.page);
        if (config.exerciseRecovery === true) { report.collectionStage = 'owned-engine-crash-resume'; report.recovery = await recoverOwnedRuntime(app, config); }
      }
      await delay(15000); report.smoke = await finish(); finish = null;
    }
    report.latency = await app.page.evaluate(() => window.__ultimaPerformance.snapshot());
    report.latency.inputStatistics = statistics(report.latency.input);
    report.latency.streamStatistics = statistics(report.latency.stream);
    await app.page.screenshot({ path: path.join(output, 'native-after-turn.png') });
  } finally { if (finish) await finish(); await app.close(); }
}
async function main() {
  if (process.platform !== 'win32') throw Error('NC-10 native runner requires Windows');
  const [planFile, outputArg] = process.argv.slice(2);
  if (!planFile || !outputArg) throw Error('Usage: node scripts/nc-10-native.mjs PRIVATE_PLAN.json NEW_OUTPUT_DIRECTORY');
  const config = JSON.parse(await readFile(planFile, 'utf8'));
  config.exe = await trustedExe(config.exe); config.codex = await trustedExe(config.codex);
  if (!Array.isArray(config.projects) || config.projects.length < 1 || config.projects.length > 3) throw Error('Choose one to three private fixture projects');
  config.projects = await Promise.all(config.projects.map(async p => {
    if (!path.isAbsolute(p)) throw Error('Use absolute private fixture project paths');
    const resolved = await realpath(p); if (!(await stat(resolved)).isDirectory()) throw Error('Project must be a directory'); return resolved;
  }));
  if (config.mode === 'stress') requireStressProjects(config.projects);
  const fixtureSha256 = await fingerprintFixtures(config.projects);
  const output = path.resolve(outputArg); await createOutputDirectory(output);
  let build = null, deviceReceipt = null;
  try { build = JSON.parse(await readFile(config.exe + '.nc10-build.json', 'utf8')); } catch { /* diagnostic only */ }
  if (config.deviceReceiptFile) {
    const data = await readFile(config.deviceReceiptFile);
    deviceReceipt = { ...JSON.parse(data.toString('utf8')), evidenceSha256: createHash('sha256').update(data).digest('hex') };
  }
  const official = JSON.parse(await readFile(path.join(root, 'protocol/managed-assets.json'), 'utf8'));
  const codexSha256 = await digest(config.codex);
  const report = { schemaVersion: 1, startedAt: new Date().toISOString(), identity: {
    platform: process.platform, physicalWindows: config.physicalWindows === true,
    sourceSha: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(),
    dirty: !!execFileSync('git', ['status', '--porcelain'], { cwd: root, encoding: 'utf8' }).trim(),
    artifactSha256: await digest(config.exe), codexSha256, build, deviceReceipt,
    codexProvenance: official.assets.some(asset => asset.target === 'x86_64-pc-windows-msvc' && asset.sha256 === codexSha256),
    codexVersion: execFileSync(config.codex, ['--version'], { encoding: 'utf8', windowsHide: true }).trim(),
    fixtureSha256, planSha256: createHash('sha256').update(await readFile(planFile)).digest('hex'),
    workload: 'official-codex', hardware: `${os.cpus()[0]?.model}; ${os.totalmem()} RAM bytes; ${os.cpus().length} logical CPUs`,
    os: `${os.type()} ${os.release()} ${os.arch()}`, powerPlan: ps('powercfg /getactivescheme'),
    graphics: 'WebView2; CDP attached; graphics backend not independently verified',
    authState: 'Existing official Codex authentication; credentials never read by driver',
    cacheState: 'warm/unspecified OS cache; isolated WebView profile', debuggerAttached: true
  }, limitations: ['CDP attachment affects timings and resource use.', 'RSS shared pages may be double-counted.',
    'Two-rAF timings are scheduling proxies, not compositor/physical input evidence.',
    'No OS cache purge, signed packaging, human IME, screen reader or owner acceptance.'] };
  await mkdir(path.join(root, 'artifacts/nc10'), { recursive: true });
  const lock = await open(path.join(root, 'artifacts/nc10/native-benchmark.lock'), 'wx');
  try {
    await lock.writeFile(JSON.stringify({ owner: 'Codex-local-20261025-NC10', pid: process.pid, output }));
    await collect(config, output, report);
  } catch (error) {
    // Preserve failed raw observations while omitting arbitrary upstream messages.
    report.collectionFailure = { stage: report.collectionStage, name: error.name, code: typeof error.code === 'string' ? error.code : 'NATIVE_OPERATION_FAILED' };
    console.error('Native collection failed; see partial receipt. No product PASS.');
    process.exitCode = 1;
  } finally {
    report.finishedAt = new Date().toISOString();
    report.identity.artifactUnchanged = (await digest(config.exe)) === report.identity.artifactSha256;
    await json(path.join(output, 'raw.json'), report);
    await json(path.join(output, 'receipt.json'), evaluateQualification(report));
    await lock.close();
    const { unlink } = await import('node:fs/promises'); await unlink(path.join(root, 'artifacts/nc10/native-benchmark.lock'));
  }
  console.log(JSON.stringify({ output, status: evaluateQualification(report).status }));
}
await main();
