#!/usr/bin/env node
// Build the wasm components the extension needs (4 syntax plugins +
// partial-manager + wast-codec) and jco-transpile each into
// dist/components/<id>/. The extension imports these at activation time
// via Node's bare-specifier resolution (preview2-shim is a runtime dep).

import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..", "..", "..");
const out = join(here, "..", "dist", "components");

function haveMise() {
  return spawnSync("mise", ["--version"], { stdio: "ignore" }).status === 0;
}

const targets = [
  { crate: "wast-syntax-raw", artifact: "wast_syntax_raw.wasm", id: "raw" },
  { crate: "wast-syntax-ruby-like", artifact: "wast_syntax_ruby_like.wasm", id: "ruby-like" },
  { crate: "wast-syntax-ts-like", artifact: "wast_syntax_ts_like.wasm", id: "ts-like" },
  { crate: "wast-syntax-rust-like", artifact: "wast_syntax_rust_like.wasm", id: "rust-like" },
  { crate: "wast-partial-manager", artifact: "wast_partial_manager.wasm", id: "partial-manager" },
  { crate: "wast-codec", artifact: "wast_codec.wasm", id: "codec" },
  { crate: "wast-compiler-component", artifact: "wast_compiler_component.wasm", id: "compiler" },
];

await rm(out, { recursive: true, force: true });
await mkdir(out, { recursive: true });

for (const p of targets) {
  console.log(`\n== ${p.id} ==`);
  const cmd = process.env.MISE_BIN || (haveMise() ? "mise" : null);
  const [prog, prefix] = cmd
    ? [cmd, ["x", "--", "cargo", "component"]]
    : ["cargo", ["component"]];
  const build = spawnSync(
    prog,
    [...prefix, "build", "-p", p.crate, "--release"],
    { cwd: root, stdio: "inherit" },
  );
  if (build.status !== 0) {
    console.error(`cargo component build failed for ${p.crate}`);
    process.exit(build.status ?? 1);
  }

  const wasm = join(root, "target", "wasm32-wasip1", "release", p.artifact);
  const dest = join(out, p.id);
  await mkdir(dest, { recursive: true });

  const t = spawnSync(
    "npx",
    [
      "jco",
      "transpile",
      wasm,
      "-o",
      dest,
      "--name",
      p.id.replace(/-/g, "_"),
      "--no-typescript",
    ],
    { stdio: "inherit" },
  );
  if (t.status !== 0) {
    console.error(`jco transpile failed for ${p.id}`);
    process.exit(t.status ?? 1);
  }

  // jco emits ESM but doesn't drop a package.json — without one Node has
  // to re-parse each load (MODULE_TYPELESS_PACKAGE_JSON warning). Stamp
  // the minimum that flips the directory's module type.
  await writeFile(
    join(dest, "package.json"),
    JSON.stringify({ type: "module" }, null, 2) + "\n",
  );

  // Patch `wasi_snapshot_preview1.random_get` to fill guest memory
  // directly from node:crypto. The default jco wiring routes through
  // the preview1→preview2 adapter, whose `cabi_import_realloc` traps
  // (assertion failed at adapter line 376) the first time std's
  // HashMap lazy-seeds itself. Bypassing the adapter avoids the trap.
  const jsPath = join(dest, `${p.id.replace(/-/g, "_")}.js`);
  let js = await readFile(jsPath, "utf-8");
  if (/random_get:\s*exports0\['?\d+'?\],/.test(js)) {
    js = js.replace(
      /^import { random } from '@bytecodealliance\/preview2-shim\/random';$/m,
      `$&\nimport { randomFillSync as _wastRandomFillSync } from 'node:crypto';`,
    );
    js = js.replace(
      /random_get:\s*exports0\['?\d+'?\],/,
      `random_get: (buf, buf_len) => {
          _wastRandomFillSync(new Uint8Array(exports1.memory.buffer, buf, buf_len));
          return 0;
        },`,
    );
    await writeFile(jsPath, js);
  }
}

console.log(`\nBuilt ${targets.length} components into ${out}`);
