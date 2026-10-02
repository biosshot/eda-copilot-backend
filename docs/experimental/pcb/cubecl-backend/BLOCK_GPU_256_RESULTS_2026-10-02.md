# Extended block GPU scorer — 2026-10-02

## Scope

Extend the existing F32 CubeCL block solver to up to 256 components and
256 primitives, including composite primitives. Keep the original CPU search
stages, budgets, candidate semantics and full-call CPU recovery. This is not a
router, board packer or refiner change.

Component bodies, through-hole boxes, primitive collision boxes, path ports,
pads and connectivity use resident shared arrays with per-primitive ranges.
Candidate descriptors remain compact rotation/translation poses. Complete
candidate scoring and shortlist reduction run on GPU; the host prepares frame
connectivity and merges only the shortlists of bounded candidate chunks.

The scorer now handles composite bodies, through holes, local access, stable
net weights, long nets, power yield, ordinary-net affinity, path/facing scores,
anchors, bounds, obstacles, target dimensions and world constraints. Frontier
dispatch covers more than 32 primitives. Beam, singles and applicable pair
stages are preserved; the existing CPU algorithm's own stage/score cutoffs are
not relaxed or removed.

## Memory and fallback

- Maximum 256 components and 256 primitives; candidate chunks at most 4096.
- Resident data and frame allocation estimates are checked against the smaller
  of the runtime allocation limit and a conservative 64 MiB job budget.
  This is an allocation budget, **not a query of currently free VRAM**.
- Net, segment and moving-pad capacities are set from the full problem, avoiding
  shader recompilation at each partial assembly size. Shared-memory requirements
  are checked against the actual device limit.
- Frontier frame cache is bounded by count and estimated bytes.
- Unsupported devices, missing GPU, memory limits, unsupported inputs and runtime
  failures use CPU F32. A failure discards partial GPU checkpoints and repeats
  the original call. No GPU retry loop.
- Remaining conservative guards: geometric routing metric, quarter-turn allowed
  rotations, nonempty component-bearing primitives, safe pad ownership/cache
  metadata, supported pad layers and finite transported data. Large nets/workspaces
  may exceed the device limits even below 256 components. Thus 256 is a supported
  input ceiling, not a promise that every possible 256-component input fits.

## Validation and measurement protocol

One CPU reference and one complete GPU pass for the saved DDR input/version;
no repeated medians or complete PortableScope board run. Both grouped and
released hypotheses are evaluated with two native workers, followed by pair
continuation from their own initial results. Timings include beam, singles,
applicable pairs and GPU initialization/compilation. They exclude surrounding
TypeScript portfolio/refiner/board/router work. Short diagnostic fixtures were
rerun only after implementation changes or a failed check.

Reference revision: `592afd3`, copied baseline addon SHA256
`aa2204f763790eb2745955fa40dda3ef5be42eb70a0fdf61b393acf8547da634`.
Input: PortableScope capture `2026-10-01T20-22-51-860Z`, process
`process-31908-thread-0`, entries `00196` and `00197` (DDR, 43 components;
12 grouped / 43 released primitives). Exact paths and addon hashes are saved
in the local result JSONs.

CPU wall time: initial **607.357 s**, pair continuation **133.425 s**, total
**740.782 s**. Final scores: grouped **9336.23828125**, released
**4201.1455078125**; both hard count zero. This is the corrected geometry baseline,
not the older failed board run's partial DDR timing. Some development compilation
overlapped the CPU run, so this single-pass comparison is not an isolated hardware
microbenchmark.

## Full DDR result

NVIDIA GeForce RTX 3060 Laptop GPU; both hypotheses used CubeCL, with no fallback.

| Complete batch stage | CPU | GPU | Speedup |
| --- | ---: | ---: | ---: |
| Initial assembly: beam + singles, both hypotheses | 607.357 s | 130.380 s | 4.66× |
| Pair continuation, both hypotheses | 133.425 s | 38.247 s | 3.49× |
| **Total** | **740.782 s** | **168.627 s** | **4.39×** |

