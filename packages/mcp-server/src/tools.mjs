// The structured write path as callable operations.
//
// Every tool is a plain async function over `{ root, runtime }`, so the tests
// drive them directly and `server.mjs` only has to deal with MCP framing.
//
// The shape of the write path is: read the component → narrow it to the funcs
// you care about (`partial-manager.extract`) → edit the IR as JSON
// (`ir-json`) → merge the edit back with validation
// (`partial-manager.merge`) → persist (`codec.write`). No text parser is
// involved at any step, which is the point: identity travels as uids and
// bodies travel as instruction trees.

import { WRITE_SYNTAX, describeError } from "./runtime.mjs";
import {
  listComponents,
  readComponentFiles,
  resolveComponentDir,
  writeComponentFiles,
  writeCompiled,
} from "./workspace.mjs";

export class ToolError extends Error {}

function symOf(entries, uid) {
  return entries.find((e) => e.uid === uid)?.displayName ?? null;
}

/** Read a component off disk and decode it into the shape the components take. */
async function open(ctx, { component, lang = "en" }) {
  const dir = resolveComponentDir(ctx.root, component);
  const files = await readComponentFiles(dir, lang);
  let decoded;
  try {
    decoded = ctx.runtime.codec.read(files.wastJson, files.symsYaml);
  } catch (err) {
    throw new ToolError(`codec.read failed for '${component}': ${describeError(err)}`);
  }
  return { dir, files, component: decoded };
}

/** Turn a `funcs` argument into partial-manager extract targets. */
function targetsOf(funcs, includeCallers) {
  if (!Array.isArray(funcs) || funcs.length === 0) return null;
  return funcs.map((sym) => {
    if (typeof sym !== "string") {
      throw new ToolError("`funcs` must be an array of func uid strings");
    }
    return { sym, includeCaller: Boolean(includeCallers) };
  });
}

/** Narrow a component to the requested funcs, or hand back the whole thing. */
function view(ctx, component, funcs, includeCallers) {
  const targets = targetsOf(funcs, includeCallers);
  if (!targets) return component;
  try {
    return ctx.runtime.partialManager.extract(component, targets);
  } catch (err) {
    throw new ToolError(`extract failed: ${describeError(err)}`);
  }
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/** Every component directory under the server root. */
export async function wastListComponents(ctx) {
  const components = await listComponents(ctx.root);
  return { root: ctx.root, components };
}

/**
 * Signatures and display names, no bodies — the cheap orientation call before
 * deciding what to read in full.
 */
export async function wastListFuncs(ctx, args) {
  const { component } = await open(ctx, args);
  const { internal, local } = component.syms;
  return {
    funcs: component.funcs.map(([uid, func]) => ({
      uid,
      name: symOf(internal, uid),
      kind: func.source.tag,
      wit_name: func.source.val,
      params: func.params.map(([paramUid, type]) => ({
        uid: paramUid,
        type,
        name: symOf(local, paramUid),
      })),
      result: func.result ?? null,
      has_body: func.body !== null && func.body !== undefined,
    })),
    types: component.types.map(([uid, def]) => ({
      uid,
      name: symOf(internal, uid),
      kind: def.source.tag,
      definition: def.definition.tag,
    })),
  };
}

/**
 * The editable document: the IR as JSON, with uids and instruction trees.
 *
 * Passing `funcs` extracts a partial, which is the normal way to work — it
 * keeps the document small and pulls in the callee signatures the edit needs.
 */
export async function wastRead(ctx, args) {
  const { component } = await open(ctx, args);
  const narrowed = view(ctx, component, args.funcs, args.include_callers);
  let text;
  try {
    text = ctx.runtime.plugins[WRITE_SYNTAX].toText(narrowed);
  } catch (err) {
    throw new ToolError(`${WRITE_SYNTAX}.to_text failed: ${describeError(err)}`);
  }
  return JSON.parse(text);
}

/**
 * Apply an edited document.
 *
 * `merge` validates before anything is written: boundary signatures against
 * the rest of the program, uid conflicts, and — the part that matters for a
 * structured write path — the bodies themselves (calls resolve to real funcs
 * with the right argument set, locals are defined). A rejected edit leaves
 * the files untouched and reports coded errors.
 *
 * With `verify` (the default) the component is compiled afterwards so type
 * errors the merge can't see surface immediately. A compile failure does not
 * roll the write back — the files on disk are the edit, and the error says
 * what to fix next.
 */
export async function wastWrite(ctx, args) {
  if (args.document === undefined || args.document === null) {
    throw new ToolError("`document` is required (the edited output of wast_read)");
  }
  const documentText =
    typeof args.document === "string" ? args.document : JSON.stringify(args.document);

  const { dir, files, component } = await open(ctx, args);

  let parsed;
  try {
    parsed = ctx.runtime.plugins[WRITE_SYNTAX].fromText(documentText, component);
  } catch (err) {
    throw new ToolError(`document rejected:\n${describeError(err)}`);
  }

  let merged;
  try {
    merged = ctx.runtime.partialManager.merge(parsed, component);
  } catch (err) {
    throw new ToolError(`merge rejected the edit (nothing written):\n${describeError(err)}`);
  }

  let encoded;
  try {
    encoded = ctx.runtime.codec.write(files.worldWit, merged);
  } catch (err) {
    throw new ToolError(`codec.write failed (nothing written):\n${describeError(err)}`);
  }

  const written = await writeComponentFiles(dir, encoded, files.lang);
  const result = { written, funcs: merged.funcs.map(([uid]) => uid) };

  if (args.verify === false) return result;

  try {
    const wasm = ctx.runtime.compiler.compile(merged, files.worldWit);
    result.compile = { ok: true, bytes: wasm.byteLength };
    return result;
  } catch (err) {
    throw new ToolError(
      `edit was written (${written.join(", ")}) but the component no longer compiles:\n` +
        describeError(err),
    );
  }
}

/** Compile the component to a wasm Component under `<component>/dist/`. */
export async function wastCompile(ctx, args) {
  const { dir, files, component } = await open(ctx, args);
  let wasm;
  try {
    wasm = ctx.runtime.compiler.compile(component, files.worldWit);
  } catch (err) {
    throw new ToolError(`compile failed:\n${describeError(err)}`);
  }
  const path = await writeCompiled(dir, wasm);
  return { path, bytes: wasm.byteLength };
}

/**
 * Project the component through any syntax plugin, for showing a human
 * readable version (or a reviewable diff) of what the IR now says.
 */
export async function wastRender(ctx, args) {
  const syntax = args.syntax ?? "ruby-like";
  const plugin = ctx.runtime.plugins[syntax];
  if (!plugin) {
    throw new ToolError(
      `unknown syntax '${syntax}' (available: ${Object.keys(ctx.runtime.plugins).join(", ")})`,
    );
  }
  const { component } = await open(ctx, args);
  const narrowed = view(ctx, component, args.funcs, args.include_callers);
  try {
    return { syntax, text: plugin.toText(narrowed) };
  } catch (err) {
    throw new ToolError(`${syntax}.to_text failed: ${describeError(err)}`);
  }
}
