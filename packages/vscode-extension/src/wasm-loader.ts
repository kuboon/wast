/**
 * Lazy-load the wasm components bundled into `dist/components/`.
 *
 * Each jco-transpiled module imports `@bytecodealliance/preview2-shim/*`
 * via bare specifiers; in the desktop VS Code host (Node.js) those resolve
 * through the extension's `node_modules`. Web hosts will need a different
 * path — out of scope for Phase 1.
 */

import * as vscode from "vscode";
import type {
  WastComponent,
  WastFuncWasm,
  WastTypeDefWasm,
} from "./wast-db.js";

export type SyntaxPluginId = "raw" | "ruby-like" | "ts-like" | "rust-like";

export interface WastError {
  message: string;
  location: string | null;
}

export interface SyntaxPlugin {
  toText(component: WastComponent): string;
  fromText(text: string, existing: WastComponent): WastComponent;
}

export interface PartialManager {
  extract(
    full: WastComponent,
    targets: { sym: string; includeCaller: boolean }[],
  ): WastComponent;
  merge(partial: WastComponent, full: WastComponent): WastComponent;
}

export interface ComponentFiles {
  wastJson: Uint8Array;
  symsEnYaml: Uint8Array | null;
}

export interface Codec {
  compileWit(worldWit: Uint8Array): ComponentFiles;
  read(wastJson: Uint8Array, symsEnYaml: Uint8Array | null): WastComponent;
  write(worldWit: Uint8Array, component: WastComponent): ComponentFiles;
  merge(
    worldWit: Uint8Array,
    full: ComponentFiles,
    partial: WastComponent,
  ): ComponentFiles;
}

export interface Compiler {
  compile(component: WastComponent, worldWit: Uint8Array): Uint8Array;
}

export interface LoadedRuntime {
  syntaxPlugins: Record<SyntaxPluginId, SyntaxPlugin>;
  partialManager: PartialManager;
  codec: Codec;
  compiler: Compiler;
}

const PLUGIN_IDS: SyntaxPluginId[] = ["raw", "ruby-like", "ts-like", "rust-like"];

function pluginModuleName(id: SyntaxPluginId): string {
  // jco --name uses snake_case; bundle script passes id.replace(/-/g, "_")
  return id.replace(/-/g, "_") + ".js";
}

let cached: Promise<LoadedRuntime> | null = null;

/**
 * Load every bundled component. The first call triggers dynamic imports;
 * subsequent calls return the same promise. Errors are surfaced to the
 * caller — the extension's activation should catch and show them.
 */
export function loadRuntime(context: vscode.ExtensionContext): Promise<LoadedRuntime> {
  if (cached) return cached;
  cached = doLoad(context).catch((err) => {
    cached = null; // allow retry after recovery
    throw err;
  });
  return cached;
}

async function doLoad(context: vscode.ExtensionContext): Promise<LoadedRuntime> {
  const componentsRoot = vscode.Uri.joinPath(
    context.extensionUri,
    "dist",
    "components",
  );

  const syntaxPlugins = {} as Record<SyntaxPluginId, SyntaxPlugin>;
  for (const id of PLUGIN_IDS) {
    const modUri = vscode.Uri.joinPath(componentsRoot, id, pluginModuleName(id));
    const m: { syntaxPlugin: SyntaxPlugin } = await import(modUri.fsPath);
    syntaxPlugins[id] = m.syntaxPlugin;
  }

  const pmUri = vscode.Uri.joinPath(
    componentsRoot,
    "partial-manager",
    "partial_manager.js",
  );
  const pm: { partialManager: PartialManager } = await import(pmUri.fsPath);

  const codecUri = vscode.Uri.joinPath(componentsRoot, "codec", "codec.js");
  const cdc: { codec: Codec } = await import(codecUri.fsPath);

  const compilerUri = vscode.Uri.joinPath(componentsRoot, "compiler", "compiler.js");
  const cmp: { compiler: Compiler } = await import(compilerUri.fsPath);

  return {
    syntaxPlugins,
    partialManager: pm.partialManager,
    codec: cdc.codec,
    compiler: cmp.compiler,
  };
}

/** Pull the WastError list out of whatever shape jco threw. */
export function extractErrors(thrown: unknown): WastError[] {
  if (Array.isArray(thrown)) return thrown as WastError[];
  if (
    typeof thrown === "object" &&
    thrown !== null &&
    "payload" in thrown &&
    Array.isArray((thrown as { payload: unknown }).payload)
  ) {
    return (thrown as { payload: WastError[] }).payload;
  }
  if (thrown instanceof Error) return [{ message: thrown.message, location: null }];
  return [{ message: String(thrown), location: null }];
}

// Re-export for callers that just need the wasm-shaped types.
export type { WastComponent, WastFuncWasm, WastTypeDefWasm };
