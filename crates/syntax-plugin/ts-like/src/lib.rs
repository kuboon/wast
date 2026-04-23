#[allow(warnings)]
#[rustfmt::skip]
mod bindings;

use bindings::wast::core::types::*;
use std::collections::BTreeMap as HashMap;
use wast_pattern_analyzer::{ArithOp, CompareOp, Instruction};

struct Component;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a map from UID -> display name using the various sym tables.
fn build_func_name_map(syms: &Syms) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for entry in &syms.internal {
        map.insert(entry.uid.clone(), entry.display_name.clone());
    }
    for (uid, display) in &syms.wit_syms {
        map.insert(uid.clone(), display.clone());
    }
    map
}

fn build_local_name_map(syms: &Syms) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for entry in &syms.local {
        map.insert(entry.uid.clone(), entry.display_name.clone());
    }
    map
}

fn build_type_name_map(_types: &[(TypeUid, WastTypeDef)], syms: &Syms) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for entry in &syms.internal {
        map.insert(entry.uid.clone(), entry.display_name.clone());
    }
    for (uid, display) in &syms.wit_syms {
        map.insert(uid.clone(), display.clone());
    }
    map
}

fn resolve_type_ref(
    type_ref: &WitTypeRef,
    types: &[(TypeUid, WastTypeDef)],
    type_names: &HashMap<String, String>,
) -> String {
    for (uid, typedef) in types {
        if uid == type_ref {
            return format_wit_type(&typedef.definition, types, type_names);
        }
    }
    type_names
        .get(type_ref.as_str())
        .cloned()
        .unwrap_or_else(|| type_ref.clone())
}

fn format_wit_type(
    wit_type: &WitType,
    types: &[(TypeUid, WastTypeDef)],
    type_names: &HashMap<String, String>,
) -> String {
    match wit_type {
        WitType::Primitive(p) => primitive_name(p).to_string(),
        WitType::Option(inner) => {
            format!("option<{}>", resolve_type_ref(inner, types, type_names))
        }
        WitType::Result((ok, err)) => {
            format!(
                "result<{}, {}>",
                resolve_type_ref(ok, types, type_names),
                resolve_type_ref(err, types, type_names)
            )
        }
        WitType::List(inner) => {
            format!("list<{}>", resolve_type_ref(inner, types, type_names))
        }
        WitType::Record(fields) => {
            let parts: Vec<String> = fields
                .iter()
                .map(|(name, tref)| {
                    let n = type_names
                        .get(name.as_str())
                        .cloned()
                        .unwrap_or_else(|| name.clone());
                    format!("{}: {}", n, resolve_type_ref(tref, types, type_names))
                })
                .collect();
            format!("record {{ {} }}", parts.join(", "))
        }
        WitType::Variant(cases) => {
            let parts: Vec<String> = cases
                .iter()
                .map(|(name, tref)| {
                    let n = type_names
                        .get(name.as_str())
                        .cloned()
                        .unwrap_or_else(|| name.clone());
                    match tref {
                        Some(t) => format!("{}({})", n, resolve_type_ref(t, types, type_names)),
                        None => n,
                    }
                })
                .collect();
            format!("variant {{ {} }}", parts.join(", "))
        }
        WitType::Tuple(refs) => {
            let parts: Vec<String> = refs
                .iter()
                .map(|r| resolve_type_ref(r, types, type_names))
                .collect();
            format!("tuple<{}>", parts.join(", "))
        }
    }
}

fn primitive_name(p: &PrimitiveType) -> &'static str {
    match p {
        PrimitiveType::U32 => "u32",
        PrimitiveType::U64 => "u64",
        PrimitiveType::I32 => "i32",
        PrimitiveType::I64 => "i64",
        PrimitiveType::F32 => "f32",
        PrimitiveType::F64 => "f64",
        PrimitiveType::Bool => "bool",
        PrimitiveType::Char => "char",
        PrimitiveType::String => "string",
    }
}

