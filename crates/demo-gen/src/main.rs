//! Emit every v0.x milestone component into a target directory so the web
//! demo can transpile them via `jco` and load them in the browser.
//!
//! Usage: `cargo run -p wast-demo-gen -- <output_dir>`
//! The binary writes one `<name>.wasm` per demo plus a `manifest.json`
//! describing each demo (function signatures, test inputs, etc.) so the
//! front-end can render a consistent UI without hard-coding per demo.

use std::fs;
use std::path::{Path, PathBuf};

use wast_pattern_analyzer::{ArithOp, CompareOp, Instruction, serialize_body};
use wast_types::{
    FuncSource, TypeSource, WastDb, WastFunc, WastFuncRow, WastTypeDef, WastTypeRow, WitType,
};

fn main() {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().unwrap_or_else(|| "dist/components".to_string()));
    fs::create_dir_all(&out).expect("mkdir output");

    let demos = all_demos();

    let mut manifest_entries = Vec::new();
    for demo in &demos {
        let wasm = wast_compiler::compile(&demo.db, "").expect(&demo.id);
        let path = out.join(format!("{}.wasm", demo.id));
        fs::write(&path, &wasm).expect("write component");
        println!("wrote {} ({} bytes)", path.display(), wasm.len());

        manifest_entries.push(demo.manifest_entry());
    }

    let manifest = format!("[\n  {}\n]\n", manifest_entries.join(",\n  "));
    fs::write(out.join("manifest.json"), manifest).expect("write manifest");
    println!("wrote manifest with {} demos", demos.len());
}

// ---------------------------------------------------------------------------
// Demo definitions
// ---------------------------------------------------------------------------

struct Demo {
    id: &'static str,
    milestone: &'static str,
    title: &'static str,
    description: &'static str,
    export: &'static str,
    /// Input widgets the UI should render. JSON-style signature tag.
    params_js: &'static str,
    /// Return type tag (matches what jco's generated JS produces).
    result_js: &'static str,
    /// Example inputs the UI can show as one-click presets. Each entry is a
    /// single JSON array literal matching the param list.
    presets: &'static [&'static str],
    db: WastDb,
}

impl Demo {
    fn manifest_entry(&self) -> String {
        let presets = format!("[{}]", self.presets.join(", "));
        format!(
            "{{ \"id\": \"{id}\", \"milestone\": \"{milestone}\", \"title\": {title}, \"description\": {desc}, \"export\": \"{export}\", \"params\": {params}, \"result\": \"{result}\", \"presets\": {presets} }}",
            id = self.id,
            milestone = self.milestone,
            title = json_string(self.title),
            desc = json_string(self.description),
            export = self.export,
            params = self.params_js,
            result = self.result_js,
            presets = presets,
        )
    }
}

fn json_string(s: &str) -> String {
    let escaped: String = s
        .chars()
        .flat_map(|c| match c {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect(),
            '\n' => "\\n".chars().collect(),
            c => vec![c],
        })
        .collect();
    format!("\"{escaped}\"")
}

fn all_demos() -> Vec<Demo> {
    vec![
        identity(),
        add(),
        is_zero(),
        max_demo(),
        sum_loop(),
        sum_of_squares(),
        unwrap_or(),
        mk_some(),
        strlen_demo(),
        hello_literal(),
        echo_string_demo(),
        greeting(),
        len_of_list(),
        echo_list_demo(),
        get_x_demo(),
        make_point_demo(),
        mk_shape(),
        make_pair_demo(),
        color_kind(),
        perms_mask(),
        wrap_greeting(),
        numbers_literal(),
        make_pair_from_points(),
    ]
}

// ---- v0.1 --------------------------------------------------------------

fn identity() -> Demo {
    Demo {
        id: "v0_1_identity",
        milestone: "v0.1",
        title: "identity(u32) → u32",
        description: "Simplest function: pass an integer through unchanged. Demonstrates static WAT emit, WIT world synthesis, wasmtime-style call.",
        export: "identity",
        params_js: "[{\"name\":\"x\",\"kind\":\"u32\"}]",
        result_js: "u32",
        presets: &["[42]", "[0]", "[4294967295]"],
        db: WastDb {
            funcs: vec![WastFuncRow {
                uid: "identity".into(),
                func: WastFunc {
                    source: FuncSource::Exported("identity".into()),
                    params: vec![("x".into(), "u32".into())],
                    result: Some("u32".into()),
                    body: Some(serialize_body(&[Instruction::LocalGet { uid: "x".into() }])),
                },
            }],
            types: vec![],
        },
    }
}

