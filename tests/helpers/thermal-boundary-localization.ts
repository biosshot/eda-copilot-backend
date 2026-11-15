type P=[number,number];
/** Only the unchanged CAD outline ring can explain Boolean-transport rounding.
 * No nearest-triangle lookup, hole repair or interior extrapolation is allowed. */
export function thermalBoundaryLocalizer(solution:any){
 const g=solution.spatial.geometry,source=g.outline?.outer as P[]|undefined;
 const booleanBound=g.booleanErrorBoundMm;
 const mappings:{polygon:number;domain:P[];source:P[];roundoffMm:number;vertexBoundMm:number}[]=[];
 if(typeof solution.spatial.boardIdentity!=='string'||!solution.spatial.boardIdentity||!Array.isArray(source)||source.length<3||source.length*source.length>1000000||!Number.isFinite(booleanBound)||booleanBound<=0)return()=>undefined;
 const scale=Math.max(1,...source.flat().map(Math.abs)),roundoffMm=8*Number.EPSILON*scale;
 // geometry/mod.rs: axis bound=max(1e-12,span*64/2^29), including both
 // Boolean operands/output. sqrt(2) converts that axis bound to Euclidean mm.
 const representationLimitMm=Math.SQRT2*booleanBound+roundoffMm;
 for(const [polygon,poly]of g.physicalDomain.entries()){
  const ring=poly.outer as P[];if(ring.length!==source.length||ring.length*ring.length>1000000)continue;
  const matches:P[][]=[];
  for(let start=0;start<source.length;start++)for(const direction of [1,-1]){
   const mapped=ring.map((_,i)=>source[(start+direction*i+source.length)%source.length]);
   if(ring.every((p,i)=>Math.hypot(p[0]-mapped[i][0],p[1]-mapped[i][1])<=representationLimitMm))matches.push(mapped);
  }
  // A different/ambiguous ring is never localized onto a convenient boundary.
  if(matches.length!==1)continue;
  const mapped=matches[0],vertexBoundMm=Math.max(...ring.map((p,i)=>Math.hypot(p[0]-mapped[i][0],p[1]-mapped[i][1])));
  mappings.push({polygon,domain:ring,source:mapped,roundoffMm,vertexBoundMm});
 }
 return(p:P)=>{
  for(const m of mappings)for(let edge=0;edge<m.domain.length;edge++){
   const next=(edge+1)%m.domain.length,a=m.domain[edge],b=m.domain[next],dx=b[0]-a[0],dy=b[1]-a[1],l=dx*dx+dy*dy;
   if(!l)continue;const t=((p[0]-a[0])*dx+(p[1]-a[1])*dy)/l;
   if(t<0||t>1)continue;
   const transported:P=[a[0]+t*dx,a[1]+t*dy];
   // The sample must itself be on this transported edge to binary64 roundoff.
   // A nearby interior/cutout/other-boundary point does not qualify.
   if(Math.hypot(p[0]-transported[0],p[1]-transported[1])>m.roundoffMm)continue;
   const sa=m.source[edge],sb=m.source[next],localizedPointMm:P=[sa[0]+t*(sb[0]-sa[0]),sa[1]+t*(sb[1]-sa[1])];
   const endpointBoundMm=Math.max(Math.hypot(a[0]-sa[0],a[1]-sa[1]),Math.hypot(b[0]-sb[0],b[1]-sb[1]));
   const displacementMm=Math.hypot(p[0]-localizedPointMm[0],p[1]-localizedPointMm[1]),limitMm=endpointBoundMm+2*m.roundoffMm;
   if(displacementMm>limitMm)continue;
   return{localizedPointMm,displacementMm,limitMm,representationLimitMm,polygon:m.polygon,edge,fraction:t,transportedEdge:[a,b],sourceEdge:[sa,sb],roundoffMm:m.roundoffMm,formula:'max(endpoint transport displacements)+2*(8*Number.EPSILON*max(1,abs(source coordinates))); whole-ring unique cyclic correspondence within sqrt(2)*reported Boolean axis bound+roundoff'};
  }
 };
}
