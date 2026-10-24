// Check that the pinned binary generated non-empty official schema and TS outputs.
import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";
const roots = process.argv.slice(2);
if (roots.length !== 2) throw new Error("Expected JSON Schema and TypeScript output directories");
async function walk(path, out = []) {
  for (const e of await readdir(path, { withFileTypes: true })) {
    const full = join(path, e.name);
    if (e.isDirectory()) await walk(full, out);
    else if (e.isFile()) out.push(full);
  }
  return out;
}
const jsonFiles = (await walk(roots[0])).filter((x) => x.endsWith(".json"));
const tsFiles = (await walk(roots[1])).filter((x) => x.endsWith(".ts"));
if (jsonFiles.length < 5 || tsFiles.length < 5) throw new Error("Incomplete upstream schema/type export");
const jsonNames = jsonFiles.map((x) => x.toLowerCase()).join("\n");
const tsNames = tsFiles.map((x) => x.toLowerCase()).join("\n");
for (const name of ["initializeparams", "modellistparams", "threadstartparams", "turnstartparams", "commandexecutionrequestapproval"]) {
  if (!jsonNames.includes(name) || !tsNames.includes(name)) throw new Error("Missing generated schema/type: " + name);
}
for (const path of jsonFiles.slice(0, 10)) JSON.parse(await readFile(path, "utf8"));
process.stdout.write("PASS: pinned official schema exports JSON=" + jsonFiles.length + " TypeScript=" + tsFiles.length + "\n");
