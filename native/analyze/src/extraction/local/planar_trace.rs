//! Exact geometry adapter from mixed conductor surface coordinates to layered traces.
use super::{error, Result, mixed_conductor::MixedMap, layered_capacitance::{Input, PlaneInput, PotentialTrace, TraceNode, Budget}};
use geo::{Intersects, LineString, Point, Polygon};
use std::{collections::BTreeMap, time::Instant};
use rstar::{RTree, RTreeObject, AABB};
#[derive(Clone)]
struct FaceIndex { index: usize, bounds: AABB<[f64;3]> }
impl RTreeObject for FaceIndex { type Envelope=AABB<[f64;3]>; fn envelope(&self)->Self::Envelope { self.bounds } }

fn key(p: [f64; 3]) -> [u64; 3] { p.map(|v| if v == 0.0 { 0 } else { v.to_bits() }) }

/// Exact field nodes only: no nearest-node interpolation or net-name welding.
/// CDT-generated vertices are not predictable here and therefore refuse.
pub(super) fn trace(input: &Input, map: &MixedMap<'_>, budget: Budget) -> Result<PotentialTrace> {
    let started = Instant::now();
    let mesh = &map.mesh().mesh;
    let polygon_vertices = input.metals.iter().try_fold(0usize, |n,m| m.holes_m.iter().try_fold(n.checked_add(m.polygon_m.len())?, |v,r| v.checked_add(r.len()))).ok_or_else(||error("MEMORY_LIMIT", "polygon size overflow"))?;
    let reserve = mesh.copper_tetrahedra.len().checked_mul(4).and_then(|v|v.checked_mul(192)).and_then(|v|v.checked_add(mesh.copper_interfaces.len().checked_mul(128)?)).and_then(|v|v.checked_add(polygon_vertices.checked_mul(64)?)).and_then(|v|v.checked_add(map.potential_count().checked_mul(128)?)).ok_or_else(||error("MEMORY_LIMIT", "trace plan overflow"))?;
    if reserve > budget.max_bytes { return Err(error("MEMORY_LIMIT", "trace plan exceeds budget")); }
    let rows = map.electric_node_trace();
    let count = map.potential_count();
    let mut surface = BTreeMap::new();
    for row in rows {
        let p = *map.mesh().mesh.nodes_m.get(row.node).ok_or_else(|| error("PLANAR_TRACE", "surface node out of range"))?;
        if p.iter().any(|v| !v.is_finite()) { return Err(error("PLANAR_TRACE", "nonfinite surface node")); }
        if let Some(old) = surface.insert(key(p), row.weights.clone()) {
            if old != row.weights { return Err(error("PLANAR_TRACE", "conflicting physical potential coordinates")); }
        }
    }
    let index_bytes = mesh.copper_interfaces.len().checked_mul(128).ok_or_else(|| error("MEMORY_LIMIT", "face index overflow"))?;
    if index_bytes > budget.max_bytes || mesh.copper_interfaces.len() > budget.max_local_terms { return Err(error("MEMORY_LIMIT", "face index exceeds budget")); }
    let index = RTree::bulk_load(mesh.copper_interfaces.iter().enumerate().map(|(index,face)| {
        let points = face.nodes.map(|i| mesh.nodes_m[i]);
        let lower = std::array::from_fn(|d| points.iter().map(|p|p[d]).fold(f64::INFINITY,f64::min)-1e-15);
        let upper = std::array::from_fn(|d| points.iter().map(|p|p[d]).fold(f64::NEG_INFINITY,f64::max)+1e-15);
        FaceIndex { index, bounds: AABB::from_corners(lower,upper) }
    }).collect());
    let polygons: Vec<_> = input.metals.iter().map(|m| Polygon::new(LineString::from(m.polygon_m.clone()), m.holes_m.iter().cloned().map(LineString::from).collect())).collect();
    // Establish ownership from physical copper tetrahedra, never coordinate alone.
    let mut owned = BTreeMap::new();
    for tetra in &mesh.copper_tetrahedra {
        if started.elapsed() >= budget.max_wall { return Err(error("DEADLINE", "trace ownership")); }
        let points = tetra.nodes.map(|i| mesh.nodes_m[i]);
        let center: [f64;3] = std::array::from_fn(|axis| points.iter().map(|p|p[axis]).sum::<f64>()/4.);
        let matches: Vec<_> = input.metals.iter().zip(&polygons).enumerate().filter(|(_, (metal,poly))| center[2] > metal.z0_m && center[2] < metal.z1_m && poly.intersects(&Point::new(center[0],center[1]))).map(|(i,_)|i).collect();
        if matches.len() != 1 { return Err(error("PLANAR_TRACE", "ambiguous or missing copper solid ownership")); }
        for point in points {
            let weights = surface.get(&key(point)).ok_or_else(||error("PLANAR_TRACE", "copper node missing mixed potential"))?;
            if let Some(old) = owned.insert((matches[0],key(point)), weights.clone()) {
                if old != *weights { return Err(error("PLANAR_TRACE", "ambiguous owned node")); }
            }
        }
        let bytes=owned.len().checked_mul(192).and_then(|v|v.checked_add(index_bytes)).ok_or_else(||error("MEMORY_LIMIT", "owned nodes overflow"))?;
        if owned.len()>budget.max_nodes || bytes>budget.max_bytes { return Err(error("MEMORY_LIMIT", "owned nodes exceed budget")); }
    }
    let mut nodes = Vec::new();
    for (plane, &mesh_id) in input.planes.iter().enumerate() {
        let z = *input.z_m.get(plane).ok_or_else(|| error("PLANAR_TRACE", "plane z missing"))?;
        let xy = match input.meshes.get(mesh_id) {
            Some(PlaneInput::Exact { xy_m, .. }) => xy_m,
            _ => return Err(error("PLANAR_TRACE", "exact field meshes required")),
        };
        for &p in xy {
            if started.elapsed() >= budget.max_wall { return Err(error("DEADLINE", "planar trace")); }
            for (metal_index, (metal, poly)) in input.metals.iter().zip(&polygons).enumerate() {
                if z >= metal.z0_m && z <= metal.z1_m && poly.intersects(&Point::from(p)) {
                    let position_m = [p[0], p[1], z];
                    let weights = if let Some(exact) = owned.get(&(metal_index,key(position_m))) { exact.clone() } else {
                        let mut candidate: Option<Vec<(usize,f64)>> = None;
                        for hit in index.locate_in_envelope_intersecting(&AABB::from_point(position_m)) {
                            if started.elapsed() >= budget.max_wall { return Err(error("DEADLINE", "planar trace interpolation")); }
                            let face = &mesh.copper_interfaces[hit.index];
                            let coords = face.nodes.map(|i| map.mesh().mesh.nodes_m[i]);
                            if !coords.iter().all(|p| p[2] >= metal.z0_m && p[2] <= metal.z1_m && poly.intersects(&Point::new(p[0],p[1]))) { continue; }
                            if let Some(bary) = barycentric(position_m, coords[0], coords[1], coords[2]) {
                                let mut combined = BTreeMap::<usize,f64>::new();
                                for j in 0..3 {
                                    let row = owned.get(&(metal_index,key(coords[j]))).ok_or_else(|| error("PLANAR_TRACE", "surface triangle missing potential"))?;
                                    for &(i,w) in row { *combined.entry(i).or_default() += bary[j]*w; }
                                }
                                let result: Vec<_> = combined.into_iter().filter(|(_,w)| *w > 1e-14).collect();
                                if let Some(ref previous) = candidate {
                                    if previous.len() != result.len() || previous.iter().zip(&result).any(|(a,b)| a.0 != b.0 || (a.1-b.1).abs() > 1e-10) {
                                        return Err(error("PLANAR_TRACE", "ambiguous surface interpolation across conductors"));
                                    }
                                } else { candidate = Some(result); }
                            }
                        }
                        candidate.ok_or_else(|| error("PLANAR_TRACE", "field node outside physical conductor surface"))?
                    };
                    if weights.iter().any(|&(i,w)| i >= count || !w.is_finite() || w < 0.0) || (weights.iter().map(|x| x.1).sum::<f64>() - 1.0).abs() > 1e-12 { return Err(error("PLANAR_TRACE", "invalid partition of unity")); }
                    nodes.push(TraceNode { metal_source_id: metal.source_id.clone(), position_m, weights });
                    let bytes = nodes.len().checked_mul(256).and_then(|x| x.checked_add(surface.len().checked_mul(128)?)).and_then(|x| x.checked_add(index_bytes)).ok_or_else(|| error("MEMORY_LIMIT", "trace size overflow"))?;
                    if nodes.len() > budget.max_nodes || bytes > budget.max_bytes { return Err(error("MEMORY_LIMIT", "trace exceeds budget")); }
                }
            }
        }
    }
    Ok(PotentialTrace { potential_nodes: (0..count).map(|i| format!("mixed:{i}")).collect(), nodes })
}

