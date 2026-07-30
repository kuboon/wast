// Filesystem side: find wast components under a root, read and write their
// source files, and keep every path inside the root.

import { readdir, readFile, stat, writeFile, mkdir } from "node:fs/promises";
import { isAbsolute, join, relative, resolve, sep } from "node:path";

/** Files that make up a component on disk. */
const WAST_JSON = "wast.json";
const WORLD_WIT = "world.wit";

/** How deep to look for components below the root. */
const MAX_DEPTH = 4;

const SKIP_DIRS = new Set([
  "node_modules",
  "target",
  "dist",
  ".git",
  "public",
  ".devcontainer",
]);

export class WorkspaceError extends Error {}

/**
 * Resolve a caller-supplied component path against the root, refusing
 * anything that escapes it.
 *
 * The server hands agents write access to whatever is under `root`; without
 * this check a `../..` in a tool argument would reach the rest of the disk.
 */
export function resolveComponentDir(root, component) {
  if (typeof component !== "string" || component.trim() === "") {
    throw new WorkspaceError("`component` must be a non-empty path relative to the server root");
  }
  if (isAbsolute(component)) {
    throw new WorkspaceError("`component` must be relative to the server root, not absolute");
  }
  const dir = resolve(root, component);
  const rel = relative(root, dir);
  if (rel.startsWith("..") || rel.split(sep).includes("..")) {
    throw new WorkspaceError(`'${component}' resolves outside the server root`);
  }
  return dir;
}

/** Directories under `root` that hold a `wast.json`, as root-relative paths. */
export async function listComponents(root) {
  const found = [];
  async function walk(dir, depth) {
    let entries;
    try {
      entries = await readdir(dir, { withFileTypes: true });
    } catch {
      return;
    }
    if (entries.some((e) => e.isFile() && e.name === WAST_JSON)) {
      found.push(relative(root, dir) || ".");
    }
    if (depth >= MAX_DEPTH) return;
    for (const entry of entries) {
      if (!entry.isDirectory() || SKIP_DIRS.has(entry.name) || entry.name.startsWith(".")) {
        continue;
      }
      await walk(join(dir, entry.name), depth + 1);
    }
  }
  await walk(root, 0);
  return found.sort();
}

async function readOptional(path) {
  try {
    return new Uint8Array(await readFile(path));
  } catch {
    return null;
  }
}

/** Read a component's on-disk source files. `syms` is optional. */
export async function readComponentFiles(dir, lang = "en") {
  const wastJson = await readOptional(join(dir, WAST_JSON));
  if (!wastJson) {
    throw new WorkspaceError(`no ${WAST_JSON} in '${dir}'`);
  }
  const worldWit = await readOptional(join(dir, WORLD_WIT));
  if (!worldWit) {
    throw new WorkspaceError(`no ${WORLD_WIT} in '${dir}' — required to write or compile`);
  }
  return {
    wastJson,
    worldWit,
    symsYaml: await readOptional(join(dir, `syms.${lang}.yaml`)),
    lang,
  };
}

/** Persist what `codec.write` produced. Returns the files actually written. */
export async function writeComponentFiles(dir, files, lang = "en") {
  const written = [];
  await writeFile(join(dir, WAST_JSON), files.wastJson);
  written.push(WAST_JSON);
  if (files.symsEnYaml !== null && files.symsEnYaml !== undefined) {
    const name = `syms.${lang}.yaml`;
    await writeFile(join(dir, name), files.symsEnYaml);
    written.push(name);
  }
  return written;
}

/** Write a compiled component next to its source, as `dist/<dirname>.wasm`. */
export async function writeCompiled(dir, wasm) {
  const outDir = join(dir, "dist");
  await mkdir(outDir, { recursive: true });
  const name = `${dir.split(sep).filter(Boolean).pop() ?? "component"}.wasm`;
  const path = join(outDir, name);
  await writeFile(path, wasm);
  return path;
}

/** Newest mtime across a component's source files, for change detection. */
export async function sourceMtime(dir, lang = "en") {
  let newest = 0;
  for (const name of [WAST_JSON, `syms.${lang}.yaml`]) {
    try {
      const st = await stat(join(dir, name));
      newest = Math.max(newest, st.mtimeMs);
    } catch {
      // absent — syms is optional
    }
  }
  return newest;
}
