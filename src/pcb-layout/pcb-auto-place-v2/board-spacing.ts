import type { Box, Layer, PcbComponent, PlacementInput, TargetRef } from '#types/pcb/layout-model.ts';
import { componentBodyBox, componentCollisionBoxes, boardHoleKeepoutRadius, pointInBoard, boxGap } from '../pcb-auto-place/geometry.ts';
import type { PlacementPrimitive } from './primitives.ts';

export function boardSpacingExemptPairs(input: PlacementInput, roots: PlacementPrimitive[]): Array<[string,string]> {
    const members = (target: TargetRef | undefined): string[] => !target ? [] : target.type==='block'
        ? input.blocks.find(b=>b.name===target.block_name)?.component_designators ?? []
        : 'designator' in target ? [target.designator] : [];
    const links: Array<[string[],string[]]> = input.blocks.filter(b=>b.attachTo).map(b=>[
        b.component_designators, input.blocks.find(p=>p.name===b.attachTo)?.component_designators ?? []]);
    for(const b of input.blocks) if(b.anchor)links.push([b.component_designators,members(b.anchor)]);
    for(const h of input.hints) if((h.relation==='near'||h.relation==='critical_pair') &&
        (('hard' in h && h.hard)||h.priority==='high'||h.priority==='critical')) links.push([members(h.source),members(h.target)]);
    const result:Array<[string,string]>=[];
    const owns=(p:PlacementPrimitive,refs:string[])=>p.placements.some(q=>refs.includes(q.designator));
    for(let i=0;i<roots.length;i++)for(let j=i+1;j<roots.length;j++)
        if(links.some(([a,b])=>owns(roots[i],a)&&owns(roots[j],b)||owns(roots[i],b)&&owns(roots[j],a))) result.push([roots[i].id,roots[j].id]);
    return result;
}

/** Bounded comfort margin. Density uses the complete inventory, never a partial
 * beam state; changing order or translating a block cannot change this budget. */
export function boardSpacingPolicy(input: PlacementInput) {
    const {width,height} = input.board.outline;
    const area=(b:Box)=>(b.right-b.left+input.board.clearances.component)*(b.bottom-b.top+input.board.clearances.component);
    const occupied = input.components.reduce((sum,c)=> {
        const pose={designator:c.designator,x:0,y:0,rotate:c.pcb.fixedPlacement?.rotate ?? 0,
            layer:c.pcb.fixedPlacement?.layer ?? c.pcb.allowedLayers[0] ?? 'top',score:0};
        return sum+input.board.allowedLayers.reduce((sideSum,layer)=>sideSum+(layer===pose.layer
            ? area(componentBodyBox(c,pose))
            : componentCollisionBoxes(c,pose,layer).reduce((boxSum,box)=>boxSum+area(box),0)),0);
    },0);
    let usable=0;
    const samples=64;
    for(const layer of input.board.allowedLayers) for(let x=0;x<samples;x++) for(let y=0;y<samples;y++) {
        const p={x:-width/2+(x+.5)*width/samples,y:-height/2+(y+.5)*height/samples};
        if(!pointInBoard(input.board,p,input.board.clearances.edge)) continue;
        if((input.boardHoles ?? []).some(h=>Math.hypot(p.x-h.x,p.y-h.y)<boardHoleKeepoutRadius(h))) continue;
        if((input.constraintRegions ?? []).some(r=>r.layers.includes(layer)&&p.x>=r.box.left&&p.x<=r.box.right&&p.y>=r.box.top&&p.y<=r.box.bottom)) continue;
        usable++;
    }
    const density=occupied/Math.max(1,usable*width*height/(samples*samples));
    const freedom=Math.max(0,Math.min(1,(.60-density)/.35));
    return {gap:3*freedom, compactnessScale:1-.9*freedom, density};
}

/** Same pair penalty as the native board objective. No reward beyond the gap. */
export function boardSpacingPenalty(input: PlacementInput, roots: PlacementPrimitive[], gap=boardSpacingPolicy(input).gap) {
    if(gap<=0)return 0;
    let score=0;
    const exempt=boardSpacingExemptPairs(input,roots);
    const components=new Map(input.components.map(component=>[component.designator,component]));
    const envelopes=new Map(roots.map(root=>[root.id,{
        top:primitiveLayerBox(components,root,'top'),bottom:primitiveLayerBox(components,root,'bottom'),
    }]));
    for(let i=0;i<roots.length;i++)for(let j=i+1;j<roots.length;j++) {
        const a=roots[i],b=roots[j];
        if(exempt.some(([x,y])=>x===a.id&&y===b.id||x===b.id&&y===a.id))continue;
        if(a.locked&&b.locked)continue;
        const distances=(['top','bottom'] as Layer[]).flatMap(layer=>{
            const x=envelopes.get(a.id)?.[layer],y=envelopes.get(b.id)?.[layer];
            return x&&y?[boxGap(x,y)]:[];
        });
        if(!distances.length)continue;
        const deficit=Math.max(0,input.board.clearances.component+gap-Math.min(...distances));
        score+=18*deficit*deficit;
    }
    return score;
}

function primitiveLayerBox(components: Map<string,PcbComponent>, primitive: PlacementPrimitive, layer: Layer): Box | undefined {
    const boxes=primitive.placements.flatMap(placement=>{
        const component=components.get(placement.designator);
        return component?componentCollisionBoxes(component,placement,layer):[];
    });
    if(!boxes.length)return undefined;
    return {left:Math.min(...boxes.map(box=>box.left)),right:Math.max(...boxes.map(box=>box.right)),
        top:Math.min(...boxes.map(box=>box.top)),bottom:Math.max(...boxes.map(box=>box.bottom))};
}
