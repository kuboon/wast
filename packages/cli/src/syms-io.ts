/**
 * Simple YAML-like parser/serializer for syms files.
 *
 * Format:
 * ```
 * wit:
 *   inventory/add-item: add item
 * internal:
 *   f3a9: calculate_drop_rate
 * local:
 *   a7f2: slot
 * ```
 */

export interface SymsData {
  wit: Map<string, string>;
  internal: Map<string, string>;
  local: Map<string, string>;
}

const SECTIONS = ["wit", "internal", "local"] as const;
type Section = (typeof SECTIONS)[number];

export function parseSyms(text: string): SymsData {
  const data: SymsData = {
    wit: new Map(),
    internal: new Map(),
    local: new Map(),
  };

  let currentSection: Section | null = null;

  for (const line of text.split("\n")) {
    const trimmed = line.trimEnd();
    if (trimmed === "" || trimmed.startsWith("#")) continue;

    // Section header: "wit:", "internal:", "local:"
    const sectionMatch = trimmed.match(/^(wit|internal|local):$/);
    if (sectionMatch) {
      currentSection = sectionMatch[1] as Section;
      continue;
    }

    // Entry: "  uid: display name"
    if (currentSection !== null) {
      const entryMatch = trimmed.match(/^\s+([^:]+):\s+(.+)$/);
      if (entryMatch) {
        data[currentSection].set(entryMatch[1].trim(), entryMatch[2].trim());
      }
    }
  }

  return data;
}

export function serializeSyms(data: SymsData): string {
  const lines: string[] = [];

  for (const section of SECTIONS) {
    const map = data[section];
    if (map.size === 0) continue;

    if (lines.length > 0) lines.push("");
    lines.push(`${section}:`);
    for (const [uid, name] of map) {
      lines.push(`  ${uid}: ${name}`);
    }
  }

  lines.push(""); // trailing newline
  return lines.join("\n");
}

/**
 * Determine which section a UID belongs to based on its format.
 * - Contains "/" -> wit (interface-scoped identifiers)
 * - 4-char hex -> internal
 * - Otherwise -> local
 */
export function classifyUid(uid: string): Section {
  if (uid.includes("/")) return "wit";
  if (/^[0-9a-f]{4}$/.test(uid)) return "internal";
  return "local";
}
