//! Direct descriptor evaluation keeps differential terminal semantics. A generic
//! infinity block is not silently advertised as a common-return tabulated block.
use super::*;
use crate::electrical::frequency::{math, Request};
pub fn calculate(request: &Request) -> Result<Value> {
    if request.recover_instance_loss || request.instances.len() != 1 || !request.connections.is_empty() || !request.instance_inductance_h.is_empty() {
        return assembled(request);
    }
    calculate_direct(request, None)
}

fn calculate_direct(request: &Request, owner_loss: Option<&LossPartition>) -> Result<Value> {
    let instance = &request.instances[0];
    let resource = &instance.model;
    let ports = resource.ports.len();
    if resource.kind != "network"
        || resource.id.trim().is_empty()
        || ports == 0
        || ports > 16
        || resource.ports.iter().any(|v| v.trim().is_empty())
        || resource.ports.iter().collect::<BTreeSet<_>>().len() != ports
        || instance.ports.len() != ports
        || request.ports.len() != ports
        || request
            .ports
            .iter()
            .enumerate()
            .any(|(i, p)| p.index != i || p.name.is_empty())
        || request
            .ports
            .iter()
            .map(|p| &p.name)
            .collect::<BTreeSet<_>>()
            .len()
            != ports
        || resource
            .analyses
            .as_ref()
            .is_some_and(|a| !a.iter().any(|v| v == "networkParameters"))
    {
        return Err(fail(
            "COUPLED_PORT",
            "Direct descriptor requires a complete ordered port mapping and authorized Resource",
        ));
    }
    let conditional = match resource
        .validity
        .as_ref()
        .and_then(|v| v["status"].as_str())
    {
        None | Some("valid") => false,
        Some("conditional") => true,
        _ => return Err(fail("COUPLED_VALIDITY", "Unavailable or invalid Resource")),
    };
    let band = if let Some(domain) = resource
        .validity
        .as_ref()
        .and_then(|v| v["domain"].get("bandHz"))
    {
        let band: Vec<f64> =
            serde_json::from_value(domain.clone()).map_err(|e| fail("COUPLED_BAND", e))?;
        if band.len() != 2
            || !band[0].is_finite()
            || !band[1].is_finite()
            || band[0] < 0.
            || band[1] < band[0]
            || request
                .frequencies
                .iter()
                .any(|f| *f < band[0] || *f > band[1])
        {
            return Err(fail(
                "COUPLED_BAND",
                "Requested frequency outside declared model validity band",
            ));
        }
        band
    } else {
        vec![0., f64::MAX]
    };
    let model: Model =
        serde_json::from_value(resource.data.clone()).map_err(|e| fail("COUPLED_INPUT", e))?;
    let refs = if request.representation == "S" {
        let refs = request.reference_ohm.as_ref().ok_or_else(|| {
            fail(
                "COUPLED_REFERENCE",
                "S output requires explicit positive real references",
            )
        })?;
        if refs.len() != ports || refs.iter().any(|v| !v.is_finite() || *v <= 0.) {
            return Err(fail("COUPLED_REFERENCE", "Invalid power-wave references"));
        }
        refs.clone()
    } else {
        if request.reference_ohm.is_some() || !["Z", "Y"].contains(&request.representation.as_str())
        {
            return Err(fail(
                "COUPLED_REFERENCE",
                "Z/Y forbid wave references; unsupported representation",
            ));
        }
        vec![]
    };
    let mut result = if request.representation == "S" {
        matched_waves(&model, &refs, &request.frequencies, request.budget.as_ref(), owner_loss)?
    } else { impedance(&model, ports, &request.frequencies, request.budget.as_ref())? };
    let mut quality = crate::electrical::frequency::Quality::default();
    if request.representation == "Y" {
        let z: Vec<math::Matrix> = serde_json::from_value(result["parameters"].clone())
            .map_err(|e| fail("COUPLED_NONFINITE", e))?;
        let converted = z
            .iter()
            .map(|z| {
                if request.representation == "Y" {
                    math::solve(z, &math::eye(ports), &mut quality)
                } else {
                    math::to_s(z, "Z", &refs, &mut quality)
                }
            })
            .collect::<Result<Vec<_>>>()?;
        result["parameters"] = json!(converted);
    }
    result["representation"] = json!(request.representation);
    result["referenceOhm"] = if refs.is_empty() {
        Value::Null
    } else {
        json!(refs)
    };
    result["diagnostics"] = json!([]);
    result["ports"] = json!(request.ports);
    result["nature"] = json!("passive");
    result["conditional"] = json!(conditional);
    result["bandHz"] = json!(band);
    result["interpolation"] = json!([]);
    result["provenance"] = json!([{"instanceId":instance.id,"resource":resource,"mapping":instance.ports,"sourcePorts":resource.ports,"sourceRepresentation":"coupled-rlc","sourceValidity":resource.validity,"allowedAnalyses":resource.analyses,"sourceReferenceOhm":null,"bandHz":band,"nature":"passive","normalization":{"physicalDescriptor":true,"referenceTopology":model.reference_topology}}]);
    result["conversionQuality"] = json!(quality);
    result["validityLimitations"]=json!("Exact evaluation of supplied full RLC model inside declared validity band; no PCB extraction accuracy inferred. Physical terminal mapping retained, no implicit common return.");
    Ok(result)
}
/// Same physical conductor composer as time, with frequency permissions/bands.
fn assembled(request: &Request) -> Result<Value> {
    let start = Instant::now();
    if request
        .instances
        .iter()
        .any(|i| i.model.data["format"] != "coupled-rlc")
    {
        return Err(fail(
            "COUPLED_TERMINAL_MAP",
            "Physical descriptor composition requires coupled models with known terminal traces",
        ));
    }
    let bands = request
        .instances
        .iter()
        .map(|i| crate::electrical::frequency::declared_band(&i.model))
        .collect::<Result<Vec<_>>>()?;
    let band = [
        bands.iter().map(|b| b[0]).fold(0., f64::max),
        bands.iter().map(|b| b[1]).fold(f64::MAX, f64::min),
    ];
    if request
        .frequencies
        .iter()
        .any(|f| *f < band[0] || *f > band[1])
    {
        return Err(fail(
            "COUPLED_BAND",
            "Requested point outside source band intersection",
        ));
    }
    let topology = composition::Topology {
        instances: request.instances.clone(),
        connections: request.connections.clone(),
        connection_contacts: request.connection_contacts.clone(),
        ports: request.ports.clone(),
        instance_inductance_h: request.instance_inductance_h.clone(),
    };
    let built = composition::build_for(
        &topology,
        request.budget.as_ref(),
        start,
        "networkParameters",
    )?;
    let outputs = request
        .frequencies
        .len()
        .saturating_mul(built.ports.len().saturating_pow(2))
        .saturating_mul(256);
    let (loss_mapping_bytes, loss_output_bytes) = if request.recover_instance_loss {
        (
            LossPartition::mapping_bytes(request, &built),
            LossPartition::output_bytes(request.instances.len(), built.ports.len(), request.frequencies.len()),
        )
    } else {
        (0, 0)
    };
    let retained = built.source_memory.saturating_add(loss_mapping_bytes);
    composition::remaining_budget(
        request.budget.as_ref(),
        retained.saturating_add(built.assembly_plan).saturating_add(outputs).saturating_add(loss_output_bytes),
    )?;
    let owner_loss = request.recover_instance_loss.then(|| LossPartition::from_assembly(request, &built)).transpose()?;
    let mut inner = request.clone();
    inner.instances = vec![crate::electrical::frequency::Instance {
        id: "assembled".into(),
        ports: built.ports.clone(),
        model: crate::electrical::frequency::Resource {
            id: "assembled".into(),
            kind: "network".into(),
            ports: built.ports.clone(),
            data: json!(built.model),
            source: Some("Physical conductor congruence assembly".into()),
            validity: None,
            analyses: Some(vec!["networkParameters".into()]),
        },
    }];
    inner.connections.clear();
    inner.connection_contacts = None;
    inner.instance_inductance_h.clear(); // Already inserted into the assembled descriptor exactly once.
    inner.ports = inner
        .ports
        .iter()
        .enumerate()
        .map(|(i, p)| crate::electrical::frequency::OutputPort {
            name: p.name.clone(),
            index: i,
        })
        .collect();
    let mut budget = composition::remaining_budget(request.budget.as_ref(), retained)?;
    budget.max_seconds = Some(
        request
            .budget
            .as_ref()
            .and_then(|b| b.max_seconds)
            .unwrap_or(120.)
            - start.elapsed().as_secs_f64(),
    );
    if budget.max_seconds.unwrap() <= 0. {
        return Err(fail(
            "NETWORK_DEADLINE",
            "Physical assembly exhausted shared deadline",
        ));
    }
    inner.budget = Some(budget);
    let mut result = calculate_direct(&inner, owner_loss.as_ref())?;
    result["physicalRecovery"] = json!(built.recovery);
    result["assembledModel"] = json!(built.model);
    result["conditional"] = json!(built.conditional);
    result["bandHz"] = json!(band);
    result["provenance"]=json!(request.instances.iter().zip(bands).map(|(i,b)|json!({"instanceId":i.id,"resource":i.model,"mapping":i.ports,"sourcePorts":i.model.ports,"sourceRepresentation":"coupled-rlc","sourceValidity":i.model.validity,"allowedAnalyses":i.model.analyses,"sourceReferenceOhm":null,"bandHz":b,"nature":"passive","normalization":{"physicalDescriptor":true,"referenceTopology":i.model.data["referenceTopology"]}})).collect::<Vec<_>>());
    result["memoryPlanBytes"] = json!(
        result["memoryPlanBytes"]
            .as_u64()
            .unwrap_or(0)
            .max(built.assembly_plan as u64)
            + retained as u64
            + outputs as u64
    );
    result["validityLimitations"]=json!("Physical conductor weld of block-diagonal supplied models; absent intermodel mutual C/L is not extracted. Close coupled objects require a jointly extracted model. AC increments do not determine initial common charge.");
    Ok(result)
}
/// Convert only explicit grounded canonical descriptors at the requested points.
/// Their reference loads define S, not additional physical loads in composition.
/// All subsequent pair joins use the existing passive wave composition backend.
pub(crate) fn common_tables(request: &Request, start: Instant) -> Result<(Request, usize, f64)> {
    if request.connection_contacts.is_some()
        || request
            .instances
            .iter()
            .any(|i| i.model.data.get("portTerminals").is_some())
    {
        return Err(fail(
            "COUPLED_TERMINAL_MAP",
            "Delay/common wave ports lack physical return/charge trace for mapped joins",
        ));
    }
    let p = request
        .instances
        .iter()
        .map(|i| i.ports.len())
        .sum::<usize>();
    let f = request.frequencies.len();
    let n = request.ports.len();
    if request.instances.is_empty()
        || request.instances.len() > 32
        || p > 128
        || n == 0
        || n > 16
        || f == 0
        || f > 100000
        || f.saturating_mul(n * n) > 131072
        || f.saturating_mul(p * p * p) > 100000000
        || request.instances.iter().any(|i| {
            i.model.ports.is_empty()
                || i.model.ports.len() > 16
                || f.saturating_mul(i.model.ports.len().pow(2)) > 262144
        })
        || request
            .frequencies
            .iter()
            .enumerate()
            .any(|(i, x)| !x.is_finite() || *x < 0. || i > 0 && *x <= request.frequencies[i - 1])
    {
        return Err(fail(
            "NETWORK_BUDGET",
            "Mixed analytic frequency output/work limits",
        ));
    }
    let elements = request
        .instances
        .iter()
        .map(|i| f * i.model.ports.len().pow(2))
        .sum::<usize>();
    if elements > 524288 {
        return Err(fail(
            "NETWORK_BUDGET",
            "Mixed frequency aggregate source elements",
        ));
    }
    let reserved = request
        .instances
        .iter()
        .map(|i| composition::source_plan(&i.model))
        .sum::<usize>()
        .saturating_add(elements.saturating_mul(576));
    let mut budget = composition::remaining_budget(request.budget.as_ref(), reserved)?;
    budget.max_seconds = Some(
        request
            .budget
            .as_ref()
            .and_then(|b| b.max_seconds)
            .unwrap_or(60.),
    );
    let mut normalized = request.clone();
    normalized.budget = Some(budget.clone());
    let mut residual = 0_f64;
    for instance in &mut normalized.instances {
        super::super::deadline(start, Some(&budget), "Mixed descriptor frequency admission")?;
        let resource = &mut instance.model;
        if resource.data["format"] != "coupled-rlc" {
            continue;
        }
        if resource.kind != "network"
            || resource
                .analyses
                .as_ref()
                .is_some_and(|a| !a.iter().any(|s| s == "networkParameters"))
        {
            return Err(fail(
                "COUPLED_RESOURCE",
                "Descriptor does not authorize frequency calculation",
            ));
        }
        match resource
            .validity
            .as_ref()
            .and_then(|v| v["status"].as_str())
        {
            None | Some("valid") | Some("conditional") => {}
            _ => return Err(fail("COUPLED_VALIDITY", "Unavailable descriptor")),
        }
        let band = crate::electrical::frequency::declared_band(resource)?;
        if request
            .frequencies
            .iter()
            .any(|f| *f < band[0] || *f > band[1])
        {
            return Err(fail(
                "COUPLED_BAND",
                "Frequency outside producer applicability",
            ));
        }
        let model: Model =
            serde_json::from_value(resource.data.clone()).map_err(|e| fail("COUPLED_INPUT", e))?;
        if model.reference_topology != "grounded" {
            return Err(fail(
                "COUPLED_TERMINAL_MAP",
                "Mixed infinity descriptor joins require explicit physical terminal mapping",
            ));
        }
        let ports = resource.ports.len();
        for port in 0..ports {
            let entries: Vec<_> = model
                .port_incidence
                .iter()
                .filter(|(_, j, v)| *j == port && *v != 0.)
                .collect();
            if entries.len() != 1 || entries[0].2 != 1. {
                return Err(fail(
                    "COUPLED_TERMINAL_MAP",
                    "Mixed frequency requires canonical positive-potential ports",
                ));
            }
        }
        let mut remaining=budget.clone();remaining.max_seconds=Some(budget.max_seconds.unwrap_or(60.)-start.elapsed().as_secs_f64());
        if remaining.max_seconds.unwrap()<=0.{return Err(fail("NETWORK_DEADLINE","Mixed matched wave deadline"));}
        let waves=matched_waves(&model,&vec![50.;ports],&request.frequencies,Some(&remaining),None)?;
        residual=residual.max(waves["quality"]["relativeResidual"].as_f64().unwrap_or(0.));
        let parameters=&waves["parameters"];
        resource.data = json!({"format":"network-parameters","representation":"S","referenceTopology":"common","nature":"passive","referenceOhm":50.,"frequenciesHz":request.frequencies,"parameters":parameters});
    }
    super::super::deadline(
        start,
        Some(&budget),
        "Mixed descriptor frequency normalization",
    )?;
    Ok((normalized, reserved, residual))
}

