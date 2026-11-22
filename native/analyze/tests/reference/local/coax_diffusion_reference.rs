use faer::c64 as C;
use std::f64::consts::PI;

pub const MU0: f64 = 4e-7 * PI;

#[derive(Clone, Copy)]
pub struct Coax {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub sigma: f64,
}

pub struct Reference {
    pub impedance: C,
    pub inner_loss: f64,
    pub return_loss: f64,
}

// q = (kr)^2/4. I1/(kr/2) avoids division by k near DC.
fn bessel_series(q: C, order: usize) -> C {
    let mut sum = C::new(1., 0.);
    let mut term = sum;
    for n in 1..=160 {
        term = term * q * (1. / (n * (n + order)) as f64);
        sum = sum + term;
        if term.norm() <= 1e-15 * sum.norm() {
            return sum;
        }
    }
    panic!("bounded coax series failed to converge");
}

fn derivative(r: f64, y: [C; 2], k2: C) -> [C; 2] {
    [y[1] * (1. / r), k2 * y[0] * r]
}

// Integrate E and r E' in the annulus from the zero exterior H boundary.
fn return_field(coax: Coax, k2: C, steps: usize) -> (Vec<C>, C) {
    let step = (coax.b - coax.c) / steps as f64;
    let mut y = [C::new(1., 0.), C::new(0., 0.)];
    let mut fields = vec![y[0]];
    for index in 0..steps {
        let r = coax.c + index as f64 * step;
        let a = derivative(r, y, k2);
        let b = derivative(
            r + step / 2.,
            std::array::from_fn(|i| y[i] + a[i] * (step / 2.)),
            k2,
        );
        let c = derivative(
            r + step / 2.,
            std::array::from_fn(|i| y[i] + b[i] * (step / 2.)),
            k2,
        );
        let d = derivative(r + step, std::array::from_fn(|i| y[i] + c[i] * step), k2);
        for i in 0..2 {
            y[i] = y[i] + (a[i] + b[i] * 2. + c[i] * 2. + d[i]) * (step / 6.);
        }
        fields.push(y[0]);
    }
    (fields, y[1])
}

fn rms_loss(sigma: f64, low: f64, high: f64, samples: &[C]) -> f64 {
    let steps = samples.len() - 1;
    assert!(steps >= 2 && steps % 2 == 0);
    let h = (high - low) / steps as f64;
    let sum = samples
        .iter()
        .enumerate()
        .map(|(i, field)| {
            let weight = if i == 0 || i == steps {
                1.
            } else if i % 2 == 0 {
                2.
            } else {
                4.
            };
            weight * (low + i as f64 * h) * field.norm().powi(2)
        })
        .sum::<f64>();
    2. * PI * sigma * h * sum / 3.
}

// Series impedance and losses per meter for one RMS ampere, exp(+jωt).
pub fn reference(coax: Coax, frequency: f64, steps: usize) -> Reference {
    let Coax { a, b, c, sigma } = coax;
    assert!(a > 0. && a < b && b < c && sigma > 0. && frequency >= 0.);
    let inner_dc = 1. / (sigma * PI * a.powi(2));
    let return_dc = 1. / (sigma * PI * (c.powi(2) - b.powi(2)));
    if frequency == 0. {
        return Reference {
            impedance: C::new(inner_dc + return_dc, 0.),
            inner_loss: inner_dc,
            return_loss: return_dc,
        };
    }
    let omega = 2. * PI * frequency;
    let k2 = C::new(0., omega * MU0 * sigma);
    assert!(k2.norm().sqrt() * a <= 40. && k2.norm().sqrt() * (c - b) <= 40.);
    let inner_scale = C::new(inner_dc, 0.) / bessel_series(k2 * (a.powi(2) / 4.), 1);
    let inner: Vec<_> = (0..=steps)
        .map(|i| {
            let r = a * i as f64 / steps as f64;
            inner_scale * bessel_series(k2 * (r.powi(2) / 4.), 0)
        })
        .collect();
    let (mut outer, boundary_flux) = return_field(coax, k2, steps);
    let return_scale = C::new(0., omega * MU0 / (2. * PI)) / boundary_flux;
    for value in &mut outer {
        *value = *value * return_scale;
    }
    outer.reverse();
    Reference {
        impedance: inner[steps] - outer[0] + C::new(0., omega * MU0 * (b / a).ln() / (2. * PI)),
        inner_loss: rms_loss(sigma, 0., a, &inner),
        return_loss: rms_loss(sigma, b, c, &outer),
    }
}
