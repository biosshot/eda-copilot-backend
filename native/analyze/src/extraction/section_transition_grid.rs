//! Internal exact-grid series extraction on one frozen physical polygon mesh.
#![allow(dead_code)] // The public transport and its acceptance are separate.
use super::{error, polygon_section, section_transition, Result};
use serde_json::{json, Value};
use std::time::Instant;

const MAX_COLUMNS: usize = 256;

pub(super) fn solve(
    request: &polygon_section::Request,
    conductivities: &[(&str, f64)],
    frequencies_hz: &[f64],
    start: Instant,
) -> Result<Value> {
    let loops = request.conductors.len().saturating_sub(1);
    if !(1..=16).contains(&loops)
        || frequencies_hz.is_empty()
        || frequencies_hz.len().saturating_mul(loops) > MAX_COLUMNS
        || frequencies_hz.iter().any(|&f| !f.is_finite() || f < 0.)
        || frequencies_hz.windows(2).any(|f| f[0] >= f[1])
    {
        return Err(error(
            "SECTION_TRANSITION_GRID",
            "strictly increasing finite nonnegative axis and at most 256 physical loop columns required",
        ));
    }
    let report_bytes = report_plan(request, frequencies_hz.len(), loops)?;
    let prepared = polygon_section::transition_mesh(request, conductivities, start)?;
    let mesh = prepared.as_mesh();
    let budget = section_transition::Budget {
        max_bytes: prepared
            .budget
            .max_bytes
            .checked_sub(report_bytes)
            .ok_or_else(|| {
                error(
                    "SECTION_TRANSITION_MEMORY",
                    "retained mesh and exact-grid output exceed budget",
                )
            })?,
        deadline: prepared.budget.deadline,
    };
    // The first retained mesh node is a deterministic numerical gauge. It
    // does not identify a physical reference conductor or fix its voltage.
    let anchor = 0;
    let reference = mesh.conductors[mesh.reference].id;
    let loop_order: Vec<_> = mesh
        .conductors
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != mesh.reference)
        .map(|(_, c)| json!({"positive":c.id,"negative":reference}))
        .collect();
    let conductor_order: Vec<_> = mesh.conductors.iter().map(|c| c.id).collect();
    let mut triangles_by_conductor = vec![0; mesh.conductors.len()];
    for triangle in mesh.triangles {
        if let Some(i) = triangle.conductor {
            triangles_by_conductor[i] += 1;
        }
    }
    let mut samples = Vec::with_capacity(frequencies_hz.len());
    let mut core_peak = 0;
    for &frequency_hz in frequencies_hz {
        check_deadline(budget.deadline)?;
        let response = section_transition::solve(&mesh, frequency_hz, anchor, &budget)?;
        core_peak = core_peak.max(response.planned_bytes);
        let dc = frequency_hz == 0.;
        samples.push(json!({
            "frequencyHz":frequency_hz,
            "endpoint": if dc {"dc-resistance"} else {"ac-magnetoquasistatic"},
            "seriesImpedanceOhmPerM":response.series_impedance_ohm_per_m,
            "seriesResistanceOhmPerM":response.resistance_ohm_per_m,
            "totalInductanceHPerM":response.total_inductance_h_per_m,
            "conductorLossWPerM":response.conductor_loss_w_per_m,
            "quality": {
                "fieldEvaluated": !dc,
                "maxFieldResidual": (!dc).then_some(response.max_field_residual),
                "maxCurrentResidualA": (!dc).then_some(response.max_current_residual_a),
                "maxReciprocityError": (!dc).then_some(response.max_reciprocity_error),
                "maxComplexPowerError": (!dc).then_some(response.max_complex_power_error),
            },
            "corePlannedBytes":response.planned_bytes,
        }));
    }
    let planned_peak =
        (prepared.retained_bytes + report_bytes + core_peak).max(prepared.meshing_plan_bytes);
    let output = json!({
        "status":"complete",
        "method":"finite-conductivity polygon section, CPU/F64",
        "frequenciesHz":frequencies_hz,
        "loops":loop_order,
        "conductorOrder":conductor_order,
        "conductors":mesh.conductors.iter().zip(&triangles_by_conductor).map(|(c,n)| {
            json!({"id":c.id,"conductivitySM":c.conductivity_s_m,"triangles":n})
        }).collect::<Vec<_>>(),
        "reference":reference,
        "samples":samples,
        "conventions": {
            "seriesMatrix":"[output loop][input loop]; exp(+jωt)",
            "conductorLoss":"[input loop][conductor]; 1 A RMS entering positive conductor",
            "dc":"uniform-current resistance endpoint, magnetic field and inductance unevaluated",
            "seriesResistance":"real symmetric Joule Gram checked against Re(Z)",
            "acInductance":"real symmetric total magnetic Gram checked against Im(Z)/ω, including conductor interior and exterior",
        },
        "mesh": {
            "nodes":mesh.nodes_m.len(),"triangles":mesh.triangles.len(),"numericalAnchorNode":anchor,
            "boundary":"finite homogeneous Neumann magnetic truncation",
            "domainHoleCount":request.domain.holes_m.len(),
            "domainHoleMeaning":"magnetic truncation boundaries; physical material voids belong inside the magnetic air domain",
        },
        "resources": {
            "retainedMeshBytes":prepared.retained_bytes, "reportReserveBytes":report_bytes,
            "meshingPlanBytes":prepared.meshing_plan_bytes, "maxCorePlanBytes":core_peak,
            "combinedPlannedPeakBytes":planned_peak,
        },
        "coverage": {
            "spatial":"unverified","domain":"unverified","band":"unverified",
            "frequency":"exact requested points only",
        },
    });
    check_deadline(budget.deadline)?;
    Ok(output)
}