// ---- v0.2 numeric ------------------------------------------------------

fn add() -> Demo {
    Demo {
        id: "v0_2_add",
        milestone: "v0.2",
        title: "add(u32, u32) → u32",
        description: "Arithmetic on primitives. Emits core `i32.add` with signedness-aware dispatch (here u32 unsigned).",
        export: "add",
        params_js: "[{\"name\":\"a\",\"kind\":\"u32\"},{\"name\":\"b\",\"kind\":\"u32\"}]",
        result_js: "u32",
        presets: &["[7, 35]", "[1, 1]", "[1000000, 2000000]"],
        db: WastDb {
            funcs: vec![WastFuncRow {
                uid: "add".into(),
                func: WastFunc {
                    source: FuncSource::Exported("add".into()),
                    params: vec![("a".into(), "u32".into()), ("b".into(), "u32".into())],
                    result: Some("u32".into()),
                    body: Some(serialize_body(&[Instruction::Arithmetic {
                        op: ArithOp::Add,
                        lhs: Box::new(local("a")),
                        rhs: Box::new(local("b")),
                    }])),
                },
            }],
            types: vec![],
        },
    }
}

fn is_zero() -> Demo {
    Demo {
        id: "v0_2_is_zero",
        milestone: "v0.2",
        title: "is-zero(i32) → bool",
        description: "Comparison with Const type inferred from the sibling LocalGet. Canonical ABI's bool is a core i32 (0/1).",
        export: "is-zero",
        params_js: "[{\"name\":\"x\",\"kind\":\"i32\"}]",
        result_js: "bool",
        presets: &["[0]", "[1]", "[-5]"],
        db: WastDb {
            funcs: vec![WastFuncRow {
                uid: "is_zero".into(),
                func: WastFunc {
                    source: FuncSource::Exported("is-zero".into()),
                    params: vec![("x".into(), "i32".into())],
                    result: Some("bool".into()),
                    body: Some(serialize_body(&[Instruction::Compare {
                        op: CompareOp::Eq,
                        lhs: Box::new(local("x")),
                        rhs: Box::new(Instruction::Const { value: 0 }),
                    }])),
                },
            }],
            types: vec![],
        },
    }
}

// ---- v0.4 control flow -------------------------------------------------

fn max_demo() -> Demo {
    Demo {
        id: "v0_4_max",
        milestone: "v0.4",
        title: "max(u32, u32) → u32",
        description: "If/Else returning a value. Emits WAT `if (result i32) … else … end`.",
        export: "max",
        params_js: "[{\"name\":\"a\",\"kind\":\"u32\"},{\"name\":\"b\",\"kind\":\"u32\"}]",
        result_js: "u32",
        presets: &["[3, 9]", "[42, 17]", "[5, 5]"],
        db: WastDb {
            funcs: vec![WastFuncRow {
                uid: "max".into(),
                func: WastFunc {
                    source: FuncSource::Exported("max".into()),
                    params: vec![("a".into(), "u32".into()), ("b".into(), "u32".into())],
                    result: Some("u32".into()),
                    body: Some(serialize_body(&[Instruction::If {
                        condition: Box::new(Instruction::Compare {
                            op: CompareOp::Gt,
                            lhs: Box::new(local("a")),
                            rhs: Box::new(local("b")),
                        }),
                        then_body: vec![local("a")],
                        else_body: vec![local("b")],
                    }])),
                },
            }],
            types: vec![],
        },
    }
}