// P1 interpolation on an actual surface triangle, with a scale-relative plane check.
fn barycentric(p: [f64;3], a: [f64;3], b: [f64;3], c: [f64;3]) -> Option<[f64;3]> {
    let sub = |x: [f64;3], y: [f64;3]| [x[0]-y[0], x[1]-y[1], x[2]-y[2]];
    let dot = |x: [f64;3], y: [f64;3]| x[0]*y[0]+x[1]*y[1]+x[2]*y[2];
    let u=sub(b,a); let v=sub(c,a); let w=sub(p,a);
    let uu=dot(u,u); let uv=dot(u,v); let vv=dot(v,v); let wu=dot(w,u); let wv=dot(w,v);
    let det=uu*vv-uv*uv;
    if !det.is_finite() || det <= 0.0 { return None; }
    let beta=(wu*vv-wv*uv)/det; let gamma=(wv*uu-wu*uv)/det; let alpha=1.0-beta-gamma;
    let residual=[w[0]-beta*u[0]-gamma*v[0],w[1]-beta*u[1]-gamma*v[1],w[2]-beta*u[2]-gamma*v[2]];
    if dot(residual,residual) > 1e-24*uu.max(vv) || [alpha,beta,gamma].iter().any(|x| *x < -1e-12 || *x > 1.0+1e-12) { return None; }
    let mut weights=[alpha.max(0.0),beta.max(0.0),gamma.max(0.0)]; let sum: f64=weights.iter().sum();
    for x in &mut weights { *x /= sum; } Some(weights)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn surface_interpolation_preserves_constant_and_rejects_void() {
        let a=[0.,0.,0.]; let b=[1.,0.,0.]; let c=[0.,1.,0.];
        let w=barycentric([0.25,0.25,0.],a,b,c).unwrap();
        assert!((w.iter().sum::<f64>()-1.).abs()<1e-14);
        assert!(barycentric([0.8,0.8,0.],a,b,c).is_none());
        assert!(barycentric([0.25,0.25,0.1],a,b,c).is_none());
    }
}

