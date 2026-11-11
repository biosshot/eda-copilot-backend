import test from 'node:test';
import assert from 'node:assert/strict';
import { shrunkDomainEvidence } from '../src/analyze/board-foil-validation.ts';

test('shrunken-domain evidence requires the approximation tier and ROI', () => {
  assert.equal(shrunkDomainEvidence({}), false);
  assert.equal(shrunkDomainEvidence({ roiFallback: { tier: 'approximation', roiMmAfter: [[0, 0], [1, 1]] } }), true);
  assert.equal(shrunkDomainEvidence({ electricalRoi: { tier: 'approximation', roiMmAfter: [[0, 0], [1, 1]] } }), true);
  assert.throws(() => shrunkDomainEvidence({ roiFallback: { tier: 'noise', roiMmAfter: [[0, 0], [1, 1]] } }), /CHANNEL_FOIL_EVIDENCE/);
  assert.throws(() => shrunkDomainEvidence({ roiFallback: { tier: 'approximation' } }), /CHANNEL_FOIL_EVIDENCE/);
});
