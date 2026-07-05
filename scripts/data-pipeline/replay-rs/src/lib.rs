//! Library surface: pure replay analysis (`analyze`) + shared AoE2/API constants
//! (`config`). All IO — network, file reads, terminal rendering — lives in the
//! binary. This boundary is what the future WASM build compiles.
pub mod analyze;
pub mod config;
pub mod postgame;
