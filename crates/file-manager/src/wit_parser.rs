//! Minimal line-by-line parser for WIT `world` definitions.
//!
//! Supports:
//! - `package ns:name;`
//! - `world name { ... }`
//! - `export name: func(...) -> type;`
//! - `import name: func(...) -> type;`
//! - `import name: interface { ... }`  (with func declarations inside)

/// A parsed WIT function signature.
#[derive(Debug, Clone, PartialEq)]
pub struct WitFunc {
    /// WIT path, e.g. `"inventory/add-item"` for interface funcs or `"handle-event"` for top-level.
    pub wit_path: String,
    /// Short name, e.g. `"add-item"`.
    pub name: String,
    /// Parameters: (name, wit_type_name).
    pub params: Vec<(String, String)>,
    /// Return type name, if any.
    pub result: Option<String>,
}

/// The result of parsing a world.wit file.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedWorld {
    pub world_name: String,
    pub imports: Vec<WitFunc>,
    pub exports: Vec<WitFunc>,
}

/// Map a WIT primitive type name to a canonical type UID string.
/// Returns `None` if the type is not a recognised primitive.
#[cfg(test)]
pub fn primitive_type_uid(name: &str) -> Option<&'static str> {
    match name {
        "u32" => Some("u32"),
        "u64" => Some("u64"),
        "i32" => Some("i32"),
        "i64" => Some("i64"),
        "f32" => Some("f32"),
        "f64" => Some("f64"),
        "bool" => Some("bool"),
        "char" => Some("char"),
        "string" => Some("string"),
        _ => None,
    }
}

