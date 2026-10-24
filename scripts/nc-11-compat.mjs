#!/usr/bin/env node
// Cloud-only read-only Codex release/policy CLI; no installation or user config access.
import { readFile, appendFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { validateMatrix, compatibilityFor, releaseDrift } from "../protocol/compatibility.mjs";
const matrix = validateMatrix(JSON.parse(await readFile(new URL("../protocol/compat-matrix.json", import.meta.url), "utf8")));
const [command, ...args] = process.argv.slice(2);
async function note(value) {
  console.log(value);
  if (process.env.GITHUB_STEP_SUMMARY) await appendFile(process.env.GITHUB_STEP_SUMMARY, value + "\n");
}
async function getOfficialReleases() {
  const headers = { Accept: "application/vnd.github+json", "User-Agent": "ultima-nc11-compat" };
  if (process.env.GITHUB_TOKEN) headers.Authorization = "Bearer " + process.env.GITHUB_TOKEN;
  // The official /latest endpoint excludes prereleases and avoids the enormous asset
  // list returned by /releases?per_page=100 (which can cause GitHub HTTP 504).
  // Still verify the signed-off GitHub URL/tag/flags in latestStableRelease.
  const url = "https://api.github.com/repos/openai/codex/releases/latest";
  for (let attempt = 1; attempt <= 3; attempt++) {
    try {
      const response = await fetch(url, {
        headers, redirect: "error", signal: AbortSignal.timeout(15000)
      });
      if (!response.ok) throw new Error("UPSTREAM_FETCH_HTTP_" + response.status);
      const release = await response.json();
      if (!release || typeof release !== "object" || Array.isArray(release))
        throw new Error("INVALID_RELEASE_RESPONSE");
      return [release];
    } catch (error) {
      if (attempt === 3) throw error;
      // Bounded retry for transient GitHub/network errors; never report a stale PASS.
      await new Promise((resolve) => setTimeout(resolve, attempt * 1500));
    }
  }
}
try {
  if (command === "matrix") {
    for (const row of matrix.versions) await note(row.version + ": " + row.state + " | " + row.release);
  } else if (command === "verify-version") {
    const [expected, output] = args;
    const match = /(?:^|\s)([0-9]+\.[0-9]+\.[0-9]+)(?=\s|$)/.exec(output || "");
    if (!matrix.versions.some(v => v.version === expected) || !match || match[1] !== expected)
      throw new Error("INSTALLED_VERSION_MISMATCH");
    await note("PASS: installed official Codex CLI version " + expected);
  } else if (command === "gate-runtime") {
    const result = compatibilityFor(matrix, args[0]);
    await note("Runtime " + String(args[0]) + ": " + result.state + " (" + result.reason + ")");
    if (!result.canStart) process.exitCode = 1;
  } else if (command === "check-latest") {
    const result = releaseDrift(matrix, await getOfficialReleases());
    await note("## NC-11 upstream release monitor\nLatest stable: [" + result.latest.tag + "](" +
      result.latest.url + "), published " + result.latest.published + "; pinned contract: " + result.pinned);
    if (result.newer) {
      await note("FAIL: new stable is UNQUALIFIED. Regenerate schemas, compare protocol diff and run full contract CI before promotion.");
      process.exitCode = 1;
    } else await note("PASS: no newer stable release than pinned contract.");
  } else if (command === "smoke-installed") {
    const version = args[0];
    if (!matrix.versions.some(v => v.version === version)) throw new Error("UNKNOWN_MATRIX_VERSION");
    execFileSync(process.execPath, [fileURLToPath(new URL("./nc-01-smoke.mjs", import.meta.url))], {
      timeout: 30000, stdio: "inherit", env: process.env
    });
    await note("PASS: " + version + " real unauthenticated initialize/initialized/model-list smoke");
  } else throw new Error("Usage: matrix | verify-version <expected> <output> | gate-runtime <version> | check-latest | smoke-installed <version>");
} catch (error) {
  console.error("FAIL NC-11: " + error.message);
  process.exitCode = 1;
}