fn parse_primitive(s: &str) -> Option<PrimitiveType> {
    match s {
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
// Body rendering
// ---------------------------------------------------------------------------

fn render_body(
    body: &[u8],
    indent: &str,
    local_names: &HashMap<String, String>,
    func_names: &HashMap<String, String>,
) -> String {
    match wast_pattern_analyzer::deserialize_body(body) {
        Ok(instructions) => render_instructions(&instructions, indent, local_names, func_names),
        Err(_) => format!("{}// [body: {} bytes]", indent, body.len()),
    }
}

fn render_instructions(
    instructions: &[Instruction],
    indent: &str,
    local_names: &HashMap<String, String>,
    func_names: &HashMap<String, String>,
) -> String {
    let mut lines = Vec::new();
    for instr in instructions {
        let rendered = render_instruction(instr, indent, local_names, func_names);
        if !rendered.is_empty() {
            lines.push(rendered);
        }
    }
    lines.join("\n")
}

fn resolve_local_name(uid: &str, local_names: &HashMap<String, String>) -> String {
    local_names
        .get(uid)
        .cloned()
        .unwrap_or_else(|| uid.to_string())
}

fn resolve_func_name_body(uid: &str, func_names: &HashMap<String, String>) -> String {
    func_names
        .get(uid)
        .cloned()
        .unwrap_or_else(|| uid.to_string())
}

fn render_instruction(
    instr: &Instruction,
    indent: &str,
    local_names: &HashMap<String, String>,
    func_names: &HashMap<String, String>,
) -> String {
    let inner = format!("{}  ", indent);
    match instr {
        Instruction::Nop => String::new(),
        Instruction::Return => format!("{}return;", indent),
        Instruction::Const { value } => format!("{}{}", indent, value),
        Instruction::LocalGet { uid } => {
            format!("{}{}", indent, resolve_local_name(uid, local_names))
        }
        Instruction::LocalSet { uid, value } => {
            let name = resolve_local_name(uid, local_names);
            let val = render_expr(value, local_names, func_names);
            format!("{}let {} = {};", indent, name, val)
        }
        Instruction::Call { func_uid, args } => {
            let name = resolve_func_name_body(func_uid, func_names);
            let args_str = args
                .iter()
                .map(|(_, arg)| render_expr(arg, local_names, func_names))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}{}({})", indent, name, args_str)
        }
        Instruction::Compare { op, lhs, rhs } => {
            let l = render_expr(lhs, local_names, func_names);
            let r = render_expr(rhs, local_names, func_names);
            let op_str = match op {
                CompareOp::Eq => "===",
                CompareOp::Ne => "!==",
                CompareOp::Lt => "<",
                CompareOp::Le => "<=",
                CompareOp::Gt => ">",
                CompareOp::Ge => ">=",
            };
            format!("{}{} {} {}", indent, l, op_str, r)
        }
        Instruction::Arithmetic { op, lhs, rhs } => {
            let l = render_expr(lhs, local_names, func_names);
            let r = render_expr(rhs, local_names, func_names);
            let op_str = match op {
                ArithOp::Add => "+",
                ArithOp::Sub => "-",
                ArithOp::Mul => "*",
                ArithOp::Div => "/",
            };
            format!("{}{} {} {}", indent, l, op_str, r)
        }
        Instruction::If {
            condition,
            then_body,
            else_body,
        } => {
            let cond = render_expr(condition, local_names, func_names);
            let then_str = render_instructions(then_body, &inner, local_names, func_names);
            if else_body.is_empty() {
                format!("{}if ({}) {{\n{}\n{}}}", indent, cond, then_str, indent)
            } else {
                let else_str = render_instructions(else_body, &inner, local_names, func_names);
                format!(
                    "{}if ({}) {{\n{}\n{}}} else {{\n{}\n{}}}",
                    indent, cond, then_str, indent, else_str, indent
                )
            }
        }
        Instruction::Loop { label, body } => {
            let body_str = render_instructions(body, &inner, local_names, func_names);
            let label_comment = match label {
                Some(l) => format!(" // {}", l),
                None => String::new(),
            };
            format!(
                "{}while (true) {{{}\n{}\n{}}}",
                indent, label_comment, body_str, indent
            )
        }
        Instruction::Block { label, body } => {
            let body_str = render_instructions(body, &inner, local_names, func_names);
            let label_comment = match label {
                Some(l) => format!(" // {}", l),
                None => String::new(),
            };
            format!("{}{{{}\n{}\n{}}}", indent, label_comment, body_str, indent)
        }
        Instruction::BrIf { label, condition } => {
            let cond = render_expr(condition, local_names, func_names);
            format!("{}if ({}) break {}; // break", indent, cond, label)
        }
        Instruction::Br { label } => format!("{}break {}; // break", indent, label),
        Instruction::Some { value } => {
            let val = render_expr(value, local_names, func_names);
            format!("{}some({})", indent, val)
        }
        Instruction::None => format!("{}none", indent),
        Instruction::Ok { value } => {
            let val = render_expr(value, local_names, func_names);
            format!("{}ok({})", indent, val)
        }
        Instruction::Err { value } => {
            let val = render_expr(value, local_names, func_names);
            format!("{}err({})", indent, val)
        }
        Instruction::IsErr { value } => {
            let val = render_expr(value, local_names, func_names);
            format!("{}isErr({})", indent, val)
        }
        Instruction::StringLen { value } => {
            let val = render_expr(value, local_names, func_names);
            format!("{}{}.length", indent, val)
        }
        Instruction::StringLiteral { bytes } => {
            let s = String::from_utf8_lossy(bytes);
            format!("{indent}{:?}", &*s)
        }
        Instruction::ListLen { value } => {
            let val = render_expr(value, local_names, func_names);
            format!("{}{}.length", indent, val)
        }
        Instruction::MatchOption {
            value,
            some_binding,
            some_body,
            none_body,
        } => {
            let val = render_expr(value, local_names, func_names);
            let binding = resolve_local_name(some_binding, local_names);
            let some_str = render_instructions(some_body, &inner, local_names, func_names);
            let none_str = render_instructions(none_body, &inner, local_names, func_names);
            format!(
                "{}switch ({}) {{\n{}case some({}):\n{}\n{}case none:\n{}\n{}}}",
                indent, val, indent, binding, some_str, indent, none_str, indent
            )
        }
        Instruction::MatchResult {
            value,
            ok_binding,
            ok_body,
            err_binding,
            err_body,
        } => {
            let val = render_expr(value, local_names, func_names);
            let ok_bind = resolve_local_name(ok_binding, local_names);
            let err_bind = resolve_local_name(err_binding, local_names);
            let ok_str = render_instructions(ok_body, &inner, local_names, func_names);
            let err_str = render_instructions(err_body, &inner, local_names, func_names);
            format!(
                "{}switch ({}) {{\n{}case ok({}):\n{}\n{}case err({}):\n{}\n{}}}",
                indent, val, indent, ok_bind, ok_str, indent, err_bind, err_str, indent
            )
        }
    }
}

fn render_expr(
    instr: &Instruction,
    local_names: &HashMap<String, String>,
    func_names: &HashMap<String, String>,
) -> String {
    match instr {
        Instruction::Nop => String::new(),
        Instruction::Return => "return".to_string(),
        Instruction::Const { value } => format!("{}", value),
        Instruction::LocalGet { uid } => resolve_local_name(uid, local_names),
        Instruction::LocalSet { uid, value } => {
            let name = resolve_local_name(uid, local_names);
            let val = render_expr(value, local_names, func_names);
            format!("{} = {}", name, val)
        }
        Instruction::Call { func_uid, args } => {
            let name = resolve_func_name_body(func_uid, func_names);
            let args_str = args
                .iter()
                .map(|(_, arg)| render_expr(arg, local_names, func_names))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({})", name, args_str)
        }
        Instruction::Compare { op, lhs, rhs } => {
            let l = render_expr(lhs, local_names, func_names);
            let r = render_expr(rhs, local_names, func_names);
            let op_str = match op {
                CompareOp::Eq => "===",
                CompareOp::Ne => "!==",
                CompareOp::Lt => "<",
                CompareOp::Le => "<=",
                CompareOp::Gt => ">",
                CompareOp::Ge => ">=",
            };
            format!("{} {} {}", l, op_str, r)
        }
        Instruction::Arithmetic { op, lhs, rhs } => {
            let l = render_expr(lhs, local_names, func_names);
            let r = render_expr(rhs, local_names, func_names);
            let op_str = match op {
                ArithOp::Add => "+",
                ArithOp::Sub => "-",
                ArithOp::Mul => "*",
                ArithOp::Div => "/",
            };
            format!("{} {} {}", l, op_str, r)
        }
        Instruction::Some { value } => {
            let val = render_expr(value, local_names, func_names);
            format!("some({})", val)
        }
        Instruction::None => "none".to_string(),
        Instruction::Ok { value } => {
            let val = render_expr(value, local_names, func_names);
            format!("ok({})", val)
        }
        Instruction::Err { value } => {
            let val = render_expr(value, local_names, func_names);
            format!("err({})", val)
        }
        Instruction::IsErr { value } => {
            let val = render_expr(value, local_names, func_names);
            format!("isErr({})", val)
        }
        _ => "(...)".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Body parsing (from_text support)
// ---------------------------------------------------------------------------

fn resolve_to_uid(name: &str, rev_map: &HashMap<String, String>) -> String {
    rev_map
        .get(name)
        .cloned()
        .unwrap_or_else(|| name.to_string())
}

fn find_matching_paren_str(s: &str, open_pos: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    for i in open_pos..bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Find the rightmost occurrence of `pattern` in `s` at paren depth 0.
fn find_rightmost_top_level(s: &str, pattern: &str) -> Option<usize> {
    let pat = pattern.as_bytes();
    let pat_len = pat.len();
    if s.len() < pat_len {
        return None;
    }
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut last = None;
    for i in 0..=s.len() - pat_len {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        if depth == 0 && &bytes[i..i + pat_len] == pat {
            last = Some(i);
        }
    }
    last
}

/// Split `s` on `delimiter` at top-level (not inside parens).
fn split_top_level(s: &str, delimiter: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            c if c == delimiter && depth == 0 => {
                parts.push(&s[start..i]);
                start = i + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

/// Skip past a `{...}` block (brace-counting). `i` should point to the first
/// line inside the block (after the opening `{`). On return, `i` points to the
/// line after the closing `}`.
fn skip_block(lines: &[&str], i: &mut usize) {
    let mut depth = 1i32;
    while *i < lines.len() {
        for ch in lines[*i].chars() {
            if ch == '{' {
                depth += 1;
            }
            if ch == '}' {
                depth -= 1;
            }
        }
        *i += 1;
        if depth <= 0 {
            break;
        }
    }
}

/// Parse a sequence of statements from `lines[*i..]`.
/// Stops (without consuming) at `}`, `} else`, or `case ` lines.
fn parse_stmts(
    lines: &[&str],
    i: &mut usize,
    rev_local: &HashMap<String, String>,
    rev_func: &HashMap<String, String>,
) -> Vec<Instruction> {
    let mut instrs = Vec::new();
    while *i < lines.len() {
        let trimmed = lines[*i].trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            *i += 1;
            continue;
        }
        if trimmed == "}" || trimmed.starts_with("} else") || trimmed.starts_with("case ") {
            break;
        }
        let saved = *i;
        match parse_stmt(lines, i, rev_local, rev_func) {
            Ok(instr) => instrs.push(instr),
            Err(_) => {
                *i = saved;
                if lines[*i].trim().ends_with('{') {
                    *i += 1;
                    skip_block(lines, i);
                } else {
                    *i += 1;
                }
            }
        }
    }
    instrs
}

/// Parse a single statement starting at `lines[*i]`.
fn parse_stmt(
    lines: &[&str],
    i: &mut usize,
    rev_local: &HashMap<String, String>,
    rev_func: &HashMap<String, String>,
) -> Result<Instruction, String> {
    let trimmed = lines[*i].trim();

    // return;
    if trimmed == "return;" {
        *i += 1;
        return Ok(Instruction::Return);
    }

    // let NAME = EXPR;
    if let Some(rest) = trimmed.strip_prefix("let ") {
        if let Some(rest) = rest.strip_suffix(';') {
            if let Some(eq) = rest.find(" = ") {
                let name = rest[..eq].trim();
                let expr_str = rest[eq + 3..].trim();
                let uid = resolve_to_uid(name, rev_local);
                let value = parse_expr_str(expr_str, rev_local, rev_func)?;
                *i += 1;
                return Ok(Instruction::LocalSet {
                    uid,
                    value: Box::new(value),
                });
            }
        }
    }

    // break LABEL; // break
    if trimmed.starts_with("break ") && trimmed.contains("// break") {
        let label = trimmed
            .strip_prefix("break ")
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .trim()
            .to_string();
        *i += 1;
        return Ok(Instruction::Br { label });
    }

    // if (COND) break LABEL; // break → BrIf
    if trimmed.starts_with("if (") && trimmed.contains(") break ") && trimmed.contains("// break") {
        let after = &trimmed[3..]; // skip "if "
        let cp = find_matching_paren_str(after, 0).ok_or("unmatched paren in BrIf")?;
        let cond_str = &after[1..cp];
        let condition = parse_expr_str(cond_str, rev_local, rev_func)?;
        let rest = after[cp + 1..].trim();
        let label = rest
            .strip_prefix("break ")
            .and_then(|s| s.split(';').next())
            .map(|s| s.trim().to_string())
            .ok_or("cannot parse BrIf label")?;
        *i += 1;
        return Ok(Instruction::BrIf {
            label,
            condition: Box::new(condition),
        });
    }

    // if (COND) { ... } [else { ... }]
    if trimmed.starts_with("if (") && trimmed.ends_with('{') {
        let after = &trimmed[3..];
        let cp = find_matching_paren_str(after, 0).ok_or("unmatched paren in if")?;
        let cond_str = &after[1..cp];
        let condition = parse_expr_str(cond_str, rev_local, rev_func)?;
        *i += 1;
        let then_body = parse_stmts(lines, i, rev_local, rev_func);
        let else_body = if *i < lines.len() && lines[*i].trim().starts_with("} else {") {
            *i += 1;
            parse_stmts(lines, i, rev_local, rev_func)
        } else {
            vec![]
        };
        if *i < lines.len() && lines[*i].trim() == "}" {
            *i += 1;
        }
        return Ok(Instruction::If {
            condition: Box::new(condition),
            then_body,
            else_body,
        });
    }

    // while (true) { // LABEL ... }
    if trimmed.starts_with("while (true) {") {
        let label = if trimmed.contains("// ") {
            Some(trimmed.rsplit("// ").next().unwrap().trim().to_string())
        } else {
            None
        };
        *i += 1;
        let body = parse_stmts(lines, i, rev_local, rev_func);
        if *i < lines.len() && lines[*i].trim() == "}" {
            *i += 1;
        }
        return Ok(Instruction::Loop { label, body });
    }

    // { // LABEL ... } (Block)
    if (trimmed == "{" || (trimmed.starts_with('{') && trimmed.contains("// ")))
        && !trimmed.contains('(')
    {
        let label = if trimmed.contains("// ") {
            Some(trimmed.rsplit("// ").next().unwrap().trim().to_string())
        } else {
            None
        };
        *i += 1;
        let body = parse_stmts(lines, i, rev_local, rev_func);
        if *i < lines.len() && lines[*i].trim() == "}" {
            *i += 1;
        }
        return Ok(Instruction::Block { label, body });
    }

    // switch (EXPR) { case ... }
    if trimmed.starts_with("switch (") {
        let after = &trimmed[7..];
        let cp = find_matching_paren_str(after, 0).ok_or("unmatched paren in switch")?;
        let val_str = &after[1..cp];
        let value = parse_expr_str(val_str, rev_local, rev_func)?;
        *i += 1;

        let mut cases: Vec<(String, Vec<Instruction>)> = Vec::new();
        while *i < lines.len() {
            let cl = lines[*i].trim();
            if cl == "}" {
                *i += 1;
                break;
            }
            if cl.starts_with("case ") && cl.ends_with(':') {
                let label = cl[5..cl.len() - 1].trim().to_string();
                *i += 1;
                let body = parse_stmts(lines, i, rev_local, rev_func);
                cases.push((label, body));
            } else {
                *i += 1;
            }
        }
        return build_match_instruction(value, cases, rev_local);
    }

    // Fall through: expression statement (strip optional trailing semicolon)
    let expr_str = trimmed.trim_end_matches(';');
    let instr = parse_expr_str(expr_str, rev_local, rev_func)?;
    *i += 1;
    Ok(instr)
}

fn parse_expr_str(
    s: &str,
    rev_local: &HashMap<String, String>,
    rev_func: &HashMap<String, String>,
) -> Result<Instruction, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty expression".into());
    }

    // Parenthesized expression
    if s.starts_with('(') {
        if let Some(cp) = find_matching_paren_str(s, 0) {
            if cp == s.len() - 1 {
                return parse_expr_str(&s[1..cp], rev_local, rev_func);
            }
        }
    }

    // Comparison operators (lowest precedence — parsed first = outermost)
    if let Some(pos) = find_rightmost_top_level(s, " === ") {
        return Ok(Instruction::Compare {
            op: CompareOp::Eq,
            lhs: Box::new(parse_expr_str(&s[..pos], rev_local, rev_func)?),
            rhs: Box::new(parse_expr_str(&s[pos + 5..], rev_local, rev_func)?),
        });
    }
    if let Some(pos) = find_rightmost_top_level(s, " !== ") {
        return Ok(Instruction::Compare {
            op: CompareOp::Ne,
            lhs: Box::new(parse_expr_str(&s[..pos], rev_local, rev_func)?),
            rhs: Box::new(parse_expr_str(&s[pos + 5..], rev_local, rev_func)?),
        });
    }
    if let Some(pos) = find_rightmost_top_level(s, " <= ") {
        return Ok(Instruction::Compare {
            op: CompareOp::Le,
            lhs: Box::new(parse_expr_str(&s[..pos], rev_local, rev_func)?),
            rhs: Box::new(parse_expr_str(&s[pos + 4..], rev_local, rev_func)?),
        });
    }
    if let Some(pos) = find_rightmost_top_level(s, " >= ") {
        return Ok(Instruction::Compare {
            op: CompareOp::Ge,
            lhs: Box::new(parse_expr_str(&s[..pos], rev_local, rev_func)?),
            rhs: Box::new(parse_expr_str(&s[pos + 4..], rev_local, rev_func)?),
        });
    }
    if let Some(pos) = find_rightmost_top_level(s, " < ") {
        return Ok(Instruction::Compare {
            op: CompareOp::Lt,
            lhs: Box::new(parse_expr_str(&s[..pos], rev_local, rev_func)?),
            rhs: Box::new(parse_expr_str(&s[pos + 3..], rev_local, rev_func)?),
        });
    }
    if let Some(pos) = find_rightmost_top_level(s, " > ") {
        return Ok(Instruction::Compare {
            op: CompareOp::Gt,
            lhs: Box::new(parse_expr_str(&s[..pos], rev_local, rev_func)?),
            rhs: Box::new(parse_expr_str(&s[pos + 3..], rev_local, rev_func)?),
        });
    }

    // Additive
    if let Some(pos) = find_rightmost_top_level(s, " + ") {
        return Ok(Instruction::Arithmetic {
            op: ArithOp::Add,
            lhs: Box::new(parse_expr_str(&s[..pos], rev_local, rev_func)?),
            rhs: Box::new(parse_expr_str(&s[pos + 3..], rev_local, rev_func)?),
        });
    }
    if let Some(pos) = find_rightmost_top_level(s, " - ") {
        return Ok(Instruction::Arithmetic {
            op: ArithOp::Sub,
            lhs: Box::new(parse_expr_str(&s[..pos], rev_local, rev_func)?),
            rhs: Box::new(parse_expr_str(&s[pos + 3..], rev_local, rev_func)?),
        });
    }

    // Multiplicative
    if let Some(pos) = find_rightmost_top_level(s, " * ") {
        return Ok(Instruction::Arithmetic {
            op: ArithOp::Mul,
            lhs: Box::new(parse_expr_str(&s[..pos], rev_local, rev_func)?),
            rhs: Box::new(parse_expr_str(&s[pos + 3..], rev_local, rev_func)?),
        });
    }
    if let Some(pos) = find_rightmost_top_level(s, " / ") {
        return Ok(Instruction::Arithmetic {
            op: ArithOp::Div,
            lhs: Box::new(parse_expr_str(&s[..pos], rev_local, rev_func)?),
            rhs: Box::new(parse_expr_str(&s[pos + 3..], rev_local, rev_func)?),
        });
    }

    parse_atom(s, rev_local, rev_func)
}

fn parse_atom(
    s: &str,
    rev_local: &HashMap<String, String>,
    rev_func: &HashMap<String, String>,
) -> Result<Instruction, String> {
    let s = s.trim();

    if s == "return" {
        return Ok(Instruction::Return);
    }
    if s == "none" {
        return Ok(Instruction::None);
    }
    if let Ok(value) = s.parse::<i64>() {
        return Ok(Instruction::Const { value });
    }

    // some(EXPR)
    if let Some(inner) = s.strip_prefix("some(") {
        if let Some(inner) = inner.strip_suffix(')') {
            let value = parse_expr_str(inner, rev_local, rev_func)?;
            return Ok(Instruction::Some {
                value: Box::new(value),
            });
        }
    }
    // ok(EXPR)
    if let Some(inner) = s.strip_prefix("ok(") {
        if let Some(inner) = inner.strip_suffix(')') {
            let value = parse_expr_str(inner, rev_local, rev_func)?;
            return Ok(Instruction::Ok {
                value: Box::new(value),
            });
        }
    }
    // err(EXPR)
    if let Some(inner) = s.strip_prefix("err(") {
        if let Some(inner) = inner.strip_suffix(')') {
            let value = parse_expr_str(inner, rev_local, rev_func)?;
            return Ok(Instruction::Err {
                value: Box::new(value),
            });
        }
    }
    // isErr(EXPR)
    if let Some(inner) = s.strip_prefix("isErr(") {
        if let Some(inner) = inner.strip_suffix(')') {
            let value = parse_expr_str(inner, rev_local, rev_func)?;
            return Ok(Instruction::IsErr {
                value: Box::new(value),
            });
        }
    }

    // NAME(ARGS) — function call
    if let Some(paren_pos) = s.find('(') {
        if s.ends_with(')') {
            let func_name = s[..paren_pos].trim();
            if !func_name.is_empty() {
                let args_str = &s[paren_pos + 1..s.len() - 1];
                let func_uid = resolve_to_uid(func_name, rev_func);
                let args = if args_str.trim().is_empty() {
                    vec![]
                } else {
                    split_top_level(args_str, ',')
                        .into_iter()
                        .map(|a| {
                            let instr = parse_expr_str(a.trim(), rev_local, rev_func)?;
                            Ok(("".to_string(), instr))
                        })
                        .collect::<Result<Vec<_>, String>>()?
                };
                return Ok(Instruction::Call { func_uid, args });
            }
        }
    }

    // Variable reference
    if s.chars().all(|c| c.is_alphanumeric() || c == '_') {
        let uid = resolve_to_uid(s, rev_local);
        return Ok(Instruction::LocalGet { uid });
    }

    Err(format!("cannot parse expression: {}", s))
}

fn build_match_instruction(
    value: Instruction,
    cases: Vec<(String, Vec<Instruction>)>,
    rev_local: &HashMap<String, String>,
) -> Result<Instruction, String> {
    let mut some_case: Option<(String, Vec<Instruction>)> = None;
    let mut none_case: Option<Vec<Instruction>> = None;
    let mut ok_case: Option<(String, Vec<Instruction>)> = None;
    let mut err_case: Option<(String, Vec<Instruction>)> = None;

    for (label, body) in cases {
        if let Some(inner) = label.strip_prefix("some(") {
            if let Some(binding) = inner.strip_suffix(')') {
                some_case = Some((resolve_to_uid(binding.trim(), rev_local), body));
                continue;
            }
        }
        if label == "none" {
            none_case = Some(body);
            continue;
        }
        if let Some(inner) = label.strip_prefix("ok(") {
            if let Some(binding) = inner.strip_suffix(')') {
                ok_case = Some((resolve_to_uid(binding.trim(), rev_local), body));
                continue;
            }
        }
        if let Some(inner) = label.strip_prefix("err(") {
            if let Some(binding) = inner.strip_suffix(')') {
                err_case = Some((resolve_to_uid(binding.trim(), rev_local), body));
                continue;
            }
        }
    }

    if let (Some((some_binding, some_body)), Some(none_body)) = (some_case, none_case) {
        Ok(Instruction::MatchOption {
            value: Box::new(value),
            some_binding,
            some_body,
            none_body,
        })
    } else if let (Some((ok_binding, ok_body)), Some((err_binding, err_body))) = (ok_case, err_case)
    {
        Ok(Instruction::MatchResult {
            value: Box::new(value),
            ok_binding,
            ok_body,
            err_binding,
            err_body,
        })
    } else {
        Err("cannot determine match type from case labels".into())
    }
}

/// Parse function body lines and return serialized instructions (or existing
/// body when the text consists entirely of comments / is empty).
fn parse_func_body(
    lines: &[&str],
    i: &mut usize,
    rev_local: &HashMap<String, String>,
    rev_func: &HashMap<String, String>,
    existing_body: Option<Vec<u8>>,
) -> Option<Vec<u8>> {
    let instructions = parse_stmts(lines, i, rev_local, rev_func);
    if *i < lines.len() && lines[*i].trim() == "}" {
        *i += 1;
    }
    if instructions.is_empty() {
        existing_body
    } else {
        Some(wast_pattern_analyzer::serialize_body(&instructions))
    }
}

// ---------------------------------------------------------------------------
// to_text
// ---------------------------------------------------------------------------

fn func_to_text(
    func_uid: &str,
    func: &WastFunc,
    func_names: &HashMap<String, String>,
    local_names: &HashMap<String, String>,
    type_names: &HashMap<String, String>,
    types: &[(TypeUid, WastTypeDef)],
) -> String {
    let source_uid = match &func.source {
        FuncSource::Internal(u) | FuncSource::Imported(u) | FuncSource::Exported(u) => u.clone(),
    };

    let name = func_names
        .get(&source_uid)
        .or_else(|| func_names.get(func_uid))
        .cloned()
        .unwrap_or_else(|| func_uid.to_string());

    let params_str = func
        .params
        .iter()
        .map(|(param_uid, type_ref)| {
            let pname = local_names
                .get(param_uid.as_str())
                .cloned()
                .unwrap_or_else(|| param_uid.clone());
            let tname = resolve_type_ref(type_ref, types, type_names);
            format!("{}: {}", pname, tname)
        })
        .collect::<Vec<_>>()
        .join(", ");

    let result_str = match &func.result {
        Some(type_ref) => format!(": {}", resolve_type_ref(type_ref, types, type_names)),
        None => String::new(),
    };

    match &func.source {
        FuncSource::Imported(_) => {
            format!("declare function {}({}){};", name, params_str, result_str)
        }
        FuncSource::Exported(_) => {
            let body_str = match &func.body {
                Some(b) => render_body(b, "  ", local_names, func_names),
                None => "  // [no body]".to_string(),
            };
            format!(
                "export function {}({}){} {{\n{}\n}}",
                name, params_str, result_str, body_str
            )
        }
        FuncSource::Internal(_) => {
            let body_str = match &func.body {
                Some(b) => render_body(b, "  ", local_names, func_names),
                None => "  // [no body]".to_string(),
            };
            format!(
                "function {}({}){} {{\n{}\n}}",
                name, params_str, result_str, body_str
            )
        }
    }
}

// ---------------------------------------------------------------------------
// from_text — parser
// ---------------------------------------------------------------------------

struct ParsedFunc {
    name: String,
    params: Vec<(String, String)>, // (param_name, type_string)
    result_type: Option<String>,
    _is_import: bool,
    _is_export: bool,
}

fn parse_type_ref_str(
    s: &str,
    types: &[(TypeUid, WastTypeDef)],
    type_names: &HashMap<String, String>,
) -> WitTypeRef {
    let s = s.trim();
    if parse_primitive(s).is_some() {
        for (uid, td) in types {
            if let WitType::Primitive(p) = &td.definition {
                if primitive_name(p) == s {
                    return uid.clone();
                }
            }
        }
        for (uid, name) in type_names {
            if name == s {
                return uid.clone();
            }
        }
        s.to_string()
    } else {
        for (uid, name) in type_names {
            if name == s {
                return uid.clone();
            }
        }
        s.to_string()
    }
}

/// Parse a signature string like `name(p1: type1, p2: type2): ret`
/// Also supports `name(p1: type1, p2: type2)` with no return type.
fn parse_signature(sig: &str) -> Option<ParsedFunc> {
    let sig = sig.trim();

    let paren_open = sig.find('(')?;
    let name = sig[..paren_open].trim().to_string();
    if name.is_empty() {
        return None;
    }

    let rest = &sig[paren_open + 1..];
    let paren_close = rest.find(')')?;
    let params_str = &rest[..paren_close];
    let after_params = rest[paren_close + 1..].trim();

    let params: Vec<(String, String)> = if params_str.trim().is_empty() {
        vec![]
    } else {
        params_str
            .split(',')
            .map(|p| {
                let p = p.trim();
                if let Some(colon) = p.find(':') {
                    (
                        p[..colon].trim().to_string(),
                        p[colon + 1..].trim().to_string(),
                    )
                } else {
                    (p.to_string(), "unknown".to_string())
                }
            })
            .collect()
    };

    // TS-like uses `: returnType` instead of `-> returnType`
    let result_type = if after_params.starts_with(':') {
        let ret = after_params[1..].trim();
        // Strip trailing semicolon or opening brace context
        let ret = ret.trim_end_matches(';').trim_end_matches('{').trim();
        if ret.is_empty() {
            None
        } else {
            Some(ret.to_string())
        }
    } else {
        None
    };

    Some(ParsedFunc {
        name,
        params,
        result_type,
        _is_import: false,
        _is_export: false,
    })
}

fn generate_uid() -> String {
    use core::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0xa000);
    let val = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:04x}", val & 0xffff)
}

// ---------------------------------------------------------------------------
// Guest implementation
// ---------------------------------------------------------------------------

impl bindings::exports::wast::core::syntax_plugin::Guest for Component {
    fn to_text(component: WastComponent) -> String {
        let func_names = build_func_name_map(&component.syms);
        let local_names = build_local_name_map(&component.syms);
        let type_names = build_type_name_map(&component.types, &component.syms);

        let mut parts: Vec<String> = Vec::new();

        for (func_uid, func) in &component.funcs {
            parts.push(func_to_text(
                func_uid,
                func,
                &func_names,
                &local_names,
                &type_names,
                &component.types,
            ));
        }

        parts.join("\n\n")
    }

    fn from_text(text: String, existing: WastComponent) -> Result<WastComponent, Vec<WastError>> {
        let func_names = build_func_name_map(&existing.syms);
        let local_names = build_local_name_map(&existing.syms);
        let type_names = build_type_name_map(&existing.types, &existing.syms);

        // Reverse maps: display_name -> uid
        let rev_func: HashMap<String, String> = func_names
            .iter()
            .map(|(k, v)| (v.clone(), k.clone()))
            .collect();
        let rev_local: HashMap<String, String> = local_names
            .iter()
            .map(|(k, v)| (v.clone(), k.clone()))
            .collect();

        // Existing funcs by uid for body preservation
        let existing_funcs: HashMap<String, &WastFunc> = existing
            .funcs
            .iter()
            .map(|(uid, f)| (uid.clone(), f))
            .collect();

        // Also build a map from source uid -> (func_uid, func)
        let existing_by_source: HashMap<String, (&str, &WastFunc)> = existing
            .funcs
            .iter()
            .map(|(uid, f)| {
                let src = match &f.source {
                    FuncSource::Internal(u) | FuncSource::Imported(u) | FuncSource::Exported(u) => {
                        u.clone()
                    }
                };
                (src, (uid.as_str(), f))
            })
            .collect();

        let mut errors: Vec<WastError> = Vec::new();
        let mut funcs: Vec<(FuncUid, WastFunc)> = Vec::new();
        let mut new_syms_internal: Vec<SymEntry> = existing.syms.internal.clone();
        let mut new_syms_local: Vec<SymEntry> = existing.syms.local.clone();

        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i].trim();

            // Skip empty lines and pure comments
            if line.is_empty() || (line.starts_with("//") && !line.starts_with("// [")) {
                i += 1;
                continue;
            }

            // Handle: declare function name(params): result;
            if line.starts_with("declare function ") {
                let sig_str = &line["declare function ".len()..];
                // Strip trailing semicolon for parsing
                let sig_str = sig_str.trim_end_matches(';');
                match parse_signature(sig_str) {
                    Some(parsed) => {
                        let (func_uid, source_uid) =
                            resolve_func_uid(&parsed.name, &rev_func, &existing_by_source, true);

                        let params = resolve_params(
                            &parsed.params,
                            &rev_local,
                            &existing.types,
                            &type_names,
                            &mut new_syms_local,
                        );
                        let result = parsed
                            .result_type
                            .as_ref()
                            .map(|r| parse_type_ref_str(r, &existing.types, &type_names));

                        let body = existing_by_source
                            .get(&source_uid)
                            .and_then(|(_, f)| f.body.clone())
                            .or_else(|| existing_funcs.get(&func_uid).and_then(|f| f.body.clone()));

                        funcs.push((
                            func_uid,
                            WastFunc {
                                source: FuncSource::Imported(source_uid),
                                params,
                                result,
                                body,
                            },
                        ));
                    }
                    None => {
                        errors.push(WastError {
                            message: format!(
                                "parse_error: cannot parse declare function: {}",
                                line
                            ),
                            location: Some(format!("line {}", i + 1)),
                        });
                    }
                }
                i += 1;
                continue;
            }

            // Handle: export function name(params): result { ... }
            if line.starts_with("export function ") {
                let sig_str = &line["export function ".len()..];
                // Strip trailing `{` if present
                let sig_str = sig_str.trim_end_matches('{').trim();
                match parse_signature(sig_str) {
                    Some(parsed) => {
                        let (func_uid, source_uid) =
                            resolve_func_uid(&parsed.name, &rev_func, &existing_by_source, false);

                        let existing_body = existing_by_source
                            .get(&source_uid)
                            .and_then(|(_, f)| f.body.clone())
                            .or_else(|| existing_funcs.get(&func_uid).and_then(|f| f.body.clone()));

                        i += 1;
                        let body =
                            parse_func_body(&lines, &mut i, &rev_local, &rev_func, existing_body);

                        let params = resolve_params(
                            &parsed.params,
                            &rev_local,
                            &existing.types,
                            &type_names,
                            &mut new_syms_local,
                        );
                        let result = parsed
                            .result_type
                            .as_ref()
                            .map(|r| parse_type_ref_str(r, &existing.types, &type_names));

                        ensure_func_sym(&source_uid, &parsed.name, &mut new_syms_internal);

                        funcs.push((
                            func_uid,
                            WastFunc {
                                source: FuncSource::Exported(source_uid),
                                params,
                                result,
                                body,
                            },
                        ));
                    }
                    None => {
                        errors.push(WastError {
                            message: format!("parse_error: cannot parse export function: {}", line),
                            location: Some(format!("line {}", i + 1)),
                        });
                        i += 1;
                    }
                }
                continue;
            }

            // Handle: function name(params): result { ... }
            if line.starts_with("function ") {
                let sig_str = &line["function ".len()..];
                let sig_str = sig_str.trim_end_matches('{').trim();
                match parse_signature(sig_str) {
                    Some(parsed) => {
                        let (func_uid, source_uid) =
                            resolve_func_uid(&parsed.name, &rev_func, &existing_by_source, false);

                        let existing_body = existing_by_source
                            .get(&source_uid)
                            .and_then(|(_, f)| f.body.clone())
                            .or_else(|| existing_funcs.get(&func_uid).and_then(|f| f.body.clone()));

                        i += 1;
                        let body =
                            parse_func_body(&lines, &mut i, &rev_local, &rev_func, existing_body);

                        let params = resolve_params(
                            &parsed.params,
                            &rev_local,
                            &existing.types,
                            &type_names,
                            &mut new_syms_local,
                        );
                        let result = parsed
                            .result_type
                            .as_ref()
                            .map(|r| parse_type_ref_str(r, &existing.types, &type_names));

                        ensure_func_sym(&source_uid, &parsed.name, &mut new_syms_internal);

                        funcs.push((
                            func_uid,
                            WastFunc {
                                source: FuncSource::Internal(source_uid),
                                params,
                                result,
                                body,
                            },
                        ));
                    }
                    None => {
                        errors.push(WastError {
                            message: format!("parse_error: cannot parse function: {}", line),
                            location: Some(format!("line {}", i + 1)),
                        });
                        i += 1;
                    }
                }
                continue;
            }

            // Unrecognized line
            errors.push(WastError {
                message: format!("parse_error: unexpected line: {}", line),
                location: Some(format!("line {}", i + 1)),
            });
            i += 1;
        }

        if !errors.is_empty() {
            return Err(errors);
        }

        Ok(WastComponent {
            funcs,
            types: existing.types,
            syms: Syms {
                wit_syms: existing.syms.wit_syms,
                internal: new_syms_internal,
                local: new_syms_local,
            },
        })
    }
}

