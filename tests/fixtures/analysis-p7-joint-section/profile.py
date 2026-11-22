import json,re,pathlib
out=pathlib.Path('debugging/p7-d1-replay-final');profiles={}
for key,folder in [('joint','1793437157149686200'),('hybrid-shifted','1793439012889795900'),('continuous-hybrid','1793439531599095900')]:
 rows=[];plan=None
 for line in pathlib.Path('debugging/p7-closure',folder,'stdout.txt').read_text().splitlines():
  if line.startswith('P7_MEMORY_PLAN '):plan={k:int(v) for k,v in re.findall(r'(\w+)=(\d+)',line)}
  elif line.startswith('P7_SYMBOLIC ') and plan is not None:plan.update({k:int(v) for k,v in re.findall(r'(\w+)=(\d+)',line)})
  elif line.startswith('P7_STAGE magneticAssemblyAndFactor') and plan is not None:plan['seconds']=float(line.split()[2][:-1]);rows.append(plan);plan=None
 profiles[key]=rows
(out/'profiles.json').write_text(json.dumps(profiles,indent=2));print(json.dumps(profiles))
