use super::*;

// The archived solver caches the core-relative scarcity frame. Keep that cache
// on the GPU; legality against the changing partial placement is never cached.
pub(super) struct Frame {
    poses: Vec<Pose>,
    floats: Vec<f64>,
    ids: Vec<u32>,
    ranges: Vec<u32>,
    resident: Option<(Handle, Handle, Handle, Handle)>,
}

fn frame_data(
    engine: &Engine,
    placed: &[WorkingPrimitive],
    dummy: &WorkingPrimitive,
    frame: &Frame,
    context: &Context,
) -> (Vec<f64>, Vec<u32>) {
    let mut ff = vec![context.problem.clearance, 0.0, 0.0];
    let mut fi = vec![0; k::HEADER];
    fi[k::N] = (placed.len() + 1) as u32;
    fi[k::MOVING] = placed.len() as u32;
    fi[k::COUNT] = frame.poses.len() as u32;
    fi[k::COMPONENTS] = context.problem.components.len() as u32;
    fi[k::CP_F..k::CP_F + 6].copy_from_slice(&engine.offsets);
    fi[k::FIXED_I] = fi.len() as u32;
    for p in placed.iter().chain(std::iter::once(dummy)) {
        let p = engine.frame_primitive(p, context);
        ff.extend(prim_f(p));
        fi.extend(prim_i(p).map(|v| v as u32));
    }
    // This kernel uses NET_I for per-source candidate ranges, not score nets.
    fi[k::NET_I] = fi.len() as u32;
    fi.extend(&frame.ranges);
    (ff, fi)
}

