//! Conservative frequency screening; passing it is not physical acceptance.
use std::f64::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Screening {
    pub skin_depth_m: f64,
    pub thickness_over_skin_depth: f64,
    /// Thickness screening alone never establishes proximity accuracy.
    pub thickness_passes: bool,
    /// Existing volume mesh uses at least three intervals per skin depth.
    pub volume_target_step_m: f64,
}

/// Inputs use SI units. Frequency may be zero; all material parameters must be positive.
pub(super) fn screen(frequency_hz: f64, thickness_m: f64, conductivity_s_m: f64, permeability_h_m: f64) -> Result<Screening, &'static str> {
    if !frequency_hz.is_finite() || frequency_hz < 0.0 || !thickness_m.is_finite() || thickness_m <= 0.0 || !conductivity_s_m.is_finite() || conductivity_s_m <= 0.0 || !permeability_h_m.is_finite() || permeability_h_m <= 0.0 {
        return Err("invalid frequency or conductor material");
    }
    if frequency_hz == 0.0 {
        return Ok(Screening { skin_depth_m: f64::INFINITY, thickness_over_skin_depth: 0.0, thickness_passes: true, volume_target_step_m: f64::INFINITY });
    }
    let inverse_depth = (PI * frequency_hz).sqrt() * permeability_h_m.sqrt() * conductivity_s_m.sqrt();
    let depth = inverse_depth.recip();
    let ratio = thickness_m * inverse_depth;
    if !depth.is_finite() || depth <= 0.0 || !ratio.is_finite() {
        return Err("skin-depth calculation outside representable range");
    }
    Ok(Screening { skin_depth_m: depth, thickness_over_skin_depth: ratio, thickness_passes: ratio <= 0.2 || (super::copper_skin_profile::foil_sheet_enabled() && ratio <= 1.0),volume_target_step_m: depth / 3.0 })
}

/// A passing thickness screen still requires independent proximity and refinement evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action { RequireProximityEvidence, EnrichOrEscalate }

impl Screening {
    pub fn action(self) -> Action {
        if self.thickness_passes { Action::RequireProximityEvidence } else { Action::EnrichOrEscalate }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dc_and_rf_are_screened_independently() {
        let mu = 4e-7 * PI;
        let dc = screen(0.0, 35e-6, 58e6, mu).unwrap();
        assert_eq!(dc.action(), Action::RequireProximityEvidence);
        assert!(dc.skin_depth_m.is_infinite());
        let rf = screen(1e9, 35e-6, 58e6, mu).unwrap();
        assert_eq!(rf.action(), Action::EnrichOrEscalate);
        assert!(rf.volume_target_step_m < rf.skin_depth_m);
    }
    #[test]
    fn invalid_inputs_refuse() {
        assert!(screen(-1.0, 1.0, 1.0, 1.0).is_err());
        assert!(screen(1.0, 1.0, f64::NAN, 1.0).is_err());
        assert!(screen(1.0, 0.0, 1.0, 1.0).is_err());
    }
}
