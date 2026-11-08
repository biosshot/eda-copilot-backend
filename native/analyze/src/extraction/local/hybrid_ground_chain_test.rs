//! Three real copper islands with two explicitly checked finite bridges.
use super::*;
use super::super::checked_packages;
use serde_json::{json,Value};

fn resistor(id:&str,a:&str,b:&str,r:f64)->checked_packages::Attachment {
    serde_json::from_value(json!({"id":id,"electrodes":[a,b],"model":{"id":id,"kind":"network","ports":["terminals"],"data":{"format":"coupled-rlc","referenceTopology":"infinity","potentialNodes":["positive","negative"],"currentDofs":["series"],"incidence":[[0,0,1.],[1,0,-1.]],"portIncidence":[[0,0,1.],[1,0,-1.]],"resistanceOhm":[[0,0,r]],"inductanceH":[],"capacitanceF":[],"structuralGaugeModes":[[1.,1.]],"conservedPotentialModes":[],"portTerminals":[{"positive":{"node":"positive"},"negative":{"node":"negative"}}]},"validity":{"status":"conditional","domain":{"bandHz":[900000.,1100000.]}},"analyses":["networkParameters"]}})).unwrap()
}
#[test]
fn three_physical_ground_islands_two_checked_bridges() {
    let mut value:Value=serde_json::from_str(include_str!("../../../../../tests/fixtures/analysis-segmented-coupon.json")).unwrap();
    value["outline"]=json!([{"x":-1.,"y":-1.},{"x":1.,"y":-1.},{"x":1.,"y":1.},{"x":-1.,"y":1.}]);
    value["nets"]=json!([{"name":"AGND"},{"name":"PGND"},{"name":"GND"}]);
    value["copper"]["fixed"]["tracks"]=json!([]);value["copper"]["fixed"]["zones"]=json!([]);value["copper"]["fixed"]["vias"]=json!([]);
    let original=value["pads"][0].clone();value["pads"]=json!([original.clone(),original.clone(),original]);
    for (i,pad) in value["pads"].as_array_mut().unwrap().iter_mut().enumerate() {
        pad["id"]=json!(format!("ground-{i}"));pad["number"]=json!(format!("{}",i+1));pad["net"]=json!(["AGND","PGND","GND"][i]);
        pad["at"]=json!({"x":-0.6+0.6*i as f64,"y":0.});pad["layers"]=json!(["TOP"]);pad["shape"]=json!({"kind":"rect","widthMm":0.25,"heightMm":0.25});
    }
    let board=crate::import_json(&value.to_string()).board.unwrap();
    let request:board_mesh::Request=serde_json::from_value(json!({"roiMm":[[-1.,-1.],[1.,1.]],"stack":[{"name":"TOP","thicknessM":0.00001,"copper":true,"conductivitySM":58e6},{"name":"air","thicknessM":0.0001,"copper":false,"epsilonR":1.},{"name":"BOTTOM","thicknessM":0.00001,"copper":true,"conductivitySM":58e6}],"airMarginM":0.0002,"maxTriangleAreaM2":0.000002,"maxVerticalStepM":0.0001,"airEdgeDivisions":1,"minTriangleAngleDeg":0.,"curveDeviationMm":0.001,"acceptUnknownFillFreshness":true,"allowInsulatedCopperCuts":false,"electrodes":[{"name":"AGND","sourcePath":"pads[0]","layer":"TOP","upper":false},{"name":"PGND","sourcePath":"pads[1]","layer":"TOP","upper":false},{"name":"GND","sourcePath":"pads[2]","layer":"TOP","upper":false}],"ports":[["AGND","GND"],["PGND","GND"]]})).unwrap();
    let budget=AllocationBudget{max_bytes:512*1024*1024,max_nodes:10000,max_tetrahedra:10000};let wall=Duration::from_secs(25);let start=Instant::now();
    let operator=CheckedOperator::HybridPlanarV3;
    let profile=operator.skin_profile(1e6,Some(1e6),request.max_vertical_step_m).unwrap().map(|p|p.with_thickness_screen([58e6]).unwrap());
    let mesh=board_mesh::build_profiled(&board,&request,budget,wall,None,&[],profile).unwrap();
    let physical=prepare(&mesh,&request.electrodes,1e6,budget,wall).unwrap();assert_eq!(physical.full.components.len(),3);drop(physical);
    let packages=checked_packages::validate(vec![resistor("AGND-PGND","AGND","PGND",25.),resistor("PGND-GND","PGND","GND",40.)],&request,&[1e6]).unwrap();
    let one=checked_packages::validate(vec![resistor("AGND-PGND","AGND","PGND",25.)],&request,&[1e6]).unwrap();
    let grid=CheckedGrid{frequencies_hz:&[1e6],reference_ohm:&[50.,50.],max_iterations:16};
    let solve=|p:&[checked_packages::Validated]|solve_checked_mesh_impl_policy(&mesh,&request,grid,profile,p,None,PartitionPolicy::AutomaticHybridV1,operator,budget,Instant::now(),wall);
    let full=solve(&packages).unwrap();let incomplete=solve(&one).unwrap();
    assert_eq!(full["coverage"],"unverified");
    let columns=full["samples"][0]["columns"].as_array().unwrap();assert_eq!(columns.len(),2);
    for (input,column) in columns.iter().enumerate() {
        assert!(column["kclRelative"].as_f64().unwrap()<=1e-8 && column["kvlRelative"].as_f64().unwrap()<=1e-8);
        let parts=column["packages"].as_array().unwrap();assert_eq!(parts.len(),2);
        let mut total=column["copperLossW"].as_f64().unwrap();
        for (k,(id,r)) in [("AGND-PGND",25.),("PGND-GND",40.)].into_iter().enumerate() {
            assert_eq!(parts[k]["id"],id);let cur=&parts[k]["currentA"];let re=cur["real"].as_f64().unwrap();let im=cur["imaginary"].as_f64().unwrap();let loss=parts[k]["lossW"].as_f64().unwrap();
            assert!(re.hypot(im)>1e-12 && (loss-r*(re*re+im*im)).abs()<=1e-12+1e-8*loss.abs());
            let diagonal=full["samples"][0]["lossGrams"]["packages"][k]["powerWPerA2"][input][input]["real"].as_f64().unwrap();
            assert!((diagonal-loss).abs()<=1e-12+1e-8*(diagonal.abs()+loss.abs()));total+=loss;
        }
        let mut wave=12.5;for output in 0..2 {let s=&full["parameters"][0][output][input];let r=s["real"].as_f64().unwrap();let i=s["imaginary"].as_f64().unwrap();wave-=12.5*(r*r+i*i);}
        assert!((wave-total).abs()<=1e-12+1e-8*(wave.abs()+total.abs()));
    }
    assert_eq!(incomplete["samples"][0]["columns"][0]["packages"].as_array().unwrap().len(),1);
    let a=&full["parameters"][0][0][0];let b=&incomplete["parameters"][0][0][0];
    let change=(a["real"].as_f64().unwrap()-b["real"].as_f64().unwrap()).hypot(a["imaginary"].as_f64().unwrap()-b["imaginary"].as_f64().unwrap());
    assert!(change>1e-5,"removing bridge must change the actual measured network");
    assert!(start.elapsed()<wall);
}
