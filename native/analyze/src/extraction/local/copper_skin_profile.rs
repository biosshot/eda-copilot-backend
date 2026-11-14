use std::f64::consts::PI;

const MU0: f64 = 4e-7 * PI;
pub const VERSION: u32 = 3;

/// `P7_FOIL_SHEET` (read once): 1 keeps a copper layer with t/delta <= 1 as one foil sheet; 2 keeps one sheet at
/// any t/delta and multiplies the sheet resistance by `foil_resistance_ratio(t/delta)` at the band upper frequency
/// (approximation: one multiplier per band, field on one side of the foil).
fn foil_sheet_mode() -> u8 {
    static MODE: std::sync::OnceLock<u8> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| match std::env::var("P7_FOIL_SHEET").as_deref() { Ok("1") => 1, Ok("2") => 2, Ok("3") => 3, Ok("4") => 4, _ => 0 })
}
pub(super) fn foil_sheet_enabled() -> bool { foil_sheet_mode() >= 1 }
pub(super) fn foil_sheet_scales_resistance() -> bool { foil_sheet_mode() >= 2 }
/// `P7_FOIL_SHEET=3`: two-sided-field reading, multiplier `foil_resistance_ratio(x/2)` instead of `(x)`.
pub(super) fn foil_sheet_two_sided() -> bool { foil_sheet_mode() == 3 }
/// `P7_FOIL_SHEET=4`: t/delta > 1 gives exactly two layers through the thickness (two half-sheets), each half's resistance
/// multiplied by `foil_resistance_ratio((t/2)/delta)` at the band top; t/delta <= 1 behaves as `=1`.
pub(super) fn foil_sheet_half_sheets() -> bool { foil_sheet_mode() == 4 }
/// Sheet resistance multiplier for t/delta `x` under the active mode (1 when off).
pub(super) fn foil_sheet_multiplier(x: f64) -> f64 {
    if !foil_sheet_scales_resistance() { return 1.; }
    foil_resistance_ratio(if foil_sheet_two_sided() { x / 2. } else { x })
}

/// `P7_BARREL_SHEET` (read once): 1 keeps a plated via wall at one radial layer at any skin depth and records
/// the estimated resistance error; 2 also scales the wall resistance by `foil_resistance_ratio(t/delta)`
/// at the band upper frequency (one multiplier per band; the spatial power recovery applies the same factor).
fn barrel_sheet_mode() -> u8 {
    static MODE: std::sync::OnceLock<u8> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| match std::env::var("P7_BARREL_SHEET").as_deref() { Ok("1") => 1, Ok("2") => 2, _ => 0 })
}
pub(super) fn barrel_sheet_enabled() -> bool { barrel_sheet_mode() >= 1 }
pub(super) fn barrel_sheet_scales_resistance() -> bool { barrel_sheet_mode() == 2 }

/// t/delta at `upper_hz` of nonmagnetic copper; None for invalid input.
pub(super) fn thickness_over_skin(thickness_m: f64, sigma_s_m: f64, upper_hz: f64) -> Option<f64> {
    let x = thickness_m * (PI * MU0 * upper_hz * sigma_s_m).sqrt();
    (x.is_finite() && x > 0.).then_some(x)
}

/// Switch set and t/delta <= 1 (any t/delta under `P7_FOIL_SHEET=2`): the layer is admitted as one sheet (approximation tier).
pub(super) fn foil_sheet_admits(thickness_m: f64, sigma_s_m: f64, upper_hz: f64) -> bool {
    foil_sheet_enabled()
        && thickness_over_skin(thickness_m, sigma_s_m, upper_hz).is_some_and(|x| x <= 1. || (foil_sheet_scales_resistance() && !foil_sheet_half_sheets()))
}

/// r(x) = Re(kt coth kt), kt = (1+j)x, x = t/delta: resistance of a foil with
/// field on one side relative to its DC value.
pub(super) fn foil_resistance_ratio(x: f64) -> f64 {
    if x < 0.5 {
        // Re series: 1 + 4x^4/45 - 16x^8/4725 (next term ~1e-7 below x = 0.5).
        let x4 = x.powi(4);
        return 1. + 4. * x4 / 45. - 16. * x4 * x4 / 4725.;
    }
    if x > 300. { return x; }
    // coth(a + ja) = (sinh 2a - j sin 2a) / (cosh 2a - cos 2a)
    x * ((2. * x).sinh() + (2. * x).sin()) / ((2. * x).cosh() - (2. * x).cos())
}

/// Relative resistance error of replacing the foil by a uniform sheet.
pub(super) fn foil_sheet_error(x: f64) -> f64 { foil_resistance_ratio(x) - 1. }

#[derive(Clone, Copy)]
pub(super) struct Profile {
    pub upper_hz: f64,
    pub current_step_m: f64,
    pub field_step_m: f64,
}

