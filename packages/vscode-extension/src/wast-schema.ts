/**
 * Structural validation for on-disk `wast.json` content.
 *
 * Deliberately free of any `vscode` import so it can be unit-tested under
 * plain Node (`node --test`) without an Extension Development Host. The
 * shapes mirror `crates/wast-types/src/lib.rs` serde output.
 *
 * `version` is REQUIRED (matching the Rust side, which has no serde
 * default): a `wast.json` without a `version` field — or with a version
 * other than `WAST_DB_CURRENT_VERSION` — is rejected with a descriptive
 * error so callers can surface it to the user instead of silently
 * mis-parsing a future format.
 */

/** The current (and only supported) `wast.json` schema version.
 *  Keep in sync with `wast_types::WastDb::CURRENT_VERSION`. */
export const WAST_DB_CURRENT_VERSION = 1;

export type WastDbValidation =
  | { ok: true }
  | { ok: false; error: string };

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

const SOURCE_KEYS = ["Internal", "Imported", "Exported"] as const;

function validateSource(v: unknown, where: string): string | null {
  if (!isRecord(v)) return `${where}: source must be an object`;
  const keys = Object.keys(v);
  if (keys.length !== 1 || !SOURCE_KEYS.includes(keys[0] as never)) {
    return `${where}: source must have exactly one of ${SOURCE_KEYS.join("/")}`;
  }
  if (typeof v[keys[0]] !== "string") {
    return `${where}: source.${keys[0]} must be a string`;
  }
  return null;
}

function validateParams(v: unknown, where: string): string | null {
  if (!Array.isArray(v)) return `${where}: params must be an array`;
  for (const p of v) {
    if (
      !Array.isArray(p) ||
      p.length !== 2 ||
      typeof p[0] !== "string" ||
      typeof p[1] !== "string"
    ) {
      return `${where}: each param must be a [name, type] string pair`;
    }
  }
  return null;
}

/**
 * Validate a parsed (`JSON.parse`d) wast.json value. Returns `{ ok: true }`
 * when the value is a structurally sound `WastDb` at the supported schema
 * version, otherwise `{ ok: false, error }` with a user-presentable message.
 */
export function validateWastDb(value: unknown): WastDbValidation {
  if (!isRecord(value)) {
    return { ok: false, error: "wast.json root must be a JSON object" };
  }

  // Version gate first — a newer format may legitimately change everything
  // below, so don't confuse users with structural errors about it.
  if (!("version" in value)) {
    return {
      ok: false,
      error: `wast.json is missing the required "version" field (expected ${WAST_DB_CURRENT_VERSION})`,
    };
  }
  const version = value.version;
  if (typeof version !== "number" || !Number.isInteger(version)) {
    return { ok: false, error: `wast.json "version" must be an integer` };
  }
  if (version !== WAST_DB_CURRENT_VERSION) {
    return {
      ok: false,
      error:
        `unsupported wast.json version ${version} — this extension supports ` +
        `version ${WAST_DB_CURRENT_VERSION}. ` +
        (version > WAST_DB_CURRENT_VERSION
          ? "Update the WAST extension to open this file."
          : "Re-save the file with current tooling to migrate it."),
    };
  }

  if (!Array.isArray(value.funcs)) {
    return { ok: false, error: `wast.json "funcs" must be an array` };
  }
  if (!Array.isArray(value.types)) {
    return { ok: false, error: `wast.json "types" must be an array` };
  }

  for (let i = 0; i < value.funcs.length; i++) {
    const row = value.funcs[i];
    const where = `funcs[${i}]`;
    if (!isRecord(row)) return { ok: false, error: `${where}: must be an object` };
    if (typeof row.uid !== "string" || row.uid.length === 0) {
      return { ok: false, error: `${where}: "uid" must be a non-empty string` };
    }
    const srcErr = validateSource(row.source, where);
    if (srcErr) return { ok: false, error: srcErr };
    const paramErr = validateParams(row.params, where);
    if (paramErr) return { ok: false, error: paramErr };
    if (row.result !== null && typeof row.result !== "string") {
      return { ok: false, error: `${where}: "result" must be a string or null` };
    }
    if (
      row.body !== null &&
      (!Array.isArray(row.body) || row.body.some((b) => typeof b !== "number"))
    ) {
      return { ok: false, error: `${where}: "body" must be null or an array of bytes` };
    }
  }

  for (let i = 0; i < value.types.length; i++) {
    const row = value.types[i];
    const where = `types[${i}]`;
    if (!isRecord(row)) return { ok: false, error: `${where}: must be an object` };
    if (typeof row.uid !== "string" || row.uid.length === 0) {
      return { ok: false, error: `${where}: "uid" must be a non-empty string` };
    }
    const srcErr = validateSource(row.source, where);
    if (srcErr) return { ok: false, error: srcErr };
    if (row.definition === undefined || row.definition === null) {
      return { ok: false, error: `${where}: missing "definition"` };
    }
  }

  return { ok: true };
}
