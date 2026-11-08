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
    // Recreate the same slab unions in their original millimetre coordinates.
    // Mesh ownership was classified in this coordinate system; an SI
    // round-trip can move the centroid of a very narrow arrangement face.
    let mut original_polygons = Vec::with_capacity(input.metals.len());
    for slab in input.z_m.windows(2) {
        if started.elapsed() >= budget.max_wall { return Err(error("DEADLINE", "trace ownership geometry")); }
        let mid = (slab[0] + slab[1]) / 2.;
        let active = map.mesh().field_copper.iter().filter(|(_,span,_)| mid > span[0] && mid < span[1]);
        let vertices = active.clone().flat_map(|(set,_,_)|set.0.iter()).flat_map(|p|std::iter::once(p.exterior()).chain(p.interiors())).try_fold(0usize,|n,r|n.checked_add(r.0.len())).ok_or_else(|| error("MEMORY_LIMIT", "trace union overflow"))?;
        let required = reserve.checked_add(vertices.checked_mul(1024).ok_or_else(||error("MEMORY_LIMIT", "trace union overflow"))?).and_then(|v|v.checked_add(polygon_vertices.checked_mul(64)?)).ok_or_else(||error("MEMORY_LIMIT", "trace union overflow"))?;
        if required > budget.max_bytes { return Err(error("MEMORY_LIMIT", "trace union exceeds budget")); }
        let union = crate::geometry::areas::union(active.flat_map(|(set,_,_)| set.0.iter().cloned()));
        original_polygons.extend(union.0);
    }
    if original_polygons.len() != input.metals.len() { return Err(error("PLANAR_TRACE", "slab ownership geometry mismatch")); }
    // Establish ownership from physical copper tetrahedra, never coordinate alone.
    let mut owned = BTreeMap::new();
    for tetra in &mesh.copper_tetrahedra {
        if started.elapsed() >= budget.max_wall { return Err(error("DEADLINE", "trace ownership")); }
        let points = tetra.nodes.map(|i| mesh.nodes_m[i]);
        // The extrusion owns the entire planar triangle by its centroid.
        // A tetrahedron centroid is biased toward one triangle corner and can
        // land on a rounded polygon boundary for a very thin arrangement face.
        let triangle = map.mesh().planar_triangles.get(tetra.planar_triangle)
            .ok_or_else(|| error("PLANAR_TRACE", "copper planar triangle out of range"))?;
        let center = ownership_probe(triangle.map(|i| mesh.nodes_m[i]), points);
        let slab = input.z_m.get(tetra.slab..=tetra.slab + 1).ok_or_else(|| error("PLANAR_TRACE", "copper slab out of range"))?;
        let xy = triangle.map(|i| map.mesh().planar_nodes_mm[i]);
        let probe_mm = Point::new(xy.iter().map(|p|p[0]).sum::<f64>()/3.,xy.iter().map(|p|p[1]).sum::<f64>()/3.);
        let matches: Vec<_> = input.metals.iter().zip(&original_polygons).enumerate().filter(|(_, (metal,poly))| slab[0] == metal.z0_m && slab[1] == metal.z1_m && poly.intersects(&probe_mm)).map(|(i,_)|i).collect();
        if matches.len() != 1 {
            let board = map.mesh();
            let xy = triangle.map(|i| board.planar_nodes_mm[i]);
            let probe_mm = Point::new(xy.iter().map(|p|p[0]).sum::<f64>()/3.,xy.iter().map(|p|p[1]).sum::<f64>()/3.);
            let original: Vec<_> = board.field_copper.iter().enumerate().filter(|(_, (poly, span, _))| slab[0] >= span[0] && slab[1] <= span[1] && poly.intersects(&probe_mm)).map(|(i,_)|i).collect();
            return Err(error("PLANAR_TRACE", &format!("ambiguous or missing copper solid ownership: probe={center:?} slab={} triangle={} matches={matches:?} original={original:?}", tetra.slab, tetra.planar_triangle)));
        }
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

fn ownership_probe(triangle: [[f64;3];3], tetra: [[f64;3];4]) -> [f64;3] {
    [triangle.iter().map(|p| p[0]).sum::<f64>() / 3.,
     triangle.iter().map(|p| p[1]).sum::<f64>() / 3.,
     tetra.iter().map(|p| p[2]).sum::<f64>() / 4.]
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
    #[test] fn ownership_probe_uses_extrusion_triangle_centroid() {
        let tri = [[0.,0.,0.],[3.,0.,0.],[0.,3.,0.]];
        let tet = [[0.,0.,0.],[3.,0.,0.],[0.,0.,1.],[0.,3.,0.]];
        assert_eq!(ownership_probe(tri, tet), [1.,1.,0.25]);
    }
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
pub(super) struct OriginalGeometry { polygons: Vec<Polygon<f64>>, bytes: usize }
pub(super) fn input_from_board(board: &super::board_mesh::BoardMesh, request: &super::board_mesh::Request, budget: Budget) -> Result<(Input, Vec<super::layered_capacitance::SpatialDielectric>, OriginalGeometry)> {
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
    let mut metals=Vec::new(); let mut originals=Vec::new(); let mut original_bytes=0usize;
    let mut output_bytes=planned;
    // Partition at existing z planes, unioning overlapping foil/barrel supports
    // in each slab. This matches the original single-owner copper mesh.
    for slab in z.windows(2) {
        if started.elapsed() >= budget.max_wall { return Err(error("DEADLINE", "field geometry adapter")); }
        let mid=(slab[0]+slab[1])/2.;
        let active=board.field_copper.iter().filter(|(_,span,_)|mid>span[0] && mid<span[1]);
        let vertices=active.clone().flat_map(|(set,_,_)|set.0.iter()).flat_map(|p|std::iter::once(p.exterior()).chain(p.interiors())).try_fold(0usize,|n,r|n.checked_add(r.0.len())).ok_or_else(||error("MEMORY_LIMIT", "copper polygon size overflow"))?;
        let needed=output_bytes.checked_add(vertices.checked_mul(1024).ok_or_else(||error("MEMORY_LIMIT", "union size overflow"))?).ok_or_else(||error("MEMORY_LIMIT", "union size overflow"))?;
        if needed>budget.max_bytes { return Err(error("MEMORY_LIMIT", "copper union exceeds budget")); }
        let union=crate::geometry::areas::union(active.flat_map(|(set,_,_)|set.0.iter().cloned()));
        let union_vertices=union.0.iter().flat_map(|p|std::iter::once(p.exterior()).chain(p.interiors())).try_fold(0usize,|v,r|v.checked_add(r.0.capacity())).ok_or_else(||error("MEMORY_LIMIT","original geometry overflow"))?;
        let union_bytes=union_vertices.checked_mul(64).and_then(|v|v.checked_add(union.0.len().checked_mul(256)?)).ok_or_else(||error("MEMORY_LIMIT","original geometry overflow"))?;
        original_bytes=original_bytes.checked_add(union_bytes).ok_or_else(||error("MEMORY_LIMIT","original geometry overflow"))?;
        output_bytes=output_bytes.checked_add(union_bytes).ok_or_else(||error("MEMORY_LIMIT","original geometry overflow"))?;
        if output_bytes>budget.max_bytes {return Err(error("MEMORY_LIMIT","original geometry exceeds budget"));}
        for poly in &union.0 {
            let vertices=std::iter::once(poly.exterior()).chain(poly.interiors()).try_fold(0usize,|n,r|n.checked_add(r.0.len())).ok_or_else(||error("MEMORY_LIMIT", "metal polygon overflow"))?;
            output_bytes=output_bytes.checked_add(vertices.checked_mul(32).ok_or_else(||error("MEMORY_LIMIT", "metal polygon overflow"))?).and_then(|v|v.checked_add(256)).ok_or_else(||error("MEMORY_LIMIT", "metal polygon overflow"))?;
            if output_bytes>budget.max_bytes { return Err(error("MEMORY_LIMIT", "metal polygons exceed budget")); }
            if metals.len() >= budget.max_local_terms { return Err(error("MESH_LIMIT", "field metal count")); }
            metals.push(Metal { source_id:format!("physical:{}",metals.len()),polygon_m:convert(poly.exterior()),holes_m:poly.interiors().iter().map(&convert).collect(),z0_m:slab[0],z1_m:slab[1] });
        }
        originals.extend(union.0);
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
    Ok((Input { meshes:vec![PlaneInput::Exact { source_id:"board-overlay".into(),xy_m:xy,triangles:board.planar_triangles.clone() }], planes,z_m:z,xy_bounds_m:bounds,metals,dielectrics:dielectric,exterior:Exterior::InfinityBox },regions,OriginalGeometry{polygons:originals,bytes:original_bytes}))
}

#[cfg(test)]
mod board_adapter_tests {
    use super::*;
    use super::super::{board_mesh::{self, Electrode, Request}, mixed_conductor, AllocationBudget};
    use std::time::Duration;
    #[test]
    fn reference_swap_two_vias_preserves_owned_spatial_trace() {
        // Captured at the native call boundary before executing the solver.
        let args: serde_json::Value = serde_json::from_str(include_str!("../../../../../tests/fixtures/analysis-local-extraction/board-integration/reference-swap-native-args.json")).unwrap();
        let board = crate::import_json(args[0].as_str().unwrap()).board.unwrap();
        let request: Request = serde_json::from_value(args[1].clone()).unwrap();
        let allocation = AllocationBudget { max_bytes:512*1024*1024, max_nodes:100_000, max_tetrahedra:200_000 };
        let wall = Duration::from_secs(20);
        let profile = mixed_conductor::CheckedOperator::HybridPlanarV3.skin_profile(1e6,Some(1e6),request.max_vertical_step_m).unwrap().unwrap().with_thickness_screen([58e6]).unwrap();
        let board_mesh = board_mesh::build_profiled(&board,&request,allocation,wall,None,&[],Some(profile)).unwrap();
        let map = mixed_conductor::prepare(&board_mesh,&request.electrodes,1e6,allocation,wall).unwrap();
        let budget = Budget { max_nodes:100_000, max_local_terms:2_400_000, max_bytes:512*1024*1024, max_wall:wall };
        let (input,regions,original) = input_from_board(&board_mesh,&request,budget).unwrap();
        assert!(certified_trace(&input,&original,&map,Budget{max_bytes:64,..budget}).is_err());
        let mut wrong_mesh=input.clone();
        if let PlaneInput::Exact { xy_m, .. } = &mut wrong_mesh.meshes[0] { xy_m[0][0] += 1e-6; }
        assert!(certified_trace(&wrong_mesh,&original,&map,budget).is_err());
        let (trace,certified) = certified_trace(&input,&original,&map,budget).unwrap();
        assert!(!trace.nodes.is_empty());
        let action = super::super::layered_capacitance::prepare_certified_action(&input,&trace,&regions,&certified,budget).unwrap();
        drop(action);
        let altered_input = input.clone();
        assert!(super::super::layered_capacitance::prepare_certified_action(&altered_input,&trace,&regions,&certified,budget).is_err());
        let mut missing = trace.clone(); missing.nodes.remove(0);
        assert!(super::super::layered_capacitance::prepare_certified_action(&input,&missing,&regions,&certified,budget).is_err());
        let mut unmatched = trace.clone(); let mut extra = unmatched.nodes[0].clone(); extra.position_m[0] += 100.; unmatched.nodes.push(extra);
        assert!(super::super::layered_capacitance::prepare_certified_action(&input,&unmatched,&regions,&certified,budget).is_err());
    }
    #[test]
    fn production_profiled_via_field_trace_covers_mixed_coordinates() {
        let board=crate::import_json(include_str!("../../../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-board.json")).board.unwrap();
        // Exact geometry request captured from automatic-hybrid DSL before native execution.
        let request:Request=serde_json::from_str(r#"{"roiMm":[[-0.54,-1.07],[10.54,0.6900000000000001]],"stack":[{"name":"TOP","copper":true,"thicknessM":1e-05,"conductivitySM":58000000,"epsilonR":null},{"name":"substrate","copper":false,"thicknessM":0.0002,"conductivitySM":null,"epsilonR":4},{"name":"BOTTOM","copper":true,"thicknessM":1e-05,"conductivitySM":58000000,"epsilonR":null}],"finishedHolePlatingM":2.5e-05,"barrelConductivitySM":58000000,"airMarginM":0.002,"maxTriangleAreaM2":1e-06,"maxVerticalStepM":0.002,"curveDeviationMm":0.001,"electrodes":[{"name":"foil-terminal-0","sourcePath":"pads[0]","layer":"TOP","upper":false},{"name":"foil-terminal-1","sourcePath":"pads[1]","layer":"BOTTOM","upper":true},{"name":"foil-terminal-2","sourcePath":"pads[2]","layer":"BOTTOM","upper":true},{"name":"foil-terminal-3","sourcePath":"pads[3]","layer":"BOTTOM","upper":true}],"ports":[["foil-terminal-0","foil-terminal-2"],["foil-terminal-1","foil-terminal-3"]],"acceptUnknownFillFreshness":true,"allowInsulatedCopperCuts":false,"minTriangleAngleDeg":0,"airEdgeDivisions":1,"domainPolygonMm":[[-0.54,-1.07],[10.54,-1.07],[10.54,0.6900000000000001],[-0.54,0.6900000000000001]],"cutPlanes":{}}"#).unwrap();
        let allocation=AllocationBudget{max_bytes:512*1024*1024,max_nodes:100_000,max_tetrahedra:200_000};
        let wall=Duration::from_secs(20);
        let profile=mixed_conductor::CheckedOperator::HybridPlanarV3.skin_profile(1e6,Some(1e6),request.max_vertical_step_m).unwrap().unwrap().with_thickness_screen([58e6]).unwrap();
        let board_mesh=board_mesh::build_profiled(&board,&request,allocation,wall,None,&[],Some(profile)).unwrap();
        let map=mixed_conductor::prepare(&board_mesh,&request.electrodes,1e6,allocation,wall).unwrap();
        let budget=Budget{max_nodes:100_000,max_local_terms:2_400_000,max_bytes:512*1024*1024,max_wall:wall};
        let (input,regions,_original)=input_from_board(&board_mesh,&request,budget).unwrap();
        let trace=trace(&input,&map,budget).unwrap();
        let referenced:std::collections::BTreeSet<_>=trace.nodes.iter().flat_map(|n|n.weights.iter().map(|&(i,_)|i)).collect();
        assert_eq!(referenced.len(),map.potential_count(),"every mixed mode must be referenced by a matched physical metal node");
        assert!(referenced.contains(&139));
        let action=super::super::layered_capacitance::prepare_traced_spatial_action(&input,&trace,&regions,budget).unwrap();
        let mut interior=vec![0.;map.potential_count()];interior[139]=1.;
        let charge=action.apply(&interior).unwrap();
        assert!(charge.iter().all(|&q|q==0.),"interior-only mode must have exactly zero field charge");
        drop(action);
        let mut unused=trace.clone();unused.potential_nodes.push("unreferenced-bogus".into());
        let failure=super::super::layered_capacitance::prepare_traced_spatial_action(&input,&unused,&regions,budget).err().unwrap();
        assert!(failure.contains("FIELD_TRACE") && failure.contains("unusedCount=1"),"{failure}");
        let mut unmatched=trace.clone();let mut extra=unmatched.nodes[0].clone();extra.position_m[0]+=100.;unmatched.nodes.push(extra);
        let failure=super::super::layered_capacitance::prepare_traced_spatial_action(&input,&unmatched,&regions,budget).err().unwrap();
        assert!(failure.contains("FIELD_TRACE") && !failure.contains("unmatchedCount=0"),"{failure}");
        let mut missing=trace.clone();missing.nodes.remove(0);
        let failure=super::super::layered_capacitance::prepare_traced_spatial_action(&input,&missing,&regions,budget).err().unwrap();
        assert!(failure.contains("FIELD_TRACE"),"{failure}");
    }
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
        let (input,regions,_original)=input_from_board(&board_mesh,&request,budget).unwrap();
        assert!(!regions.is_empty());
        assert!(regions.iter().any(|r|r.epsilon_r>1.));
        let trace=trace(&input,&map,budget).unwrap();
        assert!(!trace.nodes.is_empty());
        assert_eq!(trace.potential_nodes.len(),map.potential_count());
    }
}

/// Certify the exact board product mesh from the already classified copper and dielectric tetrahedra.
pub(super) fn certified_trace<'a>(input: &'a Input, original: &OriginalGeometry, map: &MixedMap<'_>, budget: Budget) -> Result<(PotentialTrace, super::layered_capacitance::CertifiedOverlay<'a>)> {
    use super::layered_capacitance::CertifiedOverlay;
    let start=Instant::now(); let board=map.mesh(); let mesh=&board.mesh;
    let (xy,triangles)=match input.meshes.as_slice() { [PlaneInput::Exact{xy_m,triangles,..}] => (xy_m,triangles), _=>return Err(error("PLANAR_TRACE","certificate requires one exact mesh")) };
    let n=board.planar_nodes_mm.len(); let nt=board.planar_triangles.len(); let slabs=input.z_m.len().checked_sub(1).ok_or_else(||error("PLANAR_TRACE","missing slabs"))?;
    if n==0 || xy.len()!=n || triangles!=&board.planar_triangles || input.planes.iter().any(|&x|x!=0) || input.planes.len()!=input.z_m.len() || mesh.nodes_m.len()!=n*input.z_m.len() { return Err(error("PLANAR_TRACE","certificate product mesh mismatch")); }
    for (k,&z) in input.z_m.iter().enumerate() { for (i,p) in xy.iter().enumerate() { if mesh.nodes_m[k*n+i]!=[p[0],p[1],z] {return Err(error("PLANAR_TRACE","certificate node mismatch"));} } }
    let count=slabs.checked_mul(nt).ok_or_else(||error("MEMORY_LIMIT","certificate size overflow"))?;
    let input_vertices=input.metals.iter().try_fold(0usize,|v,m|m.holes_m.iter().try_fold(v.checked_add(m.polygon_m.len())?,|v,r|v.checked_add(r.len()))).ok_or_else(||error("MEMORY_LIMIT","certificate input overflow"))?;
    let bytes=count.checked_mul(64).and_then(|v|v.checked_add(mesh.nodes_m.len().checked_mul(128)?)).and_then(|v|v.checked_add(mesh.copper_tetrahedra.len().checked_mul(4*64)?)).and_then(|v|v.checked_add(input.metals.len().checked_mul(256)?)).and_then(|v|v.checked_add(input_vertices.checked_mul(64)?)).ok_or_else(||error("MEMORY_LIMIT","certificate size overflow"))?;
    if bytes>budget.max_bytes || mesh.nodes_m.len()>budget.max_nodes || count>budget.max_local_terms {return Err(error("MEMORY_LIMIT","certificate exceeds budget"));}
    let retained_union=original.bytes;
    if bytes.checked_add(retained_union).is_none_or(|v|v>budget.max_bytes) {return Err(error("MEMORY_LIMIT","certificate retained geometry exceeds budget"));}
    if original.polygons.len()!=input.metals.len(){return Err(error("PLANAR_TRACE","certificate metal count mismatch"));}
    for (i,(metal,poly)) in input.metals.iter().zip(&original.polygons).enumerate() {
        let convert=|r:&LineString<f64>| r.0.iter().map(|p|[(p.x-board.origin_mm[0])*1e-3,(p.y-board.origin_mm[1])*1e-3]).collect::<Vec<_>>();
        if metal.source_id!=format!("physical:{i}") || metal.polygon_m!=convert(poly.exterior()) || metal.holes_m!=poly.interiors().iter().map(convert).collect::<Vec<_>>() {return Err(error("PLANAR_TRACE","certificate metal geometry mismatch"));}
    }
    let mut copper=vec![None;count]; let mut epsilon=vec![0.;count]; let mut seen=vec![0u8;count]; let mut node_owners=vec![Vec::<usize>::new();mesh.nodes_m.len()];
    let tetra_bit=|slab:usize,t:usize, nodes:[usize;4]| -> Result<u8> {
        let mut tri=triangles[t]; tri.sort(); let [a,b,c]=tri; let [a0,b0,c0]=[a,b,c].map(|i|slab*n+i); let [a1,b1,c1]=[a,b,c].map(|i|(slab+1)*n+i);
        let mut actual=nodes;actual.sort();
        for (i,mut expected) in [[a0,b0,c0,c1],[a0,b0,b1,c1],[a0,a1,b1,c1]].into_iter().enumerate() {expected.sort();if actual==expected{return Ok(1<<i);}}
        Err(error("PLANAR_TRACE","certificate tetra connectivity mismatch"))
    };
    for tet in &mesh.copper_tetrahedra {
        if start.elapsed()>=budget.max_wall{return Err(error("DEADLINE","certificate copper"));}
        if tet.slab>=slabs || tet.planar_triangle>=nt {return Err(error("PLANAR_TRACE","certificate copper index"));}
        let idx=tet.slab*nt+tet.planar_triangle; let tri=triangles[tet.planar_triangle].map(|i|board.planar_nodes_mm[i]);
        let p=Point::new(tri.iter().map(|p|p[0]).sum::<f64>()/3.,tri.iter().map(|p|p[1]).sum::<f64>()/3.);
        let found:Vec<_>=input.metals.iter().zip(&original.polygons).enumerate().filter(|(_, (m,poly))|m.z0_m==input.z_m[tet.slab]&&m.z1_m==input.z_m[tet.slab+1]&&poly.intersects(&p)).map(|(i,_)|i).collect();
        if found.len()!=1 || copper[idx].is_some_and(|v|v!=found[0]) {return Err(error("PLANAR_TRACE","certificate ambiguous copper owner"));}
        let bit=tetra_bit(tet.slab,tet.planar_triangle,tet.nodes)?; if seen[idx]&bit!=0{return Err(error("PLANAR_TRACE","duplicate copper tetra"));} copper[idx]=Some(found[0]); seen[idx]|=bit;
        for &node in &tet.nodes { let owners=node_owners.get_mut(node).ok_or_else(||error("PLANAR_TRACE","certificate node index"))?; if !owners.contains(&found[0]) {owners.push(found[0]);} }
    }
    for tet in &mesh.tetrahedra {
        if start.elapsed()>=budget.max_wall{return Err(error("DEADLINE","certificate dielectric"));}
        if tet.slab>=slabs || tet.planar_triangle>=nt || !tet.epsilon_r.is_finite() || tet.epsilon_r<=0. {return Err(error("PLANAR_TRACE","certificate dielectric index or material"));}
        let idx=tet.slab*nt+tet.planar_triangle; if copper[idx].is_some() || epsilon[idx]!=0. && epsilon[idx]!=tet.epsilon_r {return Err(error("PLANAR_TRACE","certificate material ownership conflict"));}
        let bit=tetra_bit(tet.slab,tet.planar_triangle,tet.nodes)?; if seen[idx]&bit!=0{return Err(error("PLANAR_TRACE","duplicate dielectric tetra"));} epsilon[idx]=tet.epsilon_r; seen[idx]|=bit;
    }
    if seen.iter().any(|&x|x!=7) {return Err(error("PLANAR_TRACE","certificate incomplete product ownership"));}
    let finite_contacts=certified_contacts(&copper,slabs,nt);
    let incidence_count=node_owners.iter().try_fold(0usize,|n,v|n.checked_add(v.len())).ok_or_else(||error("MEMORY_LIMIT","certificate node incidence overflow"))?;
    let trace_bytes=node_owners.iter().try_fold(0usize,|sum,owners|owners.iter().try_fold(sum,|v,&owner|v.checked_add(256)?.checked_add(input.metals[owner].source_id.len())?.checked_add(32))).ok_or_else(||error("MEMORY_LIMIT","certificate trace overflow"))?;
    if incidence_count>budget.max_nodes || bytes.checked_add(retained_union).and_then(|v|v.checked_add(trace_bytes)).is_none_or(|v|v>budget.max_bytes) {return Err(error("MEMORY_LIMIT","certificate trace exceeds budget"));}
    let rows=map.electric_node_trace(); let mut weights=vec![None;mesh.nodes_m.len()];
    for row in rows { let slot=weights.get_mut(row.node).ok_or_else(||error("PLANAR_TRACE","certificate trace index"))?; if slot.replace(row.weights).is_some(){return Err(error("PLANAR_TRACE","duplicate trace node"));} }
    let mut nodes=Vec::with_capacity(incidence_count); for (i,owners) in node_owners.iter().enumerate() { for &owner in owners {let row=weights[i].as_ref().ok_or_else(||error("PLANAR_TRACE","certificate trace missing"))?; if row.len()!=1 {return Err(error("PLANAR_TRACE","unexpected certificate node trace width"));} nodes.push(TraceNode{metal_source_id:input.metals[owner].source_id.clone(),position_m:mesh.nodes_m[i],weights:row.clone()});} }
    if nodes.len()>budget.max_nodes || bytes.checked_add(nodes.len().checked_mul(256).ok_or_else(||error("MEMORY_LIMIT","certificate trace overflow"))?).is_none_or(|v|v>budget.max_bytes){return Err(error("MEMORY_LIMIT","certificate trace exceeds budget"));}
    Ok((PotentialTrace{potential_nodes:(0..map.potential_count()).map(|i|format!("mixed:{i}")).collect(),nodes},CertifiedOverlay{input,triangles:nt,copper,epsilon,node_owners,finite_contacts,bytes:bytes.checked_add(retained_union).ok_or_else(||error("MEMORY_LIMIT","certificate retained geometry overflow"))?}))
}

fn certified_contacts(copper:&[Option<usize>], slabs:usize, triangles:usize) -> std::collections::BTreeSet<(usize,usize)> {
    let mut contacts=std::collections::BTreeSet::new();
    for k in 1..slabs { for t in 0..triangles {if let (Some(a),Some(b))=(copper[(k-1)*triangles+t],copper[k*triangles+t]) {if a!=b {contacts.insert((a.min(b),a.max(b)));}}} }
    contacts
}

#[cfg(test)]
mod certified_contact_tests {
    use super::*;
    #[test]
    fn point_or_edge_adjacency_does_not_create_area_contact() {
        let cells=[Some(0),None,None,Some(1)];
        assert!(certified_contacts(&cells,2,2).is_empty());
        let cells=[Some(0),None,Some(1),None];
        assert!(certified_contacts(&cells,2,2).contains(&(0,1)));
    }
}
