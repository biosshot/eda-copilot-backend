//! Probe of touching prism x tetra cross entries sampled from the TPS63060DSCR 1 MHz run (`P7_CROSS_DUMP`).
use super::*;
use std::time::Instant;

fn samples() -> Vec<(super::super::prism::Prism,[[f64;3];4],f64)> {
    let Ok(path)=std::env::var("P7_CROSS_SAMPLES") else {return Vec::new();};
    std::fs::read_to_string(path).unwrap().lines().filter_map(|l|{
        let v:serde_json::Value=serde_json::from_str(l).ok()?;
        let tri:[[f64;2];3]=serde_json::from_value(v["triangle"].clone()).ok()?;
        let z:[f64;2]=serde_json::from_value(v["z"].clone()).ok()?;
        let tet:[[f64;3];4]=serde_json::from_value(v["tetra"].clone()).ok()?;
        Some((super::super::prism::Prism::new("s".into(),tri,z).ok()?,tet,v["value"].as_f64()?))
    }).collect()
}

#[test]
#[ignore]
fn probe_ladder() {
    let s=samples();
    eprintln!("samples {}",s.len());
    for (k,(p,t,old)) in s.iter().enumerate().take(12) {
        let [pa,pb,pc]=p.triangle_m;let [lo,hi]=p.z_m;
        let (a0,b0,c0)=([pa[0],pa[1],lo],[pb[0],pb[1],lo],[pc[0],pc[1],lo]);
        let (a1,b1,c1)=([pa[0],pa[1],hi],[pb[0],pb[1],hi],[pc[0],pc[1],hi]);
        let mut line=format!("#{k} old {old:e}:");
        for part in [[a0,b0,c0,c1],[a0,b0,b1,c1],[a0,a1,b1,c1]] {
            let (target,source_tetra)=(part,*t);
            let source=super::super::polyhedron::PointPotential::tetrahedron(source_tetra).unwrap().with_cached_positive_geometry();
            let mut prev=0.;let mut desc=String::new();
            for order in [5,7,10,14,16] {
                let st=Instant::now();
                let v=super::super::polyhedron::tetra_integral_checked(target,&source,order,&mut||Ok(())).unwrap();
                desc+=&format!(" o{order}:{:.1}ms chg {:.1e};",st.elapsed().as_secs_f64()*1e3,if prev>0.{(v-prev).abs()/v}else{0.});
                prev=v;
            }
            line+=&format!("\n   part{desc}");
        }
        eprintln!("{line}");
    }
}

const PINNED:[([[f64;2];3],[f64;2],[[f64;3];4],f64);4]=[
([[-0.005930353045463562, 0.008146708965301513], [-0.005950081944465637, 0.008128209948539734], [-0.0045739257633686066, 0.007673502311110496]],[0.0, 1.1666666666666666e-05],[[-0.00460815167427063, 0.008548463344573974, 0.0], [-0.00460815167427063, 0.008548463344573974, 1.1666666666666666e-05], [-0.004591038718819618, 0.008536321073770524, 1.1666666666666666e-05], [-0.0045739257633686066, 0.007673502311110496, 1.1666666666666666e-05]],6.1241160598736755e-24),
([[-0.005930353045463562, 0.008146708965301513], [-0.005950081944465637, 0.008128209948539734], [-0.0045739257633686066, 0.007673502311110496]],[1.1666666666666666e-05, 2.3333333333333332e-05],[[-0.00455548357963562, 0.008511093378067016, 0.0], [-0.0045739257633686066, 0.008524178802967072, 0.0], [-0.0045739257633686066, 0.008524178802967072, 1.1666666666666666e-05], [-0.0045739257633686066, 0.007673502311110496, 1.1666666666666666e-05]],6.518487174331731e-24),
([[-0.005930353045463562, 0.009853291034698487], [-0.0045739257633686066, 0.010323897704482079], [-0.005940922692418099, 0.00986265490949154]],[2.3333333333333332e-05, 3.5e-05],[[-0.00449896252155304, 0.009517544746398926, 0.0], [-0.00455548357963562, 0.00948630666732788, 0.0], [-0.0045739257633686066, 0.010323897704482079, 0.0], [-0.0045739257633686066, 0.010323897704482079, 1.1666666666666666e-05]],1.1287611716782321e-23),
([[-0.011051363348960877, 0.0008916482925415039], [-0.011094395399093628, 0.0008434953689575196], [-0.010862979888916015, 0.0007260082960128785]],[1.1666666666666666e-05, 2.3333333333333332e-05],[[-0.011051363348960877, 0.0008916482925415039, 0.0], [-0.011051363348960877, 0.0008916482925415039, 1.1666666666666666e-05], [-0.010862979888916015, 0.0007260082960128785, 1.1666666666666666e-05], [-0.010964578866958619, 0.0009258742034435272, 1.1666666666666666e-05]],6.043318389119667e-23)
];

fn cross_value(p:&super::super::prism::Prism,t:[[f64;3];4])->f64 { cross_value_with(p,t,None) }
fn cross_value_with(p:&super::super::prism::Prism,t:[[f64;3];4],strict:Option<f64>)->f64 {
    let [pa,pb,pc]=p.triangle_m;let [lo,hi]=p.z_m;
    let (a0,b0,c0)=([pa[0],pa[1],lo],[pb[0],pb[1],lo],[pc[0],pc[1],lo]);
    let (a1,b1,c1)=([pa[0],pa[1],hi],[pb[0],pb[1],hi],[pc[0],pc[1],hi]);
    [[a0,b0,c0,c1],[a0,b0,b1,c1],[a0,a1,b1,c1]].into_iter().map(|part|{
        let mut check=||Ok(());
        match strict {Some(tol)=>super::super::volume_integral_magnetic::tet_near_fast_tol(part,t,false,tol,&mut check).unwrap().0,None=>super::super::volume_integral_magnetic::tet_near_fast_any(part,t,&mut check).unwrap().0}
    }).sum()
}

/// Touching prism x tetra entries from the 1 MHz run: the relaxed point-potential tolerance of the cross path
/// stays within 1e-3 of the strict value recorded by the old path.
#[test]
fn pinned_touching_pairs_match_the_strict_path() {
    for (tri,z,tet,old) in PINNED {
        let p=super::super::prism::Prism::new("s".into(),tri,z).unwrap();
        let new=cross_value(&p,tet);
        assert!(((new-old)/old).abs()<=1e-3,"new {new:e} old {old:e}");
    }
}

#[test]
#[ignore]
fn pin_sample_file() {
    let s=samples();
    assert!(!s.is_empty());
    let (mut old_sum,mut new_sum,mut worst,mut t_new)=(0.,0.,0f64,0.);
    for (p,t,old) in &s {
        let _=old;let old=&cross_value_with(p,*t,Some(1e-10));let st=Instant::now();let new=cross_value(p,*t);t_new+=st.elapsed().as_secs_f64();
        old_sum+=old;new_sum+=new;worst=worst.max(((new-old)/old).abs());
    }
    eprintln!("samples {} worst rel {worst:e} summed rel {:e} new seconds {t_new:.2}",s.len(),((new_sum-old_sum)/old_sum).abs());
    assert!(worst<=1e-3&&((new_sum-old_sum)/old_sum).abs()<=1e-4);
}
