#[allow(warnings)]
#[rustfmt::skip]
mod bindings;

use bindings::wast::core::types::*;
use std::collections::HashMap;
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
    // Use internal syms for type names too
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
    // First check if there's a direct type definition we can inline
    for (uid, typedef) in types {
        if uid == type_ref {
            return format_wit_type(&typedef.definition, types, type_names);
        }
    }
    // Fall back to name map or raw UID
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
        WitType::Enum(cases) => format!("enum {{ {} }}", cases.join(", ")),
        WitType::Flags(names) => format!("flags {{ {} }}", names.join(", ")),
        WitType::Resource => "resource".to_string(),
        WitType::Own(r) => format!("own<{}>", r),
        WitType::Borrow(r) => format!("borrow<{}>", r),
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
        Err(_) => format!("{}# [body: {} bytes]", indent, body.len()),
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

fn resolve_func_name(uid: &str, func_names: &HashMap<String, String>) -> String {
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
        Instruction::Return => format!("{}return", indent),
        Instruction::Const { value } => format!("{}{}", indent, value),
        Instruction::LocalGet { uid } => {
            format!("{}{}", indent, resolve_local_name(uid, local_names))
        }
        Instruction::LocalSet { uid, value } => {
            let name = resolve_local_name(uid, local_names);
            let val = render_expr(value, local_names, func_names);
            format!("{}{} = {}", indent, name, val)
        }
        Instruction::Call { func_uid, args } => {
            let name = resolve_func_name(func_uid, func_names);
            let args_str = args
                .iter()
                .map(|(param_name, arg)| {
                    let pname = resolve_local_name(param_name, local_names);
                    let val = render_expr(arg, local_names, func_names);
                    format!("{}: {}", pname, val)
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}{}({})", indent, name, args_str)
        }
        Instruction::Compare { op, lhs, rhs } => {
            let l = render_expr(lhs, local_names, func_names);
            let r = render_expr(rhs, local_names, func_names);
            let op_str = match op {
                CompareOp::Eq => "==",
                CompareOp::Ne => "!=",
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
                format!("{}if {}\n{}\n{}end", indent, cond, then_str, indent)
            } else {
                let else_str = render_instructions(else_body, &inner, local_names, func_names);
                format!(
                    "{}if {}\n{}\n{}else\n{}\n{}end",
                    indent, cond, then_str, indent, else_str, indent
                )
            }
        }
        Instruction::Loop { label, body } => {
            let body_str = render_instructions(body, &inner, local_names, func_names);
            let label_comment = match label {
                Some(l) => format!(" # {}", l),
                None => String::new(),
            };
            format!(
                "{}loop do{}\n{}\n{}end",
                indent, label_comment, body_str, indent
            )
        }
        Instruction::Block { label, body } => {
            let body_str = render_instructions(body, &inner, local_names, func_names);
            let label_comment = match label {
                Some(l) => format!(" # {}", l),
                None => String::new(),
            };
            format!(
                "{}begin{}\n{}\n{}end",
                indent, label_comment, body_str, indent
            )
        }
        Instruction::BrIf { label, condition } => {
            let cond = render_expr(condition, local_names, func_names);
            format!("{}break {} if {}", indent, label, cond)
        }
        Instruction::Br { label } => format!("{}break {}", indent, label),
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
            format!("{}is_err({})", indent, val)
        }
        Instruction::StringLen { value } => {
            let val = render_expr(value, local_names, func_names);
            format!("{}{}.bytesize", indent, val)
        }
        Instruction::StringLiteral { bytes } => {
            let s = String::from_utf8_lossy(bytes);
            format!("{indent}{:?}", &*s)
        }
        Instruction::ListLen { value } => {
            let val = render_expr(value, local_names, func_names);
            format!("{}{}.size", indent, val)
        }
        Instruction::RecordGet { value, field } => {
            let val = render_expr(value, local_names, func_names);
            format!("{}{}.{}", indent, val, field)
        }
        Instruction::RecordLiteral { fields } => {
            let pairs = fields
                .iter()
                .map(|(fname, fval)| {
                    format!("{}: {}", fname, render_expr(fval, local_names, func_names))
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{indent}{{ {pairs} }}")
        }
        Instruction::VariantCtor { case, value } => match value {
            Some(v) => {
                let val = render_expr(v, local_names, func_names);
                format!("{indent}:{case}({val})")
            }
            None => format!("{indent}:{case}"),
        },
        Instruction::MatchVariant { value, arms } => {
            let val = render_expr(value, local_names, func_names);
            let arm_lines = arms
                .iter()
                .map(|arm| {
                    let pattern = match &arm.binding {
                        Some(b) => format!("in :{}({})", arm.case, b),
                        None => format!("in :{}", arm.case),
                    };
                    let body_str = render_instructions(&arm.body, &inner, local_names, func_names);
                    format!("{inner}{pattern}\n{body_str}")
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!("{indent}case {val}\n{arm_lines}\n{indent}end")
        }
        Instruction::TupleGet { value, index } => {
            let val = render_expr(value, local_names, func_names);
            format!("{indent}{val}[{index}]")
        }
        Instruction::TupleLiteral { values } => {
            let parts = values
                .iter()
                .map(|v| render_expr(v, local_names, func_names))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{indent}[{parts}]")
        }
        Instruction::ListLiteral { values } => {
            let parts = values
                .iter()
                .map(|v| render_expr(v, local_names, func_names))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{indent}List[{parts}]")
        }
        Instruction::FlagsCtor { flags } => {
            let parts: Vec<String> = flags.iter().map(|f| format!(":{f}")).collect();
            format!("{indent}[{}]", parts.join(", "))
        }
        Instruction::ResourceNew { resource, rep } => {
            let r = render_expr(rep, local_names, func_names);
            format!("{indent}{resource}.new({r})")
        }
        Instruction::ResourceRep {
            resource: _,
            handle,
        } => render_expr(handle, local_names, func_names),
        Instruction::ResourceDrop { resource, handle } => {
            let h = render_expr(handle, local_names, func_names);
            format!("{indent}{h}.drop")
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
                "{}case {}\n{}when some({})\n{}\n{}when none\n{}\n{}end",
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
                "{}case {}\n{}when ok({})\n{}\n{}when err({})\n{}\n{}end",
                indent, val, indent, ok_bind, ok_str, indent, err_bind, err_str, indent
            )
        }
    }
}

/// Render an instruction as an inline expression (no leading indent).
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
            let name = resolve_func_name(func_uid, func_names);
            let args_str = args
                .iter()
                .map(|(param_name, arg)| {
                    let pname = resolve_local_name(param_name, local_names);
                    let val = render_expr(arg, local_names, func_names);
                    format!("{}: {}", pname, val)
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({})", name, args_str)
        }
        Instruction::Compare { op, lhs, rhs } => {
            let l = render_expr(lhs, local_names, func_names);
            let r = render_expr(rhs, local_names, func_names);
            let op_str = match op {
                CompareOp::Eq => "==",
                CompareOp::Ne => "!=",
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
            format!("is_err({})", val)
        }
        // Complex expressions that shouldn't appear inline normally,
        // but we handle them for completeness.
        _ => format!("(...)"),
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

    // rbs-inline annotation: one `#: (t1, t2) -> ret` comment line above the
    // `def`. Ruby's `def name(p1, p2)` carries no types on its own, so the
    // annotation is the only place the signature types appear.
    let param_types: Vec<String> = func
        .params
        .iter()
        .map(|(_, t)| resolve_type_ref(t, types, type_names))
        .collect();
    let param_names: Vec<String> = func
        .params
        .iter()
        .map(|(uid, _)| {
            local_names
                .get(uid.as_str())
                .cloned()
                .unwrap_or_else(|| uid.clone())
        })
        .collect();
    let result_str = match &func.result {
        Some(t) => format!(" -> {}", resolve_type_ref(t, types, type_names)),
        None => " -> void".to_string(),
    };
    let annotation = format!("#: ({}){}", param_types.join(", "), result_str);

    let def_line = format!("def {}({})", name, param_names.join(", "));

    match &func.source {
        FuncSource::Imported(_) => {
            // Imports have no body in wast; render as an annotated stub so a
            // later `from_text` can still recover the full signature.
            format!("{}\n# import\n{}; end", annotation, def_line)
        }
        FuncSource::Exported(_) => {
            let body_str = match &func.body {
                Some(b) => render_body(b, "  ", local_names, func_names),
                None => "  # [no body]".to_string(),
            };
            format!("{}\n# export\n{}\n{}\nend", annotation, def_line, body_str)
        }
        FuncSource::Internal(_) => {
            let body_str = match &func.body {
                Some(b) => render_body(b, "  ", local_names, func_names),
                None => "  # [no body]".to_string(),
            };
            format!("{}\n{}\n{}\nend", annotation, def_line, body_str)
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
        return s.to_string();
    }
    for (uid, name) in type_names {
        if name == s {
            return uid.clone();
        }
    }
    // Fall back to matching the rendered form of each existing type so a
    // round-trip on `option<u32>` lands back at the original `opt_u32`
    // uid instead of inventing a brand-new type ref.
    for (uid, td) in types {
        if format_wit_type(&td.definition, types, type_names) == s {
            return uid.clone();
        }
    }
    s.to_string()
}

/// Split `s` on `delimiter` at top-level (not inside any of `()`, `[]`,
/// `{}`, `<>` brackets). Required for parsing rendered compound types
/// like `record { x: u32, y: u32 }` or `tuple<u32, u32>`.
fn split_top_level(s: &str, delimiter: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' | '>' => depth -= 1,
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

/// Parse a `def` header — `name(p1, p2)` — with no type annotations. Types
/// come from the preceding rbs-inline `#:` line (see `parse_rbs_annotation`).
fn parse_def_header(sig: &str) -> Option<(String, Vec<String>)> {
    let sig = sig.trim();
    let paren_open = sig.find('(')?;
    let name = sig[..paren_open].trim().to_string();
    if name.is_empty() {
        return None;
    }
    let rest = &sig[paren_open + 1..];
    let paren_close = rest.rfind(')')?;
    let params_str = rest[..paren_close].trim();
    let pnames: Vec<String> = if params_str.is_empty() {
        vec![]
    } else {
        split_top_level(params_str, ',')
            .into_iter()
            .map(|p| p.trim().to_string())
            .collect()
    };
    Some((name, pnames))
}

/// Parse a single rbs-inline annotation line like `#: (u32, u32) -> u32` or
/// `#: () -> void`. Returns the param types and the result type (None when
/// the return is `void` or omitted).
fn parse_rbs_annotation(line: &str) -> Option<(Vec<String>, Option<String>)> {
    let rest = line.trim().strip_prefix("#:")?.trim_start();
    let paren_open = rest.find('(')?;
    let after_name = &rest[paren_open..]; // skip anything before '(' (should be nothing)
    let inner_start = after_name.find('(')? + 1;
    let inner_end = after_name.rfind(')')?;
    if inner_end <= inner_start {
        // empty params
    }
    let inner = &after_name[inner_start..inner_end];
    let types: Vec<String> = if inner.trim().is_empty() {
        vec![]
    } else {
        split_top_level(inner, ',')
            .into_iter()
            .map(|s| s.trim().to_string())
            .collect()
    };
    let tail = after_name[inner_end + 1..].trim();
    let result = if let Some(r) = tail.strip_prefix("->") {
        let r = r.trim();
        if r.is_empty() || r == "void" {
            None
        } else {
            Some(r.to_string())
        }
    } else {
        None
    };
    Some((types, result))
}

/// Combine an rbs-inline annotation's types with a `def` header's param
/// names. Missing entries fall back to "unknown" so `parse_type_ref_str`
/// still produces something.
fn combine_def_and_annotation(
    name: String,
    pnames: Vec<String>,
    annotation: Option<(Vec<String>, Option<String>)>,
) -> ParsedFunc {
    let (ptypes, result_type) = annotation.unwrap_or_else(|| (vec![], None));
    let params: Vec<(String, String)> = pnames
        .into_iter()
        .enumerate()
        .map(|(i, pname)| {
            let ty = ptypes.get(i).cloned().unwrap_or_else(|| "unknown".into());
            (pname, ty)
        })
        .collect();
    ParsedFunc {
        name,
        params,
        result_type,
        _is_import: false,
        _is_export: false,
    }
}

fn generate_uid() -> String {
    // Simple deterministic-ish UID from a counter mixed with some bits.
    // In a real implementation this would use randomness; for wasm32 we use
    // a simple static counter approach.
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

        // State carried forward across lines:
        //   - `pending_annotation`: the most recent `#: (...) -> ...` line
        //     since the last `def`; consumed by the next `def`.
        //   - `next_is_import` / `next_is_export`: whether the current
        //     annotation/def pair is tagged with `# import` / `# export`.
        let mut pending_annotation: Option<(Vec<String>, Option<String>)> = None;
        let mut next_is_import = false;
        let mut next_is_export = false;

        while i < lines.len() {
            let line = lines[i].trim();

            if line.is_empty() {
                i += 1;
                continue;
            }

            // rbs-inline annotation line — remember for the next def.
            if line.starts_with("#:") {
                match parse_rbs_annotation(line) {
                    Some(a) => pending_annotation = Some(a),
                    None => errors.push(WastError {
                        message: format!("parse_error: cannot parse rbs annotation: {}", line),
                        location: Some(format!("line {}", i + 1)),
                    }),
                }
                i += 1;
                continue;
            }

            if line == "# import" {
                next_is_import = true;
                i += 1;
                continue;
            }
            if line == "# export" {
                next_is_export = true;
                i += 1;
                continue;
            }

            // `def name(...)` line. For imported stubs we emitted
            // `def name(...); end` on a single line.
            if line.starts_with("def ") {
                let (header, after_end) = if let Some(idx) = line.find("; end") {
                    (&line["def ".len()..idx], true)
                } else {
                    (&line["def ".len()..], false)
                };

                match parse_def_header(header) {
                    Some((name, pnames)) => {
                        let parsed =
                            combine_def_and_annotation(name, pnames, pending_annotation.take());

                        // Walk the body to the matching `end` unless it's a
                        // one-liner stub. Body content can have nested
                        // `end`-terminated constructs (if/loop/begin/case),
                        // so count nesting depth.
                        if !after_end {
                            i += 1;
                            let mut depth: i32 = 0;
                            while i < lines.len() {
                                let bline = lines[i].trim();
                                if bline.is_empty() {
                                    i += 1;
                                    continue;
                                }
                                // Strip an inline `# …` comment so the
                                // opener heuristic ignores label suffixes
                                // (`loop do # label0` etc.).
                                let bare = bline
                                    .split_once(" #")
                                    .map(|(b, _)| b.trim())
                                    .unwrap_or(bline);
                                let opens = bare.starts_with("if ")
                                    || bare == "if"
                                    || bare.starts_with("unless ")
                                    || bare.starts_with("while ")
                                    || bare.starts_with("until ")
                                    || bare.starts_with("for ")
                                    || bare.starts_with("case ")
                                    || bare == "case"
                                    || bare == "begin"
                                    || bare.starts_with("begin ")
                                    || bare == "loop do"
                                    || bare.starts_with("loop do ")
                                    || bare.ends_with(" do");
                                if opens {
                                    depth += 1;
                                }
                                if bare == "end" || bare.starts_with("end ") {
                                    if depth == 0 {
                                        i += 1;
                                        break; // outer def end
                                    }
                                    depth -= 1;
                                }
                                i += 1;
                            }
                        } else {
                            i += 1;
                        }

                        let (func_uid, source_uid) = resolve_func_uid(
                            &parsed.name,
                            &rev_func,
                            &existing_by_source,
                            &existing_funcs,
                            next_is_import,
                        );

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

                        let source = if next_is_import {
                            FuncSource::Imported(source_uid.clone())
                        } else if next_is_export {
                            FuncSource::Exported(source_uid.clone())
                        } else {
                            FuncSource::Internal(source_uid.clone())
                        };
                        if matches!(source, FuncSource::Exported(_) | FuncSource::Internal(_)) {
                            ensure_func_sym(&source_uid, &parsed.name, &mut new_syms_internal);
                        }

                        funcs.push((
                            func_uid,
                            WastFunc {
                                source,
                                params,
                                result,
                                body,
                            },
                        ));

                        next_is_import = false;
                        next_is_export = false;
                    }
                    None => {
                        errors.push(WastError {
                            message: format!("parse_error: cannot parse def header: {}", header),
                            location: Some(format!("line {}", i + 1)),
                        });
                        i += 1;
                    }
                }
                continue;
            }

            // Skip other comment lines.
            if line.starts_with('#') {
                i += 1;
                continue;
            }

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
/// For known names, use existing UIDs; for new names, generate UIDs.
fn resolve_func_uid(
    name: &str,
    rev_func: &HashMap<String, String>,
    existing_by_source: &HashMap<String, (&str, &WastFunc)>,
    existing_funcs: &HashMap<String, &WastFunc>,
    _is_import: bool,
) -> (String, String) {
    if let Some(source_uid) = rev_func.get(name) {
        if let Some((func_uid, _)) = existing_by_source.get(source_uid.as_str()) {
            return (func_uid.to_string(), source_uid.clone());
        }
        return (source_uid.clone(), source_uid.clone());
    }
    if let Some((func_uid, _)) = existing_by_source.get(name) {
        return (func_uid.to_string(), name.to_string());
    }
    if let Some(f) = existing_funcs.get(name) {
        let source_val = match &f.source {
            FuncSource::Internal(s) | FuncSource::Imported(s) | FuncSource::Exported(s) => {
                s.clone()
            }
        };
        return (name.to_string(), source_val);
    }
    let uid = generate_uid();
    (uid.clone(), uid)
}

/// Resolve parameter names and types from parsed strings.
fn resolve_params(
    parsed: &[(String, String)],
    rev_local: &HashMap<String, String>,
    types: &[(TypeUid, WastTypeDef)],
    type_names: &HashMap<String, String>,
    _new_syms_local: &mut Vec<SymEntry>,
) -> Vec<(FuncUid, WitTypeRef)> {
    parsed
        .iter()
        .map(|(pname, ptype)| {
            // Reuse the explicit syms.local UID when one exists; otherwise
            // the displayed name IS the UID (matches what to_text emits in
            // the absence of a sym override). Generating fresh UIDs here
            // would sever body LocalGet refs.
            let param_uid = rev_local
                .get(pname.as_str())
                .cloned()
                .unwrap_or_else(|| pname.clone());
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
        assert!(text.contains("# import"), "should have import marker");
        assert!(text.contains("# export"), "should have export marker");
        assert!(text.contains("def "), "should have def keyword");
        assert!(text.contains("end"), "should have end keyword");
    }

    #[test]
    fn test_to_text_internal_func_format() {
        let comp = make_test_component();
        let text = Component::to_text(comp);
        assert!(
            text.contains("#: (u32) -> u32\ndef my_func(param_one)"),
            "internal func signature: {}",
            text
        );
    }

    #[test]
    fn test_to_text_import_format() {
        let comp = make_test_component();
        let text = Component::to_text(comp);
        // Imports: `#: (u32) -> void\n# import\ndef imported_fn(param_two); end`.
        // The result type is `void` because the test-fixture import has no
        // return; real imports with a result get `-> T` instead.
        assert!(
            text.contains("#: (u32) -> void\n# import\ndef imported_fn(param_two); end"),
            "import signature: {}",
            text
        );
    }

    #[test]
    fn test_to_text_export_format() {
        let comp = make_test_component();
        let text = Component::to_text(comp);
        assert!(
            text.contains("#: () -> u32\n# export\ndef exported_fn()"),
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

        // Internal func f1 should preserve body
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
    //
    // ruby-like's `from_text` doesn't currently parse body content — it
    // skips lines between the `def` header and the trailing `end`, then
    // restores the body bytes from the `existing` component. These tests
    // lock in that contract: to_text → from_text (with same component
    // passed as `existing`) → to_text produces identical text. A future
    // milestone would replace the skip-and-restore behaviour with a real
    // body parser; until then, these tests guarantee at least
    // round-trippable preservation.
    // -----------------------------------------------------------------------

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
                        display_name: "v".to_string(),
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
                uid: "v1".into(),
                value: Box::new(Instruction::Const { value: 42 }),
            },
            Instruction::Return,
        ]);
    }

    #[test]
    fn test_body_roundtrip_call() {
        assert_body_roundtrip(vec![Instruction::Call {
            func_uid: "f1".into(),
            args: vec![("p1".into(), Instruction::Const { value: 10 })],
        }]);
    }

    #[test]
    fn test_body_roundtrip_arithmetic() {
        assert_body_roundtrip(vec![Instruction::LocalSet {
            uid: "v1".into(),
            value: Box::new(Instruction::Arithmetic {
                op: ArithOp::Add,
                lhs: Box::new(Instruction::LocalGet { uid: "p1".into() }),
                rhs: Box::new(Instruction::Const { value: 1 }),
            }),
        }]);
    }

    #[test]
    fn test_body_roundtrip_compare() {
        assert_body_roundtrip(vec![Instruction::LocalSet {
            uid: "v1".into(),
            value: Box::new(Instruction::Compare {
                op: CompareOp::Lt,
                lhs: Box::new(Instruction::LocalGet { uid: "p1".into() }),
                rhs: Box::new(Instruction::Const { value: 100 }),
            }),
        }]);
    }

    #[test]
    fn test_body_roundtrip_if_else() {
        assert_body_roundtrip(vec![Instruction::If {
            condition: Box::new(Instruction::Compare {
                op: CompareOp::Eq,
                lhs: Box::new(Instruction::LocalGet { uid: "p1".into() }),
                rhs: Box::new(Instruction::Const { value: 0 }),
            }),
            then_body: vec![Instruction::Return],
            else_body: vec![Instruction::Nop],
        }]);
    }

    #[test]
    fn test_body_roundtrip_loop() {
        assert_body_roundtrip(vec![Instruction::Loop {
            label: Some("loop0".into()),
            body: vec![
                Instruction::BrIf {
                    label: "loop0".into(),
                    condition: Box::new(Instruction::Compare {
                        op: CompareOp::Lt,
                        lhs: Box::new(Instruction::LocalGet { uid: "v1".into() }),
                        rhs: Box::new(Instruction::Const { value: 10 }),
                    }),
                },
                Instruction::Br {
                    label: "loop0".into(),
                },
            ],
        }]);
    }

    #[test]
    fn test_body_roundtrip_block() {
        assert_body_roundtrip(vec![Instruction::Block {
            label: Some("done".into()),
            body: vec![Instruction::Nop, Instruction::Return],
        }]);
    }

    #[test]
    fn test_body_roundtrip_wit_types() {
        assert_body_roundtrip(vec![
            Instruction::Some {
                value: Box::new(Instruction::Const { value: 7 }),
            },
            Instruction::None,
            Instruction::Ok {
                value: Box::new(Instruction::Const { value: 1 }),
            },
            Instruction::Err {
                value: Box::new(Instruction::Const { value: 2 }),
            },
            Instruction::IsErr {
                value: Box::new(Instruction::LocalGet { uid: "v3".into() }),
            },
        ]);
    }

    #[test]
    fn test_body_roundtrip_match_option() {
        assert_body_roundtrip(vec![Instruction::MatchOption {
            value: Box::new(Instruction::LocalGet { uid: "v4".into() }),
            some_binding: "v2".into(),
            some_body: vec![Instruction::LocalGet { uid: "v2".into() }],
            none_body: vec![Instruction::Const { value: 0 }],
        }]);
    }

    #[test]
    fn test_body_roundtrip_match_result() {
        assert_body_roundtrip(vec![Instruction::MatchResult {
            value: Box::new(Instruction::LocalGet { uid: "v3".into() }),
            ok_binding: "v2".into(),
            ok_body: vec![Instruction::LocalGet { uid: "v2".into() }],
            err_binding: "v1".into(),
            err_body: vec![Instruction::Const { value: 0 }],
        }]);
    }

    #[test]
    fn test_body_roundtrip_nested_if_in_loop() {
        assert_body_roundtrip(vec![Instruction::Loop {
            label: Some("outer".into()),
            body: vec![
                Instruction::If {
                    condition: Box::new(Instruction::IsErr {
                        value: Box::new(Instruction::LocalGet { uid: "v3".into() }),
                    }),
                    then_body: vec![Instruction::Return],
                    else_body: vec![Instruction::Nop],
                },
                Instruction::LocalSet {
                    uid: "v1".into(),
                    value: Box::new(Instruction::Arithmetic {
                        op: ArithOp::Add,
                        lhs: Box::new(Instruction::LocalGet { uid: "v1".into() }),
                        rhs: Box::new(Instruction::Const { value: 1 }),
                    }),
                },
                Instruction::Br {
                    label: "outer".into(),
                },
            ],
        }]);
    }
}
