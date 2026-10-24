// Unauthenticated, non-destructive smoke against the real pinned Codex binary.
// This intentionally does NOT start a turn or run commands.
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { Handshake, decodeLine } from "../protocol/app-server-v0.164.0.mjs";

const handshake = new Handshake();
const proc = spawn("codex", ["app-server"], { stdio: ["pipe", "pipe", "pipe"], env: process.env });
let settled = false;
const deadline = setTimeout(() => fail("SMOKE_TIMEOUT"), 20000);
function finish(status, message) {
  if (settled) return;
  settled = true;
  clearTimeout(deadline);
  proc.kill();
  process.stdout.write(message + "\n");
  process.exitCode = status;
}
function fail(message) { finish(1, message); }
function send(message) { if (!settled) proc.stdin.write(JSON.stringify(message) + "\n"); }
proc.on("error", (error) => fail("SPAWN_ERROR: " + error.code));
proc.on("exit", (code) => { if (!settled) fail("UNEXPECTED_EXIT: " + code); });
createInterface({ input: proc.stdout, crlfDelay: Infinity }).on("line", (line) => {
  if (settled) return;
  try {
    const msg = decodeLine(line);
    if (msg.id === 0) {
      handshake.receive(msg);
      send(handshake.initialized());
      send(handshake.request(1, "model/list", { limit: 3 }));
    } else if (msg.id === 1) {
      if (msg.error) fail("MODEL_LIST_ERROR: " + msg.error.code + " " + msg.error.message);
      else if (Array.isArray(msg.result?.data) && "nextCursor" in msg.result) {
        finish(0, "PASS: real App Server initialize/initialized/model-list contract");
      } else fail("INVALID_MODEL_LIST_RESPONSE");
    }
  } catch (error) { fail("INVALID_PROTOCOL_RESPONSE: " + error.message); }
});
send(handshake.initialize(0));
