//! Sparse DC harmonic partition and split congruence before dense P6.
//! This builds a candidate; holdout/error admission is a separate mandatory step.
use super::{error,volume_current::Space,volume_field::ElectricApply,volume_magnetic::MagneticApply,AllocationBudget,Result};
use crate::electrical::coupled::{Model,Entry,physical::{Node,PortTerminals,Terminal}};
use std::collections::BTreeSet;
#[derive(serde::Serialize,serde::Deserialize)]
pub struct Basis {
    /// Partition of unity, cardinal at the authored electrodes. This retains
    /// P6's physical total-charge coordinate without changing its contract.
    pub voltage:Vec<Vec<f64>>,
    /// R-orthonormal current columns; full physical currents = Q_i i_reduced.
    pub current:Vec<Vec<f64>>,
    pub names:Vec<String>,
    pub component_columns:Vec<Vec<usize>>,
    harmonic:Vec<Vec<f64>>,
    electrode_dofs:Vec<usize>,
    bubbles:Vec<Vec<Vec<f64>>>,
    #[serde(default)] global_bubbles:Vec<Vec<f64>>,
}
fn dot(a:&[f64],b:&[f64])->f64 {a.iter().zip(b).map(|(a,b)|a*b).sum()}
fn append_current(space:&Space,basis:&mut Vec<Vec<f64>>,mut v:Vec<f64>,reference:f64)->Result<bool> {
    let inner=|a:&[f64],b:&[f64]|->f64 {space.cells.iter().enumerate().map(|(i,c)|c.resistance_ohm*(0..3).map(|k|a[3*i+k]*b[3*i+k]).sum::<f64>()).sum()};
    let original=inner(&v,&v).sqrt();
    if !original.is_finite() {return Err(error("VOLUME_REDUCTION","nonfinite R metric"));}
    for _ in 0..2 {for q in basis.iter() {let x=inner(q,&v);for (v,q) in v.iter_mut().zip(q) {*v-=x*q;}}}
    let norm=inner(&v,&v).sqrt();
    if norm<=1e-12*reference.max(original) {return Ok(false);}
    for x in &mut v {*x/=norm;}basis.push(v);Ok(true)
}
pub fn harmonic(space:&Space,electrodes:&[(String,usize)],budget:AllocationBudget)->Result<Basis> {
    if electrodes.is_empty() || electrodes.iter().map(|(_,i)|*i).collect::<BTreeSet<_>>().len()!=electrodes.len() {return Err(error("VOLUME_REDUCTION","distinct finite electrodes required"));}
    let count=electrodes.len()+space.components.len();
    budget.check(space.planned_bytes.checked_add((space.potential_count()+space.current_count()).checked_mul(count).and_then(|n|n.checked_mul(32)).ok_or_else(||error("MEMORY_LIMIT","basis plan overflow"))?).ok_or_else(||error("MEMORY_LIMIT","basis plan overflow"))?)?;
    let factor=space.dc_factor(&electrodes.iter().map(|(_,i)|*i).collect::<Vec<_>>(),budget)?;
    let nv=space.potential_count();let mut voltage=vec![vec![0.;nv];electrodes.len()];let mut names:Vec<_>=electrodes.iter().map(|(s,_)|s.clone()).collect();
    let mut component_columns=vec![];
    for (ci,component) in space.components.iter().enumerate() {
        let ports:Vec<_>=electrodes.iter().enumerate().filter(|(_,(_,node))|component.contains(node)).map(|(i,_)|i).collect();
        let mut columns=ports.clone();
        if ports.is_empty() {
            let mut v=vec![0.;nv];for &i in component {v[i]=1.;}columns.push(voltage.len());voltage.push(v);names.push(format!("floating-{ci}"));
        } else {
            for &p in &ports[..ports.len()-1] {
                let mut boundary=vec![0.;nv];boundary[electrodes[p].1]=1.;
                voltage[p]=factor.solve(space,&boundary,&vec![0.;nv])?;
            }
            let last=*ports.last().unwrap();for &i in component {voltage[last][i]=1.-ports[..ports.len()-1].iter().map(|&p|voltage[p][i]).sum::<f64>();}
        }
        component_columns.push(columns);
    }
    let mut current=vec![];let mut scale:f64=0.;
    for v in &voltage {let mut g=space.gradient(v)?;for (i,c) in space.cells.iter().enumerate() {for k in 0..3 {g[3*i+k]/=c.resistance_ohm;}}
        let norm=space.losses(&g)?.iter().sum::<f64>().sqrt();scale=scale.max(norm);append_current(space,&mut current,g,scale)?;
    }
    if current.is_empty() {return Err(error("VOLUME_REDUCTION","electrodes do not excite a finite conductive path"));}
    Ok(Basis{harmonic:voltage.clone(),electrode_dofs:electrodes.iter().map(|(_,i)|*i).collect(),bubbles:vec![vec![];space.components.len()],global_bubbles:vec![],voltage,current,names,component_columns})
}
fn symmetric(values:&[Vec<f64>])->Vec<Entry> {let mut out=vec![];for i in 0..values.len(){for j in i..values.len(){let v=values[i][j];if v!=0.{out.push((i,j,v));if i!=j{out.push((j,i,v));}}}}out}
impl Basis {
    /// Add physical current profiles without changing potentials, contacts or
    /// independent island constants. This is candidate construction only;
    /// original-space AC residual admission remains mandatory.
    pub fn include_current_profiles(&mut self, space:&Space, profiles:&[Vec<f64>], budget:AllocationBudget)->Result<usize> {
        let count=self.current.len().checked_add(profiles.len()).ok_or_else(||error("MEMORY_LIMIT","profile count overflow"))?;
        let bytes=space.current_count().checked_mul(count).and_then(|n|n.checked_mul(16)).and_then(|n|n.checked_add(space.planned_bytes)).ok_or_else(||error("MEMORY_LIMIT","profile basis overflow"))?;
        budget.check(bytes)?;
        if profiles.iter().any(|v|v.len()!=space.current_count()||v.iter().any(|x|!x.is_finite())) {
            return Err(error("SECTION_RETURN_MAP","finite full physical current columns required"));
        }
        let old=self.current.len();
        // Work on a candidate so a rejected profile cannot partly mutate Basis.
        let mut current=self.current.clone();
        for v in profiles {append_current(space,&mut current,v.clone(),0.)?;}
        self.current=current;
        Ok(self.current.len()-old)
    }
    /// Immutable cardinal electrode lifts. Trained bubbles modify `voltage`
    /// coordinates, never these original physical DC source states.
    pub fn retained_dofs(&self)->&[usize] {&self.electrode_dofs}
    pub fn harmonic_electrodes(&self)->Result<&[Vec<f64>]>{
        let count=self.electrode_dofs.len();
        if count==0||self.harmonic.len()<count{return Err(error("VOLUME_REDUCTION","cardinal harmonic electrode states missing"));}
        Ok(&self.harmonic[..count])
    }
    /// Expand both physical spaces from a full operator solution. Subtract the
    /// harmonic lift using the small shape voltage only; the separately stored
    /// conductor constants cancel analytically through partition of unity.
    pub fn include_solution(&mut self,space:&Space,solution:&super::volume_solve::Solution,budget:AllocationBudget)->Result<(usize,usize)> {
        let nv=space.potential_count();let old_i=self.current.len();
        let plan=(nv+space.current_count()).checked_mul(self.voltage.len()+self.current.len()+16).and_then(|v|v.checked_mul(64)).and_then(|v|v.checked_add(space.planned_bytes)).ok_or_else(||error("MEMORY_LIMIT","snapshot basis plan overflow"))?;budget.check(plan)?;
        let (added_v,_)=self.include_potential_profiles(space,&[solution.real_shape_v.clone(),solution.imaginary_shape_v.clone()],budget)?;
        append_current(space,&mut self.current,solution.real_currents_a.clone(),0.)?;
        append_current(space,&mut self.current,solution.imaginary_currents_a.clone(),0.)?;
        Ok((added_v,self.current.len()-old_i))
    }
    /// Add bounded physical potential profiles while preserving cardinality and island partitions.
    pub fn include_potential_profiles(&mut self,space:&Space,profiles:&[Vec<f64>],budget:AllocationBudget)->Result<(usize,usize)> {
        if !self.global_bubbles.is_empty() {return Err(error("VOLUME_REDUCTION","cannot mix global and component potential bubbles"));}
        let nv=space.potential_count();let mut added_v=0;let old_i=self.current.len();
        let count=profiles.len().checked_mul(space.components.len()).and_then(|n|n.checked_add(self.voltage.len())).and_then(|n|n.checked_add(self.current.len())).and_then(|n|n.checked_add(16)).ok_or_else(||error("MEMORY_LIMIT","potential profile count overflow"))?;
        let plan=nv.checked_add(space.current_count()).and_then(|n|n.checked_mul(count)).and_then(|v|v.checked_mul(64)).and_then(|v|v.checked_add(space.planned_bytes)).ok_or_else(||error("MEMORY_LIMIT","potential profile plan overflow"))?;budget.check(plan)?;
        if profiles.iter().any(|v|v.len()!=nv||v.iter().any(|x|!x.is_finite())) {return Err(error("VOLUME_REDUCTION","full finite potential profiles required"));}
        for shape in profiles {
            if shape.len()!=nv || shape.iter().any(|v|!v.is_finite()) {return Err(error("VOLUME_REDUCTION","full finite potential profile required"));}
            for(ci,component)in space.components.iter().enumerate() {
                let mut bubble=vec![0.;nv];
                for&i in component {bubble[i]=shape[i]-self.electrode_dofs.iter().enumerate().map(|(p,&node)|self.harmonic[p][i]*shape[node]).sum::<f64>();}
                if self.electrode_dofs.iter().any(|&i|bubble[i]!=0.) {return Err(error("VOLUME_REDUCTION","harmonic snapshot lift is not exactly cardinal at the contacts"));}
                let scale=bubble.iter().map(|v|v.abs()).fold(0.,f64::max);if scale==0. {continue;}for v in &mut bubble {*v/=scale;}
                let original=dot(&bubble,&bubble).sqrt();
                for _ in 0..2 {for q in &self.bubbles[ci] {let x=dot(q,&bubble);for(v,q)in bubble.iter_mut().zip(q){*v-=x*q;}}}
                let length=dot(&bubble,&bubble).sqrt();if length<=1e-12*original {continue;}
                for v in &mut bubble {*v/=length;}
                let base=self.component_columns[ci][0];
                for i in 0..nv {self.voltage[base][i]-=bubble[i];}
                self.component_columns[ci].push(self.voltage.len());self.names.push(format!("interior-mode-{}",self.voltage.len()));
                self.bubbles[ci].push(bubble.clone());self.voltage.push(bubble.clone());added_v+=1;
                let mut g=space.gradient(&bubble)?;for(i,c)in space.cells.iter().enumerate(){for k in 0..3{g[3*i+k]/=c.resistance_ohm;}}
                append_current(space,&mut self.current,g,0.)?;
            }
        }
        Ok((added_v,self.current.len()-old_i))
    }
    /// Add zero-contact global residual modes while preserving each island constant.
    pub fn include_global_potential_profiles(&mut self,space:&Space,profiles:&[Vec<f64>],budget:AllocationBudget)->Result<usize> {
        if self.bubbles.iter().any(|b|!b.is_empty()) {return Err(error("VOLUME_REDUCTION","cannot mix global and component potential bubbles"));}
        let count=self.voltage.len().checked_add(self.current.len()).and_then(|n|n.checked_add(profiles.len())).and_then(|n|n.checked_add(16)).ok_or_else(||error("MEMORY_LIMIT","global profile count overflow"))?;
        budget.check(space.potential_count().checked_add(space.current_count()).and_then(|n|n.checked_mul(count)).and_then(|n|n.checked_mul(64)).and_then(|n|n.checked_add(space.planned_bytes)).ok_or_else(||error("MEMORY_LIMIT","global profile plan overflow"))?)?;
        if profiles.iter().any(|p|p.len()!=space.potential_count()||p.iter().any(|v|!v.is_finite())||self.electrode_dofs.iter().any(|&i|p[i]!=0.)) {return Err(error("VOLUME_REDUCTION","finite zero-contact profiles required"));}
        let mut added=0;
        for p in profiles {
            let mut v=p.clone();let scale=v.iter().map(|x|x.abs()).fold(0.,f64::max);if scale==0. {continue;}for x in &mut v {*x/=scale;}
            let original=dot(&v,&v).sqrt();for _ in 0..2 {for q in &self.global_bubbles {let a=dot(q,&v);for(x,y) in v.iter_mut().zip(q) {*x-=a*y;}}}
            let norm=dot(&v,&v).sqrt();if norm<=1e-12*original {continue;}for x in &mut v {*x/=norm;}
            let mut g=space.gradient(&v)?;for(i,c)in space.cells.iter().enumerate(){for k in 0..3{g[3*i+k]/=c.resistance_ohm;}}
            append_current(space,&mut self.current,g,0.)?;let base=self.component_columns[0][0];for i in 0..v.len(){self.voltage[base][i]-=v[i];}
            self.component_columns[0].push(self.voltage.len());self.names.push(format!("global-interior-mode-{}",self.voltage.len()));self.voltage.push(v.clone());self.global_bubbles.push(v);added+=1;
        }Ok(added)
    }
    pub fn extend_magnetic(&mut self,space:&Space,magnetic:&dyn MagneticApply)->Result<usize> {
        let old=self.current.clone();let mut added=0;
        for q in old {let mut v=magnetic.apply(space,&q)?;for(i,c)in space.cells.iter().enumerate(){for k in 0..3{v[3*i+k]/=c.resistance_ohm;}}
            added+=usize::from(append_current(space,&mut self.current,v,0.)?);
        }Ok(added)
    }
    pub fn model(&self,space:&Space,field:&dyn ElectricApply,magnetic:&dyn MagneticApply,ports:&[[usize;2]],budget:AllocationBudget)->Result<Model> {
        let nv=self.voltage.len();let ni=self.current.len();
        budget.check((nv+ni).checked_pow(2).and_then(|n|n.checked_mul(1600)).and_then(|n|n.checked_add(magnetic.planned_bytes())).and_then(|n|n.checked_add(field.planned_bytes())).and_then(|n|n.checked_add(space.planned_bytes)).ok_or_else(||error("MEMORY_LIMIT","reduced model plan overflow"))?)?;
        let mut model=Model{format:"coupled-rlc".into(),reference_topology:"infinity".into(),potential_nodes:self.names.clone(),current_dofs:(0..ni).map(|i|format!("current-mode-{i}")).collect(),incidence:vec![],port_incidence:vec![],resistance_ohm:vec![],inductance_h:vec![],capacitance_f:vec![],conserved_potential_modes:vec![],structural_gauge_modes:vec![],port_terminals:Some(vec![])};
        let mut gradient=self.voltage.iter().map(|q|space.gradient(q)).collect::<Result<Vec<_>>>()?;
        // Differentiate the partition analytically: its last shape is exactly
        // 1 minus the preceding shapes on that component. Computing every
        // derivative independently leaves cancellation noise in pure loop
        // columns and would violate exact total-charge conservation in P6.
        for columns in &self.component_columns {
            let last=*columns.last().unwrap();
            for k in 0..space.current_count() {gradient[last][k]=-columns[..columns.len()-1].iter().map(|&a|gradient[a][k]).sum::<f64>();}
        }
        let mut incidence=vec![vec![0.;ni];nv];
        for columns in &self.component_columns {
            let last=*columns.last().unwrap();
            for &a in &columns[..columns.len()-1] {for b in 0..ni {incidence[a][b]=dot(&gradient[a],&self.current[b]);}}
            for b in 0..ni {incidence[last][b]=-columns[..columns.len()-1].iter().map(|&a|incidence[a][b]).sum::<f64>();}
        }
        for a in 0..nv {for b in 0..ni {if incidence[a][b]!=0. {model.incidence.push((a,b,incidence[a][b]));}}}
        let mut r=vec![vec![0.;ni];ni];let mut l=vec![vec![0.;ni];ni];
        // Project each full-space action immediately. Retain only the small
        // directed Gram matrix, including both directions for reciprocity.
        let mut directed=vec![vec![0.;ni];ni];
        for b in 0..ni {
            let image=magnetic.apply(space,&self.current[b])?;
            for a in 0..ni {directed[a][b]=dot(&self.current[a],&image);}
        }
        let scale=(0..ni).map(|i|directed[i][i].abs()).fold(0.,f64::max);
        for a in 0..ni {for b in a..ni {
            r[a][b]=space.cells.iter().enumerate().map(|(i,c)|c.resistance_ohm*(0..3).map(|k|self.current[a][3*i+k]*self.current[b][3*i+k]).sum::<f64>()).sum();
            l[a][b]=directed[a][b];
            if (l[a][b]-directed[b][a]).abs()>1e-9*scale {return Err(error("MAGNETIC_RECIPROCITY","directed projected Newton actions disagree"));}
        }}
        model.resistance_ohm=symmetric(&r);model.inductance_h=symmetric(&l);
        model.capacitance_f=field.energy_gram(&self.voltage)?;
        for columns in &self.component_columns {let mut v=vec![0.;nv];for &i in columns {v[i]=1.;}model.conserved_potential_modes.push(v);}
        for (p,&[a,b]) in ports.iter().enumerate() {
            if a>=nv||b>=nv||a==b {return Err(error("BOARD_PORT","distinct retained electrode coordinates required"));}
            model.port_incidence.extend([(a,p,1.),(b,p,-1.)]);model.port_terminals.as_mut().unwrap().push(PortTerminals{positive:Terminal::Node(Node{node:self.names[a].clone()}),negative:Terminal::Node(Node{node:self.names[b].clone()})});
        }
        Ok(model)
    }
}
