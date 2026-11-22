//! Independent integral references. Never compiled into the production library.
#![allow(dead_code)]
pub use pcb_analyze::extraction::local::{mesh,polyhedron,prism,volume_current,volume_magnetic,AllocationBudget,Result};
fn error(code:&str,detail:&str)->String {format!("LOCAL_{code}: {detail}")}
#[path="../../../src/extraction/local/accurate.rs"] mod accurate;
#[path="../../../src/extraction/local/compensated.rs"] mod compensated;
pub mod volume_integral;
mod volume_integral_cache;
mod volume_surface;
mod volume_surface_compensated;
pub mod volume_matrix;
