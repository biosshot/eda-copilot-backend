//! Two-F64 local arithmetic for cancelling boundary integrals. No transport type.
use super::{accurate::Sum,error,Result};
use std::{ops::{Add,Sub,Mul,Div,Neg},sync::OnceLock};
#[path="compensated_tables.rs"]mod tables;
#[derive(Clone,Copy,Default)]
pub(super) struct W(pub Sum);
impl W{
 pub const ZERO:Self=Self(Sum{hi:0.,lo:0.});
 pub const ONE:Self=Self(Sum{hi:1.,lo:0.});
 pub const PI:Self=Self(Sum{hi:std::f64::consts::PI,lo:1.2246467991473532e-16});
 const LN2:Self=Self(Sum{hi:std::f64::consts::LN_2,lo:2.3190468138462996e-17});
 #[inline]pub fn f(x:f64)->Self{Self(Sum::from(x))}
 #[inline]pub fn value(self)->f64{self.0.value()}
 #[inline]pub fn scale(self,x:f64)->Self{Self(self.0.mul(x))}
 #[inline]fn bits(p:[u64;2])->Self{Self(Sum{hi:f64::from_bits(p[0]),lo:f64::from_bits(p[1])})}
 #[inline]pub fn abs(self)->Self{if self.value()<0.{-self}else{self}}
 pub fn sqrt(self)->Self{if self.value()==0.{return Self::ZERO;}let mut x=Self::f(self.value().sqrt());for _ in 0..2{x=x+(self-x*x)/x.scale(2.);if(self-x*x).value().abs()<=8.*f64::EPSILON*f64::EPSILON*self.value().abs(){break;}}x}
 pub fn ln(self)->Self{
  if !(self.value()>0.)||!self.value().is_finite(){return Self::f(f64::NAN);}
  let exponent=((self.0.hi.to_bits()>>52)&2047)as i32-1023;
  if exponent<=-1023{return Self::f(f64::NAN);}
  let e=exponent;let m=self.scale(2_f64.powi(-e));let k=((m.value()-1.)*128.).round().clamp(0.,128.)as usize;let c=Self::f(1.+k as f64/128.);
  let z=(m-c)/(m+c);let z2=z*z;let mut term=z;let mut sum=z;
  for k in 1..30{term=term*z2;sum=sum+term*reciprocal(2*k+1);let tail=2.*term.value().abs()*z2.value().abs()/((2*k+3)as f64*(1.-z2.value().abs()));if tail<=1e-35*sum.value().abs(){break;}}
  (if k==0{sum.scale(2.)}else{sum.scale(2.)+Self::bits(tables::LN[k])})+Self::LN2.scale(e as f64)
 }
 pub fn atan2(self,x:Self)->Self{
  let ay=self.abs();let ax=x.abs();if self.value()==0.{return if x.value()<0.{Self::PI}else{Self::ZERO};}if x.value()==0.{return Self::PI.scale(if self.value()>0.{0.5}else{-0.5});}
  let inverted=ay.value()>ax.value();let t=if inverted{ax/ay}else{ay/ax};let k=(t.value()*128.).round().clamp(0.,128.)as usize;let c=Self::f(k as f64/128.);
  let z=if k==0{t}else{(t-c)/(Self::ONE+t*c)};let z2=-(z*z);let mut term=z;let mut a=z;
  for k in 1..56{term=term*z2;a=a+term*reciprocal(2*k+1);let tail=2.*term.value().abs()*z2.value().abs()/(2*k+3)as f64;if tail<=1e-35*a.value().abs(){break;}}
  if k!=0{a=a+Self::bits(tables::ATAN[k]);}if inverted{a=Self::PI.scale(0.5)-a;}if x.value()<0.{a=Self::PI-a;}if self.value()<0.{a=-a;}a
 }
}
impl Add for W{type Output=Self;#[inline]fn add(self,b:Self)->Self{Self(self.0.add(b.0))}}
impl Sub for W{type Output=Self;#[inline]fn sub(self,b:Self)->Self{Self(self.0.sub(b.0))}}
impl Neg for W{type Output=Self;#[inline]fn neg(self)->Self{Self(self.0.neg())}}
impl Mul for W{type Output=Self;#[inline]fn mul(self,b:Self)->Self{let p=self.0.hi*b.0.hi;let cross=self.0.hi*b.0.lo+self.0.lo*b.0.hi;let e=(self.0.hi.mul_add(b.0.hi,-p)+cross)+self.0.lo*b.0.lo;let hi=p+e;Self(Sum{hi,lo:e-(hi-p)})}}
impl Div for W{type Output=Self;#[inline]fn div(self,b:Self)->Self{let q0=self.0.hi/b.0.hi;let r=self-b.scale(q0);let q1=r.0.hi/b.0.hi;let r=r-b.scale(q1);Self::f(q0)+Self::f(q1)+Self::f(r.0.hi/b.0.hi)}}
fn reciprocal(n:usize)->W{static INVERSE:OnceLock<[W;128]>=OnceLock::new();INVERSE.get_or_init(||std::array::from_fn(|k|if k==0{W::ZERO}else{W::ONE/W::f(k as f64)}))[n]}
pub(super) struct Rule{pub nodes:Vec<W>,pub weights:Vec<W>}
fn legendre(n:usize,x:W)->(W,W){let(mut a,mut b)=(W::ONE,x);for k in 2..=n{let c=(x*b.scale((2*k-1)as f64)-a.scale((k-1)as f64))/W::f(k as f64);a=b;b=c;}(b,(x*b-a).scale(n as f64)/(x*x-W::ONE))}
pub(super) fn rule(order:usize)->Result<&'static Rule>{
 static RULES:[OnceLock<Rule>;65]=[const{OnceLock::new()};65];
 if !(2..=64).contains(&order){return Err(error("COMPENSATED_QUADRATURE","Gauss order2..64 required"));}if let Some(r)=RULES[order].get(){return Ok(r);}
 let base=super::prism::cached_rule(order)?;let mut nodes=Vec::with_capacity(order);let mut weights=Vec::with_capacity(order);
 for &u in base.nodes(){let mut x=W::f(2.*u-1.);for _ in 0..4{let(p,d)=legendre(order,x);x=x-p/d;}let(_,d)=legendre(order,x);nodes.push((x+W::ONE).scale(0.5));weights.push(W::ONE/((W::ONE-x*x)*d*d));}
 for k in 0..=2{let mut sum=W::ZERO;for(&x,&w)in nodes.iter().zip(&weights){if !(x.value()>0.&&x.value()<1.&&w.value()>0.){return Err(error("COMPENSATED_QUADRATURE","positive interior Gauss data required"));}let mut power=W::ONE;for _ in 0..k{power=power*x;}sum=sum+w*power;}let expected=W::ONE/W::f((k+1)as f64);if(sum-expected).value().abs()>1e-28{return Err(error("COMPENSATED_QUADRATURE","Gauss constant/linear/quadratic moments fail"));}}
 Ok(RULES[order].get_or_init(||Rule{nodes,weights}))
}
#[cfg(test)]
mod tests{
 use super::*;
 include!("../../../../../tests/fixtures/analysis-local-extraction/board-integration/compensated-elementary-reference.rs");
 include!("../../../../../tests/fixtures/analysis-local-extraction/board-integration/compensated-table-grid-reference.rs");
 include!("../../../../../tests/fixtures/analysis-local-extraction/board-integration/compensated-arithmetic-grid-reference.rs");
 fn parts(p:[u64;2])->W{W(Sum{hi:f64::from_bits(p[0]),lo:f64::from_bits(p[1])})}
 #[test]fn elementary_functions_match_independent_frozen_mpmath(){for &(input,expected)in &LN{let e=parts(expected);assert!((parts(input).ln()-e).value().abs()<=1e-28*e.value().abs().max(1e-100));}for &(y,x,expected)in &ATAN{let e=parts(expected);assert!((parts(y).atan2(parts(x))-e).value().abs()<1e-28*e.value().abs().max(1e-100));}}
 #[test]fn centered_elementary_series_match_independent_off_grid_mpmath(){for &(input,expected)in LN_GRID{let e=parts(expected);assert!((parts(input).ln()-e).value().abs()<=1e-28*e.value().abs().max(1e-100));}for &(y,x,expected)in ATAN_GRID{let e=parts(expected);assert!((parts(y).atan2(parts(x))-e).value().abs()<1e-28*e.value().abs().max(1e-100));}}
 #[test]fn corrected_division_and_checked_sqrt_match_independent_mpmath(){for &(input,expected)in SQRT_GRID{let e=parts(expected);assert!((parts(input).sqrt()-e).value().abs()<=1e-29*e.value().abs());}for &(a,b,expected)in DIV_GRID{let e=parts(expected);assert!((parts(a)/parts(b)-e).value().abs()<=1e-29*e.value().abs());}}
 #[test]fn positive_gauss_moments_through_full_polynomial_degree(){for q in [2,4,8,16,32,64]{let r=rule(q).unwrap();for k in 0..2*q{let mut sum=W::ZERO;for(&x,&w)in r.nodes.iter().zip(&r.weights){let mut v=W::ONE;for _ in 0..k{v=v*x;}sum=sum+w*v;}assert!((sum-W::ONE/W::f((k+1)as f64)).value().abs()<1e-28,"q{q} moment{k}");}}}
}
