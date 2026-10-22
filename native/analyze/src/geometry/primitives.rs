//! Inscribed circular boundaries with a sagitta bound, never a fixed segment count.
use super::{
    areas::{self, AreaSet, Poly},
    GeometryOptions,
};
use crate::model::{self, Drill, DrillShape, PadShape, Point};
use geo::{Coord, LineString, Polygon};
use std::f64::consts::{PI, TAU};

pub struct Builder {
    pub options: GeometryOptions,
    pub generated: usize,
}
impl Builder {
    pub(super) fn reserve(&mut self, n: usize) -> Result<(), String> {
        let generated = self
            .generated
            .checked_add(n)
            .ok_or("Vertex counter overflow")?;
        if generated > self.options.max_generated_vertices {
            return Err("Generated vertex budget exceeded; tolerance was not relaxed".into());
        }
        self.generated = generated;
        Ok(())
    }
    fn rect(&mut self, w: f64, h: f64) -> Result<Poly, String> {
        self.reserve(4)?;
        rect(w, h)
    }
    pub fn circle(&mut self, at: Point, radius: f64) -> Result<Poly, String> {
        if !(radius > 0.0 && radius.is_finite()) {
            return Err("Non-positive circle radius".into());
        }
        if !at.x.is_finite()
            || !at.y.is_finite()
            || radius > 10000.
            || at.x.abs().max(at.y.abs()) + radius > 10000.
        {
            return Err("Circular boundary exceeds finite +/-10000 mm coordinate limit".into());
        }
        let angle = (1.0 - (self.options.max_deviation_mm / radius).min(1.0)).acos();
        let n = ((PI / angle).ceil() as usize).max(8).div_ceil(4) * 4;
        if n > self.options.max_vertices_per_curve {
            return Err("Curve vertex limit exceeded; tolerance was not relaxed".into());
        }
        self.reserve(n)?;
        let mut c: Vec<_> = (0..n)
            .map(|i| {
                let a = TAU * i as f64 / n as f64;
                Coord {
                    x: at.x + radius * a.cos(),
                    y: at.y + radius * a.sin(),
                }
            })
            .collect();
        // Exact cardinal coordinates avoid artificial trigonometric gaps.
        c[0] = Coord {
            x: at.x + radius,
            y: at.y,
        };
        c[n / 4] = Coord {
            x: at.x,
            y: at.y + radius,
        };
        c[n / 2] = Coord {
            x: at.x - radius,
            y: at.y,
        };
        c[3 * n / 4] = Coord {
            x: at.x,
            y: at.y - radius,
        };
        c.push(c[0]);
        Ok(Polygon::new(LineString(c), vec![]))
    }
    pub fn stroke(&mut self, points: &[Point], width: f64) -> Result<AreaSet, String> {
        let mut pieces = Vec::new();
        for p in points {
            pieces.push(self.circle(*p, width / 2.0)?);
        }
        for pair in points.windows(2) {
            let a = pair[0];
            let b = pair[1];
            let d = (b.x - a.x).hypot(b.y - a.y);
            if d == 0.0 {
                continue;
            }
            let x = -(b.y - a.y) / d * width / 2.0;
            let y = (b.x - a.x) / d * width / 2.0;
            self.reserve(4)?;
            pieces.push(areas::polygon(&model::Polygon {
                outer: vec![
                    Point {
                        x: a.x + x,
                        y: a.y + y,
                    },
                    Point {
                        x: b.x + x,
                        y: b.y + y,
                    },
                    Point {
                        x: b.x - x,
                        y: b.y - y,
                    },
                    Point {
                        x: a.x - x,
                        y: a.y - y,
                    },
                ],
                holes: vec![],
            })?);
        }
        Ok(areas::union(pieces))
    }
    pub fn shape(&mut self, s: &PadShape) -> Result<(AreaSet, f64), String> {
        let e = self.options.max_deviation_mm;
        Ok(match s {
            PadShape::Circle { diameter_mm } => (
                areas::one(self.circle(Point { x: 0., y: 0. }, diameter_mm / 2.)?),
                e,
            ),
            PadShape::Rect {
                width_mm,
                height_mm,
            } => (areas::one(self.rect(*width_mm, *height_mm)?), 0.),
            PadShape::Oval {
                width_mm,
                height_mm,
            } => (
                self.round_rect(*width_mm, *height_mm, width_mm.min(*height_mm) / 2.)?,
                e,
            ),
            PadShape::RoundRect {
                width_mm,
                height_mm,
                corner_radius_mm,
            } => (
                self.round_rect(*width_mm, *height_mm, *corner_radius_mm)?,
                e,
            ),
            PadShape::Polygon(p) => (areas::one(areas::polygon(p)?), 0.),
        })
    }
    fn round_rect(&mut self, w: f64, h: f64, r: f64) -> Result<AreaSet, String> {
        if r == 0. {
            return Ok(areas::one(self.rect(w, h)?));
        }
        if r > w.min(h) / 2. {
            return Err("Corner radius exceeds half minimum dimension".into());
        }
        let mut pieces = Vec::new();
        if w > 2. * r {
            pieces.push(self.rect(w - 2. * r, h)?);
        }
        if h > 2. * r {
            pieces.push(self.rect(w, h - 2. * r)?);
        }
        for x in [-w / 2. + r, w / 2. - r] {
            for y in [-h / 2. + r, h / 2. - r] {
                pieces.push(self.circle(Point { x, y }, r)?);
            }
        }
        Ok(areas::union(pieces))
    }
    pub fn drill(&mut self, drill: &Drill, at: Point, rotation: f64) -> Result<AreaSet, String> {
        let (shape, _) = self.shape(&match drill.shape {
            DrillShape::Round => PadShape::Circle {
                diameter_mm: drill.diameter_mm,
            },
            DrillShape::Slot { length_mm } => PadShape::Oval {
                width_mm: length_mm,
                height_mm: drill.diameter_mm,
            },
        })?;
        // Hole offset is pad-local; its own rotation rotates the slot, not offset.
        let offset = drill.offset.unwrap_or(Point { x: 0., y: 0. });
        let center = transform_point(offset, at, rotation);
        Ok(transform(
            shape,
            center,
            rotation + drill.rotation_deg.unwrap_or(0.),
        ))
    }
}
fn rect(w: f64, h: f64) -> Result<Poly, String> {
    areas::polygon(&model::Polygon {
        outer: vec![
            Point {
                x: -w / 2.,
                y: -h / 2.,
            },
            Point {
                x: w / 2.,
                y: -h / 2.,
            },
            Point {
                x: w / 2.,
                y: h / 2.,
            },
            Point {
                x: -w / 2.,
                y: h / 2.,
            },
        ],
        holes: vec![],
    })
}
pub fn transform_point(p: Point, at: Point, rotation: f64) -> Point {
    let (s, c) = rotation.to_radians().sin_cos();
    Point {
        x: at.x + p.x * c - p.y * s,
        y: at.y + p.x * s + p.y * c,
    }
}
pub fn transform(mut shape: AreaSet, at: Point, rotation: f64) -> AreaSet {
    use geo::MapCoordsInPlace;
    shape.map_coords_in_place(|p| {
        let q = transform_point(Point { x: p.x, y: p.y }, at, rotation);
        Coord { x: q.x, y: q.y }
    });
    shape
}
