import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { inspectDiagnostic } from '../verify-latency.mjs';

const measure = (n = 80, p95Ms = 20) =>
  ({ samples: n, overflow: 0, p50Ms: 10, p95Ms, maxMs: p95Ms });
const receipt = () => ({
  kind: 'NC10B_GLOW_PAINT_DIAGNOSTIC',
  version: 1,
  sourceSha: 'a'.repeat(40),
  stage: 'OpenGL callback before buffer swap; not compositor presentation',
  presentationVerified: false,
  acceptance: 'NOT_RUN: independent Windows screen-presentation receipt required',
  streamReceivedToPaintCallback: measure(),
  rawKeyToComposerPaintCallback: measure(),
  rawKeyDuringActiveTurnToPaintCallback: measure(),
  processMainToFirstComposerPaintCallback: measure(1, 20),
});

test('healthy paint diagnostics cannot claim compositor acceptance', () => {
  const result = inspectDiagnostic([receipt(), receipt(), receipt()]);
  assert.equal(result.status, 'DIAGNOSTIC_ONLY__NATIVE_PRESENTATION_NOT_RUN');
  assert.equal(result.actualPixelsPresented, 'NOT_RUN');
  assert.equal(result.metrics.streamReceivedToPaintCallback.totalSamples, 240);
  assert.equal(result.metrics.rawKeyToComposerPaintCallback.allRunP95WithinDiagnosticBudget, true);
});

test('fails closed on missing, truncated, deceptive or nonnumeric receipts', () => {
  assert.throws(() => inspectDiagnostic([receipt()]), /3 independent/);
  for (const mutate of [
    x => { x.presentationVerified = true; },
    x => { x.sourceSha = 'b'.repeat(40); },
    x => { x.acceptance = 'PASS'; },
    x => { x.streamReceivedToPaintCallback.samples = 0; },
    x => { x.streamReceivedToPaintCallback.samples = 1; },
    x => { x.streamReceivedToPaintCallback.overflow = 1; },
    x => { x.rawKeyToComposerPaintCallback.p95Ms = '10'; },
    x => { x.rawKeyDuringActiveTurnToPaintCallback.samples = 0; },
    x => { x.processMainToFirstComposerPaintCallback.samples = 3; },
  ]) {
    const altered = receipt();
    mutate(altered);
    assert.throws(() => inspectDiagnostic([receipt(), altered, receipt()]));
  }
  assert.throws(() => inspectDiagnostic([receipt(), receipt(), receipt()], 'b'.repeat(40)),
    /Source SHA missing or mismatched/);
});

test('diagnostic budget exceedance is reported as failed, not coerced to PASS', () => {
  const bad = receipt();
  bad.rawKeyToComposerPaintCallback.p95Ms = 80;
  bad.rawKeyToComposerPaintCallback.maxMs = 80;
  const result = inspectDiagnostic([receipt(), bad, receipt()]);
  assert.equal(result.metrics.rawKeyToComposerPaintCallback.allRunP95WithinDiagnosticBudget, false);
  assert.equal(result.actualPixelsPresented, 'NOT_RUN');
});
