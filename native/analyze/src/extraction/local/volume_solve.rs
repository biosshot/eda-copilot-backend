//! Matrix-free frequency solves used to admit split congruent reductions.
//! Component constants are separate coordinates: a large capacitive common
//! potential is never added to the small conductive voltage before gradients.
use super::{error,volume_current::{DcFactor,Space},volume_field::ElectricApply,volume_magnetic::MagneticApply,AllocationBudget,Result};
use faer::{Mat,Side,prelude::Solve};
use std::time::{Duration,Instant};
use super::accurate::Sum;
fn dot(a:&[f64],b:&[f64])->f64 {a.iter().zip(b).map(|(a,b)|a*b).sum()}
fn norm(a:&[f64])->f64 {dot(a,a).sqrt()}
#[derive(Clone,Debug,serde::Serialize,serde::Deserialize)]
pub struct Audit {pub iterations:usize,pub scaled_relative_residual:f64,pub normalized_kcl:f64,pub normalized_kvl:f64,pub relative_complex_power_error:f64,pub relative_joule_power_error:f64}
#[derive(Clone,serde::Serialize,serde::Deserialize)]
pub struct Solution {
    pub real_shape_v:Vec<f64>,pub imaginary_shape_v:Vec<f64>,
    pub real_shape_correction_v:Vec<f64>,pub imaginary_shape_correction_v:Vec<f64>,
    pub real_constants_v:Vec<f64>,pub imaginary_constants_v:Vec<f64>,
    pub real_currents_a:Vec<f64>,pub imaginary_currents_a:Vec<f64>,pub audit:Audit,
}
pub struct Solver<'a> {
    space:&'a Space,field:&'a dyn ElectricApply,magnetic:&'a dyn MagneticApply,
    dc:DcFactor,free_nodes:Vec<usize>,component_of:Vec<usize>,
    component_charge:Vec<Vec<f64>>,charge_factor:faer::linalg::solvers::Llt<f64>,
    scale:Vec<f64>,pub planned_bytes:usize,
}
impl<'a> Solver<'a> {
    /// Independent GMRES/RHS/action buffers only; all factors/operators remain shared.
    pub fn additional_solve_bytes(&self)->Result<usize> {(self.space.potential_count()+self.space.current_count()).checked_mul(2*80*8).and_then(|n|n.checked_add(self.field.scratch_bytes())).and_then(|n|n.checked_add(self.magnetic.scratch_bytes())).and_then(|n|n.checked_add(self.dc.solve_scratch_bytes(self.space))).ok_or_else(||error("MEMORY_LIMIT","parallel solve scratch overflow"))}
    pub fn new(space:&'a Space,field:&'a dyn ElectricApply,magnetic:&'a dyn MagneticApply,budget:AllocationBudget)->Result<Self> {
        let nv=space.potential_count();let ni=space.current_count();let nc=space.components.len();let n=nv+ni;
        let plan=space.planned_bytes.checked_add(field.planned_bytes()).and_then(|v|v.checked_add(magnetic.planned_bytes())).and_then(|v|v.checked_add(n.checked_mul(2*80*8)?)).and_then(|v|v.checked_add(nv.checked_mul(nc)?.checked_mul(32)?)).ok_or_else(||error("MEMORY_LIMIT","operator GMRES plan overflow"))?;
        budget.check(plan)?;
        let mut component_of=vec![usize::MAX;nv];let mut constants=vec![];
        for (j,component) in space.components.iter().enumerate() {let mut h=vec![0.;nv];for&i in component {h[i]=1.;component_of[i]=j;}constants.push(h);}
        if component_of.contains(&usize::MAX) {return Err(error("VOLUME_SOLVE","incomplete conductor component partition"));}
        let component_charge=constants.iter().map(|h|field.apply(h)).collect::<Result<Vec<_>>>()?;
        let mut metric=Mat::from_fn(nc,nc,|_,_|0.);for(i,j,v)in field.energy_gram(&constants)? {metric[(i,j)]=v;}
        let charge_factor=metric.llt(Side::Lower).map_err(|_|error("VOLUME_SOLVE","component charge metric is not resolved positive definite; no ground or gmin"))?;
        let free_nodes=(0..nv).filter(|i|space.components[component_of[*i]][0]!=*i).collect();
        let dc=space.dc_factor(&[],budget)?;
        let plan=plan.checked_add(dc.planned_bytes).ok_or_else(||error("MEMORY_LIMIT","DC preconditioner plan overflow"))?;budget.check(plan)?;
        let mut scale=vec![1.;n];for(i,c)in space.cells.iter().enumerate(){for k in 0..3{scale[nv+3*i+k]=1./c.resistance_ohm.sqrt();}}
        Ok(Self{space,field,magnetic,dc,free_nodes,component_of,component_charge,charge_factor,scale,planned_bytes:plan})
    }
    fn shape(&self,x:&[f64])->Vec<f64> {let mut u=vec![0.;self.space.potential_count()];for(i,&node)in self.free_nodes.iter().enumerate(){u[node]=x[i];}u}
    fn charge(&self,u:&[f64],z:&[f64])->Result<Vec<f64>> {let mut q=if u.iter().all(|&v|v==0.) {vec![0.;u.len()]}else{self.field.apply(u)?};for(j,h)in self.component_charge.iter().enumerate(){for(i,&v)in h.iter().enumerate(){q[i]+=v*z[j];}}Ok(q)}
    fn magnetic(&self,i:&[f64])->Result<Vec<f64>> {if i.iter().all(|&v|v==0.) {Ok(vec![0.;i.len()])}else{self.magnetic.apply(self.space,i)}}
    fn apply(&self,x:&[f64],omega:f64)->Result<Vec<f64>> {Ok(self.apply_with_kvl_scale(x,omega)?.0)}
    fn apply_with_kvl_scale(&self,x:&[f64],omega:f64)->Result<(Vec<f64>,Vec<f64>)> {
        let nv=self.space.potential_count();let ni=self.space.current_count();let n=nv+ni;let nf=self.free_nodes.len();
        let ur=self.shape(&x[..nv]);let ui=self.shape(&x[n..n+nv]);
        let cr=self.charge(&ur,&x[nf..nv])?;let ci=self.charge(&ui,&x[n+nf..n+nv])?;
        let ir=&x[nv..n];let ii=&x[n+nv..];let lr=self.magnetic(ir)?;let li=self.magnetic(ii)?;
        let dr=self.space.divergence(ir)?;let di=self.space.divergence(ii)?;let gr=self.space.gradient(&ur)?;let gi=self.space.gradient(&ui)?;
        let mut out=vec![0.;2*n];
        for(i,&node)in self.free_nodes.iter().enumerate(){out[i]=dr[node]-omega*ci[node];out[n+i]=di[node]+omega*cr[node];}
        for(j,component)in self.space.components.iter().enumerate(){out[nf+j]=-omega*component.iter().map(|&i|ci[i]).sum::<f64>();out[n+nf+j]=omega*component.iter().map(|&i|cr[i]).sum::<f64>();}
        let mut kvl_scale=vec![0.;ni];
        for(i,c)in self.space.cells.iter().enumerate(){for k in 0..3{
            let j=3*i+k;
            out[nv+j]=Sum::from(ir[j]).mul(c.resistance_ohm).sub(Sum::from(gr[j])).sub(Sum::from(li[j]).mul(omega)).value();
            out[n+nv+j]=Sum::from(ii[j]).mul(c.resistance_ohm).sub(Sum::from(gi[j])).add(Sum::from(lr[j]).mul(omega)).value();
            let gradient_scale=(1..4).map(|a|c.incidence[a][k].abs()*((ur[c.potentials[a]]-ur[c.potentials[0]]).abs()+(ui[c.potentials[a]]-ui[c.potentials[0]]).abs())).sum::<f64>();
            kvl_scale[j]=c.resistance_ohm*(ir[j].abs()+ii[j].abs())+gradient_scale+omega*(lr[j].abs()+li[j].abs());
        }}
        for part in 0..2 {for j in 0..n {out[part*n+j]*=self.scale[j];}}
        if out.iter().any(|v|!v.is_finite()){return Err(error("VOLUME_SOLVE","nonfinite descriptor action"));}Ok((out,kvl_scale))
    }
    fn precondition(&self,input:&[f64],omega:f64)->Result<Vec<f64>> {
        let nv=self.space.potential_count();let ni=self.space.current_count();let n=nv+ni;let nf=self.free_nodes.len();let nc=self.space.components.len();
        let mut raw=input.to_vec();for part in 0..2 {for j in 0..n {raw[part*n+j]/=self.scale[j];}}
        let mut q=Mat::from_fn(nc,2,|i,k|raw[k*n+nf+i]);let multipliers=self.charge_factor.solve(&q);
        let mut shapes=vec![];let mut currents_rhs=vec![];
        for part in 0..2 {
            let mut r=vec![0.;nv];for(i,&node)in self.free_nodes.iter().enumerate(){r[node]=raw[part*n+i];}
            for(j,component)in self.space.components.iter().enumerate(){r[component[0]]=raw[part*n+nf+j]-component[1..].iter().map(|&i|r[i]).sum::<f64>();}
            let mut i=raw[part*n+nv..part*n+n].to_vec();for(k,c)in self.space.cells.iter().enumerate(){for axis in 0..3{i[3*k+axis]/=c.resistance_ohm;}}
            let div=self.space.divergence(&i)?;for node in 0..nv {r[node]-=div[node];for j in 0..nc {r[node]-=self.component_charge[j][node]*multipliers[(j,part)];}}
            shapes.push(self.dc.solve(self.space,&vec![0.;nv],&r)?);currents_rhs.push(i);
        }
        for j in 0..nc {
            let qr=q[(j,0)];let qi=q[(j,1)];q[(j,0)]=qi/omega-dot(&self.component_charge[j],&shapes[0]);q[(j,1)]=-qr/omega-dot(&self.component_charge[j],&shapes[1]);
        }
        let constants=self.charge_factor.solve(&q);let mut out=vec![0.;2*n];
        for part in 0..2 {for(i,&node)in self.free_nodes.iter().enumerate(){out[part*n+i]=shapes[part][node];}for j in 0..nc {out[part*n+nf+j]=constants[(j,part)];}
            let g=self.space.gradient(&shapes[part])?;for(i,c)in self.space.cells.iter().enumerate(){for axis in 0..3{let j=3*i+axis;out[part*n+nv+j]=currents_rhs[part][j]+g[j]/c.resistance_ohm;}}
        }Ok(out)
    }
    /// Full original coordinate solve. No dense full R/L/C matrix or full P6
    /// compilation occurs. Input is an actual nodal current injection phasor.
    pub fn solve(&self,frequency_hz:f64,real_injection:&[f64],imaginary_injection:&[f64],max_wall:Duration)->Result<Solution> {
        self.solve_with_restart_limit(frequency_hz,real_injection,imaginary_injection,max_wall,8)
    }
    /// Bounded developer allowance for more restarts with the same 30-vector
    /// Krylov window, deadline and convergence checks.
    pub fn solve_with_restart_limit(&self,frequency_hz:f64,real_injection:&[f64],imaginary_injection:&[f64],max_wall:Duration,restart_limit:usize)->Result<Solution> {
        if !(8..=16).contains(&restart_limit) {return Err(error("VOLUME_SOLVE","restart limit must be in 8..16"));}
        let start=Instant::now();let nv=self.space.potential_count();let ni=self.space.current_count();let n=nv+ni;let nf=self.free_nodes.len();
        if !frequency_hz.is_finite()||frequency_hz<=0.||real_injection.len()!=nv||imaginary_injection.len()!=nv||real_injection.iter().chain(imaginary_injection).any(|v|!v.is_finite()) {return Err(error("VOLUME_SOLVE","positive frequency and finite full nodal injection required"));}
        let omega=2.*std::f64::consts::PI*frequency_hz;let mut b=vec![0.;2*n];
        for(part,input)in [real_injection,imaginary_injection].into_iter().enumerate(){for(i,&node)in self.free_nodes.iter().enumerate(){b[part*n+i]=input[node];}for(j,component)in self.space.components.iter().enumerate(){b[part*n+nf+j]=component.iter().map(|&i|input[i]).sum();}}
        let scale=norm(&b);if scale==0. {return Err(error("VOLUME_SOLVE","nonzero excitation required"));}
        let mut x=vec![0.;2*n];let mut xlow=vec![0.;2*n];let mut residual=b.clone();let mut iterations=0;let mut relative=1.;
        for _restart in 0..restart_limit {
            let beta=norm(&residual);let mut v=vec![residual.iter().map(|x|x/beta).collect::<Vec<_>>()];let mut z=vec![];let mut h=vec![vec![0.;30];31];let mut c=[0.;30];let mut s=[0.;30];let mut g=[0.;31];g[0]=beta;let mut used=0;
            for j in 0..30 {
                if start.elapsed()>max_wall {return Err(error("DEADLINE","full operator GMRES deadline"));}
                let direction=self.precondition(&v[j],omega)?;let mut w=self.apply(&direction,omega)?;z.push(direction);
                for _ in 0..2 {for i in 0..=j {let a=dot(&v[i],&w);h[i][j]+=a;for(k,w)in w.iter_mut().enumerate(){*w-=a*v[i][k];}}}
                h[j+1][j]=norm(&w);let dependent=h[j+1][j]<=64.*f64::EPSILON;
                if !dependent {v.push(w.iter().map(|x|x/h[j+1][j]).collect());}
                for i in 0..j {let a=h[i][j];let b=h[i+1][j];h[i][j]=c[i]*a+s[i]*b;h[i+1][j]=-s[i]*a+c[i]*b;}
                let length=h[j][j].hypot(h[j+1][j]);if length==0.||!length.is_finite(){return Err(error("VOLUME_SOLVE","unresolved GMRES Hessenberg rank"));}
                c[j]=h[j][j]/length;s[j]=h[j+1][j]/length;h[j][j]=length;h[j+1][j]=0.;g[j+1]=-s[j]*g[j];g[j]*=c[j];used=j+1;iterations+=1;
                if g[j+1].abs()<=1e-15*scale||dependent {break;}
            }
            let mut y=vec![0.;used];for i in (0..used).rev(){y[i]=(g[i]-(i+1..used).map(|j|h[i][j]*y[j]).sum::<f64>())/h[i][i];}
            for j in 0..used {for i in 0..2*n {let next=Sum{hi:x[i],lo:xlow[i]}.add(Sum::from(z[j][i]).mul(y[j]));x[i]=next.hi;xlow[i]=next.lo;}}
            let (action,kvl_scale)=self.apply_with_kvl_scale(&x,omega)?;let (low_action,low_kvl_scale)=self.apply_with_kvl_scale(&xlow,omega)?;
            residual=b.iter().zip(action.iter().zip(&low_action)).map(|(b,(a,l))|Sum::from(*b).sub(Sum::from(*a)).sub(Sum::from(*l)).value()).collect();relative=norm(&residual)/scale;
            // A small global norm does not resolve weak constitutive rows.
            // Continue the existing defect correction until the original local
            // KVL metric is resolved too; final physical guards stay unchanged.
            let local_scale=kvl_scale.iter().zip(&low_kvl_scale).map(|(a,b)|a+b).collect::<Vec<_>>();
            let reference=local_scale.iter().copied().fold(0.,f64::max);
            let local=(0..ni).map(|j|residual[nv+j].hypot(residual[n+nv+j])/self.scale[nv+j]/local_scale[j].max(1e-12*reference).max(f64::MIN_POSITIVE)).fold(0.,f64::max);
            eprintln!("full refinement restart={_restart} global={relative:e} localKVL={local:e}");
            if relative<=1e-14&&local<=1e-9 {break;}
        }
        if relative>1e-12||!relative.is_finite(){
            let worst=(0..residual.len()).max_by(|&a,&b|residual[a].abs().total_cmp(&residual[b].abs())).unwrap();
            return Err(error("VOLUME_CONVERGENCE",&format!("GMRES iterations={iterations} relative={relative:e}, worst scaled row={worst}, residual={:e}, rhs={:e}",residual[worst],b[worst])));
        }
        let ur=self.shape(&x[..nv]);let ui=self.shape(&x[n..n+nv]);let ir_hi=x[nv..n].to_vec();let ii_hi=x[n+nv..].to_vec();
        let ir_low=xlow[nv..n].to_vec();let ii_low=xlow[n+nv..].to_vec();
        let urlow=self.shape(&xlow[..nv]);let uilow=self.shape(&xlow[n..n+nv]);
        let combine=|a:Vec<f64>,b:Vec<f64>|->Vec<f64>{a.iter().zip(b).map(|(a,b)|Sum::from(*a).add(Sum::from(b)).value()).collect()};
        // Keep the current correction as well as the potential correction.
        // Auditing only the high current against the corrected gradient is a
        // different state, especially in weak transverse current coordinates.
        let ir=combine(ir_hi.clone(),ir_low.clone());let ii=combine(ii_hi.clone(),ii_low.clone());
        let cr=combine(self.charge(&ur,&x[nf..nv])?,self.charge(&urlow,&xlow[nf..nv])?);let ci=combine(self.charge(&ui,&x[n+nf..n+nv])?,self.charge(&uilow,&xlow[n+nf..n+nv])?);let dr=combine(self.space.divergence(&ir_hi)?,self.space.divergence(&ir_low)?);let di=combine(self.space.divergence(&ii_hi)?,self.space.divergence(&ii_low)?);
        let mut scale_i=vec![0.;nv];for(i,cell)in self.space.cells.iter().enumerate(){for k in 0..4 {for axis in 0..3 {scale_i[cell.potentials[k]]+=cell.incidence[k][axis].abs()*(ir[3*i+axis].abs()+ii[3*i+axis].abs());}}}
        let reference=scale_i.iter().copied().fold(0.,f64::max);let mut kcl:f64=0.;
        for i in 0..nv {let denominator=scale_i[i]+omega*(cr[i].abs()+ci[i].abs())+real_injection[i].abs()+imaginary_injection[i].abs();kcl=kcl.max((dr[i]-omega*ci[i]-real_injection[i]).hypot(di[i]+omega*cr[i]-imaginary_injection[i])/denominator.max(1e-12*reference).max(f64::MIN_POSITIVE));}
        let lr_hi=self.magnetic(&ir_hi)?;let li_hi=self.magnetic(&ii_hi)?;
        let lr_low=self.magnetic(&ir_low)?;let li_low=self.magnetic(&ii_low)?;
        let lr=combine(lr_hi.clone(),lr_low.clone());let li=combine(li_hi.clone(),li_low.clone());
        let gr_hi=self.space.gradient(&ur)?;let gi_hi=self.space.gradient(&ui)?;
        let gr_low=self.space.gradient(&urlow)?;let gi_low=self.space.gradient(&uilow)?;
        let mut residual_v=vec![];let mut denominator_v=vec![];
        for(i,cell)in self.space.cells.iter().enumerate(){for axis in 0..3 {
            let j=3*i+axis;
            let real=Sum{hi:ir_hi[j],lo:ir_low[j]}.mul(cell.resistance_ohm)
                .sub(Sum{hi:gr_hi[j],lo:gr_low[j]}).sub(Sum{hi:li_hi[j],lo:li_low[j]}.mul(omega));
            let imaginary=Sum{hi:ii_hi[j],lo:ii_low[j]}.mul(cell.resistance_ohm)
                .sub(Sum{hi:gi_hi[j],lo:gi_low[j]}).add(Sum{hi:lr_hi[j],lo:lr_low[j]}.mul(omega));
            residual_v.push(real.value().hypot(imaginary.value()));
            let gradient_scale=(1..4).map(|k|cell.incidence[k][axis].abs()*((ur[cell.potentials[k]]-ur[cell.potentials[0]]).abs()+(ui[cell.potentials[k]]-ui[cell.potentials[0]]).abs())).sum::<f64>();
            denominator_v.push(cell.resistance_ohm*(ir[j].abs()+ii[j].abs())+gradient_scale+omega*(lr[j].abs()+li[j].abs()));
        }}
        let reference=denominator_v.iter().copied().fold(0.,f64::max);let kvl=residual_v.iter().zip(&denominator_v).map(|(r,s)|r/s.max(1e-12*reference).max(f64::MIN_POSITIVE)).fold(0.,f64::max);
        if kcl>1e-8||kvl>1e-8||!kcl.is_finite()||!kvl.is_finite(){let worst=(0..residual_v.len()).max_by(|&a,&b|(residual_v[a]/denominator_v[a].max(1e-12*reference)).total_cmp(&(residual_v[b]/denominator_v[b].max(1e-12*reference)))).unwrap();
            return Err(error("VOLUME_RESIDUAL",&format!("full KCL={kcl:e} KVL={kvl:e}; row={worst} residual={:e} denominator={:e} reference={reference:e} currentHi={:e} currentLow={:e} gradientHi={:e} gradientLow={:e}",residual_v[worst],denominator_v[worst],ir_hi[worst],ir_low[worst],gr_hi[worst],gr_low[worst])));}
        let mut input_real=dot(&ur,real_injection)+dot(&ui,imaginary_injection);let mut input_imaginary=dot(&ui,real_injection)-dot(&ur,imaginary_injection);
        let mut electric=dot(&ur,&cr)+dot(&ui,&ci);
        for(j,component)in self.space.components.iter().enumerate(){let qr=component.iter().map(|&i|real_injection[i]).sum::<f64>();let qi=component.iter().map(|&i|imaginary_injection[i]).sum::<f64>();let zr=x[nf+j];let zi=x[n+nf+j];input_real+=zr*qr+zi*qi;input_imaginary+=zi*qr-zr*qi;electric+=zr*component.iter().map(|&i|cr[i]).sum::<f64>()+zi*component.iter().map(|&i|ci[i]).sum::<f64>();}
        let loss=self.space.losses(&ir)?.iter().sum::<f64>()+self.space.losses(&ii)?.iter().sum::<f64>();let magnetic=dot(&ir,&lr)+dot(&ii,&li);
        let power_error=(input_real-loss).hypot(input_imaginary-omega*(magnetic-electric))/input_real.hypot(input_imaginary).max(loss+omega*(magnetic.abs()+electric.abs())).max(f64::MIN_POSITIVE);
        let joule_error=(input_real-loss).abs()/input_real.abs().max(loss).max(f64::MIN_POSITIVE);
        if power_error>1e-8||joule_error>1e-8||!power_error.is_finite()||!joule_error.is_finite(){return Err(error("VOLUME_POWER",&format!("complex={power_error:e} Joule={joule_error:e}")));}
        Ok(Solution{real_shape_v:ur,imaginary_shape_v:ui,real_shape_correction_v:urlow,imaginary_shape_correction_v:uilow,real_constants_v:x[nf..nv].to_vec(),imaginary_constants_v:x[n+nf..n+nv].to_vec(),real_currents_a:ir,imaginary_currents_a:ii,audit:Audit{iterations,scaled_relative_residual:relative,normalized_kcl:kcl,normalized_kvl:kvl,relative_complex_power_error:power_error,relative_joule_power_error:joule_error}})
    }
    pub fn port_voltage(&self,solution:&Solution,a:usize,b:usize)->[f64;2] {
        let ca=self.component_of[a];let cb=self.component_of[b];
        let difference=[solution.real_shape_v[a]-solution.real_shape_v[b],solution.imaginary_shape_v[a]-solution.imaginary_shape_v[b]];
        if ca==cb {difference}else{[difference[0]+(solution.real_constants_v[ca]-solution.real_constants_v[cb]),difference[1]+(solution.imaginary_constants_v[ca]-solution.imaginary_constants_v[cb])]}
    }
}
