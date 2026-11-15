// Direct Analysis DSL. Captured geometry is measured CAD data; every thermal
// coefficient, package path, source wattage and cooling profile below is an
// explicit conditional scenario hypothesis, not a component datasheet claim.
const a = analysis({ board });
a.log({level:'warning',message:'THERMAL CONDITIONAL HYPOTHESES',data:[
 'Geometry/source IDs come from the saved board. Stack/materials/packages/cooling are authored hypotheses.',
 'Separate P1 fields for copper/resin/core; copper contour constraints and cut-cell coefficients need refinement.',
 'No measured junction model, no CFD, no electrical feedback. Radiator contact, when present, is conditional.'
]});
const materials={
 copper:{thermalConductivityWPerMK:390,densityKgM3:8960,specificHeatJPerKgK:385,source:'Authored constant-property copper test hypothesis',state:'isotropic constant',validity:{status:'conditional',reason:'Unmeasured material hypothesis'}},
 resin:{thermalConductivityWPerMK:0.3,densityKgM3:1850,specificHeatJPerKgK:1100,source:'Authored resin test hypothesis, not a generic FR4 default',state:'isotropic constant',validity:{status:'conditional',reason:'Unmeasured material hypothesis'}}
};
const stack=stackup({boardThicknessMm:1.6,viaPlatingThicknessUm:25,platingMaterial:'copper',diameterConvention:'finished_hole',layers:[
 {kind:'copper',name:'TOP',thicknessMm:0.035,material:'copper',backgroundMaterial:'resin'},
 {kind:'dielectric',name:'core',thicknessMm:1.53,material:'resin'},
 {kind:'copper',name:'BOTTOM',thicknessMm:0.035,material:'copper',backgroundMaterial:'resin'}
]});
const packageModel=a.model({kind:'thermal-network',nodes:[{id:'junction',heatCapacityJPerK:0.2},{id:'case',heatCapacityJPerK:0.8}],links:[{id:'jc',from:'junction',to:'case',resistanceKPerW:3}],contacts:[{node:'case',target:component('U1'),resistanceKPerW:8}]});
const power={kind:'step',from:0,to:1.5,atS:5,riseS:0};
const cooling={boardSurfaces:a.model({kind:'cooling',resistanceKPerW:35})};
const base=a.thermal().components([device({designator:'U1',model:packageModel})]).stackup(stack).materials(materials)
 .heat([{target:component('U1'),node:'junction',powerW:power}]).cooling(cooling)
 .environment({name:'authored room step',ambientC:{kind:'step',from:25,to:30,atS:20,riseS:0}})
 .inputPolicy({cadFill:'snapshot'}).budget({maxSeconds:600,maxMemoryMiB:384});
// Explicit study600s shared request budget; no production default change.
const fine=await base.name('time').mesh({maxEdgeMm:1,maxNodes:50000,refinement:{maxPasses:5,relativeTolerance:0.02,validationMaxEdgeMm:0.5}}).domain('time').initialC('ambient').numerics({relativeTolerance:0.0001,absoluteTolerance:0.00001}).time({stopS:60,sampleStepS:15}).run();
a.plot({title:'Final policy junction temperature',data:fine.temperature({target:component('U1'),node:'junction'})});
a.plot({title:'Final policy case temperature',data:fine.temperature({target:component('U1'),node:'case'})});
a.plot({title:'Junction to case heat flow',data:fine.heatFlow('U1:jc')});
a.plot({title:'Integrated energy balance',data:fine.energyBalance});
a.plot({title:'Complete final layered temperature field',data:fine.temperatureMap});
a.check({name:'Expected strict thermal failure',actual:fine.temperature({target:component('U1'),node:'junction'}).max(),max:25,unit:'degC'});
a.check({name:'Continue after failed check',actual:fine.energyBalance.abs().max(),max:0.0001,unit:'J'});
a.check({name:'Conditional temperature has finite physical value',actual:fine.temperature({target:component('U1'),node:'junction'}).max(),min:25,unit:'degC'});
