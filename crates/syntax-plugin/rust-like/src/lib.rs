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
            format!("Option<{}>", resolve_type_ref(inner, types, type_names))
        }
        WitType::Result((ok, err)) => {
            format!(
                "Result<{}, {}>",
                resolve_type_ref(ok, types, type_names),
                resolve_type_ref(err, types, type_names)
            )
        }
        WitType::List(inner) => {
            format!("Vec<{}>", resolve_type_ref(inner, types, type_names))
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
            format!("struct {{ {} }}", parts.join(", "))
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
            format!("enum {{ {} }}", parts.join(", "))
        }
        WitType::Tuple(refs) => {
            let parts: Vec<String> = refs
                .iter()
                .map(|r| resolve_type_ref(r, types, type_names))
                .collect();
            format!("({})", parts.join(", "))
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
        PrimitiveType::String => "String",
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
        "String" => Some(PrimitiveType::String),
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
                format!("{}if {} {{\n{}\n{}}}", indent, cond, then_str, indent)
            } else {
                let else_str = render_instructions(else_body, &inner, local_names, func_names);
                format!(
                    "{}if {} {{\n{}\n{}}} else {{\n{}\n{}}}",
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
                "{}loop {{{}\n{}\n{}}}",
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
            format!("{}if {} {{ break {}; }}", indent, cond, label)
        }
        Instruction::Br { label } => format!("{}break {};", indent, label),
        Instruction::Some { value } => {
            let val = render_expr(value, local_names, func_names);
            format!("{}Some({})", indent, val)
        }
        Instruction::None => format!("{}None", indent),
        Instruction::Ok { value } => {
            let val = render_expr(value, local_names, func_names);
            format!("{}Ok({})", indent, val)
        }
        Instruction::Err { value } => {
            let val = render_expr(value, local_names, func_names);
            format!("{}Err({})", indent, val)
        }
        Instruction::IsErr { value } => {
            let val = render_expr(value, local_names, func_names);
            format!("{}{}.is_err()", indent, val)
        }
        Instruction::StringLen { value } => {
            let val = render_expr(value, local_names, func_names);
            format!("{}{}.len()", indent, val)
        }
        Instruction::StringLiteral { bytes } => {
            let s = String::from_utf8_lossy(bytes);
            format!("{indent}{:?}", &*s)
        }
        Instruction::ListLen { value } => {
            let val = render_expr(value, local_names, func_names);
            format!("{}{}.len()", indent, val)
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
                "{}match {} {{\n{}Some({}) => {{\n{}\n{}}}\n{}None => {{\n{}\n{}}}\n{}}}",
                indent, val, indent, binding, some_str, indent, indent, none_str, indent, indent
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
                "{}match {} {{\n{}Ok({}) => {{\n{}\n{}}}\n{}Err({}) => {{\n{}\n{}}}\n{}}}",
                indent,
                val,
                indent,
                ok_bind,
                ok_str,
                indent,
                indent,
                err_bind,
                err_str,
                indent,
                indent
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
            format!("Some({})", val)
        }
        Instruction::None => "None".to_string(),
        Instruction::Ok { value } => {
            let val = render_expr(value, local_names, func_names);
            format!("Ok({})", val)
        }
        Instruction::Err { value } => {
            let val = render_expr(value, local_names, func_names);
            format!("Err({})", val)
        }
        Instruction::IsErr { value } => {
            let val = render_expr(value, local_names, func_names);
            format!("{}.is_err()", val)
        }
        // Complex expressions that shouldn't appear inline normally,
        // but we handle them for completeness.
        _ => "(...)".to_string(),
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
        Some(type_ref) => format!(" -> {}", resolve_type_ref(type_ref, types, type_names)),
        None => String::new(),
    };

    match &func.source {
        FuncSource::Imported(_) => {
            format!(
                "extern \"wast\" {{\n    fn {}({}){};\n}}",
                name, params_str, result_str
            )
        }
        FuncSource::Exported(_) => {
            let body_str = match &func.body {
                Some(b) => render_body(b, "    ", local_names, func_names),
                None => "    // [no body]".to_string(),
            };
            format!(
                "#[export]\nfn {}({}){} {{\n{}\n}}",
                name, params_str, result_str, body_str
            )
        }
        FuncSource::Internal(_) => {
            let body_str = match &func.body {
                Some(b) => render_body(b, "    ", local_names, func_names),
                None => "    // [no body]".to_string(),
            };
            format!(
                "fn {}({}){} {{\n{}\n}}",
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
    params: Vec<(String, String)>,
    result_type: Option<String>,
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

/// Parse a signature like `name(p1: type1, p2: type2) -> ret`
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

    let result_type = if after_params.starts_with("->") {
        Some(after_params[2..].trim().to_string())
    } else {
        None
    };

    Some(ParsedFunc {
        name,
        params,
        result_type,
    })
}

fn generate_uid() -> String {
    use core::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0xa000);
    let val = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:04x}", val & 0xffff)
}

