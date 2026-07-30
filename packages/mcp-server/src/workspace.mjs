// Filesystem side: find wast components under a root, read and write their
// source files, and keep every path inside the root.

import { readdir, readFile, realpath, rename, rm, writeFile, mkdir } from "node:fs/promises";
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
export async function resolveComponentDir(root, component) {
  if (typeof component !== "string" || component.trim() === "") {
    throw new WorkspaceError("`component` must be a non-empty path relative to the server root");
  }
  if (isAbsolute(component)) {
    throw new WorkspaceError("`component` must be relative to the server root, not absolute");
  }

  const contains = (base, path) => {
    const rel = relative(base, path);
    return rel === "" || (rel !== ".." && !rel.startsWith(".." + sep) && !isAbsolute(rel));
  };

  const dir = resolve(root, component);
  if (!contains(root, dir)) {
    throw new WorkspaceError(`'${component}' resolves outside the server root`);
  }

  // `resolve` is lexical, so the check above says nothing about symlinks: a
  // link planted inside the root pointing out of it would still be read and
  // written through. Re-check the real paths. A directory that doesn't exist
  // yet can't be a link, and its absence surfaces as a clearer error later.
  try {
    const [realRoot, realDir] = await Promise.all([realpath(root), realpath(dir)]);
    if (!contains(realRoot, realDir)) {
      throw new WorkspaceError(`'${component}' resolves outside the server root via a symlink`);
    }
  } catch (err) {
    if (err instanceof WorkspaceError) throw err;
    // ENOENT and friends: nothing to resolve, so nothing to escape through.
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

/**
 * Persist what `codec.write` produced. Returns the files actually written.
 *
 * Each file is staged next to its destination and renamed into place, so a
 * failure mid-write can't leave a truncated `wast.json` behind — that file is
 * the entire program, and `writeFile` truncates before it writes. Both
 * payloads are staged before either rename lands, which keeps `wast.json` and
 * its syms file from disagreeing when the second write is the one that fails.
 */
export async function writeComponentFiles(dir, files, lang = "en") {
  const staged = [{ name: WAST_JSON, data: files.wastJson }];
  if (files.symsEnYaml !== null && files.symsEnYaml !== undefined) {
    staged.push({ name: `syms.${lang}.yaml`, data: files.symsEnYaml });
  }

  const temps = [];
  try {
    for (const { name, data } of staged) {
      const temp = join(dir, `.${name}.tmp`);
      await writeFile(temp, data);
      temps.push({ temp, final: join(dir, name) });
    }
    for (const { temp, final } of temps) {
      await rename(temp, final);
    }
  } finally {
    // Anything still staged means a write or rename failed; don't leave the
    // scratch files behind.
    await Promise.all(
      temps.map(({ temp }) => rm(temp, { force: true }).catch(() => {})),
    );
  }
  return staged.map(({ name }) => name);
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