fn sum_loop() -> Demo {
    Demo {
        id: "v0_4_sum_1_to_n",
        milestone: "v0.4",
        title: "sum(n) = 1+2+…+n",
        description: "Block + Loop + BrIf with a synthesized `acc` local seeded via `n - n` (to give the compiler a u32 anchor). Exercises every piece of the control-flow emit.",
        export: "sum",
        params_js: "[{\"name\":\"n\",\"kind\":\"u32\"}]",
        result_js: "u32",
        presets: &["[10]", "[0]", "[100]"],
        db: WastDb {
            funcs: vec![WastFuncRow {
                uid: "sum".into(),
                func: WastFunc {
                    source: FuncSource::Exported("sum".into()),
                    params: vec![("n".into(), "u32".into())],
                    result: Some("u32".into()),
                    body: Some(serialize_body(&[
                        Instruction::LocalSet {
                            uid: "acc".into(),
                            value: Box::new(Instruction::Arithmetic {
                                op: ArithOp::Sub,
                                lhs: Box::new(local("n")),
                                rhs: Box::new(local("n")),
                            }),
                        },
                        Instruction::Block {
                            label: Some("done".into()),
                            body: vec![Instruction::Loop {
                                label: Some("body".into()),
                                body: vec![
                                    Instruction::BrIf {
                                        label: "done".into(),
                                        condition: Box::new(Instruction::Compare {
                                            op: CompareOp::Eq,
                                            lhs: Box::new(local("n")),
                                            rhs: Box::new(Instruction::Const { value: 0 }),
                                        }),
                                    },
                                    Instruction::LocalSet {
                                        uid: "acc".into(),
                                        value: Box::new(Instruction::Arithmetic {
                                            op: ArithOp::Add,
                                            lhs: Box::new(local("acc")),
                                            rhs: Box::new(local("n")),
                                        }),
                                    },
                                    Instruction::LocalSet {
                                        uid: "n".into(),
                                        value: Box::new(Instruction::Arithmetic {
                                            op: ArithOp::Sub,
                                            lhs: Box::new(local("n")),
                                            rhs: Box::new(Instruction::Const { value: 1 }),
                                        }),
                                    },
                                    Instruction::Br {
                                        label: "body".into(),
                                    },
                                ],
                            }],
                        },
                        local("acc"),
                    ])),
                },
            }],
            types: vec![],
        },
    }
}

// ---- v0.6/v0.9 option/result -------------------------------------------

fn unwrap_or() -> Demo {
    Demo {
        id: "v0_9_unwrap_or",
        milestone: "v0.9",
        title: "unwrap-or(option<u32>, u32) → u32",
        description: "MatchOption destructures a compound param; some_binding is stored into a synthesized local before the branch.",
        export: "unwrap-or",
        params_js: "[{\"name\":\"o\",\"kind\":\"option\",\"inner\":\"u32\"},{\"name\":\"default\",\"kind\":\"u32\"}]",
        result_js: "u32",
        // jco lifts option<T> as `T | null` — no {tag,val} wrapper.
        presets: &["[42, 99]", "[null, 99]", "[0, 7]"],
        db: WastDb {
            funcs: vec![WastFuncRow {
                uid: "unwrap_or".into(),
                func: WastFunc {
                    source: FuncSource::Exported("unwrap-or".into()),
                    params: vec![
                        ("o".into(), "opt_u32".into()),
                        ("default".into(), "u32".into()),
                    ],
                    result: Some("u32".into()),
                    body: Some(serialize_body(&[Instruction::MatchOption {
                        value: Box::new(local("o")),
                        some_binding: "x".into(),
                        some_body: vec![local("x")],
                        none_body: vec![local("default")],
                    }])),
                },
            }],
            types: vec![WastTypeRow {
                uid: "opt_u32".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("opt_u32".into()),
                    definition: WitType::Option("u32".into()),
                },
            }],
        },
    }
}

// ---- v0.8 compound return ---------------------------------------------

fn mk_some() -> Demo {
    Demo {
        id: "v0_8_mk_some",
        milestone: "v0.8",
        title: "mk-some(u32) → option<u32>",
        description: "Returns Some(x). Indirect return: allocate 8 bytes via cabi_realloc, write disc + payload, return pointer.",
        export: "mk-some",
        params_js: "[{\"name\":\"x\",\"kind\":\"u32\"}]",
        result_js: "option<u32>",
        presets: &["[42]", "[0]"],
        db: WastDb {
            funcs: vec![WastFuncRow {
                uid: "mk_some".into(),
                func: WastFunc {
                    source: FuncSource::Exported("mk-some".into()),
                    params: vec![("x".into(), "u32".into())],
                    result: Some("opt_u32".into()),
                    body: Some(serialize_body(&[Instruction::Some {
                        value: Box::new(local("x")),
                    }])),
                },
            }],
            types: vec![WastTypeRow {
                uid: "opt_u32".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("opt_u32".into()),
                    definition: WitType::Option("u32".into()),
                },
            }],
        },
    }
}

