#[allow(warnings)]
#[rustfmt::skip]
mod bindings;

use std::collections::BTreeMap;
use wast_pattern_analyzer::{ArithOp, CompareOp, Instruction};
use wast_syntax_core::scaffold::{self, ExistingIndex, UidGen};
use wast_syntax_core::wit_types::*;
use wast_syntax_core::{RenderContext, TypePrinter, convert};

struct Component;

// ---------------------------------------------------------------------------
// Surface — TS-flavored WIT type lexemes.
// ---------------------------------------------------------------------------

struct TsTypePrinter;

impl TypePrinter for TsTypePrinter {
    fn primitive(&self, p: &wast_types::PrimitiveType) -> String {
        primitive_name(p).to_string()
    }
    fn option(&self, inner: &str) -> String {
        format!("option<{inner}>")
    }
    fn result(&self, ok: &str, err: &str) -> String {
        format!("result<{ok}, {err}>")
    }
    fn list(&self, inner: &str) -> String {
        format!("list<{inner}>")
    }
    fn record(&self, fields: &[(String, String)]) -> String {
        let parts: Vec<String> = fields.iter().map(|(n, t)| format!("{n}: {t}")).collect();
        format!("record {{ {} }}", parts.join(", "))
    }
    fn variant(&self, cases: &[(String, Option<String>)]) -> String {
        let parts: Vec<String> = cases
            .iter()
            .map(|(n, t)| match t {
                Some(t) => format!("{n}({t})"),
                None => n.clone(),
            })
            .collect();
        format!("variant {{ {} }}", parts.join(", "))
    }
    fn tuple(&self, items: &[String]) -> String {
        format!("tuple<{}>", items.join(", "))
    }
    fn enum_(&self, cases: &[String]) -> String {
        format!("enum {{ {} }}", cases.join(" | "))
    }
    fn flags(&self, cases: &[String]) -> String {
        format!("flags {{ {} }}", cases.join(" | "))
    }
    fn resource(&self) -> String {
        "resource".into()
    }
    fn own(&self, target: &str) -> String {
        format!("own<{target}>")
    }
    fn borrow(&self, target: &str) -> String {
        format!("borrow<{target}>")
    }
}

fn format_type_ref(type_ref: &WitTypeRef, ctx: &RenderContext) -> String {
    wast_syntax_core::resolve_type_ref(type_ref, ctx, &TsTypePrinter)
}

