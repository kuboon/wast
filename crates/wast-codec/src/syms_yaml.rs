//! Reader/writer for `syms.*.yaml`.
//!
//! The format is a deliberate YAML subset: three top-level mappings (`wit`,
//! `internal`, `local`), each containing flat `key: value` entries. Scalars
//! may be plain or double-quoted; the writer double-quotes (with backslash
//! escapes) any key or value that would be ambiguous or lossy as a plain
//! scalar — uids containing `:` (wit paths like `wast:sample/iface#fn`),
//! names containing `#`/quotes/newlines, or leading/trailing whitespace.
//! Standard YAML tools can read the output.
//!
//! On parse, unquoted keys may still contain `:` (legacy hand-authored
//! files): the key/value separator is the LAST `": "` (or trailing `:`)
//! outside double quotes. Files with genuinely ambiguous entries must quote.

use wast_types::{SymEntry, Syms};

/// Returns true when `s` cannot be round-tripped as a plain (unquoted) YAML
/// scalar in our subset and must be double-quoted.
fn needs_quoting(s: &str) -> bool {
    let Some(first) = s.chars().next() else {
        return true; // empty string
    };
    let last = s.chars().last().unwrap();
    if first.is_whitespace() || last.is_whitespace() {
        return true;
    }
    if s.chars()
        .any(|c| matches!(c, ':' | '#' | '"' | '\\' | '\n' | '\r' | '\t'))
    {
        return true;
    }
    // Characters that act as indicators at the start of a plain YAML scalar.
    matches!(
        first,
        '-' | '?'
            | '['
            | ']'
            | '{'
            | '}'
            | '&'
            | '*'
            | '!'
            | '|'
            | '>'
            | '%'
            | '@'
            | '`'
            | '\''
            | ','
    )
}

/// Encode a scalar for output: plain when safe, double-quoted with backslash
/// escapes otherwise.
fn encode_scalar(s: &str) -> String {
    if !needs_quoting(s) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Parse a double-quoted scalar starting at the beginning of `s` (which must
/// start with `"`). Returns the decoded string and the number of bytes
/// consumed including the closing quote.
fn parse_quoted(s: &str) -> Result<(String, usize), String> {
    debug_assert!(s.starts_with('"'));
    let mut out = String::new();
    let mut chars = s.char_indices().skip(1);
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Ok((out, i + 1)),
            '\\' => match chars.next() {
                Some((_, '"')) => out.push('"'),
                Some((_, '\\')) => out.push('\\'),
                Some((_, 'n')) => out.push('\n'),
                Some((_, 'r')) => out.push('\r'),
                Some((_, 't')) => out.push('\t'),
                Some((_, other)) => {
                    return Err(format!("unsupported escape sequence '\\{other}'"));
                }
                None => return Err("unterminated escape sequence".to_string()),
            },
            c => out.push(c),
        }
    }
    Err("unterminated double-quoted scalar".to_string())
}

/// Parse one `key: value` entry line (already trimmed).
fn parse_entry(line: &str) -> Result<(String, String), String> {
    // Quoted key.
    if line.starts_with('"') {
        let (key, consumed) = parse_quoted(line)?;
        let rest = line[consumed..].trim_start();
        let rest = rest
            .strip_prefix(':')
            .ok_or_else(|| "expected ':' after quoted key".to_string())?;
        let value = parse_value(rest.trim_start())?;
        return Ok((key, value));
    }

    // Unquoted key: the separator is the LAST ':' outside double quotes that
    // is followed by a space (or ends the line). This keeps legacy unquoted
    // uids containing ':' (e.g. `wast:sample/foo: name`) parseable, since
    // path-internal colons aren't followed by a space.
    let bytes = line.as_bytes();
    let mut in_quotes = false;
    let mut escaped = false;
    let mut separator: Option<usize> = None;
    for (i, &b) in bytes.iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        match b {
            b'\\' if in_quotes => escaped = true,
            b'"' => in_quotes = !in_quotes,
            b':' if !in_quotes => {
                if i + 1 == bytes.len() || bytes[i + 1] == b' ' {
                    separator = Some(i);
                }
            }
            _ => {}
        }
    }
    let sep = separator.ok_or_else(|| format!("expected 'key: value', got '{line}'"))?;
    let key = line[..sep].trim().to_string();
    let value = parse_value(line[sep + 1..].trim_start())?;
    Ok((key, value))
}

