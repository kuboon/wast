#!/usr/bin/env node
// Transpile every component .wasm emitted by wast-demo-gen into an
// ES module via jco, so the browser can load them with a plain
// `import(...)`.

import { readdir, mkdir, rm, readFile, writeFile, copyFile } from "node:fs/promises";
import { join, dirname, basename, extname } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const here = dirname(fileURLToPath(import.meta.url));
const demosDir = join(here, "..", "..", "..", "tmp", "demos");
const outRoot = join(here, "..", "public", "components");

await rm(outRoot, { recursive: true, force: true });
await mkdir(outRoot, { recursive: true });

// Copy manifest + samples straight through.
await copyFile(join(demosDir, "manifest.json"), join(outRoot, "manifest.json"));
await copyFile(join(demosDir, "samples.json"), join(outRoot, "samples.json"));

const entries = (await readdir(demosDir)).filter((n) => n.endsWith(".wasm"));
for (const entry of entries) {
  const id = basename(entry, extname(entry));
  const outDir = join(outRoot, id);
  await mkdir(outDir, { recursive: true });

  const src = join(demosDir, entry);
  const res = spawnSync(
    "npx",
    [
      "jco",
      "transpile",
      src,
      "-o",
      outDir,
      "--name",
      id,
      "--no-typescript",
    ],
    { stdio: "inherit" },
  );
  if (res.status !== 0) {
    console.error(`jco transpile failed for ${id}`);
    process.exit(res.status ?? 1);
  }
}

console.log(`\nTranspiled ${entries.length} components into ${outRoot}`);
