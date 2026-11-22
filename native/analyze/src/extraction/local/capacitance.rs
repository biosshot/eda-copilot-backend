//! Sparse dielectric Schur apply with explicit metal trace and far boundary.
//! All metal potentials remain; no conductor is selected as a hidden ground.
use super::{
    error,
    mesh::{self, LayeredMesh},
    AllocationBudget, Result,
};
use faer::{
    dyn_stack::{MemBuffer, MemStack},
    sparse::linalg::cholesky,
    Conj, Mat, Par,
};
use std::collections::{BTreeMap, BTreeSet};
use super::accurate::Sum;

#[derive(Clone, Debug)]
pub struct TraceRow {
    pub node: usize,
    pub weights: Vec<(usize, f64)>,
}

#[derive(Debug)]
pub struct Boundary {
    pub potential_count: usize,
    pub metal_trace: Vec<TraceRow>,
    pub far_zero_nodes: Vec<usize>,
    /// Explicitly permit natural insulated boundary faces, e.g. a plate coupon.
    /// Real infinity truncation must assign the complete external far boundary.
    pub insulated_external: bool,
}

pub struct CapacitanceOperator<'a> {
    mesh: &'a LayeredMesh,
    parallelism:Par,
    trace: BTreeMap<usize, Vec<(usize, f64)>>,
    far: BTreeSet<usize>,
    free_map: Vec<usize>,
    potential_count: usize,
    free: usize,
    terms: Vec<(usize, usize, f64)>,
    rhs_terms: Vec<(usize, usize, f64)>,
    symbolic: Option<cholesky::SymbolicCholesky<usize>>,
    factor_values: Vec<f64>,
    pub planned_peak_bytes: usize,
    pub factor_value_count: usize,
    retained_storage_bytes: usize,
}

#[derive(Debug)]
pub struct CapacitanceAction {
    pub charges_c: Vec<f64>,
    pub nodal_potentials_v: Vec<f64>,
    pub nodal_potential_corrections_v: Vec<f64>,
    pub electric_fields_v_per_m: Vec<[f64; 3]>,
    pub energy_j: f64,
    pub far_charge_c: f64,
    pub normalized_free_residual: f64,
    pub normalized_charge_balance: f64,
    pub normalized_energy_reaction_error: f64,
}

