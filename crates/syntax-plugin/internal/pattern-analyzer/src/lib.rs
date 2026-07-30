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
///
/// # Serialization compatibility
///
/// Serialized bodies (see [`serialize_body`] / [`deserialize_body`]) encode
/// each variant by its **positional index** (postcard). Therefore variants in
/// this enum may only ever be APPENDED at the end — never inserted in the
/// middle, reordered, or removed — or every previously persisted body would
/// silently re-interpret as the wrong instructions. The golden-bytes test
/// `test_golden_serialized_bytes` pins the current layout; if it fails after
/// you touched this enum, you broke the on-disk format.
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
    StringLiteral {
        /// UTF-8 bytes of the literal. Serializes as a plain string in
        /// human-readable formats (JSON — see [`body_to_json`]) and as raw
        /// bytes in binary ones (postcard — the on-disk body format), so the
        /// JSON write path reads `"bytes": "hello"` without changing a
        /// single persisted byte.
        #[serde(with = "string_bytes")]
        bytes: Vec<u8>,
    },
    StringLen {
        value: Box<Instruction>,
    },

    // List operations
    ListLen {
        value: Box<Instruction>,
    },
    ListLiteral {
        values: Vec<Instruction>,
    },

    // Record operations
    RecordGet {
        value: Box<Instruction>,
        field: String,
    },
    RecordLiteral {
        fields: Vec<(String, Instruction)>,
    },

    // General variant operations (option/result are their own dedicated IR
    // nodes; these handle user-declared variant types).
    VariantCtor {
        case: String,
        value: Option<Box<Instruction>>,
    },
    MatchVariant {
        value: Box<Instruction>,
        arms: Vec<MatchArm>,
    },

    // Tuple operations (WIT `tuple<T1, T2, …>` — anonymous record with
    // positional indices).
    TupleGet {
        value: Box<Instruction>,
        index: u32,
    },
    TupleLiteral {
        values: Vec<Instruction>,
    },

    // Flags operations (WIT `flags name { a, b, c }`). Stored as a bitmask
    // in an i32/i64 depending on flag count.
    FlagsCtor {
        flags: Vec<String>,
    },

    // Resource operations (WIT `resource R { … }`). Handles are i32 at the
    // core ABI; these IR nodes compile to calls to the canonical-ABI
    // intrinsics `[resource-new]R` / `[resource-rep]R` / `[resource-drop]R`
    // imported from the `[export]` module by the emit layer.
    ResourceNew {
        resource: String,
        rep: Box<Instruction>,
    },
    ResourceRep {
        resource: String,
        handle: Box<Instruction>,
    },
    ResourceDrop {
        resource: String,
        handle: Box<Instruction>,
    },

    // Other
    Nop,
}

/// Format-adaptive representation for [`Instruction::StringLiteral`]'s bytes.
///
/// Binary formats (postcard) get the raw `Vec<u8>` — byte-for-byte what the
/// derive produced before this module existed, which is what keeps the
/// on-disk body format stable. Human-readable formats (JSON) get a string
/// when the bytes are valid UTF-8, falling back to the numeric array
/// otherwise so the value is never lossy.
mod string_bytes {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            if let Ok(text) = core::str::from_utf8(bytes) {
                return serializer.serialize_str(text);
            }
        }
        bytes.to_vec().serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        if !deserializer.is_human_readable() {
            return Vec::<u8>::deserialize(deserializer);
        }
        /// Either surface JSON form: `"hi"` or `[104, 105]`.
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Text(String),
            Bytes(Vec<u8>),
        }
        Ok(match Repr::deserialize(deserializer)? {
            Repr::Text(text) => text.into_bytes(),
            Repr::Bytes(bytes) => bytes,
        })
    }
}

