//! Shared local direct-FEM preparation and developer verification.
//! CPU/F64, explicit physical requests, no external diagnostic process.
use super::{board_mesh,conservative_current::ConservativeSpace,copper_skin_profile,frequency_audit,volume_current,volume_field,volume_integral_magnetic,volume_reduction,volume_solve,AllocationBudget};
use crate::electrical::{coupled,Counters};
use super::volume_field::ElectricApply;
use super::volume_magnetic::MagneticApply;
// Original accepted current-space coupon sweep; keep the exact saved values.
pub const FREQUENCIES_HZ: [f64; 17] = [1000.0, 1333.5214321633239, 1778.2794100389228, 2371.373705661655, 3162.2776601683795, 4216.965034285822, 5623.413251903491, 7498.942093324558, 10000.0, 13335.21432163324, 17782.79410038923, 23713.737056616552, 31622.776601683792, 42169.65034285823, 56234.13251903491, 74989.42093324558, 100000.0];
/// Explicit developer holdouts take priority; retain the exact historical axis
/// only when no frequencies were supplied. DC uses its separate compatible solve.
pub fn verification_axis(requested:&[f64])->Result<&[f64],String>{
    let axis=if requested.is_empty(){&FREQUENCIES_HZ[..]}else{requested};
    if axis.len()>8192||axis.iter().any(|f|!f.is_finite()||*f<=0.)||axis.windows(2).any(|a|a[1]<=a[0]){return Err("LOCAL_VERIFY_AXIS: ordered positive finite AC holdouts required".into());}
    Ok(axis)
}
fn record_stage(stages:&mut serde_json::Map<String,serde_json::Value>,name:&str,start:std::time::Instant) {
    let seconds=start.elapsed().as_secs_f64();eprintln!("P7_STAGE {name} {seconds:.6}s");
    stages.insert(name.into(),serde_json::json!(seconds));
}
/// Explicit increased developer memory also admits its byte-bounded geometry;
/// the original production/default count caps remain bit-for-bit unchanged.
pub fn allocation_budget(memory_mib:usize)->AllocationBudget{
    let max_bytes=memory_mib*1024*1024-crate::extraction::local::parallel::stack_reservation();
    if memory_mib>2048{AllocationBudget{max_bytes,max_nodes:max_bytes/4096,max_tetrahedra:max_bytes/2048}}
    else{AllocationBudget{max_bytes,max_nodes:100000,max_tetrahedra:512*1024*1024/2048}}
}

