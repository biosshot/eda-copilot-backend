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
const packageModel=a.model({kind:'thermal-network',nodes:[{id:'junction',heatCapacityJPerK:0.2},{id:'case',heatCapacityJPerK:0.8}],links:[{id:'jc',from:'junction',to:'case',resistanceKPerW:3}],contacts:[{node:'case',target:component('U3'),resistanceKPerW:8}]});
const power={kind:'step',from:0,to:0.8,atS:5,riseS:0};
const cooling={boardSurfaces:a.model({kind:'cooling',resistanceKPerW:35})};
const base=a.thermal().components([device({designator:'U3',model:packageModel})]).stackup(stack).materials(materials)
 .heat([{target:component('U3'),node:'junction',powerW:power}]).cooling(cooling)
 .environment({name:'authored room step',ambientC:{kind:'step',from:25,to:30,atS:20,riseS:0}})
 .inputPolicy({cadFill:'snapshot'}).budget({maxSeconds:900,maxMemoryMiB:1024});
// One complete final control per report; the 128 MiB scenario cap is unchanged.
// Earlier 4/2 mm failures remain historical evidence, not accepted mesh gates.
const steady=await base.name('steady').mesh({maxEdgeMm:1,maxNodes:140000,geometryDeviationMm:0.005,
 refinement:{maxPasses:5,relativeTolerance:0.02,validationMaxEdgeMm:0.5}}).domain('steady').atS(60).run();
a.plot({title:'Controller U3 / authored compact package: steady junction',data:steady.temperature({target:component('U3'),node:'junction'})});
a.plot({title:'Steady compact case temperature',data:steady.temperature({target:component('U3'),node:'case'})});
a.plot({title:'Steady junction to case heat flow',data:steady.heatFlow('U3:jc')});
a.plot({title:'Steady power balance',data:steady.energyBalance});
a.plot({title:'Independent-control layered board temperature map',data:steady.temperatureMap});
a.check({name:'Expected strict thermal failure',actual:steady.temperature({target:component('U3'),node:'junction'}),max:25,unit:'degC'});
a.check({name:'Continue after failed check',actual:steady.energyBalance.abs().max(),max:0.0001,unit:'W'});
a.check({name:'Conditional temperature has finite physical value',actual:steady.temperature({target:component('U3'),node:'junction'}).max(),min:25,unit:'degC'});
