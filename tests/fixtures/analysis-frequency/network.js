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
