/**
 * TreeView data provider for WAST components.
 *
 * Walks workspace folders via `vscode.workspace.fs` (works in both desktop
 * and web hosts) looking for directories that contain a `wast.json`. Each
 * such directory is one component; its functions appear as children.
 *
 * Each func row has a checkbox (multi-select) plus an inline `+callers`
 * toggle. The view-title "Open" button reads the current selection and
 * opens one virtual document covering all checked funcs (with their
 * per-func `+callers` flag). See `SelectionState` below.
 */

import * as vscode from "vscode";
import { type LoadedComponent, type LoadedFunc, readComponent } from "./wast-db.js";

// ---------------------------------------------------------------------------
// SelectionState — per-component checkbox + +callers toggle store.
// ---------------------------------------------------------------------------

export interface FuncToggle {
  show: boolean;
  withCallers: boolean;
}

/** Tracks which funcs are checked and whether each one wants callers in.
 *  Keyed by `${componentDirUri}|${funcUid}` so multiple components in the
 *  same workspace don't clobber each other's selection. */
export class SelectionState {
  private state = new Map<string, FuncToggle>();
  private listeners: Array<() => void> = [];

  private key(component: LoadedComponent, uid: string): string {
    return `${component.dirUri.toString()}|${uid}`;
  }

  isShown(component: LoadedComponent, uid: string): boolean {
    return this.state.get(this.key(component, uid))?.show ?? false;
  }

  hasCallers(component: LoadedComponent, uid: string): boolean {
    return this.state.get(this.key(component, uid))?.withCallers ?? false;
  }

  setShow(component: LoadedComponent, uid: string, value: boolean): void {
    const k = this.key(component, uid);
    const cur = this.state.get(k) ?? { show: false, withCallers: false };
    cur.show = value;
    // Unchecking the row also drops its `+callers` request — a hidden
    // func with `withCallers=true` would just be confusing noise.
    if (!value) cur.withCallers = false;
    this.state.set(k, cur);
    this.notify();
  }

  toggleCallers(component: LoadedComponent, uid: string): void {
    const k = this.key(component, uid);
    const cur = this.state.get(k) ?? { show: false, withCallers: false };
    cur.withCallers = !cur.withCallers;
    // Implicit show — flipping `+callers` on a hidden row is the user
    // asking for that func plus its callers, not just the callers.
    if (cur.withCallers) cur.show = true;
    this.state.set(k, cur);
    this.notify();
  }

  clear(component: LoadedComponent): void {
    const prefix = `${component.dirUri.toString()}|`;
    let touched = false;
    for (const k of [...this.state.keys()]) {
      if (k.startsWith(prefix)) {
        this.state.delete(k);
        touched = true;
      }
    }
    if (touched) this.notify();
  }

  /** All checked funcs for a component, in declaration order. */
  selectionFor(component: LoadedComponent): { uid: string; withCallers: boolean }[] {
    const out: { uid: string; withCallers: boolean }[] = [];
    for (const f of component.funcs) {
      const t = this.state.get(this.key(component, f.uid));
      if (t?.show) out.push({ uid: f.uid, withCallers: t.withCallers });
    }
    return out;
  }

  onChange(cb: () => void): vscode.Disposable {
    this.listeners.push(cb);
    return new vscode.Disposable(() => {
      const i = this.listeners.indexOf(cb);
      if (i >= 0) this.listeners.splice(i, 1);
    });
  }

  private notify(): void {
    for (const cb of this.listeners) cb();
  }
}

// ---------------------------------------------------------------------------
// Tree items
// ---------------------------------------------------------------------------

class ComponentItem extends vscode.TreeItem {
  constructor(public readonly component: LoadedComponent) {
    super(component.name, vscode.TreeItemCollapsibleState.Expanded);
    this.contextValue = "wastComponent";
    this.tooltip = component.dirUri.fsPath;
    this.iconPath = new vscode.ThemeIcon("package");
  }
}

