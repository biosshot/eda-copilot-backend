# P6b frequency multiports — implemented subset

This block evaluates explicit supplied frequency models. It does not extract PCB
geometry or provide temporal models, PI/SI certification, causality or RF stability.
CPU/F64 stays inside `pcb-analyze::electrical::frequency`; N-API is a method of
the existing `AnalysisElectricalSession`, in the single production addon. Existing
MNA RLC and FEM DC algorithms are independent and unchanged.

## Resource and DSL

`a.model(NetworkResource)` describes immutable behavior; it neither registers a
device nor parses a file nor solves. Existing LocalModel overloads and guards stay.
`a.network({model,ports})` creates an instance with an exact, bijective mapping of
all resource ports. Factory, direct Resource and catalog share `networkResource()`
structural checks and early size guards. A model may have many distinct instances;
reusing one instance handle in overlapping nested graphs is rejected.

The task's section 7 is an executable factory example. Catalog equivalents use
`models.electrical.attenuatorPad` (Attenuator Pad), an ordinary network resource.
No special attenuator type or coefficient generator is added. Its coefficients
can be computed with ordinary JavaScript, e.g. `10 ** (-attenuationDb / 20)`.

Input shape is exported in `analysis-spec-doc.d.ts`: `FrequencyNetworkData`,
`TouchstoneNetworkData`, `NetworkResource`. Required ordered `Resource.ports`
sets source matrix indices, never alphabetical sorting. Normalized JSON uses
`parameters[frequency][out][in]`, S/Z/Y, Hz, common topology, explicit nature.
Scalar Resource.parameters are unsupported for these blocks; behavior lives in data.
S requires positive finite real scalar/per-port `referenceOhm`; Z/Y use physical
Ohm/Siemens and forbid that field. NaN/Infinity, duplicates, missing coefficients,
partial mappings, unsupported topology/nature or incompatible validity/analyses
are errors. Other future resource kinds retain their old description vocabulary;
the production frequency executor explicitly rejects unsupported models.

Host supplies bytes in `models`; local launchers capture files and hashes.
`source` never loads files/URLs. There is no I/O or second Touchstone parser in DSL.

## Touchstone subset