fn loaded_values(value:&serde_json::Value)->Result<Vec<f64>,String>{
    serde_json::from_value(value.clone()).map_err(|e|format!("LOCAL_LOADED_STATE: {e}"))
}
fn weighted(columns:&[Vec<f64>],weights:&[f64])->Result<Vec<f64>,String>{
    use super::accurate::Sum;
    if columns.len()!=weights.len()||columns.is_empty()||columns[0].is_empty()||columns.iter().any(|v|v.len()!=columns[0].len()||v.iter().any(|x|!x.is_finite()))||weights.iter().any(|x|!x.is_finite()){return Err("LOCAL_LOADED_STATE: finite equally sized state columns required".into());}
    Ok((0..columns[0].len()).map(|j|columns.iter().zip(weights).fold(Sum::default(),|sum,(v,w)|sum.add(Sum::from(v[j]).mul(*w))).value()).collect())
}
#[derive(Clone,Copy)]
struct ExtendedC{hi:crate::electrical::frequency::math::C,lo:crate::electrical::frequency::math::C}
impl ExtendedC{
    fn value(self)->crate::electrical::frequency::math::C{
        use crate::electrical::frequency::math::C;
        C::new(self.hi.real+self.lo.real,self.hi.imaginary+self.lo.imaginary)
    }
}
fn complex_fields_extended(checks:&[&serde_json::Value],real_key:&str,imaginary_key:&str,coefficients:&[ExtendedC])->Result<(Vec<f64>,Vec<f64>),String>{
    use super::accurate::Sum;
    if checks.len()!=coefficients.len()||checks.is_empty(){return Err("LOCAL_LOADED_STATE: complex state/coefficient count mismatch".into());}
    let real=checks.iter().map(|check|loaded_values(&check[real_key])).collect::<Result<Vec<_>,_>>()?;
    let imaginary=checks.iter().map(|check|loaded_values(&check[imaginary_key])).collect::<Result<Vec<_>,_>>()?;
    let n=real[0].len();if n==0||real.iter().chain(&imaginary).any(|v|v.len()!=n){return Err("LOCAL_LOADED_STATE: real/imaginary state size mismatch".into());}
    let mut output=(vec![0.;n],vec![0.;n]);
    for j in 0..n{let(mut rr,mut ii)=(Sum::default(),Sum::default());for p in 0..checks.len(){for a in [coefficients[p].hi,coefficients[p].lo]{rr=rr.add(Sum::from(real[p][j]).mul(a.real)).sub(Sum::from(imaginary[p][j]).mul(a.imaginary));ii=ii.add(Sum::from(real[p][j]).mul(a.imaginary)).add(Sum::from(imaginary[p][j]).mul(a.real));}}output.0[j]=rr.value();output.1[j]=ii.value();}
    Ok(output)
}
fn complex_fields(checks:&[&serde_json::Value],real_key:&str,imaginary_key:&str,coefficients:&[crate::electrical::frequency::math::C])->Result<(Vec<f64>,Vec<f64>),String>{
    let extended=coefficients.iter().map(|&hi|ExtendedC{hi,lo:crate::electrical::frequency::math::C::default()}).collect::<Vec<_>>();
    complex_fields_extended(checks,real_key,imaginary_key,&extended)
}
fn accurate_complex_dot(row:&[crate::electrical::frequency::math::C],weights:&[ExtendedC])->Result<crate::electrical::frequency::math::C,String>{
    use super::accurate::Sum;use crate::electrical::frequency::math::C;
    if row.len()!=weights.len()||row.is_empty(){return Err("LOCAL_LOADED_STATE: complex dot size mismatch".into());}
    let(mut real,mut imaginary)=(Sum::default(),Sum::default());for(&a,&w)in row.iter().zip(weights){for b in [w.hi,w.lo]{real=real.add(Sum::from(a.real).mul(b.real)).sub(Sum::from(a.imaginary).mul(b.imaginary));imaginary=imaginary.add(Sum::from(a.real).mul(b.imaginary)).add(Sum::from(a.imaginary).mul(b.real));}}
    Ok(C::new(real.value(),imaginary.value()))
}
/// Remove exactly one global voltage nullspace from the connected loaded DC
/// network. The anchor is numerical only: every original row and Norton port
/// balance is checked after reconstruction, without adding a physical ground.
pub fn anchored_loaded_dc(conductance:&[Vec<f64>],incidence:&[Vec<f64>],drive:&[f64],loads:&[f64],anchor:usize)->Result<(Vec<f64>,f64,Vec<f64>),String>{
    use super::accurate::Sum;
    use crate::electrical::frequency::{Quality,math::{C,Matrix,solve,solve_without_refinement}};
    let ne=conductance.len();let np=loads.len();if ne<2||np==0||anchor>=ne||drive.len()!=np||incidence.len()!=ne||drive.iter().any(|v|!v.is_finite())||loads.iter().any(|r|!r.is_finite()||*r<=0.)||conductance.iter().any(|row|row.len()!=ne||row.iter().any(|v|!v.is_finite()))||incidence.iter().any(|row|row.len()!=np||row.iter().any(|v|!v.is_finite())){return Err("LOCAL_LOADED_STATE: valid connected loaded network and anchor required".into());}
    let mut matrix=conductance.to_vec();for i in 0..ne{for j in 0..ne{for p in 0..np{matrix[i][j]+=incidence[i][p]*incidence[j][p]/loads[p];}}}
    let rhs:Vec<_>=(0..ne).map(|i|(0..np).map(|p|incidence[i][p]*drive[p]).sum()).collect();
    let free:Vec<_>=(0..ne).filter(|&i|i!=anchor).collect();
    let a:Matrix=free.iter().map(|&i|free.iter().map(|&j|C::new(matrix[i][j],0.)).collect()).collect();let mut b=vec![vec![C::default()];2*free.len()];for(row,&i)in free.iter().enumerate(){b[row][0]=C::new(rhs[i],0.);}
    let solved=solve(&a,&b,&mut Quality::default())?;let mut electrode_voltage=vec![Sum::default();ne];for(i,&row)in free.iter().enumerate(){electrode_voltage[row]=Sum::from(solved[i][0].real);}
    for _ in 0..4{let residual:Matrix=free.iter().map(|&i|{let mut value=Sum::from(rhs[i]);for j in 0..ne{for v in [electrode_voltage[j].hi,electrode_voltage[j].lo]{value=value.sub(Sum::from(matrix[i][j]).mul(v));}}vec![C::new(value.value(),0.)]}).collect();let correction=solve_without_refinement(&a,&residual,&mut Quality::default())?;for(row,&i)in free.iter().enumerate(){electrode_voltage[i]=electrode_voltage[i].add(Sum::from(correction[row][0].real));}}
    let mut residual:f64=0.;for i in 0..ne{let mut sum=Sum::from(rhs[i]);for j in 0..ne{for v in [electrode_voltage[j].hi,electrode_voltage[j].lo]{sum=sum.sub(Sum::from(matrix[i][j]).mul(v));}}let value=sum.value();let scale=rhs[i].abs()+(0..ne).map(|j|(matrix[i][j]*electrode_voltage[j].value()).abs()).sum::<f64>();residual=residual.max(value.abs()/scale.max(1e-30));}
    let norton:Vec<_>=(0..np).map(|p|{let mut port=Sum::default();let mut conduction=Sum::default();for i in 0..ne{for v in [electrode_voltage[i].hi,electrode_voltage[i].lo]{port=port.add(Sum::from(incidence[i][p]).mul(v));}let mut reaction=Sum::default();for j in 0..ne{for v in [electrode_voltage[j].hi,electrode_voltage[j].lo]{reaction=reaction.add(Sum::from(conductance[i][j]).mul(v));}}conduction=conduction.add(Sum::from(incidence[i][p].max(0.)).mul(reaction.value()));}Sum::from(drive[p]).sub(Sum::from(port.value()).mul(1./loads[p])).sub(conduction).value()}).collect();
    Ok((electrode_voltage.into_iter().map(|v|v.value()).collect(),residual,norton))
}
/// Fixed Norton loads act on the existing full harmonic and reduced states.
/// No auxiliary current column is required to be an insulated DC field.
fn loaded_dc(currents:&volume_current::Space,basis:&volume_reduction::Basis,candidate:&coupled::Model,ports:&[[usize;2]],contacts:&[(String,usize)],conservative:&ConservativeSpace,drive:&[f64],loads:&[f64],budget:&crate::electrical::Budget)->Result<serde_json::Value,String>{
    let ne=contacts.len();let np=ports.len();if ne==0||np==0||drive.len()!=np||loads.len()!=np||loads.iter().any(|r|!r.is_finite()||*r<=0.){return Err("LOCAL_LOADED_STATE: complete finite loaded port mapping required".into());}
    let harmonic=basis.harmonic_electrodes()?;if harmonic.len()!=ne{return Err("LOCAL_LOADED_STATE: harmonic electrode states missing".into());}
    let electrode_currents=harmonic.iter().map(|v|{let mut current=currents.gradient(v)?;for cell in 0..currents.cells.len(){for axis in 0..3{current[3*cell+axis]/=currents.cells[cell].resistance_ohm;}}Ok(current)}).collect::<Result<Vec<_>,String>>()?;
    let reactions=electrode_currents.iter().map(|current|currents.divergence(current)).collect::<Result<Vec<_>,_>>()?;
    let mut incidence=vec![vec![0.;np];ne];for(p,&[a,b])in ports.iter().enumerate(){if a>=ne||b>=ne||a==b{return Err("LOCAL_LOADED_STATE: invalid electrode incidence".into());}incidence[a][p]=1.;incidence[b][p]=-1.;}
    let mut conductance=vec![vec![0.;ne];ne];for i in 0..ne{for j in 0..ne{conductance[i][j]=reactions[j][contacts[i].1];}}
    let (electrode_voltage,residual,norton)=anchored_loaded_dc(&conductance,&incidence,drive,loads,0)?;
    let (alternate_voltage,alternate_residual,alternate_norton)=anchored_loaded_dc(&conductance,&incidence,drive,loads,1)?;
    let full_potential=weighted(harmonic,&electrode_voltage)?;let full_current=weighted(&electrode_currents,&electrode_voltage)?;let alternate_current=weighted(&electrode_currents,&alternate_voltage)?;
    let full_ports:Vec<_>=ports.iter().map(|&[a,b]|electrode_voltage[a]-electrode_voltage[b]).collect();let alternate_ports:Vec<_>=ports.iter().map(|&[a,b]|alternate_voltage[a]-alternate_voltage[b]).collect();
    let drive_scale=drive.iter().map(|v|v.abs()).fold(1e-30,f64::max);let normalized_norton=|values:&[f64]|values.iter().map(|v|v.abs()).fold(0.,f64::max)/drive_scale;
    let norton_residual=normalized_norton(&norton);let alternate_norton_residual=normalized_norton(&alternate_norton);
    let port_scale=full_ports.iter().map(|v|v.abs()).fold(1e-30,f64::max);let port_error=full_ports.iter().zip(&alternate_ports).map(|(a,b)|(a-b).abs()).fold(0.,f64::max)/port_scale;
    let current_scale=full_current.iter().map(|v|v.abs()).fold(0.,f64::max).max(drive_scale);let current_error=full_current.iter().zip(&alternate_current).map(|(a,b)|(a-b).abs()).fold(0.,f64::max)/current_scale;
    if residual>1e-8||alternate_residual>1e-8||norton_residual>1e-8||alternate_norton_residual>1e-8||port_error>1e-8||current_error>1e-8{return Err(format!("LOCAL_LOADED_STATE: anchored full residual/Norton/gauge invariance exceeds 1e-8: residual={residual:e} alternateResidual={alternate_residual:e} norton={norton_residual:e} alternateNorton={alternate_norton_residual:e} ports={port_error:e} current={current_error:e}"));}
    let mut reduced=coupled::System::compile(candidate,np,Some(budget))?;for(p,&load)in loads.iter().enumerate(){reduced.add_port_admittance(p,1./load,0.)?;}
    let nvr=candidate.potential_nodes.len();let n=nvr+candidate.current_dofs.len();let mut rhs_reduced=reduced.port_rhs(drive);rhs_reduced.resize(n,0.);let x=reduced.solve(0.,rhs_reduced,&vec![0.;n],&mut Counters::default())?;
    let reduced_potential=weighted(&basis.voltage,&x[..nvr])?;let reduced_current=weighted(&basis.current,&x[nvr..])?;
    let full_faces=conservative.recover(currents,&full_current,true).map_err(|e|format!("full loaded DC: {e}"))?;let reduced_faces=conservative.recover(currents,&reduced_current,true).map_err(|e|format!("reduced loaded DC: {e}"))?;
    Ok(serde_json::json!({"frequencyHz":0.,"driveA":drive,"loadsOhm":loads,"electrodeVoltageV":electrode_voltage,"dcConductanceS":conductance,"fullPortVoltages":full_ports.iter().map(|&v|[v,0.]).collect::<Vec<_>>(),"reducedPortReal":reduced.port_voltages(&x,np),"reducedPortImaginary":vec![0.;np],"fullPotentialRealV":full_potential,"fullPotentialImaginaryV":vec![0.;currents.potential_count()],"reducedPotentialRealV":reduced_potential,"reducedPotentialImaginaryV":vec![0.;currents.potential_count()],"fullCurrentReal":full_current,"fullCurrentImaginary":vec![0.;currents.current_count()],"reducedCurrentReal":reduced_current,"reducedCurrentImaginary":vec![0.;currents.current_count()],"fullConservativeFaceReal":full_faces,"fullConservativeFaceImaginary":vec![0.;conservative.faces.len()],"reducedConservativeFaceReal":reduced_faces,"reducedConservativeFaceImaginary":vec![0.;conservative.faces.len()],"reducedState":x,"harmonicElectrodeNormalizedResidual":residual,"nortonResidualA":norton,"dcVoltageGauge":{"anchorElectrode":0,"comparisonAnchorElectrode":1,"comparisonNormalizedResidual":alternate_residual,"comparisonNortonNormalizedResidual":alternate_norton_residual,"portNormalizedError":port_error,"currentNormalizedError":current_error}}))
}
/// Solve the fixed Norton-load superposition with one complex RHS column.
pub(crate) fn loaded_ac_coefficients(full_u:&[Vec<crate::electrical::frequency::math::C>],drive:&[f64],loads:&[f64])->Result<Vec<crate::electrical::frequency::math::C>,String>{
    Ok(loaded_ac_coefficients_extended(full_u,drive,loads)?.into_iter().map(|v|v.hi).collect())
}
fn loaded_ac_coefficients_extended(full_u:&[Vec<crate::electrical::frequency::math::C>],drive:&[f64],loads:&[f64])->Result<Vec<ExtendedC>,String>{
    use super::accurate::Sum;
    use crate::electrical::frequency::{Quality,math::{C,Matrix,solve,solve_without_refinement}};
    let np=ports_len(drive,loads)?;
    if full_u.len()!=np||full_u.iter().any(|row|row.len()!=np||row.iter().any(|v|!v.real.is_finite()||!v.imaginary.is_finite())){return Err("LOCAL_LOADED_STATE: finite square full-port matrix required".into());}
    let a:Matrix=(0..np).map(|q|(0..np).map(|p|C::new(if q==p{1.}else{0.},0.)+C::new(full_u[q][p].real/loads[q],full_u[q][p].imaginary/loads[q])).collect()).collect();
    let b:Matrix=drive.iter().map(|&v|vec![C::new(v,0.)]).collect();
    let mut coefficients:Vec<_>=solve(&a,&b,&mut Quality::default())?.into_iter().map(|row|ExtendedC{hi:row[0],lo:C::default()}).collect();
    let residual=|x:&[ExtendedC]|->Result<(Matrix,f64),String>{
        let mut r=vec![vec![C::default()];np];let mut normalized:f64=0.;
        for q in 0..np{
            // Evaluate d - x - Zx/load using the original coefficients.
            // Scale the row by its load before accumulation so rounding Z/load
            // or adding the diagonal to it cannot perturb the residual.
            let load=loads[q];
            let mut rr=Sum::from(drive[q]).sub(Sum::from(x[q].hi.real)).sub(Sum::from(x[q].lo.real)).mul(load);
            let mut ii=Sum::from(-x[q].hi.imaginary).sub(Sum::from(x[q].lo.imaginary)).mul(load);
            let mut scale=drive[q].abs();
            for p in 0..np {
                let v=full_u[q][p];
                for y in [x[p].hi,x[p].lo] {
                    let pr=Sum::from(v.real).mul(y.real).sub(Sum::from(v.imaginary).mul(y.imaginary));
                    let pi=Sum::from(v.real).mul(y.imaginary).add(Sum::from(v.imaginary).mul(y.real));
                    rr=rr.sub(pr);
                    ii=ii.sub(pi);
                }
                scale+=a[q][p].abs()*x[p].value().abs();
            }
            let value=C::new(rr.value()/load,ii.value()/load);
            if !value.real.is_finite()||!value.imaginary.is_finite(){return Err("LOCAL_LOADED_STATE: nonfinite compensated AC residual".into());}
            normalized=normalized.max(value.abs()/scale.max(1e-30));
            r[q][0]=value;
        }
        Ok((r,normalized))
    };
    // Preserve correction tails: one rounded F64 coefficient is insufficient
    // when unit-state displacement potentials cancel by eleven decimal orders.
    for _ in 0..4{let(r,_)=residual(&coefficients)?;let correction=solve_without_refinement(&a,&r,&mut Quality::default())?;for q in 0..np{let real=Sum{hi:coefficients[q].hi.real,lo:coefficients[q].lo.real}.add(Sum::from(correction[q][0].real));let imaginary=Sum{hi:coefficients[q].hi.imaginary,lo:coefficients[q].lo.imaginary}.add(Sum::from(correction[q][0].imaginary));coefficients[q]=ExtendedC{hi:C::new(real.hi,imaginary.hi),lo:C::new(real.lo,imaginary.lo)};}}
    let(_,normalized)=residual(&coefficients)?;if !normalized.is_finite()||normalized>1e-8{return Err(format!("LOCAL_LOADED_STATE: compensated AC coefficient residual={normalized:e}"));}
    Ok(coefficients)
}
/// Combine exact unit-port states first, then recover the one physical loaded state.
fn loaded_ac(currents:&volume_current::Space,basis:&volume_reduction::Basis,candidate:&coupled::Model,conservative:&ConservativeSpace,checks:&[serde_json::Value],drive:&[f64],loads:&[f64],budget:&crate::electrical::Budget)->Result<serde_json::Value,String>{
    use crate::electrical::frequency::math::C;
    let np=ports_len(drive,loads)?;let mut selected=vec![None;np];
    for check in checks{if check["frequencyHz"].as_f64()!=Some(1000.){continue;}let d=loaded_values(&check["drive"])?;if d.len()!=np{continue;}for p in 0..np{if d.iter().enumerate().all(|(j,v)|*v==if j==p{1.}else{0.}){if selected[p].replace(check).is_some(){return Err("LOCAL_LOADED_STATE: duplicate 1 kHz unit-port state".into());}}}}
    let checks:Vec<_>=selected.into_iter().enumerate().map(|(p,check)|check.ok_or_else(||format!("LOCAL_LOADED_STATE: missing 1 kHz unit-port state {p}"))).collect::<Result<_,_>>()?;
    let full_u:Vec<Vec<C>>=(0..np).map(|q|(0..np).map(|p|{let values=serde_json::from_value::<Vec<[f64;2]>>(checks[p]["fullPortVoltages"].clone()).map_err(|e|format!("LOCAL_LOADED_STATE: {e}"))?;if values.len()!=np{return Err("LOCAL_LOADED_STATE: full port size mismatch".into());}Ok(C::new(values[q][0],values[q][1]))}).collect::<Result<Vec<_>,String>>()).collect::<Result<_,_>>()?;
    let coefficients=loaded_ac_coefficients_extended(&full_u,drive,loads)?;
    let full_ports=(0..np).map(|q|accurate_complex_dot(&full_u[q],&coefficients)).collect::<Result<Vec<_>,_>>()?;
    let (full_potential_real,full_potential_imaginary)=complex_fields_extended(&checks,"fullPotentialRealV","fullPotentialImaginaryV",&coefficients)?;
    let (full_current_real,full_current_imaginary)=complex_fields_extended(&checks,"fullCurrentReal","fullCurrentImaginary",&coefficients)?;
    // Solve the small reduced loaded network directly. Combining four separately
    // rounded unit solves amplifies their errors through the large capacitive modes.
    let mut reduced=coupled::System::compile(candidate,np,Some(budget))?;for(p,&load)in loads.iter().enumerate(){reduced.add_port_admittance(p,1./load,0.)?;}
    let n=candidate.potential_nodes.len()+candidate.current_dofs.len();let mut rhs=reduced.port_rhs(drive);rhs.resize(2*n,0.);let x=reduced.solve_compensated(2.*std::f64::consts::PI*1000.,rhs,&vec![0.;n],&mut Counters::default())?;
    let reduced_port_real=reduced.port_voltages(&x,np);let reduced_port_imaginary=reduced.port_voltages(&x[n..],np);
    let nvr=candidate.potential_nodes.len();let reduced_potential_real=weighted(&basis.voltage,&x[..nvr])?;let reduced_potential_imaginary=weighted(&basis.voltage,&x[n..n+nvr])?;let reduced_current_real=weighted(&basis.current,&x[nvr..n])?;let reduced_current_imaginary=weighted(&basis.current,&x[n+nvr..])?;
    let full_face_real=conservative.recover(currents,&full_current_real,false).map_err(|e|format!("full loaded AC real: {e}"))?;
    let full_face_imaginary=conservative.recover(currents,&full_current_imaginary,false).map_err(|e|format!("full loaded AC imaginary: {e}"))?;
    let reduced_face_real=conservative.recover(currents,&reduced_current_real,false).map_err(|e|format!("reduced loaded AC real: {e}"))?;
    let reduced_face_imaginary=conservative.recover(currents,&reduced_current_imaginary,false).map_err(|e|format!("reduced loaded AC imaginary: {e}"))?;
    let norton:Vec<_>=(0..np).map(|p|coefficients[p].value()-(C::new(drive[p],0.)-full_ports[p].scale(1./loads[p]))).collect();let residual=norton.iter().map(|v|v.abs()).fold(0.,f64::max)/drive.iter().map(|v|v.abs()).fold(1e-30,f64::max);
    Ok(serde_json::json!({"frequencyHz":1000.,"driveA":drive,"loadsOhm":loads,"superpositionCoefficientsA":coefficients.iter().map(|a|[a.hi.real,a.hi.imaginary]).collect::<Vec<_>>(),"superpositionCoefficientTailsA":coefficients.iter().map(|a|[a.lo.real,a.lo.imaginary]).collect::<Vec<_>>(),"fullPortVoltages":full_ports.iter().map(|v|[v.real,v.imaginary]).collect::<Vec<_>>(),"reducedPortReal":reduced_port_real,"reducedPortImaginary":reduced_port_imaginary,"fullPotentialRealV":full_potential_real,"fullPotentialImaginaryV":full_potential_imaginary,"reducedPotentialRealV":reduced_potential_real,"reducedPotentialImaginaryV":reduced_potential_imaginary,"fullCurrentReal":full_current_real,"fullCurrentImaginary":full_current_imaginary,"reducedCurrentReal":reduced_current_real,"reducedCurrentImaginary":reduced_current_imaginary,"fullConservativeFaceReal":full_face_real,"fullConservativeFaceImaginary":full_face_imaginary,"reducedConservativeFaceReal":reduced_face_real,"reducedConservativeFaceImaginary":reduced_face_imaginary,"reducedState":x,"superpositionNormalizedResidual":residual,"nortonResidualA":norton.iter().map(|v|[v.real,v.imaginary]).collect::<Vec<_>>() }))
}
fn ports_len(drive:&[f64],loads:&[f64])->Result<usize,String>{if drive.is_empty()||drive.len()!=loads.len()||drive.iter().chain(loads).any(|v|!v.is_finite())||loads.iter().any(|r|*r<=0.){Err("LOCAL_LOADED_STATE: complete finite loaded port mapping required".into())}else{Ok(drive.len())}}

