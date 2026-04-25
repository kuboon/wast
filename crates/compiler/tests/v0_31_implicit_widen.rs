//! v0.31: implicit widen at LocalGet. When a single-slot primitive flows
//! into a wider consumer (e.g. `LocalGet(u32)` inside an `Arithmetic` whose
//! expected type is `u64`), the compiler now emits the appropriate
//! `i64.extend_i32_{u,s}` (or `f64.promote_f32`) automatically.

use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wast_pattern_analyzer::{ArithOp, Instruction, serialize_body};
use wast_types::{FuncSource, WastDb, WastFunc, WastFuncRow};

fn load(db: &WastDb) -> (Engine, Component) {
    let wasm = wast_compiler::compile(db, "").expect("compile ok");
    let engine = Engine::new(&Config::new()).unwrap();
    let component = Component::from_binary(&engine, &wasm).expect("component load");
    (engine, component)
}

#[test]
fn widen_u32_to_u64_via_arith() {
    // promote(x: u32) -> u64  { x + 0u64 }
    //
    // Arithmetic propagates the u64 expected down to its operands. The
    // u32 LocalGet(x) used to produce an i32 value and fail validation
    // against the i64.add op; now it emits `i64.extend_i32_u`.
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "promote".into(),
            func: WastFunc {
                source: FuncSource::Exported("promote".into()),
                params: vec![("x".into(), "u32".into())],
                result: Some("u64".into()),
                body: Some(serialize_body(&[Instruction::Arithmetic {
                    op: ArithOp::Add,
                    lhs: Box::new(Instruction::LocalGet { uid: "x".into() }),
                    rhs: Box::new(Instruction::Const { value: 0 }),
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
        .get_typed_func::<(u32,), (u64,)>(&mut store, "promote")
        .unwrap();
    assert_eq!(func.call(&mut store, (0u32,)).unwrap(), (0u64,));
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (42u32,)).unwrap(), (42u64,));
    func.post_return(&mut store).unwrap();
    // High bit set — confirm zero-extend (not sign-extend) for unsigned.
    assert_eq!(
        func.call(&mut store, (0x8000_0001u32,)).unwrap(),
        (0x8000_0001u64,)
    );
    func.post_return(&mut store).unwrap();
}

#[test]
fn widen_i32_signed_to_i64_via_arith() {
    // promote-s(x: s32) -> s64  { x + 0i64 }
    //
    // For signed source types the widen is `i64.extend_i32_s` so a
    // negative value preserves its sign in the wider type.
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "promote_s".into(),
            func: WastFunc {
                source: FuncSource::Exported("promote-s".into()),
                params: vec![("x".into(), "i32".into())],
                result: Some("i64".into()),
                body: Some(serialize_body(&[Instruction::Arithmetic {
                    op: ArithOp::Add,
                    lhs: Box::new(Instruction::LocalGet { uid: "x".into() }),
                    rhs: Box::new(Instruction::Const { value: 0 }),
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
        .get_typed_func::<(i32,), (i64,)>(&mut store, "promote-s")
        .unwrap();
    assert_eq!(func.call(&mut store, (-1i32,)).unwrap(), (-1i64,));
    func.post_return(&mut store).unwrap();
    assert_eq!(
        func.call(&mut store, (i32::MIN,)).unwrap(),
        (i32::MIN as i64,)
    );
    func.post_return(&mut store).unwrap();
}

#[test]
fn widen_at_return_position_directly() {
    // identity-wide(x: u32) -> u64  { x }
    //
    // Body's last instruction is a bare LocalGet(x: u32). The function's
    // return type is u64, so emit_body propagates expected=u64 and the
    // LocalGet emits a widening `i64.extend_i32_u`.
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "identity_wide".into(),
            func: WastFunc {
                source: FuncSource::Exported("identity-wide".into()),
                params: vec![("x".into(), "u32".into())],
                result: Some("u64".into()),
                body: Some(serialize_body(&[Instruction::LocalGet { uid: "x".into() }])),
            },
        }],
        types: vec![],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(u32,), (u64,)>(&mut store, "identity-wide")
        .unwrap();
    assert_eq!(
        func.call(&mut store, (u32::MAX,)).unwrap(),
        (u32::MAX as u64,)
    );
    func.post_return(&mut store).unwrap();
}
