// The playground: compile wast source to wasm in the browser and run it, with
// no server and no jco anywhere in the path.
//
// The chain is
//
//   lisp source ──raw.from_text──> wast component ──compiler.compile_core──> core wasm
//                                                                              │
//                              WebAssembly.instantiate(bytes)  <───────────────┘
//
// The last step is the point. `compiler.compile` produces a Component Model
// binary, which no browser can instantiate — that is what jco exists to work
// around. `compile_core` returns the core module sitting inside it, and for a
// program with no imports that module needs no import object at all: it
// carries its own memory and defines (rather than imports) `cabi_realloc`.
// So the compiled output runs here, natively, as-is.
//
// The compiler itself still arrives as a jco-transpiled component. Making
// that half jco-free too is the next step, not this one.

import { SAMPLES, DEFAULT_SAMPLE } from "./samples.js";

/** Exports every compiled module carries; not user functions. */
const RUNTIME_EXPORTS = new Set(["memory", "cabi_realloc"]);

const EMPTY_COMPONENT = {
  funcs: [],
  types: [],
  syms: { witSyms: [], internal: [], local: [] },
};

function h(tag, props = {}, children = []) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (k === "class") el.className = v;
    else if (k.startsWith("on") && typeof v === "function") {
      el.addEventListener(k.slice(2).toLowerCase(), v);
    } else if (v !== undefined && v !== null) el.setAttribute(k, v);
  }
  for (const c of [].concat(children)) {
    if (c === null || c === undefined) continue;
    el.append(typeof c === "string" ? document.createTextNode(c) : c);
  }
  return el;
}

/** Flatten whatever a jco rejection carries into readable text. */
function describeError(thrown) {
  const payload = thrown?.payload ?? thrown?.cause ?? thrown?.value;
  const list = Array.isArray(payload) ? payload : Array.isArray(thrown) ? thrown : null;
  if (list) {
    return list.map((e) => `${e.message ?? e}${e?.location ? ` [${e.location}]` : ""}`).join("\n");
  }
  if (payload?.message) return payload.message;
  return thrown?.message ?? String(thrown);
}

async function loadTools() {
  const url = (p) => new URL(p, import.meta.url).href;
  const raw = await import(/* @vite-ignore */ url("../public/plugins/raw/raw.js"));
  const compilerMod = await import(
    /* @vite-ignore */ url("../public/tools/compiler/compiler.js")
  );
  return { raw, compiler: compilerMod.compiler };
}

/**
 * Index the parsed component's exported funcs by the name they take in the
 * core module, so the run panel can show real WIT signatures next to the
 * bare numeric exports wasm hands back.
 */
function exportedFuncs(component) {
  const byName = new Map();
  for (const [uid, func] of component.funcs) {
    if (func.source?.tag !== "exported") continue;
    byName.set(func.source.val, { uid, func });
  }
  return byName;
}

/** Render a WIT type ref for display. Compound refs are already text. */
function typeLabel(ref) {
  return ref ?? "()";
}

function signatureOf(entry) {
  if (!entry) return "";
  const params = entry.func.params.map(([uid, ty]) => `${uid}: ${typeLabel(ty)}`).join(", ");
  const result = entry.func.result ? ` -> ${typeLabel(entry.func.result)}` : "";
  return `(${params})${result}`;
}

/**
 * Read a returned string out of the module's memory.
 *
 * A `string` result does not fit in one core value, so the Canonical ABI
 * returns a pointer to an 8-byte pair: the string's address, then its length
 * in bytes. Both live in the module's own linear memory.
 */
function readString(instance, ret) {
  const view = new DataView(instance.exports.memory.buffer);
  const bytes = new Uint8Array(instance.exports.memory.buffer);
  const ptr = view.getInt32(ret, true);
  const len = view.getInt32(ret + 4, true);
  return new TextDecoder().decode(bytes.subarray(ptr, ptr + len));
}

