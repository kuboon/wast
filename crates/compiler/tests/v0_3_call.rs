//! v0.3 test: `Instruction::Call` between internal + exported funcs within
//! a single core module. Exported `double(x)` calls internal `add(a, b)`
//! with `(a=x, b=x)` and returns the sum.

use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wast_pattern_analyzer::{ArithOp, Instruction, serialize_body};
use wast_types::{FuncSource, WastDb, WastFunc, WastFuncRow};

fn body(instrs: Vec<Instruction>) -> Option<Vec<u8>> {
    Some(serialize_body(&instrs))
}

#[test]
fn double_via_internal_add() {
    let db = WastDb {
        funcs: vec![
            // internal add(a: u32, b: u32) -> u32  { a + b }
            WastFuncRow {
                uid: "add".into(),
                func: WastFunc {
                    source: FuncSource::Internal("add".into()),
                    params: vec![("a".into(), "u32".into()), ("b".into(), "u32".into())],
                    result: Some("u32".into()),
                    body: body(vec![Instruction::Arithmetic {
                        op: ArithOp::Add,
                        lhs: Box::new(Instruction::LocalGet { uid: "a".into() }),
                        rhs: Box::new(Instruction::LocalGet { uid: "b".into() }),
                    }]),
                },
            },
            // exported double(x: u32) -> u32  { add(a=x, b=x) }
            WastFuncRow {
                uid: "double".into(),
                func: WastFunc {
                    source: FuncSource::Exported("double".into()),
                    params: vec![("x".into(), "u32".into())],
                    result: Some("u32".into()),
                    body: body(vec![Instruction::Call {
                        func_uid: "add".into(),
                        args: vec![
                            ("a".into(), Instruction::LocalGet { uid: "x".into() }),
                            ("b".into(), Instruction::LocalGet { uid: "x".into() }),
                        ],
                    }]),
                },
            },
        ],
        types: vec![],
    };

    let wasm = wast_compiler::compile(&db, "").expect("compile ok");

    let engine = Engine::new(&Config::new()).unwrap();
    let component = Component::from_binary(&engine, &wasm).expect("component load");
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(u32,), (u32,)>(&mut store, "double")
        .unwrap();
    assert_eq!(func.call(&mut store, (21,)).unwrap(), (42,));
}

#[test]
fn call_arg_reordering() {
    // Verify that callers can pass args in any order — the emitter must
    // reorder them to match the callee's declared param list.
    // sub(a, b) = a - b; called as sub(b=10, a=100) → 100-10 = 90.
    let db = WastDb {
        funcs: vec![
            WastFuncRow {
                uid: "sub".into(),
                func: WastFunc {
                    source: FuncSource::Internal("sub".into()),
                    params: vec![("a".into(), "i32".into()), ("b".into(), "i32".into())],
                    result: Some("i32".into()),
                    body: body(vec![Instruction::Arithmetic {
                        op: ArithOp::Sub,
                        lhs: Box::new(Instruction::LocalGet { uid: "a".into() }),
                        rhs: Box::new(Instruction::LocalGet { uid: "b".into() }),
                    }]),
                },
            },
            WastFuncRow {
                uid: "run".into(),
                func: WastFunc {
                    source: FuncSource::Exported("run".into()),
                    params: vec![],
                    result: Some("i32".into()),
                    body: body(vec![Instruction::Call {
                        func_uid: "sub".into(),
                        args: vec![
                            ("b".into(), Instruction::Const { value: 10 }),
                            ("a".into(), Instruction::Const { value: 100 }),
                        ],
                    }]),
                },
            },
        ],
        types: vec![],
    };

    let wasm = wast_compiler::compile(&db, "").expect("compile ok");
    let engine = Engine::new(&Config::new()).unwrap();
    let component = Component::from_binary(&engine, &wasm).expect("component load");
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(), (i32,)>(&mut store, "run")
        .unwrap();
    assert_eq!(func.call(&mut store, ()).unwrap(), (90,));
}
