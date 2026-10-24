// NC-11 official stable release discovery and fail-closed runtime version policy.
const UPSTREAM = "https://github.com/openai/codex";
const STABLE = /^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/;
export function parseStableVersion(value) {
  if (typeof value !== "string" || !STABLE.test(value) || value.trim() !== value) return null;
  const parts = value.split(".").map(Number);
  return parts.every(Number.isSafeInteger) ? parts : null;
}
export function compareStable(a, b) {
  const x = parseStableVersion(a), y = parseStableVersion(b);
  if (!x || !y) throw new Error("INVALID_STABLE_VERSION");
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] > y[i] ? 1 : -1;
  return 0;
}
export function validateMatrix(matrix) {
  if (!matrix || typeof matrix !== "object" || Array.isArray(matrix) || matrix.schemaVersion !== 1 ||
    matrix.source !== UPSTREAM || matrix.transport !== "stdio-jsonl" || matrix.stableTagPrefix !== "rust-v" ||
    !parseStableVersion(matrix.pinnedVersion) || !Array.isArray(matrix.versions) ||
    matrix.versions.length < 2 || matrix.versions.length > 10 ||
    !Array.isArray(matrix.stableMethods) || !Array.isArray(matrix.approvalMethods)) {
    throw new Error("INVALID_COMPAT_MATRIX");
  }
  for (const method of ["initialize", "initialized", "model/list", "thread/start", "thread/resume", "turn/start", "turn/interrupt"]) {
    if (!matrix.stableMethods.includes(method)) throw new Error("MISSING_REQUIRED_METHOD");
  }
  for (const method of ["item/commandExecution/requestApproval", "item/fileChange/requestApproval"]) {
    if (!matrix.approvalMethods.includes(method)) throw new Error("MISSING_REQUIRED_APPROVAL");
  }
  const seen = new Set();
  for (const item of matrix.versions) {
    if (!item || !parseStableVersion(item.version) || seen.has(item.version) ||
      item.tag !== "rust-v" + item.version || item.release !== UPSTREAM + "/releases/tag/" + item.tag ||
      !["supported", "candidate"].includes(item.state)) throw new Error("INVALID_MATRIX_ENTRY");
    seen.add(item.version);
  }
  if (!seen.has(matrix.pinnedVersion) ||
      matrix.versions.find((v) => v.version === matrix.pinnedVersion).state !== "supported" ||
      matrix.versions.filter((v) => v.state === "supported").length !== 1)
    throw new Error("UNSAFE_SUPPORTED_VERSION");
  return matrix;
}
export function compatibilityFor(matrix, version) {
  validateMatrix(matrix);
  const item = matrix.versions.find((v) => v.version === version);
  if (!item || !parseStableVersion(version)) return { state: "unverified", canStart: false, reason: "VERSION_NOT_QUALIFIED" };
  if (item.state !== "supported") return { state: item.state, canStart: false, reason: "REQUIRES_PROMOTION" };
  return { state: "supported", canStart: true, reason: "PINNED_CONTRACT" };
}
export function latestStableRelease(releases) {
  if (!Array.isArray(releases)) throw new Error("INVALID_RELEASE_RESPONSE");
  const verified = releases.filter((r) => r && r.draft === false && r.prerelease === false &&
    typeof r.tag_name === "string" && r.tag_name.startsWith("rust-v") &&
    parseStableVersion(r.tag_name.slice(6)) &&
    r.html_url === UPSTREAM + "/releases/tag/" + r.tag_name &&
    typeof r.published_at === "string" && !Number.isNaN(Date.parse(r.published_at)));
  if (verified.length === 0) throw new Error("NO_VERIFIED_STABLE_RELEASE");
  verified.sort((a,b) => compareStable(b.tag_name.slice(6), a.tag_name.slice(6)));
  const r = verified[0];
  return { version: r.tag_name.slice(6), tag: r.tag_name, published: r.published_at, url: r.html_url };
}
export function releaseDrift(matrix, releases) {
  validateMatrix(matrix);
  const latest = latestStableRelease(releases);
  return { latest, pinned: matrix.pinnedVersion, newer: compareStable(latest.version, matrix.pinnedVersion) > 0 };
}
