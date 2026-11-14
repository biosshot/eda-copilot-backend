//! Surface impedance perturbation of the vacuum TEM current basis and
//! dielectric energy participation. All lengths are metres; currents are
//! longitudinal peak phasors normalized to one ampere per signal conductor.
use super::*;
use std::collections::BTreeMap;

pub(super) struct Configuration {
    pub conductivity: Option<Vec<f64>>,
    pub minimum_skin_dimensions_m: Vec<f64>,
    pub tangents: Vec<f64>,
    /// Physical copper thickness per conductor surface, signals first (used only with `P7_SECTION_FOIL=1`).
    pub thickness_m: Vec<f64>,
    /// Signal conductor cross-section area (bounding box), 0 for reference planes (used only with `P7_SECTION_FOIL=1`).
    pub area_m2: Vec<f64>,
}

/// `P7_SECTION_FOIL` (read once): 1 replaces the thick-conductor surface impedance `(1+j)/(sigma delta)` by the finite-layer
/// `Zs = (kappa/sigma) coth(kappa s)`, `kappa = (1+j)/delta`, and lifts the `delta <= thickness/10` refusal.
/// Approximation tier, physical accuracy unverified. Unset or any other value: behaviour is unchanged.
pub(super) fn section_foil() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("P7_SECTION_FOIL").as_deref() == Ok("1"))
}
/// Conductor-loss limit `R/(omega L_ext)` of `SECTION_WEAK_LOSS`: 0.05; 0.3 with `P7_SECTION_FOIL=1` (approximation tier).
pub(super) fn section_conductor_loss_limit() -> f64 { if section_foil() { 0.3 } else { 0.05 } }
/// Effective sheet thickness: reference surface `t`; signal surface `A_k gram_kk` clamped to `[t/2, t]`.
pub(super) fn foil_sheet(is_signal: bool, area_m2: f64, gram_kk: f64, thickness_m: f64) -> f64 {
    if is_signal { (area_m2 * gram_kk).clamp(thickness_m / 2., thickness_m) } else { thickness_m }
}
/// `(Re Zs, Im Zs)` of a conducting layer of thickness `s` driven from one face, `Zs = (kappa/sigma) coth(kappa s)`.
pub(super) fn foil_surface_impedance(sigma: f64, w: f64, s: f64) -> (f64, f64) {
    let delta = (2. / (w * MU0 * sigma)).sqrt();
    let u = s / delta;
    // coth((1+j)u) = (sinh 2u - j sin 2u) / (cosh 2u - cos 2u); denominator written as 2(sinh^2 u + sin^2 u).
    let (cr, ci) = if u > 150. {
        (1., 0.)
    } else {
        let d = 2. * (u.sinh().powi(2) + u.sin().powi(2));
        ((2. * u).sinh() / d, -(2. * u).sin() / d)
    };
    // (1+j)(cr + j ci) = (cr - ci) + j (cr + ci)
    ((cr - ci) / (sigma * delta), (cr + ci) / (sigma * delta))
}
impl Configuration {
    pub fn parse(v: &Value, g: &Geometry) -> Result<Option<Self>> {
        let tangents: Vec<_> = g
            .dielectrics
            .iter()
            .map(|d| {
                if d.id == "air" {
                    0.
                } else {
                    v[if g.kind.contains("microstrip") {
                        "material"
                    } else if d.id == "bottom" {
                        "bottomMaterial"
                    } else {
                        "topMaterial"
                    }]["lossTangent"]
                        .as_f64()
                        .unwrap()
                }
            })
            .collect();
        let Some(c) = v.get("conductors") else {
            return Ok(if tangents.iter().any(|t| *t > 0.) {
                Some(Self {
                    conductivity: None,
                    minimum_skin_dimensions_m: vec![],
                    tangents,
                    thickness_m: vec![],
                    area_m2: vec![],
                })
            } else {
                None
            });
        };
        let reject = || {
            error("SECTION_CONDUCTOR_MATERIAL","conductors requires signal conductivitySM/state and complete explicit bottom/top reference thickness/material; only conductivitySM, state, id, source and kind:material supported")
        };
        let obj = c.as_object().ok_or_else(reject)?;
        if obj.len() != 2 || !obj.contains_key("signal") || !obj.contains_key("referencePlanes") {
            return Err(reject());
        }
        fn sigma(m: &Value) -> Result<f64> {
            let obj = m
                .as_object()
                .ok_or_else(|| error("SECTION_CONDUCTOR_MATERIAL", "material object required"))?;
            if obj
                .keys()
                .any(|k| !["conductivitySM", "state", "id", "source", "kind"].contains(&k.as_str()))
                || m["state"].as_str().is_none_or(|s| s.trim().is_empty())
                || ["id", "source"].iter().any(|k| {
                    m.get(k)
                        .is_some_and(|v| v.as_str().is_none_or(|s| s.trim().is_empty()))
                })
                || m.get("kind")
                    .is_some_and(|k| k.as_str() != Some("material"))
            {
                return Err(error(
                    "SECTION_CONDUCTOR_MATERIAL",
                    "unsupported conductor material field or missing state",
                ));
            }
            m["conductivitySM"]
                .as_f64()
                .filter(|x| x.is_finite() && *x > 0.)
                .ok_or_else(|| {
                    error(
                        "SECTION_CONDUCTOR_MATERIAL",
                        "positive finite conductivitySM required",
                    )
                })
        }
        let signal = sigma(&c["signal"])?;
        let r = c["referencePlanes"].as_object().ok_or_else(reject)?;
        let expected = if g.reference_y_m.len() == 1 {
            vec!["bottom"]
        } else {
            vec!["bottom", "top"]
        };
        if r.len() != expected.len() || expected.iter().any(|k| !r.contains_key(*k)) {
            return Err(reject());
        }
        let b = g.conductors[0].bounds_m;
        let mut conductivity = vec![signal; g.conductors.len()];
        let mut dimensions =
            vec![(b[2] - b[0]).min(b[3] - b[1]).min(g.feature()); g.conductors.len()];
        let mut thickness: Vec<f64> = g.conductors.iter().map(|c| c.bounds_m[3] - c.bounds_m[1]).collect();
        let mut area: Vec<f64> = g.conductors.iter().map(|c| (c.bounds_m[2] - c.bounds_m[0]) * (c.bounds_m[3] - c.bounds_m[1])).collect();
        for side in expected {
            let plane = &r[side];
            if plane.as_object().is_none_or(|o| {
                o.len() != 2 || !o.contains_key("material") || !o.contains_key("thicknessMm")
            }) {
                return Err(reject());
            }
            let t = plane["thicknessMm"]
                .as_f64()
                .filter(|t| t.is_finite() && *t > 0. && *t <= 10000.)
                .ok_or_else(reject)?
                * 1e-3;
            conductivity.push(sigma(&plane["material"])?);
            dimensions.push(t.min(g.feature()));
            thickness.push(t);
            area.push(0.);
        }
        Ok(Some(Self {
            conductivity: Some(conductivity),
            minimum_skin_dimensions_m: dimensions,
            tangents,
            thickness_m: thickness,
            area_m2: area,
        }))
    }
    pub fn screen(&self, frequencies: &[f64]) -> Result<()> {
        for f in frequencies {
            if *f <= 0. {
                return Err(error(
                    "SECTION_LOSS_FREQUENCY",
                    format!("lossy mode requires f>0; frequency {f} Hz"),
                ));
            }
            if let Some(sigmas) = &self.conductivity {
                for (i, s) in sigmas.iter().enumerate() {
                    let delta = (2. / (2. * std::f64::consts::PI * f * MU0 * s)).sqrt();
                    if !delta.is_finite() || delta <= 0. {
                        return Err(error("SECTION_SKIN_DEPTH",format!("frequency {f} Hz surface {i}: delta {delta} m is not representable as a finite positive skin depth (numeric overflow/underflow)")));
                    }
                    if !section_foil() && delta > self.minimum_skin_dimensions_m[i] / 10. {
                        return Err(error("SECTION_SKIN_DEPTH",format!("frequency {f} Hz surface {i}: delta {delta} m exceeds dimension/10={} m",self.minimum_skin_dimensions_m[i]/10.)));
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Serialize,Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Surface {
    pub id: String,
    pub edges: Vec<[usize; 2]>,
    pub node_indices: Vec<usize>,
    pub current_a_per_m_per_a: Vec<Vec<f64>>,
    pub integrated_current_a_per_a: Vec<f64>,
    pub gram_per_m: Vec<Vec<f64>>,
    /// Conductor cross-section area; filled only with `P7_SECTION_FOIL=1` on the polygon path (0 = not stored).
    #[serde(default, skip_serializing_if = "area_unset")]
    pub area_m2: f64,
}
fn area_unset(a: &f64) -> bool { *a == 0. }
#[derive(Serialize,Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Region {
    pub id: String,
    pub capacitance_f_per_m: Vec<Vec<f64>>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Integrals {
    pub surfaces: Vec<Surface>,
    pub regions: Vec<Region>,
    pub normalization: &'static str,
    pub max_current_normalization_error: f64,
    pub max_regional_capacitance_error: f64,
}

fn weighted(matrices: impl Iterator<Item = (f64, Vec<Vec<f64>>)>, n: usize) -> Vec<Vec<f64>> {
    let mut result = vec![vec![0.; n]; n];
    for (w, m) in matrices {
        for i in 0..n {
            for j in 0..n {
                result[i][j] += w * m[i][j];
            }
        }
    }
    result
}
fn conductor_matrix(p: &Integrals, c: &Configuration, n: usize) -> Vec<Vec<f64>> {
    weighted(
        p.surfaces.iter().enumerate().map(|(i, s)| {
            (
                c.conductivity.as_ref().map_or(0., |v| 1. / v[i].sqrt()),
                s.gram_per_m.clone(),
            )
        }),
        n,
    )
}
fn dielectric_matrix(p: &Integrals, c: &Configuration, n: usize) -> Vec<Vec<f64>> {
    weighted(
        p.regions
            .iter()
            .enumerate()
            .map(|(i, r)| (c.tangents[i], r.capacitance_f_per_m.clone())),
        n,
    )
}
fn modal(m: &[Vec<f64>], k: usize) -> f64 {
    if m.len() == 1 {
        m[0][0]
    } else {
        let s = if k == 0 { 1. } else { -1. };
        0.5 * (m[0][0] + m[1][1] + s * (m[0][1] + m[1][0]))
    }
}
fn compare(a: &[Vec<f64>], b: &[Vec<f64>]) -> Value {
    let n = a.len();
    let mut normalized = vec![vec![0.; n]; n];
    let mut absolute = vec![vec![0.; n]; n];
    let mut max: f64 = 0.;
    for i in 0..n {
        for j in 0..n {
            absolute[i][j] = (a[i][j] - b[i][j]).abs();
            // Zero diagonal PSD matrices have an identically zero row. No silent
            // epsilon floor: a changed zero row explicitly cannot converge.
            let scale = (a[i][i].max(b[i][i]) * a[j][j].max(b[j][j])).sqrt();
            normalized[i][j] = if scale > 0. {
                absolute[i][j] / scale
            } else if absolute[i][j] == 0. {
                0.
            } else {
                f64::INFINITY
            };
            max = max.max(normalized[i][j]);
        }
    }
    let power = if n == 1 {
        normalized[0][0]
    } else {
        0.5 * ((normalized[0][0] + normalized[1][1]).abs()
            + (normalized[0][0] - normalized[1][1]).hypot(2. * normalized[0][1]))
    };
    json!({"maxNormalizedChange":max,"normalizedPowerErrorBound":power,"absoluteChanges":absolute,"normalizedChanges":normalized})
}
pub(super) fn convergence(a: &Integrals, b: &Integrals, c: &Configuration) -> Value {
    let n = a.surfaces[0].gram_per_m.len();
    let mut max: f64 = 0.;
    let mut rows = vec![];
    for (x, y) in a.surfaces.iter().zip(&b.surfaces) {
        let d = compare(&x.gram_per_m, &y.gram_per_m);
        max = max.max(
            d["normalizedPowerErrorBound"]
                .as_f64()
                .unwrap_or(f64::INFINITY),
        );
        rows.push(json!({"surface":x.id,"comparison":d}));
    }
    for (x, y) in a.regions.iter().zip(&b.regions) {
        let d = compare(&x.capacitance_f_per_m, &y.capacitance_f_per_m);
        max = max.max(
            d["normalizedPowerErrorBound"]
                .as_f64()
                .unwrap_or(f64::INFINITY),
        );
        rows.push(json!({"region":x.id,"comparison":d}));
    }
    for (label, aa, bb) in [
        (
            "conductor",
            conductor_matrix(a, c, n),
            conductor_matrix(b, c, n),
        ),
        (
            "dielectric",
            dielectric_matrix(a, c, n),
            dielectric_matrix(b, c, n),
        ),
    ] {
        for k in 0..n {
            let x = modal(&aa, k);
            let y = modal(&bb, k);
            let d = if x == 0. && y == 0. {
                0.
            } else {
                (x - y).abs() / x.abs().max(y.abs())
            };
            max = max.max(d);
            rows.push(json!({"coefficient":label,"mode":k,"relativeChange":d,"absoluteChange":(x-y).abs()}));
        }
    }
    json!({"maxNormalizedChange":max,"comparisons":rows,"normalization":"surface/region diagonal power scale sqrt(max(Bii)*max(Bjj)); largest eigenvalue bound of entrywise absolute normalized difference; total modal conductor/dielectric ordinary relative; zero unchanged rows exactly zero"})
}
pub(super) fn samples(
    p: &Value,
    integrals: &Integrals,
    c: &Configuration,
    frequencies: &[f64],
    length: Option<f64>,
) -> Result<Vec<Value>> {
    use super::propagation::Z;
    let n = integrals.surfaces[0].gram_per_m.len();
    let external: Vec<Vec<f64>> = serde_json::from_value(p["inductanceHPerM"].clone()).unwrap();
    let capacitance: Vec<Vec<f64>> = serde_json::from_value(p["capacitanceFPerM"].clone()).unwrap();
    let bc = conductor_matrix(integrals, c, n);
    let bg = dielectric_matrix(integrals, c, n);
    let mut samples = vec![];
    let mut previous: Vec<Option<Z>> = vec![None; n];
    for f in frequencies {
        let w = 2. * std::f64::consts::PI * f;
        // P7_SECTION_FOIL=1: per-surface finite-layer impedance; `foil[k] = (Re Zs, Im Zs)`.
        let foil: Option<Vec<(f64, f64)>> = match (&c.conductivity, section_foil()) {
            (Some(sigmas), true) => Some(
                integrals.surfaces.iter().enumerate().map(|(k, s)| {
                    foil_surface_impedance(sigmas[k], w, foil_sheet(k < n, c.area_m2[k], if k < n { s.gram_per_m[k][k] } else { 0. }, c.thickness_m[k]))
                }).collect(),
            ),
            _ => None,
        };
        let r: Vec<Vec<f64>> = match &foil {
            Some(z) => weighted(integrals.surfaces.iter().enumerate().map(|(k, s)| (z[k].0, s.gram_per_m.clone())), n),
            None => bc
                .iter()
                .map(|v| v.iter().map(|x| x * (w * MU0 / 2.).sqrt()).collect())
                .collect(),
        };
        let internal: Vec<Vec<f64>> = match &foil {
            Some(z) => weighted(integrals.surfaces.iter().enumerate().map(|(k, s)| (z[k].1 / w, s.gram_per_m.clone())), n),
            None => r.iter().map(|v| v.iter().map(|x| x / w).collect()).collect(),
        };
        let total: Vec<Vec<f64>> = external
            .iter()
            .zip(&internal)
            .map(|(a, b)| a.iter().zip(b).map(|(a, b)| a + b).collect())
            .collect();
        let g: Vec<Vec<f64>> = bg
            .iter()
            .map(|v| v.iter().map(|x| x * w).collect())
            .collect();
        let mut modes = vec![];
        let mut worst_rg = 0.0_f64;
        for k in 0..n {
            let rm = modal(&r, k);
            let lm = modal(&total, k);
            let le = modal(&external, k);
            let gm = modal(&g, k);
            let cm = modal(&capacitance, k);
            let rg = rm / (w * le);
            let gg = gm / (w * cm);
            worst_rg = worst_rg.max(rg);
            if !rg.is_finite() || !gg.is_finite() || rg < 0. || gg < 0. || rg > section_conductor_loss_limit() || gg > 0.05 {
                return Err(error("SECTION_WEAK_LOSS",format!("frequency {f} Hz mode {k}: R/(omega Lexternal)={rg}, G/(omega C)={gg};limit {} (G/(omega C) 0.05)",section_conductor_loss_limit())));
            }
            for a in [&r, &g] {
                if n == 2
                    && ((a[0][0] - a[1][1]).abs() > 1e-8 * a[0][0].abs().max(1e-300)
                        || (a[0][1] - a[1][0]).abs() > 1e-8 * a[0][0].abs().max(1e-300))
                {
                    return Err(error(
                        "SECTION_LOSS_SYMMETRY",
                        "loss matrices not reflection symmetric",
                    ));
                }
            }
            let z = Z { r: rm, i: w * lm };
            let y = Z { r: gm, i: w * cm };
            let gamma = z.mul(y).sqrt();
            let zc = z.div(gamma);
            let identity = gamma.div(zc).sub(y).abs() / y.abs();
            if gamma.r < 0.
                || gamma.i <= 0.
                || zc.r <= 0.
                || identity > 1e-12
                || [gamma.r, gamma.i, zc.r, zc.i]
                    .iter()
                    .any(|v| !v.is_finite())
                || previous[k].is_some_and(|v| gamma.sub(v).abs() > gamma.add(v).abs())
            {
                return Err(error(
                    "SECTION_PROPAGATION",
                    "inconsistent/nonpassive modal root or frequency branch",
                ));
            }
            previous[k] = Some(gamma);
            modes.push(json!({"name":if n==1{"single"}else if k==0{"even"}else{"odd"},"resistanceOhmPerM":rm,"externalInductanceHPerM":le,"internalInductanceHPerM":if foil.is_some(){modal(&internal,k)}else{rm/w},"inductanceHPerM":lm,"conductanceSPerM":gm,"capacitanceFPerM":cm,"alphaNpPerM":gamma.r,"betaRadPerM":gamma.i,"characteristicImpedanceOhm":zc.value(),"phaseDelayS":length.map(|l|gamma.i*l/w),"phaseDelaySPerM":gamma.i/w,"conductorAlphaFirstOrderNpPerM":rm/2.*(cm/le).sqrt(),"dielectricAlphaFirstOrderNpPerM":gm/2.*(le/cm).sqrt(),"guards":{"resistanceToExternalReactance":rg,"conductanceToSusceptance":gg,"telegrapherIdentityRelativeError":identity}}));
        }
        let surfaces:Vec<_>=integrals.surfaces.iter().enumerate().map(|(i,s)|{let sigma=c.conductivity.as_ref().map(|v|v[i]);let rs=foil.as_ref().map_or_else(||sigma.map_or(0.,|s|(w*MU0/(2.*s)).sqrt()),|z|z[i].0);json!({"id":s.id,"conductivitySM":sigma,"skinDepthM":sigma.map(|s|(2./(w*MU0*s)).sqrt()),"minimumSkinDimensionM":c.minimum_skin_dimensions_m.get(i),"resistanceOhmPerM":s.gram_per_m.iter().map(|v|v.iter().map(|x|x*rs).collect::<Vec<_>>()).collect::<Vec<_>>()})}).collect();
        let mut sample=json!({"frequencyHz":f,"resistanceOhmPerM":r,"externalInductanceHPerM":external,"internalInductanceHPerM":internal,"totalInductanceHPerM":total,"conductanceSPerM":g,"capacitanceFPerM":capacitance,"modes":modes,"surfaceContributions":surfaces,"regionContributions":integrals.regions.iter().enumerate().map(|(i,r)|json!({"id":r.id,"lossTangent":c.tangents[i],"capacitanceFPerM":r.capacitance_f_per_m,"conductanceSPerM":r.capacitance_f_per_m.iter().map(|v|v.iter().map(|x|x*w*c.tangents[i]).collect::<Vec<_>>()).collect::<Vec<_>>()})).collect::<Vec<_>>()});
        if section_foil() && worst_rg > 0.05 { sample["approximation"] = json!("приближение"); sample["resistanceOverOmegaL"] = json!(worst_rg); }
        samples.push(sample);
    }
    Ok(samples)
}

pub fn integrals(f: &Field) -> Result<Integrals> {
    let n = f.capacitance.len();
    let mut inv = vec![vec![0.; n]; n];
    if n == 1 {
        inv[0][0] = 1. / f.vacuum_capacitance[0][0];
    } else {
        let c = &f.vacuum_capacitance;
        let det = c[0][0] * c[1][1] - c[0][1] * c[1][0];
        inv[0][0] = c[1][1] / det;
        inv[1][1] = c[0][0] / det;
        inv[0][1] = -c[0][1] / det;
        inv[1][0] = -c[1][0] / det;
    }
    // Boundary reactions are exact FE weak flux moments. Lumped boundary mass
    // recovery conserves every conductor's total current, including corner
    // nodes shared by two faces; no clipping or omission of corner moments.
    let count = n + f.geometry.reference_y_m.len();
    let mut edges = vec![Vec::new(); count];
    let mut seen = BTreeMap::new();
    for t in &f.triangles {
        for [a, b] in [[t[0], t[1]], [t[1], t[2]], [t[2], t[0]]] {
            if f.boundary[a] == 0 || f.boundary[a] != f.boundary[b] {
                continue;
            }
            let group = if f.boundary[a] > 0 {
                f.boundary[a] as usize - 1
            } else {
                let Some(k) = f.geometry.reference_y_m.iter().position(|y| {
                    (f.nodes[a][1] - y).abs() < 1e-16 && (f.nodes[b][1] - y).abs() < 1e-16
                }) else {
                    continue;
                };
                n + k
            };
            let key = (a.min(b), a.max(b));
            if seen.insert(key, ()).is_none() {
                edges[group].push([a, b]);
            }
        }
    }
    let ids=(0..count).map(|g|if g<n{f.geometry.conductors[g].id.clone()}else{if g==n{"bottom"}else{"top"}.into()}).collect::<Vec<_>>();
    let (surfaces,normalization_error)=surface_grams(&f.nodes,&f.vacuum_reaction_c_per_m,&inv,edges,&ids)?;
    let mut regions: Vec<Region> = f
        .geometry
        .dielectrics
        .iter()
        .map(|d| Region {
            id: d.id.clone(),
            capacitance_f_per_m: vec![vec![0.; n]; n],
        })
        .collect();
    for (k, t) in f.triangles.iter().enumerate() {
        let [a, b, c] = t.map(|i| f.nodes[i]);
        let area = ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])) / 2.;
        let y = (a[1] + b[1] + c[1]) / 3.;
        let r = f
            .geometry
            .dielectrics
            .iter()
            .position(|d| y > d.y_min_m && y < d.y_max_m)
            .unwrap();
        for i in 0..n {
            for j in 0..n {
                let ei = f.electric_fields_v_per_m[i][k];
                let ej = f.electric_fields_v_per_m[j][k];
                regions[r].capacitance_f_per_m[i][j] +=
                    EPS0 * f.epsilon_r[k] * area * (ei[0] * ej[0] + ei[1] * ej[1]);
            }
        }
    }
    let mut regional_error: f64 = 0.;
    for i in 0..n {
        for j in 0..n {
            let sum: f64 = regions.iter().map(|r| r.capacitance_f_per_m[i][j]).sum();
            regional_error =
                regional_error.max((sum - f.capacitance[i][j]).abs() / f.capacitance[i][i]);
        }
    }
    if normalization_error > 1e-8 || regional_error > 1e-8 {
        return Err(error(
            "SECTION_LOSS_NORMALIZATION",
            format!("current {normalization_error}; regional C {regional_error}"),
        ));
    }
    Ok(Integrals{surfaces,regions,normalization:"K = vacuum FE boundary charge moments times C0^-1 divided by lumped surface mass; longitudinal 1 A signal basis with oriented reference return; peak phasors, average dissipated power = 0.5 I* R I",max_current_normalization_error:normalization_error,max_regional_capacitance_error:regional_error})
}

/// Shared charge-conserving boundary mass recovery for finite polygon and
/// template sections. The same weak reaction moments and Gram integration are
/// used; no rectangular/symmetric-pair current formula is introduced.
pub(super) fn surface_grams(nodes:&[[f64;2]],reactions:&[Vec<f64>],inv:&[Vec<f64>],edges:Vec<Vec<[usize;2]>>,surface_ids:&[String])->Result<(Vec<Surface>,f64)>{
    surface_grams_projected(nodes,reactions,inv,edges,surface_ids,None)
}
pub(super) fn surface_grams_projected(nodes:&[[f64;2]],reactions:&[Vec<f64>],inv:&[Vec<f64>],edges:Vec<Vec<[usize;2]>>,surface_ids:&[String],budget:Option<local::AllocationBudget>)->Result<(Vec<Surface>,f64)>{
    let n=inv.len();
    let mut surfaces = vec![];
    let mut totals = vec![0.; n];
    let mut normalization_error: f64 = 0.;
    for (g, es) in edges.into_iter().enumerate() {
        let mut mass = BTreeMap::<usize, f64>::new();
        for [a, b] in &es {
            let len = (nodes[*a][0] - nodes[*b][0]).hypot(nodes[*a][1] - nodes[*b][1]);
            *mass.entry(*a).or_default() += len / 2.;
            *mass.entry(*b).or_default() += len / 2.;
        }
        let ids: Vec<_> = mass.keys().copied().collect();
        let lookup: BTreeMap<_, _> = ids.iter().enumerate().map(|(i, k)| (*k, i)).collect();
        let mut currents = vec![vec![0.; ids.len()]; n];
        let mut integrated = vec![0.; n];
        for k in 0..n {
            for (i, node) in ids.iter().enumerate() {
                let q: f64 = (0..n)
                    .map(|j| reactions[j][*node] * inv[j][k])
                    .sum();
                currents[k][i] = q / mass[node];
                integrated[k] += q;
            }
        }
        if let Some(budget)=budget {
            use faer::{sparse::{SparseColMat,Triplet},dyn_stack::{MemBuffer,MemStack}};
            let count=ids.len();let reserved=count.checked_mul(4096).ok_or_else(||error("SECTION_MEMORY_LIMIT","surface projection plan overflow"))?;budget.check(reserved)?;
            let mut entries=Vec::with_capacity(es.len()*3);
            for &[a,b] in &es {let length=(nodes[a][0]-nodes[b][0]).hypot(nodes[a][1]-nodes[b][1]);let i=lookup[&a];let j=lookup[&b];entries.push(Triplet::new(i,i,length/3.));entries.push(Triplet::new(j,j,length/3.));entries.push(Triplet::new(i.min(j),i.max(j),length/6.));}
            let matrix=SparseColMat::<usize,f64>::try_new_from_triplets(count,count,&entries).map_err(|e|error("SECTION_SURFACE_MASS",e))?;drop(entries);
            let (symbolic,values,_,persistent)=local::sparse_potential::factor(&matrix,reserved,budget,&[])?;
            let mut rhs=faer::Mat::from_fn(count,n,|i,k|currents[k][i]*mass[&ids[i]]);
            let scratch=symbolic.solve_in_place_scratch::<f64>(n,faer::Par::Seq);budget.check(persistent.checked_add(scratch.unaligned_bytes_required()).ok_or_else(||error("SECTION_MEMORY_LIMIT","surface solve plan overflow"))?)?;
            let mut memory=MemBuffer::try_new(scratch).map_err(|_|error("SECTION_MEMORY_LIMIT","surface solve scratch"))?;
            faer::sparse::linalg::cholesky::LltRef::new(&symbolic,&values).solve_in_place_with_conj(faer::Conj::No,rhs.as_mut(),faer::Par::Seq,MemStack::new(&mut memory));
            for k in 0..n {let q=(0..count).map(|i|currents[k][i]*mass[&ids[i]]).collect::<Vec<_>>();let mut action=vec![0.;count];for &[a,b]in &es{let length=(nodes[a][0]-nodes[b][0]).hypot(nodes[a][1]-nodes[b][1]);let i=lookup[&a];let j=lookup[&b];action[i]+=length*(2.*rhs[(i,k)]+rhs[(j,k)])/6.;action[j]+=length*(rhs[(i,k)]+2.*rhs[(j,k)])/6.;}let scale=q.iter().map(|v|v*v).sum::<f64>().sqrt();let residual=action.iter().zip(&q).map(|(a,b)|(a-b).powi(2)).sum::<f64>().sqrt()/scale.max(f64::MIN_POSITIVE);if !residual.is_finite()||residual>1e-10{return Err(error("SECTION_SURFACE_MASS","consistent weak flux projection residual"));}for i in 0..count{currents[k][i]=rhs[(i,k)];}integrated[k]=(0..count).map(|i|currents[k][i]*mass[&ids[i]]).sum();}
        }
        let mut gram = vec![vec![0.; n]; n];
        for [a, b] in &es {
            let len = (nodes[*a][0] - nodes[*b][0]).hypot(nodes[*a][1] - nodes[*b][1]);
            let ia = lookup[a];
            let ib = lookup[b];
            for i in 0..n {
                for j in 0..n {
                    gram[i][j] += len / 6.
                        * (2. * currents[i][ia] * currents[j][ia]
                            + currents[i][ia] * currents[j][ib]
                            + currents[i][ib] * currents[j][ia]
                            + 2. * currents[i][ib] * currents[j][ib]);
                }
            }
        }
        for k in 0..n {
            totals[k] += integrated[k];
            if g < n {
                normalization_error =
                    normalization_error.max((integrated[k] - if g == k { 1. } else { 0. }).abs());
            }
        }
        surfaces.push(Surface {
            id: surface_ids[g].clone(),
            edges: es,
            node_indices: ids,
            current_a_per_m_per_a: currents,
            integrated_current_a_per_a: integrated,
            gram_per_m: gram,
            area_m2: 0.,
        });
    }
    for q in totals {
        normalization_error = normalization_error.max(q.abs());
    }
    if normalization_error>1e-8{return Err(error("SECTION_LOSS_NORMALIZATION",format!("surface current {normalization_error:e}")));}
    Ok((surfaces,normalization_error))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn foil_surface_impedance_limits() {
        let sigma = 5.8e7;
        let w = std::f64::consts::TAU * 1e9;
        let rs = (w * MU0 / (2. * sigma)).sqrt();
        // Thick layer: Zs -> (1+j)/(sigma delta), Re = Im = Rs.
        let (re, im) = foil_surface_impedance(sigma, w, 1e-3);
        assert!((re / rs - 1.).abs() < 1e-12 && (im / rs - 1.).abs() < 1e-12);
        // Thin layer: Zs -> 1/(sigma s), Re = R_dc sheet, Im -> 0.
        let s = 1e-8;
        let (re, im) = foil_surface_impedance(sigma, w, s);
        assert!((re * sigma * s - 1.).abs() < 1e-4 && im < 1e-3 * re);
        // Clamp: signal sheet stays in [t/2, t]; reference sheet is t.
        assert_eq!(foil_sheet(true, 1., 1e9, 35e-6), 35e-6);
        assert_eq!(foil_sheet(true, 1., 1e-9, 35e-6), 17.5e-6);
        assert_eq!(foil_sheet(false, 1., 1., 35e-6), 35e-6);
    }
    #[test]
    fn skin_depth_numeric_range_reason_is_distinct_from_dimension_limit() {
        let config = Configuration {
            conductivity: Some(vec![f64::MAX]),
            minimum_skin_dimensions_m: vec![0.000035],
            tangents: vec![0.],
            thickness_m: vec![0.000035],
            area_m2: vec![0.],
        };
        let numeric = config.screen(&[1e9]).unwrap_err();
        assert!(
            numeric.contains("SECTION_SKIN_DEPTH")
                && numeric.contains("numeric overflow/underflow")
        );
        assert!(!numeric.contains("exceeds dimension"));
        let physical = Configuration {
            conductivity: Some(vec![58e6]),
            ..config
        }
        .screen(&[1.])
        .unwrap_err();
        assert!(physical.contains("exceeds dimension/10"));
        assert!(!physical.contains("numeric overflow/underflow"));
    }
    fn plate(layered: bool) -> Geometry {
        Geometry {
            conductors: vec![Conductor {
                id: "signal".into(),
                role: "PEC basis".into(),
                bounds_m: [0., 0.001, 0.002, 0.0011],
            }],
            dielectrics: if layered {
                vec![
                    Dielectric {
                        id: "bottom".into(),
                        y_min_m: 0.,
                        y_max_m: 0.0005,
                        relative_permittivity: 2.,
                        state: "analytic".into(),
                    },
                    Dielectric {
                        id: "top".into(),
                        y_min_m: 0.0005,
                        y_max_m: 0.001,
                        relative_permittivity: 6.,
                        state: "analytic".into(),
                    },
                ]
            } else {
                vec![Dielectric {
                    id: "bottom".into(),
                    y_min_m: 0.,
                    y_max_m: 0.001,
                    relative_permittivity: 4.,
                    state: "analytic".into(),
                }]
            },
            bounds_m: [0., 0., 0.002, 0.001],
            reference_y_m: vec![0.],
            length: Some(0.02),
            kind: "internal plate coupon".into(),
            artificial_boundary: "natural sides".into(),
        }
    }
    #[test]
    fn analytic_surface_resistance_internal_inductance_and_peak_power() {
        let references: Value = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/analysis-line-losses/references.json"
        ))
        .unwrap();
        let expected = &references["plate"];
        let field = solve_field(&plate(false), 0.0001).unwrap();
        let integral = integrals(&field).unwrap();
        let p = propagation::parameters(&field.capacitance, &field.vacuum_capacitance).unwrap();
        let c = Configuration {
            conductivity: Some(vec![5.8e7; 2]),
            minimum_skin_dimensions_m: vec![0.000035; 2],
            tangents: vec![0.015],
            thickness_m: vec![0.000035; 2],
            area_m2: vec![0.; 2],
        };
        let sample = &samples(&p, &integral, &c, &[1e9], Some(0.02)).unwrap()[0];
        for (key, reference) in [
            ("resistanceOhmPerM", "RTotalOhmPerM"),
            ("internalInductanceHPerM", "LinternalTotalHPerM"),
            ("capacitanceFPerM", "C_FPerM"),
            ("conductanceSPerM", "G_SPerM"),
        ] {
            let actual = sample[key][0][0].as_f64().unwrap();
            let want = expected[reference].as_f64().unwrap();
            assert!(
                (actual / want - 1.).abs() < 1e-8,
                "{key}: {actual} vs {want}"
            );
        }
        let r = sample["resistanceOhmPerM"][0][0].as_f64().unwrap();
        // For 2 A peak and two walls, each K=2/w. Average surface power
        // integrates Rs*K^2/2 independently, and equals I^2 R/2.
        let rs = (std::f64::consts::PI * 1e9 * MU0 / 5.8e7).sqrt();
        let surface_power = 2. * 0.002 * rs * (2_f64 / 0.002).powi(2) / 2.;
        assert!((surface_power / (2. * r) - 1.).abs() < 1e-10);
        assert_eq!(sample["externalInductanceHPerM"], p["inductanceHPerM"]);
    }
    #[test]
    fn layered_plate_analytic_complex_capacitance_and_local_tangent() {
        let field = solve_field(&plate(true), 0.0001).unwrap();
        let integral = integrals(&field).unwrap();
        let p = propagation::parameters(&field.capacitance, &field.vacuum_capacitance).unwrap();
        let c = Configuration {
            conductivity: None,
            minimum_skin_dimensions_m: vec![],
            tangents: vec![0.015, 0.035],
            thickness_m: vec![],
            area_m2: vec![],
        };
        let out = samples(&p, &integral, &c, &[1e9], None).unwrap();
        let g = out[0]["conductanceSPerM"][0][0].as_f64().unwrap();
        let references: Value = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/analysis-line-losses/references.json"
        ))
        .unwrap();
        let exact = references["layeredPlate"]["exactConductanceSPerM"]
            .as_f64()
            .unwrap();
        assert!((g / exact - 1.).abs() < 0.01);
        let no_bottom = Configuration {
            tangents: vec![0., 0.035],
            ..c
        };
        let changed = samples(&p, &integral, &no_bottom, &[1e9], None).unwrap();
        assert_eq!(
            out[0]["regionContributions"][1],
            changed[0]["regionContributions"][1]
        );
        assert_eq!(
            changed[0]["regionContributions"][0]["conductanceSPerM"][0][0],
            0.
        );
        let ratio = integral.regions[0].capacitance_f_per_m[0][0]
            / integral.regions[1].capacitance_f_per_m[0][0];
        assert!((ratio / 3. - 1.).abs() < 1e-8);
    }
    #[test]
    fn edge_integral_feasibility() {
        for kind in [
            "microstrip",
            "stripline",
            "coupled-microstrip",
            "coupled-stripline",
        ] {
            let mat = json!({"relativePermittivity":4.,"lossTangent":0.,"state":"test"});
            let mut line = json!({"kind":kind,"widthMm":0.3,"copperThicknessMm":0.035});
            if kind.contains("microstrip") {
                line["dielectricHeightMm"] = json!(0.18);
                line["material"] = mat.clone();
            } else {
                line["bottomDielectricHeightMm"] = json!(0.18);
                line["topDielectricHeightMm"] = json!(0.25);
                line["bottomMaterial"] = mat.clone();
                line["topMaterial"] = mat.clone();
            }
            if kind.starts_with("coupled") {
                line["gapMm"] = json!(0.2);
            }
            let g = geometry::parse(&line).unwrap().domain(8.);
            for level in 0..=3 {
                let h = g.feature() / 6. / 2_f64.powi(level);
                match solve_field(&g, h).and_then(|f| Ok((f.nodes.len(), integrals(&f)?))) {
                    Ok((nodes, p)) => println!(
                        "{}",
                        json!({"kind":kind,"level":level,"nodes":nodes,"surface":p.surfaces.iter().map(|s|json!({"id":s.id,"gram":s.gram_per_m,"total":s.integrated_current_a_per_a})).collect::<Vec<_>>()})
                    ),
                    Err(e) => {
                        println!("{kind} level {level}: {e}");
                        break;
                    }
                }
            }
        }
    }
}
