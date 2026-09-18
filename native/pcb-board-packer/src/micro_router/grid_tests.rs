//! Exact parity with the pre-optimization search and geometric predicates.
use super::*;
use super::tests::{bounds, primitive};
use super::grid::{GridShape, OutlineMask};

fn rectangle() -> Vec<Point> {
    vec![Point { x: -10.0, y: -10.0 }, Point { x: 10.0, y: -10.0 },
        Point { x: 10.0, y: 10.0 }, Point { x: -10.0, y: 10.0 }]
}

#[test]
fn grid_shape_is_checked_and_includes_non_aligned_last_cell() {
    let bounds = Box2 { left: -2.3, right: 1.0, top: 4.7, bottom: 8.0 };
    let shape = GridShape::new(bounds,1.0,25).unwrap();
    assert_eq!((shape.width,shape.height,shape.len),(5,5,25));
    assert_eq!(shape.index(4,4),Some(24));
    assert_eq!(shape.index(-1,0),None);
    assert_eq!(shape.index(0,-1),None);
    assert_eq!(shape.index(5,0),None);
    assert_eq!(shape.index(0,5),None);
    assert!(GridShape::new(bounds,1.0,24).is_none());
    for grid in [0.0,-1.0,f64::NAN,f64::INFINITY,1e-30] {
        assert!(GridShape::new(bounds,grid,1_000_000).is_none());
    }
    assert!(GridShape::new(Box2 { right: -9.0, ..bounds },1.0,100).is_none());
    assert!(GridShape::new(Box2 { right: f64::INFINITY, ..bounds },1.0,100).is_none());
}

#[test]
fn contour_mask_matches_exact_tests_on_concave_degenerate_and_empty_outlines() {
    let mut concave = rectangle();
    concave[3] = Point { x: 0.0, y: 10.0 };
    concave.extend([Point { x: 0.0, y: 2.0 }, Point { x: -10.0, y: 2.0 }]);
    let mut reversed = concave.clone(); reversed.reverse();
    for polygon in [rectangle(), concave, reversed,
        vec![Point {x: 0.0,y: -8.0},Point {x: 8.0,y: 0.0},Point {x: 0.0,y: 8.0},Point {x: -8.0,y: 0.0}],
        vec![], vec![Point {x: 1.0,y: 1.0}], vec![Point {x: 1.0,y: 1.0}; 4]] {
        let config = MicroRouteConfig::board();
        let mask = OutlineMask::new(&polygon,bounds(),&config,1000);
        let fallback = OutlineMask::with_cell_limit(&polygon,bounds(),&config,8,0);
        assert!(!fallback.is_dense());
        for x in -1..=81 {
            for y in -1..=81 {
                let cell = Cell { x,y,layer: 0 };
                let point = cell_to_point(cell,bounds(),config.grid);
                let expected = !polygon.is_empty() && (!point_in_polygon(&point,&polygon)
                    || point_to_polygon_distance(&point,&polygon) + EPS < config.clearance + config.trace_width/2.0);
                assert_eq!(mask.blocked(cell),expected);
                assert_eq!(mask.blocked(Cell { layer: 1, ..cell }),expected);
                assert_eq!(fallback.blocked(cell),expected);
            }
        }
        if !polygon.is_empty() {
            let (hits,misses,entries) = mask.stats();
            assert_eq!(misses,81*81);
            assert_eq!(entries,81*81);
            assert_eq!(hits,81*81);
        }
    }
}

#[test]
fn contour_mask_keeps_eps_and_endpoint_exceptions_outside_the_cache() {
    let config = MicroRouteConfig::board();
    let edge = config.clearance + config.trace_width/2.0;
    let cell = Cell {x: 4,y: 20,layer: 0};
    let point = cell_to_point(cell,bounds(),config.grid);
    let far = Cell { x: 40, y: 40, layer: 0 };
    let s = Arc::from("S"); let t = Arc::from("T"); let net = Arc::from("SIG");
    let sr = Arc::from("S.1"); let tr = Arc::from("T.1");
    for delta in [-2.0*EPS, 0.0, 2.0*EPS] {
        let left = point.x - edge + delta;
        let polygon = [Point {x:left,y:-10.0},Point {x:10.0,y:-10.0},Point {x:10.0,y:10.0},Point {x:left,y:10.0}];
        let cache = BoardRouteCache::new(&polygon,bounds(),&config,1000);
        let mut temp = TemporaryRoutes::new(bounds(),config.grid,2);
        let check = |at,start,temp: &TemporaryRoutes,cached| blocked(at,at,start,far,&s,&t,&sr,&tr,&net,
            bounds(),&polygon,&[],temp,&config,if cached {Some(&cache)} else {None});
        for start in [cell,far] {
            for at in [cell,Cell {layer:1,..cell},Cell {x:-1,..cell}] {
                assert_eq!(check(at,start,&temp,false),check(at,start,&temp,true));
            }
        }
        temp.reserve(&[cell], &Arc::from("OTHER"));
        assert!(check(cell,cell,&temp,true));
        assert_eq!(check(cell,cell,&temp,false),check(cell,cell,&temp,true));
    }
}

