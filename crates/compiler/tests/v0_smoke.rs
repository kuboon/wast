//! v0 smoke test: compile a WASI CLI empty-run component and execute it
//! end-to-end via wasmtime. Success = `wasi:cli/run/run` returns `Ok(())`.

use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::bindings::Command;
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiView};
use wast_types::WastDb;

struct Ctx {
    table: wasmtime_wasi::ResourceTable,
    wasi: WasiCtx,
}

impl WasiView for Ctx {
    fn table(&mut self) -> &mut wasmtime_wasi::ResourceTable {
        &mut self.table
    }
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }
}

#[tokio::test(flavor = "current_thread")]
async fn wasi_cli_empty_run_exits_ok() {
    let db = WastDb {
        funcs: vec![],
        types: vec![],
    };

    let wasm = wast_compiler::compile(&db, "").expect("compile should succeed");

    let mut config = Config::new();
    config.async_support(true);
    let engine = Engine::new(&config).unwrap();
    let component = Component::from_binary(&engine, &wasm).expect("component must load");

    let mut linker: Linker<Ctx> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker).unwrap();

    let ctx = Ctx {
        table: Default::default(),
        wasi: WasiCtxBuilder::new().build(),
    };
    let mut store = Store::new(&engine, ctx);

    let command = Command::instantiate_async(&mut store, &component, &linker)
        .await
        .expect("instantiate");

    let result = command
        .wasi_cli_run()
        .call_run(&mut store)
        .await
        .expect("call run");

    assert!(result.is_ok(), "run() should return Ok(())");
}
