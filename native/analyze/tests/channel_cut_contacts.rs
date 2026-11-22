use pcb_analyze::extraction::local::{board_mesh,AllocationBudget};
use serde_json::json;
#[test]
fn physical_transverse_cut_contacts_have_actual_copper_side_area(){
 let board=pcb_analyze::import_json(include_str!("../../../tests/fixtures/analysis-segmented-coupon.json")).board.unwrap();
 let mut r:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json")).unwrap();
 r["roiMm"]=json!([[4.,-0.3],[6.,0.3]]);r["stack"][0]["thicknessM"]=json!(1e-5);r["stack"][1]["thicknessM"]=json!(1e-4);r["stack"][2]["thicknessM"]=json!(1e-5);r["airMarginM"]=json!(0.0002);
 r["electrodes"]=json!([{"name":"sl","sourcePath":"copper.fixed.tracks[0]","layer":"TOP","upper":false},{"name":"sr","sourcePath":"copper.fixed.tracks[0]","layer":"TOP","upper":false},{"name":"rl","sourcePath":"copper.fixed.tracks[1]","layer":"BOTTOM","upper":true},{"name":"rr","sourcePath":"copper.fixed.tracks[1]","layer":"BOTTOM","upper":true}]);
 r["ports"]=json!([["sl","rl"],["sr","rr"]]);r["cutPlanes"]=json!({"sl":{"axis":0,"atMm":4.},"sr":{"axis":0,"atMm":6.},"rl":{"axis":0,"atMm":4.},"rr":{"axis":0,"atMm":6.}});
 let request:board_mesh::Request=serde_json::from_value(r.clone()).unwrap();let budget=AllocationBudget{max_bytes:512*1024*1024,max_nodes:100000,max_tetrahedra:262144};let mesh=board_mesh::build(&board,&request,budget,std::time::Duration::from_secs(30)).unwrap();
 let signal=mesh.geometry.regions.iter().find(|r|r.source.path=="copper.fixed.tracks[0]").unwrap();let lo=signal.polygon.outer.iter().map(|p|p[1]).fold(f64::INFINITY,f64::min);let hi=signal.polygon.outer.iter().map(|p|p[1]).fold(f64::NEG_INFINITY,f64::max);
 for key in ["sl","sr"]{assert!((mesh.electrode_areas_m2[key]/((hi-lo)*1e-8)-1.).abs()<1e-8);}for key in ["rl","rr"]{assert!((mesh.electrode_areas_m2[key]/5e-9-1.).abs()<1e-8);}
 let mut finite=r.clone();finite["cutPlanes"]["sl"]=json!({"originMm":[4.,0.],"normalXY":[1.,0.],"spanMm":[-0.025,0.025]});
 let mesh=board_mesh::build(&board,&serde_json::from_value(finite).unwrap(),budget,std::time::Duration::from_secs(30)).unwrap();assert!((mesh.electrode_areas_m2["sl"]/5e-10-1.).abs()<2e-6);
 r["cutPlanes"]["sl"]["atMm"]=json!(5.);assert!(board_mesh::build(&board,&serde_json::from_value(r).unwrap(),budget,std::time::Duration::from_secs(30)).is_err());
}

