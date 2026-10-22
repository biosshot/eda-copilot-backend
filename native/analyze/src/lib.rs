//! Standalone CPU/F64 board import, copper, mesh and DC. No Node, placement or GPU.
pub mod conduction;
pub mod dc;
pub mod geometry;
pub mod input;
pub mod model;

pub use input::{import_board, import_json, Diagnostic, ImportReport, Severity};
