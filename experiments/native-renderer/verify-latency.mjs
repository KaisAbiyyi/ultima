/**
 * NC-10B source-bound diagnostic receipt verifier.
 * OpenGL callback is before swap, so these figures NEVER independently prove
 * compositor presentation. Real screen/keyboard timing needs separate UIA.
 */
import { readFile } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { verifyExactBuild } from './build-exact.mjs';

const script = fileURLToPath(import.meta.url);
const MAX_SAMPLES = 4096;

export function inspectDiagnostic(receipts) {
  if (!Array.isArray(receipts) || receipts.length < 3) {
    throw Error('At least 3 independent native sessions are required');
  }
  const series = ['streamReceivedToPaintCallback', 'rawKeyToComposerPaintCallback',
    'rawKeyDuringActiveTurnToPaintCallback',
    'processMainToFirstComposerPaintCallback'];
  const sums = Object.fromEntries(series.map(name => [name, []]));
  for (const receipt of receipts) {
    if (receipt?.kind !== 'NC10B_GLOW_PAINT_DIAGNOSTIC' ||
        receipt.version !== 1 || receipt.presentationVerified !== false ||
        !receipt.stage?.includes('before buffer swap') ||
        receipt.acceptance !== 'NOT_RUN: independent Windows screen-presentation receipt required') {
      throw Error('Native timing receipt type/explicit non-acceptance is invalid');
    }
    for (const name of series) {
      const sample = receipt[name];
      const minSamples = name === 'processMainToFirstComposerPaintCallback' ? 1 : 30;
      if (!sample || !Number.isSafeInteger(sample.samples) || sample.samples < minSamples ||
          sample.samples > MAX_SAMPLES || sample.overflow !== 0 ||
          !Number.isFinite(sample.p50Ms) || !Number.isFinite(sample.p95Ms) ||
          sample.p50Ms < 0 || sample.p95Ms < sample.p50Ms ||
          !Number.isFinite(sample.maxMs) || sample.maxMs < sample.p95Ms) {
        throw Error(`Invalid or truncated timing series: ${name}`);
      }
      if (name === 'processMainToFirstComposerPaintCallback' && sample.samples !== 1) {
        throw Error('Expected exactly one startup sample per launched process');
      }
      sums[name].push({ n: sample.samples, p50Ms: sample.p50Ms, p95Ms: sample.p95Ms });
    }
  }
  return {
    status: 'DIAGNOSTIC_ONLY__NATIVE_PRESENTATION_NOT_RUN',
    p95LimitMs: 50,
    coldStartP95LimitMs: 1500,
    runs: receipts.length,
    metrics: Object.fromEntries(Object.entries(sums).map(([name, rows]) => [
      name,
      {
        totalSamples: rows.reduce((total, row) => total + row.n, 0),
        perRun: rows,
        allRunP95WithinDiagnosticBudget: rows.every(row =>
          row.p95Ms <= (name.startsWith('processMain') ? 1500 : 50)),
      },
    ])),
    actualPixelsPresented: 'NOT_RUN',
    appServerActiveStreamValidated: 'NOT_RUN (requires authenticated Windows workload evidence)',
  };
}

if (process.argv[1] && path.resolve(process.argv[1]) === script) {
  const files = process.argv.slice(2);
  const verified = await verifyExactBuild();
  const receipts = await Promise.all(files.map(async file =>
    JSON.parse(await readFile(file, 'utf8'))));
  const summary = inspectDiagnostic(receipts);
  let codexVersion = 'NOT_RUN';
  // The local fixture specifies an explicitly trusted Codex installation.
  // Never record credentials, args, prompt content, environment or paths.
  try {
    const fixture = JSON.parse(await readFile(path.join(
      path.dirname(script), '..', '..', 'artifacts', 'nc10', 'project-smoke-plan.json'), 'utf8'));
    if (path.isAbsolute(fixture.codex)) {
      codexVersion = execFileSync(fixture.codex, ['--version'], {
        encoding: 'utf8', windowsHide: true, timeout: 5000,
      }).trim().slice(0, 100);
    }
  } catch { /* An absent/troubled official CLI stays NOT_RUN. */ }
  process.stdout.write(JSON.stringify({
    ...summary,
    sourceSha: verified.sourceSha,
    binarySha256: verified.binarySha256,
    cargoLockSha256: verified.cargoLockSha256,
    buildReceiptVerified: verified.buildReceiptVerified,
    codexVersion,
  }, null, 2) + '\n');
}