#[test]
fn oriented_contacts_preserve_finite_copper_area_under_rotation_and_reversal(){
 for angle in [0_f64,45.,90.] {for reverse in [1.,-1.] {
 let mut board=pcb_analyze::import_json(include_str!("../../../tests/fixtures/analysis-segmented-coupon.json")).board.unwrap();
 let mut r:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json")).unwrap();
 r["roiMm"]=json!([[4.,-0.3],[6.,0.3]]);r["stack"][0]["thicknessM"]=json!(1e-5);r["stack"][1]["thicknessM"]=json!(1e-4);r["stack"][2]["thicknessM"]=json!(1e-5);r["airMarginM"]=json!(0.0002);
 r["electrodes"]=json!([{"name":"sl","sourcePath":"copper.fixed.tracks[0]","layer":"TOP","upper":false},{"name":"sr","sourcePath":"copper.fixed.tracks[0]","layer":"TOP","upper":false},{"name":"rl","sourcePath":"copper.fixed.tracks[1]","layer":"BOTTOM","upper":true},{"name":"rr","sourcePath":"copper.fixed.tracks[1]","layer":"BOTTOM","upper":true}]);
 r["ports"]=json!([["sl","rl"],["sr","rr"]]);r["cutPlanes"]=json!({"sl":{"axis":0,"atMm":4.},"sr":{"axis":0,"atMm":6.},"rl":{"axis":0,"atMm":4.},"rr":{"axis":0,"atMm":6.}});

 let (sin,cos)=angle.to_radians().sin_cos();
 let xy=|p:[f64;2]|[cos*p[0]-sin*p[1],sin*p[0]+cos*p[1]];
 let rotate=|p:&mut pcb_analyze::model::Point|{let q=xy([p.x,p.y]);p.x=q[0];p.y=q[1];};
 for p in &mut board.outline {rotate(p);}for t in &mut board.tracks {for p in &mut t.points {rotate(p);}}
 for p in &mut board.pads {rotate(&mut p.at);p.rotation_deg+=angle;}for v in &mut board.vias {rotate(&mut v.at);}
 let domain=[[4.,-0.3],[6.,-0.3],[6.,0.3],[4.,0.3]].map(xy);
 let lo=[domain.iter().map(|p|p[0]).fold(f64::INFINITY,f64::min),domain.iter().map(|p|p[1]).fold(f64::INFINITY,f64::min)];
 let hi=[domain.iter().map(|p|p[0]).fold(f64::NEG_INFINITY,f64::max),domain.iter().map(|p|p[1]).fold(f64::NEG_INFINITY,f64::max)];
 r["roiMm"]=json!([lo,hi]);r["domainPolygonMm"]=json!(domain);
 for (key,x) in [("sl",4.),("sr",6.),("rl",4.),("rr",6.)] {r["cutPlanes"][key]=json!({"originMm":xy([x,0.]),"normalXY":[reverse*cos,reverse*sin]});}
 let request:board_mesh::Request=serde_json::from_value(r).unwrap();
 let mesh=board_mesh::build(&board,&request,AllocationBudget{max_bytes:512*1024*1024,max_nodes:100000,max_tetrahedra:262144},std::time::Duration::from_secs(30)).unwrap();
 for key in ["sl","sr"] {assert!((mesh.electrode_areas_m2[key]/1e-9-1.).abs()<2e-6,"angle={angle} {key}");}
 for key in ["rl","rr"] {assert!((mesh.electrode_areas_m2[key]/5e-9-1.).abs()<2e-6,"angle={angle} {key}");}
 assert!(mesh.max_copper_area_change<=1e-8);assert!(mesh.max_contact_area_change<=1e-8);
 }}
}

#[test]
fn corner_has_distinct_incoming_and_outgoing_physical_faces(){
 let mut board=pcb_analyze::import_json(include_str!("../../../tests/fixtures/analysis-segmented-coupon.json")).board.unwrap();
 let mut r:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json")).unwrap();
 r["roiMm"]=json!([[4.,-0.3],[6.,0.3]]);r["stack"][0]["thicknessM"]=json!(1e-5);r["stack"][1]["thicknessM"]=json!(1e-4);r["stack"][2]["thicknessM"]=json!(1e-5);r["airMarginM"]=json!(0.0002);
 r["electrodes"]=json!([{"name":"sl","sourcePath":"copper.fixed.tracks[0]","layer":"TOP","upper":false},{"name":"sr","sourcePath":"copper.fixed.tracks[0]","layer":"TOP","upper":false},{"name":"rl","sourcePath":"copper.fixed.tracks[1]","layer":"BOTTOM","upper":true},{"name":"rr","sourcePath":"copper.fixed.tracks[1]","layer":"BOTTOM","upper":true}]);
 r["ports"]=json!([["sl","rl"],["sr","rr"]]);r["cutPlanes"]=json!({"sl":{"axis":0,"atMm":4.},"sr":{"axis":0,"atMm":6.},"rl":{"axis":0,"atMm":4.},"rr":{"axis":0,"atMm":6.}});

 use pcb_analyze::model::Point;
 board.outline=vec![Point{x:0.,y:-1.},Point{x:11.,y:-1.},Point{x:11.,y:3.},Point{x:0.,y:3.}];
 for t in &mut board.tracks {t.points=vec![Point{x:0.,y:0.},Point{x:5.,y:0.},Point{x:5.,y:2.}];}
 r["roiMm"]=json!([[4.,-0.3],[5.3,1.]]);r["domainPolygonMm"]=json!([[4.,-0.3],[5.3,-0.3],[5.3,1.],[4.,1.]]);
 for key in ["sl","rl"] {r["cutPlanes"][key]=json!({"originMm":[4.,0.],"normalXY":[1.,0.]});}
 for key in ["sr","rr"] {r["cutPlanes"][key]=json!({"originMm":[5.,1.],"normalXY":[0.,1.]});}
 let request:board_mesh::Request=serde_json::from_value(r).unwrap();
 let mesh=board_mesh::build(&board,&request,AllocationBudget{max_bytes:512*1024*1024,max_nodes:100000,max_tetrahedra:262144},std::time::Duration::from_secs(30)).unwrap();
 for key in ["sl","sr"] {assert!((mesh.electrode_areas_m2[key]/1e-9-1.).abs()<2e-6);}
 for key in ["rl","rr"] {assert!((mesh.electrode_areas_m2[key]/5e-9-1.).abs()<2e-6);}
}