/// Resolve a function name to (func_uid, source_uid).
fn resolve_func_uid(
    name: &str,
    rev_func: &HashMap<String, String>,
    existing_by_source: &HashMap<String, (&str, &WastFunc)>,
    _is_import: bool,
) -> (String, String) {
    if let Some(source_uid) = rev_func.get(name) {
        if let Some((func_uid, _)) = existing_by_source.get(source_uid.as_str()) {
            (func_uid.to_string(), source_uid.clone())
        } else {
            (source_uid.clone(), source_uid.clone())
        }
    } else {
        let uid = generate_uid();
        (uid.clone(), uid)
    }
}

/// Resolve parameter names and types from parsed strings.
fn resolve_params(
    parsed: &[(String, String)],
    rev_local: &HashMap<String, String>,
    types: &[(TypeUid, WastTypeDef)],
    type_names: &HashMap<String, String>,
    new_syms_local: &mut Vec<SymEntry>,
) -> Vec<(FuncUid, WitTypeRef)> {
    parsed
        .iter()
        .map(|(pname, ptype)| {
            let param_uid = if let Some(uid) = rev_local.get(pname.as_str()) {
                uid.clone()
            } else {
                let uid = generate_uid();
                new_syms_local.push(SymEntry {
                    uid: uid.clone(),
                    display_name: pname.clone(),
                });
                uid
            };
            let type_ref = parse_type_ref_str(ptype, types, type_names);
            (param_uid, type_ref)
        })
        .collect()
}

