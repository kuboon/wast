/**
 * `FileSystemProvider` for the `wast://` scheme.
 *
 * URI format: `wast:/<base64url(dir-uri)>/component[?func=uid1&func=uid2]`
 *
 * The encoded dir lives in the URI path (not the authority) because
 * RFC 3986 authorities are case-insensitive and VS Code lowercases them,
 * which corrupts base64url payloads.
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

  /** Newest underlying mtime observed when each component dir was last
   *  rendered (readFile) or saved (writeFile). Keyed by dir URI string.
   *  Used by writeFile to detect edits that landed on disk in between —
   *  see the conflict check there. */
  private lastSeenMtime = new Map<string, number>();

  constructor(private runtime: LoadedRuntime) {}

  watch(_uri: vscode.Uri): vscode.Disposable {
    // We watch the underlying wast.json centrally (in extension.ts) and
    // forward those events via `notifyExternalChange` below — there's no
    // per-URI watcher to set up here.
    return new vscode.Disposable(() => {});
  }

  /** Newest mtime across the files a virtual doc is rendered from
   *  (wast.json + syms.<lang>.yaml). `undefined` if none exist. */
  private async underlyingMtime(
    dirUri: vscode.Uri,
    lang: string,
  ): Promise<number | undefined> {
    let newest: number | undefined;
    for (const name of ["wast.json", `syms.${lang}.yaml`]) {
      try {
        const st = await vscode.workspace.fs.stat(vscode.Uri.joinPath(dirUri, name));
        newest = newest === undefined ? st.mtime : Math.max(newest, st.mtime);
      } catch {
        // file absent — syms is optional, wast.json absence surfaces later
      }
    }
    return newest;
  }

  async stat(uri: vscode.Uri): Promise<vscode.FileStat> {
    // Report the real ctime/mtime of the underlying wast.json (and syms)
    // via workspace.fs so VS Code's own modified-since tracking works —
    // returning constant zeros would make every external edit invisible
    // to the editor's conflict handling. Size is advisory (the rendered
    // text length differs from the on-disk JSON anyway).
    const dirUri = decodeDirUri(uri);
    if (!dirUri) {
      throw vscode.FileSystemError.FileNotFound(`invalid wast:// URI: ${uri}`);
    }
    const lang = vscode.workspace
      .getConfiguration("wast")
      .get<string>("symsLanguage", "en");
    try {
      const st = await vscode.workspace.fs.stat(
        vscode.Uri.joinPath(dirUri, "wast.json"),
      );
      const mtime = (await this.underlyingMtime(dirUri, lang)) ?? st.mtime;
      return { type: vscode.FileType.File, ctime: st.ctime, mtime, size: st.size };
    } catch {
      throw vscode.FileSystemError.FileNotFound(
        `wast.json not found in ${dirUri.fsPath}`,
      );
    }
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

    // Record the underlying mtime BEFORE reading the content. If a write
    // races in between, we'll have read the newer content but recorded the
    // older mtime — which at worst causes a spurious conflict error on the
    // next save. Statting after the read would invert that: old content
    // with a new mtime, silently overwriting the racing edit on save.
    const mtime = await this.underlyingMtime(dirUri, lang);
    if (mtime !== undefined) {
      this.lastSeenMtime.set(dirUri.toString(), mtime);
    }

    const loaded = await readComponent(dirUri, lang);
    if (!loaded) {
      throw vscode.FileSystemError.FileNotFound(
        `wast.json not readable in ${dirUri.fsPath}`,
      );
    }
    const full = toWastComponent(loaded);

    const targets = parseSelection(uri);
    let view = full;
    if (targets.length > 0) {
      try {
        view = this.runtime.partialManager.extract(full, targets);
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

    // Conflict check: if wast.json / syms changed on disk after this view
    // was last rendered, a blind save would merge the pane text into the
    // *new* disk state and silently clobber whoever wrote it. Fail the
    // save instead and let the user decide. We bump the recorded mtime so
    // a deliberate second save (after the warning) does overwrite.
    const dirKey = dirUri.toString();
    const lastSeen = this.lastSeenMtime.get(dirKey);
    const currentMtime = await this.underlyingMtime(dirUri, lang);
    if (
      lastSeen !== undefined &&
      currentMtime !== undefined &&
      currentMtime > lastSeen
    ) {
      this.lastSeenMtime.set(dirKey, currentMtime);
      throw saveError("conflict", [
        {
          message:
            `wast.json in ${dirUri.fsPath} changed on disk since this view was opened. ` +
            "Revert the editor to load the new content, or save again to overwrite it.",
          location: null,
        },
      ]);
    }

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

    // Our own write is now the latest state — record its mtime so the next
    // save doesn't flag it as a conflict.
    const written = await this.underlyingMtime(dirUri, lang);
    if (written !== undefined) {
      this.lastSeenMtime.set(dirKey, written);
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

// base64url helpers built on TextEncoder/TextDecoder + atob/btoa so they
// work in both the desktop (Node) and web extension hosts — `Buffer` is
// Node-only and breaks the browser build.

function base64UrlEncode(text: string): string {
  const bytes = new TextEncoder().encode(text);
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function base64UrlDecode(encoded: string): string {
  const b64 =
    encoded.replace(/-/g, "+").replace(/_/g, "/") +
    "=".repeat((4 - (encoded.length % 4)) % 4);
  const bin = atob(b64);
  const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
  return new TextDecoder("utf-8").decode(bytes);
}

export function encodeDir(dirUri: vscode.Uri): string {
  return base64UrlEncode(dirUri.toString());
}

export function decodeDirUri(uri: vscode.Uri): vscode.Uri | null {
  try {
    // path is "/<encoded>/component" — split and grab the first segment.
    const segments = uri.path.split("/").filter((s) => s.length > 0);
    if (segments.length === 0) return null;
    return vscode.Uri.parse(base64UrlDecode(segments[0]));
  } catch {
    return null;
  }
}

/** A single func included in a partial view, plus whether to drag in
 *  its callers (matches `partial-manager.extract`'s target shape). */
export interface FuncSelection {
  uid: string;
  withCallers: boolean;
}

/** Parse `?func=A&func=B&funcc=C` into partial-manager targets.
 *
 *  - `func=uid`  → show without callers
 *  - `funcc=uid` → show *with* callers (`+callers` checkbox in the tree)
 *
 *  If a uid appears in both lists, `funcc` wins (the more inclusive view). */
export function parseSelection(uri: vscode.Uri): { sym: string; includeCaller: boolean }[] {
  const params = new URLSearchParams(uri.query);
  const withCallers = new Set(params.getAll("funcc"));
  const out: { sym: string; includeCaller: boolean }[] = [];
  for (const uid of withCallers) out.push({ sym: uid, includeCaller: true });
  for (const uid of params.getAll("func")) {
    if (!withCallers.has(uid)) out.push({ sym: uid, includeCaller: false });
  }
  return out;
}

/** Build a wast:// URI for a component, optionally narrowed to a selection. */
export function buildUri(
  component: LoadedComponent,
  selection?: FuncSelection[],
): vscode.Uri {
  const encoded = encodeDir(component.dirUri);
  let query = "";
  if (selection && selection.length > 0) {
    const params = new URLSearchParams();
    for (const { uid, withCallers } of selection) {
      params.append(withCallers ? "funcc" : "func", uid);
    }
    query = params.toString();
  }
  return vscode.Uri.parse(`wast:/${encoded}/component${query ? "?" + query : ""}`);
}

/** Build a tab title for a virtual document. */
export function buildTitle(
  component: LoadedComponent,
  selection?: FuncSelection[],
): string {
  if (!selection || selection.length === 0) return component.name;
  const names = selection.map(({ uid, withCallers }) => {
    const f = component.funcs.find((fn) => fn.uid === uid);
    const base = f?.displayName ?? uid;
    return withCallers ? `${base}+callers` : base;
  });
  return `${component.name} — ${names.join(", ")}`;
}