#[test]
fn branch_and_disconnected_neighbour_retain_physical_connectivity(){
 let mut board=pcb_analyze::import_json(include_str!("../../../tests/fixtures/analysis-segmented-coupon.json")).board.unwrap();
 let mut r:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json")).unwrap();
 r["roiMm"]=json!([[4.,-0.3],[6.,0.3]]);r["stack"][0]["thicknessM"]=json!(1e-5);r["stack"][1]["thicknessM"]=json!(1e-4);r["stack"][2]["thicknessM"]=json!(1e-5);r["airMarginM"]=json!(0.0002);
 r["electrodes"]=json!([{"name":"sl","sourcePath":"copper.fixed.tracks[0]","layer":"TOP","upper":false},{"name":"sr","sourcePath":"copper.fixed.tracks[0]","layer":"TOP","upper":false},{"name":"rl","sourcePath":"copper.fixed.tracks[1]","layer":"BOTTOM","upper":true},{"name":"rr","sourcePath":"copper.fixed.tracks[1]","layer":"BOTTOM","upper":true}]);
 r["ports"]=json!([["sl","rl"],["sr","rr"]]);r["cutPlanes"]=json!({"sl":{"axis":0,"atMm":4.},"sr":{"axis":0,"atMm":6.},"rl":{"axis":0,"atMm":4.},"rr":{"axis":0,"atMm":6.}});

 use pcb_analyze::model::Point;
 let mut branch=board.tracks[0].clone();branch.source.path="branch".into();branch.source.id=Some("branch".into());branch.points=vec![Point{x:5.,y:0.},Point{x:5.,y:0.2}];
 let mut floating=board.tracks[0].clone();floating.source.path="floating".into();floating.source.id=Some("floating".into());floating.width_mm=0.02;floating.points=vec![Point{x:4.2,y:-0.1},Point{x:5.8,y:-0.1}];
 board.tracks.extend([branch,floating]);r["domainPolygonMm"]=json!([[4.,-0.3],[6.,-0.3],[6.,0.3],[4.,0.3]]);
 let request:board_mesh::Request=serde_json::from_value(r).unwrap();let budget=AllocationBudget{max_bytes:512*1024*1024,max_nodes:100000,max_tetrahedra:262144};
 let mesh=board_mesh::build(&board,&request,budget,std::time::Duration::from_secs(30)).unwrap();
 assert!(mesh.source_mapping.iter().any(|m|m.1=="branch"));assert!(mesh.source_mapping.iter().any(|m|m.1=="floating"));
 let contacts=request.electrodes.iter().map(|e|mesh.electrode_nodes(e).unwrap()).collect::<Vec<_>>();
 let currents=pcb_analyze::extraction::local::volume_current::assemble(&mesh.mesh,&mesh.conductivity_s_m,&contacts,budget).unwrap();
 assert_eq!(currents.components.len(),3,"branch stays on signal, disconnected same-net neighbour stays floating, finite return separate");
}