Parser follows [IBIS Touchstone 2.0 (2009)](https://www.ibis.org/touchstone_ver2.0/touchstone_ver2_0.pdf),
pp. 6–18 and 26. Accepted: S, 1.0/2.0, RI/MA/DB, Hz/kHz/MHz/GHz,
ASCII comments/whitespace, option defaults GHz/S/MA/R50, Full matrices. Two-port
1.0 order is 11/21/12/22; 2.0 requires explicit 21_12 or 12_21. Three or more
ports are row-major. DB uses 20log10 magnitude and degree angles. Positive real
2.0 `[Reference]` overrides option R and may wrap across lines.

2.0 requires version before option, port count as first subsequent keyword,
frequency count, two-port order where applicable, Network Data and End. Counts
must match. Information blocks are explicitly ignored metadata. Other physical
keywords, noise, mixed-mode, triangular data, versions 1.1/2.1, and file Z/Y/H/G
are rejected. Extra option lines follow the standard's ignore rule. For original
1.0 one/two-port data a complete record occupies one line; larger matrices and
2.0 data wrap. Errors preserve resource/instance, frequency and line where known.
Normalized reports retain the options, source order and references.

## Equations and numerical guards

Power waves use `V=D(a+b)`, `I=D^-1(a-b)`, `D=diag(sqrt(R))`, with incoming
currents. Each source matrix is interpolated in its original representation and
fixed source references; only then converted into waves at the common internal
50 Ohm mathematical normalization. This normalization adds no physical resistor
or connection between otherwise unrelated returns.

Renormalization solves `a_new=M a_old`, `b_new=N a_old`, thus `S_new=N/M` by
transposed linear solves. Z/Y blocks use their implicit V/I relations. Composition
stamps a block-diagonal S and imposes `a_i=b_j`, `a_j=b_i` for every pair, plus
independent external incident-wave basis columns. External b is recovered from
that solve. Each port is exactly external or paired; no implicit termination,
fan-out or duplicate alias. Explicit one-port loads work. All blocks must be
in one connected return group by the declared graph, with an external port.
Disconnected externally exposed groups are explicitly unsupported: normalizing
both to 50 Ohm does not join their physical returns. Nested topology is flattened
without cloning devices; common returns follow connections, not reference values.

Output S is separately renormalized. Z is `50*(I+S)/(I-S)` and Y is
`(I-S)/(I+S)/50`, all via linear solves. An ideal thru/open/short stays usable
as S even when a requested finite Z/Y does not exist. There is no gmin, clamping,
passivity repair or successful zero replacement.

Small block matrices use vectors; faer sparse LU solves their bounded real-block
systems, following P6a's complex transport and solver family. Two refinement steps;
normalized row residual <=1e-8. Infinity-norm condition number is measured using
unit-vector LU solves, accumulating row sums without materializing an inverse;
nonfinite/rank failure or condition >1e12 rejects the frequency/query. Nonfinite
recovered coefficients reject explicitly before JSON serialization.

Passive sources are tested at each evaluated point: positive definiteness of
`(1+1e-9)^2 I - S†S` via real-block Cholesky tests all incident combinations.
Individual coefficient magnitudes are insufficient. Active permits gain and
nonreciprocity. Unspecified remains unspecified. Passivity at evaluated points
proves neither causality nor broadband stability. Entire requests reject on any
failed frequency; errors are not partial zero matrices.

## Band, interpolation and results

Frequencies are strictly increasing/nonnegative. Explicit source DC=0 is retained;
the existing positive-frequency builder remains unchanged. Single-point sources
support only that point. No DC/high-frequency extrapolation. Multiple source
bands intersect. Complex linear real/imag interpolation occurs BEFORE source
conversion/composition and is recorded per resource/instance, frequency, interval,
representation and reference. Wrapped phase and dB are never interpolated.

Any interpolated or conditional input makes result DataValues conditional with
provenance. S unit is `1`, Z `Ohm`, Y `S`; existing real/imaginary/magnitude,
phaseDeg, exact at(), checks/plots/sweeps apply. Zero phase remains unavailable.
`result.network` is a new immutable normalized resource instance with only the
computed frequency grid, validity and provenance; it can be connected/recomputed,
but has no time representation. Reused resource provenance carries compact
source ids/hashes/mapping/reference/band links, without recursively copying ancestor
model tables; immutable earlier reports retain those source results.
Current descriptors/results are report-budgeted;
there is no global cache or stale fallback. Unsupported SI/transient rejects.

## Hard admission limits

| Quantity | Limit |
| --- | --- |
| Touchstone bytes per resource | 8 MiB, checked before tokenization |
| Ports per resource / external result | 16 |
| Frequencies per resource/request | 100000, with tighter product/work bounds |
| Source complex coefficients per resource / aggregate | 262144 / 524288 |
| Instances / total instance ports / nesting depth | 32 / 128 / 32 |
| Output complex coefficients | 131072 |
| Frequency count × total ports cubed | 1e8 |
| Default native time / planned memory | 60 s / 256 MiB |

TS validates products before frequency Array.from; Rust repeats admission. Text
headers cannot force allocations beyond table caps; parsing buffers one record.
Native memory plan includes source payload estimates, outputs, interpolation
records and bounded LU workspaces. It is a conservative allocation admission
estimate, not an OS RSS hard limit; existing parent process timeout/cancel and
scenario report byte/entry caps still apply. Peak RSS records OS process high-water
through the native call, including Node and previous solves.

## Reproduction

Direct synthetic examples: `tests/fixtures/analysis-frequency/network.js` and
versioned resources / original `.s2p`. Named board launches:

```sh
npm run test:pcb-analyze -- BandAmpV5 STM32G0_CAN_RS485 --name network
```

Each page separates standalone GHz coupons from a passive low-frequency impedance
hypothesis for fixture-verified input/output/return contacts. The latter is an
explicit test hypothesis, not component identification, measurement or extraction.
Launchers retain exact JS/resource bytes, resolved normalized inputs, source/native
hashes, JSON/audit, wall/RSS and self-contained HTML. Versioned audit checks complex
closed-form oracles and plot arrays; mutation tests exercise corrupted evidence.
