//! Exact process-local reuse of geometry-only finite-volume Newton kernels.
//! Current bases, block identities and budget admission remain outside the cache.
//! Canonical cell/vertex ordering makes equivalent mesh traversal orders reusable;
//! every coordinate bit, quadrature order and near-field tolerance remains in the key.
use super::{error, Result};
use std::sync::{Arc, Mutex, OnceLock};

pub type Tetra = [[f64; 3]; 4];
const MAX_KERNEL_BYTES: usize = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 24;

struct Entry {
    key: Vec<u64>,
    value: Arc<Vec<f64>>,
}
struct Cache {
    entries: Vec<Entry>,
    bytes: usize,
}
static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();

fn cache() -> &'static Mutex<Cache> {
    CACHE.get_or_init(|| Mutex::new(Cache { entries: vec![], bytes: 0 }))
}

fn vertex_key(p: &[f64; 3]) -> [u64; 3] {
    p.map(f64::to_bits)
}

/// Returns canonical cells and a canonical-to-input permutation. Repeated
/// identical cells are interchangeable because the kernel depends on geometry only.
pub fn canonical(cells: &[Tetra]) -> (Vec<Tetra>, Vec<usize>) {
    let mut entries: Vec<_> = cells.iter().enumerate().map(|(index, &cell)| {
        let mut vertices = cell.map(|p| vertex_key(&p));
        vertices.sort();
        let canonical = std::array::from_fn(|v| std::array::from_fn(|axis| f64::from_bits(vertices[v][axis])));
        (vertices, canonical, index)
    }).collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries.into_iter().map(|(_, cell, index)| (cell, index)).unzip()
}

fn key(cells: &[Tetra], order: usize, tolerance: f64) -> Vec<u64> {
    let mut key = Vec::with_capacity(3 + cells.len() * 12);
    key.extend([cells.len() as u64, order as u64, tolerance.to_bits()]);
    for cell in cells {
        for point in cell {
            key.extend(vertex_key(&point));
        }
    }
    key
}

fn validate(value: &[f64], len: usize) -> Result<()> {
    if value.len() != len || value.iter().any(|v| !v.is_finite() || *v <= 0.) {
        return Err(error("MAGNETIC_CACHE", "finite positive kernel of the exact requested shape required"));
    }
    Ok(())
}

fn get(key: &[u64], len: usize) -> Result<Option<Arc<Vec<f64>>>> {
    let mut cache = cache().lock().map_err(|_| error("MAGNETIC_CACHE", "poisoned geometry kernel cache"))?;
    let Some(index) = cache.entries.iter().position(|entry| entry.key == key) else { return Ok(None); };
    let entry = cache.entries.remove(index);
    validate(&entry.value, len)?;
    let value = entry.value.clone();
    cache.entries.push(entry);
    Ok(Some(value))
}

fn put(key: Vec<u64>, value: Vec<f64>) -> Result<Vec<f64>> {
    validate(&value, value.len())?;
    let bytes = value.len().checked_mul(8).ok_or_else(|| error("MAGNETIC_CACHE", "kernel byte count overflow"))?;
    if bytes > MAX_KERNEL_BYTES { return Ok(value); }
    let mut cache = cache().lock().map_err(|_| error("MAGNETIC_CACHE", "poisoned geometry kernel cache"))?;
    while !cache.entries.is_empty() && (cache.entries.len() >= MAX_ENTRIES || cache.bytes + bytes > MAX_KERNEL_BYTES) {
        let old = cache.entries.remove(0);
        cache.bytes -= old.value.len() * 8;
    }
    cache.bytes += bytes;
    cache.entries.push(Entry { key, value: Arc::new(value.clone()) });
    Ok(value)
}

fn restore_square(canonical: &[f64], permutation: &[usize]) -> Result<Vec<f64>> {
    let n = permutation.len();
    validate(canonical, n.checked_mul(n).ok_or_else(|| error("MAGNETIC_CACHE", "square kernel shape overflow"))?)?;
    let mut output = vec![0.; n * n];
    for (ci, &oi) in permutation.iter().enumerate() {
        for (cj, &oj) in permutation.iter().enumerate() {
            output[oi * n + oj] = canonical[ci * n + cj];
        }
    }
    Ok(output)
}

