import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { closeOwned } from '../scripts/nc-10-processes.mjs';

test('NC-10 cleanup confirms a native exit whose Node exit callback is still queued',
  { skip: process.platform !== 'win32', timeout: 10000 }, async () => {
    const child = spawn(process.execPath, ['-e', 'setTimeout(()=>process.exit(0),20)'], { stdio: 'ignore', windowsHide: true });
    await once(child, 'spawn');
    // Model the synchronous taskkill/browser-close boundary: the OS process
    // exits while Node cannot yet deliver its child exit callback.
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 1000);
    assert.equal(child.exitCode, null);
    await assert.doesNotReject(closeOwned(child));
    assert.notEqual(child.exitCode, null);
  });
test('NC-10 cleanup kills its own live child while leaving a sibling untouched',
  { skip: process.platform !== 'win32', timeout: 10000 }, async () => {
    const child = spawn(process.execPath, ['-e', 'setInterval(()=>{},1000)'], { stdio: 'ignore', windowsHide: true });
    const sibling = spawn(process.execPath, ['-e', 'setInterval(()=>{},1000)'], { stdio: 'ignore', windowsHide: true });
    await Promise.all([once(child, 'spawn'), once(sibling, 'spawn')]);
    try {
      await closeOwned(child);
      assert.notEqual(child.exitCode, null);
      process.kill(sibling.pid, 0);
    } finally { child.kill(); sibling.kill(); }
  });
