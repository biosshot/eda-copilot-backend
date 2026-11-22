//! Exact finite-volume DC contact map. Component stamps and sources belong
//! to the external network. No electric or magnetic factor is needed.
use super::{
    error,
    volume_current::{DcFactor, Space},
    AllocationBudget, Result,
};
use std::time::{Duration, Instant};

pub struct ElectrodeDc {
    factor: DcFactor,
    contacts: Vec<usize>,
    /// Galvanic component IDs in authored contact order.
    pub component_ids: Vec<usize>,
    /// Electrode current injection = conductance_s * electrode voltage.
    pub conductance_s: Vec<Vec<f64>>,
}

impl ElectrodeDc {
    pub fn new_with_deadline(
        space: &Space,
        contacts: &[usize],
        budget: AllocationBudget,
        wall: Duration,
    ) -> Result<Self> {
        let started = Instant::now();
        if !(1..=32).contains(&contacts.len()) {
            return Err(error(
                "DC_CONTACTS",
                "1..32 finite electrode coordinates required",
            ));
        }
        let extra = space
            .potential_count()
            .checked_mul(32)
            .and_then(|n| n.checked_add(contacts.len().pow(2) * 192))
            .ok_or_else(|| error("MEMORY_LIMIT", "DC electrode map plan overflow"))?;
        let solve_reserve = space
            .potential_count()
            .checked_mul(128)
            .and_then(|n| n.checked_add(space.current_count().checked_mul(32)?))
            .and_then(|n| n.checked_add(128 * 1024))
            .ok_or_else(|| error("MEMORY_LIMIT", "DC solve reserve overflow"))?;
        let max_bytes = budget
            .max_bytes
            .checked_sub(extra)
            .and_then(|n| n.checked_sub(solve_reserve))
            .ok_or_else(|| error("MEMORY_LIMIT", "DC Schur/solve reserve exceeds budget"))?;
        if started.elapsed() >= wall {
            return Err(error("DEADLINE", "DC factor deadline"));
        }
        // The sparse numeric factor is a bounded, noninterruptible call.
        let factor = space.dc_factor(
            contacts,
            AllocationBudget {
                max_bytes,
                ..budget
            },
        )?;
        if started.elapsed() >= wall {
            return Err(error("DEADLINE", "DC factor deadline"));
        }
        if factor.solve_scratch_bytes(space) > solve_reserve {
            return Err(error(
                "MEMORY_LIMIT",
                "DC solve scratch exceeds reserved bytes",
            ));
        }
        let component_ids = contacts
            .iter()
            .map(|i| {
                space
                    .components
                    .iter()
                    .position(|nodes| nodes.contains(i))
                    .ok_or_else(|| error("DC_CONTACTS", "contact missing from galvanic partition"))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut map = Self {
            factor,
            contacts: contacts.to_vec(),
            component_ids,
            conductance_s: vec![vec![0.; contacts.len()]; contacts.len()],
        };
        let mut volts = vec![0.; contacts.len()];
        for col in 0..contacts.len() {
            if started.elapsed() >= wall {
                return Err(error("DEADLINE", "DC electrode Schur deadline"));
            }
            // A singly contacted isolated conductor has only a constant
            // Dirichlet mode and exactly zero DC terminal admittance.
            if map
                .component_ids
                .iter()
                .filter(|&&id| id == map.component_ids[col])
                .count()
                == 1
            {
                continue;
            }
            volts[col] = 1.;
            let potential = map.potential(space, &volts)?;
            let reaction = space.dc_apply(&potential)?;
            for (row, &dof) in contacts.iter().enumerate() {
                map.conductance_s[row][col] = reaction[dof];
            }
            volts[col] = 0.;
        }
        let tolerance = 1e-8;
        let scales = map
            .conductance_s
            .iter()
            .map(|row| row.iter().map(|v| v.abs()).fold(0., f64::max))
            .collect::<Vec<_>>();
        for (i, row) in map.conductance_s.iter().enumerate() {
            if row.iter().sum::<f64>().abs() > tolerance * scales[i]
                || row.iter().enumerate().any(|(j, &g)| {
                    !g.is_finite()
                        || (g - map.conductance_s[j][i]).abs()
                            > tolerance * scales[i].min(scales[j])
                        || (map.component_ids[i] != map.component_ids[j] && g != 0.)
                })
            {
                return Err(error(
                    "DC_SCHUR",
                    "DC electrode map fails conservation, reciprocity, or galvanic isolation",
                ));
            }
        }
        Ok(map)
    }
    fn potential(&self, space: &Space, contact_volts: &[f64]) -> Result<Vec<f64>> {
        if contact_volts.len() != self.contacts.len()
            || contact_volts.iter().any(|v| !v.is_finite())
        {
            return Err(error(
                "DC_CONTACTS",
                "finite voltage per authored contact required",
            ));
        }
        let mut boundary = vec![0.; space.potential_count()];
        for (&index, &volts) in self.contacts.iter().zip(contact_volts) {
            boundary[index] = volts;
        }
        self.factor
            .solve(space, &boundary, &vec![0.; boundary.len()])
    }

    /// Full DC copper state for the terminal voltages solved by the connected
    /// circuit. Components with no terminals use the factor's numerical gauge.
    pub fn recover(&self, space: &Space, contact_volts: &[f64]) -> Result<(Vec<f64>, Vec<f64>)> {
        let potential = self.potential(space, contact_volts)?;
        let mut current = space.gradient(&potential)?;
        for (cell, xyz) in space.cells.iter().zip(current.chunks_exact_mut(3)) {
            for value in xyz {
                *value /= cell.resistance_ohm;
            }
        }
        Ok((potential, current))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::local::{
        board_dc::loaded,
        mesh::{self, LayeredInput, Material},
        volume_current,
    };

    #[test]
    fn finite_contact_map_and_recovery_preserve_islands_and_dc_power() {
        let budget = AllocationBudget {
            max_bytes: 16 * 1024 * 1024,
            max_nodes: 100,
            max_tetrahedra: 100,
        };
        let width = 1e-3_f64;
        let length = 2e-3;
        let sigma = 4e7;
        let copper = Material::Copper {
            source_ids: vec!["control".into()],
        };
        let gap = Material::Dielectric {
            label: "galvanic break".into(),
            epsilon_r: 4.,
        };
        let mesh = mesh::extrude(
            &LayeredInput {
                xy_m: vec![[0., 0.], [width, 0.], [width, width], [0., width]],
                triangles: vec![[0, 1, 2], [0, 2, 3]],
                boundary_edges: vec![[0, 1], [1, 2], [2, 3], [3, 0]],
                z_m: vec![0., length, 2. * length, 4. * length],
                materials: vec![vec![copper.clone(); 2], vec![gap; 2], vec![copper; 2]],
            },
            budget,
        )
        .unwrap();
        let faces = [0, 4, 8, 12].map(|start| (start..start + 4).collect::<Vec<_>>());
        let space = volume_current::assemble(
            &mesh,
            &vec![sigma; mesh.copper_tetrahedra.len()],
            &faces,
            budget,
        )
        .unwrap();
        let contacts = faces
            .each_ref()
            .map(|nodes| space.node_potential[&nodes[0]]);
        let dc = ElectrodeDc::new_with_deadline(&space, &contacts, budget, Duration::from_secs(5))
            .unwrap();
        assert_eq!(dc.component_ids[0], dc.component_ids[1]);
        assert_eq!(dc.component_ids[2], dc.component_ids[3]);
        assert_ne!(dc.component_ids[0], dc.component_ids[2]);
        let gs = [
            sigma * width.powi(2) / length,
            sigma * width.powi(2) / (2. * length),
        ];
        for (i, row) in dc.conductance_s.iter().enumerate() {
            let g = gs[i / 2];
            for (j, &actual) in row.iter().enumerate() {
                let expected = if i == j {
                    g
                } else if i / 2 == j / 2 {
                    -g
                } else {
                    0.
                };
                assert!((actual - expected).abs() < 1e-12 * g);
            }
        }
        let (_, current) = dc.recover(&space, &[1., 0., 4., 2.]).unwrap();
        let loss = space.losses(&current).unwrap().iter().sum::<f64>();
        assert!((loss - gs[0] - 4. * gs[1]).abs() < 1e-12 * gs[0]);
        let ports = [[0, 2], [1, 3]];
        let opposed = loaded(
            &dc.conductance_s,
            &dc.component_ids,
            &ports,
            &[1., -1.],
            &[None, None],
        )
        .unwrap();
        assert_ne!(opposed.components[0], opposed.components[2]);
        let loop_drop = opposed.volts[0] - opposed.volts[2] - opposed.volts[1] + opposed.volts[3];
        assert!((loop_drop - (gs[0].recip() + gs[1].recip())).abs() < 1e-12);
        assert!(loaded(
            &dc.conductance_s,
            &dc.component_ids,
            &ports,
            &[1., 0.],
            &[None, None]
        )
        .err()
        .unwrap()
        .contains("LOCAL_DC_INCOMPATIBLE"));
        let shunted = loaded(
            &dc.conductance_s,
            &dc.component_ids,
            &ports,
            &[1., 0.],
            &[Some(2.), None],
        )
        .unwrap();
        assert_eq!(shunted.components[0], shunted.components[2]);
        // Eliminating the floating return uses a 0.5 S pivot formed from
        // copper coefficients of order 10 kS. Scale the test's rounding
        // allowance by that ratio; the production KCL/power gates stay fixed.
        let expected_w = 2.;
        let rounding_w = 32. * f64::EPSILON * (gs[1] / 0.5) * expected_w;
        assert!((shunted.load_loss_w - expected_w).abs() < rounding_w,
            "shunt loss {:e} W vs {:e} W (rounding allowance {:e} W)",
            shunted.load_loss_w, expected_w, rounding_w);
    }
}