impl Profile {
    /// Tighten both shared mesh targets to at most one fifth of every skin depth.
    pub fn with_thickness_screen(self, conductivities: impl IntoIterator<Item = f64>) -> Result<Self, String> {
        if !self.upper_hz.is_finite() || self.upper_hz <= 0.
            || !self.current_step_m.is_finite() || self.current_step_m <= 0.
            || !self.field_step_m.is_finite() || self.field_step_m <= 0.
        { return Err("BOARD_PROFILE: invalid frequency or mesh target".into()); }
        let mut target = self.current_step_m.min(self.field_step_m);
        let mut any = false;
        for sigma in conductivities {
            any = true;
            if !sigma.is_finite() || sigma <= 0. { return Err("BOARD_PROFILE: invalid conductivity".into()); }
            let depth = self.skin_depth_m(sigma);
            let step = depth / 5.;
            if !step.is_finite() || step <= 0. { return Err("BOARD_PROFILE: invalid skin depth".into()); }
            target = target.min(step);
        }
        if !any { return Err("BOARD_PROFILE: no conductor conductivity".into()); }
        Ok(Self { current_step_m: target, field_step_m: target, ..self })
    }

    // This is a through-thickness screening target for planar nonmagnetic Cu.
    // It says nothing about lateral edges or proximity.
    pub fn skin_depth_m(self, conductivity_s_m: f64) -> f64 {
        ((PI * MU0 * self.upper_hz).sqrt() * conductivity_s_m.sqrt()).recip()
    }

