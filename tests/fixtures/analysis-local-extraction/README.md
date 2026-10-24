# Frozen local-extraction primitive references

FastHenry2 by FastFieldSolvers S.r.l., derived from MIT FastHenry:
<https://www.fastfieldsolvers.com>, selected upstream source
<https://github.com/ediloren/FastHenry2/tree/10959091782153754071c4e98645c82c11d5c9ac>.
Source/binary/input/output hashes and console-host compatibility changes are in
`manifest.json`. The executable and source archives are not shipped or built by CI.

The authored input uses millimetres and conductivity 58,000 S/mm. `n1` means one
uniform current filament per bar: it is the volume integral reference for a
uniform basis, not a skin/proximity resolved conductor. Two independently driven
physical bars retain self and signed mutual coefficients; the second bar is the
explicit return. No ideal return geometry is inferred. The scalar loop is
`L11 + L22 - L12 - L21`. Vendor text output has six significant figures.

The separate JSON references use independent box difference-convolution/Duffy
self integration and smooth six-dimensional product-Gauss mutual integration.
Their refinement histories are retained. They do not import production or vendor
algorithms. The triangular-prism production rule uses a signed apex fan and exact
source-z/radial integration; the alternative exact polyhedral point potential uses
edge logarithms/solid angles with outer volume quadrature. Both keep finite
thickness, and these box references exercise a different method.

`close-return-n1` retains the 10 mm long bars with 0.5 mm width, 35 um thickness
and an explicit 1 um edge gap (centre spacing 0.501 mm). This is a near-pair
reference with the same uniform basis and text precision as the other n1 cases.

These small references validate finite-volume primitives. They do not establish
board mesh, electrostatic C, via transitions, reduction/recovery, the full band,
or the remaining P7 acceptance gates.

`fastcap/` freezes independent finite-plate full Maxwell C and sphere convergence
from FastCap2 by FastFieldSolvers S.r.l., derived from MIT FastCap:
<https://github.com/ediloren/FastCap2/tree/04eee66446be0629f0607e68f369acad1072dc4d>.
The reporting-only export retains raw unsymmetrized coefficients separately from
the vendor's usual symmetrized output. `generate-inputs.py` reproduces each exact
authored SI panel input for hash verification; ordinary CI does not build/run the
external tool. Multipole orders 4/6 and panel refinement are separate controls.
The largest sphere order-six run exceeded an explicit 256 MiB probe cap and was
refused; it is not a successful reference. Order-four 8192-panel sphere is within
0.056% of analytical C; order-six 12288-panel plate differs from order-four by
less than 0.004%. These outputs still require matching native geometry and
independent native mesh/air-domain convergence.
