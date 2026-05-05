/**
 * TreeView data provider for WAST components.
 *
 * Walks workspace folders via `vscode.workspace.fs` (works in both desktop
 * and web hosts) looking for directories that contain a `wast.json`. Each
 * such directory is one component; its functions appear as children.
 */

import * as vscode from "vscode";
import { type LoadedComponent, type LoadedFunc, readComponent } from "./wast-db.js";

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
  ) {
    const label = func.displayName ?? func.uid;
    super(label, vscode.TreeItemCollapsibleState.None);
    this.contextValue = "wastFunc";
    this.description = func.sourceType;
    this.tooltip = `${func.uid} (${func.sourceType})`;
    this.iconPath = new vscode.ThemeIcon("symbol-function");
    this.command = {
      command: "wast.openVirtualDoc",
      title: "Open WAST Component",
      arguments: [component, func.uid],
    };
  }
}

type WastTreeItem = ComponentItem | FuncItem;

export class WastTreeProvider implements vscode.TreeDataProvider<WastTreeItem> {
  private _onDidChangeTreeData = new vscode.EventEmitter<WastTreeItem | undefined | void>();
  readonly onDidChangeTreeData = this._onDidChangeTreeData.event;

  private components: LoadedComponent[] = [];
  /** Initial scan promise so consumers can await readiness if needed. */
  private scanPromise: Promise<void> = Promise.resolve();

  constructor() {
    this.scanPromise = this.scanWorkspace();
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
      return element.component.funcs.map((f) => new FuncItem(element.component, f));
    }
    return [];
  }

  /** Find a loaded component by directory URI (used by other providers). */
  findByDir(dirUri: vscode.Uri): LoadedComponent | undefined {
    return this.components.find((c) => c.dirUri.toString() === dirUri.toString());
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
