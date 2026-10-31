"""D1 saved-input/reduced-evaluation diagnostic plots. No solve."""
import json,pathlib,sys,numpy as np,matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.patches import Polygon
read=lambda p:json.loads(pathlib.Path(p).read_text(encoding='utf-8'))
out=pathlib.Path(sys.argv[1]);planpath=pathlib.Path(sys.argv[2]);board=read('tests/fixtures/analysis-p7-joint-section/slot.json');plan=read(planpath/'plan-0.125.json');joint=read(out/'joint-fine.json');hybrid=read(out/'hybrid.json');comparison=read(out/'comparison.json');f=np.array(joint['frequenciesHz']);z=board['copper']['fixed']['zones'][0]['filledPolygons'][0]['polygons'][0];D=np.array([[1,0],[0,1],[-1,0],[0,-1]])
fig,ax=plt.subplots(2,2,figsize=(13,9));a=ax[0,0];a.add_patch(Polygon([[p['x'],p['y']]for p in z['outer']],facecolor='#6699bb',alpha=.5))
for h in z['holes']:a.add_patch(Polygon([[p['x'],p['y']]for p in h],facecolor='white',edgecolor='black'))
for t in board['copper']['fixed']['tracks']:a.plot([p['x']for p in t['points']],[p['y']for p in t['points']],color='red',lw=8)
for b in plan['blocks']:a.add_patch(Polygon(b['ownership'][0]['domainPolygonMm'],fill=False,edgecolor='#008800'if b['kind']=='section'else'#bb5500',linestyle='--'))
for p in board['pads']:
 x,y=p['at']['x'],p['at']['y'];a.plot(x,y,'ko'if p['layers']==['BOTTOM']else'mo');off=-.09 if p['layers']==['BOTTOM']else .04;a.annotate(p['component']+'.'+p['number'],(x,y+off),fontsize=8)
a.autoscale_view();a.set(xlabel='original CAD x mm',ylabel='original CAD y mm',title='Fixed source: signals/red, return/blue, hole/white; green section cuts');a.set_ylim(.4,-.4)
for r,label,style in [(joint,'joint fine','o-'),(hybrid,'section+local','x--')]:
 for i,j in [(2,0),(3,0),(1,0)]:ax[0,1].loglog(f,[abs(complex(m[i][j]['real'],m[i][j]['imaginary']))for m in r['parameters']],style,label=f'{label} |S{i}{j}|')
ax[0,1].set(xlabel='Hz',title='Actual complex multiport magnitude; tiny S alone is insufficient');ax[0,1].legend(fontsize=8)
for r,label,style in [(joint,'joint fine','o-'),(hybrid,'section+local','x--')]:
 gram=np.array([g['magneticJPerA2']for g in r['loadedEnergyGram']]);e=np.einsum('ai,fab,bj->fij',D,gram,D)
 for i,j in [(0,0),(1,1),(0,1)]:ax[1,0].semilogx(f,e[:,i,j]*1e9,style,label=f'{label} Wm({i},{j})')
ax[1,0].set(xlabel='Hz',ylabel='nJ for balanced RMS1A Norton drives',title='Intrinsic mean magnetic energy, actual matched loads');ax[1,0].legend(fontsize=8)
labels=['mesh','domain','representations','cuts'];x=np.arange(4);cs={**comparison['comparisons'],'cuts':read(out/'cut-shift.json')}
for k,dx in [('wave',-.22),('magnetic',0),('energy',.22)]:ax[1,1].bar(x+dx,[100*cs[l][k]for l in labels],width=.22,label=k)
ax[1,1].set(xticks=x,xticklabels=labels,ylabel='%',title='NO-GO: mesh/domain/cuts limit2%; representations8%');ax[1,1].axhline(2,color='k',ls='--');ax[1,1].axhline(8,color='r',ls=':');ax[1,1].legend();fig.suptitle('D1 diagnostic: joint magnetic mesh and hybrid/cut acceptance FAIL',fontsize=15);fig.tight_layout(rect=[0,0,1,.96]);fig.savefig(out/'d1-diagnostic.png',dpi=150)
mag=read(pathlib.Path(comparison['sourceJoint'])/'joint-fine-magnetic.json');curr=mag['returnCurrentDensity'];xy=np.array([c['atMm']for c in curr]);J=np.array([c['realAPerM2']for c in curr]);norm=np.linalg.norm(J[:,:2],axis=1);fig2,a=plt.subplots(figsize=(12,4));a.add_patch(Polygon([[p['x'],p['y']]for p in z['outer']],facecolor='#ddeeff',edgecolor='black'))
for h in z['holes']:a.add_patch(Polygon([[p['x'],p['y']]for p in h],facecolor='white',edgecolor='black'))
keep=norm>norm.max()*1e-4;q=a.quiver(xy[keep,0],xy[keep,1],J[keep,0]/norm[keep],J[keep,1]/norm[keep],np.log10(norm[keep]),angles='xy',scale_units='xy',scale=16,cmap='viridis');a.set_aspect('equal');a.set(xlim=(-.2,2.2),ylim=(.32,-.32),xlabel='original CAD x mm',ylabel='original CAD y mm',title='Native1kHz volume-candidate recovery: NOT an equilibrated flux certificate');fig2.colorbar(q,ax=a,label='log10 |Re Jxy| (A/m^2)');fig2.text(.5,.02,'Arrow length normalized; values below1e-4 peak omitted. Recovered flux at hole:0.416A versus electrode balance0.9996A.',ha='center',fontsize=9);fig2.tight_layout(rect=[0,.05,1,1]);fig2.savefig(out/'return-current.png',dpi=160)
profiles=read(out/'profiles.json');rows=[['M unknowns','39221','47417 max active'],['M input nnz','2797144','3400208 max'],['M factor values','19918956','20244879 max'],['Factor values bytes','159351648','161959032 max'],['Cold diagnostic wall s','49.8235','149.0144'],['Same-input warm wall s','see continuous control','1.1624, disk'],['Physical V/I coordinates','82 /558','62 /864 max local'],['Reduced total states','127','329']];fig3,a=plt.subplots(figsize=(11,5));a.axis('off');table=a.table(cellText=rows,colLabels=['Metric','Joint fine','Hybrid shifted'],loc='center',cellLoc='left');table.auto_set_font_size(False);table.set_fontsize(11);table.scale(1,1.7);a.set_title('Diagnostic resource profile: equal physical accuracy NOT established\nOne heavy block at a time; no whole-board speed or memory claim');fig3.tight_layout();fig3.savefig(out/'resources.png',dpi=150)
print('rendered D1 response, magnetic energy, current recovery and resources')
