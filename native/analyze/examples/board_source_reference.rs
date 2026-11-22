//! Independent analytic polyhedral-source/tetra-target reference packets.
//! Immutable geometry input; successful pairs are streamed before moving on.
#[path="../tests/reference/local/mod.rs"] mod local_reference;
use pcb_analyze::extraction::local::{mesh, polyhedron};
use std::{
    io::Write,
    time::{Duration, Instant},
};
#[path="board_source_reference/math.rs"]mod reference_math;
// Reference-only checked form of the existing positive target Duffy rule.
// Keep its point/weight/addition order bitwise; the crate-private target walker
// has no deadline hook and can otherwise overrun a whole packet inside order64.
fn ordinary_integral(target:[[f64;3];4],source:&polyhedron::PointPotential,order:usize,mut check:impl FnMut()->Result<(),String>)->Result<f64,String>{
 check()?;if !(2..=64).contains(&order){return Err("ordinary target order2..64 required".into());}
 let(nodes,weights)=reference_math::ordinary_rule_data(order)?;let volume=mesh::tetra_coefficients(target)?.0;let mut result:f64=0.;let mut samples=0;
 for(&u,&wu)in nodes.iter().zip(weights){for(&v,&wv)in nodes.iter().zip(weights){for(&w,&ww)in nodes.iter().zip(weights){
  samples+=1;if samples%128==0{check()?;}let p=std::array::from_fn(|i|target[0][i]+u*(target[1][i]-target[0][i])+(1.-u)*v*(target[2][i]-target[0][i])+(1.-u)*(1.-v)*w*(target[3][i]-target[0][i]));let weight=6.*volume*wu*wv*ww*(1.-u).powi(2)*(1.-v);result+=weight*source.at(p)?;
 }}}
 check()?;if result>0.&&result.is_finite(){Ok(result)}else{Err("LOCAL_TETRA_INTEGRAL_RANGE: finite positive tetra integral required".into())}
}
fn ordinary_controls(tetra:&[[[f64;3];4]])->Result<serde_json::Value,String>{
 let mut successes=0;let mut same_refusals=0;
 for(i,j)in[(0,1),(615,610),(636,645)]{let source=polyhedron::PointPotential::tetrahedron(tetra[j])?;for order in[2,4,8]{let old=polyhedron::tetra_integral(tetra[i],&source,order);let checked=ordinary_integral(tetra[i],&source,order,||Ok(()));match(old,checked){(Ok(a),Ok(b))if a.to_bits()==b.to_bits()=>successes+=1,(Err(a),Err(b))if a==b=>same_refusals+=1,_=>return Err(format!("checked ordinary rule differs on{i}/{j}/q{order}"))}}
  if ordinary_integral(tetra[i],&source,2,||Err("forced deadline".into())).err().as_deref()!=Some("forced deadline"){return Err("checked ordinary rule ignored cancellation".into());}
 }
 if successes==0{return Err("no positive ordinary control".into());}Ok(serde_json::json!({"bitwisePositiveControls":successes,"identicalNumericalRefusals":same_refusals,"forcedDeadlineRefused":true,"physicalAdmission":false}))
}
fn fast_ordinary_pair(tetra:&[[[f64;3];4]],sources:&[polyhedron::PointPotential],diagonal:&[f64],i:usize,j:usize,index:usize,clock:Instant)->Result<Option<serde_json::Value>,String>{
 let pair_clock=Instant::now();let check=||if clock.elapsed().as_secs_f64()>95.{Err("LOCAL_DEADLINE: hybrid reference packet".into())}else if pair_clock.elapsed().as_secs_f64()>2.{Err("REFERENCE_FAST_WORK_LIMIT: switch to controlled DD".into())}else{Ok(())};let mut previous:Option<(f64,f64)>=None;
 for order in[2,4,8,16,32]{let a=ordinary_integral(tetra[i],&sources[j],order,check)?;let b=ordinary_integral(tetra[j],&sources[i],order,check)?;let reciprocity=(a-b).abs()/(diagonal[i]*diagonal[j]).sqrt();if let Some((pa,pb))=previous{if(a-pa).abs()/a<=0.002&&(b-pb).abs()/b<=0.002&&reciprocity<=1e-9{return Ok(Some(serde_json::json!({"pair":index,"i":i,"j":j,"forward":a,"reverse":b,"previousForward":pa,"previousReverse":pb,"order":order,"reciprocity":reciprocity,"targetTransform":"checked-positive-F64-volume"})));}}previous=Some((a,b));}Ok(None)
}
fn read_tetra(path:&str)->Result<Vec<[[f64;3];4]>,String>{
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let r: serde_json::Value = serde_json::from_str(text.lines().next().ok_or("empty geometry")?)
        .map_err(|e| e.to_string())?;
    let r = r.get("signature").unwrap_or(&r);
    let (points, cells): (Vec<[f64; 3]>, Vec<[usize; 4]>) = if let Some(bits) = r.get("nodeBits") {
        let bits: Vec<[u64; 3]> =
            serde_json::from_value(bits.clone()).map_err(|e| e.to_string())?;
        (
            bits.into_iter().map(|p| p.map(f64::from_bits)).collect(),
            serde_json::from_value(r["copperCells"].clone()).map_err(|e| e.to_string())?,
        )
    } else {
        return Err("exact native vertex bits required for an independent source reference".into());
    };
    Ok(cells
        .iter()
        .map(|t| t.map(|n| points[n]))
        .collect::<Vec<_>>())
}
fn main() -> Result<(), String> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len()==3&&args[1]=="--ordinary-deadline-control"{println!("{}",ordinary_controls(&read_tetra(&args[2])?)?);return Ok(());}
    if args.len()==3&&args[1]=="--checkpoint-control"{println!("{}",reference_math::checkpoint_controls(&args[2])?);return Ok(());}
    if args.len()==3&&args[1]=="--dd-control"{let tetra=read_tetra(&args[2])?;println!("{}",reference_math::controls(&[tetra[636],tetra[615],tetra[645],tetra[1642]])?);return Ok(());}
    if args.len()==3&&args[1]=="--volume-audit"{println!("{}",reference_math::volume_audit(&read_tetra(&args[2])?)?);return Ok(());}
    if args.len()!=4&&!(args.len()==5&&["dd-volume","header-only","hybrid-volume"].contains(&args[4].as_str())){return Err("board_source_reference <geometry.json> <start-pair> <count> [dd-volume|hybrid-volume|header-only]".into());}
    let hybrid=args.get(4).is_some_and(|s|s=="hybrid-volume");let state_root=std::env::var("P7_REFERENCE_STATE_OUTPUT").ok();if hybrid&&(state_root.is_none()||std::env::var_os("P7_REFERENCE_STATE_INPUT").is_some()){return Err("hybrid batch needs fresh output root; resume a pending pair with dd-volume".into());}
    let tetra=read_tetra(&args[1])?;
    let start: usize = args[2].parse().map_err(|_| "invalid start")?;
    let count: usize = args[3].parse().map_err(|_| "invalid count")?;
    let total = tetra.len() * (tetra.len() + 1) / 2;
    if count == 0 || start.checked_add(count).is_none_or(|v| v > total) {
        return Err("invalid pair interval".into());
    }
    // Existing exact radial/facet self primitive; independent of tensor potential.
    let clock = Instant::now();
    let mut diagonal = vec![];
    for (i, &t) in tetra.iter().enumerate() {
        let mut previous = polyhedron::tetra_self_integral(t, 8)?;
        let mut value = None;
        for order in [16, 32, 64] {
            let next = polyhedron::tetra_self_integral(t, order)?;
            if (next - previous).abs() / next <= 1e-10 {
                value = Some(next);
                break;
            }
            previous = next;
        }
        diagonal.push(value.ok_or_else(|| format!("independent self{i} failed1e-10"))?);
    }
    let sources = tetra
        .iter()
        .map(|&t| polyhedron::PointPotential::tetrahedron(t))
        .collect::<Result<Vec<_>, _>>()?;
    let compensated=if hybrid||args.get(4).is_some_and(|s|s=="dd-volume"){Some(tetra.iter().map(|&t|reference_math::Source::new(t)).collect::<Result<Vec<_>,_>>()?)}else{None};
    println!(
        "{}",
        serde_json::json!({"kind":"analytic-full-source-pairs-v0","tetraVertexBits":tetra.iter().map(|t|t.map(|p|p.map(f64::to_bits))).collect::<Vec<_>>(),"diagonal":diagonal,"diagonalBits":diagonal.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),"start":start,"count":count,"headerOnly":args.get(4).is_some_and(|s|s=="header-only")})
    );
    std::io::stdout().flush().map_err(|e| e.to_string())?;
    if args.get(4).is_some_and(|s|s=="header-only"){return Ok(());}
    let mut index = 0;
    for i in 0..tetra.len() {
        for j in 0..=i {
            if index >= start && index < start + count {
                if clock.elapsed() > Duration::from_secs(105) {
                    return Err("reference packet deadline; prior pairs retained".into());
                }
                let mut ordinary_fallback=None;
                if hybrid&&i!=j{
                    match fast_ordinary_pair(&tetra,&sources,&diagonal,i,j,index,clock){Ok(Some(row))=>{println!("{row}");std::io::stdout().flush().map_err(|e|e.to_string())?;index+=1;continue;},Ok(None)=>ordinary_fallback=Some("F64 rule through order32 did not meet unchanged refinement/reciprocity gates".to_string()),Err(e)if e.starts_with("LOCAL_DEADLINE:")=>return Err(e),Err(e)=>ordinary_fallback=Some(e)}
                    std::env::set_var("P7_REFERENCE_STATE_OUTPUT",format!("{}-pair{index:06}",state_root.as_ref().unwrap()));
                }
                if i == j {
                    println!(
                        "{}",
                        serde_json::json!({"pair":index,"i":i,"j":j,"forward":diagonal[i],"reverse":diagonal[i],"previousForward":diagonal[i],"previousReverse":diagonal[i],"order":0,"reciprocity":0.})
                    );
                } else if let Some(source)=&compensated{
                    let norm=(diagonal[i]*diagonal[j]).sqrt();let mut previous:Option<(f64,f64)>=None;let mut accepted=None;let mut samples=vec![];
                    let check=||if clock.elapsed().as_secs_f64()>95.{Err("LOCAL_DEADLINE: independent compensated positive-volume pair".into())}else{Ok(())};
                    for order in [2,4,8,16]{let a=source[j].integral(&source[i],order,check)?;let b=source[i].integral(&source[j],order,check)?;let reciprocity=(a-b).abs()/norm;samples.push((order,a,b,reciprocity));
                        if let Some((pa,pb))=previous{if(a-pa).abs()/a<=0.002&&(b-pb).abs()/b<=0.002&&reciprocity<=1e-9{accepted=Some(serde_json::json!({"pair":index,"i":i,"j":j,"forward":a,"reverse":b,"previousForward":pa,"previousReverse":pb,"order":order,"reciprocity":reciprocity,"targetTransform":"positive-two-ended-tetra-volume-compensated-point1/r"}));break;}}previous=Some((a,b));
                    }
                    if accepted.is_none(){let(a,na,ea)=if std::env::var_os("P7_REFERENCE_STATE_OUTPUT").is_some(){source[j].adaptive_checkpoint(&source[i],1e-11*norm,"forward",check)?}else{source[j].adaptive(&source[i],1e-11*norm,check)?};eprintln!("reference adaptive pair{i}/{j} forward: leaves={na}, estimated/norm={:e}, seconds={}",ea/norm,clock.elapsed().as_secs_f64());let(b,nb,eb)=if std::env::var_os("P7_REFERENCE_STATE_OUTPUT").is_some(){source[i].adaptive_checkpoint(&source[j],1e-11*norm,"reverse",check)?}else{source[i].adaptive(&source[j],1e-11*norm,check)?};let reciprocity=(a-b).abs()/norm;let(pa,pb)=previous.unwrap();samples.push((0,a,b,reciprocity));if(a-pa).abs()/a<=0.002&&(b-pb).abs()/b<=0.002&&reciprocity<=1e-9{accepted=Some(serde_json::json!({"pair":index,"i":i,"j":j,"forward":a,"reverse":b,"previousForward":pa,"previousReverse":pb,"order":0,"reciprocity":reciprocity,"targetTransform":"positive-adaptive-tetra-volume-compensated-point1/r","adaptiveLeaves":[na,nb],"estimatedSelfNormalizedErrors":[ea/norm,eb/norm]}));}}
                    let mut row=accepted.ok_or_else(||format!("independent compensated reference pair{i}/{j} refuses unchanged gates: {samples:?}"))?;if let Some(reason)=ordinary_fallback{row["ordinaryFallback"]=serde_json::json!(reason);}println!("{row}");
                } else {
                    let mut previous: Option<(f64, f64)> = None;
                    let mut accepted = None;
                    let mut samples = vec![];
                    for order in [2, 4, 8, 16, 32, 64] {
                        let check=||if clock.elapsed().as_secs_f64()>95.{Err("LOCAL_DEADLINE: checked ordinary positive target rule".into())}else{Ok(())};
                        let forward = ordinary_integral(tetra[i], &sources[j], order,check)?;
                        let reverse = ordinary_integral(tetra[j], &sources[i], order,check)?;
                        let reciprocity =
                            (forward - reverse).abs() / (diagonal[i] * diagonal[j]).sqrt();
                        samples.push((order, forward, reverse, reciprocity));
                        if let Some((a, b)) = previous {
                            if (forward - a).abs() / forward <= 0.002
                                && (reverse - b).abs() / reverse <= 0.002
                                && reciprocity <= 1e-9
                            {
                                accepted = Some(
                                    serde_json::json!({"pair":index,"i":i,"j":j,"forward":forward,"reverse":reverse,"previousForward":a,"previousReverse":b,"order":order,"reciprocity":reciprocity}),
                                );
                                break;
                            }
                        }
                        previous = Some((forward, reverse));
                        if clock.elapsed() > Duration::from_secs(95) {
                            return Err(format!(
                                "reference deadline at{i}/{j}; samples={samples:?}"
                            ));
                        }
                    }
                    if accepted.is_none() {
                        let strict_i = sources[i].clone().with_tighter_tetra_tolerance(1e-12)?;
                        let strict_j = sources[j].clone().with_tighter_tetra_tolerance(1e-12)?;
                        let norm = (diagonal[i] * diagonal[j]).sqrt();
                        let mut last: Option<(f64, f64)> = None;
                        for order in [8, 16, 32, 64] {
                            let a = polyhedron::tetra_integral_z_graded(
                                tetra[i],
                                &strict_j,
                                order,
                                clock,
                                Duration::from_secs(95),
                            )?;
                            let b = polyhedron::tetra_integral_z_graded(
                                tetra[j],
                                &strict_i,
                                order,
                                clock,
                                Duration::from_secs(95),
                            )?;
                            let reciprocity = (a - b).abs() / norm;
                            if let Some((old_a, old_b)) = last {
                                if (a - old_a).abs() / a <= 0.002
                                    && (b - old_b).abs() / b <= 0.002
                                    && reciprocity <= 1e-9
                                {
                                    accepted = Some(
                                        serde_json::json!({"pair":index,"i":i,"j":j,"forward":a,"reverse":b,"previousForward":old_a,"previousReverse":old_b,"order":order,"targetTransform":"positive-two-ended-z-power4","reciprocity":reciprocity}),
                                    );
                                    break;
                                }
                            }
                            last = Some((a, b));
                        }
                        if accepted.is_some() {
                            println!("{}", accepted.unwrap());
                            std::io::stdout().flush().map_err(|e| e.to_string())?;
                            index += 1;
                            continue;
                        }
                        let a = local_reference::volume_integral::adaptive_integral_with_budget(
                            tetra[i],
                            mesh::tetra_coefficients(tetra[i])?.0,
                            &strict_j,
                            1e-11 * norm,
                            clock,
                            Duration::from_secs(95),16384,
                            pcb_analyze::extraction::local::AllocationBudget{max_bytes:16*1024*1024,max_nodes:100000,max_tetrahedra:200000},
                        )?;
                        let b = local_reference::volume_integral::adaptive_integral_with_budget(
                            tetra[j],
                            mesh::tetra_coefficients(tetra[j])?.0,
                            &strict_i,
                            1e-11 * norm,
                            clock,
                            Duration::from_secs(95),16384,
                            pcb_analyze::extraction::local::AllocationBudget{max_bytes:16*1024*1024,max_nodes:100000,max_tetrahedra:200000},
                        )?;
                        let reciprocity = (a - b).abs() / norm;
                        let (old_a, old_b) = previous.unwrap();
                        if (a - old_a).abs() / a <= 0.002
                            && (b - old_b).abs() / b <= 0.002
                            && reciprocity <= 1e-9
                        {
                            accepted = Some(
                                serde_json::json!({"pair":index,"i":i,"j":j,"forward":a,"reverse":b,"previousForward":old_a,"previousReverse":old_b,"order":0,"reciprocity":reciprocity}),
                            );
                        }
                    }
                    println!("{}",accepted.ok_or_else(||format!("reference pair{i}/{j} refuses0.2% refinement/1e-9 reciprocity; samples={samples:?}"))?);
                }
                std::io::stdout().flush().map_err(|e| e.to_string())?;
                if index % 100 == 0 {
                    eprintln!(
                        "analytic reference pair{index}/{total}; elapsed {:.3}s",
                        clock.elapsed().as_secs_f64()
                    );
                }
            }
            index += 1;
        }
    }
    let _ = mesh::tetra_coefficients(tetra[0])?;
    Ok(())
}
