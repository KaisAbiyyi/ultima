import test from "node:test";
import assert from "node:assert/strict";
import { UPSTREAM_VERSION, MAX_LINE_BYTES, decodeLine, validateClientRequest, inspectInbound, approvalReply, rejectUnknownServerRequest, Handshake } from "../protocol/app-server-v0.164.0.mjs";

const sample = (id = 0) => ({ id, result: { userAgent: "codex/0.164.0", platformFamily: "unix", platformOs: "linux", codexHome: "/tmp/codex" } });
const commandApproval = (id = 42) => ({ id, method: "item/commandExecution/requestApproval", params: { threadId: "thr_a", turnId: "turn_a", itemId: "item_a", startedAtMs: 123, command: "git status", cwd: "/tmp" } });
const fileApproval = (id = 43) => ({ id, method: "item/fileChange/requestApproval", params: { threadId: "thr_a", turnId: "turn_a", itemId: "item_b", startedAtMs: 124 } });

test("upstream release is pinned", () => assert.equal(UPSTREAM_VERSION, "0.164.0"));
test("handshake requires response and initialized ack before other methods", () => {
  const h = new Handshake();
  assert.throws(() => h.request(1, "model/list", {}), /NOT_INITIALIZED/);
  assert.throws(() => h.initialized(), /INITIALIZATION_NOT_CONFIRMED/);
  assert.deepEqual(h.initialize(0).params.clientInfo, { name: "ultima", title: "Ultima", version: "0.1.0" });
  assert.throws(() => h.initialize(1), /HANDSHAKE_ALREADY_STARTED/);
  assert.throws(() => h.receive(sample(1)), /INVALID_INITIALIZE_RESPONSE/);
  h.receive(sample());
  assert.deepEqual(h.initialized(), { method: "initialized", params: {} });
  assert.equal(h.state, "ready");
  assert.throws(() => h.initialized(), /INITIALIZATION_NOT_CONFIRMED/);
  assert.throws(() => h.request(1, "initialize", {}), /NOT_INITIALIZED/);
  assert.equal(h.request(1, "model/list", { limit: 10 }).method, "model/list");
  assert.equal(h.request(2, "thread/start", {}).method, "thread/start");
  assert.equal(h.request(3, "thread/resume", { threadId: "thr_a", excludeTurns: true }).method, "thread/resume");
  assert.equal(h.request(4, "turn/start", { threadId: "thr_a", input: [{ type: "text", text: "hello" }] }).method, "turn/start");
  assert.equal(h.request(5, "turn/interrupt", { threadId: "thr_a", turnId: "turn_a" }).method, "turn/interrupt");
});
test("rejects invalid handshake without transitioning to ready", () => {
  const h = new Handshake();
  h.initialize(0);
  assert.throws(() => h.receive({ id: 0, error: { code: -32000, message: "Not initialized" } }), /INITIALIZE_FAILED/);
  assert.equal(h.state, "pending");
});
test("validates required request fields and sandbox/profile exclusion", () => {
  const mk = (method, params) => ({ method, params, id: 0 });
  assert.equal(validateClientRequest(mk("model/list", { limit: -2 })), false);
  assert.equal(validateClientRequest(mk("thread/resume", {})), false);
  assert.equal(validateClientRequest(mk("turn/start", { threadId: "thr", input: [{ type: "file", path: "/tmp" }] })), false);
  assert.equal(validateClientRequest(mk("turn/interrupt", { threadId: "thr" })), false);
  assert.equal(validateClientRequest(mk("thread/start", { sandbox: "readOnly", permissions: "unsafe" })), false);
  assert.equal(validateClientRequest(mk("unknown", {})), false);
});
test("parses JSONL safely and bounds memory per line", () => {
  assert.deepEqual(decodeLine('{"id":0,"result":{}}'), { id: 0, result: {} });
  assert.throws(() => decodeLine("{bad"), /INVALID_JSONL/);
  assert.throws(() => decodeLine("[]"), /INVALID_JSONL/);
  assert.throws(() => decodeLine("x".repeat(MAX_LINE_BYTES + 1)), /OVERSIZED/);
});
test("unknown notifications are ignored without interrupting known event replay", () => {
  const events = [
    { method: "thread/started", params: { thread: { id: "thr_a" } } },
    { method: "future/notification", params: { huge: false } },
    { method: "turn/started", params: { turn: { id: "turn_a" } } },
    { method: "item/agentMessage/delta", params: { delta: "Hello" } },
    { method: "turn/completed", params: { turn: { id: "turn_a", status: "completed" } } }
  ];
  assert.deepEqual(events.map((e) => inspectInbound(e).known), [true, false, true, true, true]);
});
test("command and file approvals round-trip with explicit opt-in only", () => {
  const command = commandApproval();
  const file = fileApproval();
  assert.equal(inspectInbound(command).kind, "approval");
  assert.equal(inspectInbound(file).kind, "approval");
  assert.deepEqual(approvalReply(command), { id: 42, result: { decision: "decline" } });
  assert.deepEqual(approvalReply(file, "cancel"), { id: 43, result: { decision: "cancel" } });
  assert.throws(() => approvalReply(command, "accept"), /EXPLICIT_USER_APPROVAL_REQUIRED/);
  assert.deepEqual(approvalReply(command, "accept", true), { id: 42, result: { decision: "accept" } });
  assert.throws(() => approvalReply(command, "acceptForSession", true), /UNSUPPORTED/);
});
test("approval ID, payload identity and timestamps must validate", () => {
  assert.equal(inspectInbound(commandApproval(0)).kind, "approval");
  assert.equal(inspectInbound({ ...commandApproval(), params: { ...commandApproval().params, startedAtMs: -1 } }).kind, "invalid-approval");
  assert.equal(inspectInbound({ ...commandApproval(), params: { ...commandApproval().params, threadId: "" } }).kind, "invalid-approval");
  assert.equal(inspectInbound({ ...commandApproval(), id: null }).kind, "invalid");
  assert.throws(() => approvalReply({ ...commandApproval(), params: {} }), /INVALID_APPROVAL_REQUEST/);
});
test("unknown server requests fail closed, not auto-approved", () => {
  const request = { id: 75, method: "future/requestPrivilege", params: {} };
  assert.equal(inspectInbound(request).kind, "unsupported-server-request");
  assert.deepEqual(rejectUnknownServerRequest(request), { id: 75, error: { code: -32601, message: "Unsupported server request" } });
  assert.throws(() => approvalReply(request, "accept", true), /INVALID_APPROVAL_REQUEST/);
});
test("rejects invalid ambiguous envelopes and recognizes RPC errors", () => {
  assert.equal(inspectInbound({ id: 1, result: {}, error: {} }).kind, "invalid");
  assert.equal(inspectInbound({ method: "turn/started", id: 2, result: {} }).kind, "invalid");
  assert.equal(inspectInbound({ id: 2, error: { code: -32601, message: "Method not found" } }).ok, false);
});
