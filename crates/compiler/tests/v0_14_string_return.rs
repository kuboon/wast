//! v0.14 test: returning `string` from an exported function.
//! Uses indirect return (flat=2 > MAX_FLAT_RESULTS=1). The body's last
//! instruction (LocalGet of a string param, or StringLiteral) is wrapped:
//! allocate 8 bytes via cabi_realloc, write (ptr, len), return buffer ptr.

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
fn echo_string_passthrough() {
    // echo(s: string) -> string  { s }
    //
    // The input bytes are already in our memory (host wrote them via our
    // cabi_realloc before the call). We just copy (ptr, len) into the
    // return area — no memcpy needed.
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "echo".into(),
            func: WastFunc {
                source: FuncSource::Exported("echo".into()),
                params: vec![("s".into(), "string".into())],
                result: Some("string".into()),
                body: Some(serialize_body(&[Instruction::LocalGet { uid: "s".into() }])),
            },
        }],
        types: vec![],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(&str,), (String,)>(&mut store, "echo")
        .unwrap();
    assert_eq!(
        func.call(&mut store, ("hello",)).unwrap(),
        ("hello".to_string(),)
    );
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, ("",)).unwrap(), ("".to_string(),));
    func.post_return(&mut store).unwrap();
    assert_eq!(
        func.call(&mut store, ("日本語",)).unwrap(),
        ("日本語".to_string(),)
    );
    func.post_return(&mut store).unwrap();
}

#[test]
fn greeting_from_literal() {
    // greeting() -> string  { "hello, wast!" }
    //
    // Bytes live in a data segment at a fixed offset; we write (offset, len)
    // into the return area and return the buffer pointer. Host reads the
    // 12 bytes from our memory and decodes as UTF-8.
    let db = WastDb {
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
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(), (String,)>(&mut store, "greeting")
        .unwrap();
    assert_eq!(
        func.call(&mut store, ()).unwrap(),
        ("hello, wast!".to_string(),)
    );
    func.post_return(&mut store).unwrap();
}

#[test]
fn literal_return_multibyte() {
    // jp_greeting() -> string  { "こんにちは" }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "jp_greeting".into(),
            func: WastFunc {
                source: FuncSource::Exported("jp-greeting".into()),
                params: vec![],
                result: Some("string".into()),
                body: Some(serialize_body(&[Instruction::StringLiteral {
                    bytes: "こんにちは".as_bytes().to_vec(),
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
        .get_typed_func::<(), (String,)>(&mut store, "jp-greeting")
        .unwrap();
    assert_eq!(
        func.call(&mut store, ()).unwrap(),
        ("こんにちは".to_string(),)
    );
    func.post_return(&mut store).unwrap();
}