#[test]
fn multisource_union_is_exact_canonical_and_conserves_current(){
 for angle in [0_f64,45.,90.] {
 let mut board=pcb_analyze::import_json(include_str!("../../../tests/fixtures/analysis-segmented-coupon.json")).board.unwrap();
 let mut r:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json")).unwrap();
 r["roiMm"]=json!([[4.,-0.3],[6.,0.3]]);r["stack"][0]["thicknessM"]=json!(1e-5);r["stack"][1]["thicknessM"]=json!(1e-4);r["stack"][2]["thicknessM"]=json!(1e-5);r["airMarginM"]=json!(0.0002);
 r["electrodes"]=json!([{"name":"sl","sourcePath":"copper.fixed.tracks[0]","layer":"TOP","upper":false},{"name":"sr","sourcePath":"copper.fixed.tracks[0]","layer":"TOP","upper":false},{"name":"rl","sourcePath":"copper.fixed.tracks[1]","layer":"BOTTOM","upper":true},{"name":"rr","sourcePath":"copper.fixed.tracks[1]","layer":"BOTTOM","upper":true}]);
 r["ports"]=json!([["sl","rl"],["sr","rr"]]);r["cutPlanes"]=json!({"sl":{"axis":0,"atMm":4.},"sr":{"axis":0,"atMm":6.},"rl":{"axis":0,"atMm":4.},"rr":{"axis":0,"atMm":6.}});

 let mut second=board.tracks[0].clone();second.source.path="overlap".into();second.source.id=Some("overlap".into());for p in &mut second.points{p.y+=0.04;}board.tracks.push(second);
 r["domainPolygonMm"]=json!([[4.,-0.3],[6.,-0.3],[6.,0.3],[4.,0.3]]);
 for i in [0,1] {r["electrodes"][i].as_object_mut().unwrap().remove("sourcePath");r["electrodes"][i]["sourcePaths"]=json!(["overlap","copper.fixed.tracks[0]","overlap"]);}
 let (sin,cos)=angle.to_radians().sin_cos();let xy=|p:[f64;2]|[cos*p[0]-sin*p[1],sin*p[0]+cos*p[1]];
 let rotate=|p:&mut pcb_analyze::model::Point|{let q=xy([p.x,p.y]);p.x=q[0];p.y=q[1];};
 for p in &mut board.outline{rotate(p);}for t in &mut board.tracks{for p in &mut t.points{rotate(p);}}for p in &mut board.pads{rotate(&mut p.at);p.rotation_deg+=angle;}for v in &mut board.vias{rotate(&mut v.at);}
 let domain=[[4.,-0.3],[6.,-0.3],[6.,0.3],[4.,0.3]].map(xy);
 let lo=[domain.iter().map(|p|p[0]).fold(f64::INFINITY,f64::min),domain.iter().map(|p|p[1]).fold(f64::INFINITY,f64::min)];let hi=[domain.iter().map(|p|p[0]).fold(f64::NEG_INFINITY,f64::max),domain.iter().map(|p|p[1]).fold(f64::NEG_INFINITY,f64::max)];
 r["roiMm"]=json!([lo,hi]);r["domainPolygonMm"]=json!(domain);
 for (key,x) in [("sl",4.),("sr",6.),("rl",4.),("rr",6.)]{r["cutPlanes"][key]=json!({"originMm":xy([x,0.]),"normalXY":[cos,sin],"spanMm":[-0.15,0.15]});}
 // The finite return is wider than this signal span.
 for key in ["rl","rr"]{r["cutPlanes"][key].as_object_mut().unwrap().remove("spanMm");}
 let request:board_mesh::Request=serde_json::from_value(r.clone()).unwrap();
 let mut reversed=request.clone();for e in &mut reversed.electrodes {if let Some(paths)=&mut e.source_paths{paths.sort();paths.dedup();paths.reverse();}}
 assert_eq!(board_mesh::physical_signature(&request),board_mesh::physical_signature(&reversed));
 let budget=AllocationBudget{max_bytes:512*1024*1024,max_nodes:100000,max_tetrahedra:262144};
 let mesh=board_mesh::build(&board,&request,budget,std::time::Duration::from_secs(30)).unwrap();
 for key in ["sl","sr"] {assert!((mesh.electrode_areas_m2[key]/1.4e-9-1.).abs()<2e-6,"union must not count overlap twice");}
 let contacts=reversed.electrodes.iter().map(|e|mesh.electrode_nodes(e).unwrap()).collect::<Vec<_>>();
 let currents=pcb_analyze::extraction::local::volume_current::assemble(&mesh.mesh,&mesh.conductivity_s_m,&contacts,budget).unwrap();
 let a=currents.node_potential[&contacts[0][0]];let b=currents.node_potential[&contacts[1][0]];
 let mut injection=vec![0.;currents.potential_count()];injection[a]=1.;injection[b]=-1.;
 let volts=currents.dc_factor(&[],budget).unwrap().solve(&currents,&vec![0.;currents.potential_count()],&injection).unwrap();
 let actual=currents.dc_apply(&volts).unwrap();assert!((actual[a]-1.).abs()<1e-8);assert!((actual[b]+1.).abs()<1e-8);assert!(actual.iter().sum::<f64>().abs()<1e-8);
 let expected=0.002/(request.stack[0].conductivity_s_m.unwrap()*0.00014*0.00001);
 assert!(((volts[a]-volts[b])/expected-1.).abs()<2e-6,"uniform exact union resistance");
 // A real bore inside the union remains a void, rather than being filled
 // by the second source. This is a geometry/continuity control, not a second
 // analytic solid-bar resistance comparison.
 let mut bored=board.clone();let centre=xy([5.,0.]);
 bored.drills=Some(vec![pcb_analyze::model::IndependentDrill{source:pcb_analyze::model::Source{path:"holes[0]".into(),id:Some("npth".into())},at:pcb_analyze::model::Point{x:centre[0],y:centre[1]},drill:pcb_analyze::model::Drill{shape:pcb_analyze::model::DrillShape::Round,diameter_mm:0.04,offset:None,rotation_deg:None,plated:false}}]);
 let bored_mesh=board_mesh::build(&bored,&request,budget,std::time::Duration::from_secs(30)).unwrap();
 assert_eq!(bored_mesh.geometry.holes.len(),1);assert!(bored_mesh.barrel_ids.is_empty());
 assert!(bored_mesh.mesh.copper_volume_m3<mesh.mesh.copper_volume_m3);
 for key in ["sl","sr"]{assert!((bored_mesh.electrode_areas_m2[key]/mesh.electrode_areas_m2[key]-1.).abs()<1e-8);}
 // Every identity must exist on this layer and touch this finite face.
 let mut bad=r.clone();bad["electrodes"][0]["sourcePaths"]=json!(["copper.fixed.tracks[0]","copper.fixed.tracks[1]"]);
 assert!(board_mesh::build(&board,&serde_json::from_value(bad).unwrap(),budget,std::time::Duration::from_secs(30)).err().unwrap().contains("absent"));
 // Same-net, disconnected source is never turned into an equipotential union.
 for p in &mut board.tracks.last_mut().unwrap().points{p.x-=sin*0.16;p.y+=cos*0.16;}
 assert!(board_mesh::build(&board,&request,budget,std::time::Duration::from_secs(30)).is_err());
}
}

