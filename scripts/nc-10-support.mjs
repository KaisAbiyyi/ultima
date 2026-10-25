import { mkdir, readFile, readdir, stat } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';

export async function createOutputDirectory(output) {
  await mkdir(path.dirname(output), { recursive: true });
  await mkdir(output); // Existing runs must never be replaced.
}
export function requireStressProjects(projects) {
  if (projects.length !== 3 || new Set(projects.map(p => path.win32.normalize(p).toLowerCase())).size !== 3) {
    throw Error('Stress requires three distinct canonical fixture projects');
  }
}
export async function fingerprintFixtures(projects) {
  const hash = createHash('sha256');
  for (const project of projects) {
    const entries = await readdir(project, { withFileTypes: true });
    if (entries.length !== 1 || entries[0].name !== 'fixture.txt' || !entries[0].isFile() || entries[0].isSymbolicLink()) {
      throw Error('Use dedicated fixtures containing only fixture.txt; user projects and credentials are never scanned');
    }
    const file = path.join(project, 'fixture.txt');
    if ((await stat(file)).size > 1024 * 1024) throw Error('Fixture file exceeds 1 MiB');
    const data = await readFile(file);
    hash.update(String(data.length) + ':'); hash.update(data);
  }
  return hash.digest('hex');
}