fn restore_pair(canonical: &[f64], left: &[usize], right: &[usize]) -> Result<Vec<f64>> {
    validate(canonical, left.len().checked_mul(right.len()).ok_or_else(|| error("MAGNETIC_CACHE", "pair kernel shape overflow"))?)?;
    let mut output = vec![0.; left.len() * right.len()];
    for (ci, &oi) in left.iter().enumerate() {
        for (cj, &oj) in right.iter().enumerate() {
            output[oi * right.len() + oj] = canonical[ci * right.len() + cj];
        }
    }
    Ok(output)
}

pub fn square<F>(cells: &[Tetra], order: usize, tolerance: f64, compute: F) -> Result<Vec<f64>>
where F: FnOnce(&[Tetra]) -> Result<Vec<f64>> {
    let (canonical, permutation) = canonical(cells);
    let key = key(&canonical, order, tolerance);
    let n = canonical.len();
    let len = n.checked_mul(n).ok_or_else(|| error("MAGNETIC_CACHE", "square kernel shape overflow"))?;
    if let Some(value) = get(&key, len)? { return restore_square(&value, &permutation); }
    let value = put(key, compute(&canonical)?)?;
    restore_square(&value, &permutation)
}

pub fn pair<F>(left: &[Tetra], right: &[Tetra], order: usize, tolerance: f64, compute: F) -> Result<Vec<f64>>
where F: FnOnce(&[Tetra], &[Tetra]) -> Result<Vec<f64>> {
    let (left, left_permutation) = canonical(left);
    let (right, right_permutation) = canonical(right);
    let mut identity = key(&left, order, tolerance);
    identity.extend(key(&right, order, tolerance));
    let len = left.len().checked_mul(right.len()).ok_or_else(|| error("MAGNETIC_CACHE", "pair kernel shape overflow"))?;
    if let Some(value) = get(&identity, len)? { return restore_pair(&value, &left_permutation, &right_permutation); }
    let value = put(identity, compute(&left, &right)?)?;
    restore_pair(&value, &left_permutation, &right_permutation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    fn tetra(offset: f64) -> Tetra {
        [[offset, 0., 0.], [offset + 1., 0., 0.], [offset, 1., 0.], [offset, 0., 1.]]
    }
    #[test]
    fn exact_hit_skips_work_and_restores_cell_order() {
        let a = tetra(0.);
        let b = tetra(2.);
        let calls = Cell::new(0);
        let compute = |cells: &[Tetra]| {
            calls.set(calls.get() + 1);
            Ok(cells.iter().flat_map(|_| cells.iter().map(|_| 1.)).collect())
        };
        let first = square(&[a, b], 12, 1e-10, &compute).unwrap();
        let second = square(&[b, a], 12, 1e-10, &compute).unwrap();
        assert_eq!(calls.get(), 1);
        assert_eq!(first, second);
    }
    #[test]
    fn every_geometry_order_and_tolerance_bit_is_a_miss() {
        let a = tetra(0.);
        for (offset, order, tolerance) in [(3., 12, 1e-10), (0., 16, 1e-10), (0., 12, 1e-12)] {
            let calls = Cell::new(0);
            square(&[a, tetra(offset)], order, tolerance, |cells| {
                calls.set(calls.get() + 1);
                Ok(vec![1.; cells.len() * cells.len()])
            }).unwrap();
            assert_eq!(calls.get(), 1);
        }
    }
    #[test]
    fn directed_pairs_restore_both_permutations() {
        let cells = [tetra(0.), tetra(2.), tetra(4.)];
        let value = pair(&cells, &cells, 12, 1e-10, |left, right| {
            Ok(left.iter().flat_map(|a| right.iter().map(move |b| 1. + a[0][0] + 10. * b[0][0])).collect())
        }).unwrap();
        let reversed = pair(&cells, &cells, 12, 1e-10, |_, _| panic!("exact pair must hit")).unwrap();
        assert_eq!(value, reversed);
    }
}
