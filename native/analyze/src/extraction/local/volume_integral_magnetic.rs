//! Direct constant-vector finite-volume magnetostatic action on the unchanged
//! copper cells. The dense kernel is intended for bounded local blocks; its
//! quadratic storage is admitted explicitly instead of hidden behind a mesh solve.
use super::{error,magnetic_kernel_cache,mesh::LayeredMesh,polyhedron::{PointPotential,TetraCubature,separated_order,tetra_integral_adaptive,tetra_self_integral},volume_current::Space,volume_magnetic::MagneticApply,accurate::Sum,AllocationBudget,Result};

pub struct Operator<'a> {
    mesh:&'a LayeredMesh,
    kernel:Vec<f64>,
    planned_bytes:usize,
}

impl<'a> Operator<'a> {
    pub fn new(mesh:&'a LayeredMesh,budget:AllocationBudget,order:usize)->Result<Self> {
        if !(8..=16).contains(&order){return Err(error("MAGNETIC_QUADRATURE","direct volume integral orders8..16 required"));}
        let n=mesh.copper_tetrahedra.len();
        if n==0{return Err(error("MAGNETIC_MESH","direct volume integral requires copper cells"));}
        let kernel_bytes=n.checked_mul(n).and_then(|v|v.checked_mul(8)).ok_or_else(||error("MEMORY_LIMIT","direct magnetic kernel overflow"))?;
        let orders:Vec<_>=[4,6,8,12,16].into_iter().filter(|&value|value<=order).collect();
        let cubature_bytes=n.checked_mul(orders.iter().map(|value|value.pow(3)*32).sum()).ok_or_else(||error("MEMORY_LIMIT","direct magnetic cubature overflow"))?;
        let plan=mesh.retained_bytes()?.checked_add(kernel_bytes).and_then(|v|v.checked_add(n.checked_mul(16384)?)).and_then(|v|v.checked_add(cubature_bytes)).ok_or_else(||error("MEMORY_LIMIT","direct magnetic plan overflow"))?;
        budget.check(plan)?;
        let tetra:Vec<_>=mesh.copper_tetrahedra.iter().map(|t|t.nodes.map(|i|mesh.nodes_m[i])).collect();
        let kernel=magnetic_kernel_cache::square(&tetra,order,1e-10,|tetra|{
            let n=tetra.len();
            let sources:Vec<_>=tetra.iter().copied().map(PointPotential::tetrahedron).collect::<Result<Vec<_>>>()?;
            let cubatures:Vec<Vec<_>>=tetra.iter().copied().map(|cell|orders.iter().map(|&value|TetraCubature::new(cell,value)).collect::<Result<Vec<_>>>()).collect::<Result<Vec<_>>>()?;
            let mut kernel=vec![0.;n*n];
            for i in 0..n {for j in 0..=i {
                let value=if i==j{tetra_self_integral(tetra[i],order)?}else{
                    let q=cubatures[i][0].separation_q(&cubatures[j][0]);
                    if let Some(index)=separated_order(q,order).and_then(|value|orders.iter().position(|&candidate|candidate==value)){
                        cubatures[i][index].direct_integral(&cubatures[j][index])?
                    }else{tetra_integral_adaptive(tetra[i],&sources[j],order,1e-10)?}
                };
                kernel[i*n+j]=value;kernel[j*n+i]=value;
            }}
            Ok(kernel)
        })?;
        Ok(Self{mesh,kernel,planned_bytes:plan})
    }
    fn apply_current(&self,space:&Space,current:&[f64])->Result<Vec<f64>> {
        if current.len()!=space.current_count()||current.iter().any(|x|!x.is_finite()){return Err(error("MAGNETIC_INPUT","finite full current vector required"));}
        let n=self.mesh.copper_tetrahedra.len();let mut out=vec![Sum::default();current.len()];
        for(i,target)in space.cells.iter().enumerate(){for(j,source)in space.cells.iter().enumerate(){
            let coefficient=1e-7*target.density_per_amp_m2*source.density_per_amp_m2*self.kernel[target.mesh_cell*n+source.mesh_cell];
            for k in 0..3{out[3*i+k]=out[3*i+k].add(Sum::from(current[3*j+k]).mul(coefficient));}
        }}
        let out:Vec<_>=out.into_iter().map(|v|v.value()).collect();
        if out.iter().any(|x|!x.is_finite()){return Err(error("MAGNETIC_RANGE","nonfinite direct volume magnetic action"));}
        Ok(out)
    }
}

impl MagneticApply for Operator<'_> {
    fn apply(&self,space:&Space,current:&[f64])->Result<Vec<f64>>{self.apply_current(space,current)}
    fn planned_bytes(&self)->usize{self.planned_bytes}
    fn scratch_bytes(&self)->usize{self.planned_bytes}
}
