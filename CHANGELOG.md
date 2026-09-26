# Changelog

## 0.3.5 - 2026-09-26

- Hide console windows on Windows when starting PCB layout and subtree placement process workers.
- Pin workerpool to 10.0.3 and extend its fork-option validation for Node's `windowsHide` option. Preserve process isolation and fall back to default fork options if the internal allowlist is unavailable.

## 0.3.4 - 2026-09-26

- Add a regression assertion for CPU-capped direct native refinement search, including hosts where the requested thread count exceeds the available concurrency budget.

## 0.3.3 - 2026-09-26

- Make refinement and route-cost regression tests respect host CPU caps instead of assuming a fixed worker/thread count.
- Keep serial/native comparison coverage valid on small CI machines.

## 0.3.2 - 2026-09-26

- Run complete route-aware post-placement refinement in Rust with parallel candidate evaluation and one native call per search.
- Adapt refinement to component/pad complexity (3–16 passes) and enforce a cooperative 30-second native budget.
- Cap placement concurrency at half available CPUs, with a maximum of eight workers.
- Remove the retired TypeScript refiner from tracked sources and retain native correctness, timeout and package regression checks.

## 0.3.1 - 2026-09-25

- Enforce schematic wire clearance for perpendicular approaches and endpoints as well as parallel segments, including rigid route moves during refinement.
- Preserve valid schematic crossings while rejecting foreign-net wire approaches that violate the configured gap; add endpoint-clearance regression coverage.

## 0.3.0 - 2026-09-25

### Component and schematic contracts

- Resolve devices from accessible EasyEDA public libraries and preserve library-qualified part references alongside legacy LCSC UUIDs.
- Support per-connection directional net-port styles and use bidirectional net ports as the default schematic marker.
- Preserve multipart section indices even when intermediate sections are unused, and reject connections to pins absent from the library symbol.
- Expose EasyEDA symbol data for host-provided component previews.

### Schematic placement and routing

- Add dense-pin padding and reserve space for client wire labels while keeping dense labeling independent of port style. Limit wire-label padding to symbols that need it, preserving ordinary two/three-pin component bounds.
- Straighten short power-rail steps and arrange resistor pull banks on short shared buses.
- Force required cross-page ports, preserve them across dense blocks and keep dense local/cross-page signal groups as named wires.
- Improve independent block placement and routing, softly align IC rows and page blocks, and refine layouts with deterministic order seeds.
- Prefer readable orientations for short connectors and improve port orientation on passive supply branches.
- Reroute excessive wire detours and local obstacles; place long-link ports near their pins to reduce unnecessary runs.
- Align passive ladder patterns by pin geometry and compact passive supply branches.
- Add a PortableScope schematic regression bank and live schematic examples for the ADC/clock layout cases.

## 0.2.0 - 2026-09-19

- Allow fixed placement for every component role and add legalized local block
  layouts while preserving seed topology during refinement.
- Add bounded micro-A* routability scoring and route-aware post-placement
  refinement for swaps, rotations and local placement alternatives.
- Compare route candidates on identical routing obligations, preserve endpoint
  sibling-pad obstacles and report route-aware placement evidence.
- Improve ordinary-net affinity, passive-block handling, edge-group validation
  and local ground proximity for compact board placement.
- Cache board-outline distances and exact native solves, prune redundant route
  probes and accelerate grid conflict and A* searches.
- Expand placement reports and the published placement DSL for the new routing
  and local-layout behavior.

## 0.1.0 - 2026-09-12

- Initial standalone backend release with component resolution, schematic
  extraction, PCB placement, workers and native binaries for supported hosts.
