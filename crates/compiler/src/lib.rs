//! wast → wasm Component compiler.
//!
//! See `PLAN.md` for the full roadmap. v0.11 onward, we emit only a core
//! module and delegate the outer component wrapping (canon lift/lower,
//! memory options, custom sections) to `wit_component::ComponentEncoder`.

mod core_emit;
mod emit;
mod error;

pub use error::CompileError;

/// Compile a `WastDb` + `world.wit` pair into a WASM Component binary.
pub fn compile(db: &wast_types::WastDb, world_wit: &str) -> Result<Vec<u8>, CompileError> {
    emit::compile_component(db, world_wit)
}

/// Compile a `WastDb` into a bare **core** wasm module.
///
/// The Component Model binary `compile` produces needs a host that
/// understands it (jco, wasmtime). This is the module underneath it, which a
/// browser can instantiate as-is: no imports for an import-free program, its
/// own `memory`, and `cabi_realloc` defined rather than imported.
pub fn compile_core(db: &wast_types::WastDb) -> Result<Vec<u8>, CompileError> {
    emit::compile_core_module(db)
}

/// The core module WAT the compiler generates, before it is assembled to
/// bytes — the most direct view of what the emitter decided to do.
pub fn emit_wat(db: &wast_types::WastDb) -> Result<String, CompileError> {
    emit::emit_core_wat(db)
}
