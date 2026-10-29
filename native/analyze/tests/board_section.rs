use pcb_analyze::{
    extraction::{
        board_section::{self, Slice},
        local::board_mesh::Layer,
        polygon_section,
    },
    geometry::{self, GeometryOptions},
};
#[test]
fn captured_bandamp_rf_section_preserves_finite_coplanar_copper_and_source_mapping() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/pcb-analyze/BandAmpV5/BandAmpV5.json");
    let text = std::fs::read_to_string(path).unwrap();
    let board = pcb_analyze::import_json(&text).board.unwrap();
    let track = board
        .tracks
        .iter()
        .find(|t| t.source.id.as_deref() == Some("d334a42d8fb15d25"))
        .expect("original RF_OUT_PORT track20 must remain captured");
    assert_eq!(board.nets[track.net.unwrap()], "RF_OUT_PORT");
    assert_eq!(track.points.len(), 2);
    let a = track.points[0];
    let b = track.points[1];
    let length = (b.x - a.x).hypot(b.y - a.y);
    assert!(length > 0.5);
    let g = geometry::prepare(&board, GeometryOptions::default());
    // These are the existing authored two-layer stack hypotheses, not measured
    // laminate properties or a 5% full-board accuracy certificate.
    let stack = vec![
        Layer {
            name: "TOP".into(),
            thickness_m: 35e-6,
            copper: true,
            epsilon_r: None,
            conductivity_s_m: Some(58e6),
        },
        Layer {
            name: "authored-FR4".into(),
            thickness_m: 1.53e-3,
            copper: false,
            epsilon_r: Some(4.2),
            conductivity_s_m: None,
        },
        Layer {
            name: "BOTTOM".into(),
            thickness_m: 35e-6,
            copper: true,
            epsilon_r: None,
            conductivity_s_m: Some(58e6),
        },
    ];
    let slice = Slice {
        at_mm: [(a.x + b.x) / 2., (a.y + b.y) / 2.],
        transverse_unit: [-(b.y - a.y) / length, (b.x - a.x) / length],
        half_width_mm: 2.,
        air_margin_mm: 1.5,
        reference_layer: "BOTTOM".into(),
        reference_source_path: g
            .regions
            .iter()
            .find(|r| {
                r.layer == "BOTTOM" && r.source.source_id.as_deref() == Some("492c44dea9c90d31")
            })
            .expect("captured bottom ground pour")
            .source
            .path
            .clone(),
        reference_x_mm: 0.,
    };
    let extracted = board_section::slice(&g, &stack, &slice).unwrap();
    assert!(extracted["sourceMapping"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["sourcePaths"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p.as_str() == Some(&track.source.path))));
    let mut request: polygon_section::Request =
        serde_json::from_value(extracted["geometry"].clone()).unwrap();
    request.max_triangle_area_m2 = 2e-8;
    let result = polygon_section::solve(&request).unwrap();
    assert!(
        result["conductorOrder"].as_array().unwrap().len() > 1,
        "coplanar copper must not be replaced by nearest microstrip template"
    );
    assert!(
        result["quality"]["normalizedEnergyReactionError"]
            .as_f64()
            .unwrap()
            < 1e-8
    );
    request.loss=Some(polygon_section::LossInput{frequencies_hz:vec![1e9],conductivity_s_m:vec![58e6;request.conductors.len()],minimum_skin_dimensions_m:vec![35e-6;request.conductors.len()],dielectric_loss_tangents:request.dielectrics.iter().map(|d|if d.relative_permittivity>1.{0.02}else{0.}).collect(),band_hz:[1e9,1e9]});
    let lossy=polygon_section::solve(&request).unwrap();
    let sample=&lossy["frequencyDependent"]["samples"][0];
    assert!(sample["resistanceOhmPerM"][0][0].as_f64().unwrap()>0.);
    assert!(sample["conductanceSPerM"][0][0].as_f64().unwrap()>0.);
    // A local per-length matrix is not a claim that this cross section persists
    // along the entire routed trace or includes its end-pad/bend transitions.
    if let Some(directory) = std::env::var_os("P7_TEST_EVIDENCE_DIR") {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(std::path::Path::new(&directory).join("bandamp-captured-section.json"),serde_json::to_vec(&serde_json::json!({"board":"BandAmpV5","trackId":"d334a42d8fb15d25","extracted":extracted,"result":result,"validity":"conditional section only; no whole-channel/local-transition or 5% accuracy admission"})).unwrap()).unwrap();
        std::fs::write(std::path::Path::new(&directory).join("bandamp-captured-section-hf.json"),serde_json::to_vec(&serde_json::json!({"result":lossy,"authoredMaterialHypotheses":{"bulkConductivitySM":58e6,"dielectricLossTangent":0.02,"frequencyHz":1e9},"validity":"conditional local per-length matrices only; no measured material, domain or whole-channel accuracy certificate"})).unwrap()).unwrap();
    }
}
