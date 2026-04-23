use serde::{Deserialize, Serialize};

/// Comparison operators for `Compare` instructions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompareOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Arithmetic operators for `Arithmetic` instructions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// Intermediate representation for wast body instructions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Instruction {
    // Control flow (WAT-inherited)
    Block {
        label: Option<String>,
        body: Vec<Instruction>,
    },
    Loop {
        label: Option<String>,
        body: Vec<Instruction>,
    },
    If {
        condition: Box<Instruction>,
        then_body: Vec<Instruction>,
        else_body: Vec<Instruction>,
    },
    BrIf {
        label: String,
        condition: Box<Instruction>,
    },
    Br {
        label: String,
    },
    Return,

    // Function calls
    Call {
        func_uid: String,
        args: Vec<(String, Instruction)>,
    },

    // Variables
    LocalGet {
        uid: String,
    },
    LocalSet {
        uid: String,
        value: Box<Instruction>,
    },

    // Constants
    Const {
        value: i64,
    },

    // Comparison
    Compare {
        op: CompareOp,
        lhs: Box<Instruction>,
        rhs: Box<Instruction>,
    },

    // Arithmetic
    Arithmetic {
        op: ArithOp,
        lhs: Box<Instruction>,
        rhs: Box<Instruction>,
    },

    // WIT type operations (WAST extensions)
    Some {
        value: Box<Instruction>,
    },
    None,
    Ok {
        value: Box<Instruction>,
    },
    Err {
        value: Box<Instruction>,
    },
    MatchOption {
        value: Box<Instruction>,
        some_binding: String,
        some_body: Vec<Instruction>,
        none_body: Vec<Instruction>,
    },
    MatchResult {
        value: Box<Instruction>,
        ok_binding: String,
        ok_body: Vec<Instruction>,
        err_binding: String,
        err_body: Vec<Instruction>,
    },
    IsErr {
        value: Box<Instruction>,
    },

    // String operations
    StringLen {
        value: Box<Instruction>,
    },

    // Other
    Nop,
}

/// A detected pattern match in the instruction body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatternMatch {
    pub pattern: Pattern,
    /// Index in the body where the pattern starts.
    pub instruction_index: usize,
}

/// Pattern types detected from wast bodies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pattern {
    /// loop + br_if with head condition -> while
    While,
    /// loop + br_if with counter variable -> for
    For { counter_uid: String },
    /// loop + br_if with list index -> for-in
    ForIn { index_uid: String, list_uid: String },
    /// if (is_err) + return -> try / ?
    Try,
}

/// Analyze a wast function body and detect high-level control flow patterns.
pub fn analyze(body: &[Instruction]) -> Vec<PatternMatch> {
    let mut matches = Vec::new();
    for (index, instr) in body.iter().enumerate() {
        detect_patterns(instr, index, &mut matches);
    }
    matches
}

fn detect_patterns(instr: &Instruction, index: usize, matches: &mut Vec<PatternMatch>) {
    match instr {
        Instruction::Loop { body, .. } => {
            if let Some(pattern) = classify_loop(body) {
                matches.push(PatternMatch {
                    pattern,
                    instruction_index: index,
                });
            }
            // Also recurse into loop body for nested patterns.
            for (i, child) in body.iter().enumerate() {
                detect_patterns(child, i, matches);
            }
        }
        Instruction::If {
            condition,
            then_body,
            else_body,
        } => {
            if is_try_pattern(condition, then_body) {
                matches.push(PatternMatch {
                    pattern: Pattern::Try,
                    instruction_index: index,
                });
            }
            // Recurse into both branches.
            for (i, child) in then_body.iter().enumerate() {
                detect_patterns(child, i, matches);
            }
            for (i, child) in else_body.iter().enumerate() {
                detect_patterns(child, i, matches);
            }
        }
        Instruction::Block { body, .. } => {
            for (i, child) in body.iter().enumerate() {
                detect_patterns(child, i, matches);
            }
        }
        Instruction::MatchOption {
            some_body,
            none_body,
            ..
        } => {
            for (i, child) in some_body.iter().enumerate() {
                detect_patterns(child, i, matches);
            }
            for (i, child) in none_body.iter().enumerate() {
                detect_patterns(child, i, matches);
            }
        }
        Instruction::MatchResult {
            ok_body, err_body, ..
        } => {
            for (i, child) in ok_body.iter().enumerate() {
                detect_patterns(child, i, matches);
            }
            for (i, child) in err_body.iter().enumerate() {
                detect_patterns(child, i, matches);
            }
        }
        _ => {}
    }
}

