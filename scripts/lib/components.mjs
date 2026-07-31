// Shared plumbing for the jco component bundles the host packages need.
//
// Three callers want the same two steps (cargo-component build → jco
// transpile) with different tails: the Node hosts (VS Code extension, MCP
// server) need a `package.json` type stamp and the `random_get` patch below,
// while the browser host needs neither. This module owns the common part so
// adding a host doesn't mean a third copy of it.

import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

/** jco names its output after the component id with dashes flattened. */
export function moduleName(id) {
  return id.replace(/-/g, "_");
}

function haveMise() {
  return spawnSync("mise", ["--version"], { stdio: "ignore" }).status === 0;
}

/** `cargo component build -p <crate> --release`, run through mise when it's
 *  around so CI containers resolve cargo-component from its tool path. */
function buildCrate(root, crate) {
  const cmd = process.env.MISE_BIN || (haveMise() ? "mise" : null);
  const [prog, prefix] = cmd
    ? [cmd, ["x", "--", "cargo", "component"]]
    : ["cargo", ["component"]];
  const build = spawnSync(prog, [...prefix, "build", "-p", crate, "--release"], {
    cwd: root,
    stdio: "inherit",
  });
  if (build.status !== 0) {
    console.error(`cargo component build failed for ${crate}`);
    process.exit(build.status ?? 1);
  }
}

function transpile(wasm, dest, id) {
  const t = spawnSync(
    "npx",
    ["jco", "transpile", wasm, "-o", dest, "--name", moduleName(id), "--no-typescript"],
    { stdio: "inherit" },
  );
  if (t.status !== 0) {
    console.error(`jco transpile failed for ${id}`);
    process.exit(t.status ?? 1);
  }
}

/** jco emits ESM but no `package.json`; without one Node re-parses each load
 *  (MODULE_TYPELESS_PACKAGE_JSON). Stamp the minimum that sets module type. */
async function stampModuleType(dest) {
  await writeFile(
    join(dest, "package.json"),
    JSON.stringify({ type: "module" }, null, 2) + "\n",
  );
}

/** Patch `wasi_snapshot_preview1.random_get` to fill guest memory straight
 *  from node:crypto. The default jco wiring routes through the
 *  preview1→preview2 adapter, whose `cabi_import_realloc` traps the first
 *  time std's HashMap lazy-seeds itself. Bypassing the adapter avoids it. */
async function patchNodeRandomGet(dest, id) {
  const jsPath = join(dest, `${moduleName(id)}.js`);
  let js = await readFile(jsPath, "utf-8");
  if (!/random_get:\s*exports0\['?\d+'?\],/.test(js)) return;
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

/**
 * Build one crate as a wasm component and transpile it into `<outDir>/<id>/`.
 *
 * `nodeHost: true` adds the two Node-only fixups (module-type stamp and the
 * `random_get` patch); browser bundles skip both.
 */
export async function bundleComponent({ root, crate, artifact, id, outDir, nodeHost = false }) {
  console.log(`\n== ${id} ==`);
  buildCrate(root, crate);

  const wasm = join(root, "target", "wasm32-wasip1", "release", artifact);
  const dest = join(outDir, id);
  await mkdir(dest, { recursive: true });
  transpile(wasm, dest, id);

  if (nodeHost) {
    await stampModuleType(dest);
    await patchNodeRandomGet(dest, id);
  }
  return dest;
}