// ---- v0.12-v0.14 string -------------------------------------------------

fn strlen_demo() -> Demo {
    Demo {
        id: "v0_12_strlen",
        milestone: "v0.12",
        title: "strlen(string) → u32",
        description: "Byte length of a string parameter. Host encodes as UTF-8 and writes into our memory; StringLen reads the `len` slot.",
        export: "strlen",
        params_js: "[{\"name\":\"s\",\"kind\":\"string\"}]",
        result_js: "u32",
        presets: &["[\"hello\"]", "[\"\"]", "[\"あいう\"]"],
        db: WastDb {
            funcs: vec![WastFuncRow {
                uid: "strlen".into(),
                func: WastFunc {
                    source: FuncSource::Exported("strlen".into()),
                    params: vec![("s".into(), "string".into())],
                    result: Some("u32".into()),
                    body: Some(serialize_body(&[Instruction::StringLen {
                        value: Box::new(local("s")),
                    }])),
                },
            }],
            types: vec![],
        },
    }
}

fn hello_literal() -> Demo {
    Demo {
        id: "v0_13_hello_len",
        milestone: "v0.13",
        title: "hello-len() → u32",
        description: "StringLen(StringLiteral(\"hello\")) compile-time folds to i32.const 5 — no memory access.",
        export: "hello-len",
        params_js: "[]",
        result_js: "u32",
        presets: &["[]"],
        db: WastDb {
            funcs: vec![WastFuncRow {
                uid: "hello_len".into(),
                func: WastFunc {
                    source: FuncSource::Exported("hello-len".into()),
                    params: vec![],
                    result: Some("u32".into()),
                    body: Some(serialize_body(&[Instruction::StringLen {
                        value: Box::new(Instruction::StringLiteral {
                            bytes: b"hello".to_vec(),
                        }),
                    }])),
                },
            }],
            types: vec![],
        },
    }
}

fn echo_string_demo() -> Demo {
    Demo {
        id: "v0_14_echo_string",
        milestone: "v0.14",
        title: "echo(string) → string",
        description: "Passthrough. Indirect return writes (ptr, len) into an 8-byte return area.",
        export: "echo",
        params_js: "[{\"name\":\"s\",\"kind\":\"string\"}]",
        result_js: "string",
        presets: &["[\"hello\"]", "[\"日本語\"]", "[\"\"]"],
        db: WastDb {
            funcs: vec![WastFuncRow {
                uid: "echo".into(),
                func: WastFunc {
                    source: FuncSource::Exported("echo".into()),
                    params: vec![("s".into(), "string".into())],
                    result: Some("string".into()),
                    body: Some(serialize_body(&[local("s")])),
                },
            }],
            types: vec![],
        },
    }
}

fn greeting() -> Demo {
    Demo {
        id: "v0_14_greeting",
        milestone: "v0.14",
        title: "greeting() → string",
        description: "Returns a constant string from the data segment. Bytes live at a fixed offset; return area holds (offset, len).",
        export: "greeting",
        params_js: "[]",
        result_js: "string",
        presets: &["[]"],
        db: WastDb {
            funcs: vec![WastFuncRow {
                uid: "greeting".into(),
                func: WastFunc {
                    source: FuncSource::Exported("greeting".into()),
                    params: vec![],
                    result: Some("string".into()),
                    body: Some(serialize_body(&[Instruction::StringLiteral {
                        bytes: b"hello, wast!".to_vec(),
                    }])),
                },
            }],
            types: vec![],
        },
    }
}

// ---- v0.15 list --------------------------------------------------------

