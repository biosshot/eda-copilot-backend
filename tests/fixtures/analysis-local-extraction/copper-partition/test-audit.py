"""Fast saved-evidence invariants; no solver or kernel re-execution."""
import unittest,json,hashlib
import numpy as np
from pathlib import Path
from trace import ROOT,FIX,OUT,OLD,read,sha
class EvidenceTests(unittest.TestCase):
 def test_actual_native_partition_and_contacts(self):
  census=read(OUT/'census.json')
  for r in census['rows']:
   for f in r['families']:
    self.assertTrue(f['planMatchesFrozenNativeCounts'])
    self.assertEqual(sum(f['primaryCounts'].values()),f['nearPairs'])
    self.assertEqual(sum(f['whyCounts'].values()),f['nearPairs'])
    self.assertEqual(f['nearPairs']+f['farPairs'],f['supports']*(f['supports']+1)//2)
    self.assertEqual(f['primaryCounts']['overlap'],0)
    self.assertEqual(f['primaryCounts']['topology_indeterminate'],0)
 def test_one_frozen_batch_and_telemetry_parity(self):
  run=read(OUT/'run.json');pre=read(FIX/'measurement-preflight.json');a=read(OUT/'audit.json')
  self.assertEqual(run['inputSha256'],sha(FIX/'measurement-preflight.json'))
  self.assertEqual(len(pre['samples']),28);self.assertEqual(pre['fixedOrders'],[[12,8],[24,16]])
  self.assertEqual(run['exitCode'],0);self.assertIsNone(run['externalRefusal'])
  self.assertLess(run['seconds'],120);self.assertLess(run['peakBytes'],512*1024**2)
  rows=[json.loads(l) for l in (OUT/'batch.jsonl').read_text().splitlines()]
  self.assertEqual(sum(r['kind']=='done' for r in rows),1)
  self.assertEqual(sum(r['kind']=='measurement' for r in rows),56)
  self.assertTrue(all(r['instrumentationParity'] is not False for r in a['caseRows']))
 def test_provenance_and_exact_native_replay(self):
  a=read(OUT/'audit.json');t=read(OUT/'stm-trace.json')
  self.assertEqual(len(a['replay']),3);self.assertTrue(all(r['exactCoordinatesMatch'] and r['symmetricDifferenceMm2']==0 for r in a['replay']))
  self.assertTrue(all(r['differenceUlps']==0 for r in t['lineIntersectionWitnesses']))
  endpoint=t['authoredSharedEndpointMm']
  for track in t['normalizedTracks']:
   self.assertTrue(any([p['x'],p['y']]==endpoint for p in track['points']))
  b=read(OUT/'build.json')
  for p,h in b['sources'].items():self.assertEqual(sha(ROOT/p),h)
  source=(ROOT/'native/analyze/src/extraction/local/polyhedron.rs').read_text()
  replacement=b['instrumentationExactReplacement']
  self.assertEqual((OUT/'poly-count.rs').read_text(),source.replace(replacement['needle'],replacement['replacement']))
 def test_failure_and_unpriced_strata_are_not_accepted(self):
  a=read(OUT/'audit.json')
  self.assertEqual(len(a['failures']),2)
  self.assertTrue(all(r['fixture']=='STM32G0_CAN_RS485' and r['indices']==[613,613] for r in a['failures']))
  self.assertEqual(sorted(r['xyOrder'] for r in a['failures']),[12,24])
  self.assertTrue(any(r['hasObservedRefusal'] for r in a['categoryRows']))
  self.assertGreater(sum(r['unpricedPairCount'] for r in a['categoryRows']),0)
  for r in a['roiRows']:
   self.assertEqual(r['guaranteedSecondsSaved'],0);self.assertIsNone(r['guaranteedRuntimeUpper'])
   self.assertLess(r['remainingNearPairsAfterLeafFilter'],r['nearPairs'])
 def test_reused_full_physical_energy_control(self):
  a=read(OUT/'audit.json');c=a['couponControlReuse'];old=read(OLD/'magnetic-near-far/audit.json')
  ref=np.array(c['referenceMatrixH']);v,q=np.linalg.eigh((ref+ref.T)/2);self.assertGreater(v.min(),0)
  w=(q/np.sqrt(v))@q.T;matrix=np.array(c['candidateMatrixH']);delta=(matrix+matrix.T-ref-ref.T)/2
  error=max(abs(np.linalg.eigvalsh(w@delta@w)))
  self.assertAlmostEqual(error+old['reference']['workingUncertainty'],c['fullScreen'],places=12)
  bound=np.array(c['analyticBoundMatrixH']);self.assertLess(max(abs(np.linalg.eigvalsh(w@bound@w))),.01)
  self.assertLess(c['fullScreen'],.05);self.assertLess(c['rawReciprocity'],1e-9);self.assertGreater(np.linalg.eigvalsh((matrix+matrix.T)/2).min(),0)
if __name__=='__main__':unittest.main(verbosity=2)
