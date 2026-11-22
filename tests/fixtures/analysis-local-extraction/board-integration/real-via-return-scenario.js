// Direct existing DSL, supplied with STM32G0_CAN_RS485.json as `board`.
// Pending layered execution admission; no authored equivalent circuit/cache data.
const a = analysis({board});
const signal = viaId('da3574692d072a0d');
const returned = viaId('aa8ff23739620a07');
const input = a.port({positive:signal,negative:returned,layer:'TOP'});
const output = a.port({positive:signal,negative:returned,layer:'BOTTOM'});
const scope = region({layers:['TOP','BOTTOM'],polygon:{outer:[
  {x:22.7233,y:3.7219},{x:23.9733,y:3.7219},
  {x:23.9733,y:4.1719},{x:22.7233,y:4.1719},
]}});
const channel = a.channel({from:input,to:output,scope:[scope]});
const layers = stackup({boardThicknessMm:1.6,viaPlatingThicknessUm:25,
  platingMaterial:'copper',diameterConvention:'finished_hole',layers:[
    {kind:'copper',name:'TOP',thicknessMm:.035,material:'copper'},
    {kind:'dielectric',name:'authored-FR4',thicknessMm:1.53,material:'fr4'},
    {kind:'copper',name:'BOTTOM',thicknessMm:.035,material:'copper'},
  ]});
const extracted = await a.impedance().target(channel).method('layered')
  .stackup(layers).materials({copper:{conductivitySM:58e6},fr4:{relativePermittivity:4.2}})
  .frequency({startHz:1e3,stopHz:1e5,points:17,scale:'log'})
  .budget({maxSeconds:300,maxMemoryMiB:2048}).run();
a.plot({title:'Real signal/return via extraction and validity',kind:'table',data:extracted.rlgc});
const z = await a.networkParameters().network(extracted.network)
  .frequency({startHz:1e3,stopHz:1e5,points:17,scale:'log'}).representation('Z')
  .budget({maxSeconds:300,maxMemoryMiB:2048}).run();
a.plot({title:'Via/return input impedance',data:z.parameter({out:'input',in:'input'})});
a.plot({title:'Via/return transfer impedance',data:z.parameter({out:'output',in:'input'})});
