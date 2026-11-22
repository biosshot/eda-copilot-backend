"""Reproduce authored input, no solver/imported production code."""
import numpy as np
import sys
from pathlib import Path
def plates(n):
 lines=['0 Two finite 1mm square 35um copper plates in vacuum, z0/1mm; SI metres']
 for index,z in enumerate([0.,.001]):
  vertices=np.array([[x,y,h] for h in [z,z+.000035] for x,y in [(-.0005,-.0005),(.0005,-.0005),(.0005,.0005),(-.0005,.0005)]])
  for face in [[0,3,2,1],[4,5,6,7],[0,1,5,4],[1,2,6,5],[2,3,7,6],[3,0,4,7]]:
   a,b,c,d=vertices[face]
   for i in range(n):
    for j in range(n):
     corners=[a+(b-a)*u+(d-a)*v for u,v in [(i/n,j/n),((i+1)/n,j/n),((i+1)/n,(j+1)/n),(i/n,(j+1)/n)]]
     lines.append('Q plate'+str(index)+' '+' '.join(format(t,'.17g') for p in corners for t in p))
 return '\n'.join(lines)+'\n'
def sphere(level):
 vertices=np.array([[1,0,0],[-1,0,0],[0,1,0],[0,-1,0],[0,0,1],[0,0,-1]],dtype=float)
 triangles=[]
 for x in [0,1]:
  for y in [2,3]:
   for z in [4,5]:
    a,b,c=vertices[[x,y,z]]
    if np.dot(np.cross(b-a,c-a),a)<0:b,c=c,b
    triangles.append([a,b,c])
 for _ in range(level):
  refined=[]
  for a,b,c in triangles:
   midpoint=lambda x,y:(x+y)/np.linalg.norm(x+y)
   ab,bc,ca=midpoint(a,b),midpoint(b,c),midpoint(c,a)
   refined.extend([[a,ab,ca],[ab,b,bc],[ca,bc,c],[ab,bc,ca]])
  triangles=refined
 return '0 Inscribed octahedral sphere radius1mm in vacuum; SI metres\n'+'\n'.join('T sphere '+' '.join(format(t*.001,'.17g') for p in tri for t in p) for tri in triangles)+'\n'

case,refinement,destination=sys.argv[1:]
Path(destination).write_text(plates(int(refinement)) if case=="plates" else sphere(int(refinement)),encoding="ascii")
