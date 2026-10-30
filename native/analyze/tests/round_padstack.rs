use pcb_analyze::extraction::local::{board_mesh,volume_current,AllocationBudget};
use serde_json::json;
#[test]
fn round_offset_through_pad_retains_a_real_barrel_and_npth_never_invents_one(){
 let mut source:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-segmented-coupon.json")).unwrap();
 source["pads"]=json!([{"id":"through-pad","number":"1","net":"SIGNAL","at":{"x":5.,"y":0.},"rotationDeg":90.,"layers":["TOP","BOTTOM"],"shape":{"kind":"circle","diameterMm":0.6},"hole":{"shape":"round","diameterMm":0.2,"offset":{"x":0.025,"y":0.},"rotationDeg":0.,"plated":true}}]);
 source["copper"]["fixed"]["tracks"]=json!([]);
 let mut request:serde_json::Value=serde_json::from_str(include_str!("../../../tests/fixtures/analysis-segmented-reference-request.json")).unwrap();
 request["roiMm"]=json!([[4.6,-0.4],[5.4,0.4]]);request["airMarginM"]=json!(0.0002);request["electrodes"]=json!([{"name":"top","sourcePath":"pads[0]","layer":"TOP","upper":false},{"name":"bottom","sourcePath":"pads[0]","layer":"BOTTOM","upper":true}]);request["ports"]=json!([["top","bottom"]]);
 let r:board_mesh::Request=serde_json::from_value(request).unwrap();let budget=AllocationBudget{max_bytes:512*1024*1024,max_nodes:100000,max_tetrahedra:262144};
 for plated in [true,false]{source["pads"][0]["hole"]["plated"]=json!(plated);let board=pcb_analyze::import_json(&source.to_string()).board.unwrap();let mesh=board_mesh::build(&board,&r,budget,std::time::Duration::from_secs(30)).unwrap_or_else(|e|panic!("{e}; plated={plated}; geometry={}",serde_json::to_string(&pcb_analyze::geometry::prepare(&board,Default::default())).unwrap()));
  assert_eq!(mesh.barrel_ids.len(),usize::from(plated));assert_eq!(mesh.geometry.holes.len(),1);assert!(mesh.mesh.copper_volume_m3>0.);
  let electrodes=r.electrodes.iter().map(|e|mesh.electrode_nodes(e).unwrap()).collect::<Vec<_>>();let currents=volume_current::assemble(&mesh.mesh,&mesh.conductivity_s_m,&electrodes,budget).unwrap();let a=currents.node_potential[&electrodes[0][0]];let b=currents.node_potential[&electrodes[1][0]];
  assert_eq!(currents.components.iter().any(|c|c.contains(&a)&&c.contains(&b)),plated,"drill plating, never a net label, controls the physical layer connection");
  if plated{let mut injection=vec![0.;currents.potential_count()];injection[a]=1.;injection[b]=-1.;let v=currents.dc_factor(&[],budget).unwrap().solve(&currents,&vec![0.;currents.potential_count()],&injection).unwrap();assert!(v[a]>v[b]);}
 }
}
