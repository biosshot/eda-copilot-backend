//! Rational (vector fitting) model of sampled multiport Y/Z/S data with a verified passivity check.
//! Relaxed vector fitting (Gustavsen 2006), common poles, per-element QR reduction of the pole step,
//! real conjugate-pair parametrization, normalized s = j f / fmax. Passivity is only claimed after a
//! dense-grid check of the returned model; enforcement is opt-in and never silently claims success.
use super::frequency::math::{Matrix, C};
use super::*;
use faer::{Mat, Side};

#[cfg(test)]
#[path = "vector_fit_tests.rs"]
mod tests;

const MAX_PORTS: usize = 8;
const MAX_POLES: usize = 128;
const PASSIVITY_TOLERANCE: f64 = 1e-9;
const ENFORCE_ITERATIONS: usize = 40;
const MAX_ROWS: usize = 150;
/// Relative Tikhonov damping of the constraint system: rows outnumber the free parameters, so the exact
/// solve is inconsistent; damping turns it into a damped least-squares step.
const ENFORCE_DAMPING: f64 = 1e-6;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Request {
    pub kind: String,
    pub frequencies_hz: Vec<f64>,
    pub samples: Vec<Matrix>,
    /// Pole count, or the string "auto" (chosen from the fit points only, see `max_poles`).
    pub poles: PoleSpec,
    /// Upper bound for `poles: "auto"` (default: as many as keep the problem 2x overdetermined).
    pub max_poles: Option<usize>,
    pub iterations: Option<usize>,
    pub weights: Option<Vec<f64>>,
    pub fit_d: Option<bool>,
    pub fit_e: Option<bool>,
    pub enforce_passivity: Option<bool>,
    pub max_relative_error: Option<f64>,
    pub budget: Option<Budget>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum PoleSpec {
    Count(usize),
    Auto(String),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoleTrial {
    pub poles: usize,
    /// Worst entry's rms error relative to that entry's own peak (the quantity the selection uses).
    pub entry_relative_error: Option<f64>,
}

/// Automatic pole count: only the fit points are used.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoleSelection {
    pub chosen: usize,
    pub max_tried: usize,
    pub trials: Vec<PoleTrial>,
    pub rule: String,
}

/// Pole-residue model H(s) = D + s E + sum R_p / (s - p), SI units (s = j 2 pi f). Complex poles come in
/// conjugate pairs with conjugate residue matrices.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Model {
    pub kind: String,
    pub ports: usize,
    pub poles: Vec<C>,
    pub residues: Vec<Matrix>,
    pub d: Matrix,
    pub e: Matrix,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Violation {
    pub start_hz: f64,
    pub end_hz: f64,
    pub worst_hz: f64,
    /// Min eigenvalue of the Hermitian part (Y/Z) or max singular value (S).
    pub worst_value: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Passivity {
    pub passive: bool,
    pub reason: Option<String>,
    pub tolerance: f64,
    pub grid_points: usize,
    pub violations: Vec<Violation>,
    pub worst_value: f64,
    pub worst_frequency_hz: f64,
    pub asymptotic_d_value: f64,
    pub asymptotic_e_value: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Enforcement {
    pub iterations: usize,
    pub reached: bool,
    pub reason: Option<String>,
    /// The fit violated passivity at D itself (f -> infinity); it was refitted with D = 0 before enforcing.
    pub asymptote_refit: bool,
    pub max_relative_error_before: f64,
    pub max_relative_error_after: f64,
    pub rms_relative_error_before: f64,
    pub rms_relative_error_after: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    pub model: Model,
    pub rms_relative_error: f64,
    pub max_relative_error: f64,
    pub iterations: usize,
    pub passive: bool,
    pub passivity_reason: Option<String>,
    pub passivity: Passivity,
    pub enforcement: Option<Enforcement>,
    /// Present for `poles: "auto"`.
    pub pole_selection: Option<PoleSelection>,
}

fn bad(message: impl std::fmt::Display) -> String {
    fail("RATIONAL_FIT_INPUT", message)
}
fn inv(z: C) -> C {
    let d = z.real * z.real + z.imaginary * z.imaginary;
    C::new(z.real / d, -z.imaginary / d)
}
fn jmul(z: C) -> C {
    C::new(-z.imaginary, z.real)
}
fn checkpoint(start: Instant, budget: Option<&Budget>) -> Result<()> {
    super::deadline(start, budget, "Rational vector fit")
}
fn finite_c(z: &C) -> bool {
    z.real.is_finite() && z.imaginary.is_finite()
}
fn square(m: &Matrix, n: usize) -> bool {
    m.len() == n && m.iter().all(|r| r.len() == n && r.iter().all(finite_c))
}

// ---- small dense real helpers -------------------------------------------------------------------

/// Householder QR in place on columns (R left in the upper triangle), applied to `rhs` as well.
fn qr_in_place(cols: &mut [Vec<f64>], mut rhs: Option<&mut Vec<f64>>) {
    let rows = cols.first().map_or(0, Vec::len);
    for j in 0..cols.len().min(rows) {
        let norm = cols[j][j..].iter().map(|v| v * v).sum::<f64>().sqrt();
        if norm == 0. {
            continue;
        }
        let alpha = if cols[j][j] > 0. { -norm } else { norm };
        let mut v: Vec<f64> = cols[j][j..].to_vec();
        v[0] -= alpha;
        let vtv: f64 = v.iter().map(|x| x * x).sum();
        if vtv == 0. {
            continue;
        }
        let apply = |c: &mut Vec<f64>| {
            let dot: f64 = v.iter().zip(&c[j..]).map(|(a, b)| a * b).sum();
            let f = 2. * dot / vtv;
            for (a, b) in c[j..].iter_mut().zip(&v) {
                *a -= f * b;
            }
        };
        for k in j + 1..cols.len() {
            apply(&mut cols[k]);
        }
        if let Some(r) = rhs.as_deref_mut() {
            apply(r);
        }
        cols[j][j] = alpha;
        for x in cols[j][j + 1..].iter_mut() {
            *x = 0.;
        }
    }
}

/// Column-normalized least squares by QR; numerically dependent columns get a zero coefficient.
fn lstsq(mut cols: Vec<Vec<f64>>, mut rhs: Vec<f64>) -> Result<Vec<f64>> {
    let n = cols.len();
    let norms: Vec<f64> = cols.iter().map(|c| c.iter().map(|v| v * v).sum::<f64>().sqrt()).collect();
    for (c, nr) in cols.iter_mut().zip(&norms) {
        if *nr > 0. {
            c.iter_mut().for_each(|v| *v /= nr);
        }
    }
    qr_in_place(&mut cols, Some(&mut rhs));
    let mut x = vec![0.; n];
    for i in (0..n).rev() {
        if i >= rhs.len() || norms[i] == 0. || cols[i][i].abs() < 1e-13 {
            continue;
        }
        let s: f64 = (i + 1..n).map(|j| cols[j][i] * x[j]).sum();
        x[i] = (rhs[i] - s) / cols[i][i];
    }
    for (v, nr) in x.iter_mut().zip(&norms) {
        if *nr > 0. {
            *v /= nr;
        }
    }
    if x.iter().any(|v| !v.is_finite()) {
        return Err(fail("RATIONAL_FIT_NOT_CONVERGED", "Non-finite least-squares solution"));
    }
    Ok(x)
}

/// Gauss-Jordan inverse with partial pivoting (small matrices only).
fn invert(mut a: Vec<Vec<f64>>) -> Option<Vec<Vec<f64>>> {
    let n = a.len();
    let mut b: Vec<Vec<f64>> = (0..n).map(|i| (0..n).map(|j| (i == j) as u8 as f64).collect()).collect();
    for c in 0..n {
        let p = (c..n).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        if !a[p][c].is_finite() || a[p][c].abs() < 1e-300 {
            return None;
        }
        a.swap(c, p);
        b.swap(c, p);
        let d = a[c][c];
        for k in 0..n {
            a[c][k] /= d;
            b[c][k] /= d;
        }
        for r in 0..n {
            if r != c {
                let f = a[r][c];
                if f != 0. {
                    for k in 0..n {
                        a[r][k] -= f * a[c][k];
                        b[r][k] -= f * b[c][k];
                    }
                }
            }
        }
    }
    Some(b)
}

// ---- pole bookkeeping -----------------------------------------------------------------------------

/// im == 0: real pole; im > 0: the pair (re +- j im).
#[derive(Clone, Copy, Debug)]
struct P {
    re: f64,
    im: f64,
}
fn ncoef(p: &[P]) -> usize {
    p.iter().map(|p| if p.im == 0. { 1 } else { 2 }).sum()
}
fn basis(x: f64, poles: &[P], out: &mut Vec<C>) {
    out.clear();
    let s = C::new(0., x);
    for p in poles {
        if p.im == 0. {
            out.push(inv(s - C::new(p.re, 0.)));
        } else {
            let a = C::new(p.re, p.im);
            let i1 = inv(s - a);
            let i2 = inv(s - a.conj());
            out.push(i1 + i2);
            out.push(jmul(i1 - i2));
        }
    }
}
fn stabilize(mut poles: Vec<P>) -> Vec<P> {
    for p in &mut poles {
        p.re = -p.re.abs();
        if p.re == 0. {
            p.re = -1e-12 * p.im.abs().max(1.);
        }
    }
    poles.sort_by(|a, b| a.im.total_cmp(&b.im).then(a.re.total_cmp(&b.re)));
    poles
}
fn initial_poles(total: usize, xmin: f64) -> Vec<P> {
    let lo = if xmin < 1. { xmin } else { 0.5 };
    let pairs = total / 2;
    let mut out = vec![];
    for k in 0..pairs {
        let t = if pairs == 1 { 0.5 } else { k as f64 / (pairs - 1) as f64 };
        let beta = (lo.ln() * (1. - t)).exp();
        out.push(P { re: -beta / 100., im: beta });
    }
    if total % 2 == 1 {
        out.push(P { re: -lo.sqrt(), im: 0. });
    }
    stabilize(out)
}

// ---- fitting ---------------------------------------------------------------------------------------

struct Data<'a> {
    x: Vec<f64>,
    w: Vec<f64>,
    h: &'a [Matrix],
    n: usize,
    els: Vec<(usize, usize)>,
    mirror: bool,
    has_d: bool,
    has_e: bool,
    wmax: f64,
}
impl Data<'_> {
    fn phis(&self, poles: &[P]) -> Vec<Vec<C>> {
        self.x
            .iter()
            .map(|&x| {
                let mut v = vec![];
                basis(x, poles, &mut v);
                v
            })
            .collect()
    }
}
#[derive(Clone)]
struct Rat {
    poles: Vec<P>,
    /// Per element in `els`: [basis coefficients, D?, E?].
    x: Vec<Vec<f64>>,
}

fn relocate(d: &Data, poles: &[P], start: Instant, budget: Option<&Budget>) -> Result<Option<Vec<P>>> {
    let nc = ncoef(poles);
    let na = nc + d.has_d as usize + d.has_e as usize;
    let nb = nc + 1;
    let m = d.x.len();
    let phis = d.phis(poles);
    let mut stack: Vec<Vec<f64>> = vec![vec![]; nb];
    for &(a, b) in &d.els {
        checkpoint(start, budget)?;
        let mut cols = vec![vec![0.; 2 * m]; na + nb];
        for k in 0..m {
            let (w, h) = (d.w[k], d.h[k][a][b]);
            for j in 0..nc {
                let p = phis[k][j];
                cols[j][2 * k] = w * p.real;
                cols[j][2 * k + 1] = w * p.imaginary;
                let q = h * p;
                cols[na + j][2 * k] = -w * q.real;
                cols[na + j][2 * k + 1] = -w * q.imaginary;
            }
            let mut c = nc;
            if d.has_d {
                cols[c][2 * k] = w;
                c += 1;
            }
            if d.has_e {
                cols[c][2 * k + 1] = w * d.x[k];
            }
            cols[na + nc][2 * k] = -w * h.real;
            cols[na + nc][2 * k + 1] = -w * h.imaginary;
        }
        qr_in_place(&mut cols, None);
        for j in 0..nb {
            for i in 0..nb {
                stack[j].push(if i <= j { cols[na + j][na + i] } else { 0. });
            }
        }
    }
    let sumw: f64 = d.w.iter().sum();
    let fro = stack.iter().flatten().map(|v| v * v).sum::<f64>().sqrt();
    if fro == 0. || !fro.is_finite() {
        return Ok(None);
    }
    let scale = fro / sumw;
    let mut relaxed = stack.clone();
    for (j, col) in relaxed.iter_mut().enumerate() {
        col.push(if j < nc { (0..m).map(|k| d.w[k] * phis[k][j].real).sum::<f64>() * scale } else { sumw * scale });
    }
    let mut rhs = vec![0.; relaxed[0].len()];
    *rhs.last_mut().unwrap() = sumw * scale;
    let y = lstsq(relaxed, rhs)?;
    let (mut c, mut dt) = (y[..nc].to_vec(), y[nc]);
    if !dt.is_finite() || dt.abs() < 1e-8 || dt.abs() > 1e8 {
        dt = 1.;
        let rhs: Vec<f64> = stack[nc].iter().map(|v| -v).collect();
        c = lstsq(stack[..nc].to_vec(), rhs)?;
    }
    // Zeros of sigma: eig(A - b c^T / dt) in the real state form.
    let mut a = vec![vec![0.; nc]; nc];
    let mut bv = vec![0.; nc];
    let mut o = 0;
    for p in poles {
        if p.im == 0. {
            a[o][o] = p.re;
            bv[o] = 1.;
            o += 1;
        } else {
            a[o][o] = p.re;
            a[o][o + 1] = p.im;
            a[o + 1][o] = -p.im;
            a[o + 1][o + 1] = p.re;
            bv[o] = 2.;
            o += 2;
        }
    }
    let mat = Mat::from_fn(nc, nc, |i, j| a[i][j] - bv[i] * c[j] / dt);
    let eig = mat
        .eigenvalues()
        .map_err(|e| fail("RATIONAL_FIT_NOT_CONVERGED", format!("Pole eigenvalues: {e:?}")))?;
    let mut out = vec![];
    for z in eig {
        if !z.re.is_finite() || !z.im.is_finite() {
            return Ok(None);
        }
        if z.im.abs() <= 1e-13 * z.re.abs().max(1e-300) || z.im == 0. {
            out.push(P { re: z.re, im: 0. });
        } else if z.im > 0. {
            out.push(P { re: z.re, im: z.im });
        }
    }
    if ncoef(&out) != nc {
        return Ok(None);
    }
    Ok(Some(stabilize(out)))
}

fn identify(d: &Data, poles: &[P], start: Instant, budget: Option<&Budget>) -> Result<Vec<Vec<f64>>> {
    let nc = ncoef(poles);
    let na = nc + d.has_d as usize + d.has_e as usize;
    let m = d.x.len();
    let phis = d.phis(poles);
    let mut base = vec![vec![0.; 2 * m]; na];
    for k in 0..m {
        let w = d.w[k];
        for j in 0..nc {
            base[j][2 * k] = w * phis[k][j].real;
            base[j][2 * k + 1] = w * phis[k][j].imaginary;
        }
        let mut c = nc;
        if d.has_d {
            base[c][2 * k] = w;
            c += 1;
        }
        if d.has_e {
            base[c][2 * k + 1] = w * d.x[k];
        }
    }
    let mut out = vec![];
    for &(a, b) in &d.els {
        checkpoint(start, budget)?;
        let rhs: Vec<f64> = (0..m)
            .flat_map(|k| [d.w[k] * d.h[k][a][b].real, d.w[k] * d.h[k][a][b].imaginary])
            .collect();
        out.push(lstsq(base.clone(), rhs)?);
    }
    Ok(out)
}

fn to_model(d: &Data, kind: &str, rat: &Rat) -> Model {
    let n = d.n;
    let nc = ncoef(&rat.poles);
    let mut poles = vec![];
    let mut residues: Vec<Matrix> = vec![];
    let mut o = 0;
    for p in &rat.poles {
        if p.im == 0. {
            poles.push(C::new(p.re * d.wmax, 0.));
            let mut r = super::frequency::math::zero(n, n);
            for (g, &(a, b)) in d.els.iter().enumerate() {
                r[a][b] = C::new(rat.x[g][o] * d.wmax, 0.);
                if d.mirror {
                    r[b][a] = r[a][b];
                }
            }
            residues.push(r);
            o += 1;
        } else {
            poles.push(C::new(p.re * d.wmax, p.im * d.wmax));
            poles.push(C::new(p.re * d.wmax, -p.im * d.wmax));
            let mut r = super::frequency::math::zero(n, n);
            for (g, &(a, b)) in d.els.iter().enumerate() {
                r[a][b] = C::new(rat.x[g][o] * d.wmax, rat.x[g][o + 1] * d.wmax);
                if d.mirror {
                    r[b][a] = r[a][b];
                }
            }
            let rc: Matrix = r.iter().map(|row| row.iter().map(|z| z.conj()).collect()).collect();
            residues.push(r);
            residues.push(rc);
            o += 2;
        }
    }
    let mut dm = super::frequency::math::zero(n, n);
    let mut em = super::frequency::math::zero(n, n);
    for (g, &(a, b)) in d.els.iter().enumerate() {
        let mut c = nc;
        if d.has_d {
            dm[a][b] = C::new(rat.x[g][c], 0.);
            c += 1;
        }
        if d.has_e {
            em[a][b] = C::new(rat.x[g][c] / d.wmax, 0.);
        }
        if d.mirror {
            dm[b][a] = dm[a][b];
            em[b][a] = em[a][b];
        }
    }
    Model { kind: kind.into(), ports: n, poles, residues, d: dm, e: em }
}

// ---- evaluation ------------------------------------------------------------------------------------

fn model_valid(model: &Model) -> Result<()> {
    let n = model.ports;
    if !matches!(model.kind.as_str(), "Y" | "Z" | "S") {
        return Err(bad("kind must be Y, Z or S"));
    }
    if n == 0 || n > MAX_PORTS || !square(&model.d, n) || !square(&model.e, n) {
        return Err(bad("model dimensions mismatch"));
    }
    if model.poles.len() != model.residues.len()
        || !model.poles.iter().all(finite_c)
        || model.residues.iter().any(|r| !square(r, n))
    {
        return Err(bad("model poles/residues mismatch or non-finite"));
    }
    Ok(())
}
fn eval_at(model: &Model, f: f64) -> Matrix {
    let n = model.ports;
    let w = 2. * std::f64::consts::PI * f;
    let s = C::new(0., w);
    let mut h = model.d.clone();
    for i in 0..n {
        for j in 0..n {
            h[i][j] = h[i][j] + jmul(model.e[i][j]).scale(w);
        }
    }
    for (p, r) in model.poles.iter().zip(&model.residues) {
        let g = inv(s - *p);
        for i in 0..n {
            for j in 0..n {
                h[i][j] = h[i][j] + r[i][j] * g;
            }
        }
    }
    h
}
pub fn evaluate(model: &Model, frequencies_hz: &[f64]) -> Result<Vec<Matrix>> {
    model_valid(model)?;
    if frequencies_hz.iter().any(|f| !f.is_finite() || *f < 0.) {
        return Err(bad("frequencies must be finite and >= 0"));
    }
    Ok(frequencies_hz.iter().map(|&f| eval_at(model, f)).collect())
}
/// Worst entry of the root-mean-square error over the samples, each entry relative to its own peak
/// magnitude (floored at 1e-6 of the largest entry): does not let a large S21 mask a wrong S11.
fn entry_error(model: &Model, freqs: &[f64], samples: &[Matrix]) -> f64 {
    let n = model.ports;
    let fits: Vec<Matrix> = freqs.iter().map(|&f| eval_at(model, f)).collect();
    let abs = |z: C| z.real.hypot(z.imaginary);
    let gmax = samples.iter().flat_map(|h| h.iter().flatten()).map(|z| abs(*z)).fold(0., f64::max);
    let mut worst = 0_f64;
    for i in 0..n {
        for j in 0..n {
            let peak = samples.iter().map(|h| abs(h[i][j])).fold(0., f64::max).max(1e-6 * gmax).max(1e-300);
            let ms = fits.iter().zip(samples).map(|(a, b)| abs(a[i][j] - b[i][j]).powi(2)).sum::<f64>() / samples.len() as f64;
            worst = worst.max(ms.sqrt() / peak);
        }
    }
    worst
}
fn frob(h: &Matrix) -> f64 {
    h.iter().flatten().map(|z| z.real * z.real + z.imaginary * z.imaginary).sum::<f64>().sqrt()
}
fn errors(model: &Model, freqs: &[f64], samples: &[Matrix]) -> (f64, f64) {
    let (mut sum, mut max) = (0., 0_f64);
    for (&f, h) in freqs.iter().zip(samples) {
        let fit = eval_at(model, f);
        let diff: f64 = fit
            .iter()
            .flatten()
            .zip(h.iter().flatten())
            .map(|(a, b)| {
                let z = *a - *b;
                z.real * z.real + z.imaginary * z.imaginary
            })
            .sum::<f64>()
            .sqrt();
        let nh = frob(h);
        let e = if nh > 0. { diff / nh } else { diff };
        sum += e * e;
        max = max.max(e);
    }
    ((sum / freqs.len() as f64).sqrt(), max)
}

// ---- passivity -------------------------------------------------------------------------------------

fn embed(h: &Matrix, kind: &str) -> Mat<f64> {
    let n = h.len();
    if kind == "S" {
        Mat::from_fn(2 * n, 2 * n, |i, j| {
            let z = h[i % n][j % n];
            match (i < n, j < n) {
                (true, true) | (false, false) => z.real,
                (true, false) => -z.imaginary,
                (false, true) => z.imaginary,
            }
        })
    } else {
        // Hermitian part A + jB as [[A, -B], [B, A]]
        Mat::from_fn(2 * n, 2 * n, |i, j| {
            let (a, b) = (i % n, j % n);
            let re = 0.5 * (h[a][b].real + h[b][a].real);
            let im = 0.5 * (h[a][b].imaginary - h[b][a].imaginary);
            match (i < n, j < n) {
                (true, true) | (false, false) => re,
                (true, false) => -im,
                (false, true) => im,
            }
        })
    }
}
/// Y/Z: min eigenvalue of the Hermitian part; S: max singular value.
fn value(h: &Matrix, kind: &str) -> Result<f64> {
    let m = embed(h, kind);
    let v = if kind == "S" {
        m.singular_values().map(|s| s[0]).map_err(|e| format!("{e:?}"))
    } else {
        m.self_adjoint_eigenvalues(Side::Lower).map(|s| s[0]).map_err(|e| format!("{e:?}"))
    };
    v.map_err(|e| fail("RATIONAL_FIT_NOT_CONVERGED", format!("Passivity eigenvalue: {e}")))
}
fn value_vectors(h: &Matrix, kind: &str) -> Result<(f64, Vec<C>, Vec<C>)> {
    let n = h.len();
    let m = embed(h, kind);
    let pair = |u: faer::MatRef<'_, f64>| -> Vec<C> { (0..n).map(|i| C::new(u[(i, 0)], u[(n + i, 0)])).collect() };
    if kind == "S" {
        let s = m.thin_svd().map_err(|e| fail("RATIONAL_FIT_NOT_CONVERGED", format!("{e:?}")))?;
        Ok((s.S()[0], pair(s.U()), pair(s.V())))
    } else {
        let s = m
            .self_adjoint_eigen(Side::Lower)
            .map_err(|e| fail("RATIONAL_FIT_NOT_CONVERGED", format!("{e:?}")))?;
        let u = pair(s.U());
        Ok((s.S()[0], u.clone(), u))
    }
}
/// Positive severity = violation.
fn severity(v: f64, kind: &str) -> f64 {
    if kind == "S" { v - 1. } else { -v }
}

fn make_grid(model: &Model, freqs: &[f64]) -> Vec<f64> {
    let fmax = *freqs.last().unwrap();
    let fmin = freqs.iter().copied().find(|f| *f > 0.).unwrap_or(fmax);
    let count = (20 * freqs.len()).max(200);
    let (lo, hi) = (fmin / 100., fmax * 10.);
    let mut g: Vec<f64> = vec![0.];
    for k in 0..count {
        g.push((lo.ln() + (hi.ln() - lo.ln()) * k as f64 / (count - 1) as f64).exp());
    }
    g.extend(freqs.iter().copied());
    g.extend([fmax * 100., fmax * 1000.]);
    for p in &model.poles {
        if p.imaginary > 0. {
            let f = p.imaginary / (2. * std::f64::consts::PI);
            let r = (p.real.abs() / p.imaginary).min(0.5);
            g.extend([f, f * (1. - r), f * (1. + r)]);
        }
    }
    g.retain(|f| f.is_finite() && *f >= 0.);
    g.sort_by(|a, b| a.total_cmp(b));
    g.dedup();
    g
}
fn sym_min_eig(m: &Matrix) -> Result<f64> {
    value(m, "Z")
}
fn data_scale(kind: &str, samples: &[Matrix]) -> f64 {
    if kind == "S" {
        1.
    } else {
        samples.iter().map(frob).fold(0., f64::max).max(1e-300)
    }
}
fn scan(model: &Model, grid: &[f64]) -> Result<Vec<f64>> {
    grid.iter().map(|&f| value(&eval_at(model, f), &model.kind)).collect()
}
fn passivity(model: &Model, grid: &[f64], values: &[f64], scale: f64, wmax: f64) -> Result<Passivity> {
    let kind = model.kind.as_str();
    let tol = PASSIVITY_TOLERANCE * scale;
    let mut violations: Vec<Violation> = vec![];
    let mut run: Option<usize> = None;
    let flush = |from: usize, to: usize, out: &mut Vec<Violation>| {
        let w = (from..=to).max_by(|&a, &b| severity(values[a], kind).total_cmp(&severity(values[b], kind))).unwrap();
        out.push(Violation { start_hz: grid[from], end_hz: grid[to], worst_hz: grid[w], worst_value: values[w] });
    };
    for i in 0..grid.len() {
        let bad = severity(values[i], kind) > tol;
        match (bad, run) {
            (true, None) => run = Some(i),
            (false, Some(s)) => {
                flush(s, i - 1, &mut violations);
                run = None;
            }
            _ => {}
        }
    }
    if let Some(s) = run {
        flush(s, grid.len() - 1, &mut violations);
    }
    let w = (0..grid.len()).max_by(|&a, &b| severity(values[a], kind).total_cmp(&severity(values[b], kind))).unwrap();
    let (dv, ev) = if kind == "S" {
        (value(&model.d, "S")?, frob(&model.e))
    } else {
        (sym_min_eig(&model.d)?, sym_min_eig(&model.e)?)
    };
    let tol_e = tol / wmax;
    let mut reasons = vec![];
    if !violations.is_empty() {
        reasons.push(format!("{} violating band(s), worst {:.6e} at {:.6e} Hz", violations.len(), values[w], grid[w]));
    }
    if severity(dv, kind) > tol {
        reasons.push(format!("asymptotic D value {dv:.6e}"));
    }
    if (kind == "S" && ev > tol_e) || (kind != "S" && ev < -tol_e) {
        reasons.push(format!("asymptotic E term {ev:.6e}"));
    }
    Ok(Passivity {
        passive: reasons.is_empty(),
        reason: (!reasons.is_empty()).then(|| format!("RATIONAL_NOT_PASSIVE: {}", reasons.join("; "))),
        tolerance: tol,
        grid_points: grid.len(),
        violations,
        worst_value: values[w],
        worst_frequency_hz: grid[w],
        asymptotic_d_value: dv,
        asymptotic_e_value: ev,
    })
}

/// Iterative linearized residue/D perturbation, minimal change in the data-fit metric.
#[allow(clippy::too_many_arguments)]
fn enforce(
    d: &Data,
    kind: &str,
    rat: &mut Rat,
    grid: &[f64],
    scale: f64,
    start: Instant,
    budget: Option<&Budget>,
) -> Result<usize> {
    let nc = ncoef(&rat.poles);
    let nb = nc + d.has_d as usize;
    let margin = 1e-6 * scale;
    let target = if kind == "S" { 1. - 1e-6 } else { margin };
    let phis = d.phis(&rat.poles);
    let mut gram = vec![vec![0.; nb]; nb];
    for (k, phi) in phis.iter().enumerate() {
        let row = |j: usize| -> (f64, f64) {
            if j < nc { (phi[j].real, phi[j].imaginary) } else { (1., 0.) }
        };
        for i in 0..nb {
            for j in 0..nb {
                gram[i][j] += d.w[k] * d.w[k] * (row(i).0 * row(j).0 + row(i).1 * row(j).1);
            }
        }
    }
    let ridge = 1e-10 * (0..nb).map(|i| gram[i][i]).sum::<f64>() / nb as f64;
    for i in 0..nb {
        gram[i][i] += ridge;
    }
    let ginv = invert(gram).ok_or_else(|| fail("RATIONAL_PASSIVITY_NOT_ENFORCED", "Singular fit metric"))?;
    let gmax = d.h.iter().flat_map(|h| h.iter().flatten()).map(|z| z.real.hypot(z.imaginary)).fold(0., f64::max);
    let entry_scale2: Vec<f64> = d
        .els
        .iter()
        .map(|&(a, b)| {
            let m = d.h.iter().map(|h| h[a][b].real.hypot(h[a][b].imaginary)).fold(0., f64::max);
            m.max(1e-6 * gmax).powi(2)
        })
        .collect();
    let sev_of = |model: &Model| -> Result<(Vec<f64>, f64)> {
        let v = scan(model, grid)?;
        let w = v.iter().map(|x| severity(*x, kind)).fold(f64::MIN, f64::max);
        Ok((v, w))
    };
    let mut current = to_model(d, kind, rat);
    let (mut values, mut worst) = sev_of(&current)?;
    let mut alpha = 1.;
    let mut used = 0;
    while used < ENFORCE_ITERATIONS {
        if worst <= PASSIVITY_TOLERANCE * scale {
            break;
        }
        used += 1;
        checkpoint(start, budget)?;
        // Constrain every violating grid point (thinned to MAX_ROWS, peaks always kept) plus the
        // asymptote D itself (x -> infinity): the response between and beyond constrained points is
        // nonlinear, so peak-only rows let the violation migrate to neighbours (seen out of band).
        let sev: Vec<f64> = values.iter().map(|v| severity(*v, kind)).collect();
        let near = 0.;
        let bad: Vec<usize> = (0..grid.len()).filter(|&i| sev[i] > near).collect();
        let is_peak: Vec<bool> = (0..grid.len())
            .map(|i| sev[i] > near && (i == 0 || sev[i] >= sev[i - 1]) && (i + 1 == grid.len() || sev[i] >= sev[i + 1]))
            .collect();
        let mut picks: Vec<usize> = bad
            .iter()
            .copied()
            .filter(|&i| (i == 0 || sev[i] >= sev[i - 1]) && (i + 1 == grid.len() || sev[i] >= sev[i + 1]))
            .collect();
        let stride = bad.len().div_ceil(MAX_ROWS).max(1);
        picks.extend(bad.iter().copied().step_by(stride));
        picks.sort_unstable();
        picks.dedup();
        picks.sort_by(|&a, &b| sev[b].total_cmp(&sev[a]));
        picks.truncate(MAX_ROWS);
        // Linear rows: change of the passivity value per unit coefficient change.
        let mut points: Vec<(Matrix, Vec<C>)> = vec![];
        for &i in &picks {
            let mut phi = vec![];
            basis(grid[i] / (d.wmax / (2. * std::f64::consts::PI)), &rat.poles, &mut phi);
            points.push((eval_at(&current, grid[i]), phi));
        }
        if d.has_d && severity(value(&current.d, kind)?, kind) > 0. {
            points.push((current.d.clone(), vec![C::new(0., 0.); nc]));
        }
        let mut rows: Vec<Vec<Vec<f64>>> = vec![]; // [pick][group][param]
        let mut goal = vec![];
        let mut rowv = vec![];
        for (h, phi) in &points {
            let (v, u, w) = value_vectors(h, kind)?;
            let per_group: Vec<Vec<f64>> = d
                .els
                .iter()
                .map(|&(a, b)| {
                    (0..nb)
                        .map(|j| {
                            let f = if j < nc { phi[j] } else { C::new(1., 0.) };
                            let mut c = (u[a].conj() * w[b] * f).real;
                            if d.mirror && a != b {
                                c += (u[b].conj() * w[a] * f).real;
                            }
                            c
                        })
                        .collect()
                })
                .collect();
            rows.push(per_group);
            rowv.push(v);
            goal.push(if kind == "S" { (target - v).min(0.) } else { (target - v).max(0.) });
        }
        let q = rows.len();
        // Relative-error metric: the cost of changing entry g is weighted by 1 / max|H_g|^2, so the
        // minimal-cost step moves large entries and leaves small ones (S11 next to S21 ~ 1) alone.
        let mult = |g: usize| (if d.mirror && d.els[g].0 != d.els[g].1 { 2. } else { 1. }) / entry_scale2[g];
        let mut kmat = vec![vec![0.; q]; q];
        let zs: Vec<Vec<Vec<f64>>> = (0..d.els.len())
            .map(|g| {
                rows.iter()
                    .map(|r| (0..nb).map(|i| (0..nb).map(|j| ginv[i][j] * r[g][j]).sum::<f64>() / mult(g)).collect())
                    .collect()
            })
            .collect();
        for g in 0..d.els.len() {
            for i in 0..q {
                for j in 0..q {
                    kmat[i][j] += (0..nb).map(|t| rows[i][g][t] * zs[g][j][t]).sum::<f64>();
                }
            }
        }
        let tr = (0..q).map(|i| kmat[i][i]).sum::<f64>() / q as f64;
        // Active-set solve (linearized inequality constraints): start from the violation peaks and the
        // asymptote, add every row whose predicted value still violates, until none does. Rows are
        // minimal-norm equalities at the target, so inactive points are not pushed needlessly.
        let sign = if kind == "S" { 1. } else { -1. };
        let mut active: Vec<usize> = (0..q).filter(|&i| i >= picks.len() || is_peak[picks[i]]).collect();
        if active.is_empty() {
            active.push(0);
        }
        let mut y = vec![0.; q];
        for _ in 0..12 {
            let sub: Vec<Vec<f64>> = active
                .iter()
                .map(|&i| {
                    active
                        .iter()
                        .map(|&j| kmat[i][j] + if i == j { ENFORCE_DAMPING * kmat[i][i].max(1e-12 * tr).max(1e-300) } else { 0. })
                        .collect()
                })
                .collect();
            let kinv = invert(sub).ok_or_else(|| fail("RATIONAL_PASSIVITY_NOT_ENFORCED", "Singular constraint system"))?;
            y = vec![0.; q];
            for (ai, &i) in active.iter().enumerate() {
                y[i] = active.iter().enumerate().map(|(aj, &j)| kinv[ai][aj] * goal[j]).sum();
            }
            let added: Vec<usize> = (0..q)
                .filter(|i| !active.contains(i))
                .filter(|&i| {
                    let pred = rowv[i] + (0..q).map(|j| kmat[i][j] * y[j]).sum::<f64>();
                    sign * (pred - target) > 1e-3 * (sign * (rowv[i] - target)).abs().max(1e-12 * scale)
                })
                .collect();
            if added.is_empty() {
                break;
            }
            active.extend(added);
            active.sort_unstable();
        }
        let mut trial = rat.clone();
        for g in 0..d.els.len() {
            for t in 0..nb {
                let dx: f64 = (0..q).map(|j| zs[g][j][t] * y[j]).sum();
                trial.x[g][t] += alpha * dx;
            }
        }
        let model = to_model(d, kind, &trial);
        let (v2, w2) = sev_of(&model)?;
        if w2 < worst {
            *rat = trial;
            current = model;
            values = v2;
            worst = w2;
            alpha = (alpha * 2.).min(1.);
        } else {
            alpha *= 0.5;
        }
    }
    Ok(used)
}

// ---- public entry points --------------------------------------------------------------------------

pub fn fit(req: Request) -> Result<Output> {
    let start = Instant::now();
    let budget = req.budget.as_ref();
    let kind = req.kind.as_str();
    if !matches!(kind, "Y" | "Z" | "S") {
        return Err(bad("kind must be Y, Z or S"));
    }
    let m = req.frequencies_hz.len();
    if m == 0 || req.samples.len() != m {
        return Err(bad("frequenciesHz and samples must be non-empty and the same length"));
    }
    let n = req.samples[0].len();
    if n == 0 || n > MAX_PORTS || req.samples.iter().any(|s| !square(s, n)) {
        return Err(bad(format!("samples must be finite n x n matrices, 1 <= n <= {MAX_PORTS}")));
    }
    let f = &req.frequencies_hz;
    if f.iter().any(|v| !v.is_finite() || *v < 0.) || f.windows(2).any(|w| w[1] <= w[0]) || *f.last().unwrap() <= 0. {
        return Err(bad("frequenciesHz must be finite, >= 0 and strictly increasing with a positive maximum"));
    }
    match &req.poles {
        PoleSpec::Count(0) => return Err(bad(format!("poles must be 1..={MAX_POLES} or \"auto\""))),
        PoleSpec::Count(p) if *p > MAX_POLES => return Err(bad(format!("poles must be 1..={MAX_POLES} or \"auto\""))),
        PoleSpec::Count(_) => {}
        PoleSpec::Auto(s) if s == "auto" => {}
        PoleSpec::Auto(s) => return Err(bad(format!("poles must be a count or \"auto\", got \"{s}\""))),
    }
    let weights = match &req.weights {
        Some(w) if w.len() != m || w.iter().any(|v| !v.is_finite() || *v <= 0.) => {
            return Err(bad("weights must be positive, finite, one per frequency"))
        }
        Some(w) => w.clone(),
        None => vec![1.; m],
    };
    if f[0] == 0. && req.samples[0].iter().flatten().any(|z| z.imaginary.abs() > 1e-9 * frob(&req.samples[0]).max(1e-300)) {
        return Err(bad("the f = 0 sample must be real"));
    }
    let has_d = req.fit_d.unwrap_or(true);
    let has_e = req.fit_e.unwrap_or(kind != "S");
    if let PoleSpec::Count(p) = req.poles {
        let unknowns = 2 * p + 3;
        if 2 * m <= unknowns {
            return Err(bad(format!("{m} samples are too few for {p} poles (need 2 x samples > {unknowns})")));
        }
    } else if m < 3 {
        return Err(bad("poles \"auto\" needs at least 3 samples"));
    }
    let scale_m = req.samples.iter().map(frob).fold(0., f64::max);
    if scale_m == 0. {
        return Err(bad("all samples are zero"));
    }
    let mut symmetric = true;
    'sym: for h in &req.samples {
        for i in 0..n {
            for j in 0..i {
                if (h[i][j] - h[j][i]).abs() > 1e-9 * scale_m {
                    symmetric = false;
                    break 'sym;
                }
            }
        }
    }
    let els: Vec<(usize, usize)> =
        (0..n).flat_map(|a| (0..n).map(move |b| (a, b))).filter(|&(a, b)| !symmetric || a <= b).collect();
    let fmax = *f.last().unwrap();
    let data = Data {
        x: f.iter().map(|v| v / fmax).collect(),
        w: weights,
        h: &req.samples,
        n,
        els,
        mirror: symmetric,
        has_d,
        has_e,
        wmax: 2. * std::f64::consts::PI * fmax,
    };
    let xmin = data.x.iter().copied().find(|v| *v > 0.).unwrap_or(1.);
    let iterations = req.iterations.unwrap_or(10);
    let run = |data: &Data, count: usize| -> Result<(Rat, usize)> {
        let mut poles = initial_poles(count, xmin);
        let mut used = 0;
        for _ in 0..iterations {
            checkpoint(start, budget)?;
            let Some(next) = relocate(data, &poles, start, budget)? else { break };
            used += 1;
            let same = next.len() == poles.len()
                && next.iter().zip(&poles).all(|(a, b)| {
                    (a.re - b.re).hypot(a.im - b.im) <= 1e-12 * a.re.hypot(a.im).max(1e-300) && (a.im == 0.) == (b.im == 0.)
                });
            poles = next;
            if same {
                break;
            }
        }
        let x = identify(data, &poles, start, budget)?;
        Ok((Rat { poles, x }, used))
    };
    let (rat, used, selection) = match &req.poles {
        PoleSpec::Count(p) => {
            let (rat, used) = run(&data, *p)?;
            (rat, used, None)
        }
        PoleSpec::Auto(_) => {
            // Smallest count after which two further counts no longer cut the fit-point rms error by 2x.
            // Cap: at least 2 x as many real equations per element as unknowns (poles + D + E).
            let cap = (m.saturating_sub(2)).min(MAX_POLES).min(req.max_poles.unwrap_or(MAX_POLES));
            if cap == 0 {
                return Err(bad("maxPoles / sample count leave no admissible pole count"));
            }
            let mut best: Option<(usize, f64, Rat, usize)> = None;
            let (mut trials, mut stale, mut tried) = (vec![], 0, 0);
            for count in 1..=cap {
                tried = count;
                let fitted = run(&data, count).ok().and_then(|(r, u)| {
                    let e = entry_error(&to_model(&data, kind, &r), f, &req.samples);
                    e.is_finite().then_some((r, u, e))
                });
                trials.push(PoleTrial { poles: count, entry_relative_error: fitted.as_ref().map(|t| t.2) });
                let Some((r, u, e)) = fitted else {
                    stale += 1;
                    if stale >= 2 {
                        break;
                    }
                    continue;
                };
                if best.as_ref().map_or(true, |b| e < 0.5 * b.1) {
                    best = Some((count, e, r, u));
                    stale = 0;
                    if e < 1e-12 {
                        break;
                    }
                } else {
                    stale += 1;
                    if stale >= 2 {
                        break;
                    }
                }
            }
            let (chosen, _, rat, used) = best.ok_or_else(|| fail("RATIONAL_FIT_NOT_CONVERGED", "No pole count produced a finite fit"))?;
            let rule = format!(
                "fit points only: smallest count after which 2 further counts do not halve the worst-entry rms error relative to the entry's peak (or error < 1e-12); cap {cap} = min(samples - 2, maxPoles)"
            );
            (rat, used, Some(PoleSelection { chosen, max_tried: tried, trials, rule }))
        }
    };
    let pole_count = selection.as_ref().map_or_else(
        || match &req.poles {
            PoleSpec::Count(p) => *p,
            PoleSpec::Auto(_) => 0,
        },
        |s| s.chosen,
    );
    let finish = |rat: Rat, used: usize, count: usize, selection: Option<PoleSelection>| -> Result<Output> {
        let mut model = to_model(&data, kind, &rat);
        let (rms0, max0) = errors(&model, f, &req.samples);
        if !rms0.is_finite() {
            return Err(fail("RATIONAL_FIT_NOT_CONVERGED", "Fit is not finite"));
        }
        if let Some(limit) = req.max_relative_error {
            if max0 > limit {
                return Err(fail(
                    "RATIONAL_FIT_NOT_CONVERGED",
                    format!("max relative error {max0:.3e} above requested {limit:.3e} after {used} iterations"),
                ));
            }
        }
        let grid = make_grid(&model, f);
        let scale = data_scale(kind, &req.samples);
        let mut pass = passivity(&model, &grid, &scan(&model, &grid)?, scale, data.wmax)?;
        let mut enforcement = None;
        let (mut rms, mut max) = (rms0, max0);
        if req.enforce_passivity == Some(true) && !pass.passive {
            let attempt = |edata: &Data, from: &Rat| -> Result<(Rat, Model, Passivity, usize)> {
                let mut work = from.clone();
                let steps = enforce(edata, kind, &mut work, &grid, scale, start, budget)?;
                let m = to_model(edata, kind, &work);
                let after = passivity(&m, &grid, &scan(&m, &grid)?, scale, data.wmax)?;
                Ok((work, m, after, steps))
            };
            let (_, mut enforced, mut after, mut steps) = attempt(&data, &rat)?;
            // An asymptote D that violates by itself (D cancelled by a wide pole in-band) is not
            // repairable without destroying small entries: retry once from a fit with D fixed to 0.
            let mut asymptote_refit = false;
            let asym_bad = data.has_d && severity(pass.asymptotic_d_value, kind) > pass.tolerance;
            if !after.passive && asym_bad {
                let alt_data = Data { x: data.x.clone(), w: data.w.clone(), h: data.h, n, els: data.els.clone(), mirror: data.mirror, has_d: false, has_e, wmax: data.wmax };
                if let Ok((alt_rat, _)) = run(&alt_data, count) {
                    if let Ok((_, m2, a2, s2)) = attempt(&alt_data, &alt_rat) {
                        if a2.passive || severity(a2.worst_value, kind) < severity(after.worst_value, kind) {
                            (enforced, after, steps, asymptote_refit) = (m2, a2, s2, true);
                        }
                    }
                }
            }
            let (rms1, max1) = errors(&enforced, f, &req.samples);
            let reached = after.passive;
            let reason = (!reached).then(|| {
                format!(
                    "RATIONAL_PASSIVITY_NOT_ENFORCED: {} after {steps} iteration(s); asymptotic E is not adjusted",
                    after.reason.clone().unwrap_or_default()
                )
            });
            enforcement = Some(Enforcement {
                iterations: steps,
                reached,
                reason,
                asymptote_refit,
                max_relative_error_before: max0,
                max_relative_error_after: max1,
                rms_relative_error_before: rms0,
                rms_relative_error_after: rms1,
            });
            // Return the enforced model whenever it is better than the input; verified status is reported.
            if reached || severity(after.worst_value, kind) < severity(pass.worst_value, kind) {
                model = enforced;
                pass = after;
                rms = rms1;
                max = max1;
            }
        }
        Ok(Output {
            model,
            rms_relative_error: rms,
            max_relative_error: max,
            iterations: used,
            passive: pass.passive,
            passivity_reason: pass.reason.clone(),
            passivity: pass,
            enforcement,
            pole_selection: selection.clone(),
        })
    };
    let mut out = finish(rat, used, pole_count, selection.clone())?;
    // With enforcement, "auto" also tries smaller counts (down to half): a near-interpolating fit has wild
    // behaviour between points, which costs a large perturbation to repair. Still fit points only; the
    // passive result with the smallest fit-point error wins.
    if let (Some(sel), Some(true)) = (&selection, req.enforce_passivity) {
        let score = |o: &Output| {
            (!o.passive, if o.passive { entry_error(&o.model, f, &req.samples) } else { severity(o.passivity.worst_value, kind) })
        };
        for count in (pole_count.div_ceil(2).max(1)..pole_count).rev() {
            let Ok((r, u)) = run(&data, count) else { continue };
            let Ok(cand) = finish(r, u, count, Some(PoleSelection { chosen: count, ..sel.clone() })) else { continue };
            let (sc, so) = (score(&cand), score(&out));
            if sc.0 < so.0 || (sc.0 == so.0 && sc.1 < so.1) {
                out = cand;
            }
        }
        if let Some(s) = out.pole_selection.as_mut() {
            s.rule.push_str("; with enforcePassivity also counts down to half the chosen one were enforced and the passive one with the smallest fit-point worst-entry error kept");
        }
    }
    Ok(out)
}

pub fn fit_json(request: Value) -> Result<Value> {
    let req: Request = serde_json::from_value(request).map_err(|e| bad(format!("Invalid request: {e}")))?;
    serde_json::to_value(fit(req)?).map_err(|e| fail("RATIONAL_FIT_NOT_CONVERGED", e))
}

pub fn evaluate_json(request: Value) -> Result<Value> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields, rename_all = "camelCase")]
    struct Eval {
        model: Model,
        frequencies_hz: Vec<f64>,
    }
    let r: Eval = serde_json::from_value(request).map_err(|e| bad(format!("Invalid request: {e}")))?;
    Ok(json!({ "samples": evaluate(&r.model, &r.frequencies_hz)? }))
}