/// Potential coordinates weld during assembly. Current coordinates stay in
/// their source blocks, including both endpoints of every intrinsic R entry.
struct LossPartition {
    instance_ids: Vec<String>,
    owner_by_dof: Vec<usize>,
}

impl LossPartition {
    fn mapping_bytes(request: &Request, built: &composition::Built) -> usize {
        // Fixed-capacity Vecs, cloned map/report IDs and conservative allocator
        // overhead. Reserve this while the assembled descriptor remains live.
        request.instances.iter().fold(
            std::mem::size_of::<Self>().saturating_add(128).saturating_add(
                built.model.current_dofs.len().saturating_mul(std::mem::size_of::<usize>()),
            ),
            |bytes, instance| bytes
                .saturating_add(std::mem::size_of::<String>())
                .saturating_add(instance.id.len().saturating_mul(2))
                .saturating_add(64),
        )
    }

    fn from_assembly(request: &Request, built: &composition::Built) -> Result<Self> {
        let invalid = || fail("COUPLED_LOSS_MAP", "Missing or overlapping physical resistance ownership");
        let instance_ids: Vec<_> = request.instances.iter().map(|instance| instance.id.clone()).collect();
        let nv = built.model.potential_nodes.len();
        let mut owner_by_dof = vec![usize::MAX; built.model.current_dofs.len()];
        for current in &built.recovery.currents {
            let slot = current.index.checked_sub(nv)
                .and_then(|index| owner_by_dof.get_mut(index)).ok_or_else(invalid)?;
            if *slot != usize::MAX { return Err(invalid()); }
            *slot = instance_ids.iter().position(|id| id == &current.instance_id).ok_or_else(invalid)?;
        }
        if owner_by_dof.contains(&usize::MAX)
            || built.model.resistance_ohm.iter().any(|&(i, j, _)| owner_by_dof.get(i).is_none_or(|owner| owner_by_dof.get(j) != Some(owner)))
        {
            return Err(invalid());
        }
        Ok(Self { instance_ids, owner_by_dof })
    }

