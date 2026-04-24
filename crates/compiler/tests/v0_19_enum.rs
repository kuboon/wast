//! v0.19 enum: payload-less variant. Canonical ABI stores just the u8
//! discriminant (for ≤256 cases). Flat = single i32 slot — no indirect
//! return needed, and enums compose with existing MatchVariant machinery.

use wasmtime::component::{Component, ComponentType, Lift, Linker, Lower};
use wasmtime::{Config, Engine, Store};
use wast_pattern_analyzer::{Instruction, MatchArm, serialize_body};
use wast_types::{
    FuncSource, TypeSource, WastDb, WastFunc, WastFuncRow, WastTypeDef, WastTypeRow, WitType,
};

#[derive(ComponentType, Lower, Lift, Clone, Copy, Debug, PartialEq)]
#[component(enum)]
#[repr(u8)]
enum Color {
    #[component(name = "red")]
    Red,
    #[component(name = "green")]
    Green,
    #[component(name = "blue")]
    Blue,
}

fn load(db: &WastDb) -> (Engine, Component) {
    let wasm = wast_compiler::compile(db, "").expect("compile ok");
    let engine = Engine::new(&Config::new()).unwrap();
    let component = Component::from_binary(&engine, &wasm).expect("component load");
    (engine, component)
}

fn color_type_row() -> WastTypeRow {
    WastTypeRow {
        uid: "color".into(),
        def: WastTypeDef {
            source: TypeSource::Internal("color".into()),
            definition: WitType::Enum(vec!["red".into(), "green".into(), "blue".into()]),
        },
    }
}

#[test]
fn enum_constructor() {
    // mk-green() -> color  { green }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "mk_green".into(),
            func: WastFunc {
                source: FuncSource::Exported("mk-green".into()),
                params: vec![],
                result: Some("color".into()),
                body: Some(serialize_body(&[Instruction::VariantCtor {
                    case: "green".into(),
                    value: None,
                }])),
            },
        }],
        types: vec![color_type_row()],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(), (Color,)>(&mut store, "mk-green")
        .unwrap();
    assert_eq!(func.call(&mut store, ()).unwrap(), (Color::Green,));
    func.post_return(&mut store).unwrap();
}

#[test]
fn enum_match_dispatch() {
    // brightness(c: color) -> u32
    //   match c { red => 1, green => 2, blue => 4 }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "brightness".into(),
            func: WastFunc {
                source: FuncSource::Exported("brightness".into()),
                params: vec![("c".into(), "color".into())],
                result: Some("u32".into()),
                body: Some(serialize_body(&[Instruction::MatchVariant {
                    value: Box::new(Instruction::LocalGet { uid: "c".into() }),
                    arms: vec![
                        MatchArm {
                            case: "red".into(),
                            binding: None,
                            body: vec![Instruction::Const { value: 1 }],
                        },
                        MatchArm {
                            case: "green".into(),
                            binding: None,
                            body: vec![Instruction::Const { value: 2 }],
                        },
                        MatchArm {
                            case: "blue".into(),
                            binding: None,
                            body: vec![Instruction::Const { value: 4 }],
                        },
                    ],
                }])),
            },
        }],
        types: vec![color_type_row()],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(Color,), (u32,)>(&mut store, "brightness")
        .unwrap();
    assert_eq!(func.call(&mut store, (Color::Red,)).unwrap(), (1,));
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (Color::Green,)).unwrap(), (2,));
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (Color::Blue,)).unwrap(), (4,));
    func.post_return(&mut store).unwrap();
}
