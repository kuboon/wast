//! v0.10 spike: prove `wit-component` can wrap a hand-written core module
//! into a Canonical-ABI-conformant component, replacing our hand-rolled
//! `(component …)` outer shell + `canon lift` emission.
//!
//! If this works end-to-end, future milestones can stop emitting the outer
//! component WAT entirely and lean on `wit_component::ComponentEncoder` for
//! canon lift/lower, `(memory $m …)` options, and (eventually) the
//! string/list/record lowering we'd otherwise hand-write.

use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wit_component::{ComponentEncoder, StringEncoding, embed_component_metadata};
use wit_parser::Resolve;

/// Core-only WAT for `identity(x: u32) -> u32`. Exports `memory` and
/// `cabi_realloc` as the Component Model expects. `wit-component` handles
/// the `canon lift` / outer `(component …)` shell.
const CORE_WAT: &str = r#"
(module
  (memory (export "memory") 1)
  (global $heap_end (mut i32) (i32.const 1024))
  (func $cabi_realloc (export "cabi_realloc")
    (param $orig_ptr i32) (param $orig_size i32) (param $align i32) (param $new_size i32)
    (result i32)
    (local $aligned i32)
    global.get $heap_end
    local.get $align
    i32.const 1
    i32.sub
    i32.add
    local.get $align
    i32.const 1
    i32.sub
    i32.const -1
    i32.xor
    i32.and
    local.tee $aligned
    local.get $new_size
    i32.add
    global.set $heap_end
    local.get $orig_size
    if
      local.get $aligned
      local.get $orig_ptr
      local.get $orig_size
      memory.copy
    end
    local.get $aligned
  )
  (func (export "identity") (param i32) (result i32)
    local.get 0
  )
)
"#;

const WORLD_WIT: &str = r#"
package example:foo@0.1.0;

world t {
  export identity: func(x: u32) -> u32;
}
"#;

#[test]
fn wit_component_wraps_identity() {
    // 1. Core module → bytes
    let mut core_bytes = wat::parse_str(CORE_WAT).expect("core wat parse");

    // 2. Parse WIT, locate the world
    let mut resolve = Resolve::default();
    let pkg = resolve
        .push_str("world.wit", WORLD_WIT)
        .expect("wit push_str");
    let world = resolve.select_world(pkg, Some("t")).expect("select world");

    // 3. Embed `component-type` custom section so ComponentEncoder knows
    //    which world to wrap around this core module.
    embed_component_metadata(&mut core_bytes, &resolve, world, StringEncoding::UTF8)
        .expect("embed metadata");

    // 4. Wrap the core module into a component binary.
    let component_bytes = ComponentEncoder::default()
        .validate(true)
        .module(&core_bytes)
        .expect("attach module")
        .encode()
        .expect("encode component");

    // 5. Execute via wasmtime just like our existing tests.
    let engine = Engine::new(&Config::new()).unwrap();
    let component = Component::from_binary(&engine, &component_bytes).expect("component load");

    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker
        .instantiate(&mut store, &component)
        .expect("instantiate");
    let func = instance
        .get_typed_func::<(u32,), (u32,)>(&mut store, "identity")
        .expect("typed func");

    assert_eq!(func.call(&mut store, (42,)).unwrap(), (42,));
    func.post_return(&mut store).unwrap();
}

/// Follow-up spike: does wit-component also wire up an indirect-return
/// compound correctly? If so, our v0.8 hand-rolled `Some`/`Ok`/`Err` logic
/// becomes an implementation detail of the core body — wit-component handles
/// the component-layer `canon lift` including the return-area convention.
const CORE_WAT_MK_SOME: &str = r#"
(module
  (memory (export "memory") 1)
  (global $heap_end (mut i32) (i32.const 1024))
  (func $cabi_realloc (export "cabi_realloc")
    (param $orig_ptr i32) (param $orig_size i32) (param $align i32) (param $new_size i32)
    (result i32)
    (local $aligned i32)
    global.get $heap_end
    local.get $align
    i32.const 1
    i32.sub
    i32.add
    local.get $align
    i32.const 1
    i32.sub
    i32.const -1
    i32.xor
    i32.and
    local.tee $aligned
    local.get $new_size
    i32.add
    global.set $heap_end
    local.get $orig_size
    if
      local.get $aligned
      local.get $orig_ptr
      local.get $orig_size
      memory.copy
    end
    local.get $aligned
  )
  ;; Core signature matches what Canonical ABI (MAX_FLAT_RESULTS=1) expects:
  ;; indirect return — take x, produce a buffer pointer to [disc|pad|payload].
  (func (export "mk-some") (param $x i32) (result i32)
    (local $ret i32)
    i32.const 0      ;; orig_ptr
    i32.const 0      ;; orig_size
    i32.const 4      ;; align
    i32.const 8      ;; size
    call $cabi_realloc
    local.set $ret
    local.get $ret
    i32.const 1      ;; disc = 1 (some)
    i32.store8 offset=0
    local.get $ret
    local.get $x
    i32.store offset=4 align=2
    local.get $ret
  )
)
"#;

const WORLD_WIT_MK_SOME: &str = r#"
package example:foo@0.1.0;

world t {
  export mk-some: func(x: u32) -> option<u32>;
}
"#;

#[test]
fn wit_component_wraps_option_return() {
    let mut core_bytes = wat::parse_str(CORE_WAT_MK_SOME).expect("core wat parse");

    let mut resolve = Resolve::default();
    let pkg = resolve
        .push_str("world.wit", WORLD_WIT_MK_SOME)
        .expect("wit push_str");
    let world = resolve.select_world(pkg, Some("t")).expect("select world");

    embed_component_metadata(&mut core_bytes, &resolve, world, StringEncoding::UTF8)
        .expect("embed metadata");

    let component_bytes = ComponentEncoder::default()
        .validate(true)
        .module(&core_bytes)
        .expect("attach module")
        .encode()
        .expect("encode component");

    let engine = Engine::new(&Config::new()).unwrap();
    let component = Component::from_binary(&engine, &component_bytes).expect("component load");
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker
        .instantiate(&mut store, &component)
        .expect("instantiate");
    let func = instance
        .get_typed_func::<(u32,), (Option<u32>,)>(&mut store, "mk-some")
        .expect("typed func");

    assert_eq!(func.call(&mut store, (42,)).unwrap(), (Some(42),));
    func.post_return(&mut store).unwrap();
}