/// One arm of a `MatchVariant`. `binding` is the local name the payload gets
/// bound to inside `body`; `None` when the case has no payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchArm {
    pub case: String,
    pub binding: Option<String>,
    pub body: Vec<Instruction>,
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
        Instruction::MatchVariant { arms, .. } => {
            for arm in arms {
                for (i, child) in arm.body.iter().enumerate() {
                    detect_patterns(child, i, matches);
                }
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

/// Current on-disk body format version. The serialized form is a single
/// version byte followed by the postcard encoding of `Vec<Instruction>`.
pub const BODY_FORMAT_VERSION: u8 = 1;

/// Serialize a slice of instructions into a compact binary format.
///
/// Layout: `[BODY_FORMAT_VERSION, <postcard(Vec<Instruction>)>...]`.
pub fn try_serialize_body(instructions: &[Instruction]) -> Result<Vec<u8>, String> {
    let payload = postcard::to_allocvec(instructions)
        .map_err(|e| format!("body serialization failed: {e}"))?;
    let mut out = Vec::with_capacity(payload.len() + 1);
    out.push(BODY_FORMAT_VERSION);
    out.extend_from_slice(&payload);
    Ok(out)
}

/// Infallible wrapper around [`try_serialize_body`], kept for existing
/// callers (syntax plugins). Postcard serialization of `Instruction` cannot
/// realistically fail, but new code should prefer [`try_serialize_body`].
pub fn serialize_body(instructions: &[Instruction]) -> Vec<u8> {
    try_serialize_body(instructions).expect("body serialization should not fail")
}

/// Deserialize instructions from the binary format produced by
/// [`serialize_body`]. The first byte is the format version; any value other
/// than [`BODY_FORMAT_VERSION`] is rejected with a clear error.
pub fn deserialize_body(data: &[u8]) -> Result<Vec<Instruction>, String> {
    match data.split_first() {
        Option::None => {
            Err("body deserialization failed: empty body (missing format-version byte)".to_string())
        }
        Some((&BODY_FORMAT_VERSION, payload)) => {
            postcard::from_bytes(payload).map_err(|e| format!("body deserialization failed: {e}"))
        }
        Some((&other, _)) => Err(format!(
            "body deserialization failed: unsupported body format version {other} \
             (this build supports version {BODY_FORMAT_VERSION})"
        )),
    }
}

/// Convert a serialized body into its JSON instruction-tree form.
///
/// This is the read half of the structured write path: hosts and agents edit
/// bodies as JSON instruction trees rather than opaque postcard bytes. The
/// output is a JSON array of instructions, pretty-printed.
pub fn body_to_json(data: &[u8]) -> Result<String, String> {
    let instructions = deserialize_body(data)?;
    serde_json::to_string_pretty(&instructions).map_err(|e| format!("body to JSON failed: {e}"))
}

/// Convert a JSON instruction tree back into the serialized body format.
///
/// The write half of [`body_to_json`]. Rejects malformed JSON and unknown
/// instruction shapes, so a bad structured edit fails here rather than
/// producing a body that only breaks at compile time.
pub fn body_from_json(json: &str) -> Result<Vec<u8>, String> {
    let instructions: Vec<Instruction> =
        serde_json::from_str(json).map_err(|e| format!("body from JSON failed: {e}"))?;
    try_serialize_body(&instructions)
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

    #[test]
    fn test_deserialize_empty_body_errors() {
        let result = deserialize_body(&[]);
        let msg = result.unwrap_err();
        assert!(msg.contains("missing format-version byte"), "{msg}");
    }

    #[test]
    fn test_deserialize_unknown_version_errors() {
        // Version 0 (pre-versioning data) and a future version must both be
        // rejected with a clear message instead of being mis-decoded.
        for bad_version in [0u8, 2, 0xFF] {
            let mut data = vec![bad_version];
            data.extend(postcard::to_allocvec::<Vec<Instruction>>(&vec![]).unwrap());
            let msg = deserialize_body(&data).unwrap_err();
            assert!(
                msg.contains(&format!("unsupported body format version {bad_version}")),
                "{msg}"
            );
        }
    }

    #[test]
    fn test_serialized_body_starts_with_version_byte() {
        let bytes = serialize_body(&[Instruction::Nop]);
        assert_eq!(bytes[0], BODY_FORMAT_VERSION);
    }

    /// Golden-bytes test: pins the exact serialized encoding of a body that
    /// covers early, middle, and late `Instruction` variants (incl.
    /// `ResourceDrop`, variant index 32). If this fails, the enum's variant
    /// order changed and every persisted body in the wild is now corrupt —
    /// variants may only be APPENDED, never inserted/reordered/removed.
    #[test]
    fn test_golden_serialized_bytes() {
        let body = vec![
            Instruction::Const { value: 7 },
            Instruction::LocalSet {
                uid: "x".into(),
                value: Box::new(Instruction::Arithmetic {
                    op: ArithOp::Add,
                    lhs: Box::new(Instruction::LocalGet { uid: "x".into() }),
                    rhs: Box::new(Instruction::Const { value: 1 }),
                }),
            },
            Instruction::Call {
                func_uid: "callee".into(),
                args: vec![("a".into(), Instruction::Const { value: 2 })],
            },
            Instruction::RecordLiteral {
                fields: vec![(
                    "f".into(),
                    Instruction::StringLiteral {
                        bytes: b"hi".to_vec(),
                    },
                )],
            },
            Instruction::MatchVariant {
                value: Box::new(Instruction::LocalGet { uid: "v".into() }),
                arms: vec![MatchArm {
                    case: "c".into(),
                    binding: Some("b".into()),
                    body: vec![Instruction::Return],
                }],
            },
            Instruction::ResourceDrop {
                resource: "r".into(),
                handle: Box::new(Instruction::LocalGet { uid: "h".into() }),
            },
            Instruction::Nop,
        ];
        let bytes = serialize_body(&body);
        let expected: Vec<u8> = vec![
            1, // format version
            7, // 7 instructions
            9, 14, // Const(7)
            8, 1, 120, 11, 0, 7, 1, 120, 9, 2, // LocalSet x = x + 1
            6, 6, 99, 97, 108, 108, 101, 101, 1, 1, 97, 9, 4, // Call callee(a: 2)
            24, 1, 1, 102, 19, 2, 104, 105, // RecordLiteral { f: "hi" }
            26, 7, 1, 118, 1, 1, 99, 1, 1, 98, 1, 5, // MatchVariant v { c(b) => return }
            32, 1, 114, 7, 1, 104, // ResourceDrop r, handle = h
            33,  // Nop
        ];
        assert_eq!(bytes, expected);
        assert_eq!(deserialize_body(&bytes).unwrap(), body);
    }

    // -----------------------------------------------------------------------
    // JSON body codec (the structured write path's surface)
    // -----------------------------------------------------------------------

    #[test]
    fn test_json_roundtrip_matches_postcard_bytes() {
        let body = vec![
            Instruction::Call {
                func_uid: "square".into(),
                args: vec![("x".into(), Instruction::LocalGet { uid: "a".into() })],
            },
            Instruction::MatchOption {
                value: Box::new(Instruction::LocalGet { uid: "o".into() }),
                some_binding: "v".into(),
                some_body: vec![Instruction::LocalGet { uid: "v".into() }],
                none_body: vec![Instruction::Const { value: 0 }],
            },
            Instruction::StringLiteral {
                bytes: b"hello, wast!".to_vec(),
            },
        ];
        let bytes = serialize_body(&body);
        let json = body_to_json(&bytes).unwrap();
        assert_eq!(
            body_from_json(&json).unwrap(),
            bytes,
            "JSON round-trip must reproduce the exact on-disk bytes"
        );
    }

    #[test]
    fn test_json_renders_string_literal_as_text() {
        // The whole point of the human-readable bytes representation: an
        // agent editing JSON sees `"bytes": "hi"`, not `[104, 105]`.
        let bytes = serialize_body(&[Instruction::StringLiteral {
            bytes: b"hi".to_vec(),
        }]);
        let json = body_to_json(&bytes).unwrap();
        assert!(json.contains("\"bytes\": \"hi\""), "{json}");
        assert!(
            !json.contains("104"),
            "bytes must not leak as numbers: {json}"
        );
    }

    #[test]
    fn test_json_accepts_byte_array_for_non_utf8_literal() {
        // Invalid UTF-8 has no string form, so it falls back to the numeric
        // array — and that form must parse back.
        let bytes = serialize_body(&[Instruction::StringLiteral {
            bytes: vec![0xff, 0xfe],
        }]);
        let json = body_to_json(&bytes).unwrap();
        assert!(json.contains("255"), "{json}");
        assert_eq!(body_from_json(&json).unwrap(), bytes);
    }

    #[test]
    fn test_json_body_is_agent_writable_by_hand() {
        // A hand-written instruction tree (what an LLM would emit) must
        // deserialize — externally-tagged variants, unit variants as bare
        // strings, string literals as text.
        let json = r#"[
          {"LocalSet": {"uid": "acc", "value": {"Const": {"value": 1}}}},
          {"If": {
            "condition": {"Compare": {"op": "Lt",
              "lhs": {"LocalGet": {"uid": "acc"}},
              "rhs": {"Const": {"value": 10}}}},
            "then_body": [{"StringLiteral": {"bytes": "small"}}],
            "else_body": ["Return"]
          }},
          "Nop"
        ]"#;
        let bytes = body_from_json(json).unwrap();
        let instrs = deserialize_body(&bytes).unwrap();
        assert_eq!(instrs.len(), 3);
        assert!(matches!(instrs[2], Instruction::Nop));
        let Instruction::If { then_body, .. } = &instrs[1] else {
            panic!("expected If, got {:?}", instrs[1]);
        };
        assert_eq!(
            then_body[0],
            Instruction::StringLiteral {
                bytes: b"small".to_vec()
            }
        );
    }

    #[test]
    fn test_json_rejects_unknown_instruction() {
        let err = body_from_json(r#"[{"Teleport": {"to": "moon"}}]"#).unwrap_err();
        assert!(err.contains("body from JSON failed"), "{err}");
    }

    #[test]
    fn test_body_to_json_rejects_corrupt_body() {
        let err = body_to_json(&[0xFF, 1, 2]).unwrap_err();
        assert!(err.contains("unsupported body format version"), "{err}");
    }

    #[test]
    fn test_detect_patterns_recurses_into_match_variant_arms() {
        // A Try pattern nested inside a MatchVariant arm must be detected.
        let body = vec![Instruction::MatchVariant {
            value: Box::new(Instruction::LocalGet { uid: "v".into() }),
            arms: vec![MatchArm {
                case: "c".into(),
                binding: Option::None,
                body: vec![Instruction::If {
                    condition: Box::new(Instruction::IsErr {
                        value: Box::new(Instruction::LocalGet { uid: "r".into() }),
                    }),
                    then_body: vec![Instruction::Return],
                    else_body: vec![],
                }],
            }],
        }];
        let result = analyze(&body);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].pattern, Pattern::Try);
    }
}
