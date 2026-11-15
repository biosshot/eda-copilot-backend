//! Independent bounded thermal RC networks, CPU/F64. No electrical feedback.
pub mod board;
mod boundary;
#[cfg(test)]
mod coupon_experiment;
mod mesh;
mod precondition;
mod profile;
#[cfg(test)]
mod performance_experiment;
mod refinement;
mod solve;
use crate::electrical::Waveform;
use serde::{Deserialize, Serialize};
use serde_json::Value;
pub use solve::run;

#[derive(Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Signal {
    Constant(f64),
    Waveform(Waveform),
}
impl Signal {
    fn heap_bytes(&self) -> usize {
        match self {
            Self::Waveform(Waveform::Pwl { points, .. }) => points
                .capacity()
                .saturating_mul(std::mem::size_of::<[f64; 2]>()),
            _ => 0,
        }
    }
    fn sample(&self, t: f64) -> f64 {
        match self {
            Self::Constant(x) => *x,
            Self::Waveform(w) => w.sample(t),
        }
    }
    fn validate(&self) -> Result<(), String> {
        match self {
            Self::Constant(x) if x.is_finite() => Ok(()),
            Self::Waveform(w) => w.validate(),
            _ => Err("THERMAL_INPUT: nonfinite signal".into()),
        }
    }
    fn next(&self, t: f64) -> f64 {
        match self {
            Self::Constant(_) => f64::INFINITY,
            Self::Waveform(w) => w.next(t),
        }
    }
    fn minimum(&self) -> f64 {
        match self {
            Self::Constant(v) => *v,
            Self::Waveform(w) => match w {
                Waveform::Step { from, to, .. } => from.min(*to),
                Waveform::Sine {
                    offset, amplitude, ..
                } => offset - amplitude,
                Waveform::Pulse { low, high, .. } => low.min(*high),
                Waveform::Pwl { points, .. } => {
                    points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min)
                }
            },
        }
    }
    fn max_step(&self) -> f64 {
        match self {
            Self::Waveform(Waveform::Sine { period, .. }) => period / 32.,
            _ => f64::INFINITY,
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    pub boundary: Option<String>,
    pub heat_capacity_j_per_k: Option<f64>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TablePoint {
    pub delta_temperature_k: f64,
    pub heat_flow_w: f64,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Cooling {
    pub resistance_k_per_w: Option<f64>,
    pub table: Option<Vec<TablePoint>>,
    pub scale: Option<Signal>,
}
impl Cooling {
    fn heap_bytes(&self) -> usize {
        self.table
            .as_ref()
            .map_or(0, |t| {
                t.capacity()
                    .saturating_mul(std::mem::size_of::<TablePoint>())
            })
            .saturating_add(self.scale.as_ref().map_or(0, Signal::heap_bytes))
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Link {
    pub id: String,
    pub from: String,
    pub to: String,
    pub cooling: Cooling,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Source {
    pub node: String,
    pub power_w: Signal,
    pub weight: Option<f64>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Network {
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
    pub sources: Vec<Source>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Request {
    pub context_id: String,
    pub network: Network,
    pub mode: String,
    pub ambient_c: Signal,
    pub initial_c: Option<Value>,
    pub initial: Option<Value>,
    pub time: Option<crate::electrical::Time>,
    pub at_s: Option<f64>,
    pub numerics: Option<crate::electrical::Numerics>,
    pub budget: Option<crate::electrical::Budget>,
    pub board: Option<board::BoardInput>,
    #[serde(skip)]
    pub spatial: Option<board::Spatial>,
    #[serde(skip)]
    pub retained_bytes: usize,
    #[serde(skip)]
    pub capture_coordinates: bool,
    #[serde(skip)]
    pub observation_times: Option<Vec<f64>>,
}