/// Ensure a sym entry exists for a function.
fn ensure_func_sym(source_uid: &str, name: &str, syms_internal: &mut Vec<SymEntry>) {
    if !syms_internal.iter().any(|e| e.uid == source_uid) {
        syms_internal.push(SymEntry {
            uid: source_uid.to_string(),
            display_name: name.to_string(),
        });
    }
}

bindings::export!(Component with_types_in bindings);

#[cfg(test)]
mod tests {
    use super::*;
    use bindings::exports::wast::core::syntax_plugin::Guest;

    fn make_test_component() -> WastComponent {
        WastComponent {
            funcs: vec![
                (
                    "f1".to_string(),
                    WastFunc {
                        source: FuncSource::Internal("f1".to_string()),
                        params: vec![("p1".to_string(), "t1".to_string())],
                        result: Some("t1".to_string()),
                        body: Some(vec![1, 2, 3]),
                    },
                ),
                (
                    "f2".to_string(),
                    WastFunc {
                        source: FuncSource::Imported("f2".to_string()),
                        params: vec![("p2".to_string(), "t1".to_string())],
                        result: None,
                        body: None,
                    },
                ),
                (
                    "f3".to_string(),
                    WastFunc {
                        source: FuncSource::Exported("f3".to_string()),
                        params: vec![],
                        result: Some("t1".to_string()),
                        body: Some(vec![10, 20]),
                    },
                ),
            ],
            types: vec![(
                "t1".to_string(),
                WastTypeDef {
                    source: TypeSource::Internal("t1".to_string()),
                    definition: WitType::Primitive(PrimitiveType::U32),
                },
            )],
            syms: Syms {
                wit_syms: vec![("f2".to_string(), "imported_fn".to_string())],
                internal: vec![
                    SymEntry {
                        uid: "f1".to_string(),
                        display_name: "my_func".to_string(),
                    },
                    SymEntry {
                        uid: "f3".to_string(),
                        display_name: "exported_fn".to_string(),
                    },
                    SymEntry {
                        uid: "t1".to_string(),
                        display_name: "u32".to_string(),
                    },
                ],
                local: vec![
                    SymEntry {
                        uid: "p1".to_string(),
                        display_name: "param_one".to_string(),
                    },
                    SymEntry {
                        uid: "p2".to_string(),
                        display_name: "param_two".to_string(),
                    },
                ],
            },
        }
    }

