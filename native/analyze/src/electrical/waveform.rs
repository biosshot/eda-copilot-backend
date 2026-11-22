use super::*;
#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Waveform {
    Step {
        from: f64,
        to: f64,
        #[serde(rename = "atS")]
        at: f64,
        #[serde(rename = "riseS")]
        rise: f64,
    },
    Sine {
        offset: f64,
        amplitude: f64,
        #[serde(rename = "periodS")]
        period: f64,
        #[serde(rename = "phaseDeg")]
        phase: f64,
    },
    Pulse {
        low: f64,
        high: f64,
        #[serde(rename = "delayS")]
        delay: f64,
        #[serde(rename = "riseS")]
        rise: f64,
        #[serde(rename = "highS")]
        high_s: f64,
        #[serde(rename = "fallS")]
        fall: f64,
        #[serde(rename = "periodS")]
        period: f64,
        count: usize,
    },
    Pwl {
        points: Vec<[f64; 2]>,
        interpolation: String,
        repeat: Option<Repeat>,
    },
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Repeat {
    #[serde(rename = "periodS")]
    period: f64,
    count: usize,
}
/// Use the same rounded absolute boundaries as next(). Never snap a left
/// neighbour onto a boundary: subtraction/remainder alone can select the wrong cycle.
fn cycle(t: f64, delay: f64, period: f64, count: usize) -> usize {
    let mut k = (((t - delay) / period).floor() as usize).min(count);
    while k > 0 && t < delay + k as f64 * period {
        k -= 1;
    }
    while k < count && t >= delay + (k + 1) as f64 * period {
        k += 1;
    }
    k
}
fn boundary(delay: f64, k: usize, period: f64, offset: f64) -> f64 {
    if offset == period {
        delay + (k + 1) as f64 * period
    } else {
        (delay + k as f64 * period) + offset
    }
}
impl Waveform {
    pub fn validate(&self) -> Result<()> {
        let finite = serde_json::to_value(self).map_err(|e| fail("NETWORK_WAVEFORM", e))?;
        fn valid(v: &Value) -> bool {
            match v {
                Value::Null => false,
                Value::Number(n) => n.as_f64().is_some_and(f64::is_finite),
                Value::Array(a) => a.iter().all(valid),
                Value::Object(o) => o
                    .iter()
                    .all(|(k, v)| k == "repeat" && v.is_null() || valid(v)),
                _ => true,
            }
        }
        let ok = match self {
            Self::Step { at, rise, .. } => *at >= 0. && *rise >= 0.,
            Self::Sine {
                amplitude, period, ..
            } => *amplitude >= 0. && *period > 0.,
            Self::Pulse {
                delay,
                rise,
                high_s,
                fall,
                period,
                count,
                ..
            } => {
                *delay >= 0.
                    && *rise >= 0.
                    && *high_s >= 0.
                    && *fall >= 0.
                    && *period > 0.
                    && rise + high_s + fall <= *period
                    && *count > 0
                    && *count <= 100_000
            }
            Self::Pwl {
                points,
                interpolation,
                repeat,
            } => {
                points.len() >= 2
                    && points.len() <= 100_000
                    && points[0][0] == 0.
                    && points.windows(2).all(|w| w[1][0] > w[0][0])
                    && ["linear", "hold"].contains(&interpolation.as_str())
                    && repeat.as_ref().is_none_or(|r| {
                        r.period > 0.
                            && r.count > 0
                            && r.count <= 100_000
                            && points.last().unwrap()[0] == r.period
                    })
            }
        };
        if !ok || !valid(&finite) {
            return Err(fail("NETWORK_WAVEFORM", "Invalid waveform"));
        }
        let resolved = |width: f64, end: f64| {
            end.is_finite() && (width == 0. || width > 32. * f64::EPSILON * end.abs().max(1e-30))
        };
        let resolution = match self {
            Self::Step { at, rise, .. } => resolved(*rise, at + rise),
            Self::Pulse {
                delay,
                rise,
                high_s,
                fall,
                period,
                count,
                ..
            } => {
                let end = delay + *count as f64 * period;
                [*rise, *high_s, *fall, *period]
                    .iter()
                    .all(|v| resolved(*v, end))
            }
            Self::Pwl { points, repeat, .. } => {
                let end = repeat
                    .as_ref()
                    .map(|r| r.period * r.count as f64)
                    .unwrap_or(points.last().unwrap()[0]);
                points.windows(2).all(|w| resolved(w[1][0] - w[0][0], end))
            }
            _ => true,
        };
        if !resolution {
            return Err(fail(
                "NETWORK_WAVEFORM_RESOLUTION",
                "Formula intervals/fronts are below the F64 absolute-time resolution",
            ));
        }
        Ok(())
    }
    pub fn sample(&self, t: f64) -> f64 {
        match self {
            Self::Step { from, to, at, rise } => {
                if t < *at {
                    *from
                } else if *rise == 0. {
                    *to
                } else {
                    from + (to - from) * ((t - at) / rise).min(1.)
                }
            }
            Self::Sine {
                offset,
                amplitude,
                period,
                phase,
            } => {
                offset
                    + amplitude
                        * (2. * std::f64::consts::PI * t / period + phase.to_radians()).sin()
            }
            Self::Pulse {
                low,
                high,
                delay,
                rise,
                high_s,
                fall,
                period,
                count,
            } => {
                if t < *delay || t >= delay + *count as f64 * period {
                    return *low;
                }
                let k = cycle(t, *delay, *period, *count);
                let base = boundary(*delay, k, *period, 0.);
                if t < boundary(*delay, k, *period, *rise) {
                    low + (high - low) * (t - base) / rise
                } else if t < boundary(*delay, k, *period, rise + high_s) {
                    *high
                } else if t < boundary(*delay, k, *period, rise + high_s + fall) {
                    high + (low - high) * (t - boundary(*delay, k, *period, rise + high_s)) / fall
                } else {
                    *low
                }
            }
            Self::Pwl {
                points,
                interpolation,
                repeat,
            } => {
                let k = if let Some(r) = repeat {
                    if t >= r.period * r.count as f64 {
                        return points.last().unwrap()[1];
                    }
                    cycle(t, 0., r.period, r.count)
                } else {
                    0
                };
                let at = |offset| {
                    repeat
                        .as_ref()
                        .map(|r| boundary(0., k, r.period, offset))
                        .unwrap_or(offset)
                };
                for w in points.windows(2) {
                    if t < at(w[1][0]) {
                        return if interpolation == "hold" {
                            w[0][1]
                        } else {
                            w[0][1]
                                + (w[1][1] - w[0][1]) * (t - at(w[0][0]))
                                    / (at(w[1][0]) - at(w[0][0]))
                        };
                    }
                }
                points.last().unwrap()[1]
            }
        }
    }
    pub fn max_step(&self) -> f64 {
        match self {
            Self::Sine { period, .. } => period / 50.,
            _ => f64::INFINITY,
        }
    }
    pub fn discontinuous(&self) -> bool {
        match self {
            Self::Step { rise, .. } => *rise == 0.,
            Self::Pulse { rise, fall, .. } => *rise == 0. || *fall == 0.,
            Self::Pwl {
                interpolation,
                repeat,
                points,
            } => {
                interpolation == "hold"
                    || repeat.is_some() && points.first().unwrap()[1] != points.last().unwrap()[1]
            }
            _ => false,
        }
    }
    /// Next formula boundary strictly after t. No expanded repeated-event array.
    pub fn next(&self, t: f64) -> f64 {
        let after = |v: f64| {
            if v > t + 16. * f64::EPSILON * t.abs().max(1e-30) {
                v
            } else {
                f64::INFINITY
            }
        };
        match self {
            Self::Step { at, rise, .. } => after(*at).min(after(at + rise)),
            Self::Sine { .. } => f64::INFINITY,
            Self::Pulse {
                delay,
                rise,
                high_s,
                fall,
                period,
                count,
                ..
            } => {
                let k = if t < *delay {
                    0
                } else {
                    cycle(t, *delay, *period, *count)
                };
                let mut next = f64::INFINITY;
                for j in k..=(k + 1).min(*count) {
                    let base = delay + j as f64 * period;
                    if j == *count {
                        next = next.min(after(base));
                    } else {
                        for d in [0., *rise, rise + high_s, rise + high_s + fall] {
                            next = next.min(after(boundary(*delay, j, *period, d)));
                        }
                    }
                }
                next
            }
            Self::Pwl { points, repeat, .. } => {
                let k = repeat
                    .as_ref()
                    .map(|r| cycle(t, 0., r.period, r.count))
                    .unwrap_or(0);
                let count = repeat.as_ref().map(|r| r.count).unwrap_or(1);
                let period = repeat.as_ref().map(|r| r.period).unwrap_or(0.);
                let mut next = f64::INFINITY;
                for j in k..=(k + 1).min(count.saturating_sub(1)) {
                    for p in points {
                        next = next.min(after(boundary(0., j, period, p[0])));
                    }
                }
                next
            }
        }
    }
}