export async function initPlayground() {
  const host = document.getElementById("playground");
  if (!host) return;

  let tools;
  try {
    tools = await loadTools();
  } catch (err) {
    host.append(
      h("p", { class: "pg-error" }, `compiler failed to load: ${describeError(err)}`),
    );
    return;
  }

  // ── state ──
  let compiled = null; // { component, bytes, wat, instance, exports }

  // ── controls ──
  const picker = h(
    "select",
    { id: "pg-sample", "aria-label": "sample program" },
    SAMPLES.map((s) => h("option", { value: s.id }, s.label)),
  );
  const compileBtn = h("button", { class: "pg-primary", type: "button" }, "Compile");
  const status = h("span", { class: "pg-status" }, "");
  const blurb = h("p", { class: "pg-blurb" }, "");

  const editor = h("textarea", {
    class: "pg-editor",
    spellcheck: "false",
    "aria-label": "wast source",
  });

  const runPanel = h("div", { class: "pg-runs" });
  const watBody = h("pre", { class: "pg-wat" });
  const watWrap = h("details", { class: "pg-details" }, [
    h("summary", {}, "Generated core WAT"),
    watBody,
  ]);
  const downloadWrap = h("div", { class: "pg-download" });

  function setStatus(text, kind = "") {
    status.textContent = text;
    status.className = `pg-status ${kind}`;
  }

  function loadSample(id) {
    const sample = SAMPLES.find((s) => s.id === id) ?? SAMPLES[0];
    editor.value = sample.source;
    blurb.textContent = sample.blurb;
    runPanel.replaceChildren();
    watBody.textContent = "";
    downloadWrap.replaceChildren();
    watWrap.open = false;
    setStatus("");
    compiled = null;
  }

  /** One row per exported function: signature, arguments, result. */
  function buildRunRow(name, fn, entry, prefill) {
    const argsInput = h("input", {
      class: "pg-args",
      type: "text",
      value: (prefill ?? []).join(", "),
      placeholder: fn.length === 0 ? "(no arguments)" : `${fn.length} value(s)`,
      "aria-label": `arguments for ${name}`,
    });
    if (fn.length === 0) argsInput.disabled = true;
    const output = h("output", { class: "pg-result" }, "");

    const run = () => {
      const args = argsInput.value
        .split(",")
        .map((s) => s.trim())
        .filter((s) => s !== "")
        .map(Number);
      if (args.some(Number.isNaN)) {
        output.textContent = "arguments must be numbers";
        output.className = "pg-result bad";
        return;
      }
      if (args.length !== fn.length) {
        output.textContent = `expected ${fn.length} value(s), got ${args.length}`;
        output.className = "pg-result bad";
        return;
      }
      try {
        const raw = fn(...args);
        // A `string` result comes back as a pointer into the module's
        // memory; everything else is already the value.
        const value =
          entry?.func?.result === "string"
            ? JSON.stringify(readString(compiled.instance, raw))
            : String(raw);
        output.textContent = `= ${value}`;
        output.className = "pg-result good";
      } catch (err) {
        output.textContent = describeError(err);
        output.className = "pg-result bad";
      }
    };

    argsInput.addEventListener("keydown", (e) => {
      if (e.key === "Enter") run();
    });

    // A compound parameter does not survive as one core value: the
    // Canonical ABI spreads it across several. Say so, otherwise the input
    // asking for three numbers against a two-parameter signature is a puzzle.
    const witParams = entry?.func?.params?.length ?? fn.length;
    const flattened =
      witParams !== fn.length
        ? h(
            "p",
            { class: "pg-abi-note" },
            `${witParams} parameter(s) flatten to ${fn.length} core value(s) across the Canonical ABI — pass them in order.`,
          )
        : null;

    return h("div", { class: "pg-run-row" }, [
      h("div", { class: "pg-run-head" }, [
        h("code", { class: "pg-fn" }, name),
        h("span", { class: "pg-sig" }, signatureOf(entry)),
      ]),
      h("div", { class: "pg-run-controls" }, [
        argsInput,
        h("button", { type: "button", onClick: run }, "Run"),
        output,
      ]),
      flattened,
    ]);
  }

  async function compile() {
    const source = editor.value;
    runPanel.replaceChildren();
    downloadWrap.replaceChildren();
    watBody.textContent = "";
    setStatus("compiling…");

    let component;
    try {
      component = tools.raw.syntaxEditor.fromText(source, EMPTY_COMPONENT);
    } catch (err) {
      setStatus("parse failed", "bad");
      runPanel.append(h("pre", { class: "pg-error" }, describeError(err)));
      return;
    }

    let bytes, wat;
    try {
      wat = tools.compiler.emitWat(component);
      bytes = tools.compiler.compileCore(component);
    } catch (err) {
      setStatus("compile failed", "bad");
      runPanel.append(h("pre", { class: "pg-error" }, describeError(err)));
      return;
    }

    let instance;
    try {
      // No import object: an import-free program needs nothing from the host.
      ({ instance } = await WebAssembly.instantiate(bytes));
    } catch (err) {
      setStatus("instantiate failed", "bad");
      runPanel.append(h("pre", { class: "pg-error" }, describeError(err)));
      return;
    }

    compiled = { component, bytes, wat, instance };
    watBody.textContent = wat;
    setStatus(`compiled — ${bytes.byteLength} bytes, running in this page`, "good");

    const blob = new Blob([bytes], { type: "application/wasm" });
    downloadWrap.append(
      h(
        "a",
        { class: "pg-dl", href: URL.createObjectURL(blob), download: "module.wasm" },
        `download module.wasm (${bytes.byteLength} bytes)`,
      ),
    );

    const byName = exportedFuncs(component);
    const sample = SAMPLES.find((s) => s.id === picker.value);
    const prefills = new Map();
    for (const call of sample?.calls ?? []) {
      if (!prefills.has(call.func)) prefills.set(call.func, call.args);
    }

    const rows = Object.entries(instance.exports)
      .filter(([name, v]) => typeof v === "function" && !RUNTIME_EXPORTS.has(name))
      .map(([name, fn]) => buildRunRow(name, fn, byName.get(name), prefills.get(name)));

    runPanel.append(
      rows.length
        ? h("div", { class: "pg-run-list" }, rows)
        : h("p", { class: "pg-note" }, "no exported functions — add an (export $name) to a func"),
    );
  }

  picker.addEventListener("change", () => loadSample(picker.value));
  compileBtn.addEventListener("click", compile);
  editor.addEventListener("keydown", (e) => {
    // Ctrl/Cmd+Enter compiles, the shortcut every playground has.
    if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
      e.preventDefault();
      compile();
    }
  });

  host.append(
    h("div", { class: "pg-bar" }, [
      h("label", { for: "pg-sample" }, "Sample"),
      picker,
      compileBtn,
      h("span", { class: "pg-hint" }, "Ctrl/⌘+Enter"),
      status,
    ]),
    blurb,
    h("div", { class: "pg-grid" }, [
      h("div", { class: "pg-pane" }, [h("h3", {}, "Source"), editor]),
      h("div", { class: "pg-pane" }, [
        h("h3", {}, "Compiled module"),
        runPanel,
        downloadWrap,
        watWrap,
      ]),
    ]),
  );

  loadSample(DEFAULT_SAMPLE);
  await compile();
}
