// Value formatting / parsing helpers for the demo cards. Kept free of any
// DOM access so they can be unit-tested under plain `node --test`.
//
// 64-bit integers need care: jco represents u64/s64 as BigInt, and
// JSON.stringify throws `TypeError: Do not know how to serialize a BigInt`,
// while routing user input through Number() silently loses precision above
// 2^53. We stringify with a BigInt-aware replacer and parse 64-bit params
// via BigInt().

/** JSON.stringify that tolerates BigInt: a top-level BigInt renders as bare
 *  digits; BigInts nested inside lists/records render as quoted digit
 *  strings (lossless, and JSON.stringify would otherwise throw). */
export function jsonStringify(value) {
  if (typeof value === "bigint") return value.toString();
  return JSON.stringify(value, (_key, v) =>
    typeof v === "bigint" ? v.toString() : v,
  );
}

/** Map a manifest result tag + runtime value to a printable string. */
export function formatResult(tag, value) {
  if (value === undefined) return "(void)";
  if (tag === "string") return jsonStringify(value);
  if (tag?.startsWith("option<")) {
    if (value === null || value === undefined) return "none";
    return `some(${jsonStringify(value)})`;
  }
  if (tag?.startsWith("list<") || Array.isArray(value)) {
    return jsonStringify(value);
  }
  if (typeof value === "bigint") return value.toString();
  if (typeof value === "object") return jsonStringify(value);
  return String(value);
}

/** Coerce the text input of a param into the JS value jco expects. */
export function parseInputField(param, raw) {
  const trimmed = raw.trim();
  switch (param.kind) {
    case "u64": case "i64":
      // jco passes 64-bit integers as BigInt; Number() would round
      // anything above 2^53. BigInt() also rejects non-integer input
      // loudly instead of silently truncating.
      return BigInt(trimmed);
    case "u32": case "i32":
    case "f32": case "f64":
      return Number(trimmed);
    case "bool":
      return trimmed === "true";
    default:
      // Everything else is a JSON literal: strings, option, list, record,
      // tuple, variant, etc. (Note: nested 64-bit values inside compound
      // JSON inputs still parse as Number — acceptable for the demo.)
      return JSON.parse(trimmed);
  }
}