    #[test]
    fn test_to_text_contains_display_names() {
        let comp = make_test_component();
        let text = Component::to_text(comp);
        assert!(text.contains("my_func"), "should contain func name");
        assert!(text.contains("param_one"), "should contain param name");
        assert!(text.contains("imported_fn"), "should contain import name");
        assert!(text.contains("exported_fn"), "should contain export name");
        assert!(
            text.contains("declare function"),
            "should have declare keyword"
        );
        assert!(
            text.contains("export function"),
            "should have export keyword"
        );
        assert!(text.contains("function "), "should have function keyword");
    }

    #[test]
    fn test_to_text_internal_func_format() {
        let comp = make_test_component();
        let text = Component::to_text(comp);
        assert!(
            text.contains("function my_func(param_one: u32): u32"),
            "internal func signature: {}",
            text
        );
    }

    #[test]
    fn test_to_text_import_format() {
        let comp = make_test_component();
        let text = Component::to_text(comp);
        assert!(
            text.contains("declare function imported_fn(param_two: u32)"),
            "import signature: {}",
            text
        );
    }

    #[test]
    fn test_to_text_export_format() {
        let comp = make_test_component();
        let text = Component::to_text(comp);
        assert!(
            text.contains("export function exported_fn(): u32"),
            "export signature: {}",
            text
        );
    }

