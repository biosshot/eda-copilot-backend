//! Standalone board import library. No Node, placement, GPU or solver dependencies.
pub mod input;
pub mod model;

pub use input::{import_board, import_json, Diagnostic, ImportReport, Severity};
