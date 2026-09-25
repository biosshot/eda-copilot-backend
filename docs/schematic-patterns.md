# Passive ladder

The refined catalog also recognizes passive ladders. A private junction with
exactly two original terminals can join a series resistor or capacitor chain;
an externally named or cross-block junction cannot. Parallel arms between the
same two nets may contain one component or a series chain. Consecutive arms
with shared taps and a grounded end become one inline ladder, so the divider
and its parallel capacitors keep aligned tap positions. Ordinary parallel
groups remain with `parallel-two-pin`.

Parallel groups on ground or supply rails permit a 180-degree refinement turn.
When neither rail is ground or supply, the refiner may also try 90 and 270
degrees. Every turn preserves component pin identity and net names.

# Resistor pull bank

`resistor-pull-bank` recognizes at least four two-pin resistors in one functional
block. Each resistor connects the same common net to a distinct branch net.
Values may differ. Capacitors and other component types, empty/NC nets, repeated
branch nets, and symbols without opposed axial pins are excluded. Normalized
symbol widths and heights must each fit within a factor of two.

Functional patterns (including dividers and filters) and parallel pairs claim
their components first. The bank is placed inline: it does not create a child
block or generate port symbols. Its common bus is routed within the macro;
branch terminals remain available for ordinary wiring. Normal cross-block and
long-link policies still apply.

When all branches connect to one face of a neighbouring component in the same
block, the initial orientation and resistor order follow that face and its pin
coordinates. Otherwise the order is a natural sort of signal names, with
designators as a tie-breaker. The passive group permits 90, 180 and 270 degree
turns in the existing bounded refinement pass. Resistor identities, pin numbers,
and nets are preserved. A connector serving multiple branches remains outside
the movable group.

Regression tests cover four pull-ups/pull-downs at a connector in all four
orientations, an excerpt of the portable-scope DDR3 terminators R47–R51 with
W631GG6MB pin numbers, exclusion of bypass capacitors, functional pattern
priority, and a dense multipart neighbour across a block boundary.
