//! Frequency conditioning fallback using already certified physical charge
//! coordinates. The model and physical admission tolerances are unchanged.
use super::*;
pub(super) struct Coordinates {
    potential:Vec<Vec<f64>>, shape:usize,
    g:Matrix,e:Matrix,
}
#[cfg(test)]
mod tests {
    use super::*;
    fn model()->Model {
        let mut a=vec![];for(j,[u,v])in [[0,1],[1,2],[3,4],[4,5]].into_iter().enumerate(){a.extend([(u,j,1e4),(v,j,-1e4)]);}
        Model{format:"coupled-rlc".into(),reference_topology:"infinity".into(),potential_nodes:(0..6).map(|i|format!("p{i}")).collect(),current_dofs:(0..4).map(|i|format!("i{i}")).collect(),incidence:a,port_incidence:vec![(0,0,1.),(3,0,-1.),(2,1,1.),(5,1,-1.)],resistance_ohm:(0..4).map(|i|(i,i,1.)).collect(),inductance_h:(0..4).map(|i|(i,i,2e-6)).collect(),capacitance_f:(0..6).map(|i|(i,i,1e-14)).collect(),conserved_potential_modes:vec![vec![1.,1.,1.,0.,0.,0.],vec![0.,0.,0.,1.,1.,1.]],structural_gauge_modes:vec![],port_terminals:None}
    }
    #[test]
    fn physical_charge_coordinates_preserve_small_loss_and_invalidate_after_load() {
        let mut system=System::compile(&model(),2,None).unwrap();let n=system.linear.n;let mut stats=Counters::default();
        // Exact original accepted 17-frequency coupon sweep.
        let frequencies=[1000.0, 1333.5214321633239, 1778.2794100389228, 2371.373705661655, 3162.2776601683795, 4216.965034285822, 5623.413251903491, 7498.942093324558, 10000.0, 13335.21432163324, 17782.79410038923, 23713.737056616552, 31622.776601683792, 42169.65034285823, 56234.13251903491, 74989.42093324558, 100000.0];
        for f in frequencies {for drive in [[1.,0.],[0.,1.],[1.,-1.]] {
            let mut rhs=system.port_rhs(&drive);rhs.resize(2*n,0.);
            let x=system.solve_charge_coordinates(2.*std::f64::consts::PI*f,&rhs,&mut stats).unwrap();
            // Independent low-RC limit for both chains, including opposite drive.
            let a=(2.*drive[0]-drive[1])/3e4;let b=(drive[0]-2.*drive[1])/3e4;
            for(j,expected)in [a,b,-a,-b].into_iter().enumerate(){assert!((x[6+j]-expected).abs()/expected.abs()<1e-8);}
            let loss=system.joule_loss(&x[..n])+system.joule_loss(&x[n..]);
            let volts=system.port_voltages(&x,2);let source=drive.iter().zip(volts).map(|(i,v)|i*v).sum::<f64>();
            let expected=2.*(a*a+b*b);
            assert!((loss-expected).abs()/expected<1e-8);
            assert!((source-loss).abs()/loss<1e-8);
        }}
        assert!(system.charge_coordinates.is_some());
        for conductance in [1./50.,1./100.] {
            system.add_port_admittance(0,conductance,0.).unwrap();assert!(system.charge_coordinates.is_none());
            let total=if conductance==1./50. {1./50.}else{1./50.+1./100.};
            for f in frequencies {
                let mut rhs=system.port_rhs(&[1.,0.]);rhs.resize(2*n,0.);
                let x=system.solve_charge_coordinates(2.*std::f64::consts::PI*f,&rhs,&mut stats).unwrap();
                assert!((system.port_voltages(&x,2)[0]-1./total).abs()*total<1e-8);
            }
        }
    }
    #[test]
    fn cached_coordinate_corruption_is_rejected_by_original_physical_residual() {
        let mut system=System::compile(&model(),2,None).unwrap();let n=system.linear.n;
        let mut rhs=system.port_rhs(&[1.,0.]);rhs.resize(2*n,0.);
        let omega=2.*std::f64::consts::PI*1e4;
        system.solve_charge_coordinates(omega,&rhs,&mut Counters::default()).unwrap();
        for column in &mut system.charge_coordinates.as_mut().unwrap().potential {for v in column {*v*=2.;}}
        assert!(system.solve_charge_coordinates(omega,&rhs,&mut Counters::default()).unwrap_err().starts_with("COUPLED_COORDINATES:"));

    }
}
fn dot(a:&[f64],b:&[f64])->f64 {a.iter().zip(b).map(|(a,b)|a*b).sum()}
impl System {
    fn build_charge_coordinates(&self)->Result<Coordinates> {
        let nv=self.nv;let ni=self.ni;let mut potential:Vec<Vec<f64>>=vec![];
        // Disjoint component certificates permit the exact integer transform
        // [non-anchor coordinates, component constants]. This avoids creating
        // tiny artificial C couplings through an orthogonalization residue.
        let mut used=vec![false;nv];let mut indicators=vec![];let mut anchors=vec![];let mut indicator_form=true;
        for q in &self.candidates {
            let Some(anchor)=q.iter().position(|&v|v!=0.) else {return Err(fail("COUPLED_COORDINATES","empty charge certificate"));};
            if q.iter().enumerate().any(|(i,&v)|v!=0.&&(v!=q[anchor]||used[i])) {indicator_form=false;break;}
            let mut h=vec![0.;nv];for(i,&v)in q.iter().enumerate(){if v!=0. {h[i]=1.;used[i]=true;}}anchors.push(anchor);indicators.push(h);
        }
        let shape;
        if indicator_form {
            for axis in 0..nv {if !anchors.contains(&axis){let mut v=vec![0.;nv];v[axis]=1.;potential.push(v);}}
            shape=potential.len();potential.extend(indicators);
        }else{
            for axis in 0..nv {
                let mut v=vec![0.;nv];v[axis]=1.;
                for _ in 0..2 {for q in self.candidates.iter().chain(&potential){let x=dot(&v,q);for(v,q)in v.iter_mut().zip(q){*v-=x*q;}}}
                let length=dot(&v,&v).sqrt();if length>64.*f64::EPSILON*(nv+1)as f64 {for x in &mut v {*x/=length;}potential.push(v);}
            }
            shape=potential.len();potential.extend(self.candidates.iter().cloned());
        }
        if potential.len()!=nv {return Err(fail("COUPLED_COORDINATES","certified charge/complement rank mismatch"));}
        let mut g=Matrix::new();let mut e=Matrix::new();
        let mut incidence=vec![vec![0.;ni];nv];for(&(i,j),&v)in &self.linear.g {if i<nv&&j>=nv {incidence[i][j-nv]=v;}}
        for a in 0..shape {for j in 0..ni {
            let value=(0..nv).map(|i|potential[a][i]*incidence[i][j]).sum::<f64>();
            if value!=0. {g.insert((a,nv+j),value);g.insert((nv+j,a),-value);}
        }}
        // Charge-mode A^T coupling is exactly zero by the certificate already
        // checked at compile. Do not amplify a summation residue by a large
        // capacitive potential. No C mode or mutual energy is discarded.
        let images:Vec<_>=potential.iter().map(|q| {
            let mut out=vec![0.;nv];for(&(i,j),v)in &self.linear.e {if i<nv&&j<nv {out[i]+=v*q[j];}}out
        }).collect();
        for a in 0..nv {for b in a..nv {let value=dot(&potential[a],&images[b]);if !value.is_finite(){return Err(fail("COUPLED_COORDINATES","nonfinite projected C"));}if value!=0. {e.insert((a,b),value);if a!=b {e.insert((b,a),value);}}}}
        // Physical port conductances can anchor a formerly free charge mode.
        // They are retained in full, independently from the certified A zero.
        let load_images:Vec<_>=potential.iter().map(|q| {let mut out=vec![0.;nv];for(&(i,j),v)in &self.linear.g {if i<nv&&j<nv {out[i]+=v*q[j];}}out}).collect();
        for a in 0..nv {for b in a..nv {let value=dot(&potential[a],&load_images[b]);if value!=0. {g.insert((a,b),value);if a!=b {g.insert((b,a),value);}}}}
        for(&(i,j),&v)in &self.linear.g {if i>=nv&&j>=nv {g.insert((i,j),v);}}
        for(&(i,j),&v)in &self.linear.e {if i>=nv&&j>=nv {e.insert((i,j),v);}}
        Ok(Coordinates{potential,shape,g,e})
    }
    pub(crate) fn solve_charge_coordinates(&mut self,omega:f64,rhs:&[f64],stats:&mut Counters)->Result<Vec<f64>> {
        Ok(self.solve_charge_coordinates_impl(omega,rhs,stats,false)?.0)
    }
    /// Return physical coordinates and the potential shape before adding island constants.
    /// The shape has the same real/imaginary layout; its current entries are zero.
    pub(crate) fn solve_charge_coordinates_split(&mut self,omega:f64,rhs:&[f64],stats:&mut Counters)->Result<(Vec<f64>,Vec<f64>)> {
        self.solve_charge_coordinates_impl(omega,rhs,stats,true)
    }
    fn solve_charge_coordinates_impl(&mut self,omega:f64,rhs:&[f64],stats:&mut Counters,retain_shape:bool)->Result<(Vec<f64>,Vec<f64>)> {
        if self.charge_coordinates.is_none() {self.charge_coordinates=Some(self.build_charge_coordinates()?);}
        let coordinates=self.charge_coordinates.as_ref().unwrap();let n=self.linear.n;let nv=self.nv;
        let mut matrix=coordinates.g.clone();
        for(&(i,j),&v)in &coordinates.g {matrix.insert((n+i,n+j),v);}
        for(&(i,j),&v)in &coordinates.e {matrix.insert((i,n+j),-omega*v);matrix.insert((n+i,j),omega*v);}
        let mut b=rhs.to_vec();for part in 0..2 {for a in 0..nv {b[part*n+a]=dot(&coordinates.potential[a],&rhs[part*n..part*n+nv]);}}
        // Exact power-of-two row/column equilibration changes coordinates only.
        // The original equations are checked again below, including their old
        // absolute residual floor. No diagonal physical term is introduced.
        let scaling=|x:f64|->f64 {if x==0. {1.}else{2_f64.powi((-x.log2().floor()).clamp(-512.,512.) as i32)}};
        let mut row_max=vec![0_f64;2*n];for(&(i,_),v)in &matrix {row_max[i]=row_max[i].max(v.abs());}
        let row:Vec<_>=row_max.into_iter().map(scaling).collect();let mut column_max=vec![0_f64;2*n];
        for(&(i,j),v)in &matrix {column_max[j]=column_max[j].max((row[i]*v).abs());}
        let column:Vec<_>=column_max.into_iter().map(scaling).collect();
        for(&(i,j),v)in matrix.iter_mut(){*v=(*v*row[i])*column[j];}
        for(i,v)in b.iter_mut().enumerate(){*v*=row[i];}
        let(y,res,_)=self.linear.solve(matrix,b,stats)?;let coordinate:Vec<_>=y.iter().zip(&column).map(|(v,s)|v*s).collect();
        let mut x=coordinate.clone();for part in 0..2 {for i in 0..nv {x[part*n+i]=(0..nv).map(|a|coordinates.potential[a][i]*coordinate[part*n+a]).sum();}}
        let original=self.linear.matrix(0.,Some(omega));let mut residual=rhs.iter().map(|v|-v).collect::<Vec<_>>();let mut magnitude=rhs.iter().map(|v|v.abs()).collect::<Vec<_>>();
        for(&(i,j),v)in &original {residual[i]+=v*x[j];magnitude[i]+=(v*x[j]).abs();}
        let normalized=residual.iter().zip(&magnitude).map(|(r,s)|r.abs()/s.max(1e-15)).fold(0.,f64::max);
        if normalized>1e-8||x.iter().any(|v|!v.is_finite()){return Err(fail("COUPLED_COORDINATES",format!("original physical residual={normalized:e}")));}
        self.residual=self.residual.max(res).max(normalized);
        self.kcl=self.kcl.max(residual[..nv].iter().chain(&residual[n..n+nv]).map(|v|v.abs()).fold(0.,f64::max));
        let mut shape=vec![];if retain_shape {shape.resize(2*n,0.);for part in 0..2 {for i in 0..nv {shape[part*n+i]=(0..coordinates.shape).map(|a|coordinates.potential[a][i]*coordinate[part*n+a]).sum();}}}
        Ok((x,shape))
    }
}