fn len_of_list() -> Demo {
    Demo {
        id: "v0_15_len_of",
        milestone: "v0.15",
        title: "len-of(list<u32>) → u32",
        description: "Element count of a list param. Same flat (ptr, len) layout as string — `len` is elements, not bytes.",
        export: "len-of",
        params_js: "[{\"name\":\"xs\",\"kind\":\"list\",\"inner\":\"u32\"}]",
        result_js: "u32",
        presets: &["[[]]", "[[1,2,3,4,5]]", "[[10,20,30]]"],
        db: WastDb {
            funcs: vec![WastFuncRow {
                uid: "len_of".into(),
                func: WastFunc {
                    source: FuncSource::Exported("len-of".into()),
                    params: vec![("xs".into(), "list_u32".into())],
                    result: Some("u32".into()),
                    body: Some(serialize_body(&[Instruction::ListLen {
                        value: Box::new(local("xs")),
                    }])),
                },
            }],
            types: vec![WastTypeRow {
                uid: "list_u32".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("list_u32".into()),
                    definition: WitType::List("u32".into()),
                },
            }],
        },
    }
}

fn echo_list_demo() -> Demo {
    Demo {
        id: "v0_15_echo_list",
        milestone: "v0.15",
        title: "echo-list(list<u32>) → list<u32>",
        description: "Passthrough of a list. Reuses the string return wrap (it's the same (ptr, len) shape at the ABI level).",
        export: "echo-list",
        params_js: "[{\"name\":\"xs\",\"kind\":\"list\",\"inner\":\"u32\"}]",
        result_js: "list<u32>",
        presets: &["[[1,2,3]]", "[[42, 999]]", "[[]]"],
        db: WastDb {
            funcs: vec![WastFuncRow {
                uid: "echo_list".into(),
                func: WastFunc {
                    source: FuncSource::Exported("echo-list".into()),
                    params: vec![("xs".into(), "list_u32".into())],
                    result: Some("list_u32".into()),
                    body: Some(serialize_body(&[local("xs")])),
                },
            }],
            types: vec![WastTypeRow {
                uid: "list_u32".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("list_u32".into()),
                    definition: WitType::List("u32".into()),
                },
            }],
        },
    }
}

// ---- v0.16 record ------------------------------------------------------

fn get_x_demo() -> Demo {
    Demo {
        id: "v0_16_get_x",
        milestone: "v0.16",
        title: "get-x(point) → u32",
        description: "Field access on a record param. Reads the flat slot at the field's slot offset within the record.",
        export: "get-x",
        params_js: "[{\"name\":\"p\",\"kind\":\"record\",\"fields\":[[\"x\",\"u32\"],[\"y\",\"u32\"]]}]",
        result_js: "u32",
        presets: &["[{\"x\":42,\"y\":7}]", "[{\"x\":100,\"y\":200}]"],
        db: WastDb {
            funcs: vec![WastFuncRow {
                uid: "get_x".into(),
                func: WastFunc {
                    source: FuncSource::Exported("get-x".into()),
                    params: vec![("p".into(), "point".into())],
                    result: Some("u32".into()),
                    body: Some(serialize_body(&[Instruction::RecordGet {
                        value: Box::new(local("p")),
                        field: "x".into(),
                    }])),
                },
            }],
            types: vec![point_type_row()],
        },
    }
}

fn make_point_demo() -> Demo {
    Demo {
        id: "v0_16_make_point",
        milestone: "v0.16",
        title: "make-point(u32, u32) → point",
        description: "RecordLiteral at return position. Each field is written at its Canonical-ABI byte offset in the allocated return buffer.",
        export: "make-point",
        params_js: "[{\"name\":\"x\",\"kind\":\"u32\"},{\"name\":\"y\",\"kind\":\"u32\"}]",
        result_js: "record<point>",
        presets: &["[11, 22]", "[0, 0]"],
        db: WastDb {
            funcs: vec![WastFuncRow {
                uid: "make_point".into(),
                func: WastFunc {
                    source: FuncSource::Exported("make-point".into()),
                    params: vec![("x".into(), "u32".into()), ("y".into(), "u32".into())],
                    result: Some("point".into()),
                    body: Some(serialize_body(&[Instruction::RecordLiteral {
                        fields: vec![("x".into(), local("x")), ("y".into(), local("y"))],
                    }])),
                },
            }],
            types: vec![point_type_row()],
        },
    }
}

