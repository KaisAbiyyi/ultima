import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, writeFile, mkdir } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { createOutputDirectory, requireStressProjects, fingerprintFixtures } from '../scripts/nc-10-support.mjs';

test('NC-10 stress requires three distinct canonical fixture projects', () => {
  assert.throws(() => requireStressProjects(['D:\\a']));
  assert.throws(() => requireStressProjects(['D:\\a', 'd:\\a', 'D:\\b']));
  assert.doesNotThrow(() => requireStressProjects(['D:\\a', 'D:\\b', 'D:\\c']));
});
test('NC-10 creates a fresh nested output and refuses to replace an existing run', async () => {
  const folder = await mkdtemp(path.join(os.tmpdir(), 'ultima-nc10-output-'));
  const output = path.join(folder, 'new-parent', 'new-run');
  await createOutputDirectory(output);
  await writeFile(path.join(output, 'receipt.json'), 'preserve');
  await assert.rejects(createOutputDirectory(output));
  assert.equal(await readFile(path.join(output, 'receipt.json'), 'utf8'), 'preserve');
});
test('NC-10 fingerprints actual bounded fixture content and rejects unrelated files', async () => {
  const folder = await mkdtemp(path.join(os.tmpdir(), 'ultima-nc10-fixture-'));
  await writeFile(path.join(folder, 'fixture.txt'), 'one');
  const one = await fingerprintFixtures([folder]);
  await writeFile(path.join(folder, 'fixture.txt'), 'two');
  assert.notEqual(one, await fingerprintFixtures([folder]));
  await writeFile(path.join(folder, 'auth.json'), 'never read');
  await assert.rejects(fingerprintFixtures([folder]));
});