    fn output_bytes(owners: usize, ports: usize, points: usize) -> usize {
        // Matrix numbers plus row/container allocation and JSON construction.
        owners.saturating_mul(points).saturating_mul(ports.saturating_pow(2).saturating_add(1)).saturating_mul(128)
    }

    fn grams(&self, model: &Model, states: &[Vec<f64>], frequency: f64, start: Instant, budget: Option<&Budget>) -> Result<Vec<Vec<Vec<f64>>>> {
        let ports = states.len();
        let nv = model.potential_nodes.len();
        let n = nv + model.current_dofs.len();
        let mut grams = vec![vec![vec![0.; ports]; ports]; self.instance_ids.len()];
        let mut sums = vec![0.; self.instance_ids.len()];
        for a in 0..ports {
            for b in a..ports {
                super::super::deadline(start, budget, "Matched resistance loss recovery")?;
                sums.fill(0.);
                for &(i, j, r) in &model.resistance_ohm {
                    let re = states[a][nv+i] * states[b][nv+j];
                    let im = if frequency == 0. { 0. } else { states[a][n+nv+i] * states[b][n+nv+j] };
                    sums[self.owner_by_dof[i]] += r * (re + im);
                }
                if sums.iter().any(|v| !v.is_finite()) {
                    return Err(fail("COUPLED_NONFINITE", "Matched per-instance intrinsic loss overflow"));
                }
                for (matrix, &value) in grams.iter_mut().zip(&sums) {
                    matrix[a][b] = value;
                    matrix[b][a] = value;
                }
            }
        }
        Ok(grams)
    }
}

