/**
 * Virtual document provider for the `wast` URI scheme.
 *
 * URI format: wast://component/{dir-path-base64}[?func=uid1&func=uid2]
 *
 * When opened, reads the component's wast.json + syms and formats a simple
 * text representation of the requested functions (or all functions if none
 * specified).
 */

import * as vscode from "vscode";
import { type LoadedComponent, type LoadedFunc, readComponent } from "./wast-db.js";

export class WastDocumentProvider implements vscode.TextDocumentContentProvider {
  private _onDidChange = new vscode.EventEmitter<vscode.Uri>();
  readonly onDidChange = this._onDidChange.event;

  /** Notify VS Code that a virtual document's content has changed. */
  fireChange(uri: vscode.Uri): void {
    this._onDidChange.fire(uri);
  }

  provideTextDocumentContent(uri: vscode.Uri): string {
    const dir = decodeDir(uri);
    if (!dir) return "# Error: invalid wast:// URI\n";

    const lang = vscode.workspace.getConfiguration("wast").get<string>("symsLanguage", "en");
    const component = readComponent(dir, lang);
    if (!component) return `# Error: could not read wast.json in ${dir}\n`;

    // Determine which functions to show
    const requestedUids = new URLSearchParams(uri.query).getAll("func");
    const funcs =
      requestedUids.length > 0
        ? component.funcs.filter((f) => requestedUids.includes(f.uid))
        : component.funcs;

    return formatComponent(component, funcs);
  }
}

// ---------------------------------------------------------------------------
// URI encoding/decoding
// ---------------------------------------------------------------------------

export function encodeDir(dir: string): string {
  return Buffer.from(dir, "utf-8").toString("base64url");
}

function decodeDir(uri: vscode.Uri): string | null {
  try {
    // authority is the base64url-encoded directory path
    return Buffer.from(uri.authority, "base64url").toString("utf-8");
  } catch {
    return null;
  }
}

/**
 * Build a wast:// URI for a component, optionally scoped to specific function UIDs.
 */
export function buildUri(component: LoadedComponent, funcUids?: string[]): vscode.Uri {
  const encoded = encodeDir(component.dir);
  let query = "";
  if (funcUids && funcUids.length > 0) {
    const params = new URLSearchParams();
    for (const uid of funcUids) {
      params.append("func", uid);
    }
    query = params.toString();
  }
  return vscode.Uri.parse(`wast://${encoded}/component${query ? "?" + query : ""}`);
}

/**
 * Build a tab title for a virtual document.
 */
export function buildTitle(component: LoadedComponent, funcUids?: string[]): string {
  if (!funcUids || funcUids.length === 0) {
    return component.name;
  }

  const names = funcUids.map((uid) => {
    const f = component.funcs.find((fn) => fn.uid === uid);
    return f?.displayName ?? uid;
  });

  return `${component.name} \u2014 ${names.join(", ")}`;
}

// ---------------------------------------------------------------------------
// Text formatting
// ---------------------------------------------------------------------------

function formatComponent(component: LoadedComponent, funcs: LoadedFunc[]): string {
  const lines: string[] = [];
  lines.push(`# ${component.name}`);
  lines.push(`# dir: ${component.dir}`);
  lines.push("");

  if (funcs.length === 0) {
    lines.push("# (no functions)");
    return lines.join("\n");
  }

  for (const f of funcs) {
    lines.push(formatFunc(f));
    lines.push("");
  }

  return lines.join("\n");
}

function formatFunc(f: LoadedFunc): string {
  const lines: string[] = [];

  const name = f.displayName ?? f.uid;
  const tag = f.sourceType;

  // Signature line
  const params = f.params.map(([pname, ptype]) => `${pname}: ${ptype}`).join(", ");
  const ret = f.result ? ` -> ${f.result}` : "";
  lines.push(`# [${tag}]`);
  lines.push(`func ${name}(${params})${ret}`);

  // Body placeholder
  lines.push("  # [body not available — requires syntax plugin]");
  lines.push("end");

  return lines.join("\n");
}
