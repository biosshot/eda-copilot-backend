"""Solver-free direct-drive/full-state audit. Thresholds fixed before replay.
No difference of huge unit-port Z is used as a through impedance.
"""
from pathlib import Path
import json,sys
import numpy as np
TOL=.05;V_FLOOR=1e-6;LOSS_FLOOR=1e-18;WEAK_REL=1e-12
folder=Path(sys.argv[1]);result=json.loads((folder/'stdout.json').read_text());candidate=result['candidate'];basis=np.array(candidate['currentBasis']);nv=len(candidate['model']['potentialNodes']);ni=len(candidate['model']['currentDofs']);n=nv+ni;resistance=np.array(result['cellResistanceOhm'])
points=np.array(result['nodesM']);cells=np.array(result['copperCells']);z=points[cells].mean(axis=1)[:,2];stack=result['request']['stack'];upper=stack[0]['thicknessM'];lower=sum(l['thicknessM'] for l in stack[:-1]);regions={'top':z<upper,'barrel':(z>=upper)&(z<=lower),'bottom':z>lower}
expected=[1000.0,1333.5214321633239,1778.2794100389228,2371.373705661655,3162.2776601683795,4216.965034285822,5623.413251903491,7498.942093324558,10000.0,13335.21432163324,17782.79410038923,23713.737056616552,31622.776601683792,42169.65034285823,56234.13251903491,74989.42093324558,100000.0]
rows=[]
for k,row in enumerate(candidate['fullChecks']):
 assert row['frequencyHz']==expected[k//3] and row['drive']==[[1,0],[0,1],[1,-1]][k%3]
 full_v=np.array([complex(*v) for v in row['fullPortVoltages']]);reduced_v=np.array(row['reducedPortReal'])+1j*np.array(row['reducedPortImaginary']);v_error=float(np.max(abs(full_v-reduced_v)/np.maximum(abs(full_v),V_FLOOR)))
 state=np.array(row['reducedState']);real=basis.T@state[nv:n];imag=basis.T@state[n+nv:];full_real=np.array(row['fullCurrentReal']);full_imag=np.array(row['fullCurrentImaginary']);scale=np.max(np.hypot(full_real,full_imag));current_error=float(np.max(np.hypot(real-full_real,imag-full_imag)/np.maximum(np.hypot(full_real,full_imag),WEAK_REL*scale)))
 losses=lambda r,i:resistance*(r.reshape(-1,3)**2+i.reshape(-1,3)**2).sum(axis=1)
 full_loss=losses(full_real,full_imag);reduced_loss=losses(real,imag);total_error=abs(full_loss.sum()-reduced_loss.sum())/max(full_loss.sum(),LOSS_FLOOR);cell_error=np.max(abs(full_loss-reduced_loss)/np.maximum(full_loss,WEAK_REL*full_loss.max()))
 regional={name:float(abs(full_loss[mask].sum()-reduced_loss[mask].sum())/max(full_loss[mask].sum(),LOSS_FLOOR)) for name,mask in regions.items()}
 rows.append(dict(frequencyHz=row['frequencyHz'],drive=row['drive'],voltageError=v_error,currentError=current_error,totalLossError=float(total_error),cellLossError=float(cell_error),regionalLossError=regional))
maxima={key:max((r[key] for r in rows),default=0) for key in ['voltageError','currentError','totalLossError','cellLossError']};maxima['regionalLossError']=max((v for r in rows for v in r['regionalLossError'].values()),default=0)
summary=dict(status='complete' if len(rows)==51 and candidate['verificationStatus']=='complete' else 'incomplete',points=len(rows),missing=51-len(rows),voltageFloorV=V_FLOOR,lossFloorW=LOSS_FLOOR,weakCoordinateAndCellRelativeFloor=WEAK_REL,tolerance=TOL,maxima=maxima,completedPointsPass=all(np.isfinite(v) and v<=TOL for v in maxima.values()),rows=rows,verificationFailure=candidate['verificationFailure'])
(folder/'integration-audit.json').write_text(json.dumps(summary,indent=2));print(json.dumps({k:v for k,v in summary.items() if k!='rows'},indent=2));sys.exit(0 if summary['status']=='complete' and summary['completedPointsPass'] else 1)
