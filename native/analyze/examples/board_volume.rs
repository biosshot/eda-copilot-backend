//! Bounded diagnostic of the actual board material/current mesh, never a
//! precomputed circuit. This example is not a production worker/DSL entry point.
use pcb_analyze::{import_json,extraction::local::{board_mesh,volume_current,volume_field,volume_magnetic,volume_reduction,volume_solve,AllocationBudget},electrical::{coupled,Counters}};
use pcb_analyze::extraction::local::volume_field::ElectricApply;
use pcb_analyze::extraction::local::volume_magnetic::MagneticApply;
// Original accepted current-space coupon sweep; keep the exact saved values.
const FREQUENCIES_HZ: [f64; 17] = [1000.0, 1333.5214321633239, 1778.2794100389228, 2371.373705661655, 3162.2776601683795, 4216.965034285822, 5623.413251903491, 7498.942093324558, 10000.0, 13335.21432163324, 17782.79410038923, 23713.737056616552, 31622.776601683792, 42169.65034285823, 56234.13251903491, 74989.42093324558, 100000.0];
fn main()->Result<(),String> {
    let started=std::time::Instant::now();
    let remaining=||std::time::Duration::from_secs(120).checked_sub(started.elapsed())
        .filter(|t|!t.is_zero()).ok_or("LOCAL_DEADLINE: shared cold calculation deadline");
    let args:Vec<_>=std::env::args().collect();
    if !(3..=6).contains(&args.len()) {return Err("board_volume <board.json> <request.json> [magnetic-config] [geometry|full|train] [field-request.json]".into());}
    let config=args.get(3).map(String::as_str).unwrap_or("entity:6:8:16:3");
    let magnetic_config=config.strip_prefix("entity:").ok_or("only direct FEM entity:trace:body:exterior:stars is supported")?
        .split(':').map(|s|s.parse::<usize>().map_err(|_|"invalid direct FEM configuration"))
        .collect::<Result<Vec<_>,_>>()?;
    if magnetic_config.len()!=4 {return Err("entity:trace:body:exterior:stars required".into());}
    if args.get(4).is_some_and(|s|!["geometry","field","full","train"].contains(&s.as_str())) {
        return Err("only geometry/field/full/train direct FEM modes are supported".into());
    }
    let text=std::fs::read_to_string(&args[1]).map_err(|e|e.to_string())?;
    let board=import_json(&text).board.ok_or("board import rejected")?;
    let r:board_mesh::Request=serde_json::from_str(&std::fs::read_to_string(&args[2]).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    // The board builder independently reserves2048 bytes/tetra before extrusion.
    // Keep the pre-existing geometry count cap independent of the 2 GiB cap;
    // sparse field/factor/current work must still fit the remaining live bytes.
    let budget=AllocationBudget{max_bytes:2*1024*1024*1024,max_nodes:100000,max_tetrahedra:512*1024*1024/2048};
    let b=board_mesh::build(&board,&r,budget,remaining()?)?;
    let cached_field=args.get(5).and_then(|s|s.strip_prefix("field-cache:"));
    let field_b=if let Some(path)=args.get(5).filter(|_|cached_field.is_none()) {
        let fr:board_mesh::Request=serde_json::from_str(&std::fs::read_to_string(path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        if fr.roi_mm!=r.roi_mm||fr.stack!=r.stack||fr.finished_hole_plating_m!=r.finished_hole_plating_m||fr.barrel_conductivity_s_m!=r.barrel_conductivity_s_m||fr.curve_deviation_mm!=r.curve_deviation_mm||fr.electrodes!=r.electrodes||fr.ports!=r.ports||fr.accept_unknown_fill_freshness!=r.accept_unknown_fill_freshness||fr.allow_insulated_copper_cuts!=r.allow_insulated_copper_cuts {return Err("field refinement must preserve physical ROI, copper, bore, materials and finite contacts".into());}
        let fb=board_mesh::build(&board,&fr,AllocationBudget{max_bytes:budget.max_bytes.checked_sub(b.mesh.retained_bytes()?).ok_or("field mesh live budget exhausted")?,..budget},remaining()?)?;
        if (fb.mesh.copper_volume_m3-b.mesh.copper_volume_m3).abs()>1e-8*b.mesh.copper_volume_m3||fb.barrel_ids!=b.barrel_ids||fb.source_mapping!=b.source_mapping {return Err("independent field mesh changed physical copper volume or source mapping".into());}Some(fb)
    }else{None};
    let field_mesh=&field_b.as_ref().unwrap_or(&b).mesh;
    eprintln!("common arrangement {} atoms; copper area change {:e}, contact area change {:e}",b.arrangement_atoms,b.max_copper_area_change,b.max_contact_area_change);
    let electrodes=r.electrodes.iter().map(|e|b.electrode_nodes(e)).collect::<Result<Vec<_>,_>>()?;
    let currents=volume_current::assemble(&b.mesh,&b.conductivity_s_m,&electrodes,budget)?;
    let mut model=serde_json::Value::Null;
    if !electrodes.is_empty()&&!args.get(4).is_some_and(|s|s=="geometry") {
        eprintln!("material/current mesh complete: {} potentials, {} currents",currents.potential_count(),currents.current_count());
        let contacts=r.electrodes.iter().zip(&electrodes).map(|(e,n)|(e.name.clone(),currents.node_potential[&n[0]])).collect::<Vec<_>>();
        let mut basis=volume_reduction::harmonic(&currents,&contacts,budget)?;
        eprintln!("harmonic {} voltage / {} current modes",basis.voltage.len(),basis.current.len());
        let field_budget=AllocationBudget{max_bytes:budget.max_bytes.checked_sub(currents.planned_bytes).ok_or("field live budget exhausted")?,..budget};
        // Keep the existing sparse FEM factor and solve only requested RHS.
        // ElectricApply still accepts the full original potential space.
        let (field,field_metadata):(Box<dyn ElectricApply>,serde_json::Value)=if let Some(path)=cached_field {
            let field=volume_field::DenseField::load_checkpoint(&b.mesh,&currents,&board_mesh::physical_signature(&r),path,field_budget)?;
            let metadata=serde_json::json!({"kind":"complete-original-P1-checkpoint","assemblyPeakBytes":field.assembly_peak_bytes,"retainedBytes":field.planned_bytes(),"maxChargeEnergyColumnError":field.max_charge_energy_column_error,"maxDirectedReciprocity":field.max_directed_reciprocity});
            (Box::new(field),metadata)
        }else{
            let field=if field_b.is_some(){volume_field::Field::new_vertical_refined(field_mesh,&b.mesh,&currents,field_budget)?}else{volume_field::Field::new(&b.mesh,&currents,field_budget)?};
            let metadata=serde_json::json!({"kind":"sparse-original-P1-requested-RHS","plannedBytes":field.planned_bytes(),"fullSchurMaterialized":false});
            (Box::new(field),metadata)
        };
        if args.get(4).is_some_and(|s|s=="field") {
            println!("{}",serde_json::json!({"kind":"unaccepted-original-P1-field-v1","physicalAdmission":false,"potentialCount":currents.potential_count(),"currentCount":currents.current_count(),"sourceCount":currents.cells.len(),"physicalInputs":board_mesh::physical_signature(&r),"field":field_metadata,"harmonicFieldGram":field.energy_gram(&basis.voltage)?}));return Ok(());
        }
        let basis_bytes=(basis.voltage.len()*currents.potential_count()+basis.current.len()*currents.current_count()).checked_mul(32).ok_or("basis live plan overflow")?;
        let live=currents.planned_bytes.checked_add(field.planned_bytes()).and_then(|n|n.checked_add(basis_bytes)).ok_or("magnetic live plan overflow")?;
        let magnetic_budget=AllocationBudget{max_bytes:budget.max_bytes.checked_sub(live).ok_or("magnetic live budget exhausted")?,..budget};
        let magnetic=volume_magnetic::Operator::new_enriched_patch(
            &b.mesh,magnetic_budget,magnetic_config[2],magnetic_config[0],
            magnetic_config[1],magnetic_config[3])?;
        eprintln!("magnetic factor plan {} bytes",magnetic.planned_bytes());
        let left=&basis.current[0];let right=basis.current.last().unwrap();
        let combination:Vec<_>=left.iter().zip(right).map(|(a,b)|0.75*a-0.25*b).collect();
        let al=magnetic.apply(&currents,left)?;let ar=magnetic.apply(&currents,right)?;let ac=magnetic.apply(&currents,&combination)?;
        let linearity_reference=al.iter().zip(&ar).map(|(a,b)|(0.75*a).powi(2)+(0.25*b).powi(2)).sum::<f64>().sqrt();
        let magnetic_linearity=ac.iter().zip(al.iter().zip(&ar)).map(|(c,(a,b))|(c-(0.75*a-0.25*b)).powi(2)).sum::<f64>().sqrt()/linearity_reference.max(f64::MIN_POSITIVE);
        eprintln!("magnetic numerical linearity {magnetic_linearity:e}");
        if !magnetic_linearity.is_finite()||magnetic_linearity>1e-12 {return Err(format!("magnetic numerical linearity refuses {magnetic_linearity:e}"));}
        for _ in 0..2 {basis.extend_magnetic(&currents,&magnetic)?;}
        let ports=r.ports.iter().map(|pair| {
            let a=r.electrodes.iter().position(|e|e.name==pair[0]).ok_or("unknown positive electrode")?;
            let b=r.electrodes.iter().position(|e|e.name==pair[1]).ok_or("unknown return electrode")?;Ok([a,b])
        }).collect::<Result<Vec<_>,String>>()?;
        let full_mode=args.get(4).is_some_and(|s|s=="full"||s=="train");
        let solver=if full_mode {Some(volume_solve::Solver::new(&currents,field.as_ref(),&magnetic,budget)?)}else{None};
        let mut training=vec![];let mut training_solutions=vec![];
        if args.get(4).is_some_and(|s|s=="train") {
            for frequency in [1e4,1e5] {for input in 0..ports.len() {
                let mut injection=vec![0.;currents.potential_count()];let[a,b]=ports[input];injection[contacts[a].1]=1.;injection[contacts[b].1]=-1.;
                let full=solver.as_ref().unwrap().solve(frequency,&injection,&vec![0.;injection.len()],remaining()?.min(std::time::Duration::from_secs(30)))?;
                let added=basis.include_solution(&currents,&full,budget)?;eprintln!("training port {input}: added {added:?}, {:?}",full.audit);
                training.push(serde_json::json!({"frequencyHz":frequency,"port":input,"addedVoltageCurrent":added,"audit":full.audit}));training_solutions.push((frequency,input,full));
            }}
        }
        let candidate=basis.model(&currents,field.as_ref(),&magnetic,&ports,budget)?;
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
        let p6_budget=pcb_analyze::electrical::Budget{max_memory_mi_b:Some(p6_bytes as f64/1048576.),max_seconds:Some(remaining()?.as_secs_f64())};
        let response=coupled::impedance(&candidate,ports.len(),&FREQUENCIES_HZ,Some(&p6_budget))?;
        let mut full_checks=vec![];
        if let Some(solver)=solver {
            let mut reduced=coupled::System::compile(&candidate,ports.len(),Some(&p6_budget))?;
            let reduced_n=candidate.potential_nodes.len()+candidate.current_dofs.len();
            let drives=FREQUENCIES_HZ.iter().flat_map(|&f|
                [vec![1.,0.],vec![0.,1.],vec![1.,-1.]].into_iter().map(move|drive|(f,drive))).collect::<Vec<_>>();
            for (frequency,drive) in drives {
                if drive.len()!=ports.len(){return Err("full diagnostic currently requires two physical ports".into());}
                let mut injection=vec![0.;currents.potential_count()];
                for(p,&[a,b])in ports.iter().enumerate(){injection[contacts[a].1]+=drive[p];injection[contacts[b].1]-=drive[p];}
                let full=if drive==[1.,0.] {training_solutions.iter().find(|(f,p,_)|*f==frequency&&*p==0).map(|(_,_,s)|s.clone())}else{None};
                let full=match full {Some(v)=>v,None=>solver.solve(frequency,&injection,&vec![0.;injection.len()],remaining()?.min(std::time::Duration::from_secs(30)))?};
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
            }
        }
        model=serde_json::json!({"admission":"internal-candidate-requires-independent-field-mesh-domain-validation","fieldOperator":field_metadata,"magneticNumericalLinearity":magnetic_linearity,"model":candidate,"response":response,"training":training,"fullChecks":full_checks,"voltageBasis":basis.voltage,"currentBasis":basis.current});
    }
    let source_ids:Vec<_>=b.mesh.copper_tetrahedra.iter().map(|c|&c.source_ids).collect();
    let nodes:Vec<_>=b.mesh.copper_tetrahedra.iter().map(|c|c.nodes).collect();
    let mut output=serde_json::json!({"candidate":model,"arrangement":{"atoms":b.arrangement_atoms,"maxCopperAreaChange":b.max_copper_area_change,"maxContactAreaChange":b.max_contact_area_change},"electrodeAreasM2":b.electrode_areas_m2,"electrodeNodes":electrodes,"nodesM":b.mesh.nodes_m,"copperCells":nodes,"sourceIds":source_ids,"barrels":b.barrel_ids,"mapping":b.source_mapping,"originMm":b.origin_mm,"potentialCount":currents.potential_count(),"currentCount":currents.current_count(),"potentialMeshNodes":currents.mesh_nodes,"components":currents.components,"magneticMeshTetrahedra":b.mesh.tetrahedra.len()+b.mesh.copper_tetrahedra.len(),"fieldNodesM":field_mesh.nodes_m,"fieldCopperCells":field_mesh.copper_tetrahedra.iter().map(|t|t.nodes).collect::<Vec<_>>(),"fieldTetrahedra":field_mesh.tetrahedra.len(),"fieldCells":field_mesh.tetrahedra.iter().map(|t|t.nodes).collect::<Vec<_>>(),"fieldEpsilonR":field_mesh.tetrahedra.iter().map(|t|t.epsilon_r).collect::<Vec<_>>(),"externalFaces":field_mesh.external_faces,"copperVolumeM3":b.mesh.copper_volume_m3,"fieldVolumeM3":field_mesh.dielectric_volume_m3,"plannedBytes":currents.planned_bytes,"elapsedSeconds":b.elapsed_seconds});
    if let Some(path)=cached_field {for key in ["fieldNodesM","fieldCopperCells","fieldTetrahedra","fieldCells","fieldEpsilonR","fieldVolumeM3"]{output.as_object_mut().unwrap().remove(key);}output["fieldDataSource"]=serde_json::json!({"kind":"complete-source-P1-Schur-checkpoint","path":path,"geometry":"stored producer evidence; no fabricated field grid in this output","physicalAdmission":false});}
    remaining()?;
    println!("{output}");
    Ok(())
}
