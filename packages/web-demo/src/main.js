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
  // don't need type-specific widgets for Option / Record / List.
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
    default: return "null";
  }
}

/** Coerce the text input of a param into the JS value jco expects. */
function parseInputField(param, raw) {
  const trimmed = raw.trim();
  switch (param.kind) {
    case "u32": case "i32": case "u64": case "i64":
      return Number(trimmed);
    case "f32": case "f64":
      return Number(trimmed);
    case "bool":
      return trimmed === "true";
    case "char":
      return JSON.parse(trimmed);
    case "string":
      return JSON.parse(trimmed);
    case "option":
      return JSON.parse(trimmed);
    case "list":
      return JSON.parse(trimmed);
    case "record":
      return JSON.parse(trimmed);
    default: return JSON.parse(trimmed);
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

main();
