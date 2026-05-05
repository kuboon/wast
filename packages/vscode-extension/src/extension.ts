import * as vscode from "vscode";
import { WastTreeProvider } from "./tree-provider.js";
import {
  WastFileSystemProvider,
  buildUri,
  buildTitle,
} from "./wast-fs-provider.js";
import { loadRuntime } from "./wasm-loader.js";
import type { LoadedComponent } from "./wast-db.js";

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  // Load the bundled wasm components. If this fails we surface an error
  // and bail — the extension does nothing useful without them.
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

  // ── wast:// FileSystemProvider (read = to_text, write = from_text → merge → codec) ──
  const fsProvider = new WastFileSystemProvider(runtime);
  context.subscriptions.push(
    vscode.workspace.registerFileSystemProvider("wast", fsProvider, {
      isCaseSensitive: true,
      // Required for writeFile to be called on save.
      isReadonly: false,
    }),
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
      if (
        e.affectsConfiguration("wast.syntaxPlugin") ||
        e.affectsConfiguration("wast.symsLanguage")
      ) {
        fsProvider.notifyAllChanged();
      }
    }),
  );

  // ── File system watcher for wast.json changes ──
  const watcher = vscode.workspace.createFileSystemWatcher("**/wast.json");
  watcher.onDidChange((uri) => {
    void treeProvider.refresh();
    fsProvider.notifyExternalChange(vscode.Uri.joinPath(uri, ".."));
  });
  watcher.onDidCreate(() => void treeProvider.refresh());
  watcher.onDidDelete(() => void treeProvider.refresh());
  context.subscriptions.push(watcher);
}

export function deactivate(): void {
  // Disposables are tracked via context.subscriptions.
}
