import * as vscode from "vscode";
import { WastTreeProvider } from "./tree-provider.js";
import {
  WastFileSystemProvider,
  buildUri,
  buildTitle,
} from "./wast-fs-provider.js";
import { extractErrors, loadRuntime, type LoadedRuntime } from "./wasm-loader.js";
import {
  type LoadedComponent,
  readComponent,
  readWorldWit,
  toWastComponent,
} from "./wast-db.js";

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

  // ── Command: compile a component to wasm ──
  context.subscriptions.push(
    vscode.commands.registerCommand(
      "wast.compileComponent",
      async (component?: LoadedComponent) => {
        const target = component ?? (await pickComponent(treeProvider));
        if (!target) return;
        await compileComponent(runtime, target);
      },
    ),
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

// ---------------------------------------------------------------------------
// Compile flow
// ---------------------------------------------------------------------------

async function pickComponent(
  treeProvider: WastTreeProvider,
): Promise<LoadedComponent | undefined> {
  const components = (await treeProvider.getChildren()).flatMap((item) =>
    "component" in item && !("func" in item) ? [item.component] : [],
  );
  if (components.length === 0) {
    void vscode.window.showInformationMessage("WAST: no components found in workspace.");
    return undefined;
  }
  if (components.length === 1) return components[0];
  const picked = await vscode.window.showQuickPick(
    components.map((c) => ({ label: c.name, description: c.dirUri.fsPath, component: c })),
    { placeHolder: "Pick a component to compile" },
  );
  return picked?.component;
}

async function compileComponent(
  runtime: LoadedRuntime,
  component: LoadedComponent,
): Promise<void> {
  const lang = vscode.workspace.getConfiguration("wast").get<string>("symsLanguage", "en");

  // Re-read from disk so we compile what's persisted (not whatever the
  // tree last saw).
  const fresh = await readComponent(component.dirUri, lang);
  if (!fresh) {
    void vscode.window.showErrorMessage(
      `WAST: cannot read wast.json in ${component.dirUri.fsPath}`,
    );
    return;
  }
  const worldWit = await readWorldWit(component.dirUri);
  if (!worldWit) {
    void vscode.window.showErrorMessage(
      `WAST: world.wit missing in ${component.dirUri.fsPath} — required for compile.`,
    );
    return;
  }

  const wastComponent = toWastComponent(fresh);

  let wasm: Uint8Array;
  try {
    wasm = runtime.compiler.compile(wastComponent, worldWit);
  } catch (err) {
    const errs = extractErrors(err);
    const summary = errs
      .map((e) => `${e.message}${e.location ? ` [${e.location}]` : ""}`)
      .join("\n  ");
    void vscode.window.showErrorMessage(`WAST compile failed:\n  ${summary}`);
    return;
  }

  const outUri = vscode.Uri.joinPath(component.dirUri, "dist", `${component.name}.wasm`);
  await vscode.workspace.fs.createDirectory(vscode.Uri.joinPath(component.dirUri, "dist"));
  await vscode.workspace.fs.writeFile(outUri, wasm);
  void vscode.window.showInformationMessage(
    `WAST: compiled ${component.name} → ${outUri.fsPath} (${wasm.byteLength} bytes)`,
  );
}

export function deactivate(): void {
  // Disposables are tracked via context.subscriptions.
}
