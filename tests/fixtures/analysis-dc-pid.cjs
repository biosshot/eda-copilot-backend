// Test-only observation, inherited by the disposable DSL worker. No production flags.
const {appendFileSync}=require('node:fs');
const original=process.dlopen;
process.dlopen=function(...args){
  const loaded=Reflect.apply(original,this,args);
  const Session=args[0]?.exports?.AnalysisDcSession;
  if(Session){
    const solve=Session.prototype.solve;
    Session.prototype.solve=function(request){
      appendFileSync(process.env.ANALYSIS_DC_PID_LOG,JSON.stringify({pid:process.pid,phase:'solve-start',board:!!request.board_json})+'\n');
      try{return Reflect.apply(solve,this,[request]);}
      finally{appendFileSync(process.env.ANALYSIS_DC_PID_LOG,JSON.stringify({pid:process.pid,phase:'solve-end'})+'\n');}
    };
  }
  return loaded;
};
