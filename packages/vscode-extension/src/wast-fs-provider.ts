/**
 * `FileSystemProvider` for the `wast://` scheme.
 *
 * URI format: `wast://<base64url(dir-uri)>/component[?func=uid1&func=uid2]`
 *
 *  - `readFile` runs the configured syntax plugin's `to_text` on the
 *    component (narrowed via `partial-manager.extract` if `?func=` is
 *    present) and returns UTF-8 bytes of the rendered surface text.
 *  - `writeFile` reverses the trip: parse via `from_text`, merge back
 *    into the full component via `partial-manager.merge`, run
 *    `codec.write` to produce on-disk bytes, then persist them to
 *    `<dir>/wast.json` (and `syms.<lang>.yaml` if the codec produced
 *    one).
 *
 * Errors during any save stage become `vscode.FileSystemError`s with a
 * stage-tagged message, which VS Code surfaces as the standard "Unable
 * to save file" toast.
 */

import * as vscode from "vscode";
import {
  readComponent,
  readWorldWit,
  toWastComponent,
  type LoadedComponent,
} from "./wast-db.js";
import {
  extractErrors,
  type LoadedRuntime,
  type SyntaxPluginId,
  type WastComponent,
} from "./wasm-loader.js";

const ENC = new TextEncoder();
const DEC = new TextDecoder("utf-8");

export class WastFileSystemProvider implements vscode.FileSystemProvider {
  private _onDidChangeFile = new vscode.EventEmitter<vscode.FileChangeEvent[]>();
  readonly onDidChangeFile = this._onDidChangeFile.event;

  constructor(private runtime: LoadedRuntime) {}

  watch(_uri: vscode.Uri): vscode.Disposable {
    // We watch the underlying wast.json centrally (in extension.ts) and
    // forward those events via `notifyExternalChange` below — there's no
    // per-URI watcher to set up here.
    return new vscode.Disposable(() => {});
  }

  stat(_uri: vscode.Uri): vscode.FileStat {
    // Sizes / times are advisory for VS Code; returning zeros works.
    return { type: vscode.FileType.File, ctime: 0, mtime: 0, size: 0 };
  }

  // Directory operations don't apply to virtual component URIs.
  readDirectory(): never {
    throw vscode.FileSystemError.FileNotFound();
  }
  createDirectory(): never {
    throw vscode.FileSystemError.NoPermissions();
  }
  delete(): never {
    throw vscode.FileSystemError.NoPermissions();
  }
  rename(): never {
    throw vscode.FileSystemError.NoPermissions();
  }

  // -------------------------------------------------------------------------
  // readFile — render via syntax plugin
  // -------------------------------------------------------------------------

  async readFile(uri: vscode.Uri): Promise<Uint8Array> {
    const dirUri = decodeDirUri(uri);
    if (!dirUri) {
      throw vscode.FileSystemError.FileNotFound(`invalid wast:// URI: ${uri}`);
    }

    const cfg = vscode.workspace.getConfiguration("wast");
    const lang = cfg.get<string>("symsLanguage", "en");
    const pluginId = cfg.get<SyntaxPluginId>("syntaxPlugin", "ruby-like");

    const loaded = await readComponent(dirUri, lang);
    if (!loaded) {
      throw vscode.FileSystemError.FileNotFound(
        `wast.json not readable in ${dirUri.fsPath}`,
      );
    }
    const full = toWastComponent(loaded);

    const targets = parseFuncTargets(uri);
    let view = full;
    if (targets.length > 0) {
      try {
        view = this.runtime.partialManager.extract(
          full,
          targets.map((sym) => ({ sym, includeCaller: false })),
        );
      } catch (err) {
        return ENC.encode(renderErrorBanner("partial-manager.extract", err));
      }
    }

    const plugin = this.runtime.syntaxPlugins[pluginId];
    if (!plugin) {
      return ENC.encode(`# Error: syntax plugin '${pluginId}' not loaded\n`);
    }

    let text: string;
    try {
      text = plugin.toText(view);
    } catch (err) {
      text = renderErrorBanner(`${pluginId}.to_text`, err);
    }
    return ENC.encode(text);
  }

  // -------------------------------------------------------------------------
  // writeFile — parse, merge, persist
  // -------------------------------------------------------------------------