fn format_wit_type_native(t: &wast_types::WitType, ctx: &RenderContext) -> String {
    wast_syntax_core::format_wit_type(t, ctx, &TsTypePrinter)
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
        P::String => "string",
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
        "string" => Some(PrimitiveType::String),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Body rendering
// ---------------------------------------------------------------------------

/// Render a serialized body. A body that fails to deserialize is an
/// error — rendering a placeholder comment instead silently produced
/// lossy output that could not faithfully round-trip.
fn render_body(
    body: &[u8],
    indent: &str,
    returns_value: bool,
    local_names: &BTreeMap<String, String>,
    func_names: &BTreeMap<String, String>,
) -> Result<String, String> {
    let instructions = wast_pattern_analyzer::deserialize_body(body)
        .map_err(|e| format!("cannot deserialize body ({} bytes): {e}", body.len()))?;
    let mut lines = Vec::new();
    let last_idx = instructions.len().saturating_sub(1);
    for (i, instr) in instructions.iter().enumerate() {
        let rendered = render_instruction(instr, indent, local_names, func_names);
        if rendered.is_empty() {
            continue;
        }
        // TS needs an explicit `return` — a bare trailing expression
        // evaluates to nothing. Wrap the last value-producing
        // instruction in `return <expr>;` when the function returns.
        if returns_value && i == last_idx && is_value_expr(instr) {
            let leading: String = rendered.chars().take_while(|c| c.is_whitespace()).collect();
            let trimmed = rendered[leading.len()..].to_string();
            lines.push(format!("{leading}return {trimmed};"));
        } else {
            lines.push(rendered);
        }
    }
    Ok(lines.join("\n"))
}

/// Instructions whose rendered form is an expression (has a value) — these
/// are the ones we wrap in `return …;` at a function body's tail position.
/// Statements like `LocalSet` (→ `let x = …;`) or control flow (`if`/`while`)
/// are excluded.
fn is_value_expr(i: &Instruction) -> bool {
    matches!(
        i,
        Instruction::LocalGet { .. }
            | Instruction::Const { .. }
            | Instruction::Arithmetic { .. }
            | Instruction::Compare { .. }
            | Instruction::Call { .. }
            | Instruction::Some { .. }
            | Instruction::None
            | Instruction::Ok { .. }
            | Instruction::Err { .. }
            | Instruction::IsErr { .. }
            | Instruction::StringLiteral { .. }
            | Instruction::StringLen { .. }
            | Instruction::ListLen { .. }
            | Instruction::RecordGet { .. }
            | Instruction::RecordLiteral { .. }
    )
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
                    format!("{inner}case {pattern}:\n{body_str}")
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!("{indent}switch ({val}) {{\n{arm_lines}\n{indent}}}")
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
            format!("{indent}[{parts}]")
        }
        Instruction::FlagsCtor { flags } => {
            format!("{indent}{{ {} }}", flags.join(", "))
        }
        Instruction::ResourceNew { resource, rep } => {
            let r = render_expr(rep, local_names, func_names);
            format!("{indent}new {resource}({r})")
        }
        Instruction::ResourceRep {
            resource: _,
            handle,
        } => render_expr(handle, local_names, func_names),
        Instruction::ResourceDrop { resource, handle } => {
            let h = render_expr(handle, local_names, func_names);
            format!("{indent}{resource}.drop({h})")
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

fn resolve_to_uid(name: &str, rev_map: &BTreeMap<String, String>) -> String {
    rev_map
        .get(name)
        .cloned()
        .unwrap_or_else(|| name.to_string())
}

fn find_matching_paren_str(s: &str, open_pos: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut i = open_pos;
    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            // Skip escaped characters inside string literals.
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == b'"' {
                in_str = false;
            }
        } else {
            match b {
                b'"' => in_str = true,
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
        i += 1;
    }
    None
}

/// Find the rightmost occurrence of `pattern` in `s` at bracket depth 0,
/// skipping over string literals (so operators inside `"..."` never split
/// the expression).
fn find_rightmost_top_level(s: &str, pattern: &str) -> Option<usize> {
    let pat = pattern.as_bytes();
    let pat_len = pat.len();
    if s.len() < pat_len {
        return None;
    }
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut last = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' => {
                in_str = true;
                i += 1;
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            _ => {}
        }
        if depth == 0 && i + pat_len <= bytes.len() && &bytes[i..i + pat_len] == pat {
            last = Some(i);
        }
        i += 1;
    }
    last
}

use wast_syntax_core::scaffold::split_top_level;

/// Net brace depth change of a line, ignoring braces inside string
/// literals and `//` line comments.
fn brace_delta(line: &str) -> i32 {
    let bytes = line.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == b'"' {
                in_str = false;
            }
        } else {
            match b {
                b'"' => in_str = true,
                b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => break,
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
        }
        i += 1;
    }
    depth
}

/// Skip past a `{...}` block. `i` should point to the first line inside the
/// block (after the opening `{`). On return, `i` points to the line after
/// the closing `}`. Brace counting is lexically aware (strings/comments).
fn skip_block(lines: &[&str], i: &mut usize) {
    let mut depth = 1i32;
    while *i < lines.len() {
        depth += brace_delta(lines[*i]);
        *i += 1;
        if depth <= 0 {
            break;
        }
    }
}

/// Parse a sequence of statements from `lines[*i..]`.
/// Stops (without consuming) at `}`, `} else`, or `case ` lines.
///
/// A statement that fails to parse is a hard error carrying the offending
/// 0-based line index — silently skipping it used to re-serialize a
/// *partial* body, dropping the unparsed statements without any
/// diagnostic.
fn parse_stmts(
    lines: &[&str],
    i: &mut usize,
    rev_local: &BTreeMap<String, String>,
    rev_func: &BTreeMap<String, String>,
) -> Result<Vec<Instruction>, (usize, String)> {
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
            Err(e) => return Err((saved, e)),
        }
    }
    Ok(instrs)
}

