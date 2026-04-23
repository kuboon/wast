//! v0.13 test: `StringLiteral` + data segments.
//! Literal bytes live in a pre-allocated data segment starting at offset
//! 1024. `StringLen(StringLiteral(...))` folds to a compile-time constant
//! (no memory access). Passing a literal across a Call boundary exercises
//! the actual (ptr, len) wiring.

use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wast_pattern_analyzer::{Instruction, serialize_body};
use wast_types::{FuncSource, WastDb, WastFunc, WastFuncRow};

fn load(db: &WastDb) -> (Engine, Component) {
    let wasm = wast_compiler::compile(db, "").expect("compile ok");
    let engine = Engine::new(&Config::new()).unwrap();
    let component = Component::from_binary(&engine, &wasm).expect("component load");
    (engine, component)
}

#[test]
fn string_literal_compile_time_len() {
    // hello_len() -> u32  { StringLen(StringLiteral(b"hello")) }
    // Folded to `i32.const 5` at compile time — no memory access required.
    let db = WastDb {
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
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(), (u32,)>(&mut store, "hello-len")
        .unwrap();
    assert_eq!(func.call(&mut store, ()).unwrap(), (5,));
    func.post_return(&mut store).unwrap();
}

#[test]
fn literal_passed_across_call_boundary() {
    // Internal `count(s: string) -> u32 { StringLen(s) }` reads the len
    // slot from its string param. Exported `literal_count() -> u32` pushes
    // a StringLiteral as the arg, exercising actual (ptr, len) passing.
    let db = WastDb {
        funcs: vec![
            WastFuncRow {
                uid: "count".into(),
                func: WastFunc {
                    source: FuncSource::Internal("count".into()),
                    params: vec![("s".into(), "string".into())],
                    result: Some("u32".into()),
                    body: Some(serialize_body(&[Instruction::StringLen {
                        value: Box::new(Instruction::LocalGet { uid: "s".into() }),
                    }])),
                },
            },
            WastFuncRow {
                uid: "literal_count".into(),
                func: WastFunc {
                    source: FuncSource::Exported("literal-count".into()),
                    params: vec![],
                    result: Some("u32".into()),
                    body: Some(serialize_body(&[Instruction::Call {
                        func_uid: "count".into(),
                        args: vec![(
                            "s".into(),
                            Instruction::StringLiteral {
                                bytes: b"wast rocks".to_vec(),
                            },
                        )],
                    }])),
                },
            },
        ],
        types: vec![],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(), (u32,)>(&mut store, "literal-count")
        .unwrap();
    assert_eq!(func.call(&mut store, ()).unwrap(), (10,));
    func.post_return(&mut store).unwrap();
}

#[test]
fn multibyte_utf8_literal() {
    // StringLiteral of a multi-byte UTF-8 sequence. Length = byte count.
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "jp_len".into(),
            func: WastFunc {
                source: FuncSource::Exported("jp-len".into()),
                params: vec![],
                result: Some("u32".into()),
                body: Some(serialize_body(&[Instruction::StringLen {
                    value: Box::new(Instruction::StringLiteral {
                        bytes: "こんにちは".as_bytes().to_vec(),
                    }),
                }])),
            },
        }],
        types: vec![],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(), (u32,)>(&mut store, "jp-len")
        .unwrap();
    // "こんにちは" = 5 chars × 3 bytes/char = 15 bytes.
    assert_eq!(func.call(&mut store, ()).unwrap(), (15,));
    func.post_return(&mut store).unwrap();
}
