use std::f64::consts::PI;

const MU0: f64 = 4e-7 * PI;
pub const VERSION: u32 = 2;

#[derive(Clone, Copy)]
pub(super) struct Profile {
    pub upper_hz: f64,
    pub current_step_m: f64,
    pub field_step_m: f64,
}

impl Profile {
    // This is a through-thickness screening target for planar nonmagnetic Cu.
    // It says nothing about lateral edges, proximity, or barrel radial resolution.
    pub fn skin_depth_m(self, conductivity_s_m: f64) -> f64 {
        ((PI * MU0 * self.upper_hz).sqrt() * conductivity_s_m.sqrt()).recip()
    }

    pub fn pieces(
        self,
        thickness_m: f64,
        conductivity_s_m: Option<f64>,
        authored_step_m: f64,
    ) -> Option<usize> {
        let mut step = authored_step_m;
        if let Some(sigma) = conductivity_s_m {
            let skin_step = self.skin_depth_m(sigma) / 3.;
            if !skin_step.is_finite() || skin_step <= 0. {
                return None;
            }
            if thickness_m > skin_step && skin_step < self.current_step_m.max(self.field_step_m) {
                // The refinement uses identical Cu planes in both field/current meshes.
                step = skin_step.min(self.current_step_m).min(self.field_step_m);
            }
        }
        let pieces = (thickness_m / step).ceil();
        (1. ..usize::MAX as f64)
            .contains(&pieces)
            .then_some(pieces as usize)
    }
}

pub(super) fn max_extrusion_planes(
    xy_nodes: usize,
    xy_triangles: usize,
    max_nodes: usize,
    max_tetrahedra: usize,
    max_bytes: usize,
) -> Option<usize> {
    // Same reservation and topology as the actual extrusion check, expressed
    // as a plane cap to reject the grid before allocating the z-plane buffer.
    let tetra_per_slab = xy_triangles.checked_mul(3)?;
    let slab_cap = max_tetrahedra
        .min(max_bytes / 2048)
        .checked_div(tetra_per_slab)?;
    Some(
        max_nodes
            .checked_div(xy_nodes)?
            .min(slab_cap.checked_add(1)?),
    )
}

pub(super) fn require_plane_room(
    existing: usize,
    added: usize,
    reserved: usize,
    cap: usize,
) -> Result<(), String> {
    if existing
        .checked_add(added)
        .and_then(|count| count.checked_add(reserved))
        .is_none_or(|count| count > cap)
    {
        return Err("MEMORY_LIMIT: skin-profile vertical grid exceeds extrusion node, tetrahedron or byte budget".into());
    }
    Ok(())
}

pub(super) fn append_layer_planes(
    z: &mut Vec<f64>,
    span: [f64; 2],
    pieces: usize,
    reserved: usize,
    cap: usize,
    mut check_deadline: impl FnMut() -> Result<(), String>,
) -> Result<f64, String> {
    require_plane_room(z.len(), pieces, reserved, cap)?;
    let [start, stop] = span;
    let mut max_step: f64 = 0.;
    let mut previous = start;
    for i in 1..=pieces {
        if i % 1024 == 1 {
            check_deadline()?;
        }
        let plane = if i == pieces {
            stop
        } else {
            start + (stop - start) * i as f64 / pieces as f64
        };
        if plane <= previous {
            return Err(
                "BOARD_PROFILE: skin-profile slab below representable vertical resolution".into(),
            );
        }
        max_step = max_step.max(plane - previous);
        previous = plane;
        z.push(plane);
    }
    Ok(max_step)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copper_skin_profile_tracks_material_and_frequency_and_preserves_dielectric() {
        let profile = Profile {
            upper_hz: 100_000.,
            current_step_m: 0.001,
            field_step_m: 0.0005,
        };
        let copper = Some(58e6);
        let thickness = 0.0005;
        assert!((profile.skin_depth_m(58e6) - 0.000_208_980_678).abs() < 1e-8);
        let current = profile.pieces(thickness, copper, profile.current_step_m);
        let field = profile.pieces(thickness, copper, profile.field_step_m);
        assert_eq!(current, Some(8));
        assert_eq!(field, current);
        assert!(thickness / current.unwrap() as f64 <= profile.skin_depth_m(58e6) / 3.);
        assert_eq!(profile.pieces(thickness, Some(58e6 / 4.), 0.001), Some(4));
        assert_eq!(
            Profile {
                upper_hz: 25_000.,
                ..profile
            }
            .pieces(thickness, copper, 0.001),
            Some(4)
        );
        assert_eq!(profile.pieces(thickness, None, 0.001), Some(1));
        assert_eq!(profile.pieces(35e-6, copper, 0.001), Some(1));

        // Two triangles per slab are six tetrahedra. Budget permits only six
        // slabs; the eight-piece copper layer alone needs nine planes.
        let cap = max_extrusion_planes(4, 2, 100, 100, 6 * 6 * 2048).unwrap();
        assert_eq!(cap, 7);
        let mut planes = vec![0.];
        let failure = append_layer_planes(
            &mut planes,
            [0., thickness],
            current.unwrap(),
            0,
            cap,
            || Ok(()),
        );
        assert!(failure.unwrap_err().contains("MEMORY_LIMIT"));
        assert_eq!(planes, vec![0.]);
        let step = append_layer_planes(
            &mut planes,
            [0., thickness],
            current.unwrap(),
            0,
            16,
            || Ok(()),
        )
        .unwrap();
        assert_eq!(planes.len(), 9);
        assert!(step <= profile.skin_depth_m(58e6) / 3.);
        append_layer_planes(
            &mut planes,
            [thickness, 2. * thickness],
            profile.pieces(thickness, None, 0.001).unwrap(),
            0,
            16,
            || Ok(()),
        )
        .unwrap();
        assert_eq!(planes.len(), 10);
        assert_eq!(planes[9], 2. * thickness);
        assert_eq!(
            max_extrusion_planes(4, usize::MAX, 100, 100, usize::MAX),
            None
        );
    }
}