    #[test]
    fn test_roundtrip_to_text_from_text_to_text() {
        let comp = make_test_component();
        let text1 = Component::to_text(comp.clone());

        let parsed = Component::from_text(text1.clone(), comp.clone());
        assert!(parsed.is_ok(), "from_text failed: {:?}", parsed.err());
        let parsed = parsed.unwrap();

        assert_eq!(parsed.funcs.len(), comp.funcs.len(), "func count mismatch");

        let text2 = Component::to_text(parsed);
        assert_eq!(text1, text2, "roundtrip text mismatch");
    }

    #[test]
    fn test_from_text_preserves_body() {
        let comp = make_test_component();
        let text = Component::to_text(comp.clone());
        let parsed = Component::from_text(text, comp).unwrap();

        let f1 = parsed.funcs.iter().find(|(uid, _)| uid == "f1");
        assert!(f1.is_some(), "f1 should exist");
        assert_eq!(
            f1.unwrap().1.body,
            Some(vec![1, 2, 3]),
            "body should be preserved"
        );
    }

    #[test]
    fn test_from_text_preserves_func_source_kinds() {
        let comp = make_test_component();
        let text = Component::to_text(comp.clone());
        let parsed = Component::from_text(text, comp).unwrap();

        let has_internal = parsed
            .funcs
            .iter()
            .any(|(_, f)| matches!(f.source, FuncSource::Internal(_)));
        let has_imported = parsed
            .funcs
            .iter()
            .any(|(_, f)| matches!(f.source, FuncSource::Imported(_)));
        let has_exported = parsed
            .funcs
            .iter()
            .any(|(_, f)| matches!(f.source, FuncSource::Exported(_)));

        assert!(has_internal, "should have internal func");
        assert!(has_imported, "should have imported func");
        assert!(has_exported, "should have exported func");
    }

