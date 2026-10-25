/**
 * Strict read-only candidate RAM guard, NOT NC-10 product acceptance.
 * Fail closed on mismatched/dirty/incomplete Windows evidence. Do not
 * weaken the contract even if a pretty new UI is more expensive.
 */
import { readFile } from 'node:fs/promises';

const LIMIT_MIB = 150;
function percentile(values, p) {
  const sorted = [...values].sort((a, b) => a - b);
  const rank = (sorted.length - 1) * p;
  return sorted[Math.floor(rank)] + (sorted[Math.ceil(rank)] - sorted[Math.floor(rank)]) * (rank % 1);
}

export function compareReports(cli, native) {
  const fail = reason => { throw Error('Invalid native idle evidence: ' + reason); };
  for (const [mode, report] of [['cli', cli], ['connected', native]]) {
    if (report?.mode !== mode) fail('wrong mode ' + mode);
    if (report.kind !== 'NC10_RENDERER_ONLY_DIAGNOSTIC_NOT_PRODUCT_ACCEPTANCE') fail('wrong evidence scope');
    if (report.sourceDirty !== false) fail('dirty source');
    if (!/^[0-9a-f]{40}$/i.test(report.sourceSha ?? '')) fail('missing source commit');
    if (!/^[0-9a-f]{64}$/i.test(report.binarySha256 ?? '')) fail('missing renderer binary hash');
    if (report.durationMs !== 120_000 || report.warmupMs !== 60_000) fail('not the full settled 120-second idle window');
    if (!Array.isArray(report.samples) || report.samples.length !== 3) fail('expected three runs');
    for (const [i, run] of report.samples.entries()) {
      if (run.run !== i + 1) fail('run identity missing/duplicated');
      if (!Number.isInteger(run.count) || run.count < 30) fail('insufficient settled process samples');
      if (!Number.isFinite(run.rssMiB?.p95) || run.rssMiB.p95 <= 0) fail('invalid RSS p95');
      if (run.attributableCodexSamples !== run.count) fail('Codex absent in at least one settled sample');
      if (!run.processNames?.includes('codex')) fail('missing official Codex process');
      if (mode === 'connected' && !run.processNames.includes('ultima-native-renderer-spike')) {
        fail('native renderer absent in settled run');
      }
    }
    if (!Number.isFinite(report.summary?.rssMiBP95AcrossRuns) || report.summary.rssMiBP95AcrossRuns <= 0) {
      fail('invalid cumulative RSS p95');
    }
    if (Math.abs(report.summary.rssMiBP95AcrossRuns -
      percentile(report.samples.map(run => run.rssMiB.p95), 0.95)) > 1e-6) {
      fail('reported aggregate RSS conflicts with source run samples');
    }
  }
  if (cli.sourceSha !== native.sourceSha || cli.binarySha256 !== native.binarySha256) {
    fail('different source or executable');
  }
  const additionalRssMiB = native.summary.rssMiBP95AcrossRuns - cli.summary.rssMiBP95AcrossRuns;
  const result = {
    scope: 'READ_ONLY_NATIVE_UI_IDLE_FEASIBILITY_NOT_PRODUCT_ACCEPTANCE',
    sourceSha: cli.sourceSha,
    binarySha256: cli.binarySha256,
    limitMiB: LIMIT_MIB,
    cliRssMiBP95: cli.summary.rssMiBP95AcrossRuns,
    nativeRssMiBP95: native.summary.rssMiBP95AcrossRuns,
    additionalRssMiB,
    headroomMiB: LIMIT_MIB - additionalRssMiB,
    status: additionalRssMiB <= LIMIT_MIB && additionalRssMiB >= 0 ? 'PASS' : 'FAIL',
  };
  return result;
}

if (process.argv[1]?.replaceAll('\\', '/').endsWith('/check-idle-budget.mjs')) {
  const [cliPath, nativePath] = process.argv.slice(2);
  if (!cliPath || !nativePath) {
    console.error('Usage: node check-idle-budget.mjs <cli-report.json> <connected-report.json>');
    process.exitCode = 2;
  } else {
    try {
      const [cli, native] = await Promise.all([
        readFile(cliPath, 'utf8').then(JSON.parse),
        readFile(nativePath, 'utf8').then(JSON.parse),
      ]);
      const result = compareReports(cli, native);
      console.log(JSON.stringify(result, null, 2));
      if (result.status !== 'PASS') process.exitCode = 1;
    } catch (error) {
      console.error(error instanceof Error ? error.message : 'Evidence validation failed');
      process.exitCode = 2;
    }
  }
}