    pub fn pieces(
        self,
        thickness_m: f64,
        conductivity_s_m: Option<f64>,
        authored_step_m: f64,
    ) -> Option<usize> {
        if !thickness_m.is_finite() || thickness_m <= 0.
            || !authored_step_m.is_finite() || authored_step_m <= 0.
        {
            return None;
        }
        let mut step = authored_step_m;
        if let Some(sigma) = conductivity_s_m {
            if !sigma.is_finite() || sigma <= 0.
                || !self.upper_hz.is_finite() || self.upper_hz <= 0.
                || !self.current_step_m.is_finite() || self.current_step_m <= 0.
                || !self.field_step_m.is_finite() || self.field_step_m <= 0.
            {
                return None;
            }
            let skin_step = self.skin_depth_m(sigma) / 3.;
            if !skin_step.is_finite() || skin_step <= 0. {
                return None;
            }
            // P7_FOIL_SHEET=4 and t/delta > 1: exactly two layers through the thickness (existing layered path).
            if foil_sheet_half_sheets() && thickness_over_skin(thickness_m, sigma, self.upper_hz).is_some_and(|x| x > 1.) {
                return Some(2);
            }
            // P7_FOIL_SHEET=1 and t/delta <= 1: one foil sheet, authored step only.
            if !foil_sheet_admits(thickness_m, sigma, self.upper_hz) {
                // Current and field share Cu planes; never weaken an authored step.
                step = step.min(skin_step).min(self.current_step_m).min(self.field_step_m);
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

/// Bounding spacing along the spokes between the authored round barrel contours.
pub(super) struct RadialPlan {
    pub pieces: usize,
    pub maximum_step_m: f64,
    pub constraint_work: usize,
}

pub(super) fn radial_plan(
    profile: Profile,
    conductivity_s_m: f64,
    thickness_m: f64,
    contour_vertices: usize,
) -> Option<RadialPlan> {
    let target_m = profile.skin_depth_m(conductivity_s_m) / 3.;
    let pieces = if barrel_sheet_enabled() { 1. } else { (thickness_m / target_m).ceil() };
    if contour_vertices < 3 || !(1. ..usize::MAX as f64).contains(&pieces) {
        return None;
    }
    let pieces = pieces as usize;
    // Unique ring/spoke nodes plus the ring and spoke segments. Intersections
    // with the common arrangement are still preflighted by the CDT inserter.
    let segments_per_spoke = pieces.checked_mul(2)?.checked_sub(1)?;
    let constraint_work = if pieces == 1 {
        0
    } else {
        contour_vertices.checked_mul(pieces.checked_add(1)?.checked_add(segments_per_spoke)?)?
    };
    Some(RadialPlan {
        pieces,
        maximum_step_m: thickness_m / pieces as f64,
        constraint_work,
    })
}

pub(super) fn radial_point(
    inner: [f64; 2],
    outer: [f64; 2],
    index: usize,
    pieces: usize,
) -> [f64; 2] {
    if index == 0 {
        return inner;
    }
    if index == pieces {
        return outer;
    }
    let fraction = index as f64 / pieces as f64;
    std::array::from_fn(|axis| inner[axis] + (outer[axis] - inner[axis]) * fraction)
}

pub(super) fn visit_radial_constraints(
    vertices: &[([f64; 2], [f64; 2])],
    pieces: usize,
    mut emit: impl FnMut([f64; 2], [f64; 2]) -> Result<(), String>,
) -> Result<(), String> {
    let point = |vertex: usize, shell: usize| {
        let (inner, outer) = vertices[vertex];
        radial_point(inner, outer, shell, pieces)
    };
    for vertex in 0..vertices.len() {
        let next = (vertex + 1) % vertices.len();
        for shell in 1..pieces {
            emit(point(vertex, shell), point(next, shell))?;
        }
        for shell in 0..pieces {
            let (inner, outer) = (point(vertex, shell), point(vertex, shell + 1));
            if inner == outer {
                return Err(
                    "BOARD_PROFILE: radial copper shell below representable resolution".into(),
                );
            }
            emit(inner, outer)?;
        }
    }
    Ok(())
}

pub(super) fn radial_constraint_room(
    existing_work: usize,
    added_work: usize,
    max_nodes: usize,
    max_bytes: usize,
) -> Result<usize, String> {
    existing_work
        .checked_add(added_work)
        .filter(|&n| n <= max_nodes.min(max_bytes / 4096))
        .ok_or_else(|| "MEMORY_LIMIT: radial copper mesh constraints exceed XY budget".into())
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

    #[test]
    fn supplied_refinement_and_authored_limits_both_apply() {
        let profile = Profile { upper_hz: 100_000., current_step_m: 20e-6, field_step_m: 10e-6 };
        assert!(profile.skin_depth_m(58e6) / 3. > profile.field_step_m);
        let screened = Profile { current_step_m: 1e-3, field_step_m: 1e-3, ..profile }.with_thickness_screen([58e6, 29e6]).unwrap();
        assert!(screened.current_step_m <= profile.skin_depth_m(58e6) / 5.);
        assert_eq!(screened.current_step_m, screened.field_step_m);
        assert!(profile.with_thickness_screen([0.]).is_err());
        assert!(profile.with_thickness_screen([]).is_err());
        assert_eq!(profile.pieces(100e-6, Some(58e6), 1e-3), Some(10));
        assert_eq!(profile.pieces(100e-6, Some(58e6), 5e-6), Some(20));
        assert_eq!(profile.pieces(100e-6, None, 1e-3), Some(1));
        assert_eq!(profile.pieces(100e-6, Some(f64::NAN), 1e-3), None);
        assert_eq!(profile.pieces(100e-6, Some(58e6), 0.), None);
        assert_eq!(Profile { field_step_m: f64::NAN, ..profile }.pieces(100e-6, Some(58e6), 1e-3), None);
    }

    #[test]
    fn round_barrel_radial_grid_keeps_both_contours_and_refuses_oversized_plan() {
        let profile = Profile {
            upper_hz: 100_000.,
            current_step_m: 0.001,
            field_step_m: 0.0005,
        };
        let plan = radial_plan(profile, 58e6, 100e-6, 8).unwrap();
        assert_eq!(plan.pieces, 2);
        assert!(plan.maximum_step_m <= profile.skin_depth_m(58e6) / 3.);
        assert_eq!(
            radial_plan(
                Profile {
                    upper_hz: 25_000.,
                    ..profile
                },
                58e6,
                100e-6,
                8
            )
            .unwrap()
            .pieces,
            1
        );
        assert_eq!(
            radial_plan(profile, 58e6, 25e-6, 8)
                .unwrap()
                .constraint_work,
            0
        );
        let inner = [0.2, 0.];
        let outer = [0.3, 0.];
        assert_eq!(radial_point(inner, outer, 0, plan.pieces), inner);
        assert_eq!(radial_point(inner, outer, 1, plan.pieces), [0.25, 0.]);
        assert_eq!(radial_point(inner, outer, 2, plan.pieces), outer);
        let vertices = [
            (inner, outer),
            ([0., 0.2], [0., 0.3]),
            ([-0.2, 0.], [-0.3, 0.]),
            ([0., -0.2], [0., -0.3]),
        ];
        let mut constraints = Vec::new();
        visit_radial_constraints(&vertices, plan.pieces, |a, b| {
            constraints.push((a, b));
            Ok(())
        })
        .unwrap();
        assert_eq!(constraints.len(), vertices.len() * 3);
        assert!(constraints.contains(&(inner, [0.25, 0.])));
        assert!(constraints.contains(&([0.25, 0.], outer)));
        assert!(constraints.contains(&([0., -0.25], [0.25, 0.])));

        assert!(radial_constraint_room(0, plan.constraint_work, 100, 32 * 4096).is_err());
        assert_eq!(
            radial_constraint_room(0, plan.constraint_work, 100, 64 * 4096).unwrap(),
            48
        );
    }

    #[test]
    fn foil_sheet_resistance_error_estimate() {
        assert!((foil_sheet_error(0.53) - 0.007).abs() < 1e-3);
        assert!((foil_sheet_error(1.) - 0.09).abs() < 0.01);
        assert!(foil_sheet_error(0.05) < 1e-6 && foil_sheet_error(0.05) >= 0.);
        // Series and closed form agree across the switch point.
        assert!((foil_resistance_ratio(0.4999999) - foil_resistance_ratio(0.5000001)).abs() < 1e-6);
        assert!(foil_resistance_ratio(10.) > 9.9);
    }
}