Stage profiles below are per-job wall times, including contention. Jobs overlap;
do not add these rows to calculate the batch wall time.

| Hypothesis / stage | CPU | GPU |
| --- | ---: | ---: |
| Grouped beam | 130.004 s | 82.616 s |
| Grouped singles | 87.946 s | 6.434 s |
| Grouped pairs | 133.410 s | 38.233 s |
| Released beam | 190.595 s | 106.853 s |
| Released singles | 416.721 s | 23.134 s |

The released 43-primitive case does not execute expensive pairs under the
existing CPU algorithm's cutoff; that rule is unchanged on GPU. This is not
a newly skipped optimization stage. Grouped CPU/GPU pair candidate counts
match exactly: **3,241,979**.

Final grouped score matches CPU: **9336.23828125**. Released GPU score is
**4230.302734375**, versus CPU **4201.1455078125** (+0.694%). Both have zero hard
violations. Independent geometry checks pass all 785 / 903 inter-primitive
component pairs; GPU minimum clearance margins are +0.000300 / +0.000700 mm.
The margin checks passed without consuming the checker's 0.002 mm tolerance.
The released layout is different and slightly worse by the native score;
identical quality/poses are not claimed. These results do not establish
whole-board quality or whole-board speed.

Resident static buffers: 68,048 / 80,448 bytes. Each job uses a 7,168-byte
shared-memory estimate against the device's 49,152-byte limit, 4096-candidate
chunks, 32-endpoint net capacity, 128-segment and 128-moving-pad capacities.
The shared runtime reports 648,456 bytes of scratch workspaces. Those numbers
exclude driver/pipeline memory and are not total process VRAM measurements.

Remaining costs: GPU completion waits dominate (`gpu_readback` includes actual
kernel execution and synchronization, not just copying bytes). The shared
runtime's cumulative mutex wait reaches 79.654 s across the two workers;
this overlaps other work and must not be added to wall time. Cold shader
compilation is included in beam times, but was not separately timed. Grouped
pairs take 38.233 s, including 33.140 s waiting for GPU batches and about
3.377 s in the existing shortlist route evaluation. No 30-second full-cycle
claim is made. Future work should profile expensive shader passes individually,
then evaluate concurrent job workspaces/batch scheduling and reuse of compiled
pipelines in a long-lived process, preserving complete search and constraints.

Production addon SHA256:
`ec17879346e42c7ffbadfb2e4152dc97b1d73b14cbbd291e192da2064737fa0f`.
One `.node`, **21,089,792 bytes (20.11 MiB)**, slightly above the preferred
20 MiB target; no new binary dependency.

## Checks

- Rust default features: 81 passed, 4 ignored; CPU-only build: 76 passed, 2 ignored.
- TypeScript typecheck and 18 focused native/F32/stage regressions passed.
- Four new full-cycle GPU/recovery tests passed: composite + through holes +
  path/facing with chunks of 32; missing GPU; injected failure after singles;
  bounds/world/locked placement. The last was repeated after adding anchor
  coverage and the final shader changes and passed independently.
- Reduced DDR diagnostic (3 primitives, 10 components) completed full CPU/GPU
  cycles with the same final score **1734.858642578125**, hard count zero.
  Strict per-candidate score diagnostics observed a small F32 discrepancy
  (CPU 1843.7614, GPU 1843.76); exact score parity is not claimed or made a
  migration acceptance requirement. Physical constraints and final quality
  remain required.
- Independent DDR body transforms/clearance validation uses authored body
  offsets and final component poses. CPU reference passed all 785 grouped and
  903 released inter-primitive component pairs; minimum clearance margins
  +0.000300 mm and +0.001299 mm respectively.

Artifacts: ignored `debugging/gpu-blocks-256/`: `bench-ddr.cjs`,
`cpu-result.json`, `cubecl-result.json`, CPU/GPU logs, `check-ddr.cjs`,
`geometry-check.json`, focused test/build logs. No production F64 backend or
extra runtime DLL was added.
