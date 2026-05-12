#!/usr/bin/env node
// Regenerate the example workspace's wast.json + syms.en.yaml from its
// world.wit, using the bundled codec component. Run after editing
// examples/<name>/world.wit:
//
//   node scripts/seed-example.mjs           # defaults to examples/basic
//   node scripts/seed-example.mjs other     # examples/other
import { readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const pkgRoot = join(here, "..");
const name = process.argv[2] ?? "basic";
const exampleDir = join(pkgRoot, "examples", name);
const components = join(pkgRoot, "dist", "components");

const { codec } = await import(join(components, "codec", "codec.js"));

const worldWit = await readFile(join(exampleDir, "world.wit"));
const out = codec.compileWit(new Uint8Array(worldWit));

await writeFile(join(exampleDir, "wast.json"), out.wastJson);
console.log(`wrote ${join(exampleDir, "wast.json")} (${out.wastJson.byteLength} bytes)`);
if (out.symsEnYaml) {
  await writeFile(join(exampleDir, "syms.en.yaml"), out.symsEnYaml);
  console.log(`wrote ${join(exampleDir, "syms.en.yaml")} (${out.symsEnYaml.byteLength} bytes)`);
}