/// Evaluate the same S boundary condition directly, without an ill-conditioned
/// unloaded monopole Z intermediate. The intrinsic model is never modified.
fn matched_waves(model:&Model,refs:&[f64],frequencies:&[f64],budget:Option<&Budget>,owner_loss:Option<&LossPartition>)->Result<Value>{
 let started=Instant::now();let ports=refs.len();
 if ports==0||ports>16||frequencies.is_empty()||frequencies.len()>131072/ports/ports||frequencies.iter().enumerate().any(|(i,f)|!f.is_finite()||*f<0.||i>0&&*f<=frequencies[i-1]){return Err(fail("COUPLED_INPUT","Invalid or oversized frequency axis"));}
 let mut system=System::compile(model,ports,budget)?;
 let output=frequencies.len()*ports*ports*128;
 let loss_output=owner_loss.map_or(0,|partition|LossPartition::output_bytes(partition.instance_ids.len(),ports,frequencies.len()));
 let load_bound=(0..ports).map(|p|system.b.keys().filter(|(_,j)|*j==p).count().saturating_pow(2)).sum::<usize>().saturating_mul(64);
 let opposite_state=if owner_loss.is_some(){system.linear.n.saturating_mul(16)}else{0};
 let plan=system.memory_plan.saturating_add(output).saturating_add(loss_output).saturating_add(opposite_state).saturating_add(load_bound).saturating_add(system.linear.n.saturating_mul(ports).saturating_mul(16));
 if plan as f64>budget.and_then(|b|b.max_memory_mi_b).unwrap_or(256.)*1048576.{return Err(fail("COUPLED_BUDGET","Matched load/output plan exceeds request budget"));}
 for (port,r) in refs.iter().enumerate(){system.add_port_admittance(port,1./r,0.)?;}
 let n=system.linear.n;let mut stats=Counters::default();let mut samples=vec![];let mut energy=vec![];let mut instance_loss=vec![];let mut power_error:f64=0.;
 for &f in frequencies {
  if started.elapsed().as_secs_f64()>budget.and_then(|b|b.max_seconds).unwrap_or(120.){return Err(fail("NETWORK_DEADLINE","Matched wave deadline"));}
  let mut sample=vec![vec![json!({"real":0.,"imaginary":0.});ports];ports];let mut states=vec![];let mut diagonal_loss=vec![];
  for input in 0..ports {
   let mut drive=vec![0.;ports];drive[input]=1.;let mut rhs=system.port_rhs(&drive);if f!=0.{rhs.resize(2*n,0.);}
   let x=system.solve(2.*std::f64::consts::PI*f,rhs,&vec![0.;n],&mut stats)?;
   let re=system.port_voltages(&x,ports);let im=if f==0.{vec![0.;ports]}else{system.port_voltages(&x[n..],ports)};
   let mut out=0.;for output in 0..ports {let a=2.*(re[output]/refs[input].sqrt()/refs[output].sqrt())-if output==input {1.}else{0.};let b=2.*(im[output]/refs[input].sqrt()/refs[output].sqrt());if !a.is_finite()||!b.is_finite(){return Err(fail("COUPLED_NONFINITE","Matched coefficient overflow"));}sample[output][input]=json!({"real":a,"imaginary":b});out+=a*a+b*b;}
   let incident=refs[input]/4.;let loss=system.joule_loss(&x[..n])+if f==0.{0.}else{system.joule_loss(&x[n..])};let error=(incident*(1.-out)-loss).abs();
   if !error.is_finite()||!loss.is_finite(){return Err(fail("COUPLED_NONFINITE","Matched power overflow"));}let source=incident*(1.-out);power_error=power_error.max(error/(1e-12+source.abs()+loss));if error>1e-12+1e-8*(source.abs()+loss){return Err(fail("COUPLED_POWER","Matched wave power differs from intrinsic R loss"));}
   states.push(x);
   if owner_loss.is_some(){diagonal_loss.push(loss);}
  }
  let gram=|entries:&[Entry],offset:usize|{(0..ports).map(|a|(0..ports).map(|b|entries.iter().map(|&(i,j,v)|0.5*v*(states[a][offset+i]*states[b][offset+j]+if f==0.{0.}else{states[a][n+offset+i]*states[b][n+offset+j]})).sum::<f64>()).collect::<Vec<_>>()).collect::<Vec<_>>()};
  let magnetic=gram(&model.inductance_h,system.nv);let electric=gram(&model.capacitance_f,0);if magnetic.iter().chain(&electric).flatten().any(|v|!v.is_finite()){return Err(fail("COUPLED_NONFINITE","Matched intrinsic energy overflow"));}energy.push(json!({"magneticJPerA2":magnetic,"electricJPerA2":electric}));
  if let Some(partition)=owner_loss {
   let grams=partition.grams(model,&states,f,started,budget)?;
   for (port,&expected) in diagonal_loss.iter().enumerate(){
    let actual=grams.iter().map(|g|g[port][port]).sum::<f64>();
    if !actual.is_finite()||(actual-expected).abs()>1e-12+1e-8*(actual.abs()+expected.abs()){
     return Err(fail("COUPLED_POWER","Sum of instance resistance losses differs from intrinsic R loss"));
    }
   }
   // Form the cancellation-prone convergence drive in current coordinates
   // before squaring it. Do not subtract large Gram terms for a weak loss.
   let mut opposite=states[0].clone();
   for state in &states[1..]{
    for (a,b) in opposite.iter_mut().zip(state){*a-=b;}
   }
   let mut opposite_powers=vec![0.;partition.instance_ids.len()];
   for &(i,j,r) in &model.resistance_ohm{
    let i_state=system.nv+i;
    let j_state=system.nv+j;
    let re=opposite[i_state]*opposite[j_state];
    let im=if f==0.{0.}else{opposite[n+i_state]*opposite[n+j_state]};
    opposite_powers[partition.owner_by_dof[i]]+=r*(re+im);
   }
   let opposite_total=opposite_powers.iter().sum::<f64>();
   let expected=system.joule_loss(&opposite[..n])+if f==0.{0.}else{system.joule_loss(&opposite[n..])};
   if !opposite_total.is_finite()||!expected.is_finite()||
      (opposite_total-expected).abs()>1e-12+1e-8*(opposite_total.abs()+expected.abs()){
    return Err(fail("COUPLED_POWER","Sum of opposite-drive instance losses differs from intrinsic R loss"));
   }
   instance_loss.push(json!({"frequencyHz":f,"jouleWPerA2":grams,"oppositeDriveW":opposite_powers}));
  }
  samples.push(sample);
 }
 let mut result=json!({"status":"complete","parameters":samples,"frequenciesHz":frequencies,"counters":stats,"memoryPlanBytes":plan,"elapsedMs":started.elapsed().as_secs_f64()*1000.,"referenceTopology":model.reference_topology,"quality":{"relativeResidual":system.residual,"kclAbsA":system.kcl,"relativeRealPowerError":power_error},"loadedEnergyGram":energy,"energyConvention":"Mean RMS intrinsic .5*(Re(xa)^T E Re(xb)+Im(xa)^T E Im(xb)), unit1A Norton port drives with explicit real reference shunts; shunts store no energy","waveEvaluation":"Direct matched real-reference Norton solve of unchanged intrinsic full RLC"});
 if let Some(partition)=owner_loss {
  result["loadedInstanceLoss"]=json!({"instanceIds":partition.instance_ids,"samples":instance_loss,
   "convention":"RMS mean intrinsic I^H R I; real Gram of unit Norton port drives with explicit reference shunts; reference shunts excluded; opposite drive is [+1,-1,...] A RMS"});
 }
 Ok(result)
}

