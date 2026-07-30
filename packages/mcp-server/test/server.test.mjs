// Start the server as a real subprocess and talk JSON-RPC over stdio, so the
// bin, the transport wiring, and the root argument are covered — the tool
// tests bypass all three by calling handlers directly.

import { test } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { cp, mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const serverPath = join(here, "..", "src", "server.mjs");
const sampleWast = join(here, "..", "..", "sample-wast");

/** Run a scripted JSON-RPC exchange against a freshly spawned server. */
async function exchange(requests) {
  const root = await mkdtemp(join(tmpdir(), "wast-mcp-rpc-"));
  await cp(sampleWast, join(root, "sample"), { recursive: true });

  const child = spawn(process.execPath, [serverPath, root], {
    stdio: ["pipe", "pipe", "pipe"],
  });

  const responses = [];
  let buffered = "";
  const wanted = requests.filter((r) => r.id !== undefined).length;

  const done = new Promise((resolve, reject) => {
    child.stdout.on("data", (chunk) => {
      buffered += chunk;
      let newline;
      while ((newline = buffered.indexOf("\n")) !== -1) {
        const line = buffered.slice(0, newline).trim();
        buffered = buffered.slice(newline + 1);
        if (line) responses.push(JSON.parse(line));
      }
      if (responses.length >= wanted) resolve();
    });
    child.on("error", reject);
    child.on("exit", (code) => {
      if (responses.length < wanted) {
        reject(new Error(`server exited early (code ${code})`));
      }
    });
    setTimeout(() => reject(new Error("timed out waiting for responses")), 30_000).unref();
  });

  for (const request of requests) {
    child.stdin.write(JSON.stringify(request) + "\n");
  }

  try {
    await done;
  } finally {
    child.kill();
  }
  return responses;
}

const INITIALIZE = {
  jsonrpc: "2.0",
  id: 1,
  method: "initialize",
  params: {
    protocolVersion: "2024-11-05",
    capabilities: {},
    clientInfo: { name: "test", version: "0" },
  },
};
const INITIALIZED = { jsonrpc: "2.0", method: "notifications/initialized" };

test("the server initializes and advertises its tools over stdio", async () => {
  const [init, list] = await exchange([
    INITIALIZE,
    INITIALIZED,
    { jsonrpc: "2.0", id: 2, method: "tools/list", params: {} },
  ]);

  assert.equal(init.result.serverInfo.name, "wast");
  const names = list.result.tools.map((t) => t.name).sort();
  assert.deepEqual(names, [
    "wast_compile",
    "wast_list_components",
    "wast_list_funcs",
    "wast_read",
    "wast_render",
    "wast_write",
  ]);
  for (const tool of list.result.tools) {
    assert.equal(tool.inputSchema.type, "object", `${tool.name} needs an object schema`);
  }
});

test("a tools/call round-trip reaches the real components", async () => {
  const [, read] = await exchange([
    INITIALIZE,
    INITIALIZED,
    {
      jsonrpc: "2.0",
      id: 2,
      method: "tools/call",
      params: { name: "wast_read", arguments: { component: "sample", funcs: ["square"] } },
    },
  ]);

  assert.equal(read.result.isError, undefined, read.result.content?.[0]?.text);
  const doc = JSON.parse(read.result.content[0].text);
  assert.equal(doc.funcs[0].uid, "square");
});

test("a rejected edit comes back as a tool error with its code intact", async () => {
  const [, call] = await exchange([
    INITIALIZE,
    INITIALIZED,
    {
      jsonrpc: "2.0",
      id: 2,
      method: "tools/call",
      params: {
        name: "wast_write",
        arguments: {
          component: "sample",
          document: {
            version: 1,
            funcs: [
              {
                uid: "square",
                source: "exported",
                wit_name: "square",
                params: [{ uid: "x", type: "u32" }],
                result: "u32",
                body: [{ LocalGet: { uid: "ghost" } }],
              },
            ],
          },
        },
      },
    },
  ]);

  assert.equal(call.result.isError, true);
  assert.match(call.result.content[0].text, /unknown_local:/);
});
