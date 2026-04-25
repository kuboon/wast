#[allow(warnings)]
#[rustfmt::skip]
mod bindings;

use bindings::wast::core::types::*;
use wast_pattern_analyzer::{ArithOp, CompareOp, Instruction};

struct Component;

// ---------------------------------------------------------------------------
// Type rendering
// ---------------------------------------------------------------------------

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

fn render_wit_type(wit_type: &WitType, types: &[(TypeUid, WastTypeDef)]) -> String {
    let mut resolving = std::collections::BTreeSet::new();
    render_wit_type_with_guard(wit_type, types, &mut resolving)
}

fn render_wit_type_with_guard(
    wit_type: &WitType,
    types: &[(TypeUid, WastTypeDef)],
    resolving: &mut std::collections::BTreeSet<String>,
) -> String {
    match wit_type {
        WitType::Primitive(p) => primitive_name(p).to_string(),
        WitType::Option(inner) => {
            format!(
                "(option {})",
                render_type_ref_with_guard(inner, types, resolving)
            )
        }
        WitType::Result((ok, err)) => {
            format!(
                "(result {} {})",
                render_type_ref_with_guard(ok, types, resolving),
                render_type_ref_with_guard(err, types, resolving)
            )
        }
        WitType::List(inner) => {
            format!(
                "(list {})",
                render_type_ref_with_guard(inner, types, resolving)
            )
        }
        WitType::Record(fields) => {
            let parts: Vec<String> = fields
                .iter()
                .map(|(name, tref)| {
                    format!(
                        "(field ${} {})",
                        name,
                        render_type_ref_with_guard(tref, types, resolving)
                    )
                })
                .collect();
            format!("(record {})", parts.join(" "))
        }
        WitType::Variant(cases) => {
            let parts: Vec<String> = cases
                .iter()
                .map(|(name, tref)| match tref {
                    Some(t) => {
                        format!(
                            "(case ${} {})",
                            name,
                            render_type_ref_with_guard(t, types, resolving)
                        )
                    }
                    None => format!("(case ${})", name),
                })
                .collect();
            format!("(variant {})", parts.join(" "))
        }
        WitType::Tuple(refs) => {
            let parts: Vec<String> = refs
                .iter()
                .map(|r| render_type_ref_with_guard(r, types, resolving))
                .collect();
            format!("(tuple {})", parts.join(" "))
        }
        WitType::Enum(cases) => {
            let parts: Vec<String> = cases.iter().map(|c| format!("(case ${c})")).collect();
            format!("(enum {})", parts.join(" "))
        }
        WitType::Flags(names) => {
            let parts: Vec<String> = names.iter().map(|n| format!("(flag ${n})")).collect();
            format!("(flags {})", parts.join(" "))
        }
        WitType::Resource => "(resource)".to_string(),
        WitType::Own(r) => format!("(own {})", render_type_ref_with_guard(r, types, resolving)),
        WitType::Borrow(r) => format!(
            "(borrow {})",
            render_type_ref_with_guard(r, types, resolving)
        ),
    }
}

fn render_type_ref(type_ref: &WitTypeRef, types: &[(TypeUid, WastTypeDef)]) -> String {
    let mut resolving = std::collections::BTreeSet::new();
    render_type_ref_with_guard(type_ref, types, &mut resolving)
}

fn render_type_ref_with_guard(
    type_ref: &WitTypeRef,
    types: &[(TypeUid, WastTypeDef)],
    resolving: &mut std::collections::BTreeSet<String>,
) -> String {
    // Break recursive type expansion (e.g. tid1 -> record(field tid1)).
    if resolving.contains(type_ref) {
        return format!("${}", type_ref);
    }

    for (uid, typedef) in types {
        if uid == type_ref {
            resolving.insert(uid.clone());
            let rendered = render_wit_type_with_guard(&typedef.definition, types, resolving);
            resolving.remove(uid);
            return rendered;
        }
    }
    // Not an inline type — reference by uid
    format!("${}", type_ref)
}

// ---------------------------------------------------------------------------
// Instruction rendering (S-expression)
// ---------------------------------------------------------------------------

fn render_instructions(instructions: &[Instruction], indent: &str) -> String {
    let mut lines = Vec::new();
    for instr in instructions {
        lines.push(render_instruction(instr, indent));
    }
    lines.join("\n")
}