    #[test]
    fn test_from_text_error_on_invalid_input() {
        let comp = make_test_component();
        let result = Component::from_text("this is not valid syntax".to_string(), comp);
        assert!(result.is_err(), "should return error for invalid input");
    }

    #[test]
    fn test_empty_component_roundtrip() {
        let comp = WastComponent {
            funcs: vec![],
            types: vec![],
            syms: Syms {
                wit_syms: vec![],
                internal: vec![],
                local: vec![],
            },
        };
        let text = Component::to_text(comp.clone());
        assert_eq!(text, "", "empty component should produce empty text");

        let parsed = Component::from_text(text, comp);
        assert!(parsed.is_ok());
        assert_eq!(parsed.unwrap().funcs.len(), 0);
    }

    // -----------------------------------------------------------------------
    // Body roundtrip tests
    // -----------------------------------------------------------------------

    /// Helper: build a component with a single internal function containing the
    /// given body instructions.
    fn make_body_component(instructions: Vec<Instruction>) -> WastComponent {
        let body = wast_pattern_analyzer::serialize_body(&instructions);
        WastComponent {
            funcs: vec![(
                "f1".to_string(),
                WastFunc {
                    source: FuncSource::Internal("f1".to_string()),
                    params: vec![("p1".to_string(), "t1".to_string())],
                    result: Some("t1".to_string()),
                    body: Some(body),
                },
            )],
            types: vec![(
                "t1".to_string(),
                WastTypeDef {
                    source: TypeSource::Internal("t1".to_string()),
                    definition: WitType::Primitive(PrimitiveType::U32),
                },
            )],
            syms: Syms {
                wit_syms: vec![],
                internal: vec![
                    SymEntry {
                        uid: "f1".to_string(),
                        display_name: "my_func".to_string(),
                    },
                    SymEntry {
                        uid: "t1".to_string(),
                        display_name: "u32".to_string(),
                    },
                ],
                local: vec![
                    SymEntry {
                        uid: "p1".to_string(),
                        display_name: "x".to_string(),
                    },
                    SymEntry {
                        uid: "v1".to_string(),
                        display_name: "y".to_string(),
                    },
                    SymEntry {
                        uid: "v2".to_string(),
                        display_name: "val".to_string(),
                    },
                    SymEntry {
                        uid: "v3".to_string(),
                        display_name: "res".to_string(),
                    },
                    SymEntry {
                        uid: "v4".to_string(),
                        display_name: "opt".to_string(),
                    },
                ],
            },
        }
    }