/// Parse the value part of an entry: double-quoted (with escapes) or plain.
fn parse_value(s: &str) -> Result<String, String> {
    if s.starts_with('"') {
        let (value, consumed) = parse_quoted(s)?;
        if !s[consumed..].trim().is_empty() {
            return Err(format!(
                "unexpected trailing characters after quoted value: '{}'",
                &s[consumed..]
            ));
        }
        Ok(value)
    } else {
        Ok(s.trim().to_string())
    }
}

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
        if line.is_empty() || line.trim_start().starts_with('#') {
            continue;
        }

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

        let trimmed = line.trim();
        let (key, value) =
            parse_entry(trimmed).map_err(|e| format!("line {}: {}", line_num + 1, e))?;
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
    }

    Ok(syms)
}

pub fn write_syms_yaml(syms: &Syms) -> String {
    let mut out = String::new();
    let push_entry = |out: &mut String, key: &str, value: &str| {
        out.push_str(&format!(
            "  {}: {}\n",
            encode_scalar(key),
            encode_scalar(value)
        ));
    };

    if !syms.wit_syms.is_empty() {
        out.push_str("wit:\n");
        for (k, v) in &syms.wit_syms {
            push_entry(&mut out, k, v);
        }
    }

    if !syms.internal.is_empty() {
        out.push_str("internal:\n");
        for e in &syms.internal {
            push_entry(&mut out, &e.uid, &e.display_name);
        }
    }

    if !syms.local.is_empty() {
        out.push_str("local:\n");
        for e in &syms.local {
            push_entry(&mut out, &e.uid, &e.display_name);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(uid: &str, name: &str) {
        let syms = Syms {
            wit_syms: vec![(uid.to_string(), name.to_string())],
            internal: vec![SymEntry {
                uid: uid.to_string(),
                display_name: name.to_string(),
            }],
            local: vec![SymEntry {
                uid: uid.to_string(),
                display_name: name.to_string(),
            }],
        };
        let text = write_syms_yaml(&syms);
        let parsed = parse_syms_yaml(&text)
            .unwrap_or_else(|e| panic!("parse failed for uid={uid:?} name={name:?}: {e}\n{text}"));
        assert_eq!(parsed.wit_syms, vec![(uid.to_string(), name.to_string())]);
        assert_eq!(parsed.internal[0].uid, uid);
        assert_eq!(parsed.internal[0].display_name, name);
        assert_eq!(parsed.local[0].uid, uid);
        assert_eq!(parsed.local[0].display_name, name);
    }

    #[test]
    fn roundtrip_plain() {
        roundtrip("square", "square function");
    }

    #[test]
    fn roundtrip_uid_with_colons() {
        roundtrip("wast:sample/iface#fn", "my func");
        roundtrip("[constructor]r", "make resource");
    }

    #[test]
    fn roundtrip_name_with_newline() {
        roundtrip("f1", "line one\nline two");
    }

    #[test]
    fn roundtrip_name_with_hash() {
        roundtrip("f1", "issue #42");
    }

    #[test]
    fn roundtrip_name_with_quotes_and_backslash() {
        roundtrip("f1", r#"say "hi" \ bye"#);
    }

    #[test]
    fn roundtrip_name_with_leading_and_trailing_space() {
        roundtrip("f1", " padded ");
    }

    #[test]
    fn roundtrip_empty_value() {
        roundtrip("f1", "");
    }

    #[test]
    fn parses_legacy_unquoted_uid_with_colon() {
        // Hand-authored legacy style: unquoted uid containing ':' — the
        // separator is the last ': ' outside quotes.
        let parsed = parse_syms_yaml("wit:\n  wast:sample/f: my name\n").unwrap();
        assert_eq!(
            parsed.wit_syms,
            vec![("wast:sample/f".to_string(), "my name".to_string())]
        );
    }

    #[test]
    fn parses_empty_sections() {
        let parsed = parse_syms_yaml("wit:\ninternal:\nlocal:\n").unwrap();
        assert!(parsed.wit_syms.is_empty());
        assert!(parsed.internal.is_empty());
        assert!(parsed.local.is_empty());
    }

    #[test]
    fn rejects_unterminated_quote() {
        let result = parse_syms_yaml("wit:\n  \"broken: name\n");
        assert!(result.is_err());
    }
}