#[cfg(test)]
mod matched_tests {
 use super::*;
 fn model()->Model {Model{format:"coupled-rlc".into(),reference_topology:"grounded".into(),potential_nodes:vec!["a".into(),"b".into()],current_dofs:vec!["ia".into(),"ib".into()],incidence:vec![(0,0,1.),(1,1,1.)],port_incidence:vec![(0,0,1.),(1,1,1.)],resistance_ohm:vec![(0,0,2.),(1,1,3.)],inductance_h:vec![(0,0,2e-6),(1,1,3e-6),(0,1,5e-7),(1,0,5e-7)],capacitance_f:vec![(0,0,1e-9),(1,1,2e-9)],conserved_potential_modes:vec![],structural_gauge_modes:vec![],port_terminals:None}}
 #[test]fn matched_wave_equals_well_conditioned_impedance_conversion_with_unequal_references(){
  let model=model();let refs=[37.,83.];let axis=[0.,1000.,10000.];let a=matched_waves(&model,&refs,&axis,None,None).unwrap();let z=impedance(&model,2,&axis,None).unwrap();let values:Vec<math::Matrix>=serde_json::from_value(z["parameters"].clone()).unwrap();
  for(f,value)in values.iter().enumerate(){let mut quality=crate::electrical::frequency::Quality::default();let at50=math::to_s(value,"Z",&[50.,50.],&mut quality).unwrap();let expected=math::renormalize(&at50,&[50.,50.],&refs,&mut quality).unwrap();let actual:math::Matrix=serde_json::from_value(a["parameters"][f].clone()).unwrap();for i in 0..2{for j in 0..2{assert!((actual[i][j]-expected[i][j]).abs()<1e-10);}}}
  assert!(a["quality"]["relativeRealPowerError"].as_f64().unwrap()<1e-8);
  let current=37./39.;let other=83./86.;
  assert!((a["loadedEnergyGram"][0]["magneticJPerA2"][0][0].as_f64().unwrap()-0.5*2e-6*current*current).abs()<1e-18);
  assert!((a["loadedEnergyGram"][0]["magneticJPerA2"][0][1].as_f64().unwrap()-0.5*5e-7*current*other).abs()<1e-18);
 }
 #[test]fn matched_wave_preserves_axis_and_memory_guards(){for axis in [vec![],vec![-1.],vec![1000.,1000.],vec![f64::NAN]]{assert!(matched_waves(&model(),&[50.,50.],&axis,None,None).is_err());}}
 #[test]fn extreme_references_never_complete_with_null_wave_or_energy_coefficients(){
  for refs in [[1e-200,2e-200],[1e200,2e200]] {if let Ok(v)=matched_waves(&model(),&refs,&[0.,1000.],None,None){let text=serde_json::to_string(&v).unwrap();assert!(!text.contains("null"));}}
 }

    #[test]
    fn matched_resistance_loss_partitions_two_copper_owners_and_series_capacitor() {
        let copper = |id: &str, r: f64| json!({
            "id":id,"ports":["outer","pad"],"model":{
                "id":id,"kind":"network","ports":["outer","pad"],"data":{
                    "format":"coupled-rlc","referenceTopology":"infinity",
                    "potentialNodes":["outer","pad","return"],"currentDofs":["trace"],
                    "incidence":[[0,0,1.],[1,0,-1.]],
                    "portIncidence":[[0,0,1.],[2,0,-1.],[1,1,1.],[2,1,-1.]],
                    "resistanceOhm":[[0,0,r]],"inductanceH":[],
                    "capacitanceF":[[0,0,5e-9],[2,2,5e-9],[0,2,-5e-9],[2,0,-5e-9]],
                    "conservedPotentialModes":[[0.,0.,1.]],"structuralGaugeModes":[[1.,1.,1.]],
                    "portTerminals":[
                        {"positive":{"node":"outer"},"negative":{"node":"return"}},
                        {"positive":{"node":"pad"},"negative":{"node":"return"}}
                    ]
                }
            }
        });
        let capacitor = json!({
            "id":"package","ports":["pins"],"model":{
                "id":"package","kind":"network","ports":["pins"],"data":{
                    "format":"coupled-rlc","referenceTopology":"infinity",
                    "potentialNodes":["positive","negative","internal"],"currentDofs":["series"],
                    "incidence":[[0,0,1.],[2,0,-1.]],"portIncidence":[[0,0,1.],[1,0,-1.]],
                    "resistanceOhm":[[0,0,0.4]],"inductanceH":[[0,0,2e-6]],
                    "capacitanceF":[[2,2,2e-6],[1,1,2e-6],[2,1,-2e-6],[1,2,-2e-6]],
                    "conservedPotentialModes":[[0.,1.,0.]],"structuralGaugeModes":[[1.,1.,1.]],
                    "portTerminals":[{"positive":{"node":"positive"},"negative":{"node":"negative"}}]
                }
            }
        });
        let frequencies = [1000., 4000.];
        let refs = [37_f64, 83_f64];
        let request: Request = serde_json::from_value(json!({
            "instances":[copper("left",2.),copper("right",3.),capacitor],
            "connections":[[0,1],[0,2],[0,3],[1,4],[3,4]],
            "connectionContacts":["negative","negative","negative","positive","positive-negative"],
            "ports":[{"name":"left","index":0},{"name":"right","index":2}],
            "frequencies":frequencies,"representation":"S","referenceOhm":refs,
            "recoverInstanceLoss":true
        })).unwrap();
        let result = crate::electrical::frequency::run(request.clone());
        assert_eq!(result["status"],"complete","{result}");
        let report = &result["loadedInstanceLoss"];
        assert_eq!(report["instanceIds"],json!(["left","right","package"]));
        let reciprocal = |v: math::C| v.conj().scale(1. / (v.real*v.real + v.imaginary*v.imaginary));
        for (k, frequency) in frequencies.iter().copied().enumerate() {
            let omega = 2. * std::f64::consts::PI * frequency;
            let y = reciprocal(math::C::new(5.4, omega*2e-6 - 1./(omega*2e-6)));
            let g0 = math::C::new(1./refs[0], omega*5e-9);
            let g1 = math::C::new(1./refs[1], omega*5e-9);
            let determinant = (g0+y)*(g1+y)-y*y;
            let currents = [y*g1*reciprocal(determinant), (y*g0*reciprocal(determinant)).scale(-1.)];
            let sample = &report["samples"][k];
            assert_eq!(sample["frequencyHz"],json!(frequency));
            let opposite = currents[0]-currents[1];
            for (owner, r) in [2.,3.,0.4].into_iter().enumerate() {
                let value = sample["oppositeDriveW"][owner].as_f64().unwrap();
                assert!((value-r*opposite.abs().powi(2)).abs()<1e-10);
            }
            let s: math::Matrix = serde_json::from_value(result["parameters"][k].clone()).unwrap();
            for a in 0..2 {
                for b in 0..2 {
                    let mut sum = 0.;
                    for (owner, r) in [2.,3.,0.4].into_iter().enumerate() {
                        let value = sample["jouleWPerA2"][owner][a][b].as_f64().unwrap();
                        assert!((value-r*(currents[a].conj()*currents[b]).real).abs()<1e-10);
                        sum += value;
                    }
                    let incident = if a==b { refs[a]/4. } else { 0. };
                    let outgoing = (refs[a]*refs[b]).sqrt()/4. * (0..2)
                        .map(|p| (s[p][a].conj()*s[p][b]).real).sum::<f64>();
                    assert!((sum-(incident-outgoing)).abs()<1e-10);
                }
            }
        }
        let mut legacy = request.clone();
        legacy.recover_instance_loss = false;
        let ordinary = crate::electrical::frequency::run(legacy);
        assert_eq!(ordinary["parameters"], result["parameters"]);
        assert!(ordinary.get("loadedInstanceLoss").is_none());
        assert!(result["memoryPlanBytes"].as_u64().unwrap()>ordinary["memoryPlanBytes"].as_u64().unwrap());

        let mut incompatible = request;
        incompatible.representation = "Y".into();
        let rejected = crate::electrical::frequency::run(incompatible);
        assert_eq!(rejected["diagnostics"][0]["code"], "COUPLED_LOSS_INPUT");
    }

}