#[test]
fn rotated_endpoint_preserves_original_foil_area_through_shared_arrangement(){
 let board=pcb_analyze::import_json(include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/rotated45-end-contact-board.json")).board.unwrap();
 for raw in [include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/rotated45-end-contact-request.json"),include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/rotated45-end-contact-request-d1-m2.json"),include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/rotated45-end-contact-request-d2-m1.json"),include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/rotated45-end-contact-request-d2-m2.json")] {
 let request:board_mesh::Request=serde_json::from_str(raw).unwrap();
 let budget=AllocationBudget{max_bytes:512*1024*1024,max_nodes:100000,max_tetrahedra:262144};
 let mesh=board_mesh::build(&board,&request,budget,std::time::Duration::from_secs(30)).unwrap();
 let source=mesh.geometry.regions.iter().find(|r|r.source.path=="pads[3]"&&r.layer=="BOTTOM").unwrap();
 // Raw prepared area precedes the documented snapshot coordinate grid.
 // Compare it using that physical transport bound; the independent unchanged
 // 1e-8 pre-arrangement contact guard is asserted separately below.
 let perimeter=source.polygon.outer.windows(2).map(|e|(e[1][0]-e[0][0]).hypot(e[1][1]-e[0][1])).sum::<f64>();
 assert!((mesh.electrode_areas_m2["terminal-3"]-source.area_mm2*1e-6).abs()<=2.*mesh.geometry.boolean_error_bound_mm*perimeter*1e-6);
 assert!(mesh.max_contact_area_change<=1e-8);assert!(mesh.max_copper_area_change<=1e-8);
}
}

