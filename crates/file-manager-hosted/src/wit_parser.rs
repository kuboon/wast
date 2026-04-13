use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct WitFunc {
    pub wit_path: String,
    pub name: String,
    pub params: Vec<(String, String)>,
    pub result: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedWorld {
    pub world_name: String,
    pub imports: Vec<WitFunc>,
    pub exports: Vec<WitFunc>,
}

pub fn parse_world(src: &str) -> Result<ParsedWorld, String> {
    let lines: Vec<&str> = src.lines().collect();
    let interfaces = parse_interfaces(&lines);

    let mut idx = 0;
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

    let mut imports = Vec::new();
    let mut exports = Vec::new();

    while idx < lines.len() {
        let trimmed = lines[idx].trim();

        if trimmed == "}" {
            break;
        }

        if trimmed.is_empty() || trimmed.starts_with("//") {
            idx += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("import ") {
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
                            if let Some(func) = parse_func_line(inner, Some(iface_name)) {
                                imports.push(func);
                            }
                            idx += 1;
                        }
                        continue;
                    }
                }

                let sig_str = format!("{}: {}", iface_name, remainder);
                if let Some(func) = parse_func_line(&sig_str, None) {
                    imports.push(func);
                }
            } else {
                let iface = rest.trim().trim_end_matches(';').trim();
                if let Some(funcs) = interfaces.get(iface) {
                    imports.extend(funcs.iter().cloned().map(|mut f| {
                        f.wit_path = format!("{}/{}", iface, f.name);
                        f
                    }));
                }
            }
            idx += 1;
            continue;
        }

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
                            if let Some(func) = parse_func_line(inner, Some(iface_name)) {
                                exports.push(func);
                            }
                            idx += 1;
                        }
                        continue;
                    }
                }

                let sig_str = format!("{}: {}", iface_name, remainder);
                if let Some(func) = parse_func_line(&sig_str, None) {
                    exports.push(func);
                }
            } else {
                let iface = rest.trim().trim_end_matches(';').trim();
                if let Some(funcs) = interfaces.get(iface) {
                    exports.extend(funcs.iter().cloned().map(|mut f| {
                        f.wit_path = format!("{}/{}", iface, f.name);
                        f
                    }));
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

fn parse_interfaces(lines: &[&str]) -> BTreeMap<String, Vec<WitFunc>> {
    let mut map: BTreeMap<String, Vec<WitFunc>> = BTreeMap::new();
    let mut idx = 0usize;

    while idx < lines.len() {
        let trimmed = lines[idx].trim();
        if let Some(rest) = trimmed.strip_prefix("interface ") {
            let name = rest.trim().trim_end_matches('{').trim().to_string();
            idx += 1;
            let mut funcs: Vec<WitFunc> = Vec::new();

            while idx < lines.len() {
                let inner = lines[idx].trim();
                if inner == "}" {
                    idx += 1;
                    break;
                }
                if inner.is_empty() || inner.starts_with("//") || inner.starts_with("use ") {
                    idx += 1;
                    continue;
                }
                if let Some(func) = parse_func_line(inner, None) {
                    funcs.push(func);
                }
                idx += 1;
            }

            map.insert(name, funcs);
            continue;
        }
        idx += 1;
    }

    map
}

fn parse_func_line(line: &str, interface_name: Option<&str>) -> Option<WitFunc> {
    let line = line.trim_end_matches(';').trim();
    let (name, rest) = line.split_once(':')?;
    let name = name.trim();
    let rest = rest.trim();
    let rest = rest.strip_prefix("func")?.trim();
    let rest = rest.strip_prefix('(')?;
    let (params_s, rest) = rest.split_once(')')?;
    let params = parse_params(params_s);
    let result = rest
        .trim()
        .strip_prefix("->")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

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

fn parse_params(s: &str) -> Vec<(String, String)> {
    if s.trim().is_empty() {
        return Vec::new();
    }
    s.split(',')
        .filter_map(|part| {
            let (pname, ptype) = part.trim().split_once(':')?;
            Some((pname.trim().to_string(), ptype.trim().to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
        use super::*;

        #[test]
        fn parses_world_interface_reference_exports() {
                let src = r#"
package wast:core@0.1.0;

interface file-manager {
    bindgen: func(path: string) -> result<_, string>;
    read: func(path: string) -> result<string, string>;
}

world file-manager-world {
    export file-manager;
}
"#;

                let parsed = parse_world(src).expect("parse world");
                assert_eq!(parsed.world_name, "file-manager-world");
                assert_eq!(parsed.exports.len(), 2);
                assert_eq!(parsed.exports[0].wit_path, "file-manager/bindgen");
                assert_eq!(parsed.exports[1].wit_path, "file-manager/read");
        }
}
