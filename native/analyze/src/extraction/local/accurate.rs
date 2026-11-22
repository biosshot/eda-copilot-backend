//! Local compensated F64 residual arithmetic. No public state/type changes.
#[derive(Clone, Copy, Default)]
pub(crate) struct Sum { pub hi:f64, pub lo:f64 }
impl Sum {
    pub fn from(value:f64)->Self {Self{hi:value,lo:0.}}
    pub fn add(self,other:Self)->Self {
        let sum=self.hi+other.hi;
        let virtual_b=sum-self.hi;
        let error=(self.hi-(sum-virtual_b))+(other.hi-virtual_b);
        let tail=error+self.lo+other.lo;
        let hi=sum+tail;
        let virtual_tail=hi-sum;
        Self{hi,lo:(sum-(hi-virtual_tail))+(tail-virtual_tail)}
    }
    pub fn neg(self)->Self {Self{hi:-self.hi,lo:-self.lo}}
    pub fn sub(self,other:Self)->Self {self.add(other.neg())}
    pub fn mul(self,value:f64)->Self {
        let product=self.hi*value;
        let error=self.hi.mul_add(value,-product)+self.lo*value;
        let hi=product+error;
        Self{hi,lo:error-(hi-product)}
    }
    pub fn value(self)->f64 {self.hi+self.lo}
}
#[test]
fn error_free_f64_sum_and_fma_product_retain_small_terms() {
    let unit=Sum::from(1e16).add(Sum::from(1.)).sub(Sum::from(1e16));
    assert_eq!(unit.value(),1.);
    let d=2_f64.powi(-27);
    let product=Sum::from(1.+d).mul(1.+d).sub(Sum::from(1.));
    assert_eq!(product.value(),2.*d+d*d);
    let x=Sum::from(1.).add(Sum::from(2_f64.powi(-70)));
    assert_eq!(x.sub(Sum::from(1.)).value(),2_f64.powi(-70));
}
