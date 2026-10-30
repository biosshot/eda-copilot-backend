import json
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
b=json.load(open('tests/pcb-analyze/STM32G0_CAN_RS485/STM32G0_CAN_RS485.json',encoding='utf-8'))
f,ax=plt.subplots(figsize=(11,6))
for t in b['copper']['fixed']['tracks']:
 if t.get('net')=='RS485_B':
  p=t['points'];ax.plot([v['x'] for v in p],[v['y'] for v in p],color='red' if t['layer']=='TOP' else 'blue',lw=2)
for p in b['pads']:
 if p.get('net')=='RS485_B' or (p.get('component'),p['number']) in [('U5','5'),('J5','3')]:
  ax.scatter(p['at']['x'],p['at']['y'],color='black',s=40);ax.annotate(str(p.get('component'))+'.'+p['number'],(p['at']['x'],p['at']['y']),xytext=(5,8),textcoords='offset points')
for p in b['copper']['fixed']['vias']:
 if p.get('net')=='RS485_B':ax.scatter(p['at']['x'],p['at']['y'],facecolors='none',edgecolors='green',s=100)
ax.set(title='Actual STM32 RS485_B remote candidate: U5.7 to J5.2\nRed TOP / blue BOTTOM / green vias; branch copper retained. Planning only.',xlabel='x (mm)',ylabel='y (mm)');ax.axis('equal');ax.grid(alpha=.2)
f.text(.05,.01,'No section/local composition, field solution, common-band response or 5% accuracy certificate.',fontsize=10)
f.tight_layout(rect=[0,.04,1,1]);path=max(Path('debugging/p7-channel-plan').glob('remote-plan-*'),key=lambda p:p.stat().st_mtime)/'remote-copper.png';f.savefig(path,dpi=130);print(path.resolve())
