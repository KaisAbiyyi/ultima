// NC-01: stable App Server v2 contract probe, not the full NC-05 adapter.
// Source: openai/codex rust-v0.164.0 commit 6aaac694803004a63c6f393d153c96d22521a6b9.
export const UPSTREAM_VERSION = "0.164.0";
export const MAX_LINE_BYTES = 1024 * 1024;
const requestMethods = new Set(["initialize", "model/list", "thread/start", "thread/resume", "turn/start", "turn/interrupt"]);
const approvals = new Set(["item/commandExecution/requestApproval", "item/fileChange/requestApproval"]);
const nonEmpty = (value) => typeof value === "string" && value.length > 0;
const object = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
const identifier = (value) => (typeof value === "string" && value.length > 0) || (Number.isSafeInteger(value) && value >= 0);
const requireFields = (value, fields) => object(value) && fields.every((key) => nonEmpty(value[key]));

export function decodeLine(line) {
  if (typeof line !== "string" || Buffer.byteLength(line, "utf8") > MAX_LINE_BYTES) {
    throw new Error("INVALID_OR_OVERSIZED_JSONL");
  }
  try {
    const value = JSON.parse(line);
    if (!object(value)) throw new Error("Envelope must be an object");
    return value;
  } catch {
    throw new Error("INVALID_JSONL");
  }
}

export function validateClientRequest(request) {
  if (!object(request) || !identifier(request.id) || !requestMethods.has(request.method) || !object(request.params)) return false;
  const p = request.params;
  switch (request.method) {
    case "initialize": return requireFields(p.clientInfo, ["name", "version"]) && (p.clientInfo.title === undefined || typeof p.clientInfo.title === "string");
    case "model/list": return (p.cursor == null || typeof p.cursor === "string") && (p.limit == null || (Number.isInteger(p.limit) && p.limit > 0)) && (p.includeHidden == null || typeof p.includeHidden === "boolean");
    case "thread/start": return (p.cwd === undefined || nonEmpty(p.cwd)) && (p.model === undefined || nonEmpty(p.model)) && !(p.sandbox != null && p.permissions != null);
    case "thread/resume": return nonEmpty(p.threadId) && (p.excludeTurns == null || typeof p.excludeTurns === "boolean");
    case "turn/start": return nonEmpty(p.threadId) && Array.isArray(p.input) && p.input.every((i) => object(i) && i.type === "text" && typeof i.text === "string");
    case "turn/interrupt": return requireFields(p, ["threadId", "turnId"]);
    default: return false;
  }
}

export function inspectInbound(message) {
  if (!object(message)) return { kind: "invalid" };
  if ("method" in message) {
    if (!nonEmpty(message.method) || "result" in message || "error" in message) return { kind: "invalid" };
    if (!("id" in message)) return { kind: "notification", method: message.method, known: [
      "thread/started", "turn/started", "turn/completed", "item/started",
      "item/completed", "item/agentMessage/delta", "serverRequest/resolved"
    ].includes(message.method) };
    if (!identifier(message.id)) return { kind: "invalid" };
    if (!approvals.has(message.method)) return { kind: "unsupported-server-request", id: message.id, method: message.method };
    if (!requireFields(message.params, ["threadId", "turnId", "itemId"]) ||
        !Number.isSafeInteger(message.params.startedAtMs) || message.params.startedAtMs < 0 ||
        (message.params.approvalId != null && !nonEmpty(message.params.approvalId))) {
      return { kind: "invalid-approval", id: message.id };
    }
    return { kind: "approval", id: message.id, method: message.method, params: message.params };
  }
  if (!identifier(message.id) || (("result" in message) === ("error" in message))) return { kind: "invalid" };
  if ("error" in message && (!object(message.error) || !Number.isInteger(message.error.code) || typeof message.error.message !== "string")) return { kind: "invalid" };
  return { kind: "response", id: message.id, ok: "result" in message };
}

export function approvalReply(message, decision = "decline", explicitlyApproved = false) {
  const inspected = inspectInbound(message);
  if (inspected.kind !== "approval") throw new Error("INVALID_APPROVAL_REQUEST");
  if (!["accept", "decline", "cancel"].includes(decision)) throw new Error("UNSUPPORTED_APPROVAL_DECISION");
  if (decision === "accept" && !explicitlyApproved) throw new Error("EXPLICIT_USER_APPROVAL_REQUIRED");
  return { id: message.id, result: { decision } };
}

export function rejectUnknownServerRequest(message) {
  const inspected = inspectInbound(message);
  if (inspected.kind !== "unsupported-server-request") throw new Error("NOT_AN_UNKNOWN_REQUEST");
  return { id: message.id, error: { code: -32601, message: "Unsupported server request" } };
}

export class Handshake {
  #state = "fresh";
  #requestId;
  get state() { return this.#state; }
  initialize(id = 0) {
    if (this.#state !== "fresh" || !identifier(id)) throw new Error("HANDSHAKE_ALREADY_STARTED");
    this.#state = "pending";
    this.#requestId = id;
    return { id, method: "initialize", params: { clientInfo: { name: "ultima", title: "Ultima", version: "0.1.0" }, capabilities: { experimentalApi: false } } };
  }
  receive(response) {
    if (this.#state !== "pending" || response?.id !== this.#requestId) throw new Error("INVALID_INITIALIZE_RESPONSE");
    const inspected = inspectInbound(response);
    if (inspected.kind !== "response" || !inspected.ok || !requireFields(response.result, ["userAgent", "platformFamily", "platformOs"])) throw new Error("INITIALIZE_FAILED");
    this.#state = "acknowledgeable";
  }
  initialized() {
    if (this.#state !== "acknowledgeable") throw new Error("INITIALIZATION_NOT_CONFIRMED");
    this.#state = "ready";
    return { method: "initialized", params: {} };
  }
  request(id, method, params = {}) {
    if (this.#state !== "ready" || method === "initialize") throw new Error("NOT_INITIALIZED");
    const request = { id, method, params };
    if (!validateClientRequest(request)) throw new Error("INVALID_CLIENT_REQUEST");
    return request;
  }
}
