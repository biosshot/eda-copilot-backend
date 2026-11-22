"""Saved-model joint current audit. No field assembly or FEM solve."""
import json
import pathlib
import sys

import numpy as np

CUTS_MM = [0.500123, 1.000123, 1.500123]
CURRENT_FLOOR_A = 1e-6


def load(path):
    return json.loads(pathlib.Path(path).read_text())


def entries(shape, values):
    out = np.zeros(shape, dtype=float)
    for i, j, value in values:
        out[i, j] += value
    return out


def dense_state(prepared, frequency_hz):
    model = prepared['candidate']['model']
    nv = len(model['potentialNodes'])
    ni = len(model['currentDofs'])
    ports = len(prepared['request']['ports'])
    n = nv + ni
    g = np.zeros((n, n))
    e = np.zeros((n, n))
    a = entries((nv, ni), model['incidence'])
    g[:nv, nv:] = a
    g[nv:, :nv] = -a.T
    g[nv:, nv:] = entries((ni, ni), model['resistanceOhm'])
    e[:nv, :nv] = entries((nv, nv), model['capacitanceF'])
    e[nv:, nv:] = entries((ni, ni), model['inductanceH'])
    b = entries((nv, ports), model['portIncidence'])
    for port in range(ports):
        weights = b[:, port]
        g[:nv, :nv] += np.outer(weights, weights) / 50.0
    drive = np.array([1.0, 0.0, -1.0, 0.0])
    rhs = np.zeros(n, dtype=complex)
    rhs[:nv] = b @ drive
    matrix = g.astype(complex) if frequency_hz == 0 else g + 2j * np.pi * frequency_hz * e
    if frequency_hz == 0:
        # Remove only the common loaded-network voltage gauge. Returning the
        # unanchored NumPy solve can produce enormous, nonphysical offsets.
        free = np.arange(1, n)
        state = np.zeros(n, dtype=complex)
        state[free] = np.linalg.solve(matrix[np.ix_(free, free)], rhs[free])
        return state, nv, ni
    return np.linalg.solve(matrix, rhs), nv, ni


def native_sample(capture, frequency_hz):
    return next(s for s in capture['response']['samples'] if s['frequencyHz'] == frequency_hz)


def recovered_currents(sample):
    values = np.asarray(sample['currentRealImaginaryA'], dtype=float)
    return values[:, 0] + 1j * values[:, 1]


def native_ports(sample):
    return np.asarray(sample['portRealV']) + 1j * np.asarray(sample['portImaginaryV'])


def full_potentials(prepared, reduced):
    basis = np.asarray(prepared['candidate']['voltageBasis'])
    return reduced @ basis


def potential_mesh(prepared, full):
    out = np.zeros(len(prepared['nodesM']), dtype=complex)
    assigned = np.zeros(len(out), dtype=bool)
    for dof, nodes in enumerate(prepared['potentialMeshNodes']):
        for node in nodes:
            if assigned[node] and out[node] != full[dof]:
                raise ValueError('inconsistent electrode quotient')
            out[node] = full[dof]
            assigned[node] = True
    if not assigned[np.unique(np.asarray(prepared['copperCells']).ravel())].all():
        raise ValueError('incomplete copper potential mapping')
    return out


def conduction_currents(prepared, full):
    mesh = potential_mesh(prepared, full)
    result = np.zeros(3 * len(prepared['copperCells']), dtype=complex)
    for cell, nodes in enumerate(prepared['copperCells']):
        points = np.asarray([prepared['nodesM'][i] for i in nodes])
        differences = points[1:] - points[0]
        voltage_differences = mesh[nodes[1:]] - mesh[nodes[0]]
        gradient = np.linalg.solve(differences, voltage_differences)
        length = 1.0 / np.sqrt(prepared['cellDensityPerAmpM2'][cell])
        native_gradient = -length * gradient
        result[3 * cell:3 * cell + 3] = native_gradient / prepared['cellResistanceOhm'][cell]
    return result


def layer_map(prepared):
    return {row[0]: row[3] for row in prepared['mapping']}


def cell_layers(prepared):
    layers = layer_map(prepared)
    return [sorted({layers.get(source, 'UNKNOWN') for source in ids}) for ids in prepared['sourceIds']]


