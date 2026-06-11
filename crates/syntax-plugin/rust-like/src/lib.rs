#[allow(warnings)]
#[rustfmt::skip]
mod bindings;

use std::collections::BTreeMap;
use wast_pattern_analyzer::{ArithOp, CompareOp, Instruction};
use wast_syntax_core::scaffold::{self, ExistingIndex, UidGen, split_top_level};
use wast_syntax_core::wit_types::*;
use wast_syntax_core::{RenderContext, TypePrinter, convert};

struct Component;

// ---------------------------------------------------------------------------
// Surface — Rust-flavored WIT type lexemes.
// ---------------------------------------------------------------------------

struct RustTypePrinter;

impl TypePrinter for RustTypePrinter {
    fn primitive(&self, p: &wast_types::PrimitiveType) -> String {
        primitive_name(p).to_string()
    }
    fn option(&self, inner: &str) -> String {
        format!("Option<{inner}>")
    }
    fn result(&self, ok: &str, err: &str) -> String {
        format!("Result<{ok}, {err}>")
    }
    fn list(&self, inner: &str) -> String {
        format!("Vec<{inner}>")
    }
    fn record(&self, fields: &[(String, String)]) -> String {
        let parts: Vec<String> = fields.iter().map(|(n, t)| format!("{n}: {t}")).collect();
        format!("struct {{ {} }}", parts.join(", "))
    }
    fn variant(&self, cases: &[(String, Option<String>)]) -> String {
        let parts: Vec<String> = cases
            .iter()
            .map(|(n, t)| match t {
                Some(t) => format!("{n}({t})"),
                None => n.clone(),
            })
            .collect();
        format!("enum {{ {} }}", parts.join(", "))
    }
    fn tuple(&self, items: &[String]) -> String {
        format!("({})", items.join(", "))
    }
    fn enum_(&self, cases: &[String]) -> String {
        format!("enum {{ {} }}", cases.join(", "))
    }
    fn flags(&self, cases: &[String]) -> String {
        format!("bitflags! {{ {} }}", cases.join(", "))
    }
    fn resource(&self) -> String {
        "resource".into()
    }
    fn own(&self, target: &str) -> String {
        format!("Own<{target}>")
    }
    fn borrow(&self, target: &str) -> String {
        format!("Borrow<{target}>")
    }
}

fn format_type_ref(type_ref: &WitTypeRef, ctx: &RenderContext) -> String {
    wast_syntax_core::resolve_type_ref(type_ref, ctx, &RustTypePrinter)
}

fn format_wit_type_native(t: &wast_types::WitType, ctx: &RenderContext) -> String {
    wast_syntax_core::format_wit_type(t, ctx, &RustTypePrinter)
}

fn primitive_name(p: &wast_types::PrimitiveType) -> &'static str {
    use wast_types::PrimitiveType as P;
    match p {
        P::U32 => "u32",
        P::U64 => "u64",
        P::I32 => "i32",
        P::I64 => "i64",
        P::F32 => "f32",
        P::F64 => "f64",
        P::Bool => "bool",
        P::Char => "char",
        P::String => "String",
    }
}

fn primitive_name_binding(p: &PrimitiveType) -> &'static str {
    primitive_name(&convert::primitive(p))
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

/// Render a serialized body. A body that fails to deserialize is an
/// error — rendering a placeholder comment instead silently produced
/// lossy output.
fn render_body(
    body: &[u8],
    indent: &str,
    local_names: &BTreeMap<String, String>,
    func_names: &BTreeMap<String, String>,
) -> Result<String, String> {
    let instructions = wast_pattern_analyzer::deserialize_body(body)
        .map_err(|e| format!("cannot deserialize body ({} bytes): {e}", body.len()))?;
    Ok(render_instructions(
        &instructions,
        indent,
        local_names,
        func_names,
    ))
}

