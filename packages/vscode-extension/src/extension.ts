import * as vscode from "vscode";
import { WastTreeProvider } from "./tree-provider.js";
import {
  WastFileSystemProvider,
  buildUri,
  buildTitle,
  type FuncSelection,
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
    // Enable multi-select so users can shift-click a range and then hit
    // `WAST: Open selected` from the view title. Without this only one
    // row's checkbox can be flipped at a time via keyboard navigation.
    canSelectMany: true,
  });
  context.subscriptions.push(treeView);

  // VS Code reports checkbox flips here. Funnel them into SelectionState
  // so the tree row's description and view-title button reflect the new
  // state on the very next render.
  context.subscriptions.push(
    treeView.onDidChangeCheckboxState((e) => {
      for (const [item, newState] of e.items) {
        const fi = item as unknown as {
          component?: LoadedComponent;
          func?: { uid: string };
        };
        if (!fi.component || !fi.func) continue;
        treeProvider.selection.setShow(
          fi.component,
          fi.func.uid,
          newState === vscode.TreeItemCheckboxState.Checked,
        );
      }
    }),
  );

  // ── wast:// FileSystemProvider (read = to_text, write = from_text → merge → codec) ──
  const fsProvider = new WastFileSystemProvider(runtime);
  context.subscriptions.push(
    vscode.workspace.registerFileSystemProvider("wast", fsProvider, {
      isCaseSensitive: true,
      // Required for writeFile to be called on save.
      isReadonly: false,
    }),
  );

  // ── Command: open virtual document for a component's current selection ──
  context.subscriptions.push(
    vscode.commands.registerCommand(
      "wast.openVirtualDoc",
      async (component: LoadedComponent, selection?: FuncSelection[]) => {
        await openVirtualDoc(component, selection);
      },
    ),
  );

  // ── Command: refresh tree ──
  context.subscriptions.push(
    vscode.commands.registerCommand("wast.refreshTree", async () => {
      await treeProvider.refresh();
    }),
  );

  // ── Command: toggle +callers on a func row (inline action) ──
  context.subscriptions.push(
    vscode.commands.registerCommand("wast.toggleCallers", (item: unknown) => {
      const fi = item as { component?: LoadedComponent; func?: { uid: string } };
      if (!fi.component || !fi.func) return;
      treeProvider.selection.toggleCallers(fi.component, fi.func.uid);
    }),
  );

  // ── Command: open selected (view-title button) ──
  context.subscriptions.push(
    vscode.commands.registerCommand("wast.openSelected", async () => {
      const components = treeProvider.getComponents();
      const opened: string[] = [];
      for (const c of components) {
        const sel = treeProvider.selection.selectionFor(c);
        if (sel.length === 0) continue;
        await openVirtualDoc(c, sel);
        opened.push(c.name);
      }
      if (opened.length === 0) {
        void vscode.window.showInformationMessage(
          "WAST: no funcs checked — tick the checkboxes on the funcs you want first.",
        );
      }
    }),
  );

  // ── Command: clear selection (view-title button) ──
  context.subscriptions.push(
    vscode.commands.registerCommand("wast.clearSelection", () => {
      for (const c of treeProvider.getComponents()) {
        treeProvider.selection.clear(c);
      }
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
// Open virtual doc helper
// ---------------------------------------------------------------------------

async function openVirtualDoc(
  component: LoadedComponent,
  selection?: FuncSelection[],
): Promise<void> {
  const uri = buildUri(component, selection);
  const _title = buildTitle(component, selection);
  const doc = await vscode.workspace.openTextDocument(uri);
  await vscode.window.showTextDocument(doc, {
    preview: false,
    viewColumn: vscode.ViewColumn.One,
  });
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
