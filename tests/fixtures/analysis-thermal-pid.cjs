// Test-only observation of the production native function, no alternate solver.
const {appendFileSync}=require('node:fs');const original=process.dlopen;
process.dlopen=function(...args){const loaded=Reflect.apply(original,this,args),exports=args[0]?.exports;
 if(exports?.solveAnalysisThermal){const solve=exports.solveAnalysisThermal;exports.solveAnalysisThermal=function(request){
  appendFileSync(process.env.ANALYSIS_THERMAL_PID_LOG,JSON.stringify({pid:process.pid,phase:'solve-start',mode:request.mode})+'\n');
  try{return Reflect.apply(solve,this,[request]);}finally{appendFileSync(process.env.ANALYSIS_THERMAL_PID_LOG,JSON.stringify({pid:process.pid,phase:'solve-end'})+'\n');}
 };}return loaded;
};
