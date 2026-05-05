import * as vscode from "vscode";
import { WastTreeProvider } from "./tree-provider.js";
import { WastDocumentProvider, buildUri, buildTitle } from "./document-provider.js";
import { loadRuntime } from "./wasm-loader.js";
import type { LoadedComponent } from "./wast-db.js";

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  // Load the bundled wasm components. If this fails we fall back to a
  // degraded mode where the tree still works but documents render error
  // banners — better than refusing to activate.
  let runtime;
  try {
    runtime = await loadRuntime(context);
  } catch (err) {
    void vscode.window.showErrorMessage(
      `WAST: failed to load wasm components — ${err instanceof Error ? err.message : err}`,
    );
    return;
  }

  // ── TreeView provider ──
  const treeProvider = new WastTreeProvider();
  const treeView = vscode.window.createTreeView("wastComponents", {
    treeDataProvider: treeProvider,
    showCollapseAll: true,
  });
  context.subscriptions.push(treeView);

  // ── Virtual document provider ──
  const docProvider = new WastDocumentProvider(runtime);
  context.subscriptions.push(
    vscode.workspace.registerTextDocumentContentProvider("wast", docProvider),
  );

  // ── Command: open virtual document for a component / func ──
  context.subscriptions.push(
    vscode.commands.registerCommand(
      "wast.openVirtualDoc",
      async (component: LoadedComponent, funcUid?: string) => {
        const funcUids = funcUid ? [funcUid] : undefined;
        const uri = buildUri(component, funcUids);
        const _title = buildTitle(component, funcUids);
        const doc = await vscode.workspace.openTextDocument(uri);
        await vscode.window.showTextDocument(doc, {
          preview: false,
          viewColumn: vscode.ViewColumn.One,
        });
      },
    ),
  );

  // ── Command: refresh tree ──
  context.subscriptions.push(
    vscode.commands.registerCommand("wast.refreshTree", async () => {
      await treeProvider.refresh();
    }),
  );

  // ── Re-render virtual docs when the user picks a different syntax plugin ──
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("wast.syntaxPlugin") || e.affectsConfiguration("wast.symsLanguage")) {
        for (const doc of vscode.workspace.textDocuments) {
          if (doc.uri.scheme === "wast") docProvider.fireChange(doc.uri);
        }
      }
    }),
  );

  // ── File system watcher for wast.json changes ──
  const watcher = vscode.workspace.createFileSystemWatcher("**/wast.json");
  watcher.onDidChange((uri) => {
    void treeProvider.refresh();
    notifyOpenDocuments(docProvider, uri);
  });
  watcher.onDidCreate(() => void treeProvider.refresh());
  watcher.onDidDelete(() => void treeProvider.refresh());
  context.subscriptions.push(watcher);
}

/** When wast.json changes externally, fire change events for any open
 * virtual documents that belong to the same component directory. */
function notifyOpenDocuments(
  docProvider: WastDocumentProvider,
  changedDbUri: vscode.Uri,
): void {
  const changedDirStr = vscode.Uri.joinPath(changedDbUri, "..").toString();
  for (const doc of vscode.workspace.textDocuments) {
    if (doc.uri.scheme !== "wast") continue;
    let docDirStr: string;
    try {
      docDirStr = Buffer.from(doc.uri.authority, "base64url").toString("utf-8");
    } catch {
      continue;
    }
    if (docDirStr === changedDirStr) {
      docProvider.fireChange(doc.uri);
      void vscode.window.showInformationMessage(
        "WAST: wast.json changed externally, refreshing view.",
      );
    }
  }
}

export function deactivate(): void {
  // Disposables are tracked via context.subscriptions.
}
