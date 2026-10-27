import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, readFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

test('NC-10 Windows sampler includes its live root and descendants, excludes an unrelated sibling',
  { skip: process.platform !== 'win32', timeout: 30000 }, async () => {
    const folder = await mkdtemp(path.join(os.tmpdir(), 'ultima-nc10-sampler-'));
    const rootCode = "const c=require('node:child_process').spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:'ignore',windowsHide:true});console.log(c.pid);setInterval(()=>{},1000)";
    const root = spawn(process.execPath, ['-e', rootCode], { stdio: ['ignore', 'pipe', 'ignore'], windowsHide: true });
    const unrelated = spawn(process.execPath, ['-e', 'setInterval(()=>{},1000)'], { stdio: 'ignore', windowsHide: true });
    try {
      await Promise.all([once(root, 'spawn'), once(unrelated, 'spawn')]);
      const [bytes] = await once(root.stdout, 'data');
      const descendantPid = Number(bytes.toString().trim());
      const worker = spawn('powershell.exe', ['-NoProfile', '-NonInteractive', '-File', 'scripts/nc-10-sample.ps1',
        '-RootProcessId', String(root.pid), '-OutputFile', path.join(folder, 'tree.jsonl'),
        '-StopFile', path.join(folder, 'stop'), '-DurationMs', '8000'], { windowsHide: true, stdio: ['ignore', 'ignore', 'pipe'] });
      let stderrBytes = 0;
      worker.stderr.on('data', bytes => { stderrBytes += bytes.length; });
      const [code] = await once(worker, 'exit');
      assert.equal(code, 0); assert.equal(stderrBytes, 0);
      const samples = (await readFile(path.join(folder, 'tree.jsonl'), 'utf8')).trim().split('\n').map(JSON.parse);
      assert.ok(samples.length >= 2);
      for (const sample of samples) {
        assert.ok(sample.rssBytes > 0);
        assert.ok(sample.privateCommitBytes > 0);
        assert.ok(sample.processCount >= 1);
        assert.ok(sample.tree.some(process => process.pid === root.pid));
        assert.ok(sample.tree.some(process => process.pid === descendantPid));
        assert.ok(sample.tree.every(process => process.pid !== unrelated.pid));
        assert.equal(sample.rssBytes, sample.tree.reduce((n, process) => n + process.rssBytes, 0));
        assert.equal(sample.privateCommitBytes, sample.tree.reduce((n, process) => n + process.privateCommitBytes, 0));
        assert.ok(sample.tree.every(process =>
          typeof process.privateCommitBytes === 'number' && process.privateCommitBytes >= 0 &&
          process.role === 'other' && !('commandLine' in process)));
      }
    } finally {
      if (root.exitCode === null) execFileSync('taskkill.exe', ['/PID', String(root.pid), '/T', '/F'], { stdio: 'ignore', windowsHide: true });
      unrelated.kill();
    }
  });

test('NC-10 controlled crash refuses wrong executable and terminates only the expected direct child',
  { skip: process.platform !== 'win32', timeout: 30000 }, async () => {
    const rootCode = "const c=require('node:child_process').spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:'ignore',windowsHide:true});console.log(c.pid);setInterval(()=>{},1000)";
    const root = spawn(process.execPath, ['-e', rootCode], { stdio: ['ignore', 'pipe', 'ignore'], windowsHide: true });
    const unrelated = spawn(process.execPath, ['-e', 'setInterval(()=>{},1000)'], { stdio: 'ignore', windowsHide: true });
    try {
      const [bytes] = await once(root.stdout, 'data'); const descendantPid = Number(bytes.toString().trim());
      const args = ['-NoProfile', '-NonInteractive', '-File', 'scripts/nc-10-owned-crash.ps1', '-RootProcessId', String(root.pid), '-ExpectedExecutable'];
      assert.throws(() => execFileSync('powershell.exe', [...args, path.join(process.env.SystemRoot, 'System32', 'cmd.exe')], { stdio: 'ignore', windowsHide: true }));
      process.kill(descendantPid, 0); process.kill(unrelated.pid, 0);
      const receipt = JSON.parse(execFileSync('powershell.exe', [...args, process.execPath], { encoding: 'utf8', windowsHide: true }));
      assert.equal(receipt.ownedRuntimeTerminated, true); assert.equal(receipt.pid, descendantPid);
      process.kill(root.pid, 0); process.kill(unrelated.pid, 0);
      assert.throws(() => process.kill(descendantPid, 0));
    } finally {
      if (root.exitCode === null) execFileSync('taskkill.exe', ['/PID', String(root.pid), '/T', '/F'], { stdio: 'ignore', windowsHide: true });
      unrelated.kill();
    }
  });
