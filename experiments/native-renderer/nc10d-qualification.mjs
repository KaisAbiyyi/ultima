/**
 * NC-10D, fail-closed CLI/native *prototype idle* assessment. This may
 * qualify the 3x settled-idle feasibility budget but never product acceptance.
 * A genuine NC-10G product re-run and active load remain separate gates.
 */
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { compareReports } from './check-idle-budget.mjs';
import { verifyExactBuild } from './build-exact.mjs';

const SHA256 = /^[a-f0-9]{64}$/;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const percent = (values, p) => {
  const sorted = [...values].sort((a, b) => a - b);
  const at = (sorted.length - 1) * p;
  return sorted[Math.floor(at)] + (sorted[Math.ceil(at)] - sorted[Math.floor(at)]) * (at % 1);
};

export function verifyNc10dIdle(cli, native, { sourceSha, binarySha256, cargoLockSha256 } = {}) {
  // Keep the existing 150 MiB gate, including identity and native Codex process.
  const baseline = compareReports(cli, native);
  if (sourceSha && (baseline.sourceSha !== sourceSha || baseline.binarySha256 !== binarySha256 ||
    cli.cargoLockSha256 !== cargoLockSha256)) {
    throw Error('NC-10D source/build SHA differs from verified current executable');
  }
  if (!/^[a-f0-9]{40}$/.test(baseline.sourceSha)) throw Error('Invalid source SHA');
  const pair = [cli, native];
  if (!pair.every(r => SHA256.test(r.codexSha256 || '') &&
      typeof r.codexVersion === 'string' && /^codex-cli\s+\S+/.test(r.codexVersion) &&
      r.machine?.platform === 'win32' && Number.isInteger(r.machine.logicalCpus) &&
      r.machine.logicalCpus >= 1 && r.machine.totalMemoryBytes > 0 &&
      r.machine.cpuModel && r.machine.powerPlan &&
      r.machine.graphics && r.machine.cache && r.machine.osRelease)) {
    throw Error('Missing official Codex or matching Windows machine/power provenance');
  }
  if (cli.codexSha256 !== native.codexSha256 || cli.codexVersion !== native.codexVersion ||
      JSON.stringify(cli.machine) !== JSON.stringify(native.machine)) {
    throw Error('CLI/native runner is not a matched machine/version/power pair');
  }
  const seen = new Set();
  for (const [mode, r] of [['cli', cli], ['connected', native]]) {
    for (const run of r.samples) {
      if (typeof run.sessionNonce !== 'string' || !UUID.test(run.sessionNonce) ||
          seen.has(run.sessionNonce)) {
        throw Error('Missing or duplicated Windows native benchmark session identity');
      }
      seen.add(run.sessionNonce);
      if (!Number.isFinite(run.cpuPercentMean) || run.cpuPercentMean < 0 ||
          !Number.isFinite(run.rssMiB?.median) || run.rssMiB.median <= 0 ||
          !Number.isFinite(run.privateCommitMiB?.p95) || run.privateCommitMiB.p95 <= 0 ||
          run.privateCommitMiB.p95 < run.privateCommitMiB.median ||
          !Number.isFinite(run.rssMiB?.p95) || run.rssMiB.p95 < run.rssMiB.median ||
          run.attributableCodexSamples !== run.count) {
        throw Error('Incomplete or invalid native CPU/RSS/private-commit run: ' + mode);
      }
    }
  }
  if (!Number.isFinite(cli.summary?.cpuPercentMean) ||
      !Number.isFinite(native.summary?.cpuPercentMean) ||
      Math.abs(cli.summary.cpuPercentMean -
        cli.samples.reduce((s, x) => s + x.cpuPercentMean, 0) / 3) > 1e-6 ||
      Math.abs(native.summary.cpuPercentMean -
        native.samples.reduce((s, x) => s + x.cpuPercentMean, 0) / 3) > 1e-6) {
    throw Error('Aggregate CPU does not match bounded source runs');
  }
  const cpu = {
    limitPercent: 1,
    nativeMeanPercent: native.summary.cpuPercentMean,
    cliMeanPercent: cli.summary.cpuPercentMean,
    nativeRunP95Percent: percent(native.samples.map(r => r.cpuPercentMean), .95),
    status: native.summary.cpuPercentMean <= 1 ? 'PASS' : 'FAIL',
  };
  return {
    kind: 'NC10D_EXACT_SOURCE_NATIVE_IDLE_FEASIBILITY',
    sourceSha: baseline.sourceSha,
    binarySha256: baseline.binarySha256,
    cargoLockSha256: cli.cargoLockSha256,
    codexVersion: cli.codexVersion,
    codexSha256: cli.codexSha256,
    machine: cli.machine,
    repetitions: 3,
    warmupMs: 60_000,
    measuredWindowMs: 60_000,
    cli: cli.samples.map(r => ({ run: r.run, count: r.count, rssMiB: r.rssMiB,
      privateCommitMiB: r.privateCommitMiB, cpuPercentMean: r.cpuPercentMean })),
    native: native.samples.map(r => ({ run: r.run, count: r.count, rssMiB: r.rssMiB,
      privateCommitMiB: r.privateCommitMiB, cpuPercentMean: r.cpuPercentMean })),
    idleRss: baseline,
    idleCpu: cpu,
    coldStart: { status: 'NOT_RUN', reason: 'Screen-visible native cold start needs separate Windows pixel evidence' },
    activeWorkload: { status: 'NOT_RUN', reason: 'Authenticated real active Codex workload not measured by idle harness' },
    productionIntegration: { status: 'NOT_RUN', reason: 'NC-10G native production integration must be independently completed and requalified' },
    productAcceptance: 'OPEN',
    note: 'RSS includes shared pages across attributable processes; paint/UIA proxy is not physical presentation.',
  };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [cliPath, nativePath] = process.argv.slice(2);
  if (!cliPath || !nativePath) throw Error('Usage: node nc10d-qualification.mjs <cli-report.json> <connected-report.json>');
  const exact = await verifyExactBuild();
  const [cli, native] = await Promise.all([cliPath, nativePath].map(p =>
    readFile(p, 'utf8').then(JSON.parse)));
  const result = verifyNc10dIdle(cli, native, exact);
  process.stdout.write(JSON.stringify(result, null, 2) + '\n');
  // Do not return PASS for the whole NC-10D while active/native-product gates
  // are NOT_RUN; callers must explicitly inspect the scoped gate statuses.
  if (result.idleRss.status !== 'PASS' || result.idleCpu.status !== 'PASS') process.exitCode = 1;
}
