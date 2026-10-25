import { execFileSync } from 'node:child_process';
import { once } from 'node:events';

/** Only accepts a ChildProcess handle created by the runner, never a plan PID. */
export async function closeOwned(child) {
  if (!child.pid || child.exitCode !== null || child.signalCode !== null) return;
  // Hold the original child handle and await its exit event. A native exit may
  // already have happened while a synchronous taskkill blocked Node callbacks.
  const completion = once(child, 'exit').then(() => true, () => false);
  try { execFileSync('taskkill.exe', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore', windowsHide: true }); }
  catch { /* A racing native exit is confirmed below, never guessed from PID. */ }
  let timer;
  const confirmed = await Promise.race([completion, new Promise(resolve => { timer = setTimeout(() => resolve(false), 5000); })]);
  clearTimeout(timer);
  if (!confirmed || (child.exitCode === null && child.signalCode === null)) throw Error('Owned process shutdown unconfirmed');
}
