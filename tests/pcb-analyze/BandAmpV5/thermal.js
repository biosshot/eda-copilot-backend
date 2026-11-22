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
const power={kind:'step',from:0,to:3.0,atS:5,riseS:0};
const cooling={boardSurfaces:a.model({kind:'cooling',resistanceKPerW:35}),heatsinks:[{target:component('U1'),thermalNode:'case',model:a.model({kind:'heatsink',heatCapacityJPerK:20}),interface:a.model({kind:'thermal-interface',resistanceKPerW:0.8}),cooling:a.model({kind:'cooling',scale:{kind:'step',from:0.2,to:1,atS:30,riseS:0},table:[{deltaTemperatureK:-200,heatFlowW:-40},{deltaTemperatureK:0,heatFlowW:0},{deltaTemperatureK:20,heatFlowW:2},{deltaTemperatureK:100,heatFlowW:20},{deltaTemperatureK:200,heatFlowW:40}]})}]};
const base=a.thermal().components([device({designator:'U1',model:packageModel})]).stackup(stack).materials(materials)
 .heat([{target:component('U1'),node:'junction',powerW:power}]).cooling(cooling)
 .environment({name:'authored room step',ambientC:{kind:'step',from:25,to:30,atS:20,riseS:0}})
 .inputPolicy({cadFill:'snapshot'}).budget({maxSeconds:120,maxMemoryMiB:384});
const coarse=await base.name('steady-coarse').mesh({maxEdgeMm:4,maxNodes:50000}).domain('steady').atS(60).run();
const fine=await base.name('steady-fine').mesh({maxEdgeMm:2,maxNodes:50000}).domain('steady').atS(60).run();
const time=await base.name('time').mesh({maxEdgeMm:2,maxNodes:50000}).domain('time').initialC('ambient').numerics({relativeTolerance:0.0001,absoluteTolerance:0.00001}).time({stopS:60,sampleStepS:15}).run();
const refined=await base.name('time-refined').mesh({maxEdgeMm:2,maxNodes:50000}).domain('time').initialC('ambient').numerics({relativeTolerance:0.000025,absoluteTolerance:0.0000025}).time({stopS:60,sampleStepS:15}).run();
a.plot({title:'Amplifier U1 / conditional radiator and TIM: junction temperatures',data:time.temperature({target:component('U1'),node:'junction'})});
a.plot({title:'Compact case temperature',data:time.temperature({target:component('U1'),node:'case'})});
a.plot({title:'Internal junction to case heat flow',data:time.heatFlow('U1:jc')});
a.plot({title:'Discrete energy balance',data:time.energyBalance});
a.plot({title:'Layered board final temperature map',data:time.temperatureMap});
a.check({name:'Expected strict thermal failure',actual:fine.temperature({target:component('U1'),node:'junction'}),max:25,unit:'degC'});
a.check({name:'Continue after failed check',actual:time.energyBalance.abs().max(),max:0.0001,unit:'J'});
a.check({name:'Conditional temperature has finite physical value',actual:refined.temperature({target:component('U1'),node:'junction'}).max(),min:25,unit:'degC'});
