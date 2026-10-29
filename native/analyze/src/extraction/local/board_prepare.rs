//! Shared local direct-FEM preparation and developer verification.
//! CPU/F64, explicit physical requests, no external diagnostic process.
use super::{board_mesh,volume_current,volume_field,volume_magnetic,volume_reduction,volume_solve,AllocationBudget};
use crate::electrical::{coupled,Counters};
use super::volume_field::ElectricApply;
use super::volume_magnetic::MagneticApply;
// Original accepted current-space coupon sweep; keep the exact saved values.
pub const FREQUENCIES_HZ: [f64; 17] = [1000.0, 1333.5214321633239, 1778.2794100389228, 2371.373705661655, 3162.2776601683795, 4216.965034285822, 5623.413251903491, 7498.942093324558, 10000.0, 13335.21432163324, 17782.79410038923, 23713.737056616552, 31622.776601683792, 42169.65034285823, 56234.13251903491, 74989.42093324558, 100000.0];
fn record_stage(stages:&mut serde_json::Map<String,serde_json::Value>,name:&str,start:std::time::Instant) {
    let seconds=start.elapsed().as_secs_f64();eprintln!("P7_STAGE {name} {seconds:.6}s");
    stages.insert(name.into(),serde_json::json!(seconds));
}

/// Internal orchestration options. Developer verification is explicitly opt-in.
pub struct Options<'a> {
    pub mode: &'a str,
    pub developer_checkpoint:Option<&'a super::developer_checkpoint::Journal>,
    pub magnetic_config: [usize;4],
    pub field_request: Option<&'a board_mesh::Request>,
    pub cached_field: Option<&'a str>,
    pub frequencies_hz: &'a [f64],
}
/// Accepted electric discretization; original current mesh and contacts survive.
/// This is a v1 configuration, not a complete spatial accuracy certificate.
pub fn accepted_electric_request(r:&board_mesh::Request)->board_mesh::Request {
    let mut field=r.clone();
    field.air_margin_m=0.008;
    field.max_vertical_step_m=0.0005;
    field.interface_step_m=Some(0.000025);
    field.interface_growth=Some(2.);
    field
}
pub fn run(board:&crate::model::Board,r:&board_mesh::Request,options:Options<'_>)->Result<serde_json::Value,String> {
    run_with_budget(board,r,options,300.)
}
/// Shared cold budget remaining after planning/cache work.
pub fn run_with_budget(board:&crate::model::Board,r:&board_mesh::Request,options:Options<'_>,seconds:f64)->Result<serde_json::Value,String> {
    if !seconds.is_finite()||seconds<=0.||seconds>300. {return Err("LOCAL_DEADLINE: invalid remaining cold budget".into());}
    let started=std::time::Instant::now();
    let requested_jobs=std::env::var("PCB_ANALYSIS_TRAINING_JOBS").ok().map(|v|v.parse::<usize>().map_err(|_|"LOCAL_THREADS: training jobs must be integer1..4".to_string())).transpose()?.unwrap_or(1);
    if requested_jobs>1&&super::parallel::current()!=faer::Par::Seq{return Err("LOCAL_THREADS: select outer training jobs or native kernel parallelism; nested levels are not admitted".into());}
    super::parallel::map_jobs(requested_jobs,0,|_|Ok(()))?;
    let mut stages=serde_json::Map::new();
    let remaining=||std::time::Duration::from_secs_f64(seconds).checked_sub(started.elapsed())
        .filter(|t|!t.is_zero()).ok_or("LOCAL_DEADLINE: shared cold calculation deadline");
    let Options{mode,magnetic_config,field_request,cached_field,frequencies_hz,developer_checkpoint}=options;
    #[derive(serde::Serialize,serde::Deserialize)]
    struct Prepared {basis:volume_reduction::Basis,model:coupled::Model,training:Vec<serde_json::Value>,training_solutions:Vec<(f64,usize,volume_solve::Solution)>}
    let mut restored=match developer_checkpoint {Some(j)=>j.load::<Prepared>("prepared.json")?,None=>None};
    let resumed=restored.is_some();
    if !["geometry","field","full","train","prepare","verify","solve"].contains(&mode) {return Err("invalid volume preparation mode".into());}
    // The board builder independently reserves2048 bytes/tetra before extrusion.
    // Keep the pre-existing geometry count cap independent of the 2 GiB cap;
    // sparse field/factor/current work must still fit the remaining live bytes.
    let budget=AllocationBudget{max_bytes:2*1024*1024*1024-crate::extraction::local::parallel::stack_reservation(),max_nodes:100000,max_tetrahedra:512*1024*1024/2048};
    let stage=std::time::Instant::now();
    let b=board_mesh::build(&board,&r,budget,remaining()?)?;
    record_stage(&mut stages,"geometry",stage);
    let accepted=accepted_electric_request(r);
    let field_request=field_request.or(Some(&accepted));
    let field_b=if let Some(fr)=field_request.filter(|_|cached_field.is_none()) {
        if board_mesh::physical_signature(fr)!=board_mesh::physical_signature(r) {return Err("field refinement must preserve physical ROI, copper, bore, materials and finite contacts".into());}
        let fb=board_mesh::build(&board,&fr,AllocationBudget{max_bytes:budget.max_bytes.checked_sub(b.mesh.retained_bytes()?).ok_or("field mesh live budget exhausted")?,..budget},remaining()?)?;
        if (fb.mesh.copper_volume_m3-b.mesh.copper_volume_m3).abs()>1e-8*b.mesh.copper_volume_m3||fb.barrel_ids!=b.barrel_ids||fb.source_mapping!=b.source_mapping {return Err("independent field mesh changed physical copper volume or source mapping".into());}Some(fb)
    }else{None};
    let field_mesh=&field_b.as_ref().unwrap_or(&b).mesh;
    eprintln!("common arrangement {} atoms; copper area change {:e}, contact area change {:e}",b.arrangement_atoms,b.max_copper_area_change,b.max_contact_area_change);
    let electrodes=r.electrodes.iter().map(|e|b.electrode_nodes(e)).collect::<Result<Vec<_>,_>>()?;
    let stage=std::time::Instant::now();
    let currents=volume_current::assemble(&b.mesh,&b.conductivity_s_m,&electrodes,budget)?;
    record_stage(&mut stages,"currentAssembly",stage);
    let mut model=serde_json::Value::Null;
    if !electrodes.is_empty()&&!(mode=="geometry") {
        eprintln!("material/current mesh complete: {} potentials, {} currents",currents.potential_count(),currents.current_count());
        let contacts=r.electrodes.iter().zip(&electrodes).map(|(e,n)|(e.name.clone(),currents.node_potential[&n[0]])).collect::<Vec<_>>();
        let stage=std::time::Instant::now();
        let mut basis=match restored.as_mut(){Some(p)=>std::mem::replace(&mut p.basis,volume_reduction::harmonic(&currents,&contacts,budget)?),None=>volume_reduction::harmonic(&currents,&contacts,budget)?};
        if basis.voltage.iter().any(|v|v.len()!=currents.potential_count()||v.iter().any(|x|!x.is_finite()))||basis.current.iter().any(|v|v.len()!=currents.current_count()||v.iter().any(|x|!x.is_finite())) {return Err("developer restored basis shape/nonfinite refusal".into());}
        record_stage(&mut stages,"harmonicBasis",stage);
        eprintln!("harmonic {} voltage / {} current modes",basis.voltage.len(),basis.current.len());
        let field_budget=AllocationBudget{max_bytes:budget.max_bytes.checked_sub(currents.planned_bytes).ok_or("field live budget exhausted")?,..budget};
        // Keep the existing sparse FEM factor and solve only requested RHS.
        // ElectricApply still accepts the full original potential space.
        let stage=std::time::Instant::now();
        let (field,field_metadata):(Box<dyn ElectricApply>,serde_json::Value)=if let Some(path)=cached_field {
            let field=volume_field::DenseField::load_checkpoint(&b.mesh,&currents,&board_mesh::physical_signature(&r),path,field_budget)?;
            let metadata=serde_json::json!({"kind":"complete-original-P1-checkpoint","assemblyPeakBytes":field.assembly_peak_bytes,"retainedBytes":field.planned_bytes(),"maxChargeEnergyColumnError":field.max_charge_energy_column_error,"maxDirectedReciprocity":field.max_directed_reciprocity});
            (Box::new(field),metadata)
        }else{
            let field=if field_b.is_some(){volume_field::Field::new_vertical_refined(field_mesh,&b.mesh,&currents,field_budget)?}else{volume_field::Field::new(&b.mesh,&currents,field_budget)?};
            let metadata=serde_json::json!({"kind":"sparse-original-P1-requested-RHS","plannedBytes":field.planned_bytes(),"fullSchurMaterialized":false});
            (Box::new(field),metadata)
        };
        record_stage(&mut stages,"electricAssemblyAndFactor",stage);
        if mode=="field" {
            return Ok(serde_json::json!({"kind":"unaccepted-original-P1-field-v1","physicalAdmission":false,"potentialCount":currents.potential_count(),"currentCount":currents.current_count(),"sourceCount":currents.cells.len(),"physicalInputs":board_mesh::physical_signature(&r),"field":field_metadata,"harmonicFieldGram":field.energy_gram(&basis.voltage)?}));
        }
        let basis_bytes=(basis.voltage.len()*currents.potential_count()+basis.current.len()*currents.current_count()).checked_mul(32).ok_or("basis live plan overflow")?;
        let live=currents.planned_bytes.checked_add(field.planned_bytes()).and_then(|n|n.checked_add(basis_bytes)).ok_or("magnetic live plan overflow")?;
        let action_bytes=16*1024*1024;
        let magnetic_budget=AllocationBudget{max_bytes:budget.max_bytes.checked_sub(live).and_then(|n|n.checked_sub(super::magnetic_actions::Reuse::reservation(&currents,action_bytes).ok()?)).ok_or("magnetic live budget exhausted")?,..budget};
        let stage=std::time::Instant::now();
        let magnetic_operator=volume_magnetic::Operator::new_enriched_patch(
            &b.mesh,magnetic_budget,magnetic_config[2],magnetic_config[0],
            magnetic_config[1],magnetic_config[3])?;
        let magnetic=super::magnetic_actions::Reuse::new(&magnetic_operator,&currents,action_bytes)?;
        record_stage(&mut stages,"magneticAssemblyAndFactor",stage);
        eprintln!("magnetic factor plan {} bytes",magnetic.planned_bytes());
        let stage=std::time::Instant::now();
        let left=&basis.current[0];let right=basis.current.last().unwrap();
        let combination:Vec<_>=left.iter().zip(right).map(|(a,b)|0.75*a-0.25*b).collect();
        let al=magnetic.apply(&currents,left)?;let ar=magnetic.apply(&currents,right)?;let ac=magnetic.apply(&currents,&combination)?;
        let linearity_reference=al.iter().zip(&ar).map(|(a,b)|(0.75*a).powi(2)+(0.25*b).powi(2)).sum::<f64>().sqrt();
        let magnetic_linearity=ac.iter().zip(al.iter().zip(&ar)).map(|(c,(a,b))|(c-(0.75*a-0.25*b)).powi(2)).sum::<f64>().sqrt()/linearity_reference.max(f64::MIN_POSITIVE);
        eprintln!("magnetic numerical linearity {magnetic_linearity:e}");
        if !magnetic_linearity.is_finite()||magnetic_linearity>1e-12 {return Err(format!("magnetic numerical linearity refuses {magnetic_linearity:e}"));}
        if !resumed {for _ in 0..2 {basis.extend_magnetic(&currents,&magnetic)?;}}
        record_stage(&mut stages,"initialMagneticBasis",stage);
        let ports=r.ports.iter().map(|pair| {
            let a=r.electrodes.iter().position(|e|e.name==pair[0]).ok_or("unknown positive electrode")?;
            let b=r.electrodes.iter().position(|e|e.name==pair[1]).ok_or("unknown return electrode")?;Ok([a,b])
        }).collect::<Result<Vec<_>,String>>()?;
        let full_mode=mode=="full"||mode=="train"||mode=="prepare"||mode=="verify"||mode=="solve";
        let stage=std::time::Instant::now();
        let solver=if full_mode {Some(volume_solve::Solver::new(&currents,field.as_ref(),&magnetic,budget)?)}else{None};
        record_stage(&mut stages,"fullSolvePreparation",stage);
        let mut training=restored.as_ref().map(|p|p.training.clone()).unwrap_or_default();let mut training_solutions=restored.as_ref().map(|p|p.training_solutions.clone()).unwrap_or_default();
        let stage=std::time::Instant::now();
        if !resumed&&(mode=="train"||mode=="prepare"||mode=="verify"||mode=="solve") {
            let full_solver=solver.as_ref().unwrap();
            let tasks=[1e4,1e5].into_iter().flat_map(|f|(0..ports.len()).map(move|input|(f,input))).collect::<Vec<_>>();
            let retained_training_bytes=currents.potential_count().checked_mul(6).and_then(|n|n.checked_add(currents.current_count().checked_mul(2)?)).and_then(|n|n.checked_mul(8*tasks.len())).ok_or("training states byte plan overflow")?;
            let live=full_solver.planned_bytes.checked_add(retained_training_bytes).ok_or("training live byte plan overflow")?;
            let workspace=full_solver.additional_solve_bytes()?;
            let spare=budget.max_bytes.checked_sub(live).ok_or("training live budget exhausted")?;
            let jobs=requested_jobs.min(1+spare/workspace.max(1));
            budget.check(live+(jobs-1)*workspace)?;
            stages.insert("trainingJobs".into(),serde_json::json!({"requested":requested_jobs,"effective":jobs,"retainedTrainingStatesBytes":retained_training_bytes,"extraWorkspaceBytes":(jobs-1)*workspace,"factorCopies":0,"scratchBound":"audited GMRES/RHS/apply scratch; immutable factors shared","innerKernels":format!("{:?}",super::parallel::current())}));
            let solutions=super::parallel::map_jobs(jobs,tasks.len(),|index|{
                let (frequency,input)=tasks[index];let mut injection=vec![0.;currents.potential_count()];let[a,b]=ports[input];injection[contacts[a].1]=1.;injection[contacts[b].1]=-1.;
                full_solver.solve(frequency,&injection,&vec![0.;injection.len()],remaining()?)
            })?;
            for ((frequency,input),full) in tasks.into_iter().zip(solutions){
                let added=basis.include_solution(&currents,&full,budget)?;eprintln!("training port {input}: added {added:?}, {:?}",full.audit);
                training.push(serde_json::json!({"frequencyHz":frequency,"port":input,"addedVoltageCurrent":added,"audit":full.audit}));training_solutions.push((frequency,input,full));
            }

        }
        record_stage(&mut stages,"basisTraining",stage);
        let stage=std::time::Instant::now();
        let candidate=match restored.take(){Some(p)=>p.model,None=>basis.model(&currents,field.as_ref(),&magnetic,&ports,budget)?};
        if let Some(j)=developer_checkpoint {
            let p=Prepared{basis,model:candidate.clone(),training:training.clone(),training_solutions:training_solutions.clone()};j.save("prepared.json",&p)?;basis=p.basis;
        }
        record_stage(&mut stages,"reduction",stage);
        eprintln!("P6 candidate {}+{} modes",candidate.potential_nodes.len(),candidate.current_dofs.len());
        // Reserve concurrently retained extraction/basis/training storage before P6.
        // P6's descriptor plan independently covers its model, factor and workspace.
        let basis_live=(basis.voltage.len()*currents.potential_count()+basis.current.len()*currents.current_count()).checked_mul(32).ok_or("basis byte overflow")?;
        let operator_live=currents.planned_bytes.checked_add(field.planned_bytes()).and_then(|n|n.checked_add(magnetic.planned_bytes())).ok_or("operator live overflow")?;
        let reserved=solver.as_ref().map_or(operator_live,|s|s.planned_bytes).checked_add(basis_live)
            .and_then(|n|n.checked_add(training_solutions.len().checked_mul(4*currents.potential_count()+2*currents.current_count())?.checked_mul(16)?))
            .and_then(|n|n.checked_add(currents.current_count().checked_mul(64)?))
            .and_then(|n|n.checked_add(1024*1024)).ok_or("P6 live overflow")?;
        let p6_bytes=budget.max_bytes.checked_sub(reserved).filter(|&n|n>0).ok_or("P6 remaining shared memory budget exhausted")?;
        let p6_budget=crate::electrical::Budget{max_memory_mi_b:Some(p6_bytes as f64/1048576.),max_seconds:Some(remaining()?.as_secs_f64())};
        let stage=std::time::Instant::now();
        let response=if frequencies_hz.is_empty() {None}else{Some(coupled::impedance(&candidate,ports.len(),frequencies_hz,Some(&p6_budget))?)};
        if mode=="prepare" {stages.insert("userP6Solve".into(),serde_json::Value::Null);}else{record_stage(&mut stages,"userP6Solve",stage);}
        let cold_prepare_seconds=started.elapsed().as_secs_f64();
        let stage=std::time::Instant::now();
        let verification_started=std::time::Instant::now();
        let verification_remaining=||if mode=="verify" {
            std::time::Duration::from_secs(1800).checked_sub(verification_started.elapsed())
                .filter(|t|!t.is_zero()).ok_or("LOCAL_DEADLINE: developer verification deadline")
        }else{remaining()};
        let mut full_checks=vec![];let mut verification_failure:Option<String>=None;
        if let Some(solver)=solver.filter(|_|mode=="verify"||mode=="full") {
            let mut reduced=coupled::System::compile(&candidate,ports.len(),Some(&p6_budget))?;
            let reduced_n=candidate.potential_nodes.len()+candidate.current_dofs.len();
            let drives=FREQUENCIES_HZ.iter().flat_map(|&f|
                [vec![1.,0.],vec![0.,1.],vec![1.,-1.]].into_iter().map(move|drive|(f,drive))).collect::<Vec<_>>();
            for (point,(frequency,drive)) in drives.into_iter().enumerate() {
                let name=format!("point-{point:02}.json");
                if let Some(j)=developer_checkpoint {if let Some(saved)=j.load::<serde_json::Value>(&name)?{if saved["frequencyHz"]!=serde_json::json!(frequency)||saved["drive"]!=serde_json::json!(drive){return Err("developer point mapping mismatch".into());}full_checks.push(saved);continue;}}
                if drive.len()!=ports.len(){return Err("full diagnostic currently requires two physical ports".into());}
                let mut injection=vec![0.;currents.potential_count()];
                for(p,&[a,b])in ports.iter().enumerate(){injection[contacts[a].1]+=drive[p];injection[contacts[b].1]-=drive[p];}
                let training_port=if drive==[1.,0.] {Some(0)}else if drive==[0.,1.] {Some(1)}else{None};
                let full=training_port.and_then(|port|training_solutions.iter().find(|(f,p,_)|*f==frequency&&*p==port).map(|(_,_,s)|s.clone()));
                let full=match full {Some(v)=>v,None=>{let duration=match verification_remaining(){Ok(v)=>v,Err(e)=>{verification_failure=Some(e.into());break;}};match solver.solve(frequency,&injection,&vec![0.;injection.len()],duration){Ok(v)=>v,Err(e)=>{verification_failure=Some(e);break;}}}};
                eprintln!("full drive {drive:?}: {:?}",full.audit);
                let mut rhs=reduced.port_rhs(&drive);rhs.resize(2*reduced_n,0.);
                let x=reduced.solve(2.*std::f64::consts::PI*frequency,rhs,&vec![0.;reduced_n],&mut Counters::default())?;
                let nvr=candidate.potential_nodes.len();
                let recover=|offset:usize|->Vec<f64>{(0..currents.current_count()).map(|j|(0..basis.current.len()).map(|k|basis.current[k][j]*x[offset+nvr+k]).sum()).collect()};
                let rr=recover(0);let ri=recover(reduced_n);
                let scale=full.real_currents_a.iter().zip(&full.imaginary_currents_a).map(|(r,i)|r.hypot(*i)).fold(0.,f64::max);
                let errors:Vec<_>=(0..rr.len()).map(|j|(rr[j]-full.real_currents_a[j]).hypot(ri[j]-full.imaginary_currents_a[j])/full.real_currents_a[j].hypot(full.imaginary_currents_a[j]).max(1e-12*scale)).collect();
                let worst=(0..rr.len()).max_by(|&a,&b|errors[a].total_cmp(&errors[b])).unwrap();let state_error=errors[worst];
                let sum_losses=|r:&[f64],i:&[f64]|->Result<Vec<f64>,String>{Ok(currents.losses(r)?.into_iter().zip(currents.losses(i)?).map(|(a,b)|a+b).collect())};
                let full_cells=sum_losses(&full.real_currents_a,&full.imaginary_currents_a)?;let reduced_cells=sum_losses(&rr,&ri)?;
                let full_loss=full_cells.iter().sum::<f64>();let reduced_loss=reduced_cells.iter().sum::<f64>();
                let loss_scale=full_cells.iter().copied().fold(0.,f64::max);let cell_loss_error=full_cells.iter().zip(&reduced_cells).map(|(a,b)|(a-b).abs()/a.max(1e-12*loss_scale)).fold(0.,f64::max);
                let full_ports:Vec<_>=ports.iter().map(|&[a,b]|solver.port_voltage(&full,contacts[a].1,contacts[b].1)).collect();
                full_checks.push(serde_json::json!({"drive":drive,"frequencyHz":frequency,"audit":full.audit,"fullPortVoltages":full_ports,"reducedPortReal":reduced.port_voltages(&x,ports.len()),"reducedPortImaginary":reduced.port_voltages(&x[reduced_n..],ports.len()),"maxCurrentCoordinateError":state_error,"maxCellLossError":cell_loss_error,"fullLossW":full_loss,"reducedLossW":reduced_loss,"worstCurrent":{"index":worst,"axis":worst%3,"full":[full.real_currents_a[worst],full.imaginary_currents_a[worst]],"recovered":[rr[worst],ri[worst]],"weakFloor":1e-12*scale},"fullCurrentReal":full.real_currents_a,"fullCurrentImaginary":full.imaginary_currents_a,"reducedState":x}));
                if let Some(j)=developer_checkpoint {j.save(&name,full_checks.last().unwrap())?;}
            }
        }
        record_stage(&mut stages,"developerVerification",stage);
        model=serde_json::json!({"admission":"accepted-electric-v1-full-model-verification-pending","coldPrepareSeconds":cold_prepare_seconds,"fieldOperator":field_metadata,"magneticActionReuse":magnetic.stats(),"magneticNumericalLinearity":magnetic_linearity,"model":candidate,"response":response,"training":training,"fullChecks":full_checks,"verificationStatus":if verification_failure.is_some(){"incomplete"}else if mode=="verify"||mode=="full"{"complete"}else{"not-run"},"verificationFailure":verification_failure,"resumedPrepared":resumed,"voltageBasis":basis.voltage,"currentBasis":basis.current});
    }
    let source_ids:Vec<_>=b.mesh.copper_tetrahedra.iter().map(|c|&c.source_ids).collect();
    let nodes:Vec<_>=b.mesh.copper_tetrahedra.iter().map(|c|c.nodes).collect();
    let mut output=serde_json::json!({"request":r,"cellResistanceOhm":currents.cells.iter().map(|c|c.resistance_ohm).collect::<Vec<_>>(),"cellDensityPerAmpM2":currents.cells.iter().map(|c|c.density_per_amp_m2).collect::<Vec<_>>(),"candidate":model,"arrangement":{"atoms":b.arrangement_atoms,"maxCopperAreaChange":b.max_copper_area_change,"maxContactAreaChange":b.max_contact_area_change,"sourceCopperRestorations":b.source_copper_restorations},"electrodeAreasM2":b.electrode_areas_m2,"electrodeNodes":electrodes,"nodesM":b.mesh.nodes_m,"copperCells":nodes,"sourceIds":source_ids,"barrels":b.barrel_ids,"mapping":b.source_mapping,"originMm":b.origin_mm,"potentialCount":currents.potential_count(),"currentCount":currents.current_count(),"potentialMeshNodes":currents.mesh_nodes,"components":currents.components,"magneticMeshTetrahedra":b.mesh.tetrahedra.len()+b.mesh.copper_tetrahedra.len(),"fieldNodesM":field_mesh.nodes_m,"fieldCopperCells":field_mesh.copper_tetrahedra.iter().map(|t|t.nodes).collect::<Vec<_>>(),"fieldTetrahedra":field_mesh.tetrahedra.len(),"fieldCells":field_mesh.tetrahedra.iter().map(|t|t.nodes).collect::<Vec<_>>(),"fieldEpsilonR":field_mesh.tetrahedra.iter().map(|t|t.epsilon_r).collect::<Vec<_>>(),"externalFaces":field_mesh.external_faces,"copperVolumeM3":b.mesh.copper_volume_m3,"fieldVolumeM3":field_mesh.dielectric_volume_m3,"plannedBytes":currents.planned_bytes,"elapsedSeconds":b.elapsed_seconds});
    if let Some(path)=cached_field {for key in ["fieldNodesM","fieldCopperCells","fieldTetrahedra","fieldCells","fieldEpsilonR","fieldVolumeM3"]{output.as_object_mut().unwrap().remove(key);}output["fieldDataSource"]=serde_json::json!({"kind":"complete-source-P1-Schur-checkpoint","path":path,"geometry":"stored producer evidence; no fabricated field grid in this output","physicalAdmission":false});}
    if !(mode=="verify") {remaining()?;}
    output["stageSeconds"]=serde_json::Value::Object(stages);
    output["wallSeconds"]=serde_json::json!(started.elapsed().as_secs_f64());
    Ok(output)
}
