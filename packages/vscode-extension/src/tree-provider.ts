/**
 * TreeView data provider for WAST components.
 *
 * Scans workspace folders for directories containing wast.json, then shows
 * each component as a parent node with its functions as children.
 */

import * as vscode from "vscode";
import * as fs from "node:fs";
import * as path from "node:path";
import { type LoadedComponent, type LoadedFunc, readComponent } from "./wast-db.js";

// ---------------------------------------------------------------------------
// Tree item types
// ---------------------------------------------------------------------------

class ComponentItem extends vscode.TreeItem {
  constructor(public readonly component: LoadedComponent) {
    super(component.name, vscode.TreeItemCollapsibleState.Expanded);
    this.contextValue = "wastComponent";
    this.tooltip = component.dir;
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

    // Clicking a function opens the virtual document for the component
    this.command = {
      command: "wast.openVirtualDoc",
      title: "Open WAST Component",
      arguments: [component, func.uid],
    };
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

  constructor() {
    this.scanWorkspace();
  }

  refresh(): void {
    this.scanWorkspace();
    this._onDidChangeTreeData.fire();
  }

  getTreeItem(element: WastTreeItem): vscode.TreeItem {
    return element;
  }

  getChildren(element?: WastTreeItem): WastTreeItem[] {
    if (!element) {
      // Root level: return component nodes
      return this.components.map((c) => new ComponentItem(c));
    }

    if (element instanceof ComponentItem) {
      return element.component.funcs.map((f) => new FuncItem(element.component, f));
    }

    return [];
  }

  // ---------------------------------------------------------------------------
  // Workspace scanning
  // ---------------------------------------------------------------------------

  private scanWorkspace(): void {
    this.components = [];
    const lang = vscode.workspace.getConfiguration("wast").get<string>("symsLanguage", "en");

    const folders = vscode.workspace.workspaceFolders;
    if (!folders) return;

    for (const folder of folders) {
      this.scanDir(folder.uri.fsPath, lang, 0);
    }
  }

  /**
   * Recursively scan for directories containing wast.json, up to a depth limit.
   */
  private scanDir(dir: string, lang: string, depth: number): void {
    if (depth > 5) return;

    const dbPath = path.join(dir, "wast.json");
    if (fs.existsSync(dbPath)) {
      const component = readComponent(dir, lang);
      if (component) {
        this.components.push(component);
      }
      // Don't recurse into component dirs (wast.json marks a leaf)
      return;
    }

    // Recurse into subdirectories
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }

    for (const entry of entries) {
      if (entry.isDirectory() && !entry.name.startsWith(".") && entry.name !== "node_modules") {
        this.scanDir(path.join(dir, entry.name), lang, depth + 1);
      }
    }
  }
}