/// Fold a nested `parse_stmts` error (line index + message) into a plain
/// message so `parse_stmt` keeps a `String` error type; the top-level
/// caller re-attaches its own line number.
fn flatten_stmt_err((line, e): (usize, String)) -> String {
    format!("line {}: {}", line + 1, e)
}

/// Parse a single statement starting at `lines[*i]`.
fn parse_stmt(
    lines: &[&str],
    i: &mut usize,
    rev_local: &BTreeMap<String, String>,
    rev_func: &BTreeMap<String, String>,
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
        let then_body = parse_stmts(lines, i, rev_local, rev_func).map_err(flatten_stmt_err)?;
        let else_body = if *i < lines.len() && lines[*i].trim().starts_with("} else {") {
            *i += 1;
            parse_stmts(lines, i, rev_local, rev_func).map_err(flatten_stmt_err)?
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
        let body = parse_stmts(lines, i, rev_local, rev_func).map_err(flatten_stmt_err)?;
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
        let body = parse_stmts(lines, i, rev_local, rev_func).map_err(flatten_stmt_err)?;
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
                let body = parse_stmts(lines, i, rev_local, rev_func).map_err(flatten_stmt_err)?;
                cases.push((label, body));
            } else {
                *i += 1;
            }
        }
        return build_match_instruction(value, cases, rev_local);
    }

    // Fall through: expression statement (strip optional trailing semicolon).
    // Also strip a `return ` prefix — we emit `return <expr>;` at the tail of
    // value-returning functions, but the IR only stores the <expr> (the
    // implicit-return is handled by Canonical ABI at lift time).
    let mut expr_str = trimmed.trim_end_matches(';');
    if let Some(rest) = expr_str.strip_prefix("return ") {
        expr_str = rest.trim();
    }
    let instr = parse_expr_str(expr_str, rev_local, rev_func)?;
    *i += 1;
    Ok(instr)
}

fn parse_expr_str(
    s: &str,
    rev_local: &BTreeMap<String, String>,
    rev_func: &BTreeMap<String, String>,
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
    rev_local: &BTreeMap<String, String>,
    rev_func: &BTreeMap<String, String>,
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

    // "..." — string literal (inverse of the `{:?}` rendering)
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        if let Some(stripped) = s.get(1..s.len() - 1) {
            // Reject strings whose closing quote actually belongs to a
            // different literal (e.g. `"a" b "c"`): unescape validates.
            if let Ok(bytes) = unescape_string_literal(stripped) {
                return Ok(Instruction::StringLiteral { bytes });
            }
        }
    }

    // { a: EXPR, b: EXPR } — record literal; { a, b } — flags constructor
    if s.starts_with('{') && s.ends_with('}') {
        let inner = s[1..s.len() - 1].trim();
        if inner.is_empty() {
            return Ok(Instruction::FlagsCtor { flags: vec![] });
        }
        let parts: Vec<&str> = split_top_level(inner, ',')
            .into_iter()
            .map(|p| p.trim())
            .collect();
        let is_ident =
            |p: &str| !p.is_empty() && p.chars().all(|c| c.is_alphanumeric() || c == '_');
        if parts.iter().all(|p| is_ident(p)) {
            return Ok(Instruction::FlagsCtor {
                flags: parts.iter().map(|p| p.to_string()).collect(),
            });
        }
        let mut fields = Vec::new();
        for part in &parts {
            let colon = part
                .find(':')
                .ok_or_else(|| format!("record literal field missing ':': {part}"))?;
            let fname = part[..colon].trim();
            if !is_ident(fname) {
                return Err(format!("invalid record field name: {fname}"));
            }
            let fval = parse_expr_str(part[colon + 1..].trim(), rev_local, rev_func)?;
            fields.push((fname.to_string(), fval));
        }
        return Ok(Instruction::RecordLiteral { fields });
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

    // EXPR.field — record field access. `.length` is ambiguous (both
    // string-len and list-len render as `.length`), so reject it rather
    // than guess and silently corrupt the body.
    if let Some(dot) = find_rightmost_top_level(s, ".") {
        let field = s[dot + 1..].trim();
        if field == "length" {
            return Err("cannot parse `.length` (ambiguous string/list length)".to_string());
        }
        if !field.is_empty() && field.chars().all(|c| c.is_alphanumeric() || c == '_') && dot > 0 {
            let value = parse_expr_str(&s[..dot], rev_local, rev_func)?;
            return Ok(Instruction::RecordGet {
                value: Box::new(value),
                field: field.to_string(),
            });
        }
    }

    Err(format!("cannot parse expression: {}", s))
}

