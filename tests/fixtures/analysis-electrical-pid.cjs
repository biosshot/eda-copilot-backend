// Test-only instrumentation of the production native session, no alternate solver.
const{appendFileSync}=require('node:fs');
const original=process.dlopen;
process.dlopen=function(...args){
 const loaded=Reflect.apply(original,this,args),Session=args[0]?.exports?.AnalysisElectricalSession;
 if(Session){const solve=Session.prototype.solve;Session.prototype.solve=function(request){
  appendFileSync(process.env.ANALYSIS_ELECTRICAL_PID_LOG,JSON.stringify({pid:process.pid,phase:'solve-start',mode:request.mode})+'\n');
  try{return Reflect.apply(solve,this,[request]);}finally{appendFileSync(process.env.ANALYSIS_ELECTRICAL_PID_LOG,JSON.stringify({pid:process.pid,phase:'solve-end',mode:request.mode})+'\n');}
 };}return loaded;
};