/// Build a conforming layered field request from the already prepared native grid.
/// Distinct overlapping copper solids are refused until ownership is partitioned.
pub(super) fn input_from_board(board: &super::board_mesh::BoardMesh, request: &super::board_mesh::Request, budget: Budget) -> Result<(Input, Vec<super::layered_capacitance::SpatialDielectric>)> {
    use super::layered_capacitance::{Metal, Dielectric, Exterior, SpatialDielectric};
    use geo::{Area, Relate};
    use geo::algorithm::{coordinate_position::CoordPos, dimensions::Dimensions};
    let started=Instant::now();
    let n=board.planar_nodes_mm.len();
    let planned=n.checked_mul(128).and_then(|x| x.checked_add(board.planar_triangles.len().checked_mul(128)?)).and_then(|x| x.checked_add(board.mesh.nodes_m.len().checked_mul(32)?)).ok_or_else(||error("MEMORY_LIMIT", "field request size overflow"))?;
    if n > budget.max_nodes || planned > budget.max_bytes { return Err(error("MEMORY_LIMIT", "field request exceeds budget")); }
    if n == 0 || board.mesh.nodes_m.len()%n != 0 { return Err(error("PLANAR_TRACE", "invalid product mesh")); }
    let xy: Vec<_>=board.planar_nodes_mm.iter().map(|p|[(p[0]-board.origin_mm[0])*1e-3,(p[1]-board.origin_mm[1])*1e-3]).collect();
    let z: Vec<_>=board.mesh.nodes_m.chunks_exact(n).map(|p|p[0][2]).collect();
    let bounds=[[xy.iter().map(|p|p[0]).fold(f64::INFINITY,f64::min),xy.iter().map(|p|p[1]).fold(f64::INFINITY,f64::min)], [xy.iter().map(|p|p[0]).fold(f64::NEG_INFINITY,f64::max),xy.iter().map(|p|p[1]).fold(f64::NEG_INFINITY,f64::max)]];
    let convert = |ring: &geo::LineString<f64>| ring.0.iter().map(|p|[(p.x-board.origin_mm[0])*1e-3,(p.y-board.origin_mm[1])*1e-3]).collect::<Vec<_>>();
    let mut metals=Vec::new();
    let mut output_bytes=planned;
    // Partition at existing z planes, unioning overlapping foil/barrel supports
    // in each slab. This matches the original single-owner copper mesh.
    for slab in z.windows(2) {
        if started.elapsed() >= budget.max_wall { return Err(error("DEADLINE", "field geometry adapter")); }
        let mid=(slab[0]+slab[1])/2.;
        let active=board.field_copper.iter().filter(|(_,span,_)|mid>span[0] && mid<span[1]);
        let vertices=active.clone().flat_map(|(set,_,_)|set.0.iter()).flat_map(|p|std::iter::once(p.exterior()).chain(p.interiors())).try_fold(0usize,|n,r|n.checked_add(r.0.len())).ok_or_else(||error("MEMORY_LIMIT", "copper polygon size overflow"))?;
        let needed=planned.checked_add(vertices.checked_mul(1024).ok_or_else(||error("MEMORY_LIMIT", "union size overflow"))?).ok_or_else(||error("MEMORY_LIMIT", "union size overflow"))?;
        if needed>budget.max_bytes { return Err(error("MEMORY_LIMIT", "copper union exceeds budget")); }
        let union=crate::geometry::areas::union(active.flat_map(|(set,_,_)|set.0.iter().cloned()));
        for poly in &union.0 {
            let vertices=std::iter::once(poly.exterior()).chain(poly.interiors()).try_fold(0usize,|n,r|n.checked_add(r.0.len())).ok_or_else(||error("MEMORY_LIMIT", "metal polygon overflow"))?;
            output_bytes=output_bytes.checked_add(vertices.checked_mul(32).ok_or_else(||error("MEMORY_LIMIT", "metal polygon overflow"))?).and_then(|v|v.checked_add(256)).ok_or_else(||error("MEMORY_LIMIT", "metal polygon overflow"))?;
            if output_bytes>budget.max_bytes { return Err(error("MEMORY_LIMIT", "metal polygons exceed budget")); }
            if metals.len() >= budget.max_local_terms { return Err(error("MESH_LIMIT", "field metal count")); }
            metals.push(Metal { source_id:format!("physical:{}",metals.len()),polygon_m:convert(poly.exterior()),holes_m:poly.interiors().iter().map(&convert).collect(),z0_m:slab[0],z1_m:slab[1] });
        }
    }
    let dielectric=vec![Dielectric { source_id:"explicit-air-domain".into(),z0_m:z[0],z1_m:*z.last().unwrap(),epsilon_r:1. }];
    let mut regions=Vec::new(); let mut base=0.;
    for layer in &request.stack {
        let end=base+layer.thickness_m;
        if !layer.copper {
            let mut domain=crate::geometry::areas::difference(&board.field_substrate,&board.field_bores);
            for (span,bore) in &board.field_partial_bores { if span[0]<end && span[1]>base { if span[0]>base || span[1]<end { return Err(error("PLANAR_TRACE", "bore crosses dielectric interval boundary")); } domain=crate::geometry::areas::difference(&domain,bore); } }
            for poly in &domain.0 { let vertices=std::iter::once(poly.exterior()).chain(poly.interiors()).try_fold(0usize,|n,r|n.checked_add(r.0.len())).ok_or_else(||error("MEMORY_LIMIT", "dielectric polygon overflow"))?;
                output_bytes=output_bytes.checked_add(vertices.checked_mul(32).ok_or_else(||error("MEMORY_LIMIT", "dielectric polygon overflow"))?).and_then(|v|v.checked_add(256)).ok_or_else(||error("MEMORY_LIMIT", "dielectric polygon overflow"))?;
                if output_bytes>budget.max_bytes { return Err(error("MEMORY_LIMIT", "dielectric polygons exceed budget")); }
                regions.push(SpatialDielectric { z0_m:base,z1_m:end,epsilon_r:layer.epsilon_r.ok_or_else(||error("MATERIAL", "missing epsilon"))?,polygon_m:convert(poly.exterior()),holes_m:poly.interiors().iter().map(&convert).collect() }); }
        }
        base=end;
    }
    let planes=vec![0;z.len()];
    Ok((Input { meshes:vec![PlaneInput::Exact { source_id:"board-overlay".into(),xy_m:xy,triangles:board.planar_triangles.clone() }], planes,z_m:z,xy_bounds_m:bounds,metals,dielectrics:dielectric,exterior:Exterior::InfinityBox },regions))
}