impl<'a> CapacitanceOperator<'a> {
    /// Live operator buffers after ordering/assembly/factor scratch is dropped.
    /// The pinned faer symbolic indices are conservatively charged alongside
    /// every factor value plus128 bytes per free coordinate for headers/maps.
    pub fn retained_bytes(&self)->Result<usize>{
        let mut bytes=self.mesh.retained_bytes()?;let mut add=|count:usize,width:usize|->Result<()>{bytes=bytes.checked_add(count.checked_mul(width).ok_or_else(||error("MEMORY_LIMIT","retained field count overflow"))?).ok_or_else(||error("MEMORY_LIMIT","retained field storage overflow"))?;Ok(())};
        add(self.terms.capacity(),std::mem::size_of::<(usize,usize,f64)>())?;add(self.rhs_terms.capacity(),std::mem::size_of::<(usize,usize,f64)>())?;add(self.factor_values.capacity(),16)?;add(self.free_map.capacity(),8)?;add(self.free,128)?;add(self.far.len(),64)?;add(self.trace.len(),128)?;for row in self.trace.values(){add(row.capacity(),std::mem::size_of::<(usize,f64)>())?;}Ok(bytes.max(self.retained_storage_bytes))
    }
    pub fn action_workspace_bytes(&self)->Result<usize>{
        let scratch=self.symbolic.as_ref().map_or(0,|s|s.solve_in_place_scratch::<f64>(1,self.parallelism).unaligned_bytes_required());self.mesh.nodes_m.len().checked_mul(128).and_then(|v|v.checked_add(self.mesh.tetrahedra.len().checked_mul(32)?)).and_then(|v|v.checked_add(scratch)).and_then(|v|v.checked_add(524288)).ok_or_else(||error("MEMORY_LIMIT","field action workspace overflow"))
    }
    pub fn new(
        mesh: &'a LayeredMesh,
        boundary: Boundary,
        budget: AllocationBudget,
    ) -> Result<Self> {
        let n = mesh.nodes_m.len();
        let t = mesh.tetrahedra.len();
        // Extrusion/replay temporaries have been released by this phase. Count
        // actual retained mesh buffers, independently of their historical peak.
        let trace_weights=boundary.metal_trace.iter().try_fold(0usize,|sum,row|sum.checked_add(row.weights.capacity())).ok_or_else(||error("MEMORY_LIMIT","field trace count overflow"))?;
        let base = n
            .checked_mul(768)
            .and_then(|v|v.checked_add(trace_weights.checked_mul(16)?))
            .and_then(|v| v.checked_add(mesh.retained_bytes().ok()?))
            .ok_or_else(|| error("MEMORY_LIMIT", "dielectric plan overflow"))?;
        budget.check(base)?;
        if n > budget.max_nodes || t > budget.max_tetrahedra || boundary.potential_count == 0 {
            return Err(error(
                "FIELD_INPUT",
                "mesh/potential count outside explicit budget",
            ));
        }
        let external: BTreeSet<_> = mesh
            .external_faces
            .iter()
            .flat_map(|f| f.iter().copied())
            .collect();
        let copper: BTreeSet<_> = mesh
            .copper_interfaces
            .iter()
            .flat_map(|f| f.nodes)
            .collect();
        let active: BTreeSet<_> = mesh.tetrahedra.iter().flat_map(|t| t.nodes).collect();
        let mut trace = BTreeMap::new();
        let mut represented = BTreeSet::new();
        for row in boundary.metal_trace {
            if row.node >= n
                || !active.contains(&row.node)
                || (!external.contains(&row.node) && !copper.contains(&row.node))
                || row.weights.is_empty()
                || trace.contains_key(&row.node)
            {
                return Err(error(
                    "TRACE_INPUT",
                    "duplicate/nonboundary/empty metal trace row",
                ));
            }
            let mut columns = BTreeSet::new();
            let mut sum = 0.;
            for &(col, value) in &row.weights {
                if col >= boundary.potential_count
                    || !value.is_finite()
                    || value < 0.
                    || !columns.insert(col)
                {
                    return Err(error(
                        "TRACE_INPUT",
                        "invalid/duplicate metal trace support",
                    ));
                }
                if value > 0. {
                    represented.insert(col);
                }
                sum += value;
            }
            if (sum - 1.).abs() > 1e-12 {
                return Err(error(
                    "TRACE_PARTITION",
                    "metal trace must preserve constant potential",
                ));
            }
            trace.insert(row.node, row.weights);
        }
        if represented.len() != boundary.potential_count
            || copper.iter().any(|i| !trace.contains_key(i))
        {
            return Err(error(
                "TRACE_INCOMPLETE",
                "all metal interface vertices and potential modes need trace support",
            ));
        }
        let mut far = BTreeSet::new();
        for i in boundary.far_zero_nodes {
            if !external.contains(&i) || trace.contains_key(&i) || !far.insert(i) {
                return Err(error(
                    "FAR_BOUNDARY",
                    "far zero nodes must be unique external nonmetal vertices",
                ));
            }
        }
        if !boundary.insulated_external
            && external
                .iter()
                .any(|i| !trace.contains_key(i) && !far.contains(i))
        {
            return Err(error(
                "FAR_INCOMPLETE",
                "external boundary lacks explicit far/electrode condition",
            ));
        }
        let mut free_map = vec![usize::MAX; n];
        let mut free = 0;
        for &i in &active {
            if !trace.contains_key(&i) && !far.contains(&i) {
                free_map[i] = free;
                free += 1;
            }
        }
        drop(external);drop(copper);drop(active);drop(represented);
        let(terms,rhs_terms,matrix,mut plan)=super::field_assembly::assemble(mesh,&free_map,free,base,budget)?;
        let mut retained_storage_bytes=mesh.retained_bytes()?.checked_add(free_map.capacity()*8).and_then(|v|v.checked_add(terms.capacity().checked_mul(24)?)).and_then(|v|v.checked_add(rhs_terms.capacity().checked_mul(24)?)).and_then(|v|v.checked_add(trace.len().checked_mul(128)?)).and_then(|v|v.checked_add(far.len().checked_mul(64)?)).and_then(|v|v.checked_add(trace_weights.checked_mul(16)?)).ok_or_else(||error("MEMORY_LIMIT","retained sparse field overflow"))?;
        let(mut symbolic,mut factor_values)=(None,Vec::new());
        if free>0{let(s,values,peak,persistent)=super::sparse_potential::factor(&matrix,retained_storage_bytes,budget,&[]).map_err(|e|format!("{e}; dielectric Kff, no regularization"))?;plan=plan.max(peak);retained_storage_bytes=persistent.checked_sub(matrix.compute_nnz()*16).ok_or_else(||error("MEMORY_LIMIT","retained sparse field underflow"))?;symbolic=Some(s);factor_values=values;}
        drop(matrix);
        let factor_value_count = factor_values.len();
        Ok(Self {
            parallelism:super::parallel::current(),
            mesh,
            trace,
            far,
            free_map,
            potential_count: boundary.potential_count,
            free,
            terms,
            rhs_terms,
            symbolic,
            factor_values,
            planned_peak_bytes: plan,
            factor_value_count,
            retained_storage_bytes,
        })
    }