fn point_type_row() -> WastTypeRow {
    WastTypeRow {
        uid: "point".into(),
        def: WastTypeDef {
            source: TypeSource::Internal("point".into()),
            definition: WitType::Record(vec![
                ("x".into(), "u32".into()),
                ("y".into(), "u32".into()),
            ]),
        },
    }
}

// ---- v0.3 internal Call ------------------------------------------------

fn sum_of_squares() -> Demo {
    // square is an internal (not exported) helper; sum-of-squares calls it
    // twice and adds the results. Exercises the func-to-func call path
    // (v0.3 internal Call) — callers pass args by name, callee's param order
    // determines the core stack ordering.
    Demo {
        id: "v0_3_sum_of_squares",
        milestone: "v0.3",
        title: "sum-of-squares(a, b) = a²+b²",
        description: "Exported `sum-of-squares` calls an internal `square` helper twice and adds the results. Demonstrates func-to-func call: callers push args in the callee's declared param order, and both funcs live in the same core module.",
        export: "sum-of-squares",
        params_js: "[{\"name\":\"a\",\"kind\":\"u32\"},{\"name\":\"b\",\"kind\":\"u32\"}]",
        result_js: "u32",
        presets: &["[3, 4]", "[5, 12]", "[0, 7]"],
        db: WastDb {
            funcs: vec![
                WastFuncRow {
                    uid: "square".into(),
                    func: WastFunc {
                        source: FuncSource::Internal("square".into()),
                        params: vec![("x".into(), "u32".into())],
                        result: Some("u32".into()),
                        body: Some(serialize_body(&[Instruction::Arithmetic {
                            op: ArithOp::Mul,
                            lhs: Box::new(local("x")),
                            rhs: Box::new(local("x")),
                        }])),
                    },
                },
                WastFuncRow {
                    uid: "sum_of_squares".into(),
                    func: WastFunc {
                        source: FuncSource::Exported("sum-of-squares".into()),
                        params: vec![("a".into(), "u32".into()), ("b".into(), "u32".into())],
                        result: Some("u32".into()),
                        body: Some(serialize_body(&[Instruction::Arithmetic {
                            op: ArithOp::Add,
                            lhs: Box::new(Instruction::Call {
                                func_uid: "square".into(),
                                args: vec![("x".into(), local("a"))],
                            }),
                            rhs: Box::new(Instruction::Call {
                                func_uid: "square".into(),
                                args: vec![("x".into(), local("b"))],
                            }),
                        }])),
                    },
                },
            ],
            types: vec![],
        },
    }
}

// ---- v0.17 variant -----------------------------------------------------

fn mk_shape() -> Demo {
    // variant shape { circle(u32), square(u32), unit }. Keeping the body a
    // single VariantCtor (the current return-wrap requires the top-level
    // instruction to be a literal ctor — no branching). Flip the comment/
    // body to try the other cases.
    Demo {
        id: "v0_17_mk_shape",
        milestone: "v0.17",
        title: "mk-shape(n: u32) → shape",
        description: "General variant with three cases (two carry a u32 payload, one is unit). This body constructs `circle(n)`; emits u8 disc + payload into an indirect return buffer.",
        export: "mk-shape",
        params_js: "[{\"name\":\"n\",\"kind\":\"u32\"}]",
        result_js: "variant<shape>",
        presets: &["[5]", "[0]", "[42]"],
        db: WastDb {
            funcs: vec![WastFuncRow {
                uid: "mk_shape".into(),
                func: WastFunc {
                    source: FuncSource::Exported("mk-shape".into()),
                    params: vec![("n".into(), "u32".into())],
                    result: Some("shape".into()),
                    body: Some(serialize_body(&[Instruction::VariantCtor {
                        case: "circle".into(),
                        value: Some(Box::new(local("n"))),
                    }])),
                },
            }],
            types: vec![WastTypeRow {
                uid: "shape".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("shape".into()),
                    definition: WitType::Variant(vec![
                        ("circle".into(), Some("u32".into())),
                        ("square".into(), Some("u32".into())),
                        ("unit".into(), None),
                    ]),
                },
            }],
        },
    }
}

