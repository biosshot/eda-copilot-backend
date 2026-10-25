//! Bounded physical foil coupon -> extracted descriptor -> existing P6.
use pcb_analyze::{
    electrical::{
        coupled::{self, System},
        Counters,
    },
    extraction::local::{
        current_space, layered_capacitance::extract_traced, reduction::project_currents,
    },
};
use serde_json::json;
use std::time::{Duration, Instant};
#[path = "../../../tests/fixtures/analysis-local-extraction/current-space/fixture.rs"]
mod fixture;
fn main() {
    let total = Instant::now();
    let mut runs = vec![];
    for name in ["cold", "same-process-repeat"] {
        let start = Instant::now();
        let (input, trace, foils) = fixture::quotient();
        let electric = extract_traced(&input, &trace, fixture::budget()).unwrap();
        let electric_seconds = start.elapsed().as_secs_f64();
        let space = current_space::assemble(
            &foils,
            &electric,
            &[[0, 1], [2, 3]],
            1e5,
            current_space::Budget {
                max_bytes: 512 * 1024 * 1024,
                max_wall: Duration::from_secs(120),
            },
        )
        .unwrap();
        let q = 1. / 2f64.sqrt();
        let reduced = project_currents(
            &space.model,
            2,
            vec![
                vec![q, 0., q, 0., 0., 0., 0., 0.],
                vec![0., 0., 0., 0., q, 0., q, 0.],
            ],
            512 * 1024 * 1024,
            Duration::from_secs(60),
        )
        .unwrap();
        let frequencies: Vec<_> = (0..17).map(|i| 1e3 * 10f64.powf(i as f64 / 8.)).collect();
        let mut p6 = System::compile(&space.model, 2, None).unwrap();
        let mut rp6 = System::compile(&reduced.model, 2, None).unwrap();
        let mut sweep = vec![];
        for &f in &frequencies {
            let mut rhs = p6.port_rhs(&[1., -1.]);
            rhs.resize(24, 0.);
            let x_result = p6
                .solve(
                    2. * std::f64::consts::PI * f,
                    rhs,
                    &[0.; 12],
                    &mut Counters::default(),
                );
            let mut rhs = rp6.port_rhs(&[1., -1.]);
            rhs.resize(12, 0.);
            let rx_result = rp6
                .solve(
                    2. * std::f64::consts::PI * f,
                    rhs,
                    &[0.; 6],
                    &mut Counters::default(),
                );
            let (x,rx)=match (x_result,rx_result) {
                (Ok(x),Ok(rx))=>(x,rx),
                (x,rx)=>{sweep.push(json!({"frequencyHz":f,"nativePointAdmitted":false,"diagnostic":format!("full={:?}; reduced={:?}",x.err(),rx.err())}));continue;}
            };
            let ir = reduced.recover_currents(&rx[4..6]).unwrap();
            let ii = reduced.recover_currents(&rx[10..12]).unwrap();
            let loss = p6.joule_loss(&x[..12]) + p6.joule_loss(&x[12..]);
            let reduced_loss = rp6.joule_loss(&rx[..6]) + rp6.joule_loss(&rx[6..]);
            let z = [x[0] - x[1] - x[2] + x[3], x[12] - x[13] - x[14] + x[15]];
            let rz = [rx[0] - rx[1] - rx[2] + rx[3], rx[6] - rx[7] - rx[8] + rx[9]];
            let norm = x[4..12]
                .iter()
                .chain(&x[16..24])
                .map(|v| v.abs())
                .fold(0., f64::max);
            let error = (0..8)
                .map(|i| (ir[i] - x[4 + i]).abs().max((ii[i] - x[16 + i]).abs()) / norm)
                .fold(0., f64::max);
            let admission=coupled::impedance(&fixture::loop_model(&space.model),1,&[f],None);
            let admitted=admission.is_ok();
            let diagnostic=admission.err();
            sweep.push(json!({"frequencyHz":f,"nativePointAdmitted":admitted,"diagnostic":diagnostic,"loopZOhm":z,"reducedLoopZOhm":rz,"currentsRealA":&x[4..12],"currentsImagA":&x[16..24],"recoveredRealA":ir,"recoveredImagA":ii,"currentNormalizedError":error,"jouleLossWPeakNorm":loss,"reducedJouleLossWPeakNorm":reduced_loss,"cellLossesWPeakNorm":space.cell_losses_w(&x[4..12]).unwrap().iter().zip(space.cell_losses_w(&x[16..24]).unwrap()).map(|(a,b)|a+b).collect::<Vec<_>>() }));
        }
        let rhs = p6.port_rhs(&[1., -1.]);
        let dc = p6
            .solve(0., rhs, &[0.; 12], &mut Counters::default())
            .unwrap();
        let full_z = coupled::impedance(&fixture::loop_model(&space.model), 1, &frequencies, None);
        let reduced_z = coupled::impedance(&fixture::loop_model(&reduced.model), 1, &frequencies, None);
        let seconds = start.elapsed().as_secs_f64();
        runs.push(json!({"name":name,"input":input,"trace":trace,"foils":foils,"model":space.model,"loopModel":fixture::loop_model(&space.model),"reducedLoopModel":fixture::loop_model(&reduced.model),"reducedModel":reduced.model,"currentProjection":reduced.basis,"directedKernelM5":space.directed_kernel_m5,"rawReactionCapacitanceF":electric.capacitance_f,"fieldPlan":electric.plan,"currentPlanBytes":space.planned_bytes,"reductionPlanBytes":reduced.planned_bytes,"electricSeconds":electric_seconds,"currentAssemblySeconds":space.elapsed_seconds,"reductionSeconds":reduced.elapsed_seconds,"fullPathSeconds":seconds,"maxQuadratureChange":space.max_quadrature_change,"rawReciprocity":space.max_reciprocity_error,"dcState":dc,"dcDensityAPerM2":space.recover_density(&dc[4..]).unwrap(),"dcCellLossesW":space.cell_losses_w(&dc[4..]).unwrap(),"frequency":full_z,"reducedFrequency":reduced_z,"sweep":sweep}));
    }
    println!(
        "{}",
        json!({"status":"partial","dcStatus":"complete","acBandAdmission":"rejected","class":"insulated analytic two-foil thin-volume coupon; no via, PCB or broadband admission","bandHz":[0.,1e5],"repeatMeaning":"fresh field/kernel/model construction in same process; no cached factorization or geometry acceleration claimed","totalSeconds":total.elapsed().as_secs_f64(),"runs":runs})
    );
}
