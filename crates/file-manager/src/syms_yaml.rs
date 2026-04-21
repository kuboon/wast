//! Simple YAML parser/writer for syms files.
//!
//! The syms YAML format is deliberately flat — three sections (`wit`, `internal`,
//! `local`) each containing `key: value` pairs. We avoid pulling in `serde_yaml`
//! (which has trouble on wasm32-wasip1) and instead implement a small bespoke
//! parser/writer for this known schema.

use wast_types::{SymEntry, Syms};

/// Parse a syms YAML string into a `Syms` struct.
pub fn parse_syms_yaml(input: &str) -> Result<Syms, String> {
    let mut syms = Syms {
        wit_syms: Vec::new(),
        internal: Vec::new(),
        local: Vec::new(),
    };

    #[derive(Clone, Copy)]
    enum Section {
        None,
        Wit,
        Internal,
        Local,
    }

    let mut current = Section::None;

    for (line_num, raw_line) in input.lines().enumerate() {
        let line = raw_line.trim_end();

        // Skip blank lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Section header — a top-level key followed by ':'
        if !line.starts_with(' ') && !line.starts_with('\t') {
            let trimmed = line.trim();
            match trimmed.trim_end_matches(':') {
                "wit" => current = Section::Wit,
                "internal" => current = Section::Internal,
                "local" => current = Section::Local,
                other => {
                    return Err(format!(
                        "line {}: unknown section '{}'",
                        line_num + 1,
                        other
                    ));
                }
            }
            continue;
        }

        // Indented key: value entry
        let trimmed = line.trim();
        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim().to_string();
            let value = value.trim().to_string();
            match current {
                Section::Wit => syms.wit_syms.push((key, value)),
                Section::Internal => syms.internal.push(SymEntry {
                    uid: key,
                    display_name: value,
                }),
                Section::Local => syms.local.push(SymEntry {
                    uid: key,
                    display_name: value,
                }),
                Section::None => {
                    return Err(format!(
                        "line {}: entry outside of any section",
                        line_num + 1
                    ));
                }
            }
        } else {
            return Err(format!(
                "line {}: expected 'key: value', got '{}'",
                line_num + 1,
                trimmed
            ));
        }
    }

    Ok(syms)
}

/// Serialize a `Syms` struct to YAML text.
pub fn write_syms_yaml(syms: &Syms) -> String {
    let mut out = String::new();

    if !syms.wit_syms.is_empty() {
        out.push_str("wit:\n");
        for (k, v) in &syms.wit_syms {
            out.push_str(&format!("  {}: {}\n", k, v));
        }
    }

    if !syms.internal.is_empty() {
        out.push_str("internal:\n");
        for e in &syms.internal {
            out.push_str(&format!("  {}: {}\n", e.uid, e.display_name));
        }
    }

    if !syms.local.is_empty() {
        out.push_str("local:\n");
        for e in &syms.local {
            out.push_str(&format!("  {}: {}\n", e.uid, e.display_name));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let syms = Syms {
            wit_syms: vec![("inventory/add-item".into(), "Add item".into())],
            internal: vec![SymEntry {
                uid: "f3a9".into(),
                display_name: "drop rate calc".into(),
            }],
            local: vec![SymEntry {
                uid: "a7f2".into(),
                display_name: "slot number".into(),
            }],
        };
        let yaml = write_syms_yaml(&syms);
        let parsed = parse_syms_yaml(&yaml).unwrap();
        assert_eq!(parsed.wit_syms, syms.wit_syms);
        assert_eq!(parsed.internal.len(), 1);
        assert_eq!(parsed.internal[0].uid, "f3a9");
        assert_eq!(parsed.local[0].display_name, "slot number");
    }

    #[test]
    fn parse_empty() {
        let syms = parse_syms_yaml("").unwrap();
        assert!(syms.wit_syms.is_empty());
        assert!(syms.internal.is_empty());
        assert!(syms.local.is_empty());
    }
}