pub(in crate::block_solver) fn scarcity(
    remaining: &[WorkingPrimitive],
    placed: &[WorkingPrimitive],
    context: &Context,
) -> Vec<f64> {
    let _span = context.detail.span("gpu_frontier");
    let cores: Vec<_> = placed
        .iter()
        .filter(|p| {
            p.components
                .iter()
                .any(|(_, c)| c.role.as_deref() == Some("main_ic"))
        })
        .cloned()
        .collect();
    if cores.is_empty() || remaining.is_empty() {
        return vec![0.0; context.problem.primitives.len()];
    }
    let key: Vec<_> = cores.iter().map(primitive_pose_key).collect();
    let mut borrow = context.gpu_engine.borrow_mut();
    let engine = borrow.as_mut().unwrap();
    if !engine.frontiers.contains_key(&key) {
        let mut poses = Vec::new();
        let mut ranges = Vec::new();
        for p in &context.gpu_sources {
            let start = poses.len();
            if !cores.iter().any(|c| c.source_index == p.source_index) {
                poses.extend(candidates::scarcity(p, &cores, context));
            }
            ranges.extend([start as u32, poses.len() as u32]);
        }
        let floats: Vec<_> = poses.iter().flat_map(|p| [p.dx, p.dy]).collect();
        let ids: Vec<_> = poses
            .iter()
            .enumerate()
            .flat_map(|(i, p)| [p.template_index, i as u32])
            .collect();
        engine.frontiers.insert(
            key.clone(),
            Frame {
                poses,
                floats,
                ids,
                ranges,
                resident: None,
            },
        );
    }
    let frame = &engine.frontiers[&key];
    if frame.poses.is_empty() {
        return vec![0.0; context.problem.primitives.len()];
    }
    let (ff, fi) = frame_data(engine, placed, &remaining[0], frame, context);
    let core_data = frame
        .resident
        .is_none()
        .then(|| frame_data(engine, &cores, &remaining[0], frame, context));
    if !finite(&ff) || !finite(&frame.floats) || !finite(&engine.sf) {
        fail("unsupported numeric range in GPU frontier".into());
    }
    let count = frame.poses.len();
    let sources = context.problem.primitives.len();
    let counts = gpu_runtime::with_session(|session| {
        if engine.handles.is_none() {
            engine.handles = Some((
                session.client.create_from_slice(f64::as_bytes(&engine.sf)),
                session.client.create_from_slice(i32::as_bytes(&engine.si)),
            ));
        }
        let (sf, si) = engine.handles.as_ref().unwrap();
        let frame = engine.frontiers.get_mut(&key).unwrap();
        if frame.resident.is_none() {
            frame.resident = Some((
                session
                    .client
                    .create_from_slice(f64::as_bytes(&frame.floats)),
                session.client.create_from_slice(u32::as_bytes(&frame.ids)),
                session.client.empty(count * 4),
                session.client.empty(sources * 4),
            ));
        }
        let (pf, pi, nearby, pins) = frame.resident.as_ref().unwrap();
        let scores = session.workspace(16, count * 8);
        let tags = session.workspace(17, count * 2 * 4);
        let legal = session.workspace(18, count * 4);
        let output = session.workspace(19, sources * 3 * 4);
        let client = &session.client;
        let input = |ff: &Vec<f64>, fi: &Vec<u32>| unsafe {
            let fh = client.create_from_slice(f64::as_bytes(ff));
            let ih = client.create_from_slice(u32::as_bytes(fi));
            k::InputLaunch::new(
                ArrayArg::from_raw_parts(sf.clone(), engine.sf.len()),
                ArrayArg::from_raw_parts(si.clone(), engine.si.len()),
                ArrayArg::from_raw_parts(fh, ff.len()),
                ArrayArg::from_raw_parts(ih, fi.len()),
                ArrayArg::from_raw_parts(pf.clone(), frame.floats.len()),
                ArrayArg::from_raw_parts(pi.clone(), frame.ids.len()),
            )
        };
        let groups = CubeCount::Static(count.div_ceil(128) as u32, 1, 1);
        let dim = CubeDim::new_1d(128);
        unsafe {
            macro_rules! a {
                ($h:expr,$n:expr) => {
                    ArrayArg::from_raw_parts($h.clone(), $n)
                };
            }
            if let Some((cf, ci)) = &core_data {
                k::scarcity_probe::launch_unchecked::<WgpuRuntime>(
                    client,
                    groups.clone(),
                    dim,
                    input(cf, ci),
                    a!(scores, count),
                    a!(tags, count * 2),
                );
                k::scarcity_nearby::launch_unchecked::<WgpuRuntime>(
                    client,
                    CubeCount::Static(1, 1, 1),
                    CubeDim::new_1d(32),
                    input(cf, ci),
                    a!(scores, count),
                    a!(tags, count * 2),
                    a!(nearby, count),
                    a!(pins, sources),
                );
            }
            k::scarcity_legal::launch_unchecked::<WgpuRuntime>(
                client,
                groups,
                dim,
                input(&ff, &fi),
                a!(nearby, count),
                a!(legal, count),
            );
            k::scarcity_counts::launch_unchecked::<WgpuRuntime>(
                client,
                CubeCount::Static(1, 1, 1),
                CubeDim::new_1d(32),
                input(&ff, &fi),
                a!(nearby, count),
                a!(pins, sources),
                a!(legal, count),
                a!(output, sources * 3),
            );
        }
        let bytes = client
            .read_one(output)
            .map_err(|e| format!("GPU frontier readback: {e:?}"))?;
        Ok(u32::from_bytes(&bytes)[..sources * 3].to_vec())
    })
    .unwrap_or_else(|reason| fail(reason));
    engine.frontier_batches += 1;
    engine.frontier_candidates += count;
    let mut result = vec![0.0; sources];
    for p in remaining {
        let index = p.source_index * 3;
        let pins = counts[index] as usize;
        let nearby = counts[index + 1] as usize;
        let legal = counts[index + 2] as usize;
        if pins > p.primitive.connection_points.len() || legal > nearby || nearby > count {
            fail("invalid GPU frontier counts".into());
        }
        // Only one scalar per remaining primitive is formed on the controller.
        // The candidate distance and collision passes above are entirely GPU.
        result[p.source_index] = if pins == 0 {
            0.0
        } else {
            (if legal > 0 {
                60.0 * (1.0 - legal as f64 / nearby.max(1) as f64)
            } else {
                0.0
            }) + 40.0 * pins.saturating_sub(1).min(2) as f64
        };
    }
    drop(borrow);
    if std::env::var_os("PCB_BLOCK_GPU_VERIFY").is_some()
        || std::env::var_os("PCB_BLOCK_GPU_VERIFY_FRONTIER").is_some()
    {
        for p in remaining {
            assert_eq!(
                result[p.source_index],
                super::super::frontier_scarcity(p, placed, context),
                "GPU frontier {}",
                p.primitive.id
            );
        }
    }
    result
}
