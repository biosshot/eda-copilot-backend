// Extracted internal certificate arrives with model data; users supply no gauges.
const a=analysis();
for(const [name,data,ports] of [['full',candidate.model,['top','bottom']],['projected',candidate.reducedModel,['top','bottom']]]){
 const m=a.model({id:'structural-foil-'+name,kind:'network',source:'Native insulated analytic two-foil extraction with structural reference certificate',ports,analyses:['networkParameters'],validity:{status:'conditional',reason:'Bounded insulated analytic coupon only; no PCB, via or full-P7 admission',domain:{bandHz:[0,1e5]}},data});
 const network=a.network({model:m,ports:{top:'top',bottom:'bottom'}});
 const result=await a.networkParameters().network(network).frequency({startHz:1e3,stopHz:1e5,points:17,scale:'log'}).representation('Z').run();
 a.plot({title:name+' physical longitudinal |Z11|',data:result.parameter({out:'top',in:'top'}).magnitude()});
 a.log({level:'info',message:'p6-p7.structural-gauge.complete',data:{name}});
 a.check({name:name+' coupon AC completed',actual:1,min:1,max:1,unit:'1'});
}
a.check({name:'intentional false full-P7-completion claim',actual:0,min:1,unit:'1'});
a.check({name:'execution continues after failed check',actual:1,min:1,unit:'1'});
a.log({level:'info',message:'p6-p7.structural-gauge.continued',data:{p7Complete:false,pcbExtractionAdmitted:false,viaAdmitted:false}});
