// Drive every tool against a throwaway copy of packages/sample-wast, so the
// write tools can actually write. Goes through `callTool` — the same entry
// the MCP transport uses — so the tool table and error framing are covered
// alongside the behaviour.

import { test } from "node:test";
import assert from "node:assert/strict";
import { cp, mkdtemp, readdir, readFile, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { loadRuntime } from "../src/runtime.mjs";
import { TOOLS, callTool } from "../src/server.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const sampleWast = join(here, "..", "..", "sample-wast");

const runtime = await loadRuntime();

/** A fresh workspace root containing `sample/` (a copy of sample-wast). */
async function freshRoot() {
  const root = await mkdtemp(join(tmpdir(), "wast-mcp-"));
  await cp(sampleWast, join(root, "sample"), { recursive: true });
  return { root, runtime };
}

/** Call a tool and fail loudly if it reported an error. */
async function ok(ctx, name, args) {
  const res = await callTool(ctx, name, args);
  assert.equal(res.isError, undefined, `${name} failed: ${res.content[0]?.text}`);
  const text = res.content[0].text;
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

/** Call a tool expecting failure; returns the error text. */
async function fails(ctx, name, args) {
  const res = await callTool(ctx, name, args);
  assert.equal(res.isError, true, `${name} unexpectedly succeeded: ${res.content[0]?.text}`);
  return res.content[0].text;
}

test("every tool in the table has a schema and a handler", () => {
  for (const tool of TOOLS) {
    assert.ok(tool.name.startsWith("wast_"), `${tool.name} should be namespaced`);
    assert.ok(tool.description?.length > 20, `${tool.name} needs a usable description`);
    assert.equal(tool.inputSchema.type, "object");
    assert.equal(typeof tool.handler, "function");
  }
});

test("wast_list_components finds the component in the root", async () => {
  const ctx = await freshRoot();
  const result = await ok(ctx, "wast_list_components", {});
  assert.deepEqual(result.components, ["sample"]);
});

test("wast_list_funcs reports signatures and display names without bodies", async () => {
  const ctx = await freshRoot();
  const result = await ok(ctx, "wast_list_funcs", { component: "sample" });

  const square = result.funcs.find((f) => f.uid === "square");
  assert.equal(square.kind, "internal");
  assert.equal(square.has_body, true);
  assert.deepEqual(
    square.params.map((p) => [p.uid, p.type]),
    [["x", "u32"]],
  );

  const exported = result.funcs.find((f) => f.uid === "sum_of_squares");
  assert.equal(exported.kind, "exported");
  assert.equal(exported.wit_name, "sum-of-squares", "the WIT-level name travels too");
  assert.ok(result.types.some((t) => t.uid === "point"));
  // No bodies in the orientation call.
  assert.ok(!JSON.stringify(result).includes("LocalGet"));
});

test("wast_read returns an editable document with explicit uids and instruction trees", async () => {
  const ctx = await freshRoot();
  const doc = await ok(ctx, "wast_read", { component: "sample", funcs: ["square"] });

  assert.equal(doc.version, 1);
  const square = doc.funcs.find((f) => f.uid === "square");
  assert.ok(square, "the requested func must be present");
  assert.deepEqual(
    square.params.map((p) => p.uid),
    ["x"],
    "params carry uids, not just names",
  );
  assert.ok(Array.isArray(square.body), "the body is an instruction tree");
  assert.ok(
    JSON.stringify(square.body).includes("LocalGet"),
    `expected instructions, got ${JSON.stringify(square.body)}`,
  );
});

test("wast_read narrows to the requested funcs and pulls in callee signatures", async () => {
  const ctx = await freshRoot();
  const doc = await ok(ctx, "wast_read", { component: "sample", funcs: ["cube"] });
  const uids = doc.funcs.map((f) => f.uid).sort();
  // cube calls square, so square comes along as a signature-only import.
  assert.deepEqual(uids, ["cube", "square"]);
  const square = doc.funcs.find((f) => f.uid === "square");
  assert.equal(square.source, "imported");
  assert.equal(square.body, undefined, "callee stubs carry no body");
});

test("wast_write applies a body edit and the component still compiles", async () => {
  const ctx = await freshRoot();
  const doc = await ok(ctx, "wast_read", { component: "sample", funcs: ["square"] });

  // x * x → x + x, by editing the tree directly.
  doc.funcs.find((f) => f.uid === "square").body = [
    {
      Arithmetic: {
        op: "Add",
        lhs: { LocalGet: { uid: "x" } },
        rhs: { LocalGet: { uid: "x" } },
      },
    },
  ];

  const result = await ok(ctx, "wast_write", { component: "sample", document: doc });
  assert.ok(result.written.includes("wast.json"));
  assert.equal(result.compile.ok, true, "verify should compile by default");

  // The edit is on disk: re-reading shows the new tree.
  const after = await ok(ctx, "wast_read", { component: "sample", funcs: ["square"] });
  assert.equal(
    JSON.stringify(after.funcs.find((f) => f.uid === "square").body).includes("\"Add\""),
    true,
  );
  // Untouched funcs survived.
  const all = await ok(ctx, "wast_list_funcs", { component: "sample" });
  assert.ok(all.funcs.some((f) => f.uid === "greeting"));
});

test("wast_write renames through the name field, leaving the uid alone", async () => {
  const ctx = await freshRoot();
  const doc = await ok(ctx, "wast_read", { component: "sample", funcs: ["square"] });
  const square = doc.funcs.find((f) => f.uid === "square");
  square.name = "squared";
  delete square.body; // signature/name-only edit

  await ok(ctx, "wast_write", { component: "sample", document: doc });

  const listed = await ok(ctx, "wast_list_funcs", { component: "sample" });
  const renamed = listed.funcs.find((f) => f.uid === "square");
  assert.equal(renamed.name, "squared", "display name changed");
  assert.equal(renamed.uid, "square", "uid is stable across a rename");
  assert.equal(renamed.has_body, true, "omitting body preserved it");

  const syms = await readFile(join(ctx.root, "sample", "syms.en.yaml"), "utf-8");
  assert.match(syms, /squared/, "the rename landed in syms, not in the IR");
});

test("wast_write rejects a body with an undefined local and writes nothing", async () => {
  const ctx = await freshRoot();
  const before = await readFile(join(ctx.root, "sample", "wast.json"), "utf-8");

  const doc = await ok(ctx, "wast_read", { component: "sample", funcs: ["square"] });
  doc.funcs.find((f) => f.uid === "square").body = [{ LocalGet: { uid: "nope" } }];

  const error = await fails(ctx, "wast_write", { component: "sample", document: doc });
  assert.match(error, /unknown_local:/, `expected a coded error, got: ${error}`);
  assert.match(error, /nothing written/);
  assert.equal(
    await readFile(join(ctx.root, "sample", "wast.json"), "utf-8"),
    before,
    "a rejected edit must not touch the file",
  );
});

test("wast_write rejects a call whose args don't match the callee", async () => {
  const ctx = await freshRoot();
  const doc = await ok(ctx, "wast_read", { component: "sample", funcs: ["cube"] });
  // square takes `x`; pass something else.
  doc.funcs.find((f) => f.uid === "cube").body = [
    { Call: { func_uid: "square", args: [["wrong", { LocalGet: { uid: "x" } }]] } },
  ];
  const error = await fails(ctx, "wast_write", { component: "sample", document: doc });
  assert.match(error, /call_arg_unknown:/, error);
});

test("wast_write rejects a signature change that hides callers", async () => {
  const ctx = await freshRoot();
  // `square` is called by cube/poly/sum_of_squares. Reading it without
  // include_callers locks the signature, so changing it must be refused.
  const doc = await ok(ctx, "wast_read", { component: "sample", funcs: ["square"] });
  const square = doc.funcs.find((f) => f.uid === "square");
  square.params = [{ uid: "x", type: "u64" }];
  square.result = "u64";
  delete square.body;

  const error = await fails(ctx, "wast_write", { component: "sample", document: doc });
  assert.match(error, /signature_mismatch:|caller_not_included:/, error);
});

test("wast_write reports malformed documents as parse errors", async () => {
  const ctx = await freshRoot();
  const error = await fails(ctx, "wast_write", {
    component: "sample",
    document: "{ not json",
  });
  assert.match(error, /parse_error:/, error);
});

test("wast_compile writes a wasm component next to the source", async () => {
  const ctx = await freshRoot();
  const result = await ok(ctx, "wast_compile", { component: "sample" });
  assert.ok(result.bytes > 0);
  const wasm = new Uint8Array(await readFile(result.path));
  assert.deepEqual([...wasm.slice(0, 4)], [0x00, 0x61, 0x73, 0x6d]);
});

test("wast_render projects the IR through a read-only syntax", async () => {
  const ctx = await freshRoot();
  const result = await ok(ctx, "wast_render", {
    component: "sample",
    syntax: "ruby-like",
    funcs: ["square"],
  });
  assert.equal(result.syntax, "ruby-like");
  assert.match(result.text, /def /);
});

test("wast_render rejects an unknown syntax by name", async () => {
  const ctx = await freshRoot();
  const error = await fails(ctx, "wast_render", { component: "sample", syntax: "cobol-like" });
  assert.match(error, /unknown syntax 'cobol-like'/);
});

test("paths outside the server root are refused", async () => {
  const ctx = await freshRoot();
  for (const component of ["../escape", "sample/../../escape", "/etc"]) {
    const error = await fails(ctx, "wast_list_funcs", { component });
    assert.match(error, /outside the server root|must be relative/, `${component}: ${error}`);
  }
});

test("a symlink pointing out of the root is refused too", async () => {
  // `resolve` is lexical, so a link planted inside the root would otherwise
  // pass the containment check and be read and written through.
  const ctx = await freshRoot();
  const outside = await mkdtemp(join(tmpdir(), "wast-mcp-outside-"));
  await cp(sampleWast, join(outside, "secret"), { recursive: true });
  await symlink(join(outside, "secret"), join(ctx.root, "link"));

  const error = await fails(ctx, "wast_list_funcs", { component: "link" });
  assert.match(error, /outside the server root/, error);
});

test("a rejected write leaves no scratch files behind", async () => {
  const ctx = await freshRoot();
  const doc = await ok(ctx, "wast_read", { component: "sample", funcs: ["square"] });
  doc.funcs.find((f) => f.uid === "square").body = [{ LocalGet: { uid: "nope" } }];
  await fails(ctx, "wast_write", { component: "sample", document: doc });

  const entries = await readdir(join(ctx.root, "sample"));
  assert.deepEqual(
    entries.filter((e) => e.startsWith(".") || e.endsWith(".tmp")),
    [],
    `unexpected leftovers: ${entries.join(", ")}`,
  );
});

test("a successful write leaves no scratch files behind", async () => {
  const ctx = await freshRoot();
  const doc = await ok(ctx, "wast_read", { component: "sample", funcs: ["square"] });
  await ok(ctx, "wast_write", { component: "sample", document: doc });

  const entries = await readdir(join(ctx.root, "sample"));
  assert.deepEqual(
    entries.filter((e) => e.startsWith(".") || e.endsWith(".tmp")),
    [],
    `unexpected leftovers: ${entries.join(", ")}`,
  );
});

test("an extracted callee stub does not drag its body into validation", async () => {
  // `extract` gives `square` to the document as a signature-only import.
  // Re-attaching full's body to it on the way back would put a func this
  // edit doesn't own under merge's body validation.
  const ctx = await freshRoot();
  const doc = await ok(ctx, "wast_read", { component: "sample", funcs: ["cube"] });
  const stub = doc.funcs.find((f) => f.uid === "square");
  assert.equal(stub.source, "imported");
  assert.equal(stub.body, undefined);

  const result = await ok(ctx, "wast_write", { component: "sample", document: doc });
  assert.equal(result.compile.ok, true);
});

test("a missing component reports which file is absent", async () => {
  const ctx = await freshRoot();
  const error = await fails(ctx, "wast_list_funcs", { component: "not-there" });
  assert.match(error, /no wast\.json/);
});

test("unknown tool names are reported, not thrown", async () => {
  const ctx = await freshRoot();
  const res = await callTool(ctx, "wast_teleport", {});
  assert.equal(res.isError, true);
  assert.match(res.content[0].text, /unknown tool/);
});