#[test]
fn workspace_rejects_changed_grid_origin_clearance_or_layers() {
    let a = primitive("A",-4.0,0.0,"SIG"); let b = primitive("B",4.0,0.0,"SIG");
    let config = MicroRouteConfig::board();
    let cache = BoardRouteCache::new(&rectangle(),bounds(),&config,1000);
    let mut changed = config.clone(); changed.grid = 0.5;
    let mut clearance = config.clone(); clearance.clearance += 0.1;
    let mut layers = config.clone(); layers.layers.pop(); layers.via_transitions.clear();
    let shifted = Box2 {left:-9.5,..bounds()};
    for (domain,other) in [(bounds(),changed),(bounds(),clearance),(bounds(),layers),(shifted,config)] {
        assert!(!cache.matches(domain,&other));
        assert_eq!(candidate_penalty(&a,&[&b],&[],domain,cache.polygon(),&[],&other).to_bits(),
            candidate_penalty_cached(&a,&[&b],&[],domain,&cache,&[],&other).to_bits());
    }
}

#[test]
fn early_best_and_dense_copper_preserve_reference_routes_and_expansion_counts() {
    let a = primitive("A",-4.0,0.0,"SIG"); let b = primitive("B",4.0,0.0,"SIG");
    let config = MicroRouteConfig::board();
    let cache = BoardRouteCache::new(&rectangle(),bounds(),&config,1000);
    for wall in [false,true] {
        let obstacles = collect_obstacles(&[&a,&b],
            if wall { &[Box2 {left:-1.0,right:1.0,top:-2.0,bottom:2.0}] } else { &[] }, &[], &config);
        let mut sparse = TemporaryRoutes::default();
        let mut dense = TemporaryRoutes::new(bounds(),config.grid,2);
        let path = [Cell {x:35,y:25,layer:0},Cell {x:35,y:26,layer:0},Cell {x:35,y:26,layer:1}];
        sparse.reserve(&path,&Arc::from("OTHER")); dense.reserve(&path,&Arc::from("OTHER"));
        let job = schedule_jobs(&a,&[&b],&[],&config).remove(0);
        for budget in [0,1,8,1500] {
            for via in [config.via_cost,config.grid*4.0] {
                let expected = reference_search(&job,bounds(),cache.polygon(),&obstacles,&sparse,&config,via,budget,None);
                let actual = route_job_search(&job,bounds(),cache.polygon(),&obstacles,&dense,&config,via,budget,Some(&cache));
                assert_eq!(format!("{actual:?}"),format!("{expected:?}"));
            }
        }
    }
}

// Frozen pre-optimization loop: deliberately checks blocked() before best.
// The reference uses uncached contour tests and sparse temporary storage.
#[allow(clippy::too_many_arguments)]
fn reference_search(
    job: &RouteJob,
    bounds: Box2,
    board_outline: &[Point],
    obstacles: &[StaticObstacle],
    temporary: &TemporaryRoutes,
    config: &MicroRouteConfig,
    search_via_cost: f64,
    max_expanded: usize,
    route_cache: Option<&BoardRouteCache>,
) -> RouteOutcome {
    let start_cell = point_to_cell(job.source.point, job.source.layer, bounds, config.grid);
    let goal_cell = point_to_cell(job.target.point, job.target.layer, bounds, config.grid);
    // A forbidden landing cannot be reached even by going to another layer.
    for cell in [start_cell, goal_cell] {
        if blocked(cell, cell, start_cell, goal_cell, &job.source.primitive_id, &job.target.primitive_id,
            &job.source.reference, &job.target.reference, &job.net, bounds, board_outline,
            obstacles, temporary, config, route_cache) {
            return RouteOutcome::NoPath { expanded: 0 };
        }
    }
    let start = State { cell: start_cell, direction: 4 };
    let mut open = BinaryHeap::new();
    let mut best: FxHashMap<State, PathCost> = FxHashMap::default();
    let mut previous: FxHashMap<State, State> = FxHashMap::default();
    let mut serial = 0usize;
    best.insert(start, PathCost::zero());
    open.push(OpenNode {
        state: start,
        g: PathCost::zero(),
        estimate: heuristic(start_cell, goal_cell, config, search_via_cost),
        serial,
    });
    let mut expanded = 0usize;

    while let Some(node) = open.pop() {
        let Some(known) = best.get(&node.state).copied() else { continue; };
        if compare_cost(node.g, known) == Ordering::Greater { continue; }
        if expanded >= max_expanded { return RouteOutcome::BudgetExhausted { expanded }; }
        expanded += 1;
        if node.state.cell.x == goal_cell.x && node.state.cell.y == goal_cell.y && node.state.cell.layer == goal_cell.layer {
            let cells = reconstruct(node.state, start, &previous);
            return RouteOutcome::Found(RouteResult { cost: node.g, cells, expanded, used_fallback: false });
        }

        for (next, step_cost) in neighbors(node.state, config, search_via_cost) {
            if blocked(
                next.cell,
                node.state.cell,
                start_cell,
                goal_cell,
                &job.source.primitive_id,
                &job.target.primitive_id,
                &job.source.reference,
                &job.target.reference,
                &job.net,
                bounds,
                board_outline,
                obstacles,
                temporary,
                config,
                route_cache,
            ) { continue; }
            let next_cost = add_cost(node.g, step_cost);
            if best.get(&next).is_some_and(|old| compare_cost(next_cost, *old) != Ordering::Less) {
                continue;
            }
            best.insert(next, next_cost);
            previous.insert(next, node.state);
            serial += 1;
            open.push(OpenNode {
                state: next,
                g: next_cost,
                estimate: next_cost.physical + heuristic(next.cell, goal_cell, config, search_via_cost),
                serial,
            });
        }
    }
    RouteOutcome::NoPath { expanded }
}