class FuncItem extends vscode.TreeItem {
  constructor(
    public readonly component: LoadedComponent,
    public readonly func: LoadedFunc,
    selection: SelectionState,
  ) {
    const label = func.displayName ?? func.uid;
    super(label, vscode.TreeItemCollapsibleState.None);
    const withCallers = selection.hasCallers(component, func.uid);
    this.contextValue = withCallers ? "wastFuncWithCallers" : "wastFunc";
    this.description = withCallers ? `${func.sourceType} · +callers` : func.sourceType;
    this.tooltip = `${func.uid} (${func.sourceType})`;
    this.iconPath = new vscode.ThemeIcon("symbol-function");
    this.checkboxState = selection.isShown(component, func.uid)
      ? vscode.TreeItemCheckboxState.Checked
      : vscode.TreeItemCheckboxState.Unchecked;
  }
}

type WastTreeItem = ComponentItem | FuncItem;

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

export class WastTreeProvider implements vscode.TreeDataProvider<WastTreeItem> {
  private _onDidChangeTreeData = new vscode.EventEmitter<WastTreeItem | undefined | void>();
  readonly onDidChangeTreeData = this._onDidChangeTreeData.event;

  private components: LoadedComponent[] = [];
  /** Initial scan promise so consumers can await readiness if needed. */
  private scanPromise: Promise<void> = Promise.resolve();

  constructor(public readonly selection: SelectionState = new SelectionState()) {
    this.scanPromise = this.scanWorkspace();
    // Re-render on any selection edit so `+callers` description toggles
    // appear immediately without waiting for a full workspace refresh.
    this.selection.onChange(() => this._onDidChangeTreeData.fire());
  }

  /** Re-scan the workspace; resolves once components have been re-loaded. */
  async refresh(): Promise<void> {
    this.scanPromise = this.scanWorkspace();
    await this.scanPromise;
    this._onDidChangeTreeData.fire();
  }

  getTreeItem(element: WastTreeItem): vscode.TreeItem {
    return element;
  }

  async getChildren(element?: WastTreeItem): Promise<WastTreeItem[]> {
    // Wait for the initial scan before serving children — VS Code calls
    // getChildren synchronously after createTreeView, so on first call
    // `this.components` may still be empty.
    await this.scanPromise;

    if (!element) {
      return this.components.map((c) => new ComponentItem(c));
    }
    if (element instanceof ComponentItem) {
      return element.component.funcs.map(
        (f) => new FuncItem(element.component, f, this.selection),
      );
    }
    return [];
  }

  /** Find a loaded component by directory URI (used by other providers). */
  findByDir(dirUri: vscode.Uri): LoadedComponent | undefined {
    return this.components.find((c) => c.dirUri.toString() === dirUri.toString());
  }

  /** All components currently loaded — used by the view-title "Open"
   *  command to walk every component and open one virtual doc per
   *  component that has a non-empty selection. */
  getComponents(): readonly LoadedComponent[] {
    return this.components;
  }

  // ---------------------------------------------------------------------------
  // Workspace scanning
  // ---------------------------------------------------------------------------

  private async scanWorkspace(): Promise<void> {
    this.components = [];
    const lang = vscode.workspace.getConfiguration("wast").get<string>("symsLanguage", "en");
    const folders = vscode.workspace.workspaceFolders;
    if (!folders) return;

    for (const folder of folders) {
      await this.scanDir(folder.uri, lang, 0);
    }
  }

  /** Recursively walk subtree until either a `wast.json` is found (component
   * leaf, no further descent) or the depth limit is hit. Skips `.git` and
   * `node_modules` for sanity. */
  private async scanDir(dirUri: vscode.Uri, lang: string, depth: number): Promise<void> {
    if (depth > 5) return;

    const dbUri = vscode.Uri.joinPath(dirUri, "wast.json");
    let dbExists = false;
    try {
      await vscode.workspace.fs.stat(dbUri);
      dbExists = true;
    } catch {
      // not a component directory
    }

    if (dbExists) {
      const component = await readComponent(dirUri, lang);
      if (component) this.components.push(component);
      return;
    }

    let entries: [string, vscode.FileType][];
    try {
      entries = await vscode.workspace.fs.readDirectory(dirUri);
    } catch {
      return;
    }

    for (const [name, kind] of entries) {
      if (kind !== vscode.FileType.Directory) continue;
      if (name.startsWith(".") || name === "node_modules") continue;
      await this.scanDir(vscode.Uri.joinPath(dirUri, name), lang, depth + 1);
    }
  }
}