// ---- v0.18 tuple -------------------------------------------------------

fn make_pair_demo() -> Demo {
    Demo {
        id: "v0_18_make_pair",
        milestone: "v0.18",
        title: "make-pair(u32, u32) → tuple<u32, u32>",
        description: "Anonymous positional record. Same byte layout as a record with fields \"0\", \"1\" — WIT inlines it as `tuple<u32, u32>` at the use site.",
        export: "make-pair",
        params_js: "[{\"name\":\"a\",\"kind\":\"u32\"},{\"name\":\"b\",\"kind\":\"u32\"}]",
        result_js: "tuple<u32, u32>",
        presets: &["[11, 22]", "[0, 0]"],
        db: WastDb {
            funcs: vec![WastFuncRow {
                uid: "make_pair".into(),
                func: WastFunc {
                    source: FuncSource::Exported("make-pair".into()),
                    params: vec![("a".into(), "u32".into()), ("b".into(), "u32".into())],
                    result: Some("u32_pair".into()),
                    body: Some(serialize_body(&[Instruction::TupleLiteral {
                        values: vec![local("a"), local("b")],
                    }])),
                },
            }],
            types: vec![WastTypeRow {
                uid: "u32_pair".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("u32_pair".into()),
                    definition: WitType::Tuple(vec!["u32".into(), "u32".into()]),
                },
            }],
        },
    }
}

// ---- v0.19 enum & flags ------------------------------------------------

fn color_kind() -> Demo {
    Demo {
        id: "v0_19_color_kind",
        milestone: "v0.19",
        title: "favorite() → color",
        description: "Enum = payload-less variant. `VariantCtor { case: red }` emits a bare `i32.const 0` — no memory needed because the flat form is a single i32 disc.",
        export: "favorite",
        params_js: "[]",
        result_js: "enum<color>",
        presets: &["[]"],
        db: WastDb {
            funcs: vec![WastFuncRow {
                uid: "favorite".into(),
                func: WastFunc {
                    source: FuncSource::Exported("favorite".into()),
                    params: vec![],
                    result: Some("color".into()),
                    body: Some(serialize_body(&[Instruction::VariantCtor {
                        case: "green".into(),
                        value: None,
                    }])),
                },
            }],
            types: vec![WastTypeRow {
                uid: "color".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("color".into()),
                    definition: WitType::Enum(vec!["red".into(), "green".into(), "blue".into()]),
                },
            }],
        },
    }
}

fn perms_mask() -> Demo {
    Demo {
        id: "v0_19_perms",
        milestone: "v0.19",
        title: "perms() → flags<perms>",
        description: "`FlagsCtor { flags: [read, write] }` compile-time folds to a bitmask (1 | 2 = 3). ≤32 flags fit in an i32.",
        export: "perms",
        params_js: "[]",
        result_js: "flags<perms>",
        presets: &["[]"],
        db: WastDb {
            funcs: vec![WastFuncRow {
                uid: "perms".into(),
                func: WastFunc {
                    source: FuncSource::Exported("perms".into()),
                    params: vec![],
                    result: Some("perms_t".into()),
                    body: Some(serialize_body(&[Instruction::FlagsCtor {
                        flags: vec!["read".into(), "write".into()],
                    }])),
                },
            }],
            types: vec![WastTypeRow {
                uid: "perms_t".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("perms_t".into()),
                    definition: WitType::Flags(vec![
                        "read".into(),
                        "write".into(),
                        "execute".into(),
                    ]),
                },
            }],
        },
    }
}

// ---- v0.20 nested compound (string field in record) --------------------