/// Internal orchestration options. Developer verification is explicitly opt-in.
pub struct Options<'a> {
    pub mode: &'a str,
    pub developer_checkpoint:Option<&'a super::developer_checkpoint::Journal>,
    pub magnetic_config: [usize;4],
    pub magnetic_air_degree:usize,
    pub field_request: Option<&'a board_mesh::Request>,
    pub cached_field: Option<&'a str>,
    pub frequencies_hz: &'a [f64],
}
/// Accepted electric discretization; original current mesh and contacts survive.
/// This is a v1 configuration, not a complete spatial accuracy certificate.
pub fn accepted_electric_request(r:&board_mesh::Request)->Result<board_mesh::Request,String> {
    let mut field=r.clone();
    field.air_margin_m=r.electric_air_margin_m.unwrap_or(0.008);
    field.max_vertical_step_m=r.max_vertical_step_m.min(0.0005);
    field.interface_step_m=Some(0.000025);
    field.interface_growth=Some(2.);
    let Some(margin)=r.electric_air_margin_m else {return Ok(field);};

    // Automatic channel cuts need air resolution near metal that is independent
    // of the far box and does not alter the current mesh or its metal trace.
    let xy_step=r.max_triangle_area_m2.sqrt();
    let longest_span=(0..2).map(|axis|(r.roi_mm[1][axis]-r.roi_mm[0][axis])*1e-3).fold(0.,f64::max);
    let stack_height=r.stack.iter().map(|layer|layer.thickness_m).sum::<f64>();
    let dielectric_step=r.stack.iter().filter(|layer|!layer.copper).map(|layer|layer.thickness_m).reduce(f64::min)
        .ok_or("LOCAL_ELECTRIC_GRADING: a dielectric layer is required")?;
    if [margin,xy_step,longest_span,stack_height,dielectric_step].into_iter().any(|v|!v.is_finite()||v<=0.) {
        return Err("LOCAL_ELECTRIC_GRADING: finite positive field margin, ROI, mesh and stack dimensions required".into());
    }
    let interface_step=r.interface_step_m.unwrap_or(0.000025).min(0.000025);
    field.interface_step_m=Some(interface_step);
    field.interface_growth=Some(r.interface_growth.unwrap_or(2.));
    let first_step=interface_step.min(field.max_vertical_step_m).min(dielectric_step).min(xy_step).min(margin);
    field.air_first_step_m=Some(first_step.min(r.air_first_step_m.unwrap_or(first_step)));

    // Use the same four-stack-height neighbourhood as the board field mesh.
    let near_distance=(4.*stack_height).max(r.air_near_distance_m.unwrap_or(0.)).min(margin);
    let divisions=((longest_span+2.*near_distance)/xy_step).ceil();
    if !(1. ..usize::MAX as f64).contains(&divisions) {
        return Err("LOCAL_ELECTRIC_GRADING: near-air division count is not representable".into());
    }
    field.air_near_distance_m=Some(near_distance);
    field.air_edge_divisions=Some((divisions as usize).max(r.air_edge_divisions.unwrap_or(1)));
    field.air_far_edge_divisions=r.air_far_edge_divisions.or(r.air_edge_divisions).or(Some(4));
    Ok(field)
}
pub fn run(board:&crate::model::Board,r:&board_mesh::Request,options:Options<'_>)->Result<serde_json::Value,String> {
    run_with_budget(board,r,options,300.)
}
/// Shared cold budget remaining after planning/cache work.
pub fn run_with_budget(board:&crate::model::Board,r:&board_mesh::Request,options:Options<'_>,seconds:f64)->Result<serde_json::Value,String> {
    run_with_frequency_audit(board,r,options,seconds,None)
}