    /// Assert that to_text → from_text → to_text produces identical text.
    fn assert_body_roundtrip(instructions: Vec<Instruction>) {
        let comp = make_body_component(instructions);
        let text1 = Component::to_text(comp.clone());
        let parsed = Component::from_text(text1.clone(), comp);
        assert!(parsed.is_ok(), "from_text failed: {:?}", parsed.err());
        let text2 = Component::to_text(parsed.unwrap());
        assert_eq!(
            text1, text2,
            "body roundtrip text mismatch:\n--- expected ---\n{}\n--- actual ---\n{}",
            text1, text2
        );
    }

    #[test]
    fn test_body_roundtrip_simple_instructions() {
        assert_body_roundtrip(vec![
            Instruction::LocalSet {
                uid: "v1".to_string(),
                value: Box::new(Instruction::Const { value: 42 }),
            },
            Instruction::Return,
        ]);
    }

    #[test]
    fn test_body_roundtrip_call() {
        assert_body_roundtrip(vec![Instruction::Call {
            func_uid: "f1".to_string(),
            args: vec![("".to_string(), Instruction::Const { value: 10 })],
        }]);
    }

    #[test]
    fn test_body_roundtrip_arithmetic() {
        assert_body_roundtrip(vec![Instruction::LocalSet {
            uid: "v1".to_string(),
            value: Box::new(Instruction::Arithmetic {
                op: ArithOp::Add,
                lhs: Box::new(Instruction::LocalGet {
                    uid: "p1".to_string(),
                }),
                rhs: Box::new(Instruction::Const { value: 1 }),
            }),
        }]);
    }

    #[test]
    fn test_body_roundtrip_compare() {
        assert_body_roundtrip(vec![Instruction::LocalSet {
            uid: "v1".to_string(),
            value: Box::new(Instruction::Compare {
                op: CompareOp::Eq,
                lhs: Box::new(Instruction::LocalGet {
                    uid: "p1".to_string(),
                }),
                rhs: Box::new(Instruction::Const { value: 0 }),
            }),
        }]);
    }

    #[test]
    fn test_body_roundtrip_if_else() {
        assert_body_roundtrip(vec![Instruction::If {
            condition: Box::new(Instruction::Compare {
                op: CompareOp::Lt,
                lhs: Box::new(Instruction::LocalGet {
                    uid: "p1".to_string(),
                }),
                rhs: Box::new(Instruction::Const { value: 10 }),
            }),
            then_body: vec![Instruction::Return],
            else_body: vec![Instruction::LocalSet {
                uid: "v1".to_string(),
                value: Box::new(Instruction::Const { value: 99 }),
            }],
        }]);
    }

    #[test]
    fn test_body_roundtrip_loop() {
        assert_body_roundtrip(vec![Instruction::Loop {
            label: Some("loop0".to_string()),
            body: vec![
                Instruction::BrIf {
                    label: "loop0".to_string(),
                    condition: Box::new(Instruction::Compare {
                        op: CompareOp::Lt,
                        lhs: Box::new(Instruction::LocalGet {
                            uid: "p1".to_string(),
                        }),
                        rhs: Box::new(Instruction::Const { value: 5 }),
                    }),
                },
                Instruction::LocalSet {
                    uid: "p1".to_string(),
                    value: Box::new(Instruction::Arithmetic {
                        op: ArithOp::Add,
                        lhs: Box::new(Instruction::LocalGet {
                            uid: "p1".to_string(),
                        }),
                        rhs: Box::new(Instruction::Const { value: 1 }),
                    }),
                },
            ],
        }]);
    }

    #[test]
    fn test_body_roundtrip_wit_types() {
        assert_body_roundtrip(vec![
            Instruction::LocalSet {
                uid: "v2".to_string(),
                value: Box::new(Instruction::Some {
                    value: Box::new(Instruction::Const { value: 1 }),
                }),
            },
            Instruction::LocalSet {
                uid: "v3".to_string(),
                value: Box::new(Instruction::Ok {
                    value: Box::new(Instruction::LocalGet {
                        uid: "v2".to_string(),
                    }),
                }),
            },
        ]);
    }

    #[test]
    fn test_body_roundtrip_match_option() {
        assert_body_roundtrip(vec![Instruction::MatchOption {
            value: Box::new(Instruction::LocalGet {
                uid: "v4".to_string(),
            }),
            some_binding: "v2".to_string(),
            some_body: vec![Instruction::Return],
            none_body: vec![Instruction::LocalSet {
                uid: "v1".to_string(),
                value: Box::new(Instruction::Const { value: 0 }),
            }],
        }]);
    }

    #[test]
    fn test_body_roundtrip_match_result() {
        assert_body_roundtrip(vec![Instruction::MatchResult {
            value: Box::new(Instruction::LocalGet {
                uid: "v3".to_string(),
            }),
            ok_binding: "v2".to_string(),
            ok_body: vec![Instruction::Return],
            err_binding: "v1".to_string(),
            err_body: vec![Instruction::LocalSet {
                uid: "v1".to_string(),
                value: Box::new(Instruction::Const { value: -1 }),
            }],
        }]);
    }

    #[test]
    fn test_body_roundtrip_nested_if_in_loop() {
        assert_body_roundtrip(vec![Instruction::Loop {
            label: Some("outer".to_string()),
            body: vec![
                Instruction::BrIf {
                    label: "outer".to_string(),
                    condition: Box::new(Instruction::Compare {
                        op: CompareOp::Lt,
                        lhs: Box::new(Instruction::LocalGet {
                            uid: "p1".to_string(),
                        }),
                        rhs: Box::new(Instruction::Const { value: 100 }),
                    }),
                },
                Instruction::If {
                    condition: Box::new(Instruction::IsErr {
                        value: Box::new(Instruction::LocalGet {
                            uid: "v3".to_string(),
                        }),
                    }),
                    then_body: vec![Instruction::Return],
                    else_body: vec![],
                },
                Instruction::LocalSet {
                    uid: "p1".to_string(),
                    value: Box::new(Instruction::Arithmetic {
                        op: ArithOp::Add,
                        lhs: Box::new(Instruction::LocalGet {
                            uid: "p1".to_string(),
                        }),
                        rhs: Box::new(Instruction::Const { value: 1 }),
                    }),
                },
            ],
        }]);
    }

    #[test]
    fn test_body_roundtrip_block() {
        assert_body_roundtrip(vec![Instruction::Block {
            label: Some("blk".to_string()),
            body: vec![
                Instruction::LocalSet {
                    uid: "v1".to_string(),
                    value: Box::new(Instruction::Const { value: 1 }),
                },
                Instruction::Br {
                    label: "blk".to_string(),
                },
            ],
        }]);
    }

    #[test]
    fn test_body_roundtrip_err_and_is_err() {
        assert_body_roundtrip(vec![
            Instruction::LocalSet {
                uid: "v3".to_string(),
                value: Box::new(Instruction::Err {
                    value: Box::new(Instruction::Const { value: 404 }),
                }),
            },
            Instruction::If {
                condition: Box::new(Instruction::IsErr {
                    value: Box::new(Instruction::LocalGet {
                        uid: "v3".to_string(),
                    }),
                }),
                then_body: vec![Instruction::Return],
                else_body: vec![],
            },
        ]);
    }
}
