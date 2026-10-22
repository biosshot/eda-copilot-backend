use super::*;

// Fault injection isolates mapping/readiness from triangulator quality choices.
#[test]
fn required_contact_mapping_and_refinement_failures_block_only_dependents() {
    let input: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/conduction-contact-coupon.json"
    ))
    .unwrap();
    let board = crate::import_json(&input["board"].to_string())
        .board
        .unwrap();
    let mut options: ConductionOptions = serde_json::from_value(input["options"].clone()).unwrap();
    options.mesh.max_nodes = 50_000;
    let g = geometry::prepare(&board, options.geometry);
    for failure in [
        "boundary_mapping_missing",
        "refinement_incomplete",
        "domain_mapping_missing",
    ] {
        let mut r = prepare_geometry(&board, &g, options.clone());
        assert!(r.barrels[0].ready, "{:?}", r.barrels[0]);
        assert_eq!(r.barrels[0].expected_contacts.len(), 4);
        let index = r
            .domains
            .iter()
            .position(|d| d.layer == "TOP" && d.source_paths.contains(&"pads[0]".into()))
            .unwrap();
        let region_id = r.domains[index].region_ids[0].clone();
        match failure {
            "boundary_mapping_missing" => r.domains[index].boundaries.clear(),
            "refinement_incomplete" => {
                assert!(!r.domains[index].triangles.is_empty());
                r.domains[index].readiness.mesh_complete = false;
                r.domains[index].readiness.dc_ready = false;
                r.domains[index].readiness.reasons.push(failure.into());
            }
            _ => {
                r.domains.remove(index);
            }
        }
        r.barrels.clear();
        build_barrels(&board, &g, &mut r);
        let b = &r.barrels[0];
        assert!(!b.ready, "{failure}");
        let missing = b
            .expected_contacts
            .iter()
            .find(|c| c.region_id == region_id)
            .unwrap();
        assert!(!missing.ready);
        assert!(
            missing.reasons.iter().any(|s| s.starts_with(failure)),
            "{missing:?}"
        );
        assert_eq!(
            b.contacts.len(),
            if failure == "refinement_incomplete" {
                4
            } else {
                3
            }
        );
        assert!(r
            .domains
            .iter()
            .filter(|d| d.source_paths.contains(&"pads[0]".into()))
            .all(|d| !d.readiness.dc_ready));
        assert!(
            r.domains
                .iter()
                .find(|d| d.source_paths.contains(&"pads[1]".into()))
                .unwrap()
                .readiness
                .dc_ready
        );
    }
}
