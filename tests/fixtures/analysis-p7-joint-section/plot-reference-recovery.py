"""Plot the saved-model reference-recovery audit; no numerical solve."""
import json
import pathlib
import sys

import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt


def load(path):
    return json.loads(pathlib.Path(path).read_text())


def complex_value(value):
    return complex(value['real'], value['imag'])


def state(audit, frequency):
    return next(item for item in audit['states'] if item['frequencyHz'] == frequency)


def series(audit, frequency, key, part='real'):
    current = state(audit, frequency)
    return [complex_value(cut[key]).real if part == 'real' else complex_value(cut[key]).imag for cut in current['recoveredCuts']]


if len(sys.argv) != 4:
    raise SystemExit('usage: plot-reference-recovery.py <slot-audit.json> <continuous-audit.json> <output.png>')
slot = load(sys.argv[1])
continuous = load(sys.argv[2])
x = slot['cutsMm']
fig, axes = plt.subplots(2, 2, figsize=(12, 8), constrained_layout=True)
ax = axes[0, 0]
ax.plot(x, series(slot, 1000, 'BOTTOMA'), 'o-', label='slot recovered')
ax.plot(x, [complex_value(cut['BOTTOMA']).real for cut in state(slot, 1000)['sigmaECuts']], 'x--', label='slot sigma E')
ax.plot(x, series(continuous, 1000, 'BOTTOMA'), 's-', label='continuous recovered')
ax.axhline(complex_value(state(slot, 1000)['electrodeBalanceExpectedLeftA']).real, color='black', linestyle=':', label='electrode balance')
ax.set(title='Return flux through +x tetrahedral sections at 1 kHz', xlabel='x (mm)', ylabel='real RMS current (A)')
ax.grid(True, alpha=0.25)
ax.legend(fontsize=8)

ax = axes[0, 1]
ax.plot(x, series(slot, 1000, 'BOTTOMA', 'imag'), 'o-', label='slot')
ax.plot(x, series(continuous, 1000, 'BOTTOMA', 'imag'), 's-', label='continuous')
ax.set(title='Imaginary return flux at 1 kHz', xlabel='x (mm)', ylabel='imaginary RMS current (A)')
ax.grid(True, alpha=0.25)
ax.legend()

ax = axes[1, 0]
ax.plot(x, series(slot, 0, 'allA'), 'o--', label='slot DC')
ax.plot(x, series(slot, 1000, 'allA'), 'o-', label='slot 1 kHz')
ax.plot(x, series(continuous, 1000, 'allA'), 's-', label='continuous 1 kHz')
ax.axhline(0, color='black', linewidth=0.8)
ax.set(title='Sum of conduction flux over all physical conductors', xlabel='x (mm)', ylabel='real RMS current (A)')
ax.grid(True, alpha=0.25)
ax.legend(fontsize=8)

ax = axes[1, 1]
inside_slot = state(slot, 1000)['recoveredCuts'][1]
inside_continuous = state(continuous, 1000)['recoveredCuts'][1]
labels = ['slot negative-y', 'slot positive-y', 'continuous negative-y', 'continuous positive-y']
values = [complex_value(inside_slot['negativeYReturnA']).real, complex_value(inside_slot['positiveYReturnA']).real,
          complex_value(inside_continuous['negativeYReturnA']).real, complex_value(inside_continuous['positiveYReturnA']).real]
ax.bar(labels, values, color=['#b34a44', '#d59a45', '#4477aa', '#66aa88'])
ax.set(title='Branch currents at x = 1.000123 mm', ylabel='real RMS current (A)')
ax.tick_params(axis='x', rotation=20, labelsize=8)
ax.grid(True, axis='y', alpha=0.25)
fig.suptitle('P7-D1R saved joint current audit (not a physical reference certificate)')
output = pathlib.Path(sys.argv[3])
output.parent.mkdir(parents=True, exist_ok=True)
fig.savefig(output, dpi=160)
print(output)
