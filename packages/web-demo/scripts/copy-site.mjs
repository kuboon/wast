#!/usr/bin/env node
// Copy the static index.html + src/ + public/ tree into dist/ ready for
// GitHub Pages. Everything is plain ES modules, no bundler.

import { cp, mkdir, rm } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const dist = join(root, "dist");

await rm(dist, { recursive: true, force: true });
await mkdir(dist, { recursive: true });

await cp(join(root, "index.html"), join(dist, "index.html"));
await cp(join(root, "src"), join(dist, "src"), { recursive: true });
await cp(join(root, "public"), join(dist, "public"), { recursive: true });

console.log(`Site copied to ${dist}`);
