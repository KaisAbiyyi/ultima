import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import os from 'node:os';
import path from 'node:path';
import { evaluateQualification, statistics } from '../scripts/nc-10-evidence.mjs';

test('NC-10 statistics retain p50/p95 and sample variance without coercing invalid data', () => {
  assert.deepEqual(statistics([1, 2, 3]), { count: 3, p50: 2, p95: 2.9, variance: 1 });
  assert.equal(statistics([]), null);
  assert.throws(() => statistics([1, NaN]));
  assert.throws(() => statistics([-1]));
});
test('NC-10 missing and synthetic evidence never produce a native PASS', () => {
  for (const report of [{}, { identity: { platform: 'linux' } }, { identity: { platform: 'win32', workload: 'synthetic' } }]) {
    const result = evaluateQualification(report);
    assert.notEqual(result.status, 'PASS');
    assert.equal(result.gates.some(gate => gate.status === 'PASS'), false);
  }
});
test('NC-10 collection errors are FAIL and cannot masquerade as an unperformed gate', () => {
  assert.equal(evaluateQualification({ collectionFailure: { stage: 'endurance-project-switch' } }).status, 'FAIL');
});
const sha = 'a'.repeat(64);
const identity = { platform: 'win32', physicalWindows: true, sourceSha: 'b'.repeat(40),
  dirty: false, artifactSha256: sha, codexSha256: sha, codexVersion: 'test-version',
  workload: 'official-codex', hardware: 'test-device', powerPlan: 'balanced',
  graphics: 'WebView2', authState: 'existing-login', cacheState: 'warm', fixtureSha256: sha,
  artifactUnchanged: true, codexProvenance: true,
  build: { sourceSha: 'b'.repeat(40), artifactSha256: sha, clean: true },
  deviceReceipt: { reviewedBy: 'test-reviewer', manufacturer: 'test-physical-vendor', model: 'test-model', evidenceSha256: sha } };
test('NC-10 proxy timing cannot be presented as input/event paint acceptance', () => {
  const result = evaluateQualification({ identity, latency: { boundary: 'flush-two-raf-proxy', input: [1, 2, 3], stream: [1, 2, 3] } });
  assert.equal(result.gates.find(g => g.id === 'input').status, 'NOT_RUN');
  assert.equal(result.gates.find(g => g.id === 'stream').status, 'NOT_RUN');
  assert.equal(result.ownerAcceptance, 'NOT_RUN');
});
test('NC-10 requires three matched idle runs, settled samples and no early exits', () => {
  const run = { valid: true, durationMs: 120000, samples: Array.from({ length: 121 }, (_, i) =>
    ({ elapsedMs: i * 1000, rssBytes: 100 + i, cpuPercent: 0.1, processCount: 2,
       tree: [{ name: 'codex', pid: 7, parentPid: 3, rssBytes: 80 }] })) };
  const baseline = { cli: [run, run, run], desktop: [run, run, run], matched: true };
  let result = evaluateQualification({ identity, idle: baseline });
  assert.equal(result.gates.find(g => g.id === 'idle-ram').status, 'PASS');
  assert.equal(result.gates.find(g => g.id === 'idle-cpu').status, 'PASS');
  result = evaluateQualification({ identity, idle: { ...baseline, matched: false } });
  assert.equal(result.gates.find(g => g.id === 'idle-ram').status, 'NOT_RUN');
  result = evaluateQualification({ identity, idle: { ...baseline, desktop: [{ ...run, valid: false }, run, run] } });
  assert.equal(result.gates.find(g => g.id === 'idle-ram').status, 'FAIL');
  const missingEngine = { ...run, samples: run.samples.map(sample =>
    sample.elapsedMs >= 60000 ? { ...sample, tree: [{ name: 'ultima-desktop', pid: 3 }] } : sample) };
  result = evaluateQualification({ identity, idle: { ...baseline, desktop: [run, run, missingEngine] } });
  assert.equal(result.gates.find(g => g.id === 'idle-ram').status, 'FAIL');
  assert.equal(result.gates.find(g => g.id === 'idle-cpu').status, 'FAIL');
  assert.match(result.gates.find(g => g.id === 'idle-ram').reason, /missing official Codex process/);
});
test('NC-10 does not accept missing sample coverage, nonfinite duration or changed artifacts', () => {
  const run = { valid: true, durationMs: 120000, samples: [
    { elapsedMs: 60000, rssBytes: 100, cpuPercent: 0, processCount: 2 },
    { elapsedMs: 120000, rssBytes: 100, cpuPercent: 0, processCount: 2 }
  ] };
  let result = evaluateQualification({ identity, idle: { matched: true, cli: [run, run, run], desktop: [run, run, run] } });
  assert.equal(result.gates.find(g => g.id === 'idle-ram').status, 'FAIL');
  result = evaluateQualification({ identity: { ...identity, artifactUnchanged: false },
    coldStart: { cache: 'cold', boundary: 'process-launch-to-responsive-composer', values: [1, 1, 1] } });
  assert.equal(result.gates.find(g => g.id === 'cold-start').status, 'NOT_RUN');
  const stress = { durationMs: NaN, turns: 12, projectSwitches: 3, pauseResume: true, restartRecovery: true, samples: run.samples };
  result = evaluateQualification({ identity, stress });
  assert.equal(result.gates.find(g => g.id === 'endurance').status, 'NOT_RUN');
});
test('NC-10 endurance needs a full hour, workload coverage and reports a growing tree', () => {
  const stress = { durationMs: 3600000, turns: 12, projectSwitches: 3, pauseResume: true,
    restartRecovery: true, valid: true, samples: Array.from({ length: 721 }, (_, i) => ({
      elapsedMs: i * 5000, rssBytes: 100 * 1024 ** 2 + i * 1024 ** 2, cpuPercent: 0, processCount: 3
    })) };
  let result = evaluateQualification({ identity, stress });
  assert.equal(result.gates.find(g => g.id === 'endurance').status, 'FAIL');
  result = evaluateQualification({ identity, stress: { ...stress, durationMs: 3599999 } });
  assert.equal(result.gates.find(g => g.id === 'endurance').status, 'NOT_RUN');
});
test('NC-10 refuses absent provenance, unreviewed device and missing artifact end-check', () => {
  for (const changes of [{ artifactUnchanged: undefined }, { build: undefined }, { codexProvenance: false },
    { deviceReceipt: undefined }, { build: { ...identity.build, sourceSha: 'c'.repeat(40) } }]) {
    const result = evaluateQualification({ identity: { ...identity, ...changes },
      coldStart: { cache: 'cold', boundary: 'process-launch-to-responsive-composer', values: [1, 1, 1] } });
    assert.equal(result.eligibleIdentity, false);
    assert.equal(result.gates.some(g => g.status === 'PASS'), false);
  }
});
test('NC-10 evidence CLI preserves existing raw and receipt files', async () => {
  const folder = await mkdtemp(path.join(os.tmpdir(), 'ultima-nc10-evidence-'));
  const input = path.join(folder, 'raw.json'); const output = path.join(folder, 'receipt.json');
  await writeFile(input, '{}'); await writeFile(output, 'existing receipt');
  for (const target of [input, output]) assert.throws(() => execFileSync(process.execPath,
    ['scripts/nc-10-evidence.mjs', input, target], { stdio: 'ignore' }));
  assert.equal(await readFile(input, 'utf8'), '{}');
  assert.equal(await readFile(output, 'utf8'), 'existing receipt');
});