/// Classify a loop body as While, For, ForIn, or none.
fn classify_loop(body: &[Instruction]) -> Option<Pattern> {
    // The loop must start with a BrIf (condition at top).
    let (condition, _br_label) = match body.first()? {
        Instruction::BrIf { condition, label } => (condition.as_ref(), label),
        _ => return Option::None,
    };

    let rest = &body[1..];

    // Try to detect ForIn: condition compares an index variable against a list
    // length call, and the body increments the index.
    if let Some(pattern) = try_detect_for_in(condition, rest) {
        return Some(pattern);
    }

    // Try to detect For: condition compares a counter to a limit, and the body
    // increments the counter.
    if let Some(pattern) = try_detect_for(condition, rest) {
        return Some(pattern);
    }

    // Fallback: plain while loop.
    Some(Pattern::While)
}

/// Try to detect a `For` pattern: condition is `Compare(counter, limit)` and
/// the body contains `LocalSet(counter, Arithmetic(Add, LocalGet(counter), Const(..)))`.
fn try_detect_for(condition: &Instruction, rest: &[Instruction]) -> Option<Pattern> {
    let counter_uid = extract_counter_from_condition(condition)?;
    if body_increments_variable(rest, &counter_uid) {
        Some(Pattern::For { counter_uid })
    } else {
        Option::None
    }
}

/// Try to detect a `ForIn` pattern: condition is `Compare(index, Call("len", list))`
/// and the body increments the index.
fn try_detect_for_in(condition: &Instruction, rest: &[Instruction]) -> Option<Pattern> {
    let (index_uid, list_uid) = extract_index_and_list_from_condition(condition)?;
    if body_increments_variable(rest, &index_uid) {
        Some(Pattern::ForIn {
            index_uid,
            list_uid,
        })
    } else {
        Option::None
    }
}

/// Extract a counter variable uid from a comparison condition like
/// `Compare(_, LocalGet(uid), ...)`.
fn extract_counter_from_condition(condition: &Instruction) -> Option<String> {
    match condition {
        Instruction::Compare { lhs, .. } => match lhs.as_ref() {
            Instruction::LocalGet { uid } => Some(uid.clone()),
            _ => Option::None,
        },
        _ => Option::None,
    }
}

/// Extract index uid and list uid from a condition like
/// `Compare(Lt, LocalGet(index), Call("len", [(_, LocalGet(list))]))`.
fn extract_index_and_list_from_condition(condition: &Instruction) -> Option<(String, String)> {
    match condition {
        Instruction::Compare { op: _, lhs, rhs } => {
            let index_uid = match lhs.as_ref() {
                Instruction::LocalGet { uid } => uid.clone(),
                _ => return Option::None,
            };
            // rhs should be a Call to a length-like function with a list argument.
            let list_uid = match rhs.as_ref() {
                Instruction::Call { func_uid, args } if is_length_func(func_uid) => {
                    match args.first() {
                        Some((_, Instruction::LocalGet { uid })) => uid.clone(),
                        _ => return Option::None,
                    }
                }
                _ => return Option::None,
            };
            Some((index_uid, list_uid))
        }
        _ => Option::None,
    }
}

/// Check if a function name looks like a length/size function.
fn is_length_func(name: &str) -> bool {
    matches!(name, "len" | "length" | "size" | "count")
}

/// Check if the body contains an increment of the given variable:
/// `LocalSet(uid, Arithmetic(Add, LocalGet(uid), Const(..)))`.
fn body_increments_variable(body: &[Instruction], uid: &str) -> bool {
    body.iter().any(|instr| is_increment(instr, uid))
}

fn is_increment(instr: &Instruction, uid: &str) -> bool {
    match instr {
        Instruction::LocalSet {
            uid: set_uid,
            value,
        } if set_uid == uid => match value.as_ref() {
            Instruction::Arithmetic {
                op: ArithOp::Add,
                lhs,
                rhs,
            } => match (lhs.as_ref(), rhs.as_ref()) {
                (Instruction::LocalGet { uid: get_uid }, Instruction::Const { .. })
                    if get_uid == uid =>
                {
                    true
                }
                (Instruction::Const { .. }, Instruction::LocalGet { uid: get_uid })
                    if get_uid == uid =>
                {
                    true
                }
                _ => false,
            },
            _ => false,
        },
        _ => false,
    }
}