/// Decode the escapes produced by Rust's `{:?}` string formatting (which
/// `to_text` uses for string literals): `\n`, `\t`, `\r`, `\\`, `\"`,
/// `\'`, `\0` and `\u{HEX}`.
fn unescape_string_literal(s: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(s.len());
    let mut chars = s.chars();
    let mut buf = [0u8; 4];
    while let Some(c) = chars.next() {
        if c == '"' {
            // An unescaped quote means the trailing quote we stripped was
            // not this literal's terminator.
            return Err("unescaped quote inside string literal".to_string());
        }
        if c != '\\' {
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            continue;
        }
        match chars.next() {
            Some('n') => out.push(b'\n'),
            Some('t') => out.push(b'\t'),
            Some('r') => out.push(b'\r'),
            Some('\\') => out.push(b'\\'),
            Some('"') => out.push(b'"'),
            Some('\'') => out.push(b'\''),
            Some('0') => out.push(0),
            Some('u') => {
                if chars.next() != Some('{') {
                    return Err("bad \\u escape".to_string());
                }
                let mut hex = String::new();
                for h in chars.by_ref() {
                    if h == '}' {
                        break;
                    }
                    hex.push(h);
                }
                let cp = u32::from_str_radix(&hex, 16).map_err(|e| format!("bad \\u: {e}"))?;
                let ch = char::from_u32(cp).ok_or("bad \\u codepoint")?;
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            }
            other => return Err(format!("unknown escape: \\{other:?}")),
        }
    }
    Ok(out)
}

