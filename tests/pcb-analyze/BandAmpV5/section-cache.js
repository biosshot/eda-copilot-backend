// Direct captured-board channel extraction. All copper in the ROI is retained.
// ROI cuts are insulated; stack/material/plating hypotheses are explicit.
const a=analysis({board});
const copper={conductivitySM:58e6,state:'Authored bulk copper at fixed 25 C; no roughness or inferred temperature law'};
const fr4={relativePermittivity:4.2,state:'Authored constant lossless dielectric; dielectric loss not available in local v1'};
const layers=stackup({boardThicknessMm:1.6,viaPlatingThicknessUm:25,platingMaterial:'copper',diameterConvention:'finished_hole',layers:[
 {kind:'copper',name:'TOP',thicknessMm:.035,material:'copper'},
 {kind:'dielectric',name:'authored-FR4',thicknessMm:1.53,material:'fr4'},
 {kind:'copper',name:'BOTTOM',thicknessMm:.035,material:'copper'}]});
// Independent supported P7b RF section, from the exact captured RF_IN_PORT track.
// Its explicit 0.4 mm dielectric hypothesis and 500 MHz band differ from local v1.
const rfTrack=board.copper.fixed.tracks.find(t=>t.id==='dcc17162579803b7');
if(!rfTrack||rfTrack.widthMm!==0.6736)throw Error('Captured RF section identity changed');
const rf=a.line({kind:'microstrip',widthMm:rfTrack.widthMm,copperThicknessMm:.035,dielectricHeightMm:.4,lengthMm:10.6299,
 material:{relativePermittivity:4.2,lossTangent:.015,state:'Explicit supported P7b section hypothesis, not measured whole-board RF stack'},
 conductors:{signal:copper,referencePlanes:{bottom:{thicknessMm:.035,material:copper}}}});
const rfBand={startHz:5e8,stopHz:1e9,points:5,scale:'linear'};
const section=await a.impedance().target(rf).method('cross-section').frequency(rfBand)
 .mesh({maxNodes:300000,maxTriangles:600000,maxEdgeMm:1/60,domainScale:16}).budget({maxSeconds:120,maxMemoryMiB:1024}).run();
a.plot({title:'RF section supported weak-loss RLGC, separate applicability',data:section.rlgc});
const rfS=await a.networkParameters().network(section.network).frequency(rfBand).representation('S').referenceOhm(50).run();
a.plot({title:'Supported P7b RF section S21 magnitude',data:rfS.parameter({out:'output',in:'input'}).magnitude()});
a.plot({title:'Supported section cold/cache metadata',kind:'table',data:section.solution});
const warmSection=await a.impedance().target(rf).method('cross-section').frequency(rfBand)
 .mesh({maxNodes:300000,maxTriangles:600000,maxEdgeMm:1/60,domainScale:16}).budget({maxSeconds:120,maxMemoryMiB:1024}).run();
a.plot({title:'Supported section warm reuse',kind:'table',data:warmSection.solution});
a.check({name:'expected physical fail: passive RF section transmission is not zero',actual:rfS.parameter({out:'output',in:'input'}).magnitude().min(),target:0,tolerance:0,unit:'1'});
a.check({name:'continued: supported RF transmission finite',actual:rfS.parameter({out:'output',in:'input'}).magnitude().min(),min:0,max:1,unit:'1'});