#[cfg(test)]
mod board_adapter_tests {
    use super::*;
    use super::super::{board_mesh::{self, Electrode, Request}, mixed_conductor, AllocationBudget};
    use std::time::Duration;
    #[test]
    fn prepared_fr4_via_board_builds_owned_spatial_trace() {
        let board=crate::import_json(include_str!("../../../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-board.json")).board.unwrap();
        let mut request:Request=serde_json::from_str(include_str!("../../../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-request-d1-m1.json")).unwrap();
        request.cut_planes.clear(); request.allow_insulated_copper_cuts=false; request.domain_polygon_mm=None;
        request.electrodes=["s0","s1","r0","r1"].into_iter().enumerate().map(|(i,name)|Electrode {name:name.into(),source_path:Some(format!("pads[{i}]")),source_paths:None,layer:if i==0 {"TOP"} else {"BOTTOM"}.into(),upper:i!=0}).collect();
        request.ports=vec![["s0".into(),"r0".into()],["s1".into(),"r1".into()]];
        let allocation=AllocationBudget {max_bytes:1<<30,max_nodes:200_000,max_tetrahedra:400_000};
        let wall=Duration::from_secs(20);
        let board_mesh=board_mesh::build(&board,&request,allocation,wall).unwrap();
        let map=mixed_conductor::prepare(&board_mesh,&request.electrodes,10_000.,allocation,wall).unwrap();
        let budget=Budget {max_nodes:200_000,max_local_terms:4_000_000,max_bytes:1<<30,max_wall:wall};
        let (input,regions)=input_from_board(&board_mesh,&request,budget).unwrap();
        assert!(!regions.is_empty());
        assert!(regions.iter().any(|r|r.epsilon_r>1.));
        let trace=trace(&input,&map,budget).unwrap();
        assert!(!trace.nodes.is_empty());
        assert_eq!(trace.potential_nodes.len(),map.potential_count());
    }
}
