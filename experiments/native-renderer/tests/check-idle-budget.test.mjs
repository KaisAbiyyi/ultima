import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { compareReports } from '../check-idle-budget.mjs';

const base = {
  kind: 'NC10_RENDERER_ONLY_DIAGNOSTIC_NOT_PRODUCT_ACCEPTANCE',
  sourceDirty: false,
  sourceSha: 'a'.repeat(40),
  binarySha256: 'b'.repeat(64),
  durationMs: 120_000,
  warmupMs: 60_000,
};
function report(mode, rssMiB) {
  return {
    ...base,
    mode,
    summary: { rssMiBP95AcrossRuns: rssMiB },
    samples: [1, 2, 3].map(run => ({
      run,
      count: 48,
      attributableCodexSamples: 48,
      processNames: mode === 'cli' ? ['codex', 'conhost'] : ['ultima-native-renderer-spike', 'codex', 'conhost'],
      rssMiB: { p95: rssMiB },
    })),
  };
}
describe('read-only prototype RAM budget: fail closed', () => {
  it('accepts a clean, exact binary match under unchanged 150MiB incremental RSS', () => {
    const result = compareReports(report('cli', 70), report('connected', 210));
    assert.equal(result.status, 'PASS');
    assert.equal(result.additionalRssMiB, 140);
    assert.equal(result.headroomMiB, 10);
    assert.match(result.scope, /NOT_PRODUCT_ACCEPTANCE/);
  });
  it('fails when the design regresses above the 150MiB budget', () => {
    const result = compareReports(report('cli', 70), report('connected', 221));
    assert.equal(result.status, 'FAIL');
    assert.equal(result.headroomMiB, -1);
  });
  it('rejects dirty sources, changed binaries and absent engine samples', () => {
    const cli = report('cli', 70);
    assert.throws(() => compareReports(cli, { ...report('connected', 180), sourceDirty: true }), /dirty source/);
    assert.throws(() => compareReports(cli, { ...report('connected', 180), binarySha256: 'c'.repeat(64) }), /different source or executable/);
    const missing = report('connected', 180);
    missing.samples[1].attributableCodexSamples = 47;
    assert.throws(() => compareReports(cli, missing), /Codex absent/);
  });
  it('rejects short/incomplete runs and wrong scope', () => {
    const cli = report('cli', 70);
    assert.throws(() => compareReports(cli, { ...report('connected', 180), durationMs: 45_000 }), /full settled/);
    assert.throws(() => compareReports(cli, { ...report('connected', 180), samples: [] }), /three runs/);
    assert.throws(() => compareReports(cli, { ...report('connected', 180), kind: 'NC10_PRODUCT_PASS' }), /scope/);
    assert.throws(
      () => compareReports(cli, { ...report('connected', 180), summary: { rssMiBP95AcrossRuns: 10 } }),
      /aggregate RSS conflicts/,
    );
  });
});
