use super::*;
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conductor {
    pub id: String,
    pub role: String,
    pub bounds_m: [f64; 4],
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dielectric {
    pub id: String,
    pub y_min_m: f64,
    pub y_max_m: f64,
    pub relative_permittivity: f64,
    pub state: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Geometry {
    pub conductors: Vec<Conductor>,
    pub dielectrics: Vec<Dielectric>,
    pub bounds_m: [f64; 4],
    pub reference_y_m: Vec<f64>,
    pub length: Option<f64>,
    pub kind: String,
    pub artificial_boundary: String,
}
impl Geometry {
    pub fn scale(&self) -> f64 {
        let w = self
            .conductors
            .iter()
            .map(|c| c.bounds_m[2])
            .fold(f64::NEG_INFINITY, f64::max)
            - self
                .conductors
                .iter()
                .map(|c| c.bounds_m[0])
                .fold(f64::INFINITY, f64::min);
        let height = if self.dielectrics.iter().any(|d| d.id == "air") {
            self.conductors[0].bounds_m[3]
        } else {
            self.dielectrics
                .iter()
                .map(|d| d.y_max_m - d.y_min_m)
                .sum::<f64>()
        };
        w.max(height)
    }
    pub fn feature(&self) -> f64 {
        let b = self.conductors[0].bounds_m;
        let mut h = (b[2] - b[0]).min(
            self.reference_y_m
                .iter()
                .map(|y| (y - b[1]).abs().min((y - b[3]).abs()))
                .fold(f64::INFINITY, f64::min),
        );
        if self.conductors.len() == 2 {
            let other = self.conductors[1].bounds_m;
            h = h.min(if other[0] > b[0] {
                other[0] - b[2]
            } else {
                b[0] - other[2]
            });
        }
        h
    }
    pub fn max_epsilon(&self) -> f64 {
        self.dielectrics
            .iter()
            .map(|d| d.relative_permittivity)
            .fold(1., f64::max)
    }
    pub fn domain(&self, scale: f64) -> Self {
        let mut g = self.clone();
        let pad = self.scale() * scale;
        g.bounds_m[0] = self
            .conductors
            .iter()
            .map(|c| c.bounds_m[0])
            .fold(f64::INFINITY, f64::min)
            - pad;
        g.bounds_m[2] = self
            .conductors
            .iter()
            .map(|c| c.bounds_m[2])
            .fold(f64::NEG_INFINITY, f64::max)
            + pad;
        if let Some(d) = g.dielectrics.iter_mut().find(|d| d.id == "air") {
            d.y_max_m = d.y_min_m + pad;
            g.bounds_m[3] = d.y_max_m;
        }
        g
    }
}
fn positive(v: &Value, k: &str) -> Result<f64> {
    let x = v[k]
        .as_f64()
        .ok_or_else(|| error("SECTION_INPUT", format!("missing {k}")))?;
    if !x.is_finite() || x <= 0. || x > 10000. {
        Err(error("SECTION_INPUT", format!("invalid {k}")))
    } else {
        Ok(x * 1e-3)
    }
}
fn dielectric(v: &Value, id: &str, lo: f64, hi: f64) -> Result<Dielectric> {
    let obj = v.as_object().ok_or_else(|| error("SECTION_MATERIAL", id))?;
    if obj
        .keys()
        .any(|k| !["relativePermittivity", "lossTangent", "state"].contains(&k.as_str()))
    {
        return Err(error(
            "SECTION_UNSUPPORTED",
            "only explicit nondispersive epsilon/lossTangent/state material fields supported",
        ));
    }
    let e = v["relativePermittivity"]
        .as_f64()
        .ok_or_else(|| error("SECTION_MATERIAL", "explicit epsilon required"))?;
    if !e.is_finite()
        || e <= 0.
        || e > 100.
        || v["lossTangent"].as_f64() != Some(0.)
        || v["state"].as_str().is_none_or(|s| s.trim().is_empty())
    {
        return Err(error(
            "SECTION_MATERIAL",
            "positive epsilon<=100, explicit lossTangent:0 and nonempty state required",
        ));
    }
    Ok(Dielectric {
        id: id.into(),
        y_min_m: lo,
        y_max_m: hi,
        relative_permittivity: e,
        state: v["state"].as_str().unwrap().into(),
    })
}
pub fn parse(v: &Value) -> Result<Geometry> {
    let kind = v["kind"].as_str().unwrap_or("");
    let micro = matches!(kind, "microstrip" | "coupled-microstrip");
    let pair = kind.starts_with("coupled-");
    if !micro && !matches!(kind, "stripline" | "coupled-stripline") {
        return Err(error(
            "SECTION_UNSUPPORTED",
            "only four explicit line cross sections supported",
        ));
    }
    if v.get("solderMask").is_some() {
        return Err(error("SECTION_UNSUPPORTED", "solder mask is not supported"));
    }
    let common = [
        "kind",
        "widthMm",
        "copperThicknessMm",
        "lengthMm",
        "gapMm",
        "dielectricHeightMm",
        "material",
        "topDielectricHeightMm",
        "bottomDielectricHeightMm",
        "topMaterial",
        "bottomMaterial",
    ];
    if v.as_object()
        .is_none_or(|o| o.keys().any(|k| !common.contains(&k.as_str())))
    {
        return Err(error("SECTION_INPUT", "unknown line field"));
    }
    if (!pair && v.get("gapMm").is_some())
        || (micro
            && [
                "topDielectricHeightMm",
                "bottomDielectricHeightMm",
                "topMaterial",
                "bottomMaterial",
            ]
            .iter()
            .any(|k| v.get(k).is_some()))
        || (!micro
            && ["material", "dielectricHeightMm"]
                .iter()
                .any(|k| v.get(k).is_some()))
    {
        return Err(error(
            "SECTION_INPUT",
            "fields inconsistent with geometry kind",
        ));
    }
    let w = positive(v, "widthMm")?;
    let t = positive(v, "copperThicknessMm")?;
    let gap = if pair { positive(v, "gapMm")? } else { 0. };
    let length = if v.get("lengthMm").is_some() {
        Some(positive(v, "lengthMm")?)
    } else {
        None
    };
    let h = positive(
        v,
        if micro {
            "dielectricHeightMm"
        } else {
            "bottomDielectricHeightMm"
        },
    )?;
    let top = if micro {
        h * 8.
    } else {
        positive(v, "topDielectricHeightMm")?
    };
    if [w, t, h, top].iter().any(|x| *x < 1e-10)
        || t / h > 10.
        || t / top > 10.
        || w / h > 1000.
        || h / w > 1000.
        || pair && gap / w < 1e-5
    {
        return Err(error(
            "SECTION_GEOMETRY_LIMIT",
            "extreme aspect ratio/feature below geometric cap",
        ));
    }
    let center = if pair { -(w + gap) / 2. } else { 0. };
    let mut conductors = vec![];
    for i in 0..if pair { 2 } else { 1 } {
        let x = center + i as f64 * (w + gap);
        conductors.push(Conductor {
            id: if pair {
                if i == 0 {
                    "positive"
                } else {
                    "negative"
                }
            } else {
                "signal"
            }
            .into(),
            role: "signal PEC".into(),
            bounds_m: [x - w / 2., h, x + w / 2., h + t],
        });
    }
    let mut dielectrics = vec![dielectric(
        &v[if micro { "material" } else { "bottomMaterial" }],
        "bottom",
        0.,
        h,
    )?];
    // Stripline interface is at the lower signal face. Top material fills gaps and above metal.
    dielectrics.push(if micro {
        Dielectric {
            id: "air".into(),
            y_min_m: h,
            y_max_m: h + t + top,
            relative_permittivity: 1.,
            state: "vacuum/air ideal".into(),
        }
    } else {
        dielectric(&v["topMaterial"], "top", h, h + t + top)?
    });
    let reference_y_m = if micro {
        vec![0.]
    } else {
        vec![0., h + t + top]
    };
    Ok(Geometry {
        conductors,
        dielectrics,
        bounds_m: [-w * 8., 0., w * 8., h + t + top],
        reference_y_m,
        length,
        kind: kind.into(),
        artificial_boundary: "homogeneous natural Neumann, increased independently of mesh".into(),
    })
}
