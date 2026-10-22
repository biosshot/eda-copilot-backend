//! Explicit DC material resolution. All conversions to SI are centralized here.
use super::{issue, ConductionReport};
use crate::model::{Board, StackLayer};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub fn metres(mm: f64) -> f64 {
    mm * 1e-3
}
pub fn square_metres(mm2: f64) -> f64 {
    mm2 * 1e-6
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Material {
    pub conductivity_s_m: f64,
    pub state: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum StackEntry {
    Copper { layer: String, thickness_mm: f64 },
    Dielectric { name: String, thickness_mm: f64 },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiameterConvention {
    FinishedHole,
    BeforePlating,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BarrelProfile {
    pub plating_thickness_mm: Option<f64>,
    pub material: Option<Material>,
    pub diameter_convention: Option<DiameterConvention>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PhysicalProfile {
    pub stackup: Option<Vec<StackEntry>>,
    pub materials: BTreeMap<String, Material>,
    pub barrel: Option<BarrelProfile>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedLayer {
    pub layer: String,
    pub thickness_mm: Option<f64>,
    pub z_mm: Option<f64>,
    pub geometry_origin: Option<String>,
    pub material: Option<Material>,
    pub material_origin: Option<String>,
    pub sheet_conductance_s: Option<f64>,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct ResolvedBarrel {
    pub plating_thickness_mm: Option<f64>,
    pub plating_origin: Option<String>,
    pub material: Option<Material>,
    pub material_origin: Option<String>,
    pub diameter_convention: Option<DiameterConvention>,
    pub diameter_convention_origin: Option<String>,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct ResolvedProfile {
    pub layers: Vec<ResolvedLayer>,
    pub barrel: ResolvedBarrel,
}
fn positive(v: f64) -> bool {
    v.is_finite() && v > 0. && v <= 1e12
}
fn material_valid(m: &Material) -> bool {
    positive(m.conductivity_s_m) && !m.state.trim().is_empty()
}
pub fn resolve(board: &Board, input: Option<&PhysicalProfile>, r: &mut ConductionReport) -> bool {
    let empty = PhysicalProfile::default();
    let p = input.unwrap_or(&empty);
    let mut valid = true;
    for (name, m) in &p.materials {
        if !board.layers.iter().any(|l| &l.name == name) || !material_valid(m) {
            issue(
                r,
                "invalid_material",
                &format!("profile.materials.{name}"),
                None,
                "Unknown layer, nonpositive/nonfinite conductivity or missing material state",
                "invalid",
            );
            valid = false;
        }
    }
    let board_entries: Vec<_> = board
        .stackup
        .as_ref()
        .map(|s| {
            s.layers
                .iter()
                .map(|l| match l {
                    StackLayer::Copper {
                        layer,
                        thickness_mm,
                    } => StackEntry::Copper {
                        layer: board.layers[*layer].name.clone(),
                        thickness_mm: *thickness_mm,
                    },
                    StackLayer::Dielectric {
                        name, thickness_mm, ..
                    } => StackEntry::Dielectric {
                        name: name.clone().unwrap_or_default(),
                        thickness_mm: *thickness_mm,
                    },
                })
                .collect()
        })
        .unwrap_or_default();
    let entries = p.stackup.as_ref().unwrap_or(&board_entries);
    let origin = if p.stackup.is_some() {
        "profile"
    } else {
        "board"
    };
    let mut resolved = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut last = None;
    let mut z = 0.;
    let mut previous_copper = false;
    for (i, entry) in entries.iter().enumerate() {
        let path = format!("{origin}.stackup[{i}]");
        let thickness = match entry {
            StackEntry::Copper { thickness_mm, .. }
            | StackEntry::Dielectric { thickness_mm, .. } => *thickness_mm,
        };
        if !positive(thickness) || thickness > 100. {
            issue(
                r,
                "invalid_stackup",
                &path,
                None,
                "Thickness must be finite, positive and <=100 mm",
                "invalid",
            );
            valid = false;
            continue;
        }
        if let StackEntry::Copper { layer, .. } = entry {
            if previous_copper {
                issue(
                    r,
                    "invalid_stackup",
                    &path,
                    None,
                    "Distinct copper layers require an explicit positive dielectric separation",
                    "invalid",
                );
                valid = false;
            }
            let index = board.layers.iter().position(|l| &l.name == layer);
            if index.is_none()
                || !seen.insert(layer.clone())
                || last.is_some_and(|a| a >= index.unwrap_or(0))
            {
                issue(
                    r,
                    "invalid_stackup",
                    &path,
                    None,
                    "Copper layer must exist, be unique and follow physical board order",
                    "invalid",
                );
                valid = false;
            } else {
                last = index;
                resolved.insert(layer.clone(), (thickness, z + thickness / 2.));
            }
        }
        previous_copper = matches!(entry, StackEntry::Copper { .. });
        z += thickness;
    }
    // Board thickness is evidence only for a complete board-owned stack, never a guessed dielectric gap.
    if p.stackup.is_none() && seen.len() == board.layers.len() && !entries.is_empty() {
        if let Some(total) = board.stackup.as_ref().and_then(|s| s.board_thickness_mm) {
            if (total - z).abs() > 1e-6 {
                issue(
                    r,
                    "stackup_thickness_mismatch",
                    "board.stackup",
                    None,
                    "Stack thickness differs from declared board thickness",
                    "invalid",
                );
                valid = false;
            }
        }
    }
    let complete_order = seen.len() == board.layers.len();
    for layer in &board.layers {
        let data = resolved.get(&layer.name);
        let material = p.materials.get(&layer.name).cloned();
        let sheet = data
            .zip(material.as_ref())
            .map(|((t, _), m)| metres(*t) * m.conductivity_s_m);
        if sheet.is_some_and(|s| !s.is_finite() || s <= 0.) {
            issue(
                r,
                "invalid_sheet_coefficient",
                &format!("layers.{}", layer.name),
                None,
                "Resolved sheet coefficient is nonfinite or underflows to zero in F64/SI",
                "invalid",
            );
            valid = false;
        }
        if data.is_none() || material.is_none() {
            issue(
                r,
                "missing_layer_physics",
                &format!("layers.{}", layer.name),
                None,
                "Explicit copper thickness and conductivity/material state required",
                "missing",
            );
        }
        r.profile.layers.push(ResolvedLayer {
            layer: layer.name.clone(),
            thickness_mm: data.map(|d| d.0),
            z_mm: if complete_order {
                data.map(|d| d.1)
            } else {
                None
            },
            geometry_origin: data.map(|_| origin.into()),
            material_origin: material.as_ref().map(|_| "profile".into()),
            material,
            sheet_conductance_s: sheet,
        });
    }
    let b = p.barrel.clone().unwrap_or_default();
    let board_plating = board
        .stackup
        .as_ref()
        .and_then(|s| s.via_plating_thickness_um)
        .map(|v| v / 1000.);
    let plating = b.plating_thickness_mm.or(board_plating);
    if plating.is_some_and(|v| !positive(v) || v > 10.)
        || b.material.as_ref().is_some_and(|m| !material_valid(m))
    {
        issue(
            r,
            "invalid_barrel_profile",
            "profile.barrel",
            None,
            "Plating and conductivity must be positive finite; material state required",
            "invalid",
        );
        valid = false;
    }
    r.profile.barrel = ResolvedBarrel {
        plating_thickness_mm: plating,
        plating_origin: plating.map(|_| {
            if b.plating_thickness_mm.is_some() {
                "profile".into()
            } else {
                "board".into()
            }
        }),
        material_origin: b.material.as_ref().map(|_| "profile".into()),
        material: b.material,
        diameter_convention_origin: b.diameter_convention.as_ref().map(|_| "profile".into()),
        diameter_convention: b.diameter_convention,
    };
    if let Some((t, convention)) = r
        .profile
        .barrel
        .plating_thickness_mm
        .zip(r.profile.barrel.diameter_convention.clone())
    {
        let pads = board.pads.iter().filter_map(|p| {
            p.drill
                .as_ref()
                .filter(|d| d.plated && matches!(d.shape, crate::model::DrillShape::Round))
                .map(|d| (&p.source, d.diameter_mm))
        });
        let vias = board.vias.iter().map(|v| (&v.source, v.drill_mm));
        let holes = board
            .drills
            .iter()
            .flatten()
            .filter(|d| d.drill.plated && matches!(d.drill.shape, crate::model::DrillShape::Round))
            .map(|d| (&d.source, d.drill.diameter_mm));
        for (source, diameter) in pads.chain(vias).chain(holes) {
            if barrel_area_mm2(diameter, t, &convention).is_none() {
                issue(
                    r,
                    "invalid_barrel_cross_section",
                    &source.path,
                    source.id.clone(),
                    "Plating produces a nonpositive or unrepresentable F64 barrel cross-section",
                    "invalid",
                );
                valid = false;
            }
        }
    }
    valid
}
pub fn barrel_area_mm2(d: f64, t: f64, convention: &DiameterConvention) -> Option<f64> {
    let (inner, outer) = match convention {
        DiameterConvention::FinishedHole => (d / 2., d / 2. + t),
        DiameterConvention::BeforePlating => (d / 2. - t, d / 2.),
    };
    let area = std::f64::consts::PI * (outer * outer - inner * inner);
    (inner > 0. && outer.is_finite() && area.is_finite() && area > 0.).then_some(area)
}