    pub fn apply(&self, volts: &[f64]) -> Result<CapacitanceAction> {
        if volts.len() != self.potential_count || volts.iter().any(|v| !v.is_finite()) {
            return Err(error(
                "FIELD_VOLTAGE",
                "finite full potential vector required",
            ));
        }
        // Only an entirely insulated exterior has a constant-potential gauge.
        // Infinity truncation has explicit far nodes and preserves common mode.
        let gauge = if self.far.is_empty() { volts[0] } else { 0. };
        let shifted: Vec<_> = volts.iter().map(|v| v - gauge).collect();
        let mut phi = vec![0.; self.mesh.nodes_m.len()];
        for (&node, row) in &self.trace {
            phi[node] = row.iter().map(|&(col, w)| shifted[col] * w).sum();
        }
        if self.free > 0 {
            let mut rhs = Mat::<f64>::zeros(self.free, 1);
            for &(a, b, k) in &self.rhs_terms {
                if self.free_map[a] != usize::MAX && self.free_map[b] == usize::MAX {
                    rhs[(self.free_map[a], 0)] -= k * phi[b];
                }
            }
            let s = self.symbolic.as_ref().unwrap();
            let scratch = s.solve_in_place_scratch::<f64>(1, self.parallelism);
            let mut workspace = MemBuffer::try_new(scratch)
                .map_err(|_| error("MEMORY_LIMIT", "solve scratch allocation"))?;
            cholesky::LltRef::new(s, &self.factor_values).solve_in_place_with_conj(
                Conj::No,
                rhs.as_mut(),
                self.parallelism,
                MemStack::new(&mut workspace),
            );
            for (i, &map) in self.free_map.iter().enumerate() {
                if map != usize::MAX {
                    phi[i] = rhs[(map, 0)];
                }
            }
        }
        // Refine against the variational action evaluated in nodal differences.
        // This is the same P1 operator; it avoids cancellation of common-mode
        // stiffness terms on thin, exact material/contact intersections.
        let mut phi_low=vec![0.;phi.len()];
        for _ in 0..8 {
            if self.free == 0 { break; }
            let (reaction, _, _) = self.variational_action(&phi,&phi_low);
            let free_error=self.free_map.iter().enumerate().filter(|(_,m)| **m!=usize::MAX).map(|(i,_)|reaction[i].abs()).sum::<f64>();
            let physical_scale=reaction.iter().map(|q|q.abs()).sum::<f64>();
            if free_error <= 1e-14*physical_scale { break; }
            let mut correction = Mat::<f64>::zeros(self.free,1);
            for (i,&m) in self.free_map.iter().enumerate() { if m != usize::MAX { correction[(m,0)] = -reaction[i]/super::super::EPS0; }}
            let s=self.symbolic.as_ref().unwrap();
            let mut workspace=MemBuffer::try_new(s.solve_in_place_scratch::<f64>(1,self.parallelism)).map_err(|_|error("MEMORY_LIMIT","refinement scratch"))?;
            cholesky::LltRef::new(s,&self.factor_values).solve_in_place_with_conj(Conj::No,correction.as_mut(),self.parallelism,MemStack::new(&mut workspace));
            for (i,&m) in self.free_map.iter().enumerate() { if m != usize::MAX {
                let next=Sum{hi:phi[i],lo:phi_low[i]}.add(Sum::from(correction[(m,0)]));phi[i]=next.hi;phi_low[i]=next.lo;
            }}
        }
        let eps0 = super::super::EPS0;
        let (reaction, scale, fields) = self.variational_action(&phi,&phi_low);
        let twice_energy = phi.iter().zip(&reaction).map(|(v,q)|v*q).sum::<f64>();
        let mut charges = vec![0.; volts.len()];
        for (&node, row) in &self.trace {
            for &(col, w) in row {
                charges[col] += w * reaction[node];
            }
        }
        let far_charge_c = self.far.iter().map(|&i| reaction[i]).sum::<f64>();
        let residual = self
            .free_map
            .iter()
            .enumerate()
            .filter(|(_, m)| **m != usize::MAX)
            .map(|(i, _)| reaction[i].abs() / scale[i].max(f64::MIN_POSITIVE))
            .fold(0., f64::max);
        let charge_scale = charges.iter().map(|x| x.abs()).sum::<f64>() + far_charge_c.abs();
        let balance = (charges.iter().sum::<f64>() + far_charge_c).abs()
            / charge_scale.max(f64::MIN_POSITIVE);
        let boundary_work = shifted
            .iter()
            .zip(&charges)
            .map(|(v, q)| v * q)
            .sum::<f64>();
        let field_energy_twice = self
            .mesh
            .tetrahedra
            .iter()
            .zip(&fields)
            .map(|(t, e)| eps0 * t.epsilon_r * t.volume_m3 * e.iter().map(|x| x * x).sum::<f64>())
            .sum::<f64>();
        let energy_error = (field_energy_twice - boundary_work).abs()
            / field_energy_twice
                .abs()
                .max(boundary_work.abs())
                .max(f64::MIN_POSITIVE);
        if field_energy_twice == 0. && fields.iter().flatten().any(|x| *x != 0.) {
            return Err(error(
                "FIELD_RANGE",
                "nonzero recovered field energy underflow",
            ));
        }
        if phi
            .iter()
            .chain(charges.iter())
            .chain(fields.iter().flatten())
            .any(|x| !x.is_finite())
            || ![residual, balance, energy_error, twice_energy]
                .iter()
                .all(|x| x.is_finite())
            || residual > 1e-8
            || balance > 1e-8
            || energy_error > 1e-8
            || field_energy_twice < 0.
        {
            return Err(error(
                "FIELD_BALANCE",
                &format!("free KCL={residual:e}, charge={balance:e}, energy/reaction={energy_error:e}, field2E={field_energy_twice:e}, work={boundary_work:e}; unchanged1e-8 gates"),
            ));
        }
        for (i, p) in phi.iter_mut().enumerate() {
            if self.free_map[i] != usize::MAX || self.trace.contains_key(&i) {
                let restored=Sum{hi:*p,lo:phi_low[i]}.add(Sum::from(gauge));*p=restored.hi;phi_low[i]=restored.lo;
            }
        }
        if phi.iter().any(|x| !x.is_finite()) {
            return Err(error("FIELD_RANGE", "potential gauge recovery overflow"));
        }
        Ok(CapacitanceAction {
            charges_c: charges,
            nodal_potentials_v: phi,
            nodal_potential_corrections_v: phi_low,
            electric_fields_v_per_m: fields,
            energy_j: field_energy_twice / 2.,
            far_charge_c,
            normalized_free_residual: residual,
            normalized_charge_balance: balance,
            normalized_energy_reaction_error: energy_error,
        })
    }

