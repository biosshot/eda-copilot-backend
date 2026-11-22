//! Verified CAD-outline/Boolean-coordinate correspondence, never extrapolation.
use geo::MultiPolygon;
use serde_json::{json, Value};
type P = [f64; 2];
#[derive(Clone)]
pub(super) struct Source {
    pub identity: String,
    ring: Vec<P>,
    boolean_axis_bound_mm: f64,
}
impl Source {
    pub fn from_report(report: &Value) -> Option<Self> {
        let identity = report["spatial"]["boardIdentity"].as_str()?.to_owned();
        let g = &report["spatial"]["geometry"];
        let ring: Vec<P> = serde_json::from_value(g["outline"]["outer"].clone()).ok()?;
        let boolean_axis_bound_mm = g["booleanErrorBoundMm"].as_f64()?;
        if identity.is_empty()
            || ring.len() < 3
            || ring.len() > 1000
            || !boolean_axis_bound_mm.is_finite()
            || boolean_axis_bound_mm <= 0.
            || ring.iter().flatten().any(|v| !v.is_finite())
        {
            return None;
        }
        Some(Self {
            identity,
            ring,
            boolean_axis_bound_mm,
        })
    }
    pub fn bytes(&self) -> usize {
        self.identity.capacity() + self.ring.capacity() * 16 + 64
    }
}
struct Mapping {
    polygon: usize,
    domain: Vec<P>,
    source: Vec<P>,
    roundoff: f64,
    representation_limit: f64,
}
pub(super) struct Localizer {
    mappings: Vec<Mapping>,
}
pub(super) struct Proof {
    pub point: P,
    pub displacement: f64,
    pub report: Value,
}
fn distance(a: P, b: P) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
impl Localizer {
    pub fn new(domain: &MultiPolygon, source: Option<&Source>, identity_matches: bool) -> Self {
        let mut mappings = vec![];
        let Some(source) = source.filter(|_| identity_matches) else {
            return Self { mappings };
        };
        let scale = source
            .ring
            .iter()
            .flatten()
            .map(|x| x.abs())
            .fold(1., f64::max);
        let roundoff = 8. * f64::EPSILON * scale;
        // geometry/mod.rs reports axis bound=max(1e-12,span*64/2^29).
        let representation_limit =
            std::f64::consts::SQRT_2 * source.boolean_axis_bound_mm + roundoff;
        for (polygon, poly) in domain.0.iter().enumerate() {
            let ring: Vec<P> = poly.exterior().0[..poly.exterior().0.len().saturating_sub(1)]
                .iter()
                .map(|p| [p.x, p.y])
                .collect();
            if ring.len() != source.ring.len() {
                continue;
            }
            let n = ring.len();
            let mut matches = vec![];
            for start in 0..n {
                for direction in [1isize, -1] {
                    let mapped: Vec<P> = (0..n)
                        .map(|i| {
                            source.ring[(start as isize + direction * i as isize)
                                .rem_euclid(n as isize)
                                as usize]
                        })
                        .collect();
                    if ring
                        .iter()
                        .zip(&mapped)
                        .all(|(a, b)| distance(*a, *b) <= representation_limit)
                    {
                        matches.push(mapped);
                    }
                }
            }
            if matches.len() == 1 {
                mappings.push(Mapping {
                    polygon,
                    domain: ring,
                    source: matches.pop().unwrap(),
                    roundoff,
                    representation_limit,
                });
            }
        }
        Self { mappings }
    }
    pub fn at(&self, p: P) -> Option<Proof> {
        for m in &self.mappings {
            for edge in 0..m.domain.len() {
                let next = (edge + 1) % m.domain.len();
                let a = m.domain[edge];
                let b = m.domain[next];
                let dx = b[0] - a[0];
                let dy = b[1] - a[1];
                let l = dx * dx + dy * dy;
                if l == 0. {
                    continue;
                }
                let t = ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l;
                if !(0. ..=1.).contains(&t) {
                    continue;
                }
                if distance(p, [a[0] + t * dx, a[1] + t * dy]) > m.roundoff {
                    continue;
                }
                let sa = m.source[edge];
                let sb = m.source[next];
                let q = [sa[0] + t * (sb[0] - sa[0]), sa[1] + t * (sb[1] - sa[1])];
                let limit = distance(a, sa).max(distance(b, sb)) + 2. * m.roundoff;
                let displacement = distance(p, q);
                if displacement > limit {
                    continue;
                }
                return Some(Proof {
                    point: q,
                    displacement,
                    report: json!({"pointMm":p,"localizedPointMm":q,"displacementMm":displacement,"limitMm":limit,"representationLimitMm":m.representation_limit,"polygon":m.polygon,"edge":edge,"fraction":t,"transportedEdge":[a,b],"sourceEdge":[sa,sb],"roundoffMm":m.roundoff,"formula":"max(endpoint transport displacements)+2*(8*EPSILON*max(1,abs(source coordinates))); unique whole-outline cyclic correspondence within sqrt(2)*reported Boolean axis bound+roundoff"}),
                });
            }
        }
        None
    }
}
