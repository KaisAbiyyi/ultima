import { test } from 'node:test';
import assert from 'node:assert/strict';
import { verifyNc10dIdle } from '../nc10d-qualification.mjs';

let serial = 0;
const source = 'a'.repeat(40), binary = 'b'.repeat(64), lock = 'c'.repeat(64), codex = 'd'.repeat(64);
const machine = { platform: 'win32', osRelease: '10.0', cpuModel: 'synthetic',
  logicalCpus: 16, totalMemoryBytes: 16 * 1024 ** 3, cache: 'warm',
  powerPlan: 'Balanced', graphics: 'egui/glow' };
function report(mode, rss, cpu = 0.4) {
  return {
    kind: 'NC10_RENDERER_ONLY_DIAGNOSTIC_NOT_PRODUCT_ACCEPTANCE',
    mode, sourceSha: source, binarySha256: binary, cargoLockSha256: lock,
    codexSha256: codex, codexVersion: 'codex-cli 0.164.0', machine,
    sourceDirty: false, buildReceiptVerified: true, durationMs: 120000, warmupMs: 60000,
    samples: [1, 2, 3].map(run => ({
      run, sessionNonce: '123e4567-e89b-42d3-a456-' + (++serial).toString(16).padStart(12, '0'),
      count: 60, attributableCodexSamples: 60,
      processNames: mode === 'cli' ? ['codex'] : ['ultima-native-renderer-spike','codex'],
      rssMiB: { median: rss - 1, p95: rss },
      privateCommitMiB: { median: 20, p95: 30 },
      cpuPercentMean: cpu,
    })),
    summary: { rssMiBP95AcrossRuns: rss, cpuPercentMean: cpu },
  };
}
const check = (cli, native) => verifyNc10dIdle(cli, native, {
  sourceSha: source, binarySha256: binary, cargoLockSha256: lock,
});

test('separates prototype idle PASS from real active and product NOT_RUN', () => {
  const result = check(report('cli', 70), report('connected', 160));
  assert.equal(result.idleRss.status, 'PASS');
  assert.equal(result.idleCpu.status, 'PASS');
  assert.equal(result.productAcceptance, 'OPEN');
  assert.equal(result.productionIntegration.status, 'NOT_RUN');
  assert.equal(result.activeWorkload.status, 'NOT_RUN');
});
test('does not relax 150MiB delta or 1 percent native idle CPU', () => {
  assert.equal(check(report('cli', 60), report('connected', 211)).idleRss.status, 'FAIL');
  assert.equal(check(report('cli', 60), report('connected', 90, 1.1)).idleCpu.status, 'FAIL');
});
test('refuses mismatched exact source, Codex, machine and power plan', () => {
  const c = report('cli', 70), n = report('connected', 120);
  assert.throws(() => check(c, {...n, codexSha256: 'f'.repeat(64)}), /matched machine/);
  assert.throws(() => check(c, {...n, machine: {...machine, powerPlan: 'changed'}}), /matched machine/);
  assert.throws(() => check(c, {...n, sourceSha: 'f'.repeat(40)}), /different source/);
  assert.throws(() => verifyNc10dIdle(c, n, {sourceSha: 'f'.repeat(40)}), /source\/build SHA/);
});
test('refuses missing CPU, coverage, rep counts, or replayed sessions', () => {
  const c = report('cli', 70), n = report('connected', 120);
  n.samples[1].sessionNonce = n.samples[0].sessionNonce;
  assert.throws(() => check(c, n), /duplicated/);
  n.samples[1].sessionNonce = '123e4567-e89b-42d3-a456-000000000011';
  n.samples[1].cpuPercentMean = Number.NaN;
  assert.throws(() => check(c, n), /CPU/);
  n.samples[1].cpuPercentMean = .4;
  n.samples[1].attributableCodexSamples = 59;
  assert.throws(() => check(c, n), /Codex absent/);
  assert.throws(() => check(c, {...n, samples: []}), /three runs/);
});
