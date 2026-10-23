//! Bounded complex blocks; faer real-block LU, no explicit matrix inverse.
use super::*;
use faer::{
    prelude::Solve,
    sparse::{
        linalg::solvers::{Lu, SymbolicLu},
        SparseColMat, Triplet,
    },
    Mat,
};
#[derive(Clone, Copy, Default, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct C {
    pub real: f64,
    pub imaginary: f64,
}
impl C {
    pub fn new(real: f64, imaginary: f64) -> Self {
        Self { real, imaginary }
    }
    pub fn abs(self) -> f64 {
        self.real.hypot(self.imaginary)
    }
    pub fn conj(self) -> Self {
        Self::new(self.real, -self.imaginary)
    }
    pub fn scale(self, v: f64) -> Self {
        Self::new(self.real * v, self.imaginary * v)
    }
}
impl std::ops::Add for C {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self::new(self.real + b.real, self.imaginary + b.imaginary)
    }
}
impl std::ops::Sub for C {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        Self::new(self.real - b.real, self.imaginary - b.imaginary)
    }
}
impl std::ops::Mul for C {
    type Output = Self;
    fn mul(self, b: Self) -> Self {
        Self::new(
            self.real * b.real - self.imaginary * b.imaginary,
            self.real * b.imaginary + self.imaginary * b.real,
        )
    }
}
pub type Matrix = Vec<Vec<C>>;
pub fn zero(n: usize, m: usize) -> Matrix {
    vec![vec![C::default(); m]; n]
}
pub fn eye(n: usize) -> Matrix {
    let mut a = zero(n, n);
    for (i, r) in a.iter_mut().enumerate() {
        r[i].real = 1.;
    }
    a
}
fn transpose(a: &Matrix) -> Matrix {
    (0..a[0].len())
        .map(|j| a.iter().map(|r| r[j]).collect())
        .collect()
}
/// Infinity-norm condition number from LU unit-vector solves (not an inverse
/// matrix). Absolute scales cancel; no gmin, pivot replacement or repair.
pub fn solve(a: &Matrix, b: &Matrix, q: &mut Quality) -> Result<Matrix> {
    let n = a.len();
    let m = b[0].len();
    let dim = 2 * n;
    let real = |i: usize, j: usize| {
        let z = a[i % n][j % n];
        if i < n && j < n || i >= n && j >= n {
            z.real
        } else if i < n {
            -z.imaginary
        } else {
            z.imaginary
        }
    };
    let triplets = (0..dim)
        .flat_map(|i| {
            (0..dim).filter_map(move |j| {
                let v = real(i, j);
                (v != 0.).then_some(Triplet::new(i, j, v))
            })
        })
        .collect::<Vec<_>>();
    if triplets.iter().any(|t| !t.val.is_finite()) {
        return Err(fail("NETWORK_NONFINITE", "matrix"));
    }
    let mat = SparseColMat::try_new_from_triplets(dim, dim, &triplets)
        .map_err(|e| fail("NETWORK_MATRIX", e))?;
    let sym = SymbolicLu::try_new(mat.symbolic()).map_err(|e| fail("NETWORK_SINGULAR", e))?;
    let lu =
        Lu::try_new_with_symbolic(sym, mat.as_ref()).map_err(|e| fail("NETWORK_SINGULAR", e))?;
    let norm = (0..dim)
        .map(|i| (0..dim).map(|j| real(i, j).abs()).sum::<f64>())
        .fold(0., f64::max);
    let mut sums = vec![0.; dim];
    for j in 0..dim {
        let x = lu.solve(Mat::from_fn(dim, 1, |i, _| if i == j { 1. } else { 0. }));
        for i in 0..dim {
            if !x[(i, 0)].is_finite() {
                return Err(fail(
                    "NETWORK_SINGULAR",
                    "Nonfinite LU condition probe: rank-deficient system",
                ));
            }
            sums[i] += x[(i, 0)].abs();
        }
    }
    let condition = norm * sums.into_iter().fold(0., f64::max);
    if norm == 0. || !condition.is_finite() || condition > 1e12 {
        return Err(fail(
            "NETWORK_SINGULAR",
            format!("nonfinite/rank deficient or ill-conditioned system; condition={condition}"),
        ));
    }
    q.max_condition = q.max_condition.max(condition);
    let rhs = Mat::from_fn(dim, m, |i, j| {
        if i < n {
            b[i][j].real
        } else {
            b[i - n][j].imaginary
        }
    });
    let mut x = lu.solve(&rhs);
    // Fixed iterative refinement, same F64 solver family as P6a.
    for _ in 0..2 {
        let r = Mat::from_fn(dim, m, |i, j| {
            rhs[(i, j)] - (0..dim).map(|k| real(i, k) * x[(k, j)]).sum::<f64>()
        });
        let d = lu.solve(r);
        for i in 0..dim {
            for j in 0..m {
                x[(i, j)] += d[(i, j)];
            }
        }
    }
    let mut residual: f64 = 0.;
    for i in 0..dim {
        for j in 0..m {
            let mut r = -rhs[(i, j)];
            let mut scale = rhs[(i, j)].abs();
            for k in 0..dim {
                r += real(i, k) * x[(k, j)];
                scale += (real(i, k) * x[(k, j)]).abs();
            }
            residual = residual.max(r.abs() / scale.max(1e-30));
        }
    }
    if !residual.is_finite()
        || residual > 1e-8
        || (0..dim).any(|i| (0..m).any(|j| !x[(i, j)].is_finite()))
    {
        return Err(fail("NETWORK_RESIDUAL", residual));
    }
    q.max_residual = q.max_residual.max(residual);
    q.solves += 1;
    Ok((0..n)
        .map(|i| (0..m).map(|j| C::new(x[(i, j)], x[(i + n, j)])).collect())
        .collect())
}
fn right(b: &Matrix, a: &Matrix, q: &mut Quality) -> Result<Matrix> {
    Ok(transpose(&solve(&transpose(a), &transpose(b), q)?))
}
/// Power waves V=D(a+b), I=D^-1(a-b). Row=out, col=in.
pub fn renormalize(s: &Matrix, from: &[f64], to: &[f64], q: &mut Quality) -> Result<Matrix> {
    if from == to {
        return Ok(s.clone());
    }
    let n = s.len();
    let mut a = zero(n, n);
    let mut b = zero(n, n);
    for i in 0..n {
        let d = (from[i] / to[i]).sqrt();
        for j in 0..n {
            let id = if i == j { 1. } else { 0. };
            let v = (C::new(id, 0.) + s[i][j]).scale(d);
            let cur = (C::new(id, 0.) - s[i][j]).scale(1. / d);
            a[i][j] = (v + cur).scale(0.5);
            b[i][j] = (v - cur).scale(0.5);
        }
    }
    right(&b, &a, q)
}
pub fn to_s(a: &Matrix, rep: &str, refs: &[f64], q: &mut Quality) -> Result<Matrix> {
    if rep == "S" {
        return renormalize(a, refs, &vec![50.; a.len()], q);
    }
    let n = a.len();
    let mut lhs = zero(n, n);
    let mut rhs = zero(n, n);
    let d = 50_f64.sqrt();
    for i in 0..n {
        for j in 0..n {
            let id = if i == j { 1. } else { 0. };
            if rep == "Z" {
                lhs[i][j] = a[i][j].scale(1. / d) + C::new(d * id, 0.);
                rhs[i][j] = a[i][j].scale(1. / d) - C::new(d * id, 0.);
            } else {
                lhs[i][j] = a[i][j].scale(d) + C::new(id / d, 0.);
                rhs[i][j] = C::new(id / d, 0.) - a[i][j].scale(d);
            }
        }
    }
    solve(&lhs, &rhs, q)
}
pub fn from_s(s: &Matrix, rep: &str, refs: &[f64], q: &mut Quality) -> Result<Matrix> {
    if rep == "S" {
        return renormalize(s, &vec![50.; s.len()], refs, q);
    }
    let n = s.len();
    let mut plus = eye(n);
    let mut minus = eye(n);
    for i in 0..n {
        for j in 0..n {
            plus[i][j] = plus[i][j] + s[i][j];
            minus[i][j] = minus[i][j] - s[i][j];
        }
    }
    let mut out = if rep == "Z" {
        right(&plus, &minus, q)?
    } else {
        right(&minus, &plus, q)?
    };
    for row in &mut out {
        for z in row {
            *z = z.scale(if rep == "Z" { 50. } else { 1. / 50. });
        }
    }
    Ok(out)
}
/// Positive definiteness of (1+tol)^2 I - S* S via real-block Cholesky.
/// This tests all incident wave combinations, not individual coefficients.
pub fn passive(s: &Matrix) -> bool {
    let n = s.len();
    let mut h = zero(n, n);
    for i in 0..n {
        for j in 0..n {
            h[i][j] = C::new(if i == j { (1. + 1e-9_f64).powi(2) } else { 0. }, 0.);
            for row in s {
                h[i][j] = h[i][j] - row[i].conj() * row[j];
            }
        }
    }
    let mut l = vec![vec![0.; 2 * n]; 2 * n];
    for i in 0..2 * n {
        for j in 0..=i {
            let z = h[i % n][j % n];
            let mut v = if i < n && j < n || i >= n && j >= n {
                z.real
            } else if i < n {
                -z.imaginary
            } else {
                z.imaginary
            };
            for k in 0..j {
                v -= l[i][k] * l[j][k];
            }
            if i == j {
                if !v.is_finite() || v <= 0. {
                    return false;
                }
                l[i][j] = v.sqrt();
            } else {
                l[i][j] = v / l[j][j];
            }
        }
    }
    true
}
