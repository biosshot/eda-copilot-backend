import test from 'node:test';
import assert from 'node:assert/strict';
import { DEFAULT_MAX_UNRESOLVED_RETURN, returnGateDecision, runAutoFoil } from '../src/analyze/auto-foil-response.ts';
import type { AutoFoilPlan } from '../src/analyze/auto-foil-plan.ts';

test('return gate: full domain expands, shrunken domain is truncated up to the limit and refused above it',()=>{
 assert.equal(DEFAULT_MAX_UNRESOLVED_RETURN,0.10);
 assert.equal(returnGateDecision(true,0.5,true,0.1),'passed');
 assert.equal(returnGateDecision(false,0.059,false,0.1),'expand');
 // NanoScope 2026-11-11: 5.9% unresolved on the 2 mm corridor.
 assert.equal(returnGateDecision(false,0.059,true,0.1),'truncated');
 assert.equal(returnGateDecision(false,0.1,true,0.1),'truncated');
 assert.equal(returnGateDecision(false,0.1000001,true,0.1),'refuse');
 assert.equal(returnGateDecision(false,0.059,true,0.05),'refuse');
});

test('truncated-domain limit must lie in (0,1]',()=>{
 for(const maxUnresolvedReturn of [0,-0.1,1.5,Number.NaN])
  assert.throws(()=>runAutoFoil({} as AutoFoilPlan,[],{maxSeconds:1,maxMemoryMiB:1,maxUnresolvedReturn},undefined,()=>{throw Error('unreachable');}),
   /CHANNEL_FOIL_GRID: budget.maxUnresolvedReturn/);
});
