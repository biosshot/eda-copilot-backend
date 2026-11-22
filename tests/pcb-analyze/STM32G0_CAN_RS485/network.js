const binding={"input": {"positive": ["U2", "1", "+5V"], "negative": ["U2", "2", "GND"]}, "output": {"positive": ["U2", "5", "+3V3"], "negative": ["U2", "2", "GND"]}, "assumptions": "U2 regulator input/output/return contacts verified in saved PCB. The independent GHz RF examples below are synthetic benchmarks; this low-frequency controller is NOT modeled as a GHz amplifier. No coupling of models to PCB copper."};
const bindingContext=analysis({board});bindingContext.log({level:"warning",message:"network.binding",data:binding});
// Synthetic frequency models. No PCB extraction, measured RF claims or transient model.
const a=analysis();
const first=a.network({model:models.electrical.attenuatorPad,ports:{input:'p1',output:'p2'}});
const second=a.network({model:models.electrical.secondPad,ports:{input:'p1',output:'p2'}});
const cascade=a.connect({networks:[first,second],connections:[{from:first.port('output'),to:second.port('input')}],ports:{input:first.port('input'),output:second.port('output')}});
const amplifier=a.network({model:a.model(models.electrical.amplifier),ports:{input:'p1',output:'p2'}});
const coupled=a.network({model:models.electrical.coupled,ports:{input:'p1',output:'p2',load:'p3'}});
const load=a.network({model:models.electrical.load,ports:{termination:'p'}});
const terminated=a.connect({networks:[coupled,load],connections:[{from:coupled.port('load'),to:load.port('termination')}],ports:{input:coupled.port('input'),output:coupled.port('output')}});
const frequency={startHz:1e9,stopHz:3e9,points:9,scale:'linear'};
for(const [name,network,referenceOhm] of [['Attenuator Pad',first,50],['reflecting cascade',cascade,50],['cascade reference 75',cascade,75],['active nonreciprocal',amplifier,50],['coupled with explicit load',terminated,50]]){
 const r=await a.networkParameters().network(network).frequency(frequency).representation('S').referenceOhm(referenceOhm).run();
 a.log({level:'info',message:'network.case',data:{name,resultId:r.id,referenceOhm,source:'synthetic authored frequency model'}});
 for(const [out,input] of [['output','input'],['input','output'],['input','input'],['output','output']]){
  const parameter=r.parameter({out,in:input});
  for(const [part,data] of [['magnitude',parameter.magnitude()],['phase',parameter.phaseDeg()],['real',parameter.real()],['imaginary',parameter.imaginary()]])a.plot({title:name+':'+out+'<-'+input+':'+part,data});
 }
 a.check({name:name+': expected fail',actual:r.parameter({out:'output',in:'input'}).magnitude().max(),max:0,unit:'1'});
 a.check({name:name+': continued',actual:r.parameter({out:'output',in:'input'}).magnitude().max(),min:0,unit:'1'});
}

// Explicit section hypothesis at its stated low-frequency band, independent of RF benchmarks.
const sectionContext=analysis();
const section=sectionContext.network({model:models.electrical.boardSection,ports:{input:'input',output:'output'}});
const sectionResult=await sectionContext.networkParameters().network(section).frequency({startHz:1000,stopHz:100000,points:9,scale:'linear'}).representation('Z').run();
sectionContext.log({level:'info',message:'board.network.case',data:{resultId:sectionResult.id,resourceId:models.electrical.boardSection.id,binding}});
for(const part of ['magnitude','phaseDeg','real','imaginary'])sectionContext.plot({title:'board section:'+part,data:sectionResult.parameter({out:'output',in:'input'})[part]()});
sectionContext.check({name:'board section mutual impedance hypothesis',actual:sectionResult.parameter({out:'output',in:'input'}).real().min(),target:20,tolerance:1e-8,unit:'Ohm'});
