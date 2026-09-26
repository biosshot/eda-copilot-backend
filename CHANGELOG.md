# Changelog

## 0.3.4 - 2026-09-26

- Run complete route-aware post-placement refinement in Rust with parallel candidate evaluation and one native call per search.
- Adapt refinement to component/pad complexity (3–16 passes) and enforce a cooperative 30-second native budget.
- Cap placement concurrency at half available CPUs, with a maximum of eight workers.
- Validate CPU-capped wrapper behavior on small CI hosts and test CPU-capped Rust search directly.
- Remove the retired TypeScript refiner from tracked sources and retain native correctness, timeout and package regression checks.

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
