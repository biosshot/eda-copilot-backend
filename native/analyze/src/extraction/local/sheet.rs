//! Constant in-plane current bases dual to linear sheet potentials.
use super::{error, Result};

#[derive(Clone, Debug)]
pub struct SheetCell {
    pub source_id: String,
    pub triangle_m: [[f64; 2]; 3],
    pub thickness_m: f64,
    pub area_m2: f64,
    pub gradients_per_m: [[f64; 2]; 3],
    /// J = (i_x, i_y)/(thickness * sqrt(area)); modal coefficients are amperes.
    pub current_density_per_amp_m2: f64,
    pub resistance_ohm: f64,
    /// A = -integral grad(N) dot w, including both independent current bases.
    pub incidence: [[f64; 2]; 3],
}

impl SheetCell {
    pub fn new(
        source_id: String,
        triangle_m: [[f64; 2]; 3],
        thickness_m: f64,
        sigma_s_per_m: f64,
    ) -> Result<Self> {
        if source_id.is_empty()
            || !triangle_m.iter().flatten().all(|x| x.is_finite())
            || !thickness_m.is_finite()
            || thickness_m <= 0.
            || !sigma_s_per_m.is_finite()
            || sigma_s_per_m <= 0.
        {
            return Err(error(
                "SHEET_INPUT",
                "finite SI geometry, source and positive material required",
            ));
        }
        let [a, b, c] = triangle_m;
        let twice = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        if twice <= 0. || !twice.is_finite() {
            return Err(error(
                "SHEET_GEOMETRY",
                "triangle must be nondegenerate and counterclockwise",
            ));
        }
        let area_m2 = twice / 2.;
        let gradients_per_m = [
            [(b[1] - c[1]) / twice, (c[0] - b[0]) / twice],
            [(c[1] - a[1]) / twice, (a[0] - c[0]) / twice],
            [(a[1] - b[1]) / twice, (b[0] - a[0]) / twice],
        ];
        let density = 1. / (thickness_m * area_m2.sqrt());
        let resistance_ohm = 1. / (sigma_s_per_m * thickness_m);
        let incidence = gradients_per_m.map(|g| g.map(|x| -area_m2.sqrt() * x));
        if !density.is_finite()
            || density <= 0.
            || !resistance_ohm.is_finite()
            || resistance_ohm <= 0.
            || !incidence.iter().flatten().all(|x| x.is_finite())
        {
            return Err(error(
                "SHEET_RANGE",
                "basis/material coefficients outside F64 range",
            ));
        }
        Ok(Self {
            source_id,
            triangle_m,
            thickness_m,
            area_m2,
            gradients_per_m,
            current_density_per_amp_m2: density,
            resistance_ohm,
            incidence,
        })
    }

    pub fn recover_current_density(&self, current_a: [f64; 2]) -> Result<[f64; 2]> {
        let value = current_a.map(|i| i * self.current_density_per_amp_m2);
        if value.iter().all(|x| x.is_finite()) {
            Ok(value)
        } else {
            Err(error("RECOVERY_RANGE", "current-density recovery overflow"))
        }
    }
}
