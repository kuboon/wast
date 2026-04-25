// Load the manifest and render a card per demo. Each card dynamically
// imports the corresponding transpiled component module when the user hits
// Run — that keeps the initial page weight small.

const MANIFEST_URL = new URL("../public/components/manifest.json", import.meta.url);

async function loadManifest() {
  const res = await fetch(MANIFEST_URL);
  if (!res.ok) throw new Error(`manifest fetch failed: ${res.status}`);
  return res.json();
}

function moduleUrl(id) {
  // jco transpile writes <id>/<id>.js
  return new URL(`../public/components/${id}/${id}.js`, import.meta.url).href;
}

function h(tag, props = {}, children = []) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (k === "class") el.className = v;
    else if (k === "on" && typeof v === "object") {
      for (const [evt, fn] of Object.entries(v)) el.addEventListener(evt, fn);
    } else el.setAttribute(k, v);
  }
  for (const c of [].concat(children)) {
    if (c == null) continue;
    el.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return el;
}

/** Map a manifest result tag + runtime value to a printable string. */
function formatResult(tag, value) {
  if (value === undefined) return "(void)";
  if (tag === "string") return JSON.stringify(value);
  if (tag?.startsWith("option<")) {
    if (value === null || value === undefined) return "none";
    return `some(${JSON.stringify(value)})`;
  }
  if (tag?.startsWith("list<") || Array.isArray(value)) {
    return JSON.stringify(value);
  }
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

function buildInputField(param) {
  // Each field renders to a single input element. Users edit as JSON so we
  // don't need type-specific widgets for Option / Record / List / Tuple etc.
  const wrapper = h("div", { class: "field" }, [
    h("label", {}, `${param.name}: ${paramTypeLabel(param)}`),
  ]);
  const input = h("input", { type: "text", "data-name": param.name });
  input.value = defaultInputFor(param);
  wrapper.append(input);
  return wrapper;
}

function paramTypeLabel(param) {
  switch (param.kind) {
    case "u32": case "i32": case "u64": case "i64": case "f32": case "f64":
    case "bool": case "char": case "string":
      return param.kind;
    case "option": return `option<${param.inner}>`;
    case "list": return `list<${param.inner}>`;
    case "record": return `record { ${param.fields.map(([n, t]) => `${n}: ${t}`).join(", ")} }`;
    case "tuple": return `tuple<${param.elems.join(", ")}>`;
    case "variant": return `variant<${param.name ?? "..."}>`;
    case "enum": return `enum<${param.name ?? "..."}>`;
    case "flags": return `flags<${param.name ?? "..."}>`;
    default: return param.kind;
  }
}

function defaultInputFor(param) {
  switch (param.kind) {
    case "u32": case "i32": case "u64": case "i64":
    case "f32": case "f64":
      return "0";
    case "bool": return "false";
    case "string": return '""';
    case "option": return "null";
    case "list": return "[]";
    case "record":
      return JSON.stringify(
        Object.fromEntries(param.fields.map(([n]) => [n, 0])),
      );
    case "tuple":
      return JSON.stringify(param.elems.map(() => 0));
    default: return "null";
  }
}

/** Coerce the text input of a param into the JS value jco expects. */
function parseInputField(param, raw) {
  const trimmed = raw.trim();
  switch (param.kind) {
    case "u32": case "i32": case "u64": case "i64":
    case "f32": case "f64":
      return Number(trimmed);
    case "bool":
      return trimmed === "true";
    default:
      // Everything else is a JSON literal: strings, option, list, record,
      // tuple, variant, etc.
      return JSON.parse(trimmed);
  }
}

function buildCard(demo) {
  const card = h("div", { class: "card", "data-id": demo.id });

  card.append(h("h2", {}, [
    h("span", {}, demo.title),
    h("span", { class: "milestone" }, demo.milestone),
  ]));
  card.append(h("p", { class: "desc" }, demo.description));

  const inputs = h("div", { class: "inputs" });
  for (const p of demo.params) inputs.append(buildInputField(p));
  card.append(inputs);

  const presets = h("div", { class: "presets" });
  for (const preset of demo.presets) {
    // `preset` is already an Array (parsed from the manifest JSON). Don't
    // JSON.parse it — each element maps straight to the corresponding input.
    presets.append(h("button", {
      type: "button",
      on: {
        click: () => {
          demo.params.forEach((p, i) => {
            const el = inputs.querySelector(`input[data-name="${p.name}"]`);
            if (!el) return;
            el.value = JSON.stringify(preset[i]);
          });
        },
      },
    }, JSON.stringify(preset)));
  }
  card.append(presets);

  const output = h("div", { class: "output" }, "(not run yet)");
  const run = h("button", {
    class: "run",
    type: "button",
    on: {
      click: async () => {
        output.className = "output";
        output.textContent = "…loading module…";
        try {
          const mod = await import(/* @vite-ignore */ moduleUrl(demo.id));
          const exported = exportOf(mod, demo.export);
          const args = demo.params.map((p) => {
            const el = inputs.querySelector(`input[data-name="${p.name}"]`);
            return parseInputField(p, el.value);
          });
          const result = exported(...args);
          output.className = "output ok";
          output.textContent = formatResult(demo.result, result);
        } catch (err) {
          output.className = "output err";
          output.textContent = `${err.name || "Error"}: ${err.message || err}`;
        }
      },
    },
  }, "Run");
  card.append(run);
  card.append(output);

  return card;
}

function exportOf(mod, name) {
  // jco normalizes kebab-case export names to camelCase JS identifiers.
  // e.g. "is-zero" → isZero, "echo-list" → echoList.
  const cc = name.replace(/-([a-z])/g, (_, c) => c.toUpperCase());
  const fn = mod[cc] ?? mod[name] ?? mod.default?.[cc] ?? mod.default?.[name];
  if (typeof fn !== "function") {
    throw new Error(`export ${name} (camelCase: ${cc}) not found on module: ${Object.keys(mod).join(", ")}`);
  }
  return fn;
}

async function main() {
  // Plugin showcase is the primary content, so render it first (it also
  // owns the sample picker everyone's eyes go to).
  await initPluginShowcase();

  // Then the compile-and-run cards, which live behind a <details> below.
  const root = document.getElementById("demos");
  try {
    const demos = await loadManifest();
    for (const demo of demos) {
      root.append(buildCard(demo));
    }
  } catch (err) {
    root.append(h("div", { class: "card" }, [
      h("h2", {}, "failed to load demos"),
      h("p", { class: "desc" }, String(err)),
    ]));
  }
}

// ---------------------------------------------------------------------------
// Syntax plugin showcase (single rich sample, per-func show/caller toggles)
// ---------------------------------------------------------------------------

const PLUGINS = [
  {
    id: "raw",
    label: "raw",
    module: "raw/raw.js",
    capability: "S-expression — full structural roundtrip (signature + body)",
  },
  {
    id: "ruby-like",
    label: "ruby-like",
    module: "ruby-like/ruby_like.js",
    capability: "signature edits roundtrip · body edits dropped (preservation)",
  },
  {
    id: "ts-like",
    label: "ts-like",
    module: "ts-like/ts_like.js",
    capability: "signature + body edits roundtrip (v0.16-era IR)",
  },
  {
    id: "rust-like",
    label: "rust-like",
    module: "rust-like/rust_like.js",
    capability: "signature edits roundtrip · body edits dropped (preservation)",
  },
];

async function initPluginShowcase() {
  const host = document.getElementById("plugin-showcase");
  if (!host) return;

  let showcase;
  try {
    const res = await fetch(
      new URL("../public/components/plugin_showcase.json", import.meta.url),
    );
    showcase = await res.json();
  } catch (err) {
    host.append(h("p", { class: "err" }, `showcase load failed: ${err}`));
    return;
  }

  const pluginMods = {};
  for (const p of PLUGINS) {
    try {
      const m = await import(
        /* @vite-ignore */ new URL(
          `../public/plugins/${p.module}`,
          import.meta.url,
        ).href
      );
      pluginMods[p.id] = m.syntaxPlugin;
    } catch (err) {
      console.warn(`plugin ${p.id} load failed:`, err);
    }
  }

  // partial-manager rejoins the partial returned by from_text with the
  // full component so funcs/types not visible in the pane survive a Sync.
  let partialManager = null;
  try {
    const m = await import(
      /* @vite-ignore */ new URL(
        "../public/tools/partial-manager/partial_manager.js",
        import.meta.url,
      ).href
    );
    partialManager = m.partialManager;
  } catch (err) {
    console.warn("partial-manager load failed:", err);
  }

  const wc = showcase.wastComponent;

  // callGraph is keyed by source-inner name; normalize both ways so we can
  // map UIDs <-> names and find callers.
  const uidBySourceName = Object.fromEntries(
    wc.funcs.map(([uid, row]) => [row.source.val, uid]),
  );
  const sourceNameByUid = Object.fromEntries(
    wc.funcs.map(([uid, row]) => [uid, row.source.val]),
  );
  const callersBySourceName = {};
  for (const [caller, callees] of Object.entries(showcase.callGraph)) {
    for (const callee of callees) {
      (callersBySourceName[callee] ||= []).push(caller);
    }
  }
  function callersOf(uid) {
    const name = sourceNameByUid[uid];
    return (callersBySourceName[name] ?? [])
      .map((cn) => uidBySourceName[cn])
      .filter(Boolean);
  }

  // Per-func UI state: { uid -> { show, withCallers } }. Default: show
  // only the topmost internal func — that highlights the partial-manager
  // rule "no owned caller in B → Exported", because the lone target gets
  // promoted to `export` even though it's `internal` in `full`.
  const defaultUid = wc.funcs.find(([, row]) => row.source.tag === "internal")?.[0]
    ?? wc.funcs[0]?.[0];
  const toggles = {};
  for (const [uid] of wc.funcs) {
    toggles[uid] = {
      show: uid === defaultUid,
      withCallers: false,
    };
  }

  // The wast-component is mutable: edits in the controls table or
  // successful from_text parses replace `wc.funcs` / `wc.syms` in place,
  // and we re-render the panes from the new state.
  // Holds Mut so we can reassign on parse.
  let wcRef = wc;

  host.append(
    h("p", { class: "desc" }, [
      "This is ",
      h("strong", {}, "one wast component"),
      " with 12 functions that call each other. Toggle which ones appear, rename any, or ",
      h("strong", {}, "edit a pane and click Sync"),
      " to round-trip through ",
      h("code", {}, "from_text"),
      " — the other panes will reflect your changes via the IR. The four plugins are WASM Components themselves, transpiled by ",
      h("code", {}, "jco"),
      " and loaded as ES modules.",
    ]),
  );

  const controls = h("div", { class: "func-controls" });
  const grid = h("div", { class: "plugin-grid" });
  host.append(controls, grid);

  /** Build the partial WastComponent that the panes should render.
   *
   * Goes through partial-manager.extract so the result is a proper partial:
   *  - Show-without-callers targets are marked `Exported` in the partial
   *    (sig changes will be caught by partial-manager.merge against full's
   *    callers, since the syntax plugin can't see them).
   *  - Show-with-callers targets stay `Internal`; callers are pulled in so
   *    the syntax plugin can type-check call sites itself.
   *  - Direct callees are pulled in as `Imported` (signature only).
   */
  function paneComponent() {
    const targets = [];
    for (const [uid, t] of Object.entries(toggles)) {
      if (t.show) {
        targets.push({ sym: uid, includeCaller: t.withCallers });
      }
    }
    if (targets.length === 0) return null;
    if (!partialManager) {
      // Fallback when partial-manager failed to load. Just filter wcRef
      // shallowly; sync from this state will likely lose internals.
      const keep = new Set(targets.map((t) => t.sym));
      return { ...wcRef, funcs: wcRef.funcs.filter(([uid]) => keep.has(uid)) };
    }
    return partialManager.extract(wcRef, targets);
  }

  /** Pull the WastError list out of whatever a jco from_text rejection
   * looks like (could be a thrown Error with a `.payload`, an array, or
   * just a string message).
   */
  function extractParseErrors(thrown) {
    if (Array.isArray(thrown)) return thrown;
    if (thrown?.payload && Array.isArray(thrown.payload)) return thrown.payload;
    if (thrown?.message) return [{ message: thrown.message }];
    return [{ message: String(thrown) }];
  }

  function renderGrid() {
    grid.innerHTML = "";
    const comp = paneComponent();
    if (!comp || comp.funcs.length === 0) {
      grid.append(h("p", { class: "desc" }, "(nothing selected — toggle a func above)"));
      return;
    }
    for (const p of PLUGINS) {
      const box = h("div", { class: "plugin-box" });
      box.append(h("h3", {}, p.label));
      const ta = h("textarea", { class: "pane-text", spellcheck: "false" });
      ta.rows = 14;
      const errBox = h("div", { class: "pane-errors" });
      const sync = h(
        "button",
        { type: "button", class: "pane-sync" },
        "Sync from this pane →",
      );
      try {
        const plugin = pluginMods[p.id];
        if (!plugin) throw new Error("plugin module not loaded");
        ta.value = plugin.toText(comp);
        box.classList.add("ok");
      } catch (err) {
        ta.value = `error: ${err.message || err}`;
        box.classList.add("err");
      }
      sync.addEventListener("click", () => {
        errBox.innerHTML = "";
        errBox.className = "pane-errors";
        const plugin = pluginMods[p.id];
        if (!plugin || !plugin.fromText) {
          errBox.classList.add("err");
          errBox.textContent = "this plugin does not implement from_text";
          return;
        }
        let stage = "from_text";
        try {
          // Pane text only contains the filtered subset, so from_text
          // returns a *partial* WastComponent. partial-manager.merge
          // rejoins it with the full wcRef — funcs/types not in the
          // partial keep their existing entries in full.
          // jco unpacks result<T, E> — Ok arrives as a plain value, Err throws.
          const parsed = plugin.fromText(ta.value, wcRef);
          let merged;
          if (partialManager) {
            stage = "merge";
            merged = partialManager.merge(parsed, wcRef);
          } else {
            // Fallback: without partial-manager we'd lose internals.
            // Keep the parsed-as-full behaviour but warn loudly.
            console.warn("partial-manager unavailable; sync may drop funcs");
            merged = parsed;
          }
          wcRef = merged;
          // Rebuild toggles for any new uids introduced by the parser
          // (e.g. when a fresh func/local name appears in the edited text).
          for (const [uid, row] of wcRef.funcs) {
            if (!toggles[uid]) {
              toggles[uid] = {
                show: row.source.tag === "exported",
                withCallers: false,
              };
            }
          }
          renderControls();
          renderGrid();
        } catch (err) {
          const errs = extractParseErrors(err);
          errBox.classList.add("err");
          errBox.append(
            h("strong", {}, `${stage} failed (${errs.length} error${errs.length === 1 ? "" : "s"}):`),
          );
          const ul = h("ul", {});
          for (const e of errs) {
            const msg = e.message ?? String(e);
            const loc = e.location ? ` [${e.location}]` : "";
            ul.append(h("li", {}, `${msg}${loc}`));
          }
          errBox.append(ul);
        }
      });
      box.append(ta, h("div", { class: "pane-toolbar" }, [sync]));
      box.append(errBox);
      box.append(
        h("p", { class: "pane-cap" }, p.capability),
      );
      grid.append(box);
    }
  }

  function renderControls() {
    controls.innerHTML = "";
    const table = h("table", { class: "funcs-table" });
    table.append(
      h("thead", {}, h("tr", {}, [
        h("th", {}, "show"),
        h("th", {}, "+ callers"),
        h("th", {}, "uid"),
        h("th", {}, "display name (syms override)"),
        h("th", {}, "source"),
      ])),
    );
    const tbody = h("tbody", {});
    for (const [uid, row] of wcRef.funcs) {
      const tr = h("tr", {});

      const showBox = h("input", { type: "checkbox" });
      showBox.checked = toggles[uid]?.show ?? false;
      showBox.addEventListener("change", () => {
        toggles[uid].show = showBox.checked;
        renderGrid();
      });
      tr.append(h("td", {}, showBox));

      const callerBox = h("input", { type: "checkbox" });
      callerBox.checked = toggles[uid]?.withCallers ?? false;
      const nCallers = callersOf(uid).length;
      callerBox.disabled = nCallers === 0;
      callerBox.title =
        nCallers === 0
          ? "no callers"
          : `pull in ${nCallers} caller(s): ${callersOf(uid).join(", ")}`;
      callerBox.addEventListener("change", () => {
        toggles[uid].withCallers = callerBox.checked;
        renderGrid();
      });
      tr.append(h("td", {}, callerBox));

      tr.append(h("td", { class: "uid" }, uid));

      const nameInput = h("input", {
        type: "text",
        placeholder: `(default: ${uid})`,
      });
      const existing = wcRef.syms.internal.find((e) => e.uid === uid);
      nameInput.value = existing?.displayName ?? "";
      nameInput.addEventListener("input", () => {
        const val = nameInput.value.trim();
        wcRef.syms.internal = wcRef.syms.internal.filter((e) => e.uid !== uid);
        if (val !== "") {
          wcRef.syms.internal.push({ uid, displayName: val });
        }
        renderGrid();
      });
      tr.append(h("td", {}, nameInput));

      const tag = row.source.tag;
      tr.append(
        h("td", { class: "source" }, [
          h("span", { class: `source-tag source-${tag}` }, tag),
        ]),
      );

      tbody.append(tr);
    }
    table.append(tbody);
    controls.append(table);
  }

  renderControls();
  renderGrid();
}

main();