fn render_instructions(
    instructions: &[Instruction],
    indent: &str,
    local_names: &BTreeMap<String, String>,
    func_names: &BTreeMap<String, String>,
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

fn resolve_local_name(uid: &str, local_names: &BTreeMap<String, String>) -> String {
    local_names
        .get(uid)
        .cloned()
        .unwrap_or_else(|| uid.to_string())
}

fn resolve_func_name_body(uid: &str, func_names: &BTreeMap<String, String>) -> String {
    func_names
        .get(uid)
        .cloned()
        .unwrap_or_else(|| uid.to_string())
}

fn render_instruction(
    instr: &Instruction,
    indent: &str,
    local_names: &BTreeMap<String, String>,
    func_names: &BTreeMap<String, String>,
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
            format!("{indent}Record {{ {pairs} }}")
        }
        Instruction::VariantCtor { case, value } => match value {
            Some(v) => {
                let val = render_expr(v, local_names, func_names);
                format!("{indent}{case}({val})")
            }
            None => format!("{indent}{case}"),
        },
        Instruction::MatchVariant { value, arms } => {
            let val = render_expr(value, local_names, func_names);
            let arm_lines = arms
                .iter()
                .map(|arm| {
                    let pattern = match &arm.binding {
                        Some(b) => format!("{}({})", arm.case, b),
                        None => arm.case.clone(),
                    };
                    let body_str = render_instructions(&arm.body, &inner, local_names, func_names);
                    format!("{inner}{pattern} => {{\n{body_str}\n{inner}}}")
                })
                .collect::<Vec<_>>()
                .join(",\n");
            format!("{indent}match {val} {{\n{arm_lines}\n{indent}}}")
        }
        Instruction::TupleGet { value, index } => {
            let val = render_expr(value, local_names, func_names);
            format!("{indent}{val}.{index}")
        }
        Instruction::TupleLiteral { values } => {
            let parts = values
                .iter()
                .map(|v| render_expr(v, local_names, func_names))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{indent}({parts})")
        }
        Instruction::ListLiteral { values } => {
            let parts = values
                .iter()
                .map(|v| render_expr(v, local_names, func_names))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{indent}vec![{parts}]")
        }
        Instruction::FlagsCtor { flags } => {
            format!("{indent}Flags::{}", flags.join(" | "))
        }
        Instruction::ResourceNew { resource, rep } => {
            let r = render_expr(rep, local_names, func_names);
            format!("{indent}{resource}::new({r})")
        }
        Instruction::ResourceRep {
            resource: _,
            handle,
        } => render_expr(handle, local_names, func_names),
        Instruction::ResourceDrop { resource, handle } => {
            let h = render_expr(handle, local_names, func_names);
            format!("{indent}drop::<{resource}>({h})")
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
    local_names: &BTreeMap<String, String>,
    func_names: &BTreeMap<String, String>,
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

fn func_to_text(func_uid: &str, func: &WastFunc, ctx: &RenderContext) -> Result<String, String> {
    let source_uid = match &func.source {
        FuncSource::Internal(u) | FuncSource::Imported(u) | FuncSource::Exported(u) => u.clone(),
    };

    let name = ctx
        .func_names
        .get(&source_uid)
        .or_else(|| ctx.func_names.get(func_uid))
        .cloned()
        .unwrap_or_else(|| func_uid.to_string());

    let params_str = func
        .params
        .iter()
        .map(|(param_uid, type_ref)| {
            let pname = ctx.local_name(param_uid).to_string();
            let tname = format_type_ref(type_ref, ctx);
            format!("{}: {}", pname, tname)
        })
        .collect::<Vec<_>>()
        .join(", ");

    let result_str = match &func.result {
        Some(type_ref) => format!(" -> {}", format_type_ref(type_ref, ctx)),
        None => String::new(),
    };

    let render = |b: &Option<Vec<u8>>| -> Result<String, String> {
        match b {
            Some(b) => render_body(b, "    ", &ctx.local_names, &ctx.func_names)
                .map_err(|e| format!("func '{func_uid}': {e}")),
            None => Ok("    // [no body]".to_string()),
        }
    };

    match &func.source {
        FuncSource::Imported(_) => Ok(format!(
            "extern \"wast\" {{\n    fn {}({}){};\n}}",
            name, params_str, result_str
        )),
        FuncSource::Exported(_) => {
            let body_str = render(&func.body)?;
            Ok(format!(
                "#[export]\nfn {}({}){} {{\n{}\n}}",
                name, params_str, result_str, body_str
            ))
        }
        FuncSource::Internal(_) => {
            let body_str = render(&func.body)?;
            Ok(format!(
                "fn {}({}){} {{\n{}\n}}",
                name, params_str, result_str, body_str
            ))
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
    ctx: &RenderContext,
) -> WitTypeRef {
    let s = s.trim();
    if parse_primitive(s).is_some() {
        for (uid, td) in types {
            if let WitType::Primitive(p) = &td.definition {
                if primitive_name_binding(p) == s {
                    return uid.clone();
                }
            }
        }
        for (uid, name) in &ctx.type_names {
            if name == s {
                return uid.clone();
            }
        }
        return s.to_string();
    }
    for (uid, name) in &ctx.type_names {
        if name == s {
            return uid.clone();
        }
    }
    // Fall back to matching the rendered form of each existing type so
    // round-tripping `Option<u32>` lands back at the original `opt_u32`
    // uid instead of inventing a brand-new type ref.
    for (uid, td) in types {
        if format_wit_type_native(&convert::wit_type(&td.definition), ctx) == s {
            return uid.clone();
        }
    }
    s.to_string()
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
        split_top_level(params_str, ',')
            .into_iter()
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

// ---------------------------------------------------------------------------
// Lexically-aware body scanning
// ---------------------------------------------------------------------------

/// Cross-line lexical state for [`rust_brace_delta`].
#[derive(Default)]
struct RustLexState {
    in_block_comment: bool,
}

/// Net brace depth change of a Rust-like source line, ignoring braces in
/// string literals, char literals, `//` line comments and `/* … */` block
/// comments (block-comment state carries across lines).
///
/// A naive per-char count treated `let s = "}";` or `// {` as real braces
/// and lost track of a function's closing brace.
fn rust_brace_delta(line: &str, state: &mut RustLexState) -> i32 {
    let bytes = line.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    while i < bytes.len() {
        if state.in_block_comment {
            if bytes[i] == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                state.in_block_comment = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        match bytes[i] {
            b'"' => {
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == b'"' {
                        break;
                    }
                    i += 1;
                }
                i += 1; // past closing quote (or end of line)
            }
            b'\'' => {
                // Char literal ('x' or '\x'); a lone quote (lifetime) is
                // left alone.
                if i + 2 < bytes.len() && bytes[i + 1] != b'\\' && bytes[i + 2] == b'\'' {
                    i += 3;
                } else if i + 3 < bytes.len() && bytes[i + 1] == b'\\' && bytes[i + 3] == b'\'' {
                    i += 4;
                } else {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => break,
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                state.in_block_comment = true;
                i += 2;
            }
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    depth
}

// ---------------------------------------------------------------------------
// Guest implementation
// ---------------------------------------------------------------------------

impl bindings::exports::wast::core::syntax_plugin::Guest for Component {
    fn to_text(component: WastComponent) -> Result<String, Vec<WastError>> {
        let native_syms = convert::syms(&component.syms);
        let native_types = convert::type_list(&component.types);
        let ctx = RenderContext::new(&native_syms, &native_types);

        let mut parts: Vec<String> = Vec::new();
        let mut errors: Vec<WastError> = Vec::new();
        for (func_uid, func) in &component.funcs {
            match func_to_text(func_uid, func, &ctx) {
                Ok(text) => parts.push(text),
                Err(e) => errors.push(WastError {
                    message: format!("render_error: {e}"),
                    location: Some(format!("func {func_uid}")),
                }),
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(parts.join("\n\n"))
    }

    fn from_text(text: String, existing: WastComponent) -> Result<WastComponent, Vec<WastError>> {
        let native_syms = convert::syms(&existing.syms);
        let native_types = convert::type_list(&existing.types);
        let ctx = RenderContext::new(&native_syms, &native_types);

        // Reverse map: display_name -> uid (funcs). The *local* reverse
        // map is built per function — see `scaffold::func_rev_local`.
        let rev_func = ExistingIndex::reverse(&ctx.func_names);
        let index = ExistingIndex::new(&existing);
        let mut uid_gen = UidGen::new(index.used_uids.clone());

        let mut errors: Vec<WastError> = Vec::new();
        let mut funcs: Vec<(FuncUid, WastFunc)> = Vec::new();
        let mut new_syms_internal: Vec<SymEntry> = existing.syms.internal.clone();
        let new_syms_local: Vec<SymEntry> = existing.syms.local.clone();

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
                                let (func_uid, source_uid) = scaffold::resolve_func_uid(
                                    &parsed.name,
                                    &rev_func,
                                    &index,
                                    &mut uid_gen,
                                );
                                let existing_func = index.find(&source_uid, &func_uid);
                                let rev_local =
                                    scaffold::func_rev_local(&ctx.local_names, existing_func);

                                let params =
                                    scaffold::resolve_params(&parsed.params, &rev_local, |t| {
                                        parse_type_ref_str(t, &existing.types, &ctx)
                                    });
                                let result = parsed
                                    .result_type
                                    .as_ref()
                                    .map(|r| parse_type_ref_str(r, &existing.types, &ctx));

                                let body = existing_func.and_then(|f| f.body.clone());

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
                            // Consume body until the matching closing }
                            // (lexically aware — see `rust_brace_delta`).
                            i += 1;
                            let mut brace_depth = 1;
                            let mut lex = RustLexState::default();
                            while i < lines.len() && brace_depth > 0 {
                                brace_depth += rust_brace_delta(lines[i], &mut lex);
                                i += 1;
                            }

                            let (func_uid, source_uid) = scaffold::resolve_func_uid(
                                &parsed.name,
                                &rev_func,
                                &index,
                                &mut uid_gen,
                            );
                            let existing_func = index.find(&source_uid, &func_uid);
                            let rev_local =
                                scaffold::func_rev_local(&ctx.local_names, existing_func);

                            let params =
                                scaffold::resolve_params(&parsed.params, &rev_local, |t| {
                                    parse_type_ref_str(t, &existing.types, &ctx)
                                });
                            let result = parsed
                                .result_type
                                .as_ref()
                                .map(|r| parse_type_ref_str(r, &existing.types, &ctx));

                            let body = existing_func.and_then(|f| f.body.clone());

                            scaffold::ensure_func_sym(
                                &source_uid,
                                &parsed.name,
                                &mut new_syms_internal,
                            );

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
                        // Consume body until the matching closing }
                        // (lexically aware — see `rust_brace_delta`).
                        i += 1;
                        let mut brace_depth = 1;
                        let mut lex = RustLexState::default();
                        while i < lines.len() && brace_depth > 0 {
                            brace_depth += rust_brace_delta(lines[i], &mut lex);
                            i += 1;
                        }

                        let (func_uid, source_uid) = scaffold::resolve_func_uid(
                            &parsed.name,
                            &rev_func,
                            &index,
                            &mut uid_gen,
                        );
                        let existing_func = index.find(&source_uid, &func_uid);
                        let rev_local = scaffold::func_rev_local(&ctx.local_names, existing_func);

                        let params = scaffold::resolve_params(&parsed.params, &rev_local, |t| {
                            parse_type_ref_str(t, &existing.types, &ctx)
                        });
                        let result = parsed
                            .result_type
                            .as_ref()
                            .map(|r| parse_type_ref_str(r, &existing.types, &ctx));

                        let body = existing_func.and_then(|f| f.body.clone());

                        scaffold::ensure_func_sym(
                            &source_uid,
                            &parsed.name,
                            &mut new_syms_internal,
                        );

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
                        body: Some(wast_pattern_analyzer::serialize_body(&[
                            Instruction::LocalGet {
                                uid: "p1".to_string(),
                            },
                        ])),
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
                        body: Some(wast_pattern_analyzer::serialize_body(&[
                            Instruction::Const { value: 7 },
                        ])),
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
        let text = Component::to_text(comp).unwrap();
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
        let text = Component::to_text(comp).unwrap();
        assert!(
            text.contains("fn my_func(param_one: u32) -> u32"),
            "internal func signature: {}",
            text
        );
    }

    #[test]
    fn test_to_text_import_format() {
        let comp = make_test_component();
        let text = Component::to_text(comp).unwrap();
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
        let text = Component::to_text(comp).unwrap();
        assert!(
            text.contains("#[export]\nfn exported_fn() -> u32"),
            "export signature: {}",
            text
        );
    }

    #[test]
    fn test_roundtrip_to_text_from_text_to_text() {
        let comp = make_test_component();
        let text1 = Component::to_text(comp.clone()).unwrap();

        let parsed = Component::from_text(text1.clone(), comp.clone());
        assert!(parsed.is_ok(), "from_text failed: {:?}", parsed.err());
        let parsed = parsed.unwrap();

        assert_eq!(parsed.funcs.len(), comp.funcs.len(), "func count mismatch");

        let text2 = Component::to_text(parsed).unwrap();
        assert_eq!(text1, text2, "roundtrip text mismatch");
    }

    #[test]
    fn test_from_text_preserves_body() {
        let comp = make_test_component();
        let expected = comp
            .funcs
            .iter()
            .find(|(uid, _)| uid == "f1")
            .unwrap()
            .1
            .body
            .clone();
        let text = Component::to_text(comp.clone()).unwrap();
        let parsed = Component::from_text(text, comp).unwrap();

        let f1 = parsed.funcs.iter().find(|(uid, _)| uid == "f1");
        assert!(f1.is_some(), "f1 should exist");
        assert_eq!(f1.unwrap().1.body, expected, "body should be preserved");
    }

    #[test]
    fn test_from_text_preserves_func_source_kinds() {
        let comp = make_test_component();
        let text = Component::to_text(comp.clone()).unwrap();
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
        let text = Component::to_text(comp.clone()).unwrap();
        assert_eq!(text, "", "empty component should produce empty text");

        let parsed = Component::from_text(text, comp);
        assert!(parsed.is_ok());
        assert_eq!(parsed.unwrap().funcs.len(), 0);
    }

    // -----------------------------------------------------------------------
    // Body roundtrip tests
    //
    // rust-like's `from_text` doesn't currently parse body content — it
    // counts brace nesting from the `fn ... {` opener and skips until the
    // matching `}`, then restores the body bytes from the `existing`
    // component. These tests lock in that contract: to_text → from_text
    // (with same component as `existing`) → to_text produces identical
    // text. A future milestone would replace the skip-and-restore with a
    // real recursive-descent parser; until then, these tests guarantee
    // round-trippable preservation across a representative IR sample.
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
        let text1 = Component::to_text(comp.clone()).unwrap();
        let parsed = Component::from_text(text1.clone(), comp);
        assert!(parsed.is_ok(), "from_text failed: {:?}", parsed.err());
        let text2 = Component::to_text(parsed.unwrap()).unwrap();
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

    #[test]
    fn test_to_text_errors_on_undeserializable_body() {
        let mut comp = make_test_component();
        comp.funcs[0].1.body = Some(vec![0xff, 0xfe, 0xfd]);
        let result = Component::to_text(comp);
        assert!(result.is_err(), "garbage body must surface an error");
        assert!(result.unwrap_err()[0].message.contains("render_error"));
    }

    #[test]
    fn test_body_roundtrip_string_literal_with_braces() {
        // The body skip must not count braces *inside string literals*.
        assert_body_roundtrip(vec![
            Instruction::LocalSet {
                uid: "v1".into(),
                value: Box::new(Instruction::StringLiteral {
                    bytes: b"}}}".to_vec(),
                }),
            },
            Instruction::LocalSet {
                uid: "v2".into(),
                value: Box::new(Instruction::StringLiteral {
                    bytes: b"{ // }".to_vec(),
                }),
            },
            Instruction::Return,
        ]);
    }

    #[test]
    fn test_body_skip_ignores_strings_and_comments() {
        let comp = make_test_component();
        // The first fn's body contains a stray close brace in a string and
        // an open brace in a comment; both must be ignored so the second
        // fn is still found.
        let text = concat!(
            "fn my_func(param_one: u32) -> u32 {\n",
            "    let s = \"}\";\n",
            "    // {\n",
            "    /* { */\n",
            "    param_one\n",
            "}\n",
            "\n",
            "#[export]\n",
            "fn exported_fn() -> u32 {\n",
            "    7\n",
            "}\n",
        );
        let parsed = Component::from_text(text.to_string(), comp).unwrap();
        assert_eq!(parsed.funcs.len(), 2, "both fns must be found");
        assert!(parsed.funcs.iter().any(|(uid, _)| uid == "f1"));
        assert!(parsed.funcs.iter().any(|(uid, _)| uid == "f3"));
    }

    #[test]
    fn test_rust_brace_delta_lexing() {
        let mut st = RustLexState::default();
        assert_eq!(rust_brace_delta("let s = \"}\";", &mut st), 0);
        assert_eq!(rust_brace_delta("// }", &mut st), 0);
        assert_eq!(rust_brace_delta("let c = '}';", &mut st), 0);
        assert_eq!(rust_brace_delta("if x { /* } */", &mut st), 1);
        assert!(!st.in_block_comment);
        assert_eq!(rust_brace_delta("/* start", &mut st), 0);
        assert!(st.in_block_comment);
        assert_eq!(rust_brace_delta("} still comment", &mut st), 0);
        assert_eq!(rust_brace_delta("end */ }", &mut st), -1);
        assert!(!st.in_block_comment);
    }
}