def section_polygons(prepared, cut_x_mm):
    cut = (cut_x_mm - prepared['originMm'][0]) * 1e-3
    for cell, nodes in enumerate(prepared['copperCells']):
        vertices = np.asarray([prepared['nodesM'][i] for i in nodes])
        points = []
        for a in range(4):
            for b in range(a):
                xa = vertices[a, 0] - cut
                xb = vertices[b, 0] - cut
                if xa * xb < 0:
                    points.append((vertices[a] + (vertices[b] - vertices[a]) * xa / (xa - xb))[1:])
        if len(points) < 3:
            continue
        points = np.unique(np.asarray(points), axis=0)
        if len(points) < 3:
            continue
        center = points.mean(axis=0)
        order = np.argsort(np.arctan2(points[:, 1] - center[1], points[:, 0] - center[0]))
        points = points[order]
        twice_area = sum(points[i, 0] * points[(i + 1) % len(points), 1] - points[i, 1] * points[(i + 1) % len(points), 0] for i in range(len(points)))
        area = abs(twice_area) / 2.0
        physical_y_mm = center[0] * 1000.0 + prepared['originMm'][1]
        yield cell, area, physical_y_mm


def flux_by_cut(prepared, currents, cut_x_mm):
    currents = np.asarray(currents).reshape((-1, 3))
    density = np.asarray(prepared['cellDensityPerAmpM2'])
    layers = cell_layers(prepared)
    result = {'cutXMm': cut_x_mm, 'allA': 0j, 'TOPA': 0j, 'BOTTOMA': 0j,
              'negativeYReturnA': 0j, 'positiveYReturnA': 0j,
              'bottomAreaM2': 0.0, 'allAreaM2': 0.0, 'bottomCells': 0,
              'unknownLayerCells': 0}
    for cell, area, y_mm in section_polygons(prepared, cut_x_mm):
        flux = area * currents[cell, 0] * density[cell]
        result['allA'] += flux
        result['allAreaM2'] += area
        cell_layerset = set(layers[cell])
        if 'UNKNOWN' in cell_layerset:
            result['unknownLayerCells'] += 1
        for layer in ['TOP', 'BOTTOM']:
            if layer in cell_layerset:
                result[layer + 'A'] += flux
        if 'BOTTOM' in cell_layerset:
            result['bottomAreaM2'] += area
            result['bottomCells'] += 1
            key = 'negativeYReturnA' if y_mm < -0.08 else 'positiveYReturnA'
            result[key] += flux
    return result


