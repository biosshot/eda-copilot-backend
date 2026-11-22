//! The only RoutingBoardV2 boundary. Validate first, then resolve references.
mod convert;
pub mod dto;
mod validate;

use crate::model::Board;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Limitation,
    Missing,
}
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub path: String,
    pub source_id: Option<String>,
    pub message: String,
}
#[derive(Debug, Default, Clone, Serialize)]
pub struct Accepted {
    pub layers: usize,
    pub nets: usize,
    pub components: usize,
    pub pads: usize,
    pub tracks: usize,
    pub vias: usize,
    pub zones: usize,
    pub keepouts: usize,
    pub drills: usize,
    pub available_fills: usize,
    pub unavailable_fills: usize,
}
#[derive(Debug)]
pub struct ImportReport {
    /// None on any error: no partially accepted board can reach later preparation.
    pub board: Option<Board>,
    pub accepted: Accepted,
    pub diagnostics: Vec<Diagnostic>,
}
impl ImportReport {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

pub fn import_json(json: &str) -> ImportReport {
    let mut deserializer = serde_json::Deserializer::from_str(json);
    let parsed = serde_path_to_error::deserialize::<_, dto::RoutingBoardV2>(&mut deserializer);
    match parsed {
        Ok(board) => match deserializer.end() {
            Ok(()) => import_board(board),
            Err(e) => parse_error("$".into(), e.to_string()),
        },
        Err(e) => parse_error(e.path().to_string(), e.inner().to_string()),
    }
}
fn parse_error(path: String, message: String) -> ImportReport {
    ImportReport {
        board: None,
        accepted: Accepted::default(),
        diagnostics: vec![Diagnostic {
            severity: Severity::Error,
            code: "invalid_structure",
            path,
            source_id: None,
            message,
        }],
    }
}
pub fn import_board(board: dto::RoutingBoardV2) -> ImportReport {
    let diagnostics = validate::validate(&board);
    if diagnostics.iter().any(|d| d.severity == Severity::Error) {
        return ImportReport {
            board: None,
            accepted: Accepted::default(),
            diagnostics,
        };
    }
    let board = convert::convert(board);
    let mut accepted = Accepted {
        layers: board.layers.len(),
        nets: board.nets.len(),
        components: board.components.len(),
        pads: board.pads.len(),
        tracks: board.tracks.len(),
        vias: board.vias.len(),
        zones: board.zones.len(),
        keepouts: board.keepouts.len(),
        drills: board.drills.as_ref().map_or(0, Vec::len),
        ..Accepted::default()
    };
    for z in &board.zones {
        for fill in &z.fills {
            match fill.state {
                crate::model::CadFillState::Available { .. } => accepted.available_fills += 1,
                crate::model::CadFillState::Unavailable { .. } => accepted.unavailable_fills += 1,
            }
        }
    }
    ImportReport {
        board: Some(board),
        accepted,
        diagnostics,
    }
}