    fn variational_action(&self, phi: &[f64], low:&[f64]) -> (Vec<f64>, Vec<f64>, Vec<[f64;3]>) {
        let mut reaction=vec![Sum::default();phi.len()];
        // Retain the inherited assembled-equation backward-error scale. The
        // sum of element reaction magnitudes alone vanishes for a constant
        // field, and would turn roundoff at zero-field nodes into O(1) error.
        let mut scale=vec![0.;phi.len()];
        for &(a,b,k) in &self.terms {scale[a]+=(super::super::EPS0*k*phi[b]).abs();}
        let mut fields=Vec::with_capacity(self.mesh.tetrahedra.len());
        for t in &self.mesh.tetrahedra {
            let gradient:[Sum;3]=std::array::from_fn(|a|(1..4).fold(Sum::default(),|sum,i| {
                let node=t.nodes[i];let anchor=t.nodes[0];
                sum.add(Sum{hi:phi[node],lo:low[node]}.sub(Sum{hi:phi[anchor],lo:low[anchor]}).mul(t.gradients_per_m[i][a]))
            }));
            let weight=super::super::EPS0*t.epsilon_r*t.volume_m3;
            let mut q=[Sum::default();4];
            for i in 1..4 {q[i]=(0..3).fold(Sum::default(),|sum,a|sum.add(gradient[a].mul(t.gradients_per_m[i][a]))).mul(weight);}
            q[0]=q[1].add(q[2]).add(q[3]).neg();
            for i in 0..4 {reaction[t.nodes[i]]=reaction[t.nodes[i]].add(q[i]);}
            fields.push(gradient.map(|x|-x.value()));
        }
        (reaction.into_iter().map(Sum::value).collect(),scale,fields)
    }
}