fn check_deadline(deadline: Instant) -> Result<()> {
    if Instant::now() >= deadline {
        return Err(error(
            "SECTION_TRANSITION_DEADLINE",
            "shared exact-grid deadline exhausted",
        ));
    }
    Ok(())
}

fn report_plan(
    request: &polygon_section::Request,
    frequencies: usize,
    loops: usize,
) -> Result<usize> {
    // Reserve retained JSON and construction copies before any point solve.
    // The conductor IDs are caller-authored strings and are included explicitly.
    let nc = request.conductors.len();
    let ids = request.conductors.iter().try_fold(0_usize, |total, c| {
        total.checked_add(c.id.len().checked_mul(32)?)
    });
    let sample_bytes = loops
        .checked_mul(loops)
        .and_then(|n| n.checked_mul(768))
        .and_then(|n| n.checked_add(loops.checked_mul(nc)?.checked_mul(128)?))
        .and_then(|n| n.checked_add(4096));
    ids.and_then(|ids| sample_bytes?.checked_mul(frequencies)?.checked_add(ids))
        .and_then(|n| n.checked_add(32768))
        .ok_or_else(|| {
            error(
                "SECTION_TRANSITION_MEMORY",
                "exact-grid output byte plan overflow",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use polygon_section::{Contour, MaterialRegion, Metal};

    fn rectangle(x0: f64, y0: f64, x1: f64, y1: f64) -> Contour {
        Contour {
            outer_m: vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
            holes_m: vec![],
        }
    }

    #[test]
    fn polygon_transition_grid_preserves_dc_area_and_checked_ac_order() {
        let domain = rectangle(-0.5e-3, -0.5e-3, 0.5e-3, 0.5e-3);
        let request = polygon_section::Request {
            loss: None,
            domain: domain.clone(),
            conductors: vec![
                Metal {
                    id: "return".into(),
                    role: "reference".into(),
                    contour: rectangle(-0.35e-3, -0.1e-3, -0.15e-3, 0.1e-3),
                },
                Metal {
                    id: "signal".into(),
                    role: "signal".into(),
                    contour: rectangle(0.1e-3, -0.075e-3, 0.35e-3, 0.075e-3),
                },
            ],
            dielectrics: vec![MaterialRegion {
                id: "air".into(),
                relative_permittivity: 1.,
                contour: domain,
            }],
            max_triangle_area_m2: 3e-8,
            max_nodes: 512,
            max_seconds: 20.,
            max_memory_mib: 32,
        };
        let sigma = [("signal", 5e7), ("return", 4e7)];
        let axis = [0., 1000., 3000.];
        let report = solve(&request, &sigma, &axis, Instant::now()).unwrap();
        assert_eq!(report["frequenciesHz"], json!(axis));
        assert_eq!(report["conductorOrder"], json!(["signal", "return"]));
        assert_eq!(
            report["loops"],
            json!([{"positive":"signal","negative":"return"}])
        );
        let samples = report["samples"].as_array().unwrap();
        let dc = &samples[0];
        let expected_signal = 1. / (sigma[0].1 * 0.25e-3 * 0.15e-3);
        let expected_reference = 1. / (sigma[1].1 * 0.2e-3 * 0.2e-3);
        let actual = dc["seriesImpedanceOhmPerM"][0][0]["real"].as_f64().unwrap();
        assert!((actual / (expected_signal + expected_reference) - 1.).abs() < 1e-12);
        for (actual, expected) in dc["conductorLossWPerM"][0]
            .as_array()
            .unwrap()
            .iter()
            .zip([expected_signal, expected_reference])
        {
            assert!((actual.as_f64().unwrap() / expected - 1.).abs() < 1e-12);
        }
        assert_eq!(dc["seriesResistanceOhmPerM"][0][0], json!(actual));
        assert_eq!(dc["endpoint"], "dc-resistance");
        assert!(dc["totalInductanceHPerM"].is_null());
        assert!(dc["quality"]["maxFieldResidual"].is_null());
        for (sample, frequency) in samples[1..].iter().zip(axis[1..].iter()) {
            assert_eq!(sample["endpoint"], "ac-magnetoquasistatic");
            assert_eq!(sample["frequencyHz"], json!(frequency));
            let z = &sample["seriesImpedanceOhmPerM"][0][0];
            let r = sample["seriesResistanceOhmPerM"][0][0].as_f64().unwrap();
            let l = sample["totalInductanceHPerM"][0][0].as_f64().unwrap();
            assert!((z["real"].as_f64().unwrap() / r - 1.).abs() < 1e-8);
            assert!(l > 0.);
            assert!(
                (z["imaginary"].as_f64().unwrap() / (l * std::f64::consts::TAU * frequency) - 1.)
                    .abs()
                    < 1e-8
            );
            for name in [
                "maxFieldResidual",
                "maxCurrentResidualA",
                "maxReciprocityError",
                "maxComplexPowerError",
            ] {
                assert!(sample["quality"][name].as_f64().unwrap() <= 1e-8);
            }
            let loss = sample["conductorLossWPerM"][0]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .sum::<f64>();
            assert!((loss / z["real"].as_f64().unwrap() - 1.).abs() < 1e-8);
        }
        let bytes = report["resources"]["combinedPlannedPeakBytes"]
            .as_u64()
            .unwrap();
        assert!(bytes <= 32 << 20);
        assert_eq!(report["coverage"]["domain"], "unverified");
        assert!(solve(&request, &sigma, &[1000., 0.], Instant::now())
            .err()
            .unwrap()
            .contains("SECTION_TRANSITION_GRID"));
        eprintln!(
            "polygon grid mesh={} resources={} AC qualities={:?}",
            report["mesh"],
            report["resources"],
            samples[1..]
                .iter()
                .map(|v| &v["quality"])
                .collect::<Vec<_>>()
        );
    }
}
