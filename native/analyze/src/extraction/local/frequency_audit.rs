use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

const TRAINING_HZ: [f64; 2] = [10_000., 100_000.];
const MAX_SAMPLES: usize = 33;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Request {
    pub interval_hz: [f64; 2],
    pub max_samples: usize,
    pub max_step_hz: f64,
    pub relative_tolerance: f64,
    pub impedance_floor_ohm: f64,
    pub loss_floor_w: f64,
}

impl Request {
    pub fn validate(&self) -> Result<(), String> {
        let [start, stop] = self.interval_hz;
        if !(1000. ..=100000.).contains(&start)
            || !(start..=100000.).contains(&stop)
            || !(1..=MAX_SAMPLES).contains(&self.max_samples)
            || ![
                self.max_step_hz,
                self.relative_tolerance,
                self.impedance_floor_ohm,
                self.loss_floor_w,
            ]
            .into_iter()
            .all(|value| value.is_finite() && value > 0.)
            || self.relative_tolerance > 1.
        {
            return Err("LOCAL_FREQUENCY_AUDIT: expected bounded positive gates and a 1 kHz..100 kHz interval".into());
        }
        Ok(())
    }

    pub fn retained_bytes(&self, ports: usize) -> usize {
        // Result JSON values, queue and the compact full/reduced samples coexist.
        self.max_samples * (ports * ports + ports + 1) * 1024
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Observables {
    pub impedance_ohm: Vec<[f64; 2]>,
    pub total_copper_loss_w: Vec<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    pub full: Observables,
    pub reduced: Observables,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Errors {
    impedance: f64,
    copper_loss: f64,
}

impl Errors {
    fn passes(&self, tolerance: f64) -> bool {
        self.impedance <= tolerance && self.copper_loss <= tolerance
    }
}

fn errors(
    actual: &Observables,
    left: &Observables,
    right: &Observables,
    t: f64,
    request: &Request,
) -> Errors {
    let score = |error: f64, scale: f64| {
        let value = error / scale;
        if value.is_finite() && scale.is_finite() {
            value
        } else {
            f64::MAX
        }
    };
    let interpolate = |a: f64, b: f64| (1. - t) * a + t * b;
    Errors {
        impedance: actual
            .impedance_ohm
            .iter()
            .zip(&left.impedance_ohm)
            .zip(&right.impedance_ohm)
            .map(|(([re, im], [lr, li]), [rr, ri])| {
                score(
                    (interpolate(*lr, *rr) - re).hypot(interpolate(*li, *ri) - im),
                    re.hypot(*im).max(request.impedance_floor_ohm),
                )
            })
            .fold(0., f64::max),
        copper_loss: actual
            .total_copper_loss_w
            .iter()
            .zip(&left.total_copper_loss_w)
            .zip(&right.total_copper_loss_w)
            .map(|((&loss, &l), &r)| {
                score(
                    (interpolate(l, r) - loss).abs(),
                    loss.abs().max(request.loss_floor_w),
                )
            })
            .fold(0., f64::max),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Holdout {
    interval_hz: [f64; 2],
    frequency_hz: f64,
    interpolation_errors: Errors,
    reduction_errors: Errors,
    accepted: bool,
    promoted: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Gap {
    interval_hz: [f64; 2],
    reason: String,
}

fn gap(interval_hz: [f64; 2], reason: impl Into<String>) -> Gap {
    Gap {
        interval_hz,
        reason: reason.into(),
    }
}

pub fn run(
    request: &Request,
    ports: usize,
    mut evaluate: impl FnMut(f64) -> Result<Sample, String>,
) -> serde_json::Value {
    let [start, stop] = request.interval_hz;
    let mut samples = Vec::new();
    let mut fit = Vec::new();
    let mut seed_checks = Vec::new();
    let mut holdouts = Vec::new();
    let mut validated = Vec::new();
    let mut gaps = Vec::new();
    let mut count = 0;
    let mut sample = |frequency| -> Result<Sample, String> {
        if count == request.max_samples {
            return Err("sample-budget".into());
        }
        count += 1;
        let point = evaluate(frequency)?;
        if !(1..=16).contains(&ports)
            || [&point.full, &point.reduced].iter().any(|observables| {
                observables.impedance_ohm.len() != ports * ports
                    || observables.total_copper_loss_w.len() != ports
                    || observables
                        .impedance_ohm
                        .iter()
                        .flatten()
                        .any(|value| !value.is_finite())
                    || observables
                        .total_copper_loss_w
                        .iter()
                        .any(|value| !value.is_finite() || *value < 0.)
            })
        {
            return Err("invalid-physical-observables".into());
        }
        Ok(point)
    };

    if start == stop {
        if TRAINING_HZ.contains(&start) {
            gaps.push(gap([start, stop], "training-frequency"));
        } else {
            match sample(start) {
                Ok(point) => {
                    let comparison =
                        errors(&point.full, &point.reduced, &point.reduced, 0., request);
                    if comparison.passes(request.relative_tolerance) {
                        validated.push([start, stop]);
                    } else {
                        gaps.push(gap([start, stop], "reduction-error"));
                    }
                    samples.push((start, point));
                    holdouts.push(Holdout {
                        interval_hz: [start, stop],
                        frequency_hz: start,
                        interpolation_errors: Errors {
                            impedance: 0.,
                            copper_loss: 0.,
                        },
                        accepted: comparison.passes(request.relative_tolerance),
                        reduction_errors: comparison,
                        promoted: false,
                    });
                }
                Err(reason) => gaps.push(gap([start, stop], reason)),
            }
        }
    } else {
        let seeds = [
            Some(start),
            (start < TRAINING_HZ[0] && TRAINING_HZ[0] < stop).then_some(TRAINING_HZ[0]),
            Some(stop),
        ];
        let seeds: Vec<_> = seeds.into_iter().flatten().collect();
        let indices: Vec<_> = seeds
            .iter()
            .map(|&frequency| match sample(frequency) {
                Ok(point) => {
                    let comparison = errors(&point.full, &point.reduced, &point.reduced, 0., request);
                    let accepted = comparison.passes(request.relative_tolerance);
                    seed_checks.push(serde_json::json!({
                        "frequencyHz": frequency, "reductionErrors": comparison, "accepted": accepted,
                    }));
                    samples.push((frequency, point));
                    if accepted {
                        fit.push(frequency);
                        Ok(samples.len() - 1)
                    } else {
                        Err("endpoint-reduction-error".into())
                    }
                }
                Err(reason) => Err(reason),
            })
            .collect();
        let mut pending = VecDeque::new();
        for (index, edge) in indices.windows(2).enumerate() {
            match (&edge[0], &edge[1]) {
                (Ok(left), Ok(right)) => pending.push_back((*left, *right)),
                (Err(reason), _) | (_, Err(reason)) => {
                    gaps.push(gap([seeds[index], seeds[index + 1]], reason))
                }
            }
        }
        while let Some((left, right)) = pending.pop_front() {
            let a = samples[left].0;
            let b = samples[right].0;
            let width = b - a;
            let midpoint = a + width / 2.;
            if !(a < midpoint && midpoint < b) {
                gaps.push(gap([a, b], "floating-point-resolution"));
                continue;
            }
            let point = match sample(midpoint) {
                Ok(point) => point,
                Err(reason) => {
                    gaps.push(gap([a, b], reason));
                    continue;
                }
            };
            let interpolation = errors(
                &point.full,
                &samples[left].1.reduced,
                &samples[right].1.reduced,
                (midpoint - a) / width,
                request,
            );
            let reduction = errors(&point.full, &point.reduced, &point.reduced, 0., request);
            let reduced_ok = reduction.passes(request.relative_tolerance);
            let accepted = reduced_ok
                && width <= request.max_step_hz
                && interpolation.passes(request.relative_tolerance);
            holdouts.push(Holdout {
                interval_hz: [a, b],
                frequency_hz: midpoint,
                interpolation_errors: interpolation,
                reduction_errors: reduction,
                accepted,
                promoted: reduced_ok && !accepted,
            });
            samples.push((midpoint, point));
            let index = samples.len() - 1;
            if accepted {
                validated.push([a, b]);
            } else if reduced_ok {
                fit.push(midpoint);
                pending.push_back((left, index));
                pending.push_back((index, right));
            } else {
                // Refinement cannot repair a demonstrated error in the fixed reduced model.
                gaps.push(gap([a, b], "reduction-error"));
            }
        }
    }

    validated.sort_by(|a, b| a[0].total_cmp(&b[0]));
    gaps.sort_by(|a, b| a.interval_hz[0].total_cmp(&b.interval_hz[0]));
    fit.sort_by(f64::total_cmp);
    serde_json::json!({
        "version": 1, "status": if gaps.is_empty() { "validated" } else { "unverified" },
        "policy": request, "requestedIntervalHz": request.interval_hz, "validatedIntervalsHz": validated,
        "unverified": gaps, "basisTrainingFrequenciesHz": TRAINING_HZ, "interpolationFrequenciesHz": fit,
        "seedChecks": seed_checks, "holdouts": holdouts, "samples": samples, "sampleAttempts": count,
        "observables": "Intrinsic complex Z in input-major order and total Cu RMS Joule loss per 1 A unit-port drive"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> Request {
        Request {
            interval_hz: [1000., 20_000.],
            max_samples: 9,
            max_step_hz: 6_000.,
            relative_tolerance: 0.02,
            impedance_floor_ohm: 1e-6,
            loss_floor_w: 1e-12,
        }
    }

    fn point(f: f64, reduction_factor: f64) -> Result<Sample, String> {
        let v = f / 1e5;
        Ok(Sample {
            full: Observables {
                impedance_ohm: vec![[v, v]],
                total_copper_loss_w: vec![v],
            },
            reduced: Observables {
                impedance_ohm: vec![[v * reduction_factor, v]],
                total_copper_loss_w: vec![v],
            },
        })
    }

    #[test]
    fn splits_at_training_and_uses_fresh_holdouts_after_promotion() {
        let report = run(&request(), 1, |f| point(f, 1.));
        assert_eq!(report["status"], "validated");
        let holdouts = report["holdouts"].as_array().unwrap();
        assert_eq!(holdouts.len(), 6);
        assert!(holdouts
            .iter()
            .all(|p| !TRAINING_HZ.contains(&p["frequencyHz"].as_f64().unwrap())));
        assert_eq!(holdouts[0]["promoted"], true);
    }

    #[test]
    fn refuses_reduced_mismatch_and_preserves_budget_gaps() {
        let report = run(&request(), 1, |f| {
            point(f, if f == 5500. { 2. } else { 1. })
        });
        assert_eq!(report["status"], "unverified");
        assert_eq!(report["unverified"][0]["reason"], "reduction-error");
        let report = run(
            &Request {
                max_samples: 3,
                ..request()
            },
            1,
            |f| point(f, 1.),
        );
        assert_eq!(report["unverified"][0]["reason"], "sample-budget");
    }

    #[test]
    fn step_gate_exposes_a_feature_missed_by_coarse_midpoints() {
        let report = run(&request(), 1, |f| {
            let factor = if f == 12_500. { 2. } else { 1. };
            let mut sample = point(f, 1.)?;
            sample.full.impedance_ohm[0][0] *= factor;
            sample.reduced.impedance_ohm[0][0] *= factor;
            Ok(sample)
        });
        assert_eq!(report["status"], "unverified");
        let feature = report["holdouts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|h| h["frequencyHz"] == 12_500.)
            .unwrap();
        assert_eq!(feature["accepted"], false);
        assert_eq!(feature["promoted"], true);
        assert!(
            feature["interpolationErrors"]["impedance"]
                .as_f64()
                .unwrap()
                > 0.02
        );
    }

    #[test]
    fn refuses_missing_mismatched_nonfinite_and_negative_observables() {
        let request = Request {
            interval_hz: [1000., 2000.],
            ..request()
        };
        for corruption in 0..4 {
            let report = run(&request, 1, |f| {
                let mut sample = point(f, 1.)?;
                if f == 1500. {
                    match corruption {
                        0 => sample.full.impedance_ohm.clear(),
                        1 => sample.reduced.impedance_ohm.push([1., 1.]),
                        2 => sample.full.impedance_ohm[0][0] = f64::NAN,
                        _ => sample.reduced.total_copper_loss_w[0] = -1.,
                    }
                }
                Ok(sample)
            });
            assert_eq!(report["status"], "unverified");
            assert_eq!(
                report["unverified"][0]["reason"],
                "invalid-physical-observables"
            );
            assert!(report["validatedIntervalsHz"]
                .as_array()
                .unwrap()
                .is_empty());
        }
    }

    #[test]
    fn rejects_opposite_endpoint_errors_that_cancel_at_the_midpoint() {
        let request = Request {
            interval_hz: [1000., 2000.],
            ..request()
        };
        let report = run(&request, 1, |f| {
            let mut sample = point(f, 1.)?;
            sample.full.impedance_ohm[0] = [1., 0.];
            sample.reduced.impedance_ohm[0] = [1. + 0.2 * (f - 1500.) / 500., 0.];
            Ok(sample)
        });
        assert_eq!(report["status"], "unverified");
        assert_eq!(
            report["unverified"][0]["reason"],
            "endpoint-reduction-error"
        );
        assert!(report["validatedIntervalsHz"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(report["holdouts"].as_array().unwrap().is_empty());
        let seeds = report["seedChecks"].as_array().unwrap();
        assert_eq!(seeds.len(), 2);
        assert!(seeds.iter().all(|seed| seed["accepted"] == false));
        assert_eq!(report["sampleAttempts"], 2);
    }
}