#[test]
fn via_annulus_conserves_source_area_on_common_arrangement(){
 let board=pcb_analyze::import_json(include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-board.json")).board.unwrap();
 for raw in [include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-request-d1-m1.json"),include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-request-d1-m2.json"),include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-request-d2-m1.json"),include_str!("../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-request-d2-m2.json")] {
 let request:board_mesh::Request=serde_json::from_str(raw).unwrap();
 let mesh=board_mesh::build(&board,&request,AllocationBudget{max_bytes:1024*1024*1024,max_nodes:100000,max_tetrahedra:262144},std::time::Duration::from_secs(30)).unwrap();
 assert_eq!(mesh.barrel_ids.len(),1);assert_eq!(mesh.geometry.holes.len(),1);
 assert!(mesh.max_copper_area_change<=1e-8);assert!(mesh.max_contact_area_change<=1e-8);
 let contacts=request.electrodes.iter().map(|e|mesh.electrode_nodes(e).unwrap()).collect::<Vec<_>>();
 let budget=AllocationBudget{max_bytes:1024*1024*1024,max_nodes:100000,max_tetrahedra:262144};
 let currents=pcb_analyze::extraction::local::volume_current::assemble(&mesh.mesh,&mesh.conductivity_s_m,&contacts,budget).unwrap();
 assert_eq!(currents.components.len(),if request.roi_mm[1][0]-request.roi_mm[0][0]>5.{6}else{2},"wide domain has three separated islands, compact domain only central via; each keeps finite return separate");
 // This exact local excludes both uniform run corridors. Only the central
 // incoming TOP / outgoing BOTTOM contacts should join through its barrel.
 let central=|path:&str|request.electrodes.iter().enumerate().filter(|(_,e)|e.canonical_paths().unwrap().contains(&path.to_string())&&request.cut_planes.contains_key(&e.name)).min_by(|(_,a),(_,b)|{
    let x=|e:&board_mesh::Electrode|serde_json::to_value(&request.cut_planes[&e.name]).unwrap()["originMm"][0].as_f64().unwrap();
    (x(a)-5.).abs().total_cmp(&(x(b)-5.).abs())
 }).unwrap().0;
 let a=currents.node_potential[&contacts[central("copper.fixed.tracks[0]")][0]];
 let b=currents.node_potential[&contacts[central("copper.fixed.tracks[2]")][0]];
 assert!(currents.components.iter().any(|c|c.contains(&a)&&c.contains(&b)),"barrel physically joins central signal contacts");
 let return_node=currents.node_potential[&contacts[1][0]];
 assert!(!currents.components.iter().any(|c|c.contains(&a)&&c.contains(&return_node)),"finite return is not an implicit ground weld");
 assert_ne!(a,b);
 let mut injection=vec![0.;currents.potential_count()];injection[a]=1.;injection[b]=-1.;
 let volts=currents.dc_factor(&[],budget).unwrap().solve(&currents,&vec![0.;currents.potential_count()],&injection).unwrap();
 let actual=currents.dc_apply(&volts).unwrap();assert!((actual[a]-1.).abs()<1e-8);assert!((actual[b]+1.).abs()<1e-8);assert!(volts[a]>volts[b]);
}
}