fn wrap_greeting() -> Demo {
    Demo {
        id: "v0_20_wrap_greeting",
        milestone: "v0.20",
        title: "wrap(msg: string, count: u32) → greeting",
        description: "Record with a string field (not just primitives). `emit_field_store` handles the (ptr, len) pair at the field's byte offset inside the record buffer.",
        export: "wrap",
        params_js: "[{\"name\":\"msg\",\"kind\":\"string\"},{\"name\":\"n\",\"kind\":\"u32\"}]",
        result_js: "record<greeting>",
        presets: &["[\"hello\", 3]", "[\"\", 0]", "[\"日本語\", 42]"],
        db: WastDb {
            funcs: vec![WastFuncRow {
                uid: "wrap".into(),
                func: WastFunc {
                    source: FuncSource::Exported("wrap".into()),
                    params: vec![("msg".into(), "string".into()), ("n".into(), "u32".into())],
                    result: Some("greeting".into()),
                    body: Some(serialize_body(&[Instruction::RecordLiteral {
                        fields: vec![
                            ("message".into(), local("msg")),
                            ("count".into(), local("n")),
                        ],
                    }])),
                },
            }],
            types: vec![WastTypeRow {
                uid: "greeting".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("greeting".into()),
                    definition: WitType::Record(vec![
                        ("message".into(), "string".into()),
                        ("count".into(), "u32".into()),
                    ]),
                },
            }],
        },
    }
}

// ---- v0.21 ListLiteral -------------------------------------------------

fn numbers_literal() -> Demo {
    Demo {
        id: "v0_21_numbers",
        milestone: "v0.21",
        title: "numbers() → list<u32>",
        description: "Runtime list construction. Allocates count·size bytes via cabi_realloc, stores each element at offset i·size, returns (ptr, count).",
        export: "numbers",
        params_js: "[]",
        result_js: "list<u32>",
        presets: &["[]"],
        db: WastDb {
            funcs: vec![WastFuncRow {
                uid: "numbers".into(),
                func: WastFunc {
                    source: FuncSource::Exported("numbers".into()),
                    params: vec![],
                    result: Some("list_u32".into()),
                    body: Some(serialize_body(&[Instruction::ListLiteral {
                        values: vec![
                            Instruction::Const { value: 2 },
                            Instruction::Const { value: 3 },
                            Instruction::Const { value: 5 },
                            Instruction::Const { value: 7 },
                            Instruction::Const { value: 11 },
                        ],
                    }])),
                },
            }],
            types: vec![WastTypeRow {
                uid: "list_u32".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("list_u32".into()),
                    definition: WitType::List("u32".into()),
                },
            }],
        },
    }
}

// ---- v0.24 LocalGet of compound in field position ----------------------

fn make_pair_from_points() -> Demo {
    // record pair { a: point, b: point } built by copying two point-typed
    // locals directly into the parent buffer via emit_copy_from_local.
    Demo {
        id: "v0_24_pair_of_points",
        milestone: "v0.24",
        title: "make-pair(p1: point, p2: point) → pair",
        description: "Each field's source is `LocalGet(point_local)` — v0.24's `emit_copy_from_local` walks the point type and writes x/y as direct i32 stores at the Canonical-ABI byte offsets.",
        export: "make-pair",
        params_js: "[{\"name\":\"p1\",\"kind\":\"record\",\"fields\":[[\"x\",\"u32\"],[\"y\",\"u32\"]]},{\"name\":\"p2\",\"kind\":\"record\",\"fields\":[[\"x\",\"u32\"],[\"y\",\"u32\"]]}]",
        result_js: "record<pair>",
        presets: &["[{\"x\":1,\"y\":2}, {\"x\":3,\"y\":4}]"],
        db: WastDb {
            funcs: vec![WastFuncRow {
                uid: "make_pair".into(),
                func: WastFunc {
                    source: FuncSource::Exported("make-pair".into()),
                    params: vec![("p1".into(), "point".into()), ("p2".into(), "point".into())],
                    result: Some("pair".into()),
                    body: Some(serialize_body(&[Instruction::RecordLiteral {
                        fields: vec![("a".into(), local("p1")), ("b".into(), local("p2"))],
                    }])),
                },
            }],
            types: vec![
                point_type_row(),
                WastTypeRow {
                    uid: "pair".into(),
                    def: WastTypeDef {
                        source: TypeSource::Internal("pair".into()),
                        definition: WitType::Record(vec![
                            ("a".into(), "point".into()),
                            ("b".into(), "point".into()),
                        ]),
                    },
                },
            ],
        },
    }
}

// Helper --------------------------------------------------------------------

fn local(uid: &str) -> Instruction {
    Instruction::LocalGet { uid: uid.into() }
}

#[allow(dead_code)]
fn _ensure_path_kept(_: &Path) {}
