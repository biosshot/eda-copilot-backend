//! Independent finite-prism control of both near and nested far interpolation.
#[path="reference/local/mod.rs"] mod local_reference;
use local_reference::volume_integral;
use pcb_analyze::extraction::local::{mesh::{self,LayeredInput,Material},polyhedron::{self,PointPotential},prism::{Prism,GaussRule},far::Moments,volume_current,AllocationBudget};
use std::time::Duration;
include!("../../../tests/fixtures/analysis-local-extraction/board-integration/tetra-coefficients-reference.rs");
#[test]
fn actual_thin_prism_coefficients_match_exact_rational_vertices_in_every_orientation(){
 for &(vertices,volume,gradient)in EXACT_COEFFICIENTS{let(v,g)=mesh::tetra_coefficients(vertices.map(|p|p.map(f64::from_bits))).unwrap();assert_eq!(v.to_bits(),volume,"exact original volume must round identically in all orientations");for i in 0..4{for k in 0..3{let expected=f64::from_bits(gradient[i][k]);if expected==0.{assert_eq!(g[i][k],0.);}else{assert_eq!(g[i][k].to_bits(),gradient[i][k],"correctly rounded exact rational P1 coefficient");}}}}
}
#[test]
fn complete_eight_prism_space_matches_independent_near_and_far_reference() {
    complete_control(false,1)
}
#[test]
fn signed_surface_near_complete_space_matches_frozen_independent_reference() {
    complete_control(true,1)
}
#[test]
fn four_worker_complete_source_space_matches_single_worker_bitwise_and_frozen_reference(){complete_control(true,4)}
#[test]
fn two_worker_complete_source_space_matches_single_worker_bitwise_and_frozen_reference(){complete_control(true,2)}
#[test]
fn eight_worker_complete_source_space_matches_single_worker_bitwise_and_frozen_reference(){complete_control(true,8)}
fn complete_control(surface:bool,workers:usize) {
    let budget=AllocationBudget{max_bytes:32*1024*1024,max_nodes:1000,max_tetrahedra:1000};
    let mut xy=vec![];let mut triangles=vec![];let mut boundary=vec![];let mut prisms=vec![];
    for i in 0..8 {
        let x=if i<4 {i as f64*0.002}else{0.1+(i-4)as f64*0.002};
        let tri=[[x,0.],[x+0.001,0.],[x,0.0005]];
        let first=xy.len();xy.extend(tri);triangles.push([first,first+1,first+2]);boundary.extend([[first,first+1],[first+1,first+2],[first+2,first]]);
        prisms.push(Prism::new(format!("actual-prism-{i}"),tri,[0.,0.0005]).unwrap());
    }
    let mesh=mesh::extrude(&LayeredInput{xy_m:xy,triangles,boundary_edges:boundary,z_m:vec![0.,0.0005],materials:vec![(0..8).map(|i|Material::Copper{source_ids:vec![format!("actual-prism-{i}")]}).collect()]},budget).unwrap();
    let space=volume_current::assemble(&mesh,&vec![58e6;24],&[],budget).unwrap();
    let checkpoint=std::env::temp_dir().join(format!("p7-parallel-space-{}-{}.jsonl",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
    let operator=if workers>1{volume_integral::Operator::new_surface_checkpointed_workers(&mesh,3,32,budget,Duration::from_secs(90),&[],&checkpoint.to_string_lossy(),workers)}else if surface{volume_integral::Operator::new_surface(&mesh,3,32,budget,Duration::from_secs(90))}else{volume_integral::Operator::new(&mesh,3,32,budget,Duration::from_secs(90))}.unwrap();
    // Compare identical canonical checkpoint semantics in both constructors.
    // The uncached constructor can retain the other directed integral when
    // tree order reverses a pair; the directed gate bounds that difference.
    let single_checkpoint=checkpoint.with_extension("single.jsonl");
    let sequential=if workers>1{Some(volume_integral::Operator::new_surface_checkpointed(&mesh,3,32,budget,Duration::from_secs(90),&[],&single_checkpoint.to_string_lossy()).unwrap())}else{None};
    if let Some(single)=&sequential{assert_eq!(operator.constructor_workers,workers);assert!(operator.planned_bytes>single.planned_bytes);}
    assert!(operator.far_blocks>0,"test must exercise nested far blocks");
    let mut actual=vec![vec![0.;8];8];let mut tetra_actual=vec![vec![0.;space.cells.len()];space.cells.len()];
    for column in 0..space.cells.len() {
        let mut q=vec![0.;space.current_count()];
        q[3*column+2]=1./space.cells[column].density_per_amp_m2;
        let image=operator.apply(&space,&q).unwrap();
        if let Some(single)=&sequential{assert_eq!(image,single.apply(&space,&q).unwrap(),"every original scalar source column must be bit-identical");for axis in 0..2{let mut cartesian=q.clone();cartesian[3*column+axis]=cartesian[3*column+2];cartesian[3*column+2]=0.;assert_eq!(operator.apply(&space,&cartesian).unwrap(),single.apply(&space,&cartesian).unwrap(),"all original Cartesian source coordinates");}}
        for(i,c)in space.cells.iter().enumerate(){
            let value=image[3*i+2]/c.density_per_amp_m2;
            tetra_actual[i][column]=value;
            actual[mesh.copper_tetrahedra[c.mesh_cell].planar_triangle][mesh.copper_tetrahedra[space.cells[column].mesh_cell].planar_triangle]+=value;
        }
    }
    if workers>1{std::fs::remove_file(checkpoint).unwrap();std::fs::remove_file(single_checkpoint).unwrap();}
    eprintln!("complete-tetra-matrix {}",serde_json::json!({"tetraVerticesM":mesh.copper_tetrahedra.iter().map(|t|t.nodes.map(|n|mesh.nodes_m[n])).collect::<Vec<_>>(),"matrix":tetra_actual,"maxDirectedReciprocity":operator.max_directed_reciprocity,"nearPairs":operator.near_pairs,"farBlocks":operator.far_blocks,"plannedBytes":operator.planned_bytes}));
    // Independent5D finite-volume expectation, generated once with four fixed
    // randomized Sobol seeds. Keep its empirical uncertainty in the gate;
    // replaying this fixture never resamples or reruns the costly reference.
    let frozen:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/tetra-space-reference.json")).unwrap();
    // Compare exact source bits through JSON integers: the default serde_json
    // decimal parser is not a correctly rounded binary64 transport boundary.
    let expected_vertices:Vec<[[u64;3];4]>=serde_json::from_value(frozen["tetraVertexBits"].clone()).unwrap();
    assert_eq!(expected_vertices,mesh.copper_tetrahedra.iter().map(|t|t.nodes.map(|n|mesh.nodes_m[n].map(f64::to_bits))).collect::<Vec<_>>());
    let reference_matrix:Vec<Vec<f64>>=serde_json::from_value(frozen["matrix"].clone()).unwrap();
    let n=space.cells.len();assert_eq!(reference_matrix.len(),n);assert!(reference_matrix.iter().all(|row|row.len()==n));
    let mut factor=vec![vec![0.;n];n];
    for i in 0..n {for j in 0..=i {
        let value=reference_matrix[i][j]-(0..j).map(|k|factor[i][k]*factor[j][k]).sum::<f64>();
        factor[i][j]=if i==j{assert!(value>0.);value.sqrt()}else{value/factor[j][j]};
    }}
    let mut inverse=vec![vec![0.;n];n];
    for j in 0..n {for i in 0..n {inverse[i][j]=((if i==j{1.}else{0.})-(0..i).map(|k|factor[i][k]*inverse[k][j]).sum::<f64>())/factor[i][i];}}
    let mut norm_squared=0.;
    for i in 0..n {for j in 0..n {
        let mut error=0.;for k in 0..n {for m in 0..n {error+=inverse[i][k]*(tetra_actual[k][m]-reference_matrix[k][m])*inverse[j][m];}}
        norm_squared+=error*error;
    }}
    let uncertainty=frozen["empiricalWhitenedReferenceScreen"].as_f64().unwrap();
    assert!(uncertainty.is_finite()&&uncertainty>=0.&&uncertainty<0.01);
    eprintln!("complete24-tetra space error {}, empirical reference screen {}, sum {}",norm_squared.sqrt(),uncertainty,norm_squared.sqrt()+uncertainty);
    assert!(norm_squared.sqrt()+uncertainty<0.01,"complete independent tetra-space energy screen failed");
    let sources=prisms.iter().map(|p|PointPotential::new(p).unwrap()).collect::<Vec<_>>();
    let moments=prisms.iter().map(|p|Moments::new(p).unwrap()).collect::<Vec<_>>();
    let rule=GaussRule::new(24).unwrap();let mut reference=vec![vec![0.;8];8];let mut directed_checks=vec![];
    for i in 0..8 {for j in i..8 {
        let value=if i/4!=j/4 {
            let v=moments[i].pair(&moments[j]).unwrap();
            assert!(v.absolute_remainder_m5/v.integral_m5<1e-7);v.integral_m5
        }else{let v=polyhedron::volume_integral(&prisms[i],&sources[j],&rule).unwrap();if i!=j {directed_checks.push((i,j,1e-7*(v-polyhedron::volume_integral(&prisms[j],&sources[i],&rule).unwrap()).abs()));}v};
        reference[i][j]=1e-7*value;reference[j][i]=reference[i][j];
    }}
    for(i,j,difference)in directed_checks {assert!(difference/(reference[i][i]*reference[j][j]).sqrt()<1e-9);}
    // Whiten the entire eight-dimensional physical current space. Its Frobenius
    // norm bounds every relative quadratic-energy error, including weak modes.
    let mut l=vec![vec![0.;8];8];
    for i in 0..8 {for j in 0..=i {let v=reference[i][j]-(0..j).map(|k|l[i][k]*l[j][k]).sum::<f64>();l[i][j]=if i==j{assert!(v>0.);v.sqrt()}else{v/l[j][j]};}}
    let mut inverse=vec![vec![0.;8];8];for j in 0..8 {for i in 0..8 {inverse[i][j]=((if i==j{1.}else{0.})-(0..i).map(|k|l[i][k]*inverse[k][j]).sum::<f64>())/l[i][i];}}
    let mut norm=0.;for i in 0..8 {for j in 0..8 {
        let mut value=0.;for k in 0..8 {for m in 0..8 {value+=inverse[i][k]*(actual[k][m]-reference[k][m])*inverse[j][m];}}
        norm+=value*value;
    }}
    eprintln!("uniform-prism complete-space relative energy error bound {}",norm.sqrt());
    assert!(norm.sqrt()<0.01,"complete-space relative energy error bound {}",norm.sqrt());
    let low=AllocationBudget{max_bytes:mesh.retained_bytes().unwrap()+1,..budget};
    assert!(volume_integral::Operator::new(&mesh,3,16,low,Duration::from_secs(1)).err().unwrap().contains("MEMORY_LIMIT"));
    assert!(operator.apply(&space,&[0.]).is_err());
    assert!(operator.apply(&space,&vec![f64::NAN;space.current_count()]).is_err());
}

#[test]
fn positive_layered_target_grading_preserves_original_moments_and_reciprocity(){
 use std::time::Instant;
 let a=35e-6;let h=1.53e-3;let source=[[0.,0.,-a],[a,0.,-a],[0.,a,-a],[0.,0.,0.]];let phi=PointPotential::tetrahedron(source).unwrap().with_tighter_tetra_tolerance(1e-12).unwrap();let ds=polyhedron::tetra_self_integral(source,32).unwrap();
 for target in [[[0.,0.,0.],[a,0.,0.],[0.,a,0.],[0.,a,h]],[[0.,0.,0.],[a,0.,0.],[a,0.,h],[0.,a,h]],[[0.,0.,0.],[0.,0.,h],[a,0.,h],[0.,a,h]]]{
  let start=Instant::now();let other=PointPotential::tetrahedron(target).unwrap().with_tighter_tetra_tolerance(1e-12).unwrap();let dt=polyhedron::tetra_self_integral(target,32).unwrap();let norm=(ds*dt).sqrt();let mut previous:Option<(f64,f64)>=None;let mut accepted=false;
  for order in [8,16,32,64]{let forward=polyhedron::tetra_integral_z_graded(target,&phi,order,start,Duration::from_secs(110)).unwrap();let reverse=polyhedron::tetra_integral_z_graded(source,&other,order,start,Duration::from_secs(110)).unwrap();let discrepancy=(forward-reverse).abs()/norm;eprintln!("graded target order{order}: directed={discrepancy:e}");if let Some((a,b))=previous{if(forward-a).abs()/forward<=0.002&&(reverse-b).abs()/reverse<=0.002&&discrepancy<=1e-9{accepted=true;break;}}previous=Some((forward,reverse));}
  assert!(accepted,"unchanged layered target/source reciprocity/refinement");
 }
 assert!(phi.clone().with_tighter_tetra_tolerance(1e-9).is_err());
}

#[test]
fn bounded_positive_far_point_matches_independent_source_transform(){
 let a=35e-6;let t=[[0.,0.,0.],[a,0.,0.],[0.,a,0.],[0.,0.,a]];let p=[0.001,0.001,0.001];let source=PointPotential::tetrahedron(t).unwrap().with_tighter_tetra_tolerance(1e-12).unwrap();let value=source.at_diagnostic(p).unwrap();let direct=polyhedron::positive_tetra_potential_duffy(t,p,31).unwrap();assert!((value.value_m2-direct).abs()/direct<1e-11);assert!(value.source_relative_change<=1e-12);
}

#[test]
fn original_pair_checkpoints_replay_every_cartesian_current_and_reject_wrong_identity(){
 let budget=AllocationBudget{max_bytes:32*1024*1024,max_nodes:100,max_tetrahedra:100};
 let mesh=mesh::extrude(&LayeredInput{xy_m:vec![[0.,0.],[0.001,0.],[0.,0.001]],triangles:vec![[0,1,2]],boundary_edges:vec![[0,1],[1,2],[2,0]],z_m:vec![0.,0.001],materials:vec![vec![Material::Copper{source_ids:vec!["checkpoint-copper".into()]}]]},budget).unwrap();
 let space=volume_current::assemble(&mesh,&vec![58e6;3],&[],budget).unwrap();let dir=std::env::temp_dir().join(format!("p7-volume-checkpoint-{}",std::process::id()));std::fs::create_dir(&dir).unwrap();let a=dir.join("a.jsonl");let b=dir.join("b.jsonl");let c=dir.join("c.jsonl");let a_name=a.to_string_lossy().into_owned();let b_name=b.to_string_lossy().into_owned();let c_name=c.to_string_lossy().into_owned();
 let first=volume_integral::Operator::new_surface_checkpointed(&mesh,3,32,budget,Duration::from_secs(30),&[],&a_name).unwrap();assert_eq!(first.written_pairs,3);assert_eq!(first.cached_pairs,0);let before=std::fs::read(&a).unwrap();
 for workers in [0,9]{assert!(volume_integral::Operator::new_surface_checkpointed_workers(&mesh,3,32,budget,Duration::from_secs(30),&[],&c_name,workers).err().unwrap().contains("CPU workers"));}
 assert!(volume_integral::Operator::new_surface_checkpointed_workers(&mesh,3,32,AllocationBudget{max_bytes:1024*1024,..budget},Duration::from_secs(30),&[],&c_name,4).err().unwrap().contains("MEMORY_LIMIT"));assert!(!c.exists());
 let second=volume_integral::Operator::new_surface_checkpointed(&mesh,3,32,budget,Duration::from_secs(30),&[a_name.clone()],&b_name).unwrap();assert_eq!(second.cached_pairs,3);assert_eq!(second.written_pairs,0);
 for k in 0..4{let vector=(0..9).map(|i|((i+1)as f64*(k+1)as f64).sin()).collect::<Vec<_>>();assert_eq!(first.apply(&space,&vector).unwrap(),second.apply(&space,&vector).unwrap());}assert_eq!(std::fs::read(&a).unwrap(),before);
 assert!(volume_integral::Operator::new_surface_checkpointed(&mesh,4,32,budget,Duration::from_secs(30),&[a_name.clone()],&c_name).err().unwrap().contains("identity"));
 assert!(volume_integral::Operator::new_surface_checkpointed(&mesh,3,32,budget,Duration::from_secs(30),&[a_name.clone(),a_name.clone()],&c_name).err().unwrap().contains("duplicate"));
 let mut changed=mesh.clone();changed.nodes_m[0][0]+=1e-7;assert!(volume_integral::Operator::new_surface_checkpointed(&changed,3,32,budget,Duration::from_secs(30),&[a_name.clone()],&c_name).err().unwrap().contains("identity"));
 let rejected=dir.join("CACHE-REJECTED.json");std::fs::write(&rejected,b"{\"reason\":\"regression quarantine\"}").unwrap();
 assert!(volume_integral::Operator::new_surface_checkpointed(&mesh,3,32,budget,Duration::from_secs(30),&[a_name],&c_name).err().unwrap().contains("explicitly rejected"));
 assert!(!c.exists());std::fs::remove_file(rejected).unwrap();
 for path in [a,b]{std::fs::remove_file(path).unwrap();}std::fs::remove_dir(dir).unwrap();
}
