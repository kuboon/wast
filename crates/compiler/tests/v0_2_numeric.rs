//! v0.2 test: numeric type expansion (Const, Arithmetic, Compare) across
//! integer widths, signedness, and floats. Each case compiles a single-func
//! component and verifies the result end-to-end via wasmtime.

use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wast_pattern_analyzer::{ArithOp, CompareOp, Instruction, serialize_body};
use wast_types::{FuncSource, WastDb, WastFunc, WastFuncRow};

fn compile(db: &WastDb) -> Vec<u8> {
    wast_compiler::compile(db, "").expect("compile ok")
}

fn load(wasm: &[u8]) -> (Engine, Component) {
    let engine = Engine::new(&Config::new()).unwrap();
    let component = Component::from_binary(&engine, wasm).expect("component load");
    (engine, component)
}

fn single_export(
    name: &str,
    params: Vec<(&str, &str)>,
    result: Option<&str>,
    body: Vec<Instruction>,
) -> WastDb {
    WastDb {
        funcs: vec![WastFuncRow {
            uid: name.into(),
            func: WastFunc {
                source: FuncSource::Exported(name.into()),
                params: params
                    .into_iter()
                    .map(|(n, t)| (n.into(), t.into()))
                    .collect(),
                result: result.map(|s| s.into()),
                body: Some(serialize_body(&body)),
            },
        }],
        types: vec![],
    }
}

#[test]
fn u32_add() {
    let db = single_export(
        "add",
        vec![("a", "u32"), ("b", "u32")],
        Some("u32"),
        vec![Instruction::Arithmetic {
            op: ArithOp::Add,
            lhs: Box::new(Instruction::LocalGet { uid: "a".into() }),
            rhs: Box::new(Instruction::LocalGet { uid: "b".into() }),
        }],
    );
    let (engine, component) = load(&compile(&db));
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(u32, u32), (u32,)>(&mut store, "add")
        .unwrap();
    assert_eq!(func.call(&mut store, (7, 35)).unwrap(), (42,));
}

#[test]
fn i64_mul() {
    let db = single_export(
        "mul",
        vec![("a", "i64"), ("b", "i64")],
        Some("i64"),
        vec![Instruction::Arithmetic {
            op: ArithOp::Mul,
            lhs: Box::new(Instruction::LocalGet { uid: "a".into() }),
            rhs: Box::new(Instruction::LocalGet { uid: "b".into() }),
        }],
    );
    let (engine, component) = load(&compile(&db));
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(i64, i64), (i64,)>(&mut store, "mul")
        .unwrap();
    assert_eq!(func.call(&mut store, (-6, 7)).unwrap(), (-42,));
}

#[test]
fn u32_div_unsigned() {
    // Unsigned div: 10 / 3 = 3 (u32 uses div_u, not div_s)
    let db = single_export(
        "div",
        vec![("a", "u32"), ("b", "u32")],
        Some("u32"),
        vec![Instruction::Arithmetic {
            op: ArithOp::Div,
            lhs: Box::new(Instruction::LocalGet { uid: "a".into() }),
            rhs: Box::new(Instruction::LocalGet { uid: "b".into() }),
        }],
    );
    let (engine, component) = load(&compile(&db));
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(u32, u32), (u32,)>(&mut store, "div")
        .unwrap();
    assert_eq!(func.call(&mut store, (10, 3)).unwrap(), (3,));
}

#[test]
fn u32_lt_unsigned() {
    // Unsigned lt: 0 < (u32::MAX as i32 == -1 reinterpreted) must be true.
    // If we emit lt_s instead of lt_u this would be false.
    let db = single_export(
        "lt",
        vec![("a", "u32"), ("b", "u32")],
        Some("bool"),
        vec![Instruction::Compare {
            op: CompareOp::Lt,
            lhs: Box::new(Instruction::LocalGet { uid: "a".into() }),
            rhs: Box::new(Instruction::LocalGet { uid: "b".into() }),
        }],
    );
    let (engine, component) = load(&compile(&db));
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(u32, u32), (bool,)>(&mut store, "lt")
        .unwrap();
    assert_eq!(func.call(&mut store, (0, u32::MAX)).unwrap(), (true,));
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (5, 5)).unwrap(), (false,));
    func.post_return(&mut store).unwrap();
}

#[test]
fn i32_eq_const() {
    // Const needs to be inferred as i32 from the LocalGet(x) sibling.
    let db = single_export(
        "is-zero",
        vec![("x", "i32")],
        Some("bool"),
        vec![Instruction::Compare {
            op: CompareOp::Eq,
            lhs: Box::new(Instruction::LocalGet { uid: "x".into() }),
            rhs: Box::new(Instruction::Const { value: 0 }),
        }],
    );
    let (engine, component) = load(&compile(&db));
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(i32,), (bool,)>(&mut store, "is-zero")
        .unwrap();
    assert_eq!(func.call(&mut store, (0,)).unwrap(), (true,));
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (1,)).unwrap(), (false,));
    func.post_return(&mut store).unwrap();
}

#[test]
fn f32_sub() {
    let db = single_export(
        "sub",
        vec![("a", "f32"), ("b", "f32")],
        Some("f32"),
        vec![Instruction::Arithmetic {
            op: ArithOp::Sub,
            lhs: Box::new(Instruction::LocalGet { uid: "a".into() }),
            rhs: Box::new(Instruction::LocalGet { uid: "b".into() }),
        }],
    );
    let (engine, component) = load(&compile(&db));
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(f32, f32), (f32,)>(&mut store, "sub")
        .unwrap();
    assert_eq!(func.call(&mut store, (3.5, 1.25)).unwrap(), (2.25,));
}