def serializable(value):
    if isinstance(value, np.ndarray):
        return serializable(value.tolist())
    if isinstance(value, complex):
        return {'real': value.real, 'imag': value.imag, 'magnitude': abs(value)}
    if isinstance(value, np.complexfloating):
        return serializable(complex(value))
    if isinstance(value, np.floating):
        return float(value)
    if isinstance(value, np.integer):
        return int(value)
    if isinstance(value, dict):
        return {key: serializable(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [serializable(item) for item in value]
    return value


def audit(packet, capture_path):
    packet = pathlib.Path(packet)
    capture = load(capture_path)
    prepared = load(packet / 'joint-fine.json')['report']['prepared']
    q = np.asarray(prepared['candidate']['currentBasis'])
    cell_count = len(prepared['copperCells'])
    result = {
        'packet': str(packet),
        'capture': str(pathlib.Path(capture_path)),
        'sourceSha256': capture['sourceSha256'],
        'addonSha256': capture['addonSha256'],
        'counts': {'potentialFull': prepared['potentialCount'], 'currentFull': prepared['currentCount'],
                   'potentialReduced': len(prepared['candidate']['model']['potentialNodes']),
                   'currentReduced': len(prepared['candidate']['model']['currentDofs']),
                   'copperCells': cell_count},
        'savedFullState': {'fullChecks': len(prepared['candidate'].get('fullChecks', [])),
                           'verificationStatus': prepared['candidate'].get('verificationStatus'),
                           'missing': 'No fullSpace current/potential state at the loaded 1 kHz drive is saved.'},
        'states': [], 'cutsMm': CUTS_MM, 'currentFloorA': CURRENT_FLOOR_A,
    }
    for frequency_hz in [0, 1000]:
        sample = native_sample(capture, frequency_hz)
        reduced, nv, ni = dense_state(prepared, frequency_hz)
        native = recovered_currents(sample)
        reconstructed = reduced[nv:] @ q
        reconstruction_error = np.max(np.abs(native - reconstructed))
        ports = native_ports(sample)
        expected_left = -1.0 + (ports[0] + ports[1]) / 50.0
        expected_right = -1.0 - (ports[2] + ports[3]) / 50.0
        full = full_potentials(prepared, reduced[:nv])
        conduction = conduction_currents(prepared, full)
        conduction_error = np.linalg.norm(native - conduction) / max(np.linalg.norm(native), 1e-30)
        state = {
            'frequencyHz': frequency_hz,
            'nativePortsV': ports,
            'densePortsV': entries((len(prepared['candidate']['model']['potentialNodes']), 4), prepared['candidate']['model']['portIncidence']).T @ reduced[:nv],
            'maxRecoveredCurrentReconstructionErrorA': reconstruction_error,
            'relativeRecoveredVersusSigmaE': conduction_error,
            'electrodeBalanceExpectedLeftA': expected_left,
            'electrodeBalanceExpectedRightA': expected_right,
            'recoveredCuts': [flux_by_cut(prepared, native, cut) for cut in CUTS_MM],
            'sigmaECuts': [flux_by_cut(prepared, conduction, cut) for cut in CUTS_MM],
        }
        if frequency_hz == 0:
            state['harmonicDc'] = {
                'initialElectrodeVoltageModes': 6,
                'initialHarmonicCurrentModes': 3,
                'maxAbsBubbleVoltageCoordinate': np.max(np.abs(reduced[6:nv])),
                'maxAbsNonHarmonicCurrentCoordinate': np.max(np.abs(reduced[nv + 3:])),
                'meaning': 'The loaded DC current stays in the original full-space harmonic current lift; later training/magnetic current modes do not create the section defect. Zero-gradient potential bubble coordinates remain gauge freedoms.',
            }
        result['states'].append(state)
    bypass_vectors = []
    longitudinal_vectors = []
    for mode in q:
        before = flux_by_cut(prepared, mode.astype(complex), CUTS_MM[0])
        inside = flux_by_cut(prepared, mode.astype(complex), CUTS_MM[1])
        after = flux_by_cut(prepared, mode.astype(complex), CUTS_MM[2])
        bypass_vectors.append([inside['negativeYReturnA'].real, inside['positiveYReturnA'].real])
        longitudinal_vectors.append([before['BOTTOMA'].real, inside['BOTTOMA'].real, after['BOTTOMA'].real])
    bypass_vectors = np.asarray(bypass_vectors)
    longitudinal_vectors = np.asarray(longitudinal_vectors)
    singular = np.linalg.svd(bypass_vectors, compute_uv=False)
    longitudinal_singular = np.linalg.svd(longitudinal_vectors, compute_uv=False)
    tolerance = max(bypass_vectors.shape) * np.finfo(float).eps * singular[0]
    longitudinal_tolerance = max(longitudinal_vectors.shape) * np.finfo(float).eps * longitudinal_singular[0]
    result['currentBasisCapability'] = {
        'bypassFluxSingularValuesA': singular,
        'independentBypassFluxRank': int(np.sum(singular > tolerance)),
        'rankTolerance': tolerance,
        'longitudinalReturnFluxSingularValuesA': longitudinal_singular,
        'independentLongitudinalFluxRank': int(np.sum(longitudinal_singular > longitudinal_tolerance)),
        'longitudinalRankTolerance': longitudinal_tolerance,
        'meaning': 'Independent basis functionals prove representability of unequal bypass currents and longitudinal redistribution, not physical continuity.',
    }
    result['sourceCoverage'] = {
        'bottomCopperCells': sum('BOTTOM' in layers for layers in cell_layers(prepared)),
        'unknownLayerCopperCells': sum('UNKNOWN' in layers for layers in cell_layers(prepared)),
        'sourceIdsPerCellEmpty': sum(not ids for ids in prepared['sourceIds']),
    }
    return serializable(result)


if __name__ == '__main__':
    if len(sys.argv) != 4:
        raise SystemExit('usage: audit-reference-recovery.py <joint-packet> <capture.json> <output.json>')
    output = pathlib.Path(sys.argv[3])
    result = audit(sys.argv[1], sys.argv[2])
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2))
    print(json.dumps(result, indent=2))
