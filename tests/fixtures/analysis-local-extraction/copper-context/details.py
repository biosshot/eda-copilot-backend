"""Source-ID topology changes and localized corrected-support overlays."""
from pathlib import Path
import json,collections,math
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from shapely.geometry import Polygon
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'debugging/p7-design-checkpoint/copper-context';read=lambda p:json.loads(p.read_text())
changes=[]
for path in sorted(OUT.glob('*-G1-opt.json')):
    name=path.name[:-len('-G1-opt.json')];a=read(OUT/(name+'-G0-opt.json'))['report'];b=read(path)['report']
    def rows(r):
        reg={x['id']:x for x in r['regions']};d=collections.defaultdict(list)
        for c in r['contacts']:
            pair=tuple(sorted((reg[c[x]]['source']['path'],reg[c[x]]['layer']) for x in ['a','b']))
            d[(pair,c['drill_id'])].append({'kind':c['kind'],'reliable':c['reliable'],'sources':[reg[c[x]]['source'] for x in ['a','b']]})
        return d
    aa,bb=rows(a),rows(b)
    for k in sorted(set(aa)|set(bb),key=str):
        if aa[k]!=bb[k]:changes.append({'board':name,'pair':k,'before':aa[k],'after':bb[k]})
assert len(changes)==9 and all(not x['reliable'] for c in changes for x in c['before']+c['after'])
(OUT/'contact-differences.json').write_text(json.dumps(changes,indent=2))

a=read(OUT/'STM32G0_CAN_RS485-G0-opt.json')['report'];b=read(OUT/'STM32G0_CAN_RS485-G1-opt.json')['report'];ids=['88d9932c804b2bb2','ddd0394190d6ae65']
regions=lambda r:{p['source']['source_id']:p for p in r['regions'] if p['source']['source_id'] in ids}
aa,bb=regions(a),regions(b);old=read(ROOT/'tests/fixtures/analysis-local-extraction/copper-partition/stm613-trace.json')['cell']['triangleMm'];origin=old[0]
closest=[min(bb[s]['polygon']['outer'],key=lambda p:math.dist(p,origin)) for s in ids];assert closest[0]==closest[1]
(OUT/'shared-cap-regression.json').write_text(json.dumps({'sourceIds':ids,'sharedNewCapVertexMm':closest[0],'oldShortEdgeMm':math.dist(old[0],old[1]),'newlySharedExactVertex':True},indent=2))
fig,axes=plt.subplots(1,3,figsize=(15,4.7),layout='constrained');w=read(OUT/'postfix-witnesses.json')
for c,color,label in [(old,'#cb442f','old613 degenerate cell'),(w[0]['cell']['triangleMm'],'#126cab','post-fix534 / aspect20')]:
    p=c+[c[0]];axes[0].plot(*zip(*p),color=color,label=label)
axes[0].set_aspect('equal');axes[0].set_title('Source -> native union -> shared-grid CDT');axes[0].legend(fontsize=8);axes[0].set_xlabel('x / mm');axes[0].set_ylabel('y / mm')
for r,col in [(aa,'#cb442f'),(bb,'#126cab')]:
    for sid in ids:
        p=r[sid]['polygon']['outer'];p=p+[p[0]];axes[1].plot([(q[0]-origin[0])*1e6 for q in p],[(q[1]-origin[1])*1e6 for q in p],color=col,lw=1)
axes[1].scatter((closest[0][0]-origin[0])*1e6,(closest[0][1]-origin[1])*1e6,color='#126cab',s=30)
axes[1].set_xlim(-.2,.2);axes[1].set_ylim(-.2,.2);axes[1].set_title('Both G1 sources share exactly one cap vertex');axes[1].set_xlabel('dx / nm');axes[1].set_ylabel('dy / nm')
bridge=read(ROOT/'tests/fixtures/analysis-local-extraction/copper-geometry/construction-613.json')['bridge'];pad=bridge['padPolygonMm'];cx=sum(x[0] for x in pad)/4;cy=sum(x[1] for x in pad)/4
p=bb[ids[1]]['polygon']['outer'];overlap=Polygon(p).intersection(Polygon(pad)).area;assert overlap>1e-8
for p,col,label in [(p,'#126cab','actual G1 straight side'),(pad,'black','authored0.2um pad bridge')]:
    p=p+[p[0]];axes[2].plot([(q[0]-cx)*1e3 for q in p],[(q[1]-cy)*1e3 for q in p],color=col,label=label)
axes[2].set_xlim(-.3,.3);axes[2].set_ylim(-.3,.3);axes[2].set_aspect('equal');axes[2].set_title('True bridge retained: overlap %.3g mm2'%overlap);axes[2].legend(fontsize=8);axes[2].set_xlabel('dx / um');axes[2].set_ylabel('dy / um')
fig.savefig(OUT/'stm-fixed-supports.png',dpi=170)
