use crate::serde_types::Syms;

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
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if !line.starts_with(' ') && !line.starts_with('\t') {
            let trimmed = line.trim();
            match trimmed.trim_end_matches(':') {
                "wit" => current = Section::Wit,
                "internal" => current = Section::Internal,
                "local" => current = Section::Local,
                other => {
                    return Err(format!("line {}: unknown section '{}'", line_num + 1, other));
                }
            }
            continue;
        }

        let trimmed = line.trim();
        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim().to_string();
            let value = value.trim().to_string();
            match current {
                Section::Wit => syms.wit_syms.push((key, value)),
                Section::Internal => syms.internal.push(crate::serde_types::SymEntry {
                    uid: key,
                    display_name: value,
                }),
                Section::Local => syms.local.push(crate::serde_types::SymEntry {
                    uid: key,
                    display_name: value,
                }),
                Section::None => {
                    return Err(format!("line {}: entry outside of any section", line_num + 1));
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
