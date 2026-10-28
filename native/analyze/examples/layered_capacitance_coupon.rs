//! Internal offline evidence harness using the actual shared crate primitives.
#![allow(dead_code)]
use pcb_analyze::extraction::local;
use local::layered_capacitance::*;
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let started = Instant::now();
    let mut input: Input =
        serde_json::from_str(&std::fs::read_to_string(&args[1]).unwrap()).unwrap();
    if args.iter().any(|a| a == "--cdt") {
        for m in &mut input.meshes {
            if let PlaneInput::Exact {
                source_id,
                xy_m,
                triangles,
            } = m.clone()
            {
                let mut constraints = BTreeSet::new();
                for t in triangles {
                    let center = [
                        t.iter().map(|i| xy_m[*i][0]).sum::<f64>() / 3.,
                        t.iter().map(|i| xy_m[*i][1]).sum::<f64>() / 3.,
                    ];
                    if center[0].abs().max(center[1].abs()) <= 0.0005 {
                        continue;
                    }
                    for k in 0..3 {
                        let mut e = [t[k], t[(k + 1) % 3]];
                        e.sort();
                        constraints.insert(e);
                    }
                }
                *m = PlaneInput::Cdt {
                    source_id,
                    xy_m,
                    constraints: constraints.into_iter().collect(),
                };
            }
        }
    }
    let budget = Budget {
        max_nodes: 50_000,
        max_local_terms: 6_000_000,
        max_bytes: 512 * 1024 * 1024,
        max_wall: Duration::from_secs(60),
    };
    match extract(&input, budget) {
        Ok(s) => {
            let common = s.recover(&vec![1.; input.metals.len()]).unwrap();
            let diff = s
                .recover(
                    &(0..input.metals.len())
                        .map(|i| if i % 2 == 0 { 1. } else { -1. })
                        .collect::<Vec<_>>(),
                )
                .unwrap();
            let field_slice: Vec<_> = (0..input.z_m.len() - 1)
                .filter_map(|slab| {
                    common.fields.iter().enumerate()
                        .filter(|(_, f)| f.slab == slab)
                        .min_by(|(_, a), (_, b)| {
                            let distance = |p: [f64; 3]| (p[0] - 0.001).powi(2) + p[1].powi(2);
                            distance(a.position_m).total_cmp(&distance(b.position_m))
                        })
                        .map(|(i, f)| serde_json::json!({"positionM":f.position_m,"commonE":f.electric_field_v_per_m,"differentialE":diff.fields[i].electric_field_v_per_m}))
                }).collect();
            println!(
                "{}",
                serde_json::json!({"status":"completed","capacitanceF":s.capacitance_f,"sourceIds":s.source_ids,"meshSourceIds":s.mesh_source_ids,"plan":s.plan,"wholeHarnessSeconds":started.elapsed().as_secs_f64(),"commonEnergyJ":common.energy_j,"differentialEnergyJ":diff.energy_j,"fieldSamples":common.fields.len(),"fieldSlice":field_slice,"recoveryMax":common.recovered_energy_relative_error.max(diff.recovered_energy_relative_error),"freeResidualMax":common.normalized_free_residual.max(diff.normalized_free_residual),"chargeBalanceMax":common.normalized_charge_balance.max(diff.normalized_charge_balance),"workBalanceMax":common.normalized_energy_reaction_error.max(diff.normalized_energy_reaction_error)})
            );
        }
        Err(error) => {
            println!(
                "{}",
                serde_json::json!({"status":"refused","error":error,"seconds":started.elapsed().as_secs_f64()})
            );
            std::process::exit(2);
        }
    }
}