fn render_instruction(instr: &Instruction, indent: &str) -> String {
    let inner = format!("{}  ", indent);
    match instr {
        Instruction::Nop => format!("{}(nop)", indent),
        Instruction::Return => format!("{}(return)", indent),
        Instruction::Const { value } => format!("{}(i64.const {})", indent, value),
        Instruction::LocalGet { uid } => {
            format!("{}(local.get ${})", indent, uid)
        }
        Instruction::LocalSet { uid, value } => {
            format!(
                "{}(local.set ${}\n{})",
                indent,
                uid,
                render_instruction(value, &inner)
            )
        }
        Instruction::Call { func_uid, args } => {
            if args.is_empty() {
                format!("{}(call ${})", indent, func_uid)
            } else {
                let arg_strs: Vec<String> = args
                    .iter()
                    .map(|(param_uid, arg)| {
                        format!(
                            "{}(; ${} ;)\n{}",
                            inner,
                            param_uid,
                            render_instruction(arg, &inner)
                        )
                    })
                    .collect();
                format!("{}(call ${}\n{})", indent, func_uid, arg_strs.join("\n"))
            }
        }
        Instruction::Compare { op, lhs, rhs } => {
            let op_str = match op {
                CompareOp::Eq => "eq",
                CompareOp::Ne => "ne",
                CompareOp::Lt => "lt",
                CompareOp::Le => "le",
                CompareOp::Gt => "gt",
                CompareOp::Ge => "ge",
            };
            format!(
                "{}(i64.{}\n{}\n{})",
                indent,
                op_str,
                render_instruction(lhs, &inner),
                render_instruction(rhs, &inner)
            )
        }
        Instruction::Arithmetic { op, lhs, rhs } => {
            let op_str = match op {
                ArithOp::Add => "add",
                ArithOp::Sub => "sub",
                ArithOp::Mul => "mul",
                ArithOp::Div => "div",
            };
            format!(
                "{}(i64.{}\n{}\n{})",
                indent,
                op_str,
                render_instruction(lhs, &inner),
                render_instruction(rhs, &inner)
            )
        }
        Instruction::Block { label, body } => {
            let label_str = match label {
                Some(l) => format!(" ${}", l),
                None => String::new(),
            };
            let body_str = render_instructions(body, &inner);
            format!("{}(block{}\n{}\n{})", indent, label_str, body_str, indent)
        }
        Instruction::Loop { label, body } => {
            let label_str = match label {
                Some(l) => format!(" ${}", l),
                None => String::new(),
            };
            let body_str = render_instructions(body, &inner);
            format!("{}(loop{}\n{}\n{})", indent, label_str, body_str, indent)
        }
        Instruction::If {
            condition,
            then_body,
            else_body,
        } => {
            let cond_str = render_instruction(condition, &inner);
            let then_str = render_instructions(then_body, &format!("{}  ", inner));
            if else_body.is_empty() {
                format!(
                    "{}(if\n{}\n{}(then\n{}\n{}))",
                    indent, cond_str, inner, then_str, inner
                )
            } else {
                let else_str = render_instructions(else_body, &format!("{}  ", inner));
                format!(
                    "{}(if\n{}\n{}(then\n{}\n{})\n{}(else\n{}\n{}))",
                    indent, cond_str, inner, then_str, inner, inner, else_str, inner
                )
            }
        }
        Instruction::BrIf { label, condition } => {
            format!(
                "{}(br_if ${}\n{})",
                indent,
                label,
                render_instruction(condition, &inner)
            )
        }
        Instruction::Br { label } => {
            format!("{}(br ${})", indent, label)
        }
        Instruction::Some { value } => {
            format!("{}(some\n{})", indent, render_instruction(value, &inner))
        }
        Instruction::None => format!("{}(none)", indent),
        Instruction::Ok { value } => {
            format!("{}(ok\n{})", indent, render_instruction(value, &inner))
        }
        Instruction::Err { value } => {
            format!("{}(err\n{})", indent, render_instruction(value, &inner))
        }
        Instruction::IsErr { value } => {
            format!("{}(is_err\n{})", indent, render_instruction(value, &inner))
        }
        Instruction::StringLen { value } => {
            format!(
                "{}(string.len\n{})",
                indent,
                render_instruction(value, &inner)
            )
        }
        Instruction::StringLiteral { bytes } => {
            // Escape as hex for round-trip safety (raw syntax shouldn't
            // interpret UTF-8 specially — it's just bytes).
            let escaped: String = bytes.iter().map(|b| format!("\\{b:02x}")).collect();
            format!("{indent}(string.literal \"{escaped}\")")
        }
        Instruction::ListLen { value } => {
            format!(
                "{}(list.len\n{})",
                indent,
                render_instruction(value, &inner)
            )
        }
        Instruction::RecordGet { value, field } => {
            format!(
                "{}(record.get ${}\n{})",
                indent,
                field,
                render_instruction(value, &inner)
            )
        }
        Instruction::RecordLiteral { fields } => {
            let fields_str = fields
                .iter()
                .map(|(fname, fval)| {
                    format!(
                        "{inner}(; ${fname} ;)\n{}",
                        render_instruction(fval, &inner)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!("{indent}(record.literal\n{fields_str})")
        }
        Instruction::VariantCtor { case, value } => match value {
            Some(v) => format!(
                "{}(variant.case ${}\n{})",
                indent,
                case,
                render_instruction(v, &inner)
            ),
            None => format!("{}(variant.case ${})", indent, case),
        },
        Instruction::MatchVariant { value, arms } => {
            let val_str = render_instruction(value, &inner);
            let arms_str = arms
                .iter()
                .map(|arm| {
                    let body_str = render_instructions(&arm.body, &format!("{}  ", inner));
                    match &arm.binding {
                        Some(b) => format!(
                            "{inner}(case ${case} ${binding}\n{body}\n{inner})",
                            case = arm.case,
                            binding = b,
                            body = body_str
                        ),
                        None => format!(
                            "{inner}(case ${case}\n{body}\n{inner})",
                            case = arm.case,
                            body = body_str
                        ),
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!("{indent}(match_variant\n{val_str}\n{arms_str})")
        }
        Instruction::TupleGet { value, index } => {
            format!(
                "{}(tuple.get {}\n{})",
                indent,
                index,
                render_instruction(value, &inner)
            )
        }
        Instruction::TupleLiteral { values } => {
            let vals = values
                .iter()
                .map(|v| render_instruction(v, &inner))
                .collect::<Vec<_>>()
                .join("\n");
            format!("{indent}(tuple.literal\n{vals})")
        }
        Instruction::ListLiteral { values } => {
            let vals = values
                .iter()
                .map(|v| render_instruction(v, &inner))
                .collect::<Vec<_>>()
                .join("\n");
            format!("{indent}(list.literal\n{vals})")
        }
        Instruction::FlagsCtor { flags } => {
            let parts: Vec<String> = flags.iter().map(|f| format!("${f}")).collect();
            format!("{indent}(flags.ctor {})", parts.join(" "))
        }
        Instruction::ResourceNew { resource, rep } => {
            let r = render_instruction(rep, &inner);
            format!("{indent}(resource.new ${resource}\n{r})")
        }
        Instruction::ResourceRep { resource, handle } => {
            let h = render_instruction(handle, &inner);
            format!("{indent}(resource.rep ${resource}\n{h})")
        }
        Instruction::ResourceDrop { resource, handle } => {
            let h = render_instruction(handle, &inner);
            format!("{indent}(resource.drop ${resource}\n{h})")
        }
        Instruction::MatchOption {
            value,
            some_binding,
            some_body,
            none_body,
        } => {
            let val_str = render_instruction(value, &inner);
            let some_str = render_instructions(some_body, &format!("{}  ", inner));
            let none_str = render_instructions(none_body, &format!("{}  ", inner));
            format!(
                "{}(match_option\n{}\n{}(some ${}\n{}\n{})\n{}(none\n{}\n{}))",
                indent, val_str, inner, some_binding, some_str, inner, inner, none_str, inner
            )
        }
        Instruction::MatchResult {
            value,
            ok_binding,
            ok_body,
            err_binding,
            err_body,
        } => {
            let val_str = render_instruction(value, &inner);
            let ok_str = render_instructions(ok_body, &format!("{}  ", inner));
            let err_str = render_instructions(err_body, &format!("{}  ", inner));
            format!(
                "{}(match_result\n{}\n{}(ok ${}\n{}\n{})\n{}(err ${}\n{}\n{}))",
                indent,
                val_str,
                inner,
                ok_binding,
                ok_str,
                inner,
                inner,
                err_binding,
                err_str,
                inner
            )
        }
    }
}

fn render_body(body: &[u8], indent: &str) -> String {
    match wast_pattern_analyzer::deserialize_body(body) {
        Ok(instructions) => render_instructions(&instructions, indent),
        Err(_) => format!("{}(; body: {} bytes ;)", indent, body.len()),
    }
}

// ---------------------------------------------------------------------------
// Function rendering
// ---------------------------------------------------------------------------

fn render_func_source(source: &FuncSource) -> (&'static str, &str) {
    match source {
        FuncSource::Internal(u) => ("internal", u.as_str()),
        FuncSource::Imported(u) => ("import", u.as_str()),
        FuncSource::Exported(u) => ("export", u.as_str()),
    }
}

fn func_to_text(func_uid: &str, func: &WastFunc, types: &[(TypeUid, WastTypeDef)]) -> String {
    let (source_kind, source_uid) = render_func_source(&func.source);

    let params_str: String = func
        .params
        .iter()
        .map(|(param_uid, type_ref)| {
            format!(
                " (param ${} {})",
                param_uid,
                render_type_ref(type_ref, types)
            )
        })
        .collect();

    let result_str = match &func.result {
        Some(type_ref) => format!(" (result {})", render_type_ref(type_ref, types)),
        None => String::new(),
    };

    let body_str = match &func.body {
        Some(b) => format!("\n{}", render_body(b, "    ")),
        None => String::new(),
    };

    format!(
        "  (func ${} ({} ${}){}{}{}\n  )",
        func_uid, source_kind, source_uid, params_str, result_str, body_str
    )
}

// ---------------------------------------------------------------------------
// Type definition rendering
// ---------------------------------------------------------------------------

fn render_type_source(source: &TypeSource) -> (&'static str, &str) {
    match source {
        TypeSource::Internal(u) => ("internal", u.as_str()),
        TypeSource::Imported(u) => ("import", u.as_str()),
        TypeSource::Exported(u) => ("export", u.as_str()),
    }
}

fn type_to_text(type_uid: &str, typedef: &WastTypeDef, types: &[(TypeUid, WastTypeDef)]) -> String {
    let (source_kind, source_uid) = render_type_source(&typedef.source);
    let def_str = render_wit_type(&typedef.definition, types);
    format!(
        "  (type ${} ({} ${}) {})",
        type_uid, source_kind, source_uid, def_str
    )
}

// ---------------------------------------------------------------------------
// Syms rendering
// ---------------------------------------------------------------------------

fn syms_to_text(syms: &Syms) -> String {
    let mut lines = Vec::new();
    lines.push("  (syms".to_string());

    if !syms.wit_syms.is_empty() {
        lines.push("    (wit".to_string());
        for (uid, display) in &syms.wit_syms {
            lines.push(format!("      (sym \"{}\" \"{}\")", uid, display));
        }
        lines.push("    )".to_string());
    }

    if !syms.internal.is_empty() {
        lines.push("    (internal".to_string());
        for entry in &syms.internal {
            lines.push(format!(
                "      (sym \"{}\" \"{}\")",
                entry.uid, entry.display_name
            ));
        }
        lines.push("    )".to_string());
    }

    if !syms.local.is_empty() {
        lines.push("    (local".to_string());
        for entry in &syms.local {
            lines.push(format!(
                "      (sym \"{}\" \"{}\")",
                entry.uid, entry.display_name
            ));
        }
        lines.push("    )".to_string());
    }

    lines.push("  )".to_string());
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// from_text — S-expression tokenizer / parser
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum Tok {
    Open,
    Close,
    Atom(String),
    Str(Vec<u8>),
    Comment(String),
}

fn tokenize(text: &str) -> Result<Vec<Tok>, String> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if b == b';' && i + 1 < bytes.len() && bytes[i + 1] == b';' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b == b'(' && i + 1 < bytes.len() && bytes[i + 1] == b';' {
            i += 2;
            let start = i;
            while i + 1 < bytes.len() && !(bytes[i] == b';' && bytes[i + 1] == b')') {
                i += 1;
            }
            if i + 1 >= bytes.len() {
                return Err("unterminated block comment".to_string());
            }
            let content = std::str::from_utf8(&bytes[start..i])
                .map_err(|e| format!("comment utf-8: {e}"))?
                .trim()
                .to_string();
            tokens.push(Tok::Comment(content));
            i += 2;
            continue;
        }
        if b == b'(' {
            tokens.push(Tok::Open);
            i += 1;
            continue;
        }
        if b == b')' {
            tokens.push(Tok::Close);
            i += 1;
            continue;
        }
        if b == b'"' {
            i += 1;
            let mut out = Vec::new();
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    let c = bytes[i + 1];
                    match c {
                        b'\\' => {
                            out.push(b'\\');
                            i += 2;
                        }
                        b'"' => {
                            out.push(b'"');
                            i += 2;
                        }
                        b'n' => {
                            out.push(b'\n');
                            i += 2;
                        }
                        b't' => {
                            out.push(b'\t');
                            i += 2;
                        }
                        b'r' => {
                            out.push(b'\r');
                            i += 2;
                        }
                        _ if c.is_ascii_hexdigit() => {
                            if i + 2 >= bytes.len() || !bytes[i + 2].is_ascii_hexdigit() {
                                return Err(format!("bad hex escape at offset {i}"));
                            }
                            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap();
                            let val = u8::from_str_radix(hex, 16)
                                .map_err(|e| format!("hex parse: {e}"))?;
                            out.push(val);
                            i += 3;
                        }
                        _ => {
                            return Err(format!("unknown escape \\{} at offset {i}", c as char));
                        }
                    }
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            if i >= bytes.len() {
                return Err("unterminated string".to_string());
            }
            i += 1;
            tokens.push(Tok::Str(out));
            continue;
        }
        let start = i;
        while i < bytes.len() {
            let c = bytes[i];
            if c.is_ascii_whitespace() || c == b'(' || c == b')' || c == b'"' {
                break;
            }
            if c == b';' && i + 1 < bytes.len() && bytes[i + 1] == b';' {
                break;
            }
            i += 1;
        }
        if i == start {
            return Err(format!("unexpected byte 0x{b:02x} at offset {start}"));
        }
        let atom = std::str::from_utf8(&bytes[start..i])
            .map_err(|e| format!("atom utf-8: {e}"))?
            .to_string();
        tokens.push(Tok::Atom(atom));
    }
    Ok(tokens)
}

#[derive(Debug, Clone)]
enum Sexp {
    Atom(String),
    Str(Vec<u8>),
    List(Vec<Sexp>),
    Comment(String),
}

fn parse_sexps(tokens: &[Tok]) -> Result<Vec<Sexp>, String> {
    let mut pos = 0;
    let mut out = Vec::new();
    while pos < tokens.len() {
        out.push(parse_one_sexp(tokens, &mut pos)?);
    }
    Ok(out)
}

fn parse_one_sexp(tokens: &[Tok], pos: &mut usize) -> Result<Sexp, String> {
    if *pos >= tokens.len() {
        return Err("unexpected end of input".into());
    }
    match &tokens[*pos] {
        Tok::Open => {
            *pos += 1;
            let mut items = Vec::new();
            while *pos < tokens.len() {
                if matches!(tokens[*pos], Tok::Close) {
                    *pos += 1;
                    return Ok(Sexp::List(items));
                }
                items.push(parse_one_sexp(tokens, pos)?);
            }
            Err("unterminated list".into())
        }
        Tok::Close => Err("unexpected ')'".into()),
        Tok::Atom(a) => {
            let s = a.clone();
            *pos += 1;
            Ok(Sexp::Atom(s))
        }
        Tok::Str(b) => {
            let s = b.clone();
            *pos += 1;
            Ok(Sexp::Str(s))
        }
        Tok::Comment(c) => {
            let s = c.clone();
            *pos += 1;
            Ok(Sexp::Comment(s))
        }
    }
}

// ---------------------------------------------------------------------------
// from_text — Sexp → WastComponent walker
// ---------------------------------------------------------------------------

fn as_atom(s: &Sexp) -> Option<&str> {
    match s {
        Sexp::Atom(a) => Some(a.as_str()),
        _ => None,
    }
}

fn as_list(s: &Sexp) -> Option<&[Sexp]> {
    match s {
        Sexp::List(l) => Some(l.as_slice()),
        _ => None,
    }
}

/// Strip a leading `$` from an atom; everything else passes through.
fn strip_dollar(s: &str) -> &str {
    s.strip_prefix('$').unwrap_or(s)
}

/// `drop_comments` filters out `Sexp::Comment(_)` entries, cloning the rest
/// so the returned Vec owns its items and indexing works ergonomically
/// without borrow-of-borrow gymnastics.
fn drop_comments(items: &[Sexp]) -> Vec<Sexp> {
    items
        .iter()
        .filter(|s| !matches!(s, Sexp::Comment(_)))
        .cloned()
        .collect()
}

fn parse_primitive_atom(s: &str) -> Option<PrimitiveType> {
    Some(match s {
        "u32" => PrimitiveType::U32,
        "u64" => PrimitiveType::U64,
        "i32" => PrimitiveType::I32,
        "i64" => PrimitiveType::I64,
        "f32" => PrimitiveType::F32,
        "f64" => PrimitiveType::F64,
        "bool" => PrimitiveType::Bool,
        "char" => PrimitiveType::Char,
        "string" => PrimitiveType::String,
        _ => return None,
    })
}

/// Map from each existing type's *rendered inline form* to its uid. The
/// renderer expands inline whenever it can, so any type ref in the round-
/// tripped text matches one of these keys (or is an explicit `$uid` atom).
type TypeLookup = std::collections::HashMap<String, TypeUid>;

fn build_type_lookup(types: &[(TypeUid, WastTypeDef)]) -> TypeLookup {
    let mut map: TypeLookup = std::collections::HashMap::new();
    for (uid, td) in types {
        let key = render_wit_type(&td.definition, types);
        map.entry(key).or_insert_with(|| uid.clone());
    }
    map
}

/// Render a Sexp back to the same compact one-line form `render_wit_type`
/// produces, so we can use it as a lookup key for inline type refs.
fn sexp_to_compact(s: &Sexp) -> String {
    match s {
        Sexp::Atom(a) => a.clone(),
        Sexp::Str(_) => String::new(),
        Sexp::Comment(_) => String::new(),
        Sexp::List(items) => {
            let parts: Vec<String> = items
                .iter()
                .filter(|x| !matches!(x, Sexp::Comment(_)))
                .map(sexp_to_compact)
                .filter(|s| !s.is_empty())
                .collect();
            format!("({})", parts.join(" "))
        }
    }
}

fn parse_wit_type(sexp: &Sexp, lookup: &TypeLookup) -> Result<WitType, String> {
    match sexp {
        Sexp::Atom(a) => parse_primitive_atom(a)
            .map(WitType::Primitive)
            .ok_or_else(|| format!("not a primitive type: {a}")),
        Sexp::List(items) => {
            let kids = drop_comments(items);
            let head = kids
                .first()
                .and_then(|s| as_atom(s))
                .ok_or_else(|| "expected list head atom".to_string())?;
            match head {
                "option" => {
                    let inner = kids.get(1).ok_or("option needs 1 arg")?;
                    Ok(WitType::Option(parse_type_ref(inner, lookup)?))
                }
                "result" => {
                    let ok = kids.get(1).ok_or("result needs 2 args")?;
                    let err = kids.get(2).ok_or("result needs 2 args")?;
                    Ok(WitType::Result((
                        parse_type_ref(ok, lookup)?,
                        parse_type_ref(err, lookup)?,
                    )))
                }
                "list" => {
                    let inner = kids.get(1).ok_or("list needs 1 arg")?;
                    Ok(WitType::List(parse_type_ref(inner, lookup)?))
                }
                "record" => {
                    let mut fields = Vec::new();
                    for f in &kids[1..] {
                        let fl = as_list(f).ok_or("record field must be list")?;
                        let fkids = drop_comments(fl);
                        if fkids.first().and_then(as_atom) != Some("field") {
                            return Err("expected (field $name TYPE)".into());
                        }
                        let name = fkids
                            .get(1)
                            .and_then(|s| as_atom(s))
                            .ok_or("field needs name")?;
                        let ty = fkids.get(2).ok_or("field needs type")?;
                        fields.push((strip_dollar(name).to_string(), parse_type_ref(ty, lookup)?));
                    }
                    Ok(WitType::Record(fields))
                }
                "variant" => {
                    let mut cases = Vec::new();
                    for f in &kids[1..] {
                        let fl = as_list(f).ok_or("variant case must be list")?;
                        let fkids = drop_comments(fl);
                        if fkids.first().and_then(as_atom) != Some("case") {
                            return Err("expected (case $name [TYPE])".into());
                        }
                        let name = fkids
                            .get(1)
                            .and_then(|s| as_atom(s))
                            .ok_or("case needs name")?;
                        let payload = match fkids.get(2) {
                            Some(t) => Some(parse_type_ref(t, lookup)?),
                            None => None,
                        };
                        cases.push((strip_dollar(name).to_string(), payload));
                    }
                    Ok(WitType::Variant(cases))
                }
                "tuple" => {
                    let mut refs = Vec::new();
                    for f in &kids[1..] {
                        refs.push(parse_type_ref(f, lookup)?);
                    }
                    Ok(WitType::Tuple(refs))
                }
                "enum" => {
                    let mut names = Vec::new();
                    for f in &kids[1..] {
                        let fl = as_list(f).ok_or("enum case must be list")?;
                        let fkids = drop_comments(fl);
                        if fkids.first().and_then(as_atom) != Some("case") {
                            return Err("expected (case $name)".into());
                        }
                        let name = fkids
                            .get(1)
                            .and_then(|s| as_atom(s))
                            .ok_or("enum case needs name")?;
                        names.push(strip_dollar(name).to_string());
                    }
                    Ok(WitType::Enum(names))
                }
                "flags" => {
                    let mut names = Vec::new();
                    for f in &kids[1..] {
                        let fl = as_list(f).ok_or("flag entry must be list")?;
                        let fkids = drop_comments(fl);
                        if fkids.first().and_then(as_atom) != Some("flag") {
                            return Err("expected (flag $name)".into());
                        }
                        let name = fkids
                            .get(1)
                            .and_then(|s| as_atom(s))
                            .ok_or("flag needs name")?;
                        names.push(strip_dollar(name).to_string());
                    }
                    Ok(WitType::Flags(names))
                }
                "resource" => Ok(WitType::Resource),
                "own" => {
                    let inner = kids.get(1).ok_or("own needs 1 arg")?;
                    Ok(WitType::Own(parse_type_ref(inner, lookup)?))
                }
                "borrow" => {
                    let inner = kids.get(1).ok_or("borrow needs 1 arg")?;
                    Ok(WitType::Borrow(parse_type_ref(inner, lookup)?))
                }
                other => Err(format!("unknown type form: {other}")),
            }
        }
        _ => Err("expected type sexp".into()),
    }
}

/// Resolve a type ref sexp against the lookup map. `$uid` atoms strip the
/// dollar; bare atoms / list forms match by rendered inline form.
fn parse_type_ref(sexp: &Sexp, lookup: &TypeLookup) -> Result<WitTypeRef, String> {
    if let Sexp::Atom(a) = sexp {
        if let Some(uid) = a.strip_prefix('$') {
            return Ok(uid.to_string());
        }
        if let Some(uid) = lookup.get(a) {
            return Ok(uid.clone());
        }
        // Bare atom that isn't a known type — keep it (may match a bare-name
        // type in the existing component, or a freshly added one).
        return Ok(a.clone());
    }
    let key = sexp_to_compact(sexp);
    if let Some(uid) = lookup.get(&key) {
        return Ok(uid.clone());
    }
    Ok(key)
}

fn parse_source_kind(items: &[Sexp]) -> Result<(String, String), String> {
    if items.len() != 2 {
        return Err("expected (KIND $UID)".into());
    }
    let kind = as_atom(&items[0]).ok_or("source kind atom missing")?;
    let uid_atom = as_atom(&items[1]).ok_or("source uid atom missing")?;
    Ok((kind.to_string(), strip_dollar(uid_atom).to_string()))
}

fn parse_func_source(sexp: &Sexp) -> Result<FuncSource, String> {
    let l = as_list(sexp).ok_or("func source must be list")?;
    let kids = drop_comments(l);
    let (kind, uid) = parse_source_kind(&kids)?;
    Ok(match kind.as_str() {
        "internal" => FuncSource::Internal(uid),
        "import" => FuncSource::Imported(uid),
        "export" => FuncSource::Exported(uid),
        other => return Err(format!("unknown func source: {other}")),
    })
}

fn parse_type_source(sexp: &Sexp) -> Result<TypeSource, String> {
    let l = as_list(sexp).ok_or("type source must be list")?;
    let kids = drop_comments(l);
    let (kind, uid) = parse_source_kind(&kids)?;
    Ok(match kind.as_str() {
        "internal" => TypeSource::Internal(uid),
        "import" => TypeSource::Imported(uid),
        "export" => TypeSource::Exported(uid),
        other => return Err(format!("unknown type source: {other}")),
    })
}

fn parse_type_form(sexp: &[Sexp], lookup: &TypeLookup) -> Result<(TypeUid, WastTypeDef), String> {
    let kids = drop_comments(sexp);
    if kids.first().and_then(as_atom) != Some("type") {
        return Err("expected (type ...)".into());
    }
    let uid_atom = kids.get(1).and_then(|s| as_atom(s)).ok_or("type uid")?;
    let uid = strip_dollar(uid_atom).to_string();
    let source = parse_type_source(kids.get(2).ok_or("type needs source")?)?;
    let def_sexp = kids.get(3).ok_or("type needs definition")?;
    let definition = parse_wit_type(def_sexp, lookup)?;
    Ok((uid, WastTypeDef { source, definition }))
}

fn parse_func_form(items: &[Sexp], lookup: &TypeLookup) -> Result<(FuncUid, WastFunc), String> {
    let kids = drop_comments(items);
    if kids.first().and_then(as_atom) != Some("func") {
        return Err("expected (func ...)".into());
    }
    let uid_atom = kids.get(1).and_then(|s| as_atom(s)).ok_or("func uid")?;
    let uid = strip_dollar(uid_atom).to_string();
    let source = parse_func_source(kids.get(2).ok_or("func needs source")?)?;

    let mut params: Vec<(FuncUid, WitTypeRef)> = Vec::new();
    let mut result: Option<WitTypeRef> = None;
    let mut body_instrs: Vec<Instruction> = Vec::new();
    let mut have_body = false;

    for child in &kids[3..] {
        match child {
            Sexp::List(l) => {
                let cl = drop_comments(l);
                let head = cl.first().and_then(as_atom).unwrap_or("");
                match head {
                    "param" => {
                        let pname = cl.get(1).and_then(as_atom).ok_or("param name")?;
                        let pty = cl.get(2).ok_or("param type")?;
                        params.push((
                            strip_dollar(pname).to_string(),
                            parse_type_ref(pty, lookup)?,
                        ));
                    }
                    "result" => {
                        let rty = cl.get(1).ok_or("result type")?;
                        result = Some(parse_type_ref(rty, lookup)?);
                    }
                    _ => {
                        // Body instruction
                        body_instrs.push(parse_instruction(child)?);
                        have_body = true;
                    }
                }
            }
            _ => continue,
        }
    }

    let body = if have_body {
        Some(wast_pattern_analyzer::serialize_body(&body_instrs))
    } else {
        None
    };

    Ok((
        uid,
        WastFunc {
            source,
            params,
            result,
            body,
        },
    ))
}

fn parse_instruction(sexp: &Sexp) -> Result<Instruction, String> {
    let list = as_list(sexp).ok_or("instruction must be list")?;
    let kids = drop_comments(list);
    let head = kids.first().and_then(as_atom).ok_or("instruction head")?;
    match head {
        "nop" => Ok(Instruction::Nop),
        "return" => Ok(Instruction::Return),
        "i64.const" => {
            let v = kids
                .get(1)
                .and_then(as_atom)
                .ok_or("i64.const needs value")?;
            let n: i64 = v.parse().map_err(|e| format!("i64.const: {e}"))?;
            Ok(Instruction::Const { value: n })
        }
        "local.get" => {
            let u = kids.get(1).and_then(as_atom).ok_or("local.get needs uid")?;
            Ok(Instruction::LocalGet {
                uid: strip_dollar(u).to_string(),
            })
        }
        "local.set" => {
            let u = kids.get(1).and_then(as_atom).ok_or("local.set needs uid")?;
            let v = kids.get(2).ok_or("local.set needs value")?;
            Ok(Instruction::LocalSet {
                uid: strip_dollar(u).to_string(),
                value: Box::new(parse_instruction(v)?),
            })
        }
        "call" => {
            let u = kids.get(1).and_then(as_atom).ok_or("call needs uid")?;
            let func_uid = strip_dollar(u).to_string();
            // Walk the *original* (with comments) list to pair
            // `(; $param ;)` comments with the next instruction.
            let mut args: Vec<(String, Instruction)> = Vec::new();
            let mut pending_name: Option<String> = None;
            for item in &list[2..] {
                match item {
                    Sexp::Comment(c) => {
                        pending_name = Some(strip_dollar(c.trim()).to_string());
                    }
                    Sexp::List(_) => {
                        let name = pending_name.take().unwrap_or_default();
                        args.push((name, parse_instruction(item)?));
                    }
                    _ => {}
                }
            }
            Ok(Instruction::Call { func_uid, args })
        }
        "i64.eq" | "i64.ne" | "i64.lt" | "i64.le" | "i64.gt" | "i64.ge" => {
            let op = match head {
                "i64.eq" => CompareOp::Eq,
                "i64.ne" => CompareOp::Ne,
                "i64.lt" => CompareOp::Lt,
                "i64.le" => CompareOp::Le,
                "i64.gt" => CompareOp::Gt,
                _ => CompareOp::Ge,
            };
            let lhs = kids.get(1).ok_or("compare needs lhs")?;
            let rhs = kids.get(2).ok_or("compare needs rhs")?;
            Ok(Instruction::Compare {
                op,
                lhs: Box::new(parse_instruction(lhs)?),
                rhs: Box::new(parse_instruction(rhs)?),
            })
        }
        "i64.add" | "i64.sub" | "i64.mul" | "i64.div" => {
            let op = match head {
                "i64.add" => ArithOp::Add,
                "i64.sub" => ArithOp::Sub,
                "i64.mul" => ArithOp::Mul,
                _ => ArithOp::Div,
            };
            let lhs = kids.get(1).ok_or("arith needs lhs")?;
            let rhs = kids.get(2).ok_or("arith needs rhs")?;
            Ok(Instruction::Arithmetic {
                op,
                lhs: Box::new(parse_instruction(lhs)?),
                rhs: Box::new(parse_instruction(rhs)?),
            })
        }
        "block" | "loop" => {
            // Optional `$label` then body instructions
            let mut idx = 1;
            let label = match kids.get(idx).and_then(as_atom) {
                Some(a) if a.starts_with('$') => {
                    idx += 1;
                    Some(a.trim_start_matches('$').to_string())
                }
                _ => None,
            };
            let mut body = Vec::new();
            for item in &kids[idx..] {
                if matches!(item, Sexp::List(_)) {
                    body.push(parse_instruction(item)?);
                }
            }
            if head == "block" {
                Ok(Instruction::Block { label, body })
            } else {
                Ok(Instruction::Loop { label, body })
            }
        }
        "if" => {
            let cond = kids.get(1).ok_or("if needs condition")?;
            let condition = Box::new(parse_instruction(cond)?);
            let mut then_body = Vec::new();
            let mut else_body = Vec::new();
            for item in &kids[2..] {
                let il = as_list(item).ok_or("if branch must be list")?;
                let ikids = drop_comments(il);
                let h = ikids.first().and_then(as_atom).unwrap_or("");
                match h {
                    "then" => {
                        for x in &ikids[1..] {
                            then_body.push(parse_instruction(x)?);
                        }
                    }
                    "else" => {
                        for x in &ikids[1..] {
                            else_body.push(parse_instruction(x)?);
                        }
                    }
                    _ => return Err(format!("unexpected if branch: {h}")),
                }
            }
            Ok(Instruction::If {
                condition,
                then_body,
                else_body,
            })
        }
        "br_if" => {
            let l = kids.get(1).and_then(as_atom).ok_or("br_if label")?;
            let c = kids.get(2).ok_or("br_if condition")?;
            Ok(Instruction::BrIf {
                label: strip_dollar(l).to_string(),
                condition: Box::new(parse_instruction(c)?),
            })
        }
        "br" => {
            let l = kids.get(1).and_then(as_atom).ok_or("br label")?;
            Ok(Instruction::Br {
                label: strip_dollar(l).to_string(),
            })
        }
        "some" => {
            let v = kids.get(1).ok_or("some value")?;
            Ok(Instruction::Some {
                value: Box::new(parse_instruction(v)?),
            })
        }
        "none" => Ok(Instruction::None),
        "ok" => {
            let v = kids.get(1).ok_or("ok value")?;
            Ok(Instruction::Ok {
                value: Box::new(parse_instruction(v)?),
            })
        }
        "err" => {
            let v = kids.get(1).ok_or("err value")?;
            Ok(Instruction::Err {
                value: Box::new(parse_instruction(v)?),
            })
        }
        "is_err" => {
            let v = kids.get(1).ok_or("is_err value")?;
            Ok(Instruction::IsErr {
                value: Box::new(parse_instruction(v)?),
            })
        }
        "string.len" => {
            let v = kids.get(1).ok_or("string.len value")?;
            Ok(Instruction::StringLen {
                value: Box::new(parse_instruction(v)?),
            })
        }
        "string.literal" => {
            let s = kids.get(1).ok_or("string.literal needs string")?;
            match s {
                Sexp::Str(b) => Ok(Instruction::StringLiteral { bytes: b.clone() }),
                _ => Err("string.literal arg must be string".into()),
            }
        }
        "list.len" => {
            let v = kids.get(1).ok_or("list.len value")?;
            Ok(Instruction::ListLen {
                value: Box::new(parse_instruction(v)?),
            })
        }
        "record.get" => {
            let f = kids.get(1).and_then(as_atom).ok_or("record.get field")?;
            let v = kids.get(2).ok_or("record.get value")?;
            Ok(Instruction::RecordGet {
                value: Box::new(parse_instruction(v)?),
                field: strip_dollar(f).to_string(),
            })
        }
        "record.literal" => {
            // Walk original list (with comments) to recover field names.
            let mut fields: Vec<(String, Instruction)> = Vec::new();
            let mut pending: Option<String> = None;
            for item in &list[1..] {
                match item {
                    Sexp::Comment(c) => {
                        pending = Some(strip_dollar(c.trim()).to_string());
                    }
                    Sexp::List(_) => {
                        let name = pending.take().unwrap_or_default();
                        fields.push((name, parse_instruction(item)?));
                    }
                    _ => {}
                }
            }
            Ok(Instruction::RecordLiteral { fields })
        }
        "variant.case" => {
            let c = kids.get(1).and_then(as_atom).ok_or("variant.case name")?;
            let value = match kids.get(2) {
                Some(v) => Some(Box::new(parse_instruction(v)?)),
                None => None,
            };
            Ok(Instruction::VariantCtor {
                case: strip_dollar(c).to_string(),
                value,
            })
        }
        "match_variant" => {
            let v = kids.get(1).ok_or("match_variant value")?;
            let value = Box::new(parse_instruction(v)?);
            let mut arms = Vec::new();
            for item in &kids[2..] {
                let il = as_list(item).ok_or("arm must be list")?;
                let ikids = drop_comments(il);
                if ikids.first().and_then(as_atom) != Some("case") {
                    return Err("expected (case ...) arm".into());
                }
                let case_name = ikids.get(1).and_then(as_atom).ok_or("case name")?;
                let mut idx = 2;
                let binding = match ikids.get(idx).and_then(as_atom) {
                    Some(a) if a.starts_with('$') => {
                        idx += 1;
                        Some(a.trim_start_matches('$').to_string())
                    }
                    _ => None,
                };
                let mut body = Vec::new();
                for b in &ikids[idx..] {
                    if matches!(b, Sexp::List(_)) {
                        body.push(parse_instruction(b)?);
                    }
                }
                arms.push(wast_pattern_analyzer::MatchArm {
                    case: strip_dollar(case_name).to_string(),
                    binding,
                    body,
                });
            }
            Ok(Instruction::MatchVariant { value, arms })
        }
        "tuple.get" => {
            let n = kids.get(1).and_then(as_atom).ok_or("tuple.get index")?;
            let idx: u32 = n.parse().map_err(|e| format!("tuple.get: {e}"))?;
            let v = kids.get(2).ok_or("tuple.get value")?;
            Ok(Instruction::TupleGet {
                value: Box::new(parse_instruction(v)?),
                index: idx,
            })
        }
        "tuple.literal" => {
            let mut values = Vec::new();
            for item in &kids[1..] {
                if matches!(item, Sexp::List(_)) {
                    values.push(parse_instruction(item)?);
                }
            }
            Ok(Instruction::TupleLiteral { values })
        }
        "list.literal" => {
            let mut values = Vec::new();
            for item in &kids[1..] {
                if matches!(item, Sexp::List(_)) {
                    values.push(parse_instruction(item)?);
                }
            }
            Ok(Instruction::ListLiteral { values })
        }
        "flags.ctor" => {
            let mut flags = Vec::new();
            for item in &kids[1..] {
                if let Some(a) = as_atom(item) {
                    flags.push(strip_dollar(a).to_string());
                }
            }
            Ok(Instruction::FlagsCtor { flags })
        }
        "resource.new" => {
            let r = kids.get(1).and_then(as_atom).ok_or("resource.new")?;
            let v = kids.get(2).ok_or("resource.new value")?;
            Ok(Instruction::ResourceNew {
                resource: strip_dollar(r).to_string(),
                rep: Box::new(parse_instruction(v)?),
            })
        }
        "resource.rep" => {
            let r = kids.get(1).and_then(as_atom).ok_or("resource.rep")?;
            let v = kids.get(2).ok_or("resource.rep handle")?;
            Ok(Instruction::ResourceRep {
                resource: strip_dollar(r).to_string(),
                handle: Box::new(parse_instruction(v)?),
            })
        }
        "resource.drop" => {
            let r = kids.get(1).and_then(as_atom).ok_or("resource.drop")?;
            let v = kids.get(2).ok_or("resource.drop handle")?;
            Ok(Instruction::ResourceDrop {
                resource: strip_dollar(r).to_string(),
                handle: Box::new(parse_instruction(v)?),
            })
        }
        "match_option" => {
            let v = kids.get(1).ok_or("match_option value")?;
            let value = Box::new(parse_instruction(v)?);
            let mut some_binding = String::new();
            let mut some_body = Vec::new();
            let mut none_body = Vec::new();
            for item in &kids[2..] {
                let il = as_list(item).ok_or("match_option arm must be list")?;
                let ikids = drop_comments(il);
                let h = ikids.first().and_then(as_atom).unwrap_or("");
                match h {
                    "some" => {
                        let b = ikids.get(1).and_then(as_atom).ok_or("some binding")?;
                        some_binding = strip_dollar(b).to_string();
                        for x in &ikids[2..] {
                            if matches!(x, Sexp::List(_)) {
                                some_body.push(parse_instruction(x)?);
                            }
                        }
                    }
                    "none" => {
                        for x in &ikids[1..] {
                            if matches!(x, Sexp::List(_)) {
                                none_body.push(parse_instruction(x)?);
                            }
                        }
                    }
                    _ => return Err(format!("unexpected match_option arm: {h}")),
                }
            }
            Ok(Instruction::MatchOption {
                value,
                some_binding,
                some_body,
                none_body,
            })
        }
        "match_result" => {
            let v = kids.get(1).ok_or("match_result value")?;
            let value = Box::new(parse_instruction(v)?);
            let mut ok_binding = String::new();
            let mut err_binding = String::new();
            let mut ok_body = Vec::new();
            let mut err_body = Vec::new();
            for item in &kids[2..] {
                let il = as_list(item).ok_or("match_result arm must be list")?;
                let ikids = drop_comments(il);
                let h = ikids.first().and_then(as_atom).unwrap_or("");
                match h {
                    "ok" => {
                        let b = ikids.get(1).and_then(as_atom).ok_or("ok binding")?;
                        ok_binding = strip_dollar(b).to_string();
                        for x in &ikids[2..] {
                            if matches!(x, Sexp::List(_)) {
                                ok_body.push(parse_instruction(x)?);
                            }
                        }
                    }
                    "err" => {
                        let b = ikids.get(1).and_then(as_atom).ok_or("err binding")?;
                        err_binding = strip_dollar(b).to_string();
                        for x in &ikids[2..] {
                            if matches!(x, Sexp::List(_)) {
                                err_body.push(parse_instruction(x)?);
                            }
                        }
                    }
                    _ => return Err(format!("unexpected match_result arm: {h}")),
                }
            }
            Ok(Instruction::MatchResult {
                value,
                ok_binding,
                ok_body,
                err_binding,
                err_body,
            })
        }
        other => Err(format!("unknown instruction: {other}")),
    }
}

fn parse_syms_form(items: &[Sexp]) -> Result<Syms, String> {
    let kids = drop_comments(items);
    if kids.first().and_then(as_atom) != Some("syms") {
        return Err("expected (syms ...)".into());
    }
    let mut wit_syms = Vec::new();
    let mut internal = Vec::new();
    let mut local = Vec::new();
    for child in &kids[1..] {
        let l = as_list(child).ok_or("syms section must be list")?;
        let lkids = drop_comments(l);
        let head = lkids.first().and_then(as_atom).unwrap_or("");
        let mut entries: Vec<(String, String)> = Vec::new();
        for sym in &lkids[1..] {
            let sl = as_list(sym).ok_or("sym must be list")?;
            let skids = drop_comments(sl);
            if skids.first().and_then(as_atom) != Some("sym") {
                return Err("expected (sym ...)".into());
            }
            let uid = match skids.get(1) {
                Some(Sexp::Str(b)) => {
                    String::from_utf8(b.clone()).map_err(|e| format!("sym uid utf-8: {e}"))?
                }
                _ => return Err("sym uid must be string".into()),
            };
            let name = match skids.get(2) {
                Some(Sexp::Str(b)) => {
                    String::from_utf8(b.clone()).map_err(|e| format!("sym name utf-8: {e}"))?
                }
                _ => return Err("sym name must be string".into()),
            };
            entries.push((uid, name));
        }
        match head {
            "wit" => wit_syms = entries,
            "internal" => {
                internal = entries
                    .into_iter()
                    .map(|(uid, display_name)| SymEntry { uid, display_name })
                    .collect();
            }
            "local" => {
                local = entries
                    .into_iter()
                    .map(|(uid, display_name)| SymEntry { uid, display_name })
                    .collect();
            }
            _ => return Err(format!("unknown syms section: {head}")),
        }
    }
    Ok(Syms {
        wit_syms,
        internal,
        local,
    })
}

fn from_text_inner(text: &str, existing: &WastComponent) -> Result<WastComponent, String> {
    let tokens = tokenize(text)?;
    let sexps = parse_sexps(&tokens)?;

    let component_items = sexps
        .iter()
        .find_map(|s| match s {
            Sexp::List(items) => match items.first() {
                Some(Sexp::Atom(a)) if a == "component" => Some(items.as_slice()),
                _ => None,
            },
            _ => None,
        })
        .ok_or_else(|| "missing (component ...) form".to_string())?;

    let mut type_forms: Vec<&[Sexp]> = Vec::new();
    let mut func_forms: Vec<&[Sexp]> = Vec::new();
    let mut syms_form: Option<&[Sexp]> = None;

    for child in &component_items[1..] {
        let l = match child {
            Sexp::List(l) => l,
            _ => continue,
        };
        match l.first().and_then(as_atom).unwrap_or("") {
            "type" => type_forms.push(l.as_slice()),
            "func" => func_forms.push(l.as_slice()),
            "syms" => syms_form = Some(l.as_slice()),
            _ => {}
        }
    }

    // Build the type lookup from the existing component. For a no-op round
    // trip the existing types match the rendered text exactly, so every
    // inline form maps back to its uid. For genuinely new types added by
    // the user, falls back to keeping the inline rendering as the ref.
    let lookup = build_type_lookup(&existing.types);

    let mut types: Vec<(TypeUid, WastTypeDef)> = Vec::new();
    for tf in &type_forms {
        types.push(parse_type_form(tf, &lookup)?);
    }

    let mut funcs: Vec<(FuncUid, WastFunc)> = Vec::new();
    for ff in &func_forms {
        funcs.push(parse_func_form(ff, &lookup)?);
    }

    let syms = match syms_form {
        Some(s) => parse_syms_form(s)?,
        None => Syms {
            wit_syms: vec![],
            internal: vec![],
            local: vec![],
        },
    };

    Ok(WastComponent { funcs, types, syms })
}

// ---------------------------------------------------------------------------
// Guest implementation
// ---------------------------------------------------------------------------

impl bindings::exports::wast::core::syntax_plugin::Guest for Component {
    fn to_text(component: WastComponent) -> String {
        let mut parts: Vec<String> = Vec::new();

        parts.push("(component".to_string());

        // Types
        for (type_uid, typedef) in &component.types {
            parts.push(type_to_text(type_uid, typedef, &component.types));
        }

        // Funcs
        for (func_uid, func) in &component.funcs {
            parts.push(func_to_text(func_uid, func, &component.types));
        }

        // Syms
        parts.push(syms_to_text(&component.syms));

        parts.push(")".to_string());
        parts.join("\n")
    }

    fn from_text(text: String, existing: WastComponent) -> Result<WastComponent, Vec<WastError>> {
        from_text_inner(&text, &existing).map_err(|e| {
            vec![WastError {
                message: e,
                location: None,
            }]
        })
    }
}

bindings::export!(Component with_types_in bindings);

#[cfg(test)]
mod tests {
    use super::*;
    use bindings::exports::wast::core::syntax_plugin::Guest;

    fn make_empty_syms() -> Syms {
        Syms {
            wit_syms: vec![],
            internal: vec![],
            local: vec![],
        }
    }

    #[test]
    fn test_empty_component() {
        let comp = WastComponent {
            funcs: vec![],
            types: vec![],
            syms: make_empty_syms(),
        };
        let text = Component::to_text(comp);
        assert!(text.starts_with("(component"));
        assert!(text.ends_with(')'));
    }

    #[test]
    fn test_primitive_types() {
        assert_eq!(primitive_name(&PrimitiveType::U32), "u32");
        assert_eq!(primitive_name(&PrimitiveType::String), "string");
        assert_eq!(primitive_name(&PrimitiveType::Bool), "bool");
    }

    #[test]
    fn test_render_type_ref_inline() {
        let types = vec![(
            "tid1".to_string(),
            WastTypeDef {
                source: TypeSource::Internal("tid1".to_string()),
                definition: WitType::Primitive(PrimitiveType::U32),
            },
        )];
        assert_eq!(render_type_ref(&"tid1".to_string(), &types), "u32");
    }

    #[test]
    fn test_render_type_ref_not_found() {
        let types: Vec<(TypeUid, WastTypeDef)> = vec![];
        assert_eq!(
            render_type_ref(&"unknown_tid".to_string(), &types),
            "$unknown_tid"
        );
    }

    #[test]
    fn test_func_imported() {
        let types = vec![(
            "tid_str".to_string(),
            WastTypeDef {
                source: TypeSource::Imported("tid_str".to_string()),
                definition: WitType::Primitive(PrimitiveType::String),
            },
        )];
        let func = WastFunc {
            source: FuncSource::Imported("log".to_string()),
            params: vec![("p1".to_string(), "tid_str".to_string())],
            result: None,
            body: None,
        };
        let text = func_to_text("f1", &func, &types);
        assert!(text.contains("(func $f1 (import $log)"));
        assert!(text.contains("(param $p1 string)"));
        assert!(!text.contains("(result"));
    }

    #[test]
    fn test_func_exported_with_body() {
        let types = vec![(
            "tid_u32".to_string(),
            WastTypeDef {
                source: TypeSource::Internal("tid_u32".to_string()),
                definition: WitType::Primitive(PrimitiveType::U32),
            },
        )];
        let body = wast_pattern_analyzer::serialize_body(&[
            Instruction::LocalGet {
                uid: "p1".to_string(),
            },
            Instruction::Return,
        ]);
        let func = WastFunc {
            source: FuncSource::Exported("handle".to_string()),
            params: vec![("p1".to_string(), "tid_u32".to_string())],
            result: Some("tid_u32".to_string()),
            body: Some(body),
        };
        let text = func_to_text("f2", &func, &types);
        assert!(text.contains("(func $f2 (export $handle)"));
        assert!(text.contains("(param $p1 u32)"));
        assert!(text.contains("(result u32)"));
        assert!(text.contains("(local.get $p1)"));
        assert!(text.contains("(return)"));
    }

    #[test]
    fn test_render_option_type() {
        let types = vec![(
            "tid_u32".to_string(),
            WastTypeDef {
                source: TypeSource::Internal("tid_u32".to_string()),
                definition: WitType::Primitive(PrimitiveType::U32),
            },
        )];
        let wt = WitType::Option("tid_u32".to_string());
        assert_eq!(render_wit_type(&wt, &types), "(option u32)");
    }

    #[test]
    fn test_render_result_type() {
        let types = vec![
            (
                "tid_u32".to_string(),
                WastTypeDef {
                    source: TypeSource::Internal("tid_u32".to_string()),
                    definition: WitType::Primitive(PrimitiveType::U32),
                },
            ),
            (
                "tid_str".to_string(),
                WastTypeDef {
                    source: TypeSource::Internal("tid_str".to_string()),
                    definition: WitType::Primitive(PrimitiveType::String),
                },
            ),
        ];
        let wt = WitType::Result(("tid_u32".to_string(), "tid_str".to_string()));
        assert_eq!(render_wit_type(&wt, &types), "(result u32 string)");
    }

    #[test]
    fn test_render_list_type() {
        let types = vec![(
            "tid_i64".to_string(),
            WastTypeDef {
                source: TypeSource::Internal("tid_i64".to_string()),
                definition: WitType::Primitive(PrimitiveType::I64),
            },
        )];
        let wt = WitType::List("tid_i64".to_string());
        assert_eq!(render_wit_type(&wt, &types), "(list i64)");
    }

    #[test]
    fn test_render_record_type() {
        let types = vec![(
            "tid_u32".to_string(),
            WastTypeDef {
                source: TypeSource::Internal("tid_u32".to_string()),
                definition: WitType::Primitive(PrimitiveType::U32),
            },
        )];
        let wt = WitType::Record(vec![("x".to_string(), "tid_u32".to_string())]);
        assert_eq!(render_wit_type(&wt, &types), "(record (field $x u32))");
    }

    #[test]
    fn test_instructions_block_loop() {
        let body = wast_pattern_analyzer::serialize_body(&[
            Instruction::Block {
                label: Some("blk".to_string()),
                body: vec![Instruction::Nop],
            },
            Instruction::Loop {
                label: Some("lp".to_string()),
                body: vec![
                    Instruction::BrIf {
                        label: "lp".to_string(),
                        condition: Box::new(Instruction::LocalGet {
                            uid: "flag".to_string(),
                        }),
                    },
                    Instruction::Br {
                        label: "blk".to_string(),
                    },
                ],
            },
        ]);
        let text = render_body(&body, "  ");
        assert!(text.contains("(block $blk"));
        assert!(text.contains("(loop $lp"));
        assert!(text.contains("(br_if $lp"));
        assert!(text.contains("(br $blk)"));
    }

    #[test]
    fn test_instructions_if_else() {
        let body = wast_pattern_analyzer::serialize_body(&[Instruction::If {
            condition: Box::new(Instruction::Compare {
                op: CompareOp::Eq,
                lhs: Box::new(Instruction::LocalGet {
                    uid: "x".to_string(),
                }),
                rhs: Box::new(Instruction::Const { value: 0 }),
            }),
            then_body: vec![Instruction::Return],
            else_body: vec![Instruction::Nop],
        }]);
        let text = render_body(&body, "  ");
        assert!(text.contains("(if"));
        assert!(text.contains("(i64.eq"));
        assert!(text.contains("(then"));
        assert!(text.contains("(return)"));
        assert!(text.contains("(else"));
    }

    #[test]
    fn test_instructions_call_with_args() {
        let body = wast_pattern_analyzer::serialize_body(&[Instruction::Call {
            func_uid: "add".to_string(),
            args: vec![
                ("a".to_string(), Instruction::Const { value: 1 }),
                ("b".to_string(), Instruction::Const { value: 2 }),
            ],
        }]);
        let text = render_body(&body, "  ");
        assert!(text.contains("(call $add"));
        assert!(text.contains("(; $a ;)"));
        assert!(text.contains("(i64.const 1)"));
        assert!(text.contains("(; $b ;)"));
        assert!(text.contains("(i64.const 2)"));
    }

    #[test]
    fn test_instructions_arithmetic() {
        let body = wast_pattern_analyzer::serialize_body(&[Instruction::Arithmetic {
            op: ArithOp::Add,
            lhs: Box::new(Instruction::LocalGet {
                uid: "x".to_string(),
            }),
            rhs: Box::new(Instruction::Const { value: 1 }),
        }]);
        let text = render_body(&body, "  ");
        assert!(text.contains("(i64.add"));
        assert!(text.contains("(local.get $x)"));
        assert!(text.contains("(i64.const 1)"));
    }

    #[test]
    fn test_instructions_match_option() {
        let body = wast_pattern_analyzer::serialize_body(&[Instruction::MatchOption {
            value: Box::new(Instruction::LocalGet {
                uid: "opt".to_string(),
            }),
            some_binding: "val".to_string(),
            some_body: vec![Instruction::Return],
            none_body: vec![Instruction::Nop],
        }]);
        let text = render_body(&body, "  ");
        assert!(text.contains("(match_option"));
        assert!(text.contains("(some $val"));
        assert!(text.contains("(none"));
    }

    #[test]
    fn test_instructions_match_result() {
        let body = wast_pattern_analyzer::serialize_body(&[Instruction::MatchResult {
            value: Box::new(Instruction::LocalGet {
                uid: "res".to_string(),
            }),
            ok_binding: "v".to_string(),
            ok_body: vec![Instruction::Return],
            err_binding: "e".to_string(),
            err_body: vec![Instruction::Nop],
        }]);
        let text = render_body(&body, "  ");
        assert!(text.contains("(match_result"));
        assert!(text.contains("(ok $v"));
        assert!(text.contains("(err $e"));
    }

    #[test]
    fn test_instructions_some_none_ok_err() {
        let body = wast_pattern_analyzer::serialize_body(&[
            Instruction::Some {
                value: Box::new(Instruction::Const { value: 42 }),
            },
            Instruction::None,
            Instruction::Ok {
                value: Box::new(Instruction::Const { value: 1 }),
            },
            Instruction::Err {
                value: Box::new(Instruction::Const { value: 0 }),
            },
            Instruction::IsErr {
                value: Box::new(Instruction::LocalGet {
                    uid: "r".to_string(),
                }),
            },
        ]);
        let text = render_body(&body, "  ");
        assert!(text.contains("(some"));
        assert!(text.contains("(i64.const 42)"));
        assert!(text.contains("(none)"));
        assert!(text.contains("(ok"));
        assert!(text.contains("(err"));
        assert!(text.contains("(is_err"));
    }

    #[test]
    fn test_syms_rendering() {
        let syms = Syms {
            wit_syms: vec![("wasi:http/handler".to_string(), "handler".to_string())],
            internal: vec![SymEntry {
                uid: "f1".to_string(),
                display_name: "myFunc".to_string(),
            }],
            local: vec![SymEntry {
                uid: "p1".to_string(),
                display_name: "count".to_string(),
            }],
        };
        let text = syms_to_text(&syms);
        assert!(text.contains("(syms"));
        assert!(text.contains("(wit"));
        assert!(text.contains("(sym \"wasi:http/handler\" \"handler\")"));
        assert!(text.contains("(internal"));
        assert!(text.contains("(sym \"f1\" \"myFunc\")"));
        assert!(text.contains("(local"));
        assert!(text.contains("(sym \"p1\" \"count\")"));
    }

    #[test]
    fn test_type_definition_rendering() {
        let types = vec![(
            "tid1".to_string(),
            WastTypeDef {
                source: TypeSource::Exported("my-record".to_string()),
                definition: WitType::Record(vec![
                    ("x".to_string(), "tid1".to_string()), // self-referencing won't happen in practice
                ]),
            },
        )];
        let text = type_to_text("tid1", &types[0].1, &types);
        assert!(text.contains("(type $tid1 (export $my-record)"));
    }

    #[test]
    fn test_full_component_roundtrip() {
        let types = vec![(
            "tid_u32".to_string(),
            WastTypeDef {
                source: TypeSource::Internal("tid_u32".to_string()),
                definition: WitType::Primitive(PrimitiveType::U32),
            },
        )];
        let body = wast_pattern_analyzer::serialize_body(&[
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
            Instruction::Return,
        ]);
        let comp = WastComponent {
            funcs: vec![(
                "f1".to_string(),
                WastFunc {
                    source: FuncSource::Exported("incr".to_string()),
                    params: vec![("p1".to_string(), "tid_u32".to_string())],
                    result: Some("tid_u32".to_string()),
                    body: Some(body),
                },
            )],
            types,
            syms: Syms {
                wit_syms: vec![],
                internal: vec![SymEntry {
                    uid: "f1".to_string(),
                    display_name: "incr".to_string(),
                }],
                local: vec![SymEntry {
                    uid: "p1".to_string(),
                    display_name: "n".to_string(),
                }],
            },
        };
        let text = Component::to_text(comp);
        // Should contain component wrapper
        assert!(text.starts_with("(component"));
        assert!(text.ends_with(')'));
        // Should contain type, func, and syms
        assert!(text.contains("(type $tid_u32"));
        assert!(text.contains("(func $f1 (export $incr)"));
        assert!(text.contains("(local.set $p1"));
        assert!(text.contains("(i64.add"));
        assert!(text.contains("(return)"));
        assert!(text.contains("(syms"));
    }

    #[test]
    fn test_from_text_empty_component() {
        let comp = WastComponent {
            funcs: vec![],
            types: vec![],
            syms: make_empty_syms(),
        };
        let text = Component::to_text(comp.clone());
        let result = Component::from_text(text, comp).unwrap();
        assert!(result.funcs.is_empty());
        assert!(result.types.is_empty());
    }

    #[test]
    fn test_from_text_invalid_returns_error() {
        let comp = WastComponent {
            funcs: vec![],
            types: vec![],
            syms: make_empty_syms(),
        };
        let result = Component::from_text("not a component".to_string(), comp);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_text_simple_func_roundtrip() {
        let types = vec![(
            "tid_u32".to_string(),
            WastTypeDef {
                source: TypeSource::Internal("tid_u32".to_string()),
                definition: WitType::Primitive(PrimitiveType::U32),
            },
        )];
        let body = wast_pattern_analyzer::serialize_body(&[
            Instruction::LocalGet {
                uid: "x".to_string(),
            },
            Instruction::Return,
        ]);
        let comp = WastComponent {
            funcs: vec![(
                "f1".to_string(),
                WastFunc {
                    source: FuncSource::Exported("f1".to_string()),
                    params: vec![("x".to_string(), "tid_u32".to_string())],
                    result: Some("tid_u32".to_string()),
                    body: Some(body),
                },
            )],
            types,
            syms: make_empty_syms(),
        };
        let text = Component::to_text(comp.clone());
        let parsed = Component::from_text(text, comp.clone()).unwrap();
        assert_eq!(parsed.funcs.len(), 1);
        assert_eq!(parsed.funcs[0].0, "f1");
        assert_eq!(parsed.funcs[0].1.params.len(), 1);
        assert_eq!(parsed.funcs[0].1.params[0].0, "x");
        assert_eq!(parsed.funcs[0].1.params[0].1, "tid_u32");
        assert_eq!(parsed.funcs[0].1.result.as_deref(), Some("tid_u32"));
        assert_eq!(parsed.types.len(), 1);
        assert_eq!(parsed.types[0].0, "tid_u32");
    }

    #[test]
    fn test_from_text_complex_roundtrip() {
        let types = vec![
            (
                "tid_u32".to_string(),
                WastTypeDef {
                    source: TypeSource::Internal("tid_u32".to_string()),
                    definition: WitType::Primitive(PrimitiveType::U32),
                },
            ),
            (
                "point".to_string(),
                WastTypeDef {
                    source: TypeSource::Internal("point".to_string()),
                    definition: WitType::Record(vec![
                        ("x".to_string(), "tid_u32".to_string()),
                        ("y".to_string(), "tid_u32".to_string()),
                    ]),
                },
            ),
            (
                "opt_u32".to_string(),
                WastTypeDef {
                    source: TypeSource::Internal("opt_u32".to_string()),
                    definition: WitType::Option("tid_u32".to_string()),
                },
            ),
        ];
        let body = wast_pattern_analyzer::serialize_body(&[Instruction::Call {
            func_uid: "make_point".to_string(),
            args: vec![
                ("x".to_string(), Instruction::LocalGet { uid: "x".into() }),
                ("y".to_string(), Instruction::LocalGet { uid: "y".into() }),
            ],
        }]);
        let comp = WastComponent {
            funcs: vec![
                (
                    "make_point".to_string(),
                    WastFunc {
                        source: FuncSource::Exported("make-point".into()),
                        params: vec![
                            ("x".into(), "tid_u32".into()),
                            ("y".into(), "tid_u32".into()),
                        ],
                        result: Some("point".into()),
                        body: None,
                    },
                ),
                (
                    "use_pt".to_string(),
                    WastFunc {
                        source: FuncSource::Exported("use-pt".into()),
                        params: vec![
                            ("x".into(), "tid_u32".into()),
                            ("y".into(), "tid_u32".into()),
                        ],
                        result: Some("point".into()),
                        body: Some(body),
                    },
                ),
            ],
            types,
            syms: make_empty_syms(),
        };
        let text = Component::to_text(comp.clone());
        let parsed = Component::from_text(text, comp.clone()).unwrap();
        // Type refs should resolve back to their uids, not inline forms.
        assert_eq!(parsed.funcs[0].1.result.as_deref(), Some("point"));
        assert_eq!(parsed.types[1].0, "point");
        if let WitType::Record(fields) = &parsed.types[1].1.definition {
            assert_eq!(fields[0].0, "x");
            assert_eq!(fields[0].1, "tid_u32");
        } else {
            panic!("expected record");
        }
        // Body call args preserved.
        let body = parsed.funcs[1].1.body.as_ref().unwrap();
        let instrs = wast_pattern_analyzer::deserialize_body(body).unwrap();
        if let Instruction::Call { func_uid, args } = &instrs[0] {
            assert_eq!(func_uid, "make_point");
            assert_eq!(args[0].0, "x");
            assert_eq!(args[1].0, "y");
        } else {
            panic!("expected call");
        }
    }
}