fn build_match_instruction(
    value: Instruction,
    cases: Vec<(String, Vec<Instruction>)>,
    rev_local: &BTreeMap<String, String>,
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

/// Parse function body lines and return serialized instructions (or the
/// existing body when the text consists entirely of comments / is empty).
///
/// Any statement that fails to parse aborts the body with an error
/// (line index + message); the caller surfaces it as a `WastError` and
/// skips past the body so parsing can continue with the next function.
fn parse_func_body(
    lines: &[&str],
    i: &mut usize,
    rev_local: &BTreeMap<String, String>,
    rev_func: &BTreeMap<String, String>,
    existing_body: Option<Vec<u8>>,
) -> Result<Option<Vec<u8>>, (usize, String)> {
    let instructions = parse_stmts(lines, i, rev_local, rev_func)?;
    if *i < lines.len() && lines[*i].trim() == "}" {
        *i += 1;
    }
    if instructions.is_empty() {
        Ok(existing_body)
    } else {
        Ok(Some(wast_pattern_analyzer::serialize_body(&instructions)))
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
        Some(type_ref) => format!(": {}", format_type_ref(type_ref, ctx)),
        None => String::new(),
    };

    let render = |b: &Option<Vec<u8>>, returns: bool| -> Result<String, String> {
        match b {
            Some(b) => render_body(b, "  ", returns, &ctx.local_names, &ctx.func_names)
                .map_err(|e| format!("func '{func_uid}': {e}")),
            None => Ok("  // [no body]".to_string()),
        }
    };

    match &func.source {
        FuncSource::Imported(_) => Ok(format!(
            "declare function {}({}){};",
            name, params_str, result_str
        )),
        FuncSource::Exported(_) => {
            let body_str = render(&func.body, func.result.is_some())?;
            Ok(format!(
                "export function {}({}){} {{\n{}\n}}",
                name, params_str, result_str, body_str
            ))
        }
        FuncSource::Internal(_) => {
            let body_str = render(&func.body, func.result.is_some())?;
            Ok(format!(
                "function {}({}){} {{\n{}\n}}",
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
    params: Vec<(String, String)>, // (param_name, type_string)
    result_type: Option<String>,
    _is_import: bool,
    _is_export: bool,
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
    // Fall back to matching the rendered form of each existing type so a
    // round-trip on `option<u32>` lands back at the original `opt_u32`
    // uid instead of inventing a brand-new type ref.
    for (uid, td) in types {
        if format_wit_type_native(&convert::wit_type(&td.definition), ctx) == s {
            return uid.clone();
        }
    }
    s.to_string()
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

// ---------------------------------------------------------------------------
// Guest implementation
// ---------------------------------------------------------------------------

impl bindings::exports::wast::core::syntax_renderer::Guest for Component {
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
}

impl bindings::exports::wast::core::syntax_editor::Guest for Component {
    fn from_text(text: String, existing: WastComponent) -> Result<WastComponent, Vec<WastError>> {
        let native_syms = convert::syms(&existing.syms);
        let native_types = convert::type_list(&existing.types);
        let ctx = RenderContext::new(&native_syms, &native_types);

        // Reverse map: display_name -> uid (funcs). The *local* reverse
        // map is built per function inside the loop — see
        // `scaffold::func_rev_local` for why a global one is wrong.
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
                        let (func_uid, source_uid) = scaffold::resolve_func_uid(
                            &parsed.name,
                            &rev_func,
                            &index,
                            &mut uid_gen,
                        );
                        let existing_func = index.find(&source_uid, &func_uid);
                        let rev_local = scaffold::func_rev_local(&ctx.local_names, existing_func);
                        let existing_body = existing_func.and_then(|f| f.body.clone());

                        i += 1;
                        let body = match parse_func_body(
                            &lines,
                            &mut i,
                            &rev_local,
                            &rev_func,
                            existing_body,
                        ) {
                            Ok(b) => b,
                            Err((line_idx, msg)) => {
                                errors.push(WastError {
                                    message: format!("parse_error: {msg}"),
                                    location: Some(format!("line {}", line_idx + 1)),
                                });
                                // Skip past the rest of this body so the
                                // following functions still get parsed
                                // (and their own errors reported).
                                skip_block(&lines, &mut i);
                                continue;
                            }
                        };

                        let params = scaffold::resolve_params(&parsed.params, &rev_local, |t| {
                            parse_type_ref_str(t, &existing.types, &ctx)
                        });
                        let result = parsed
                            .result_type
                            .as_ref()
                            .map(|r| parse_type_ref_str(r, &existing.types, &ctx));

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
                        let (func_uid, source_uid) = scaffold::resolve_func_uid(
                            &parsed.name,
                            &rev_func,
                            &index,
                            &mut uid_gen,
                        );
                        let existing_func = index.find(&source_uid, &func_uid);
                        let rev_local = scaffold::func_rev_local(&ctx.local_names, existing_func);
                        let existing_body = existing_func.and_then(|f| f.body.clone());

                        i += 1;
                        let body = match parse_func_body(
                            &lines,
                            &mut i,
                            &rev_local,
                            &rev_func,
                            existing_body,
                        ) {
                            Ok(b) => b,
                            Err((line_idx, msg)) => {
                                errors.push(WastError {
                                    message: format!("parse_error: {msg}"),
                                    location: Some(format!("line {}", line_idx + 1)),
                                });
                                // Skip past the rest of this body so the
                                // following functions still get parsed
                                // (and their own errors reported).
                                skip_block(&lines, &mut i);
                                continue;
                            }
                        };

                        let params = scaffold::resolve_params(&parsed.params, &rev_local, |t| {
                            parse_type_ref_str(t, &existing.types, &ctx)
                        });
                        let result = parsed
                            .result_type
                            .as_ref()
                            .map(|r| parse_type_ref_str(r, &existing.types, &ctx));

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

        // Post-process: ts-like surface syntax doesn't render Call arg
        // parameter names (positional only), so the body parser leaves
        // them empty. Recover them from each call target's signature so
        // the IR round-trips structurally — the IR's Call args carry
        // (param_name, value) pairs to keep the wast layer keyword-style.
        let params_by_func: BTreeMap<String, Vec<String>> = funcs
            .iter()
            .map(|(uid, f)| {
                (
                    uid.clone(),
                    f.params.iter().map(|(n, _)| n.clone()).collect(),
                )
            })
            .collect();
        for (_, f) in funcs.iter_mut() {
            if let Some(body) = &f.body {
                if let Ok(mut instrs) = wast_pattern_analyzer::deserialize_body(body) {
                    let mut changed = false;
                    for instr in &mut instrs {
                        if wast_pattern_analyzer::fill_call_arg_names(instr, &params_by_func) {
                            changed = true;
                        }
                    }
                    if changed {
                        f.body = Some(wast_pattern_analyzer::serialize_body(&instrs));
                    }
                }
            }
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
    use bindings::exports::wast::core::syntax_editor::Guest as _;
    use bindings::exports::wast::core::syntax_renderer::Guest as _;

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
        let text = Component::to_text(comp).unwrap();
        assert!(
            text.contains("function my_func(param_one: u32): u32"),
            "internal func signature: {}",
            text
        );
    }

    #[test]
    fn test_to_text_import_format() {
        let comp = make_test_component();
        let text = Component::to_text(comp).unwrap();
        assert!(
            text.contains("declare function imported_fn(param_two: u32)"),
            "import signature: {}",
            text
        );
    }

    #[test]
    fn test_to_text_export_format() {
        let comp = make_test_component();
        let text = Component::to_text(comp).unwrap();
        assert!(
            text.contains("export function exported_fn(): u32"),
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
        assert_eq!(f1.unwrap().1.body, expected, "body should round-trip");
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
    fn test_call_arg_names_recovered_inside_nested_containers() {
        // ts-like's surface renders call args positionally, so the parser
        // leaves their names empty and `fixup_call_args` recovers them from
        // the callee's signature. It used to stop at container variants, so a
        // call inside a record/list/tuple/variant/match kept empty names —
        // and the compiler, which resolves args *by name*, rejected the body.
        let mut params = BTreeMap::new();
        params.insert("f1".to_string(), vec!["p1".to_string()]);

        let call = || Instruction::Call {
            func_uid: "f1".to_string(),
            args: vec![(String::new(), Instruction::Const { value: 1 })],
        };
        let cases = vec![
            Instruction::RecordLiteral {
                fields: vec![("field".to_string(), call())],
            },
            Instruction::ListLiteral {
                values: vec![call()],
            },
            Instruction::TupleLiteral {
                values: vec![call()],
            },
            Instruction::VariantCtor {
                case: "c".to_string(),
                value: Some(Box::new(call())),
            },
            Instruction::MatchVariant {
                value: Box::new(Instruction::LocalGet {
                    uid: "v".to_string(),
                }),
                arms: vec![wast_pattern_analyzer::MatchArm {
                    case: "c".to_string(),
                    binding: None,
                    body: vec![call()],
                }],
            },
            Instruction::ResourceNew {
                resource: "r".to_string(),
                rep: Box::new(call()),
            },
        ];

        for mut instr in cases {
            let changed = wast_pattern_analyzer::fill_call_arg_names(&mut instr, &params);
            assert!(changed, "expected a fixup in {instr:?}");
            let mut names = Vec::new();
            collect_call_arg_names(&instr, &mut names);
            assert_eq!(names, vec!["p1"], "arg name not recovered in {instr:?}");
        }
    }

    /// Every `Call` arg name appearing anywhere in an instruction tree.
    fn collect_call_arg_names(instr: &Instruction, out: &mut Vec<String>) {
        if let Instruction::Call { args, .. } = instr {
            out.extend(args.iter().map(|(n, _)| n.clone()));
        }
        let mut copy = instr.clone();
        wast_pattern_analyzer::for_each_child_mut(&mut copy, &mut |child| {
            collect_call_arg_names(child, out)
        });
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

    #[test]
    fn test_to_text_errors_on_undeserializable_body() {
        let mut comp = make_test_component();
        comp.funcs[0].1.body = Some(vec![0xff, 0xfe, 0xfd]);
        let result = Component::to_text(comp);
        assert!(result.is_err(), "garbage body must surface an error");
        assert!(result.unwrap_err()[0].message.contains("render_error"));
    }

    #[test]
    fn test_from_text_unparsable_statement_is_an_error() {
        let comp = make_test_component();
        let text = "export function exported_fn(): u32 {\n  @@@ not a statement\n}\n";
        let result = Component::from_text(text.to_string(), comp);
        assert!(result.is_err(), "unparsable body statement must error");
        let errs = result.unwrap_err();
        assert!(
            errs.iter().any(|e| e.message.contains("parse_error")),
            "expected parse_error, got: {errs:?}"
        );
        assert!(
            errs[0].location.as_deref() == Some("line 2"),
            "error should carry the body line: {errs:?}"
        );
    }

    #[test]
    fn test_body_roundtrip_record_literal_and_get() {
        assert_body_roundtrip(vec![Instruction::RecordLiteral {
            fields: vec![
                (
                    "x".to_string(),
                    Instruction::LocalGet {
                        uid: "p1".to_string(),
                    },
                ),
                ("y".to_string(), Instruction::Const { value: 2 }),
            ],
        }]);
        assert_body_roundtrip(vec![Instruction::RecordGet {
            value: Box::new(Instruction::LocalGet {
                uid: "p1".to_string(),
            }),
            field: "x".to_string(),
        }]);
    }

    #[test]
    fn test_body_roundtrip_string_literal() {
        assert_body_roundtrip(vec![Instruction::StringLiteral {
            bytes: "hello, \"wast\" \\ world\n".as_bytes().to_vec(),
        }]);
    }

    #[test]
    fn test_local_names_scoped_per_function() {
        // Two functions whose params share the display name "x" but have
        // different uids. Each body must resolve "x" back to *its own*
        // param uid, not the other function's.
        let body1 = wast_pattern_analyzer::serialize_body(&[Instruction::LocalGet {
            uid: "p1".to_string(),
        }]);
        let body2 = wast_pattern_analyzer::serialize_body(&[Instruction::LocalGet {
            uid: "p2".to_string(),
        }]);
        let comp = WastComponent {
            funcs: vec![
                (
                    "f1".to_string(),
                    WastFunc {
                        source: FuncSource::Internal("f1".to_string()),
                        params: vec![("p1".to_string(), "t1".to_string())],
                        result: Some("t1".to_string()),
                        body: Some(body1.clone()),
                    },
                ),
                (
                    "f2".to_string(),
                    WastFunc {
                        source: FuncSource::Internal("f2".to_string()),
                        params: vec![("p2".to_string(), "t1".to_string())],
                        result: Some("t1".to_string()),
                        body: Some(body2.clone()),
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
                wit_syms: vec![],
                internal: vec![
                    SymEntry {
                        uid: "f1".to_string(),
                        display_name: "alpha".to_string(),
                    },
                    SymEntry {
                        uid: "f2".to_string(),
                        display_name: "beta".to_string(),
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
                        uid: "p2".to_string(),
                        display_name: "x".to_string(),
                    },
                ],
            },
        };
        let text = Component::to_text(comp.clone()).unwrap();
        let parsed = Component::from_text(text, comp).unwrap();
        let f1 = &parsed.funcs.iter().find(|(u, _)| u == "f1").unwrap().1;
        let f2 = &parsed.funcs.iter().find(|(u, _)| u == "f2").unwrap().1;
        assert_eq!(f1.params[0].0, "p1");
        assert_eq!(f2.params[0].0, "p2");
        assert_eq!(f1.body, Some(body1), "f1 body must reference p1");
        assert_eq!(f2.body, Some(body2), "f2 body must reference p2");
    }
}