  async writeFile(
    uri: vscode.Uri,
    content: Uint8Array,
    _options: { create: boolean; overwrite: boolean },
  ): Promise<void> {
    const dirUri = decodeDirUri(uri);
    if (!dirUri) {
      throw vscode.FileSystemError.FileNotFound(`invalid wast:// URI: ${uri}`);
    }

    const cfg = vscode.workspace.getConfiguration("wast");
    const lang = cfg.get<string>("symsLanguage", "en");
    const pluginId = cfg.get<SyntaxPluginId>("syntaxPlugin", "ruby-like");

    const loaded = await readComponent(dirUri, lang);
    if (!loaded) {
      throw saveError("setup", [
        { message: `wast.json not readable in ${dirUri.fsPath}`, location: null },
      ]);
    }

    const worldWit = await readWorldWit(dirUri);
    if (!worldWit) {
      throw saveError("setup", [
        {
          message: `world.wit missing in ${dirUri.fsPath} — required for codec.write`,
          location: null,
        },
      ]);
    }

    const plugin = this.runtime.syntaxPlugins[pluginId];
    if (!plugin) {
      throw saveError("setup", [
        { message: `syntax plugin '${pluginId}' not loaded`, location: null },
      ]);
    }

    const full = toWastComponent(loaded);
    const text = DEC.decode(content);

    // Stage 1: parse pane text into a partial WastComponent.
    let parsed: WastComponent;
    try {
      parsed = plugin.fromText(text, full);
    } catch (err) {
      throw saveError("from_text", extractErrors(err));
    }

    // Stage 2: merge the partial back into full (signature + uid checks).
    let merged: WastComponent;
    try {
      merged = this.runtime.partialManager.merge(parsed, full);
    } catch (err) {
      throw saveError("merge", extractErrors(err));
    }

    // Stage 3: run merged through the codec to produce on-disk bytes.
    let files: { wastJson: Uint8Array; symsEnYaml: Uint8Array | null };
    try {
      files = this.runtime.codec.write(worldWit, merged);
    } catch (err) {
      throw saveError("codec.write", extractErrors(err));
    }

    // Persist. wast.json is always rewritten; syms is rewritten only when
    // the codec returned one (it does when there are syms to express).
    const wastJsonUri = vscode.Uri.joinPath(dirUri, "wast.json");
    await vscode.workspace.fs.writeFile(wastJsonUri, files.wastJson);
    if (files.symsEnYaml !== null) {
      const symsUri = vscode.Uri.joinPath(dirUri, `syms.${lang}.yaml`);
      await vscode.workspace.fs.writeFile(symsUri, files.symsEnYaml);
    }
  }

  // -------------------------------------------------------------------------
  // External-change forwarding
  // -------------------------------------------------------------------------

  /** Called by the workspace watcher on `wast.json` change. Fires a
   * `Changed` event for every open `wast://` URI under that directory. */
  notifyExternalChange(changedDirUri: vscode.Uri): void {
    const events: vscode.FileChangeEvent[] = [];
    const target = changedDirUri.toString();
    for (const doc of vscode.workspace.textDocuments) {
      if (doc.uri.scheme !== "wast") continue;
      const decoded = decodeDirUri(doc.uri);
      if (decoded?.toString() === target) {
        events.push({ type: vscode.FileChangeType.Changed, uri: doc.uri });
      }
    }
    if (events.length > 0) this._onDidChangeFile.fire(events);
  }

  /** Force-refresh all open wast:// docs (e.g. after a settings change). */
  notifyAllChanged(): void {
    const events: vscode.FileChangeEvent[] = [];
    for (const doc of vscode.workspace.textDocuments) {
      if (doc.uri.scheme === "wast") {
        events.push({ type: vscode.FileChangeType.Changed, uri: doc.uri });
      }
    }
    if (events.length > 0) this._onDidChangeFile.fire(events);
  }
}

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

function renderErrorBanner(stage: string, err: unknown): string {
  const errs = extractErrors(err);
  const lines = [
    `# ${stage} failed (${errs.length} error${errs.length === 1 ? "" : "s"}):`,
  ];
  for (const e of errs) {
    const loc = e.location ? ` [${e.location}]` : "";
    lines.push(`#   ${e.message}${loc}`);
  }
  return lines.join("\n") + "\n";
}

function saveError(
  stage: string,
  errs: { message: string; location: string | null }[],
): vscode.FileSystemError {
  const summary = errs
    .map((e) => `${e.message}${e.location ? ` [${e.location}]` : ""}`)
    .join("; ");
  return vscode.FileSystemError.NoPermissions(`${stage} failed: ${summary}`);
}

// ---------------------------------------------------------------------------
// URI codec
// ---------------------------------------------------------------------------

export function encodeDir(dirUri: vscode.Uri): string {
  return Buffer.from(dirUri.toString(), "utf-8").toString("base64url");
}

export function decodeDirUri(uri: vscode.Uri): vscode.Uri | null {
  try {
    return vscode.Uri.parse(
      Buffer.from(uri.authority, "base64url").toString("utf-8"),
    );
  } catch {
    return null;
  }
}

function parseFuncTargets(uri: vscode.Uri): string[] {
  return new URLSearchParams(uri.query).getAll("func");
}

/** Build a wast:// URI for a component, optionally narrowed to func uids. */
export function buildUri(
  component: LoadedComponent,
  funcUids?: string[],
): vscode.Uri {
  const encoded = encodeDir(component.dirUri);
  let query = "";
  if (funcUids && funcUids.length > 0) {
    const params = new URLSearchParams();
    for (const uid of funcUids) params.append("func", uid);
    query = params.toString();
  }
  return vscode.Uri.parse(`wast://${encoded}/component${query ? "?" + query : ""}`);
}

/** Build a tab title for a virtual document. */
export function buildTitle(component: LoadedComponent, funcUids?: string[]): string {
  if (!funcUids || funcUids.length === 0) return component.name;
  const names = funcUids.map((uid) => {
    const f = component.funcs.find((fn) => fn.uid === uid);
    return f?.displayName ?? uid;
  });
  return `${component.name} — ${names.join(", ")}`;
}