/// An audited production preparation keeps the independent FEM operator live
/// for the adaptive loop. The returned evidence is not a reusable FEM cache.
pub fn run_with_frequency_audit(board:&crate::model::Board,r:&board_mesh::Request,options:Options<'_>,seconds:f64,audit:Option<&frequency_audit::Request>)->Result<serde_json::Value,String> {
    if let Some(request)=audit {
        request.validate()?;
        if options.mode!="prepare"||options.developer_checkpoint.is_some()||options.cached_field.is_some()||r.ports.is_empty()||r.ports.len()>16 {
            return Err("LOCAL_FREQUENCY_AUDIT: production preparation and 1..16 physical ports required".into());
        }
    }
    let seconds_cap=verification_limit("P7_VERIFY_SECONDS",300,1,3600)? as f64;
    let memory_mib=memory_limit_mib(super::roi_fallback::request_memory_mib())?;
    if !seconds.is_finite()||seconds<=0.||seconds>seconds_cap {return Err("LOCAL_DEADLINE: invalid remaining cold budget".into());}
    let started=std::time::Instant::now();
    let requested_jobs=std::env::var("PCB_ANALYSIS_TRAINING_JOBS").ok().map(|v|v.parse::<usize>().map_err(|_|"LOCAL_THREADS: training jobs must be integer1..4".to_string())).transpose()?.unwrap_or(1);
    if requested_jobs>1&&super::parallel::current()!=faer::Par::Seq{return Err("LOCAL_THREADS: select outer training jobs or native kernel parallelism; nested levels are not admitted".into());}
    super::parallel::map_jobs(requested_jobs,0,|_|Ok(()))?;
    let mut stages=serde_json::Map::new();
    let remaining=||std::time::Duration::from_secs_f64(seconds).checked_sub(started.elapsed())
        .filter(|t|!t.is_zero()).ok_or("LOCAL_DEADLINE: shared cold calculation deadline");
    let Options{mode,magnetic_config,magnetic_air_degree,field_request,cached_field,frequencies_hz,developer_checkpoint}=options;
    if ![2,3].contains(&magnetic_air_degree){return Err("MAGNETIC_DEGREE: explicit air2/3 required".into());}
    #[derive(serde::Serialize,serde::Deserialize)]
    struct Prepared {basis:volume_reduction::Basis,model:coupled::Model,training:Vec<serde_json::Value>,training_solutions:Vec<(f64,usize,volume_solve::Solution)>}
    let mut restored=match developer_checkpoint {Some(j)=>j.load::<Prepared>("prepared.json")?,None=>None};
    let resumed=restored.is_some();
    if !["geometry","field","full","train","prepare","verify","solve"].contains(&mode) {return Err("invalid volume preparation mode".into());}
    // The board builder independently reserves2048 bytes/tetra before extrusion.
    // Keep the pre-existing geometry count cap independent of the 2 GiB cap;
    // sparse field/factor/current work must still fit the remaining live bytes.
    let budget=allocation_budget(memory_mib);
    // Explicit independent field refinement uses the same byte preflight as
    // extrusion. Legacy count caps still bound every production/default mesh.
    let field_limits=if r.electric_max_edge_m.is_some(){AllocationBudget{max_nodes:budget.max_bytes/4096,max_tetrahedra:budget.max_bytes/2048,..budget}}else{budget};
    let accepted=if field_request.is_none()&&cached_field.is_none(){Some(accepted_electric_request(r)?)}else{None};
    let field_request=field_request.or(accepted.as_ref());
    let stage=std::time::Instant::now();
    let skin=audit.map(|request|copper_skin_profile::Profile{
        upper_hz:frequency_audit::TRAINING_HZ.into_iter().fold(request.interval_hz[1],f64::max),
        current_step_m:r.max_vertical_step_m,
        field_step_m:field_request.map_or(r.max_vertical_step_m,|field|field.max_vertical_step_m),
    });
    let b=board_mesh::build_profiled(board,r,budget,remaining()?,None,&[],skin)?;
    record_stage(&mut stages,"geometry",stage);
    let field_b=if let Some(fr)=field_request.filter(|_|cached_field.is_none()) {
        if board_mesh::physical_signature(fr)!=board_mesh::physical_signature(r) {return Err("field refinement must preserve physical ROI, copper, bore, materials and finite contacts".into());}
        let fb=board_mesh::build_profiled(board,fr,AllocationBudget{max_bytes:budget.max_bytes.checked_sub(b.mesh.retained_bytes()?).ok_or("field mesh live budget exhausted")?,..field_limits},remaining()?,if r.electric_max_edge_m.is_some(){Some((&b.planar_nodes_mm,&b.planar_triangles))}else{None},&[],skin)?;
        if (fb.mesh.copper_volume_m3-b.mesh.copper_volume_m3).abs()>1e-8*b.mesh.copper_volume_m3||fb.barrel_ids!=b.barrel_ids||fb.source_mapping!=b.source_mapping {return Err("independent field mesh changed physical copper volume or source mapping".into());}Some(fb)
    }else{None};
    let field_mesh=&field_b.as_ref().unwrap_or(&b).mesh;
    eprintln!("common arrangement {} atoms; copper area change {:e}, contact area change {:e}",b.arrangement_atoms,b.max_copper_area_change,b.max_contact_area_change);
    let electrodes=r.electrodes.iter().map(|e|b.electrode_nodes(e)).collect::<Result<Vec<_>,_>>()?;
    let stage=std::time::Instant::now();
    let currents=volume_current::assemble(&b.mesh,&b.conductivity_s_m,&electrodes,budget)?;
    record_stage(&mut stages,"currentAssembly",stage);
    let mut model=serde_json::Value::Null;
    let mut frequency_evidence=None;
    if !electrodes.is_empty()&&!(mode=="geometry") {
        eprintln!("material/current mesh complete: {} potentials, {} currents",currents.potential_count(),currents.current_count());
        let contacts=r.electrodes.iter().zip(&electrodes).map(|(e,n)|(e.name.clone(),currents.node_potential[&n[0]])).collect::<Vec<_>>();
        let stage=std::time::Instant::now();
        let mut basis=match restored.as_mut(){Some(p)=>std::mem::replace(&mut p.basis,volume_reduction::harmonic(&currents,&contacts,budget)?),None=>volume_reduction::harmonic(&currents,&contacts,budget)?};
        if basis.voltage.iter().any(|v|v.len()!=currents.potential_count()||v.iter().any(|x|!x.is_finite()))||basis.current.iter().any(|v|v.len()!=currents.current_count()||v.iter().any(|x|!x.is_finite())) {return Err("developer restored basis shape/nonfinite refusal".into());}
        record_stage(&mut stages,"harmonicBasis",stage);
        eprintln!("harmonic {} voltage / {} current modes",basis.voltage.len(),basis.current.len());
        let field_budget=AllocationBudget{max_bytes:budget.max_bytes.checked_sub(currents.planned_bytes).ok_or("field live budget exhausted")?,..field_limits};
        // Keep the existing sparse FEM factor and solve only requested RHS.
        // ElectricApply still accepts the full original potential space.
        let stage=std::time::Instant::now();
        let (field,field_metadata):(Box<dyn ElectricApply>,serde_json::Value)=if let Some(path)=cached_field {
            let field=volume_field::DenseField::load_checkpoint(&b.mesh,&currents,&board_mesh::physical_signature(&r),path,field_budget)?;
            let metadata=serde_json::json!({"kind":"complete-original-P1-checkpoint","assemblyPeakBytes":field.assembly_peak_bytes,"retainedBytes":field.planned_bytes(),"maxChargeEnergyColumnError":field.max_charge_energy_column_error,"maxDirectedReciprocity":field.max_directed_reciprocity});
            (Box::new(field),metadata)
        }else{
            let field=if r.electric_max_edge_m.is_some(){volume_field::Field::new_surface_refined(field_mesh,&b.mesh,&currents,field_budget,remaining()?)?}else if field_b.is_some(){volume_field::Field::new_vertical_refined(field_mesh,&b.mesh,&currents,field_budget)?}else{volume_field::Field::new(&b.mesh,&currents,field_budget)?};
            let metadata=serde_json::json!({"kind":"sparse-original-P1-requested-RHS","plannedBytes":field.planned_bytes(),"fullSchurMaterialized":false,"surfaceRefinement":r.electric_max_edge_m.map(|h|serde_json::json!({"maximumConstraintEdgeM":h,"trace":"positive source-surface barycentric sampling; nodal charge/energy congruence; current mesh unchanged","nestedCurrentTrace":false}))});
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
        let magnetic_operator=volume_integral_magnetic::Operator::new(&b.mesh,magnetic_budget,12)?;
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
        let full_mode=trains_basis(mode);
        let stage=std::time::Instant::now();
        let solver=if full_mode {Some(volume_solve::Solver::new(&currents,field.as_ref(),&magnetic,budget)?)}else{None};
        record_stage(&mut stages,"fullSolvePreparation",stage);
        let mut training=restored.as_ref().map(|p|p.training.clone()).unwrap_or_default();let mut training_solutions=restored.as_ref().map(|p|p.training_solutions.clone()).unwrap_or_default();
        let stage=std::time::Instant::now();
        if !resumed&&full_mode {
            let full_solver=solver.as_ref().unwrap();
            let tasks=frequency_audit::TRAINING_HZ.into_iter().flat_map(|f|(0..ports.len()).map(move|input|(f,input))).collect::<Vec<_>>();
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
        let mut reserved=solver.as_ref().map_or(operator_live,|s|s.planned_bytes).checked_add(basis_live)
            .and_then(|n|n.checked_add(training_solutions.len().checked_mul(4*currents.potential_count()+2*currents.current_count())?.checked_mul(16)?))
            .and_then(|n|n.checked_add(currents.current_count().checked_mul(64)?))
            .and_then(|n|n.checked_add(1024*1024)).ok_or("P6 live overflow")?;
        let conservative_stage=std::time::Instant::now();
        let mut conservative_current=None;
        let conservative=if mode=="full"&&ports.len()==4&&frequencies_hz.contains(&1000.) {
            let available=budget.max_bytes.checked_sub(reserved).filter(|&n|n>0).ok_or("conservative current live budget exhausted")?;
            let c=ConservativeSpace::new(&b.mesh,&currents,&electrodes,AllocationBudget{max_bytes:available,..budget})?;
            reserved=reserved.checked_add(c.planned_bytes).ok_or("conservative reserved byte overflow")?;
            conservative_current=Some(serde_json::json!({"kind":"rt0-loaded-state-v2","fluxUnits":"A","potentialUnits":"V","phasorConvention":"RMS","nodalReactionPerOutwardFaceFlux":1./3.,"faces":&c.faces,"cardinalHarmonicVoltageBasis":basis.harmonic_electrodes()?,"driveA":[1.,0.,-1.,0.],"loadsOhm":[50.,50.,50.,50.],"states":[],"plannedBytes":c.planned_bytes}));
            Some(c)
        }else{None};
        if conservative.is_some(){record_stage(&mut stages,"conservativeCurrentTopology",conservative_stage);}
        let audit_bytes=audit.map_or(0,|request|request.retained_bytes(ports.len()));
        let p6_bytes=budget.max_bytes.checked_sub(reserved).and_then(|n|n.checked_sub(audit_bytes)).filter(|&n|n>0).ok_or("P6 remaining shared memory budget exhausted")?;
        let p6_budget=crate::electrical::Budget{max_memory_mi_b:Some(p6_bytes as f64/1048576.),max_seconds:Some(remaining()?.as_secs_f64())};
        let stage=std::time::Instant::now();
        let response=if frequencies_hz.is_empty() {None}else{Some(coupled::impedance(&candidate,ports.len(),frequencies_hz,Some(&p6_budget))?)};
        if mode=="prepare" {stages.insert("userP6Solve".into(),serde_json::Value::Null);}else{record_stage(&mut stages,"userP6Solve",stage);}
        let cold_prepare_seconds=started.elapsed().as_secs_f64();
        if let Some(request)=audit {
            let stage=std::time::Instant::now();
            // Leave the remainder of the scenario's deadline available to serialize
            // the prepared model and solve the caller's unchanged fixed output grid.
            let audit_time=remaining()?.mul_f64(0.75);
            let audit_remaining=||audit_time.checked_sub(stage.elapsed())
                .filter(|t|!t.is_zero()).ok_or("LOCAL_FREQUENCY_AUDIT_DEADLINE");
            let full=solver.as_ref().ok_or("LOCAL_FREQUENCY_AUDIT: full operator absent")?;
            let mut reduced=coupled::System::compile(&candidate,ports.len(),Some(&p6_budget))?;
            let nv=candidate.potential_nodes.len();
            let n=nv+candidate.current_dofs.len();
            let mut counters=Counters::default();
            let mut injection=vec![0.;currents.potential_count()];
            let zero=vec![0.;injection.len()];
            let mut drive=vec![0.;ports.len()];
            let mut evidence=frequency_audit::run(request,ports.len(),|frequency| {
                let mut exact=frequency_audit::Observables {
                    impedance_ohm:Vec::with_capacity(ports.len()*ports.len()),
                    total_copper_loss_w:Vec::with_capacity(ports.len()),
                };
                let mut approximate=frequency_audit::Observables {
                    impedance_ohm:Vec::with_capacity(ports.len()*ports.len()),
                    total_copper_loss_w:Vec::with_capacity(ports.len()),
                };
                for (input,&[a,b]) in ports.iter().enumerate() {
                    injection.fill(0.);
                    injection[contacts[a].1]+=1.;
                    injection[contacts[b].1]-=1.;
                    let state=full.solve(frequency,&injection,&zero,audit_remaining()?)?;
                    exact.impedance_ohm.extend(ports.iter().map(|&[a,b]|
                        full.port_voltage(&state,contacts[a].1,contacts[b].1)));
                    exact.total_copper_loss_w.push(currents.losses(&state.real_currents_a)?.into_iter()
                        .chain(currents.losses(&state.imaginary_currents_a)?).sum());
                    drop(state);
                    audit_remaining()?;
                    drive.fill(0.);
                    drive[input]=1.;
                    let mut rhs=reduced.port_rhs(&drive);
                    rhs.resize(2*n,0.);
                    let state=reduced.solve(2.*std::f64::consts::PI*frequency,rhs,&vec![0.;n],&mut counters)?;
                    approximate.impedance_ohm.extend(reduced.port_voltages(&state,ports.len()).into_iter()
                        .zip(reduced.port_voltages(&state[n..],ports.len())).map(|(real,imaginary)|[real,imaginary]));
                    let recovered_loss=|offset:usize|->Result<f64,String>{
                        let recovered:Vec<_>=(0..currents.current_count()).map(|j|basis.current.iter().enumerate()
                            .map(|(k,v)|v[j]*state[offset+nv+k]).sum()).collect();
                        Ok(currents.losses(&recovered)?.into_iter().sum())
                    };
                    approximate.total_copper_loss_w.push(recovered_loss(0)?+recovered_loss(n)?);
                    audit_remaining()?;
                }
                if [&exact,&approximate].into_iter().any(|sample|sample.impedance_ohm.iter().flatten()
                    .chain(&sample.total_copper_loss_w).any(|v|!v.is_finite())) {
                    return Err("LOCAL_FREQUENCY_AUDIT: nonfinite physical observable".into());
                }
                Ok(frequency_audit::Sample {full:exact,reduced:approximate})
            });
            evidence["reducedNumericalFactorizations"]=serde_json::json!(counters.numerical_factorizations);
            evidence["reservedAuditBytes"]=serde_json::json!(audit_bytes);
            frequency_evidence=Some(evidence);
            record_stage(&mut stages,"adaptiveFrequencyAudit",stage);
        }
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
            let drive_templates=if ports.len()==2 {vec![vec![1.,0.],vec![0.,1.],vec![1.,-1.]]}else{(0..ports.len()).map(|p|{let mut d=vec![0.;ports.len()];d[p]=1.;d}).collect::<Vec<_>>()};
            if drive_templates.is_empty(){return Err("full diagnostic requires physical ports".into());}
            let drives=verification_axis(frequencies_hz)?.iter().flat_map(|&f|
                drive_templates.iter().cloned().map(move|drive|(f,drive))).collect::<Vec<_>>();
            for (point,(frequency,drive)) in drives.into_iter().enumerate() {
                let name=format!("point-{point:02}.json");
                if let Some(j)=developer_checkpoint {if let Some(saved)=j.load::<serde_json::Value>(&name)?{if saved["frequencyHz"]!=serde_json::json!(frequency)||saved["drive"]!=serde_json::json!(drive){return Err("developer point mapping mismatch".into());}full_checks.push(saved);continue;}}
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
                let potential=|shape:&[f64],correction:&[f64],constants:&[f64]|{let mut v:Vec<f64>=shape.iter().zip(correction).map(|(a,b)|a+b).collect();for(component,nodes)in currents.components.iter().enumerate(){for&node in nodes{v[node]+=constants[component];}}v};
                let full_potential_real=potential(&full.real_shape_v,&full.real_shape_correction_v,&full.real_constants_v);
                let full_potential_imaginary=potential(&full.imaginary_shape_v,&full.imaginary_shape_correction_v,&full.imaginary_constants_v);
                let check=serde_json::json!({"drive":drive,"frequencyHz":frequency,"audit":&full.audit,"fullPortVoltages":full_ports,"reducedPortReal":reduced.port_voltages(&x,ports.len()),"reducedPortImaginary":reduced.port_voltages(&x[reduced_n..],ports.len()),"maxCurrentCoordinateError":state_error,"maxCellLossError":cell_loss_error,"fullLossW":full_loss,"reducedLossW":reduced_loss,"worstCurrent":{"index":worst,"axis":worst%3,"full":[full.real_currents_a[worst],full.imaginary_currents_a[worst]],"recovered":[rr[worst],ri[worst]],"weakFloor":1e-12*scale},"fullPotentialRealV":full_potential_real,"fullPotentialImaginaryV":full_potential_imaginary,"fullCurrentReal":&full.real_currents_a,"fullCurrentImaginary":&full.imaginary_currents_a,"reducedCurrentReal":&rr,"reducedCurrentImaginary":&ri,"reducedState":x});
                full_checks.push(check);
                if let Some(j)=developer_checkpoint {j.save(&name,full_checks.last().unwrap())?;}
            }
        }
        record_stage(&mut stages,"developerVerification",stage);
        if let (Some(c),Some(packet))=(conservative.as_ref(),conservative_current.as_mut()){
            if verification_failure.is_none(){
                let stage=std::time::Instant::now();let drive=[1.,0.,-1.,0.];let loads=[50.;4];
                let dc=loaded_dc(&currents,&basis,&candidate,&ports,&contacts,c,&drive,&loads,&p6_budget)?;
                let ac=loaded_ac(&currents,&basis,&candidate,c,&full_checks,&drive,&loads,&p6_budget)?;
                packet["states"]=serde_json::json!([dc,ac]);record_stage(&mut stages,"conservativeLoadedStateRecovery",stage);
            }
        }
        model=serde_json::json!({"admission":"accepted-electric-v1-full-model-verification-pending","coldPrepareSeconds":cold_prepare_seconds,"fieldOperator":field_metadata,"magneticActionReuse":magnetic.stats(),"magneticNumericalLinearity":magnetic_linearity,"model":candidate,"response":response,"training":training,"fullChecks":full_checks,"verificationStatus":if verification_failure.is_some(){"incomplete"}else if mode=="verify"||mode=="full"{"complete"}else{"not-run"},"verificationFailure":verification_failure,"resumedPrepared":resumed,"voltageBasis":basis.voltage,"currentBasis":basis.current});
        if let Some(current)=conservative_current{model["conservativeCurrent"]=current;}
    }
    let source_ids:Vec<_>=b.mesh.copper_tetrahedra.iter().map(|c|&c.source_ids).collect();
    let nodes:Vec<_>=b.mesh.copper_tetrahedra.iter().map(|c|c.nodes).collect();
    let mut output=serde_json::json!({"request":r,"cellResistanceOhm":currents.cells.iter().map(|c|c.resistance_ohm).collect::<Vec<_>>(),"cellDensityPerAmpM2":currents.cells.iter().map(|c|c.density_per_amp_m2).collect::<Vec<_>>(),"candidate":model,"arrangement":{"atoms":b.arrangement_atoms,"maxCopperAreaChange":b.max_copper_area_change,"maxContactAreaChange":b.max_contact_area_change,"sourceCopperRestorations":b.source_copper_restorations},"electrodeAreasM2":b.electrode_areas_m2,"electrodeNodes":electrodes,"nodesM":b.mesh.nodes_m,"copperCells":nodes,"sourceIds":source_ids,"barrels":b.barrel_ids,"mapping":b.source_mapping,"originMm":b.origin_mm,"potentialCount":currents.potential_count(),"currentCount":currents.current_count(),"potentialMeshNodes":currents.mesh_nodes,"components":currents.components,"magneticMeshTetrahedra":b.mesh.tetrahedra.len()+b.mesh.copper_tetrahedra.len(),"fieldNodesM":field_mesh.nodes_m,"fieldCopperCells":field_mesh.copper_tetrahedra.iter().map(|t|t.nodes).collect::<Vec<_>>(),"fieldTetrahedra":field_mesh.tetrahedra.len(),"fieldCells":field_mesh.tetrahedra.iter().map(|t|t.nodes).collect::<Vec<_>>(),"fieldEpsilonR":field_mesh.tetrahedra.iter().map(|t|t.epsilon_r).collect::<Vec<_>>(),"externalFaces":field_mesh.external_faces,"copperVolumeM3":b.mesh.copper_volume_m3,"fieldVolumeM3":field_mesh.dielectric_volume_m3,"plannedBytes":currents.planned_bytes,"elapsedSeconds":b.elapsed_seconds});
    if let Some(path)=cached_field {for key in ["fieldNodesM","fieldCopperCells","fieldTetrahedra","fieldCells","fieldEpsilonR","fieldVolumeM3"]{output.as_object_mut().unwrap().remove(key);}output["fieldDataSource"]=serde_json::json!({"kind":"complete-source-P1-Schur-checkpoint","path":path,"geometry":"stored producer evidence; no fabricated field grid in this output","physicalAdmission":false});}
    if !(mode=="verify") {remaining()?;}
    if let Some(evidence)=frequency_evidence {output["frequencyAudit"]=evidence;}
    if let (Some(profile),Some(request))=(skin,audit) {
        output["copperSkinMesh"]=serde_json::json!({
            "version":copper_skin_profile::VERSION,"requestedUpperHz":request.interval_hz[1],
            "effectiveUpperHz":profile.upper_hz,"skinDepthDivisor":3,
            "current":b.copper_skin_layers,"electric":field_b.as_ref().unwrap_or(&b).copper_skin_layers,
            "currentRoundBarrels":b.copper_skin_barrels,
            "electricRoundBarrels":field_b.as_ref().unwrap_or(&b).copper_skin_barrels,
            "scope":"Planar copper through-thickness and round-barrel radial mesh screening only; lateral, proximity and spatial accuracy remain unverified"
        });
    }
    output["stageSeconds"]=serde_json::Value::Object(stages);
    output["wallSeconds"]=serde_json::json!(started.elapsed().as_secs_f64());
    Ok(output)
}

/// Explicit developer-only limits; production defaults remain unchanged.
/// Memory limit of a local preparation in MiB. `P7_VERIFY_MEMORY_MIB`, when set, takes precedence over the request
/// budget (`budget.maxMemoryMiB`); without either the earlier 2048 MiB default stays.
pub(super) fn memory_limit_mib(requested:Option<f64>)->Result<usize,String>{
    if std::env::var_os("P7_VERIFY_MEMORY_MIB").is_some() {return verification_limit("P7_VERIFY_MEMORY_MIB",2048,256,8192);}
    Ok(match requested {Some(m) if m.is_finite()&&m>=256. => m.floor() as usize,_=>2048})
}
pub(super) fn verification_limit(name:&str,default:usize,min:usize,max:usize)->Result<usize,String>{
    let value=std::env::var(name).ok().map(|s|s.parse::<usize>().map_err(|_|format!("LOCAL_BUDGET: invalid {name}"))).transpose()?.unwrap_or(default);
    if value<min||value>max{return Err(format!("LOCAL_BUDGET: {name} outside {min}..{max}"));}Ok(value)
}

fn trains_basis(mode:&str)->bool{matches!(mode,"full"|"train"|"prepare"|"verify"|"solve")}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn request_memory_sets_the_domain_budget(){
        if std::env::var_os("P7_VERIFY_MEMORY_MIB").is_some() {return;}
        assert_eq!(memory_limit_mib(Some(6144.)).unwrap(),6144);
        assert_eq!(allocation_budget(memory_limit_mib(Some(6144.)).unwrap()).max_bytes,6144*1024*1024-crate::extraction::local::parallel::stack_reservation());
        assert_eq!(memory_limit_mib(None).unwrap(),2048);
    }
    use crate::electrical::frequency::math::C;
    #[test]
    fn full_mode_trains_the_reduced_basis(){
        for mode in ["full","train","prepare","verify","solve"]{assert!(trains_basis(mode),"mode={mode}");}
        for mode in ["geometry","field"]{assert!(!trains_basis(mode),"mode={mode}");}
    }
    #[test]
    fn loaded_dc_gauge_replays_the_saved_ill_conditioned_matrix(){
        let conductance=vec![
            vec![32.22039867437215,-32.22039867435913,0.,0.,0.,0.],
            vec![-32.220398674391106,32.22039867439511,0.,0.,0.,0.],
            vec![0.,0.,147.70531808524322,-147.70531808527502,0.,0.],
            vec![0.,0.,-147.70531808541276,147.70531808535986,0.,0.],
            vec![0.,0.,0.,0.,32.220398674321736,-32.22039867432234],
            vec![0.,0.,0.,0.,-32.220398674403306,32.22039867440122],
        ];
        let mut incidence=vec![vec![0.;4];6];for(p,(a,b))in[(0,2),(4,2),(1,3),(5,3)].into_iter().enumerate(){incidence[a][p]=1.;incidence[b][p]=-1.;}
        let first=anchored_loaded_dc(&conductance,&incidence,&[1.,0.,-1.,0.],&[50.;4],0).unwrap();
        let second=anchored_loaded_dc(&conductance,&incidence,&[1.,0.,-1.,0.],&[50.;4],1).unwrap();
        let ports=|v:&[f64]|(0..4).map(|p|(0..6).map(|i|incidence[i][p]*v[i]).sum::<f64>()).collect::<Vec<_>>();
        let(a,b)=(ports(&first.0),ports(&second.0));let scale=a.iter().map(|v|v.abs()).fold(1e-30,f64::max);
        let error=a.iter().zip(b).map(|(a,b)|(a-b).abs()).fold(0.,f64::max)/scale;
        assert!((error-3.5875160555196278e-9).abs()<1e-10,"error={error:e}");
    }
    #[test]
    fn loaded_ac_coefficients_use_one_complex_rhs_column_without_panic(){
        let u=vec![vec![C::new(1.,0.2),C::new(0.2,-0.1)],vec![C::new(-0.1,0.05),C::new(0.8,0.3)]];let drive=[1.,-0.4];let loads=[50.,75.];
        let a=loaded_ac_coefficients(&u,&drive,&loads).unwrap();assert_eq!(a.len(),2);assert!(a.iter().any(|v|v.imaginary.abs()>1e-6));
        for q in 0..2{let port=(0..2).map(|p|u[q][p]*a[p]).fold(C::default(),|x,y|x+y);let actual=a[q]+port.scale(1./loads[q]);assert!((actual.real-drive[q]).abs()<1e-12&&actual.imaginary.abs()<1e-12);}
        assert!(loaded_ac_coefficients(&u[..1],&drive,&loads).is_err());
    }
    #[test]
    fn loaded_ac_coefficients_retain_small_displacement_solution(){
        // Exact saved F64 unit-port response: large physical displacement
        // voltages make naive residual refinement lose the small loaded answer.
        let u=vec![
            vec![C::new(0.010379297598521475,-531407926.6536519),C::new(0.001966568193960444,-142288239.6626979),C::new(-0.00866400602448398,-531407926.65365434),C::new(-0.0016010056684434358,-142288239.66269863)],
            vec![C::new(0.0019665681939604416,-142288239.6626979),C::new(0.011257380073307798,-605657485.0379196),C::new(-0.0015872420435947868,-142288239.66269866),C::new(-0.008381316362861099,-605657485.0379227)],
            vec![C::new(-0.008664006024483986,-531407926.65365434),C::new(-0.001587242043594787,-142288239.66269863),C::new(0.010099166491953734,-531407926.6536519),C::new(0.0016154210340939693,-142288239.66269797)],
            vec![C::new(-0.0016010056684434364,-142288239.66269863),C::new(-0.008381316362861093,-605657485.0379226),C::new(0.0016154210340939697,-142288239.662698),C::new(0.009786463494481831,-605657485.03792)],
        ];
        // 90-digit reference with the exact physical load on these F64 columns.
        let expected=[C::new(0.9996220826959653,-4.88371205639137e-8),C::new(-6.765120696788148e-5,-1.369164918839761e-8),C::new(-0.9996220826959654,4.883691895763385e-8),C::new(6.765120696798639e-5,1.3691418212867497e-8)];
        let actual=loaded_ac_coefficients_extended(&u,&[1.,0.,-1.,0.],&[50.;4]).unwrap();
        assert!(actual.iter().any(|a|a.lo.real!=0.||a.lo.imaginary!=0.));
        for(a,e)in actual.iter().zip(expected){let a=a.value();assert!((a.real-e.real).abs()<1e-12&&(a.imaginary-e.imaginary).abs()<1e-12,"actual={a:?} expected={e:?}");}
    }
    #[test]
    fn loaded_ac_port_voltages_match_exact_load_on_saved_joint_columns(){
        // D1 slot joint full-space 1 kHz columns in physical port order.
        // Reference is a 90-digit solve of (I + Z/50) i = d from these
        // exact F64 columns, followed by V = Z i.
        let z=vec![
            vec![C::new(0.010378877787982686,-531480920.15353894), C::new(0.0019648847445566543,-142340205.24015158), C::new(-0.008663961887874156,-531480920.1535417), C::new(-0.0016016548413945605,-142340205.24015224)],
            vec![C::new(0.001964884744556654,-142340205.24015158), C::new(0.011261309690860739,-606356182.7796291), C::new(-0.001586852435774449,-142340205.24015233), C::new(-0.00838062688611846,-606356182.7796322)],
            vec![C::new(-0.008663961887874154,-531480920.15354156), C::new(-0.0015868524357744466,-142340205.2401523), C::new(0.010099674576460129,-531480920.1535391), C::new(0.0016168449149091054,-142340205.24015158)],
            vec![C::new(-0.0016016548413945587,-142340205.2401523), C::new(-0.00838062688611846,-606356182.7796322), C::new(0.001616844914909106,-142340205.24015167), C::new(0.009783912829791401,-606356182.7796293)],
        ];
        let expected=[
            C::new(0.018895865202113404,2.590755035657824e-06),
            C::new(0.00338256034667187,6.845642278473417e-07),
            C::new(-0.018895865202126952,-2.5907449516863768e-06),
            C::new(-0.0033825603466736,-6.845528649980224e-07),
        ];
        let coefficients=loaded_ac_coefficients_extended(&z,&[1.,0.,-1.,0.],&[50.;4]).unwrap();
        assert!(coefficients.iter().any(|x|x.lo.real!=0.||x.lo.imaginary!=0.));
        for (port,(row,expected)) in z.iter().zip(expected).enumerate(){
            let actual=accurate_complex_dot(row,&coefficients).unwrap();
            let relative=(actual-expected).abs()/expected.abs().max(1e-12);
            assert!(relative<=1e-8,"port={port} relative={relative:e} actual={actual:?} expected={expected:?}");
        }
    }
    #[test]
    fn loaded_complex_fields_compensate_cancellation_before_rounding(){
        let checks=vec![serde_json::json!({"r":[1e16],"i":[0.]}),serde_json::json!({"r":[1.],"i":[0.]}),serde_json::json!({"r":[-1e16],"i":[0.]}),serde_json::json!({"r":[0.],"i":[0.]})];let refs=checks.iter().collect::<Vec<_>>();
        let(real,imaginary)=complex_fields(&refs,"r","i",&[C::new(1.,0.);4]).unwrap();assert_eq!(real,[1.]);assert_eq!(imaginary,[0.]);
    }
    #[test]
    fn loaded_complex_fields_retain_saved_displacement_cancellation(){
        let checks=vec![
            serde_json::json!({"r":[-0.007130298083237795],"i":[-437067492.8601971]}),
            serde_json::json!({"r":[-9.526983875073045e-05],"i":[-32077456.540122166]}),
            serde_json::json!({"r":[0.00854696349329805],"i":[-437067492.8601941]}),
            serde_json::json!({"r":[0.0002064724162030045],"i":[-32077456.540122345]}),
        ];let refs=checks.iter().collect::<Vec<_>>();
        let coefficients=[C::new(0.9996220826959655,-4.8390419772161816e-8),C::new(-6.765120696788671e-5,-1.3719562713476454e-8),C::new(-0.9996220826959655,4.839021816588197e-8),C::new(6.765120696798915e-5,1.371933173794634e-8)];
        let(real,imaginary)=complex_fields(&refs,"r","i",&coefficients).unwrap();
        // Decimal replay of these exact saved F64 coefficients, not a solve with
        // unrepresentable higher-precision coefficients, gives these values.
        assert_eq!(real,[-0.01576684111382995]);
        assert_eq!(imaginary,[-3.0412235349060057e-6]);
    }
}
