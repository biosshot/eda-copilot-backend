//! Independent-control geometry for the sparse Newton candidate, one bounded
//! mesh/domain configuration per process. No PCB is represented by this cube.
#[path="../tests/reference/local/mod.rs"] mod local_reference;
use local_reference::volume_integral;
use pcb_analyze::extraction::local::{mesh::{self,LayeredInput,Material},volume_current,volume_magnetic,AllocationBudget};
fn main()->Result<(),String> {
    let start=std::time::Instant::now();let args:Vec<_>=std::env::args().collect();
    if !(3..=7).contains(&args.len()) {return Err("volume_magnetic_reference <cube-divisions> <air-margin/cube-side> [exterior-quadrature-order] [null|uniform] [integral-interpolation-order] [integral-near-order]".into());}
    let n:usize=args[1].parse().map_err(|_|"invalid divisions")?;let margin:f64=args[2].parse().map_err(|_|"invalid margin")?;
    if !(2..=32).contains(&n)||!margin.is_finite()||margin<=0. {return Err("invalid explicit mesh/domain".into());}
    let l=1e-3;let h=l/n as f64;let mut distances=vec![];let mut d=h.min(l*margin);
    loop {distances.push(d);if d==l*margin {break;}d=(2.*d).min(l*margin);}
    let mut grid:Vec<_>=distances.iter().rev().map(|d|-d).collect();grid.extend((0..=n).map(|i|l*i as f64/n as f64));grid.extend(distances.iter().map(|d|l+d));
    let m=grid.len();let xy_m:Vec<_>=grid.iter().flat_map(|&y|grid.iter().map(move|&x|[x,y])).collect();
    let mut triangles=vec![];let mut inside=vec![];
    for j in 0..m-1 {for i in 0..m-1 {let a=j*m+i;triangles.extend([[a,a+1,a+m+1],[a,a+m+1,a+m]]);let cu=grid[i]>=0.&&grid[i+1]<=l&&grid[j]>=0.&&grid[j+1]<=l;inside.extend([cu,cu]);}}
    let mut boundary_edges=vec![];for i in 0..m-1 {boundary_edges.extend([[i,i+1],[(m-1)*m+i,(m-1)*m+i+1],[i*m,(i+1)*m],[i*m+m-1,(i+1)*m+m-1]]);}
    let materials=grid.windows(2).map(|z|inside.iter().map(|&cu|if cu&&z[0]>=0.&&z[1]<=l {Material::Copper{source_ids:vec!["reference-cube".into()]}}else{Material::Dielectric{label:"air".into(),epsilon_r:1.}}).collect()).collect();
    let budget=AllocationBudget{max_bytes:512*1024*1024,max_nodes:100000,max_tetrahedra:200000};
    let mesh=mesh::extrude(&LayeredInput{xy_m,triangles,boundary_edges,z_m:grid,materials},budget)?;
    let space=volume_current::assemble(&mesh,&vec![58e6;mesh.copper_tetrahedra.len()],&[],budget)?;
    let order:usize=if args.len()>=4 {args[3].parse().map_err(|_|"invalid exterior order")?}else{0};
    let null_control=args.get(4).is_some_and(|s|s=="null");
    if args.len()>=5&&!null_control&&args[4]!="uniform" {return Err("unknown reference current control".into());}
    let mut q=vec![0.;space.current_count()];for(i,c)in space.cells.iter().enumerate(){
        q[3*i+2]=if null_control {if i<6 {(if i%2==0 {1.}else{-1.})/c.density_per_amp_m2}else{0.}}else{1./(l*l*c.density_per_amp_m2)};
    }
    let (image,planned_bytes,exterior_radius,integral_metadata)=if args.len()>=6 {
        let p:usize=args[5].parse().map_err(|_|"invalid interpolation order")?;
        let near:usize=args.get(6).map(|s|s.parse()).transpose().map_err(|_|"invalid near order")?.unwrap_or(8);
        let operator=volume_integral::Operator::new(&mesh,p,near,budget,std::time::Duration::from_secs(110))?;
        (operator.apply(&space,&q)?,operator.planned_bytes,None,serde_json::json!({"interpolationOrder":p,"nearOrder":near,"nearPairs":operator.near_pairs,"farBlocks":operator.far_blocks,"maxDirectedReciprocity":operator.max_directed_reciprocity}))
    }else{
        let operator=if order==0 {volume_magnetic::Operator::new(&mesh,budget)?}else{volume_magnetic::Operator::new_enriched(&mesh,budget,order)?};
        (operator.apply(&space,&q)?,operator.planned_bytes,operator.enrichment_radius_m,serde_json::Value::Null)
    };
    let value=q.iter().zip(&image).map(|(a,b)|a*b).sum::<f64>();
    println!("{}",serde_json::json!({"cubeDivisions":n,"airMarginPerSide":margin,"exteriorQuadratureOrder":order,"exteriorRadiusM":exterior_radius,"integral":integral_metadata,"inductanceH":if null_control{None}else{Some(value)},"control":if null_control{"alternating-ordering-tetra-current"}else{"uniform-cube"},"activeCubeSideM":if null_control{h}else{l},"quadraticMagneticEnergyTimesTwoJ":value,"nodes":mesh.nodes_m.len(),"tetrahedra":mesh.tetrahedra.len()+mesh.copper_tetrahedra.len(),"currentCells":space.cells.len(),"plannedBytes":planned_bytes,"elapsedSeconds":start.elapsed().as_secs_f64()}));Ok(())
}
