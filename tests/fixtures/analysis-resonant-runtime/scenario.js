// Harness prefixes authored + coarseAuthored from immutable checkpoint fixtures.
const a=analysis();
const model=a.model(authored),network=a.network({model,ports:{left:'p1',right:'p2'}});
const drivers=['left','right'].map((p,i)=>({port:network.port(p),model:{kind:'linear-driver',lowV:0,highV:i===0?1:.4,outputResistanceOhm:25},stimulus:{kind:'waveform',voltageV:{kind:'step',from:0,to:i===0?1:.4,atS:i===0?0:1.5e-7,riseS:2e-7}}}));
for(const [name,relativeTolerance,absoluteTolerance,maxStepS]of [['coarse',1e-5,1e-9,2e-9],['fine',2.5e-6,2.5e-10,1e-9]]){
 const result=await a.si().network(network).initial('steady').drivers(drivers).time({stopS:2e-5,sampleStepS:1e-8}).numerics({relativeTolerance,absoluteTolerance,maxStepS}).budget({maxSeconds:60,maxMemoryMiB:256}).run();
 a.log({level:'info',message:'resonant.case',data:{name,resultId:result.id}});
 for(const p of ['left','right'])a.plot({title:name+':'+p,data:result.voltage(network.port(p))});
 if(name==='fine')a.plot({title:'fine:left-abs',data:result.voltage(network.port('left')).abs()});
}
const frequencies=Array.from({length:129},(_,i)=>1e4*10**((i+.31873)/33));
// Ordinary AC uses original samples; SI fitting does not alter this path.
const ac=await a.networkParameters().network(network).frequency({startHz:frequencies[0],stopHz:frequencies.at(-1),points:129,scale:'log'}).representation('Y').budget({maxSeconds:60,maxMemoryMiB:256}).run();
a.log({level:'info',message:'resonant.case',data:{name:'ac',resultId:ac.id}});
for(const out of ['left','right'])for(const input of ['left','right'])a.plot({title:'ac:'+out+':'+input,data:ac.parameter({out,in:input}).magnitude()});
for(const kind of ['U','closure','shape','NaN','active','conditioning','grid','source','Z','class']){
 try{
  const r=JSON.parse(JSON.stringify(kind==='grid'?coarseAuthored:authored));r.id='negative-'+kind;
  if(kind==='U')r.data.temporal.modeTransform=[[1,1],[0,1]];
  if(kind==='closure')delete r.data.temporal.dcSlopeF;
  if(kind==='shape')r.data.temporal.highFrequency.inverseInductancePerH=[1e5];
  if(kind==='NaN')r.data.temporal.normalizationOhm=NaN;
  if(kind==='active')for(const m of r.data.parameters)for(const row of m)for(const z of row){z.real=-z.real;z.imaginary=-z.imaginary;}
  if(kind==='conditioning')r.data.frequenciesHz=r.data.frequenciesHz.map((f,i)=>i===0?0:1e6*(1+i*1e-15));
  if(kind==='source')r.validity.domain.bandHz=[0,2e6];
  if(kind==='Z')r.data.representation='Z';
  if(kind==='class')r.data.temporal.method='unknown-resonant-class';
  const n=a.network({model:a.model(r),ports:{left:'p1',right:'p2'}});
  await a.si().network(n).initial('steady').drivers([{...drivers[0],port:n.port('left')},{...drivers[1],port:n.port('right')}]).time({stopS:1e-6,sampleStepS:1e-8}).budget({maxSeconds:60,maxMemoryMiB:256}).run();
  throw Error('Unexpected acceptance '+kind);
 }catch(e){if(String(e).includes('Unexpected acceptance'))throw e;a.log({level:'info',message:'resonant.negative',data:{kind,error:String(e)}});}
}
