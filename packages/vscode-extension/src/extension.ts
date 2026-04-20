import * as vscode from "vscode";
import { WastTreeProvider } from "./tree-provider.js";
import { WastDocumentProvider, buildUri, buildTitle } from "./document-provider.js";
import type { LoadedComponent } from "./wast-db.js";

export function activate(context: vscode.ExtensionContext) {
  // ── TreeView provider ──
  const treeProvider = new WastTreeProvider();
  const treeView = vscode.window.createTreeView("wastComponents", {
    treeDataProvider: treeProvider,
    showCollapseAll: true,
  });
  context.subscriptions.push(treeView);

  // ── Virtual document provider ──
  const docProvider = new WastDocumentProvider();
  context.subscriptions.push(
    vscode.workspace.registerTextDocumentContentProvider("wast", docProvider),
  );

  // ── Command: open virtual document for a component ──
  context.subscriptions.push(
    vscode.commands.registerCommand(
      "wast.openVirtualDoc",
      async (component: LoadedComponent, funcUid?: string) => {
        const funcUids = funcUid ? [funcUid] : undefined;
        const uri = buildUri(component, funcUids);
        const title = buildTitle(component, funcUids);
        const doc = await vscode.workspace.openTextDocument(uri);
        await vscode.window.showTextDocument(doc, {
          preview: false,
          viewColumn: vscode.ViewColumn.One,
        });
        // Set the tab title (best-effort — VS Code uses the URI path by default)
        void title; // title is available for future use with custom editors
      },
    ),
  );

  // ── Command: refresh tree ──
  context.subscriptions.push(
    vscode.commands.registerCommand("wast.refreshTree", () => {
      treeProvider.refresh();
    }),
  );

  // ── File system watcher for wast.json changes ──
  const watcher = vscode.workspace.createFileSystemWatcher("**/wast.json");

  watcher.onDidChange((uri) => {
    treeProvider.refresh();
    // Notify any open virtual documents that reference the changed component
    notifyOpenDocuments(docProvider, uri);
  });

  watcher.onDidCreate(() => {
    treeProvider.refresh();
  });

  watcher.onDidDelete(() => {
    treeProvider.refresh();
  });

  context.subscriptions.push(watcher);
}

/**
 * When a wast.json file changes, fire change events for any open virtual
 * documents that belong to the same component directory, and optionally
 * show a notification.
 */
function notifyOpenDocuments(
  docProvider: WastDocumentProvider,
  changedDbUri: vscode.Uri,
): void {
  const changedDir = vscode.Uri.joinPath(changedDbUri, "..").fsPath;

  for (const doc of vscode.workspace.textDocuments) {
    if (doc.uri.scheme !== "wast") continue;

    // Decode the directory from the wast:// URI authority
    let docDir: string;
    try {
      docDir = Buffer.from(doc.uri.authority, "base64url").toString("utf-8");
    } catch {
      continue;
    }

    if (docDir === changedDir) {
      docProvider.fireChange(doc.uri);
      void vscode.window.showInformationMessage(
        `WAST: wast.json changed externally, refreshing view.`,
      );
    }
  }
}

export function deactivate() {
  // Nothing to clean up — all disposables are registered via context.subscriptions
}
