/** N-port DC algebra of the board PI endpoint (contract section 6). Pure functions over the native
 * electrode Schur matrix of the copper; sources, loads and components enter only as the small-signal DC
 * termination of their port (conductance in S: 0 open, Infinity short). No physics beyond linear algebra. */

/** Dense Gaussian elimination with partial pivoting. */
export function solveDense(matrix:readonly (readonly number[])[],rhs:readonly number[]):number[]{
 const n=rhs.length,a=matrix.map((row,i)=>[...row,rhs[i]]);
 for(let c=0;c<n;c++){
  let p=c;for(let r=c+1;r<n;r++)if(Math.abs(a[r][c])>Math.abs(a[p][c]))p=r;
  if(!(Math.abs(a[p][c])>0)||!Number.isFinite(a[p][c]))throw new Error('PI_DC_SINGULAR: singular DC port system');
  [a[c],a[p]]=[a[p],a[c]];
  for(let r=c+1;r<n;r++){const f=a[r][c]/a[c][c];if(f!==0)for(let k=c;k<=n;k++)a[r][k]-=f*a[c][k];}
 }
 const x=Array<number>(n).fill(0);
 for(let r=n-1;r>=0;r--){let s=a[r][n];for(let k=r+1;k<n;k++)s-=a[r][k]*x[k];x[r]=s/a[r][r];}
 return x;
}

/** Port voltages for +1 A injected at port `j` (positive electrode in, return electrode out) with every other port
 * terminated by its DC conductance and the port's own termination removed. `galvanic[e]` is the copper island of electrode e.
 * Returns null when the two electrodes of port `j` are not joined by copper plus terminations (open circuit, no finite impedance). */
export function terminatedPortResponse(
 G:readonly (readonly number[])[],galvanic:readonly number[],pairs:readonly (readonly [number,number])[],
 conductanceS:readonly number[],j:number,
):number[]|null{
 const e=G.length,parent=[...Array(e).keys()];
 const find=(x:number):number=>parent[x]===x?x:(parent[x]=find(parent[x]));
 // `parent` merges electrodes only where a short makes them one node; `cp` is copper connectivity (islands, shorts, terminations).
 pairs.forEach(([p,n],k)=>{if(k!==j&&conductanceS[k]===Infinity)parent[find(p)]=find(n);});
 const roots=[...new Set([...Array(e).keys()].map(find))],at=new Map(roots.map((r,i)=>[r,i]));
 const m=roots.length,idx=(x:number)=>at.get(find(x))!;
 const cp=[...Array(m).keys()];
 const cfind=(x:number):number=>cp[x]===x?x:(cp[x]=cfind(cp[x]));
 const cjoin=(a:number,b:number)=>{cp[cfind(a)]=cfind(b);};
 const first=new Map<number,number>();
 galvanic.forEach((c,i)=>{const f=first.get(c);if(f===undefined)first.set(c,idx(i));else cjoin(idx(i),f);});
 const A=Array.from({length:m},()=>Array<number>(m).fill(0));
 for(let a=0;a<e;a++)for(let b=0;b<e;b++)if(G[a][b]!==0)A[idx(a)][idx(b)]+=G[a][b];
 pairs.forEach(([p,n],k)=>{
  const g=conductanceS[k];
  if(k===j||!(g>0)||g===Infinity)return;
  const [u,v]=[idx(p),idx(n)];if(u===v)return;
  A[u][u]+=g;A[v][v]+=g;A[u][v]-=g;A[v][u]-=g;cjoin(u,v);
 });
 const comp=Array.from({length:m},(_,i)=>cfind(i));
 const [pj,nj]=[idx(pairs[j][0]),idx(pairs[j][1])];
 if(comp[pj]!==comp[nj])return null;
 const size=new Map<number,number>();for(const c of comp)size.set(c,(size.get(c)??0)+1);
 // Add 1_c 1_c^T/|c| per island: nonsingular, and zero-mean (pseudo-inverse) for balanced currents.
 const system=A.map((row,a)=>row.map((v,b)=>v+(comp[a]===comp[b]?1/size.get(comp[a])!:0)));
 const rhs=Array<number>(m).fill(0);rhs[pj]+=1;rhs[nj]-=1;
 const v=solveDense(system,rhs);
 const scale=Math.max(...A.flat().map(Math.abs),Number.MIN_VALUE);
 const residual=Math.max(...A.map((row,a)=>Math.abs(row.reduce((s,g,b)=>s+g*v[b],0)-rhs[a])));
 if(!(residual<=1e-8*Math.max(1,scale*Math.max(...v.map(Math.abs)))))throw new Error('LOCAL_DC_EVIDENCE: port DC solve residual '+residual);
 return pairs.map(([p,n])=>v[idx(p)]-v[idx(n)]);
}

/** `z[i][j]`: voltage at port i for 1 A at port j (others terminated). The diagonal is the impedance seen at the
 * port with its own termination removed. A null column means port j is open at DC. */
export function portDcImpedanceMatrix(G:readonly (readonly number[])[],galvanic:readonly number[],pairs:readonly (readonly [number,number])[],conductanceS:readonly number[]):(number|null)[][]{
 const z=pairs.map(()=>pairs.map(():number|null=>null));
 pairs.forEach((_,j)=>{const col=terminatedPortResponse(G,galvanic,pairs,conductanceS,j);if(col)col.forEach((v,i)=>{z[i][j]=v;});});
 return z;
}
