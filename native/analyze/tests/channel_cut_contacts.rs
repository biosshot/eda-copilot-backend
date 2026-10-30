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
 r["cutPlanes"]["sl"]["atMm"]=json!(5.);assert!(board_mesh::build(&board,&serde_json::from_value(r).unwrap(),budget,std::time::Duration::from_secs(30)).is_err());
}
