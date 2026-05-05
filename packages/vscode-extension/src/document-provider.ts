/**
 * Virtual document provider for the `wast` URI scheme.
 *
 * URI format: `wast://<base64url-dir>/component[?func=uid1&func=uid2]`
 *
 * On open, reads the component (wast.json + syms.<lang>.yaml) and runs the
 * configured syntax plugin's `to_text` to produce real surface text. If
 * `?func=` is present, the partial-manager's `extract` first narrows the
 * component to just those targets so the rendered output is focused.
 */

import * as vscode from "vscode";
import { type LoadedComponent, readComponent, toWastComponent } from "./wast-db.js";
import {
  extractErrors,
  type LoadedRuntime,
  type SyntaxPluginId,
} from "./wasm-loader.js";

export class WastDocumentProvider implements vscode.TextDocumentContentProvider {
  private _onDidChange = new vscode.EventEmitter<vscode.Uri>();
  readonly onDidChange = this._onDidChange.event;

  constructor(private runtime: LoadedRuntime) {}

  fireChange(uri: vscode.Uri): void {
    this._onDidChange.fire(uri);
  }

  async provideTextDocumentContent(uri: vscode.Uri): Promise<string> {
    const dirUri = decodeDirUri(uri);
    if (!dirUri) return "# Error: invalid wast:// URI\n";

    const cfg = vscode.workspace.getConfiguration("wast");
    const lang = cfg.get<string>("symsLanguage", "en");
    const pluginId = cfg.get<SyntaxPluginId>("syntaxPlugin", "ruby-like");

    const component = await readComponent(dirUri, lang);
    if (!component) {
      return `# Error: could not read wast.json in ${dirUri.fsPath}\n`;
    }

    const wastComponent = toWastComponent(component);

    // Narrow to requested funcs via partial-manager.extract so the syntax
    // plugin sees a self-consistent partial (callees pulled in as imports,
    // unselected callers stay outside).
    const requestedUids = new URLSearchParams(uri.query).getAll("func");
    let viewComponent = wastComponent;
    if (requestedUids.length > 0) {
      try {
        viewComponent = this.runtime.partialManager.extract(
          wastComponent,
          requestedUids.map((sym) => ({ sym, includeCaller: false })),
        );
      } catch (err) {
        return renderErrorBanner("partial-manager.extract", err);
      }
    }

    const plugin = this.runtime.syntaxPlugins[pluginId];
    if (!plugin) {
      return `# Error: syntax plugin '${pluginId}' not loaded\n`;
    }

    try {
      return plugin.toText(viewComponent);
    } catch (err) {
      return renderErrorBanner(`${pluginId}.to_text`, err);
    }
  }
}

function renderErrorBanner(stage: string, err: unknown): string {
  const errs = extractErrors(err);
  const lines = [`# ${stage} failed (${errs.length} error${errs.length === 1 ? "" : "s"}):`];
  for (const e of errs) {
    const loc = e.location ? ` [${e.location}]` : "";
    lines.push(`#   ${e.message}${loc}`);
  }
  return lines.join("\n") + "\n";
}

// ---------------------------------------------------------------------------
// URI encoding/decoding
// ---------------------------------------------------------------------------

export function encodeDir(dirUri: vscode.Uri): string {
  return Buffer.from(dirUri.toString(), "utf-8").toString("base64url");
}

function decodeDirUri(uri: vscode.Uri): vscode.Uri | null {
  try {
    const decoded = Buffer.from(uri.authority, "base64url").toString("utf-8");
    return vscode.Uri.parse(decoded);
  } catch {
    return null;
  }
}

/** Build a wast:// URI for a component, optionally narrowed to func uids. */
export function buildUri(component: LoadedComponent, funcUids?: string[]): vscode.Uri {
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
