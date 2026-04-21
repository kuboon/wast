//! wast → wasm Component compiler.
//!
//! See `PLAN.md` for the full roadmap. v0.1 compiles `WastDb` funcs with
//! primitive WIT types into Component WAT and converts to a `.wasm` binary
//! via the `wat` crate.

mod core_emit;
mod emit;
mod error;

pub use error::CompileError;

/// Compile a `WastDb` + `world.wit` pair into a WASM Component binary.
pub fn compile(db: &wast_types::WastDb, world_wit: &str) -> Result<Vec<u8>, CompileError> {
    let wat = emit::compile_component(db, world_wit)?;
    wat::parse_str(&wat).map_err(|e| CompileError::WatParse(format!("{e}\n--- WAT ---\n{wat}")))
}
