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