/// Map a WIT type name to a `PrimitiveType` variant name that the bindings layer understands.
pub fn to_primitive_type(name: &str) -> Option<crate::bindings::wast::core::types::PrimitiveType> {
    use crate::bindings::wast::core::types::PrimitiveType;
    match name {
        "u32" => Some(PrimitiveType::U32),
        "u64" => Some(PrimitiveType::U64),
        "i32" => Some(PrimitiveType::I32),
        "i64" => Some(PrimitiveType::I64),
        "f32" => Some(PrimitiveType::F32),
        "f64" => Some(PrimitiveType::F64),
        "bool" => Some(PrimitiveType::Bool),
        "char" => Some(PrimitiveType::Char),
        "string" => Some(PrimitiveType::String),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Parser internals
// ---------------------------------------------------------------------------

/// Parse a WIT source string and return the first `world` block found.
pub fn parse_world(src: &str) -> Result<ParsedWorld, String> {
    let lines: Vec<&str> = src.lines().collect();
    let mut idx = 0;

    // 1. Find `world <name> {`
    let mut world_name: Option<String> = None;
    while idx < lines.len() {
        let trimmed = lines[idx].trim();
        if let Some(rest) = trimmed.strip_prefix("world ") {
            let rest = rest.trim();
            if let Some(name) = rest.strip_suffix('{') {
                world_name = Some(name.trim().to_string());
                idx += 1;
                break;
            }
        }
        idx += 1;
    }

    let world_name = world_name.ok_or_else(|| "no `world ... {` block found".to_string())?;

    let mut imports: Vec<WitFunc> = Vec::new();
    let mut exports: Vec<WitFunc> = Vec::new();

    // 2. Parse inside the world block until closing `}`
    while idx < lines.len() {
        let trimmed = lines[idx].trim();

        // End of world block
        if trimmed == "}" {
            break;
        }

        // Skip blank / comment lines
        if trimmed.is_empty() || trimmed.starts_with("//") {
            idx += 1;
            continue;
        }

        // import name: interface { ... }
        if let Some(rest) = trimmed.strip_prefix("import ") {
            if let Some((iface_name, remainder)) = rest.split_once(':') {
                let iface_name = iface_name.trim();
                let remainder = remainder.trim();

                if remainder.starts_with("interface") {
                    // Parse interface block
                    let block_start = remainder.strip_prefix("interface").unwrap().trim();
                    if block_start == "{" || block_start.is_empty() {
                        idx += 1;
                        // Parse funcs until `}`
                        while idx < lines.len() {
                            let inner = lines[idx].trim();
                            if inner == "}" {
                                idx += 1;
                                break;
                            }
                            if inner.is_empty() || inner.starts_with("//") {
                                idx += 1;
                                continue;
                            }
                            // Try to parse `name: func(...) -> type;`
                            if let Some(f) = parse_func_line(inner, Some(iface_name)) {
                                imports.push(f);
                            }
                            idx += 1;
                        }
                        continue;
                    }
                }

                // import name: func(...) -> type;
                let sig_str = format!("{}: {}", iface_name, remainder);
                if let Some(f) = parse_func_line(&sig_str, None) {
                    imports.push(f);
                }
            }
            idx += 1;
            continue;
        }

        // export name: interface { ... }
        if let Some(rest) = trimmed.strip_prefix("export ") {
            if let Some((iface_name, remainder)) = rest.split_once(':') {
                let iface_name = iface_name.trim();
                let remainder = remainder.trim();

                if remainder.starts_with("interface") {
                    let block_start = remainder.strip_prefix("interface").unwrap().trim();
                    if block_start == "{" || block_start.is_empty() {
                        idx += 1;
                        while idx < lines.len() {
                            let inner = lines[idx].trim();
                            if inner == "}" {
                                idx += 1;
                                break;
                            }
                            if inner.is_empty() || inner.starts_with("//") {
                                idx += 1;
                                continue;
                            }
                            if let Some(f) = parse_func_line(inner, Some(iface_name)) {
                                exports.push(f);
                            }
                            idx += 1;
                        }
                        continue;
                    }
                }

                // export name: func(...) -> type;
                let sig_str = format!("{}: {}", iface_name, remainder);
                if let Some(f) = parse_func_line(&sig_str, None) {
                    exports.push(f);
                }
            }
            idx += 1;
            continue;
        }

        idx += 1;
    }

    Ok(ParsedWorld {
        world_name,
        imports,
        exports,
    })
}

/// Parse a line like `name: func(param: type, ...) -> rettype;`
/// If `interface_name` is Some, the wit_path will be `"interface/name"`.
fn parse_func_line(line: &str, interface_name: Option<&str>) -> Option<WitFunc> {
    // Strip trailing `;`
    let line = line.trim().trim_end_matches(';').trim();

    // Split on first `:`
    let (name, rest) = line.split_once(':')?;
    let name = name.trim();
    let rest = rest.trim();

    // Must start with `func`
    let rest = rest.strip_prefix("func")?;
    let rest = rest.trim();

    // Parse params between `(` and `)`
    let rest = rest.strip_prefix('(')?;
    let (params_str, rest) = rest.split_once(')')?;
    let params = parse_params(params_str);

    // Parse optional return type after `->`
    let rest = rest.trim();
    let result = if let Some(ret) = rest.strip_prefix("->") {
        let ret = ret.trim();
        if ret.is_empty() {
            None
        } else {
            Some(ret.to_string())
        }
    } else {
        None
    };

    let wit_path = match interface_name {
        Some(iface) => format!("{}/{}", iface, name),
        None => name.to_string(),
    };

    Some(WitFunc {
        wit_path,
        name: name.to_string(),
        params,
        result,
    })
}

/// Parse `"slot: u32, count: u32"` into `[("slot", "u32"), ("count", "u32")]`.
fn parse_params(s: &str) -> Vec<(String, String)> {
    let s = s.trim();
    if s.is_empty() {
        return Vec::new();
    }
    s.split(',')
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            let (pname, ptype) = part.split_once(':')?;
            Some((pname.trim().to_string(), ptype.trim().to_string()))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_export() {
        let src = r#"
package myapp:bot;

world bot {
  export hello: func() -> string;
}
"#;
        let parsed = parse_world(src).unwrap();
        assert_eq!(parsed.world_name, "bot");
        assert!(parsed.imports.is_empty());
        assert_eq!(parsed.exports.len(), 1);
        let f = &parsed.exports[0];
        assert_eq!(f.wit_path, "hello");
        assert_eq!(f.name, "hello");
        assert!(f.params.is_empty());
        assert_eq!(f.result.as_deref(), Some("string"));
    }

    #[test]
    fn parse_import_interface_and_export() {
        let src = r#"
package myapp:bot;

world bot {
  import inventory: interface {
    add-item: func(slot: u32, count: u32) -> bool;
    remove-item: func(slot: u32) -> bool;
  }

  export handle-event: func(event-id: u32) -> bool;
}
"#;
        let parsed = parse_world(src).unwrap();
        assert_eq!(parsed.world_name, "bot");
        assert_eq!(parsed.imports.len(), 2);
        assert_eq!(parsed.exports.len(), 1);

        let add = &parsed.imports[0];
        assert_eq!(add.wit_path, "inventory/add-item");
        assert_eq!(add.name, "add-item");
        assert_eq!(
            add.params,
            vec![
                ("slot".to_string(), "u32".to_string()),
                ("count".to_string(), "u32".to_string()),
            ]
        );
        assert_eq!(add.result.as_deref(), Some("bool"));

        let remove = &parsed.imports[1];
        assert_eq!(remove.wit_path, "inventory/remove-item");

        let export = &parsed.exports[0];
        assert_eq!(export.wit_path, "handle-event");
        assert_eq!(export.result.as_deref(), Some("bool"));
    }

    #[test]
    fn parse_no_return_type() {
        let src = r#"
package test:pkg;

world w {
  export do-stuff: func(a: u32);
}
"#;
        let parsed = parse_world(src).unwrap();
        let f = &parsed.exports[0];
        assert_eq!(f.name, "do-stuff");
        assert_eq!(f.params, vec![("a".to_string(), "u32".to_string())]);
        assert_eq!(f.result, None);
    }

    #[test]
    fn parse_import_func_directly() {
        let src = r#"
package test:pkg;

world w {
  import log: func(msg: string);
}
"#;
        let parsed = parse_world(src).unwrap();
        assert_eq!(parsed.imports.len(), 1);
        let f = &parsed.imports[0];
        assert_eq!(f.wit_path, "log");
        assert_eq!(f.name, "log");
        assert_eq!(f.params, vec![("msg".to_string(), "string".to_string())]);
        assert_eq!(f.result, None);
    }

    #[test]
    fn parse_no_params_no_return() {
        let src = r#"
package test:pkg;

world w {
  export init: func();
}
"#;
        let parsed = parse_world(src).unwrap();
        let f = &parsed.exports[0];
        assert_eq!(f.name, "init");
        assert!(f.params.is_empty());
        assert_eq!(f.result, None);
    }

    #[test]
    fn parse_export_interface() {
        let src = r#"
package test:pkg;

world w {
  export api: interface {
    get-value: func(key: string) -> string;
    set-value: func(key: string, val: string);
  }
}
"#;
        let parsed = parse_world(src).unwrap();
        assert!(parsed.imports.is_empty());
        assert_eq!(parsed.exports.len(), 2);
        assert_eq!(parsed.exports[0].wit_path, "api/get-value");
        assert_eq!(parsed.exports[0].result.as_deref(), Some("string"));
        assert_eq!(parsed.exports[1].wit_path, "api/set-value");
        assert_eq!(parsed.exports[1].result, None);
    }

    #[test]
    fn parse_multiple_types() {
        let src = r#"
package test:pkg;

world w {
  export compute: func(x: f64, y: f64) -> f64;
  import check: func(flag: bool) -> bool;
}
"#;
        let parsed = parse_world(src).unwrap();
        assert_eq!(parsed.exports.len(), 1);
        assert_eq!(parsed.imports.len(), 1);

        let e = &parsed.exports[0];
        assert_eq!(
            e.params,
            vec![
                ("x".to_string(), "f64".to_string()),
                ("y".to_string(), "f64".to_string()),
            ]
        );
        assert_eq!(e.result.as_deref(), Some("f64"));
    }

    #[test]
    fn error_on_no_world() {
        let src = "package test:pkg;\n";
        assert!(parse_world(src).is_err());
    }

    #[test]
    fn primitive_type_uid_known() {
        assert_eq!(primitive_type_uid("u32"), Some("u32"));
        assert_eq!(primitive_type_uid("string"), Some("string"));
        assert_eq!(primitive_type_uid("bool"), Some("bool"));
        assert_eq!(primitive_type_uid("unknown"), None);
    }
}
