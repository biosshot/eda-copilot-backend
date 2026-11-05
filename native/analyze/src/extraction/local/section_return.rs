//! Conforming section-current lift, not an admitted shared-return model.
//! Profiles include their physical reference copper. Several columns may use
//! the same return cells; those cells and common R/L/C operators exist once.
//! No section impedance or field-energy stamp is added here.
#![allow(dead_code)] // Private mapping control; no production dispatch yet.
use super::{error, mesh::LayeredMesh, volume_current::Space, AllocationBudget, Result};
use crate::extraction::section_transition::{Mesh, Recovery};
use std::collections::BTreeMap;

/// Lift one section through a contiguous set of native extrusion slabs, along
/// the mesh Z axis. Exact authored XY vertex correspondence is required.
/// The P1 profile is integrated exactly on each tetrahedron and represented
/// by its P0 mean. This conserves its integrated current, not its P1 loss;
/// Joule energy must come from the physical cell R operator after projection.
pub(in crate::extraction) fn lift(
    section:&Mesh<'_>, recovery:&Recovery, mesh:&LayeredMesh, space:&Space,
    slabs:std::ops::Range<usize>, budget:AllocationBudget,
) -> Result<Vec<Vec<f64>>> {
    if slabs.is_empty() {return Err(error("SECTION_RETURN_MAP","nonempty section extent required"));}
    let loops=section.conductors.len().checked_sub(1).ok_or_else(||error("SECTION_RETURN_MAP","conductors required"))?;
    let bytes=space.current_count().checked_mul(loops).and_then(|n|n.checked_mul(16)).and_then(|n|n.checked_add(space.planned_bytes)).ok_or_else(||error("MEMORY_LIMIT","section lift overflow"))?;
    budget.check(bytes)?;
    let mut rows=BTreeMap::new();
    for row in &recovery.triangles {
        let tri=section.triangles.get(row.triangle_index).ok_or_else(||error("SECTION_RETURN_MAP","unknown section triangle"))?;
        if row.nodes!=tri.nodes||tri.conductor!=Some(row.conductor)||row.jz_a_per_m2.len()!=loops||rows.insert(row.triangle_index,row).is_some() {
            return Err(error("SECTION_RETURN_MAP","duplicate or mismatched section ownership"));
        }
    }
    let mut out=vec![vec![0.;space.current_count()];2*loops];
    let mut covered=vec![false;section.triangles.len()];
    for (ci,cell) in space.cells.iter().enumerate() {
        let tet=mesh.copper_tetrahedra.get(cell.mesh_cell).ok_or_else(||error("SECTION_RETURN_MAP","unknown physical cell"))?;
        if !slabs.contains(&tet.slab) {continue;}
        let row=rows.get(&tet.planar_triangle).ok_or_else(||error("SECTION_RETURN_MAP","copper cell lacks exclusive section owner"))?;
        let mut weights=[0.;3];
        for &node in &tet.nodes {
            let p=mesh.nodes_m.get(node).ok_or_else(||error("SECTION_RETURN_MAP","unknown physical vertex"))?;
            let local=row.nodes.iter().position(|&n|section.nodes_m.get(n).is_some_and(|q|q[0]==p[0]&&q[1]==p[1])).ok_or_else(||error("SECTION_RETURN_MAP","nonconforming section vertex"))?;
            weights[local]+=0.25;
        }
        if !cell.density_per_amp_m2.is_finite()||cell.density_per_amp_m2<=0. {return Err(error("SECTION_RETURN_MAP","invalid physical density scale"));}
        for p in 0..loops {
            let real=(0..3).map(|k|weights[k]*row.jz_a_per_m2[p][k].real).sum::<f64>()/cell.density_per_amp_m2;
            let imag=(0..3).map(|k|weights[k]*row.jz_a_per_m2[p][k].imaginary).sum::<f64>()/cell.density_per_amp_m2;
            if !real.is_finite()||!imag.is_finite() {return Err(error("SECTION_RETURN_MAP","nonfinite profile"));}
            out[2*p][3*ci+2]=real;out[2*p+1][3*ci+2]=imag;
        }
        covered[tet.planar_triangle]=true;
    }
    if rows.keys().any(|&i|!covered[i]) {return Err(error("SECTION_RETURN_MAP","incomplete section copper support"));}
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{mesh::{self,LayeredInput,Material},volume_current,volume_reduction};
    use crate::extraction::section_transition::{self,Conductor,Triangle};
    use std::time::{Instant,Duration};

    #[test]
    fn actual_section_profiles_share_return_and_conserve_slab_current() {
        let budget=AllocationBudget{max_bytes:64*1024*1024,max_nodes:10000,max_tetrahedra:10000};
        let nodes:Vec<_>=(0..8).flat_map(|y|[[0.,y as f64*1e-3],[0.5e-3,y as f64*1e-3],[1e-3,y as f64*1e-3]]).collect();
        let triangles:Vec<_>=(0..7).flat_map(|y|(0..2).flat_map(move |x|{
            let conductor=if y%2==1 {Some(y/2)}else{None};
            let a=3*y+x;
            [[a,a+1,a+4],[a,a+4,a+3]].map(|nodes|Triangle{nodes,conductor})
        })).collect();
        let conductors=[Conductor{id:"signal-a",conductivity_s_m:58e6},Conductor{id:"signal-b",conductivity_s_m:58e6},Conductor{id:"finite-return",conductivity_s_m:58e6}];
        let section=Mesh{nodes_m:&nodes,triangles:&triangles,conductors:&conductors,reference:2};
        let (_,recovery)=section_transition::solve_with_recovery(&section,1000.,0,&section_transition::Budget{max_bytes:budget.max_bytes,deadline:Instant::now()+Duration::from_secs(10)},1024*1024).unwrap();
        let material:Vec<_>=triangles.iter().map(|t|match t.conductor {Some(c)=>Material::Copper{source_ids:vec![conductors[c].id.into()]},None=>Material::Dielectric{label:"gap".into(),epsilon_r:1.}}).collect();
        let mut edges=vec![[0,1],[1,2],[21,22],[22,23]];
        for y in 0..7 {edges.extend([[3*y,3*y+3],[3*y+2,3*y+5]]);}
        let physical=mesh::extrude(&LayeredInput{xy_m:nodes.clone(),triangles:triangles.iter().map(|t|t.nodes).collect(),boundary_edges:edges,z_m:vec![0.,1e-3,2e-3],materials:vec![material;2]},budget).unwrap();
        let electrodes:Vec<Vec<usize>>=(0..3).flat_map(|c|[0.,2e-3].map(|z|physical.nodes_m.iter().enumerate().filter_map(|(i,p)|(p[2]==z&&p[1]>=(2*c+1)as f64*1e-3&&p[1]<=(2*c+2)as f64*1e-3).then_some(i)).collect())).collect();
        let space=volume_current::assemble(&physical,&vec![58e6;physical.copper_tetrahedra.len()],&electrodes,budget).unwrap();
        let contacts:Vec<_>=electrodes.iter().enumerate().map(|(i,n)|(format!("contact-{i}"),space.node_potential[&n[0]])).collect();
        let mut basis=volume_reduction::harmonic(&space,&contacts,budget).unwrap();
        let original_voltage=basis.voltage.clone();let original_components=basis.component_columns.clone();
        let mut both=vec![];
        for slab in 0..2 {
            let profiles=lift(&section,&recovery,&physical,&space,slab..slab+1,budget).unwrap();
            for p in 0..2 {for c in 0..3 {
                let total:f64=space.cells.iter().enumerate().filter(|(_,cell)|{let t=&physical.copper_tetrahedra[cell.mesh_cell];t.slab==slab&&triangles[t.planar_triangle].conductor==Some(c)}).map(|(i,cell)|profiles[2*p][3*i+2]*cell.density_per_amp_m2*physical.copper_tetrahedra[cell.mesh_cell].volume_m3/1e-3).sum();
                let expected=if c==p {1.}else if c==2 {-1.}else{0.};
                assert!((total-expected).abs()<1e-8,"slab={slab}, conductor={c}: {total}");
            }}
            // Both signal profiles overlap on the SAME physical return cells.
            assert!(space.cells.iter().enumerate().any(|(i,_)|profiles[0][3*i+2]!=0.&&profiles[2][3*i+2]!=0.));
            both.extend(profiles);
        }
        basis.include_current_profiles(&space,&both,budget).unwrap();
        assert_eq!(basis.voltage,original_voltage);assert_eq!(basis.component_columns,original_components);assert_eq!(basis.component_columns.len(),3);
        let before=basis.current.clone();
        assert!(basis.include_current_profiles(&space,&[vec![f64::NAN;space.current_count()]],budget).is_err());
        assert_eq!(basis.current,before);
        let mut bad=physical.clone();bad.nodes_m[3][0]+=1e-9;
        assert!(lift(&section,&recovery,&bad,&space,0..1,budget).unwrap_err().contains("nonconforming"));
        assert!(lift(&section,&recovery,&physical,&space,2..3,budget).unwrap_err().contains("incomplete"));
    }
}