/// Detect a Try pattern: `If { condition: IsErr(expr), then: [Return], .. }`.
fn is_try_pattern(condition: &Instruction, then_body: &[Instruction]) -> bool {
    let is_err_condition = matches!(condition, Instruction::IsErr { .. });
    let then_returns = then_body
        .iter()
        .any(|instr| matches!(instr, Instruction::Return));
    is_err_condition && then_returns
}

/// Serialize a slice of instructions into a compact binary format.
pub fn serialize_body(instructions: &[Instruction]) -> Vec<u8> {
    postcard::to_allocvec(instructions).expect("serialization should not fail")
}

/// Deserialize instructions from the binary format produced by [`serialize_body`].
pub fn deserialize_body(data: &[u8]) -> Result<Vec<Instruction>, String> {
    postcard::from_bytes(data).map_err(|e| format!("deserialization failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_body() {
        let result = analyze(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_no_pattern_plain_instructions() {
        let body = vec![
            Instruction::LocalSet {
                uid: "x".into(),
                value: Box::new(Instruction::Const { value: 42 }),
            },
            Instruction::Nop,
            Instruction::Return,
        ];
        let result = analyze(&body);
        assert!(result.is_empty());
    }

    #[test]
    fn test_while_pattern() {
        // loop { br_if(condition); ...body... }
        let body = vec![Instruction::Loop {
            label: Some("loop0".into()),
            body: vec![
                Instruction::BrIf {
                    label: "loop0".into(),
                    condition: Box::new(Instruction::Compare {
                        op: CompareOp::Ne,
                        lhs: Box::new(Instruction::LocalGet { uid: "done".into() }),
                        rhs: Box::new(Instruction::Const { value: 1 }),
                    }),
                },
                Instruction::Call {
                    func_uid: "do_work".into(),
                    args: vec![],
                },
            ],
        }];
        let result = analyze(&body);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].pattern, Pattern::While);
        assert_eq!(result[0].instruction_index, 0);
    }

    #[test]
    fn test_for_pattern() {
        // loop { br_if(i < 10); ...body...; i = i + 1 }
        let body = vec![Instruction::Loop {
            label: Some("loop0".into()),
            body: vec![
                Instruction::BrIf {
                    label: "loop0".into(),
                    condition: Box::new(Instruction::Compare {
                        op: CompareOp::Lt,
                        lhs: Box::new(Instruction::LocalGet { uid: "i".into() }),
                        rhs: Box::new(Instruction::Const { value: 10 }),
                    }),
                },
                Instruction::Call {
                    func_uid: "process".into(),
                    args: vec![],
                },
                Instruction::LocalSet {
                    uid: "i".into(),
                    value: Box::new(Instruction::Arithmetic {
                        op: ArithOp::Add,
                        lhs: Box::new(Instruction::LocalGet { uid: "i".into() }),
                        rhs: Box::new(Instruction::Const { value: 1 }),
                    }),
                },
            ],
        }];
        let result = analyze(&body);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].pattern,
            Pattern::For {
                counter_uid: "i".into()
            }
        );
        assert_eq!(result[0].instruction_index, 0);
    }

    #[test]
    fn test_for_in_pattern() {
        // loop { br_if(idx < len(items)); ...body...; idx = idx + 1 }
        let body = vec![Instruction::Loop {
            label: Some("loop0".into()),
            body: vec![
                Instruction::BrIf {
                    label: "loop0".into(),
                    condition: Box::new(Instruction::Compare {
                        op: CompareOp::Lt,
                        lhs: Box::new(Instruction::LocalGet { uid: "idx".into() }),
                        rhs: Box::new(Instruction::Call {
                            func_uid: "len".into(),
                            args: vec![(
                                "list".into(),
                                Instruction::LocalGet {
                                    uid: "items".into(),
                                },
                            )],
                        }),
                    }),
                },
                Instruction::Call {
                    func_uid: "use_item".into(),
                    args: vec![],
                },
                Instruction::LocalSet {
                    uid: "idx".into(),
                    value: Box::new(Instruction::Arithmetic {
                        op: ArithOp::Add,
                        lhs: Box::new(Instruction::LocalGet { uid: "idx".into() }),
                        rhs: Box::new(Instruction::Const { value: 1 }),
                    }),
                },
            ],
        }];
        let result = analyze(&body);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].pattern,
            Pattern::ForIn {
                index_uid: "idx".into(),
                list_uid: "items".into(),
            }
        );
    }

    #[test]
    fn test_try_pattern() {
        // if (is_err(result)) { return; } else { use_ok_value; }
        let body = vec![Instruction::If {
            condition: Box::new(Instruction::IsErr {
                value: Box::new(Instruction::LocalGet {
                    uid: "result".into(),
                }),
            }),
            then_body: vec![Instruction::Return],
            else_body: vec![Instruction::Call {
                func_uid: "use_value".into(),
                args: vec![],
            }],
        }];
        let result = analyze(&body);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].pattern, Pattern::Try);
        assert_eq!(result[0].instruction_index, 0);
    }

    #[test]
    fn test_nested_patterns() {
        // A for loop containing a try pattern inside its body.
        let body = vec![Instruction::Loop {
            label: Some("outer".into()),
            body: vec![
                Instruction::BrIf {
                    label: "outer".into(),
                    condition: Box::new(Instruction::Compare {
                        op: CompareOp::Lt,
                        lhs: Box::new(Instruction::LocalGet { uid: "i".into() }),
                        rhs: Box::new(Instruction::Const { value: 5 }),
                    }),
                },
                // Nested try pattern inside the loop body.
                Instruction::If {
                    condition: Box::new(Instruction::IsErr {
                        value: Box::new(Instruction::LocalGet { uid: "res".into() }),
                    }),
                    then_body: vec![Instruction::Return],
                    else_body: vec![Instruction::Nop],
                },
                Instruction::LocalSet {
                    uid: "i".into(),
                    value: Box::new(Instruction::Arithmetic {
                        op: ArithOp::Add,
                        lhs: Box::new(Instruction::LocalGet { uid: "i".into() }),
                        rhs: Box::new(Instruction::Const { value: 1 }),
                    }),
                },
            ],
        }];
        let result = analyze(&body);
        // Should detect: For pattern (outer loop) and Try pattern (nested if).
        assert_eq!(result.len(), 2);
        assert_eq!(
            result[0].pattern,
            Pattern::For {
                counter_uid: "i".into()
            }
        );
        assert_eq!(result[1].pattern, Pattern::Try);
    }

    #[test]
    fn test_loop_without_br_if_no_pattern() {
        // A loop that doesn't start with br_if should not match any loop pattern.
        let body = vec![Instruction::Loop {
            label: Some("loop0".into()),
            body: vec![
                Instruction::Call {
                    func_uid: "work".into(),
                    args: vec![],
                },
                Instruction::Br {
                    label: "loop0".into(),
                },
            ],
        }];
        let result = analyze(&body);
        assert!(result.is_empty());
    }

    #[test]
    fn test_if_without_is_err_no_try() {
        // An if whose condition is not IsErr should not be detected as Try.
        let body = vec![Instruction::If {
            condition: Box::new(Instruction::Compare {
                op: CompareOp::Eq,
                lhs: Box::new(Instruction::LocalGet { uid: "x".into() }),
                rhs: Box::new(Instruction::Const { value: 0 }),
            }),
            then_body: vec![Instruction::Return],
            else_body: vec![Instruction::Nop],
        }];
        let result = analyze(&body);
        assert!(result.is_empty());
    }

    #[test]
    fn test_while_pattern_no_counter_increment() {
        // A loop with br_if comparing a variable to a limit but without
        // incrementing that variable should be detected as While, not For.
        let body = vec![Instruction::Loop {
            label: Some("loop0".into()),
            body: vec![
                Instruction::BrIf {
                    label: "loop0".into(),
                    condition: Box::new(Instruction::Compare {
                        op: CompareOp::Lt,
                        lhs: Box::new(Instruction::LocalGet { uid: "i".into() }),
                        rhs: Box::new(Instruction::Const { value: 10 }),
                    }),
                },
                Instruction::Call {
                    func_uid: "work".into(),
                    args: vec![],
                },
            ],
        }];
        let result = analyze(&body);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].pattern, Pattern::While);
    }

    #[test]
    fn test_multiple_top_level_patterns() {
        let body = vec![
            Instruction::If {
                condition: Box::new(Instruction::IsErr {
                    value: Box::new(Instruction::LocalGet { uid: "a".into() }),
                }),
                then_body: vec![Instruction::Return],
                else_body: vec![],
            },
            Instruction::Loop {
                label: Some("l".into()),
                body: vec![
                    Instruction::BrIf {
                        label: "l".into(),
                        condition: Box::new(Instruction::LocalGet { uid: "flag".into() }),
                    },
                    Instruction::Nop,
                ],
            },
        ];
        let result = analyze(&body);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].pattern, Pattern::Try);
        assert_eq!(result[0].instruction_index, 0);
        assert_eq!(result[1].pattern, Pattern::While);
        assert_eq!(result[1].instruction_index, 1);
    }

    #[test]
    fn test_serialize_roundtrip_empty() {
        let body: Vec<Instruction> = vec![];
        let bytes = serialize_body(&body);
        let restored = deserialize_body(&bytes).unwrap();
        assert_eq!(body, restored);
    }

    #[test]
    fn test_serialize_roundtrip_simple_instructions() {
        let body = vec![
            Instruction::Nop,
            Instruction::Return,
            Instruction::Const { value: 42 },
            Instruction::LocalGet { uid: "x".into() },
            Instruction::LocalSet {
                uid: "y".into(),
                value: Box::new(Instruction::Const { value: -7 }),
            },
        ];
        let bytes = serialize_body(&body);
        let restored = deserialize_body(&bytes).unwrap();
        assert_eq!(body, restored);
    }

    #[test]
    fn test_serialize_roundtrip_nested() {
        let body = vec![
            Instruction::Loop {
                label: Some("loop0".into()),
                body: vec![
                    Instruction::BrIf {
                        label: "loop0".into(),
                        condition: Box::new(Instruction::Compare {
                            op: CompareOp::Lt,
                            lhs: Box::new(Instruction::LocalGet { uid: "i".into() }),
                            rhs: Box::new(Instruction::Const { value: 10 }),
                        }),
                    },
                    Instruction::LocalSet {
                        uid: "i".into(),
                        value: Box::new(Instruction::Arithmetic {
                            op: ArithOp::Add,
                            lhs: Box::new(Instruction::LocalGet { uid: "i".into() }),
                            rhs: Box::new(Instruction::Const { value: 1 }),
                        }),
                    },
                ],
            },
            Instruction::If {
                condition: Box::new(Instruction::IsErr {
                    value: Box::new(Instruction::LocalGet { uid: "res".into() }),
                }),
                then_body: vec![Instruction::Return],
                else_body: vec![Instruction::Nop],
            },
        ];
        let bytes = serialize_body(&body);
        let restored = deserialize_body(&bytes).unwrap();
        assert_eq!(body, restored);
    }

    #[test]
    fn test_serialize_roundtrip_wit_types() {
        let body = vec![
            Instruction::Some {
                value: Box::new(Instruction::Const { value: 1 }),
            },
            Instruction::None,
            Instruction::Ok {
                value: Box::new(Instruction::Const { value: 2 }),
            },
            Instruction::Err {
                value: Box::new(Instruction::Const { value: 3 }),
            },
            Instruction::MatchOption {
                value: Box::new(Instruction::LocalGet { uid: "opt".into() }),
                some_binding: "val".into(),
                some_body: vec![Instruction::Return],
                none_body: vec![Instruction::Nop],
            },
            Instruction::MatchResult {
                value: Box::new(Instruction::LocalGet { uid: "res".into() }),
                ok_binding: "ok_val".into(),
                ok_body: vec![Instruction::Return],
                err_binding: "err_val".into(),
                err_body: vec![Instruction::Nop],
            },
        ];
        let bytes = serialize_body(&body);
        let restored = deserialize_body(&bytes).unwrap();
        assert_eq!(body, restored);
    }

    #[test]
    fn test_serialize_roundtrip_call_with_args() {
        let body = vec![Instruction::Call {
            func_uid: "my_func".into(),
            args: vec![
                ("a".into(), Instruction::Const { value: 1 }),
                ("b".into(), Instruction::LocalGet { uid: "x".into() }),
            ],
        }];
        let bytes = serialize_body(&body);
        let restored = deserialize_body(&bytes).unwrap();
        assert_eq!(body, restored);
    }

    #[test]
    fn test_deserialize_invalid_data() {
        let result = deserialize_body(&[0xFF, 0xFF, 0xFF]);
        assert!(result.is_err());
    }
}