fn resolve_func_uid(
    name: &str,
    rev_func: &HashMap<String, String>,
    existing_by_source: &HashMap<String, (&str, &WastFunc)>,
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

fn ensure_func_sym(source_uid: &str, name: &str, syms_internal: &mut Vec<SymEntry>) {
    if !syms_internal.iter().any(|e| e.uid == source_uid) {
        syms_internal.push(SymEntry {
            uid: source_uid.to_string(),
            display_name: name.to_string(),
        });
    }
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

        let rev_func: HashMap<String, String> = func_names
            .iter()
            .map(|(k, v)| (v.clone(), k.clone()))
            .collect();
        let rev_local: HashMap<String, String> = local_names
            .iter()
            .map(|(k, v)| (v.clone(), k.clone()))
            .collect();

        let existing_funcs: HashMap<String, &WastFunc> = existing
            .funcs
            .iter()
            .map(|(uid, f)| (uid.clone(), f))
            .collect();

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

            if line.is_empty() {
                i += 1;
                continue;
            }

            // Parse: extern "wast" { fn name(params) -> result; }
            if line.starts_with("extern") && line.contains("\"wast\"") {
                // Could be single-line or multi-line extern block
                // Collect all lines until closing }
                let mut block = String::new();
                if line.contains('}') {
                    // Single line: extern "wast" { fn name(params) -> result; }
                    block = line.to_string();
                    i += 1;
                } else {
                    // Multi-line
                    i += 1;
                    while i < lines.len() {
                        let l = lines[i].trim();
                        if l == "}" {
                            i += 1;
                            break;
                        }
                        if !l.is_empty() {
                            block.push_str(l);
                            block.push('\n');
                        }
                        i += 1;
                    }
                }

                // Extract fn declarations from the block
                let fn_decls: Vec<&str> = if block.starts_with("extern") {
                    // Single-line form: extract between { and }
                    if let (Some(open), Some(close)) = (block.find('{'), block.rfind('}')) {
                        let inner = block[open + 1..close].trim();
                        vec![inner]
                    } else {
                        vec![]
                    }
                } else {
                    // Multi-line: each line is a fn decl
                    block.lines().collect()
                };

                for decl in fn_decls {
                    let decl = decl.trim().trim_end_matches(';').trim();
                    if let Some(fn_start) = decl.find("fn ") {
                        let sig_str = &decl[fn_start + 3..];
                        match parse_signature(sig_str) {
                            Some(parsed) => {
                                let (func_uid, source_uid) =
                                    resolve_func_uid(&parsed.name, &rev_func, &existing_by_source);

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
                                    .or_else(|| {
                                        existing_funcs.get(&func_uid).and_then(|f| f.body.clone())
                                    });

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
                                        "parse_error: cannot parse extern fn: {}",
                                        decl
                                    ),
                                    location: Some(format!("line {}", i)),
                                });
                            }
                        }
                    }
                }
                continue;
            }

            // Parse: #[export] followed by fn
            if line == "#[export]" {
                i += 1;
                while i < lines.len() && lines[i].trim().is_empty() {
                    i += 1;
                }
                if i < lines.len() && lines[i].trim().starts_with("fn ") {
                    let fn_line = lines[i].trim();
                    let sig_str = fn_line["fn ".len()..].trim_end_matches('{').trim();
                    match parse_signature(sig_str) {
                        Some(parsed) => {
                            // Consume body until closing }
                            i += 1;
                            let mut brace_depth = 1;
                            while i < lines.len() && brace_depth > 0 {
                                let l = lines[i].trim();
                                for ch in l.chars() {
                                    if ch == '{' {
                                        brace_depth += 1;
                                    } else if ch == '}' {
                                        brace_depth -= 1;
                                    }
                                }
                                i += 1;
                            }

                            let (func_uid, source_uid) =
                                resolve_func_uid(&parsed.name, &rev_func, &existing_by_source);

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
                                .or_else(|| {
                                    existing_funcs.get(&func_uid).and_then(|f| f.body.clone())
                                });

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
                                message: format!(
                                    "parse_error: cannot parse fn after #[export]: {}",
                                    fn_line
                                ),
                                location: Some(format!("line {}", i)),
                            });
                            i += 1;
                        }
                    }
                } else {
                    errors.push(WastError {
                        message: "parse_error: expected 'fn' after '#[export]'".to_string(),
                        location: Some(format!("line {}", i + 1)),
                    });
                }
                continue;
            }

            // Parse: fn name(params) -> result { ... } (internal)
            if line.starts_with("fn ") {
                let sig_str = line["fn ".len()..].trim_end_matches('{').trim();
                match parse_signature(sig_str) {
                    Some(parsed) => {
                        // Consume body until closing }
                        i += 1;
                        let mut brace_depth = 1;
                        while i < lines.len() && brace_depth > 0 {
                            let l = lines[i].trim();
                            for ch in l.chars() {
                                if ch == '{' {
                                    brace_depth += 1;
                                } else if ch == '}' {
                                    brace_depth -= 1;
                                }
                            }
                            i += 1;
                        }

                        let (func_uid, source_uid) =
                            resolve_func_uid(&parsed.name, &rev_func, &existing_by_source);

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
                            message: format!("parse_error: cannot parse fn: {}", line),
                            location: Some(format!("line {}", i + 1)),
                        });
                        i += 1;
                    }
                }
                continue;
            }

            // Skip comment lines
            if line.starts_with("//") {
                i += 1;
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
            text.contains("extern \"wast\""),
            "should have extern block for imports"
        );
        assert!(text.contains("#[export]"), "should have export attribute");
        assert!(text.contains("fn "), "should have fn keyword");
    }

    #[test]
    fn test_to_text_internal_func_format() {
        let comp = make_test_component();
        let text = Component::to_text(comp);
        assert!(
            text.contains("fn my_func(param_one: u32) -> u32"),
            "internal func signature: {}",
            text
        );
    }

    #[test]
    fn test_to_text_import_format() {
        let comp = make_test_component();
        let text = Component::to_text(comp);
        assert!(
            text.contains("fn imported_fn(param_two: u32)"),
            "import signature: {}",
            text
        );
        assert!(
            text.contains("extern \"wast\""),
            "import should be in extern block: {}",
            text
        );
    }

    #[test]
    fn test_to_text_export_format() {
        let comp = make_test_component();
        let text = Component::to_text(comp);
        assert!(
            text.contains("#[export]\nfn exported_fn() -> u32"),
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
}
