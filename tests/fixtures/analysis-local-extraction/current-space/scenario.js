// candidate is the saved native extraction output, injected verbatim by runner.
// Direct existing DSL; this is a refusal audit, not an admitted PCB analyzer.
const a=analysis();
for(const [name,data] of [['full',candidate.loopModel],['projected',candidate.reducedLoopModel]]){
 const m=a.model({id:'spatial-foil-'+name,kind:'network',source:'Native finite-volume foil candidate; spatial field traces; numerical null-mode blocker preserved',ports:['loop'],analyses:['networkParameters'],validity:{status:'conditional',reason:'Diagnostic extraction candidate; expected whole-band rejection, no reusable model accepted',domain:{bandHz:[0,1e5]}},data});
 const network=a.network({model:m,ports:{loop:'loop'}});
 let rejected=false;
 try{
  await a.networkParameters().network(network).frequency({startHz:1e3,stopHz:1e5,points:17,scale:'log'}).representation('Z').run();
 }catch(error){
  rejected=true;a.log({level:'info',message:'p7.current-space.refusal',data:{name,error:String(error)}});
 }
 if(!rejected)throw Error('Unexpected band admission '+name);
 a.check({name:name+' extracted AC band refused',actual:Number(rejected),min:1,max:1,unit:'1'});
}
a.check({name:'intentional false full-P7-completion claim',actual:0,min:1,unit:'1'});
a.check({name:'execution continues after failed check',actual:1,min:1,unit:'1'});
a.log({level:'info',message:'p7.current-space.completed-refusal-audit',data:{p7Complete:false,pcbExtractionAdmitted:false,viaAdmitted:false}});
