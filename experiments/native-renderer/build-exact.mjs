/** Reproducible provenance for the local Windows native-renderer experiment. */
import { spawnSync, execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(root, '../..');
const exe = path.join(root, 'target', 'release', 'ultima-native-renderer-spike.exe');
const lock = path.join(root, 'Cargo.lock');
const receiptPath = path.join(root, 'target', 'release', 'nc10-build-receipt.json');
const hash = data => createHash('sha256').update(data).digest('hex');
const git = args => execFileSync('git', args, { cwd: repo, encoding: 'utf8' }).trim();
const sourceClean = () => !git(['status', '--porcelain=v1', '--untracked-files=all']);

export async function verifyExactBuild() {
  if (!sourceClean()) throw Error('Source is dirty: commit every change before qualification');
  const head = git(['rev-parse', 'HEAD']);
  const binarySha256 = hash(await readFile(exe));
  const cargoLockSha256 = hash(await readFile(lock));
  const receipt = JSON.parse(await readFile(receiptPath, 'utf8'));
  if (receipt.kind !== 'NC10_NATIVE_EXACT_SOURCE_BUILD' || receipt.sourceSha !== head ||
      receipt.binarySha256 !== binarySha256 || receipt.cargoLockSha256 !== cargoLockSha256) {
    throw Error('Native binary does not match verified HEAD build receipt; run build-exact.mjs');
  }
  return { sourceSha: head, binarySha256, cargoLockSha256, buildReceiptVerified: true };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (!sourceClean()) throw Error('Refusing an exact-source build with uncommitted changes');
  const head = git(['rev-parse', 'HEAD']);
  const cargo = process.env.CARGO || (
    process.platform === 'win32'
      ? path.join(process.env.USERPROFILE ?? '', '.cargo', 'bin', 'cargo.exe')
      : 'cargo'
  );
  const build = spawnSync(cargo, [
    'build', '--release', '--locked', '--manifest-path', path.join(root, 'Cargo.toml'),
  ], { cwd: repo, stdio: 'inherit', timeout: 900_000 });
  if (build.error) throw build.error;
  if (build.status !== 0) throw Error('Cargo native release build failed');
  if (!sourceClean() || git(['rev-parse', 'HEAD']) !== head) {
    throw Error('Sources changed while building; do not use this executable');
  }
  const receipt = {
    kind: 'NC10_NATIVE_EXACT_SOURCE_BUILD',
    sourceSha: head,
    binarySha256: hash(await readFile(exe)),
    cargoLockSha256: hash(await readFile(lock)),
    compiler: 'cargo build --release --locked',
  };
  await writeFile(receiptPath, JSON.stringify(receipt, null, 2) + '\n', { flag: 'w' });
  await verifyExactBuild();
  console.log(JSON.stringify(receipt));
}
