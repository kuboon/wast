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

    fn from_text(_text: String, _existing: WastComponent) -> Result<WastComponent, Vec<WastError>> {
        // Raw syntax is read-only: return existing unchanged.
        Err(vec![WastError {
            message: "raw syntax plugin is read-only (to_text only)".to_string(),
            location: None,
        }])
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
    fn test_from_text_returns_error() {
        let comp = WastComponent {
            funcs: vec![],
            types: vec![],
            syms: make_empty_syms(),
        };
        let result = Component::from_text("anything".to_string(), comp);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("read-only"));
    }
}
