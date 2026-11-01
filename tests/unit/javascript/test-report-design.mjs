import assert from 'node:assert/strict';
import { createServer } from 'vite';

// v1 acceptance: presentation edits reuse the producing run and exact results,
// preserve other Report pins, validate bindings before writing, and save with
// optimistic concurrency. No analytical execution or global adoption occurs.
const server = await createServer({configFile:'vite.config.ts',appType:'custom',logLevel:'error',server:{middlewareMode:true,hmr:false,ws:false}});
try {
 const { applyReportDesign } = await server.ssrLoadModule('/src/lib/report-design.ts');
 const spec={root:'doc',elements:{doc:{type:'Document',props:{},children:['chart']},chart:{type:'Chart',props:{resultId:'months',spec:{mark:'line',encoding:{x:{field:'date',type:'temporal',timeUnit:'month'},y:{field:'revenue',type:'quantitative'},color:{field:'year',type:'nominal'}}}},children:[]}}};
 const results=[{id:'months',columns:['date','revenue','year'],rows:[['2024-01-01',10,2024],['2025-01-01',20,2025]],exact_rows:[['2024-01-01','10','2024'],['2025-01-01','20','2025']],row_count:2,complete:true}];
 const saved={artifact:{id:'art',workspace:'/sample',current_revision:1},revision:{revision:1},run:{id:'original-run'},results};
 const document={id:'report',workspace:'fella-app-workspace',version:7,title:'Report',body:'Notes',placements:[{id:'target',artifact_id:'art',revision:1,view_state:{year:2025}},{id:'other',artifact_id:'other',revision:4,view_state:{}}]};
 const proposal={base_version:7,message:'Overlay the years',requires_analysis:false,update:structuredClone(document),presentation_edits:[{placement_id:'target',spec}]};
 const calls=[];
 const bridge={artifactResolveSaved:async()=>saved,artifactRevise:async(...args)=>{calls.push(['revise',...args]);return {revision:{revision:2}};},studioSave:async(...args)=>{calls.push(['save',...args]);return {...args[3],id:'report',workspace:args[0],version:8};}};
 const input=JSON.stringify({document,saved,proposal});
 const next=await applyReportDesign(document,proposal,bridge);
 assert.equal(next.placements[0].revision,2);
 assert.deepEqual(next.placements[1],document.placements[1]);
 assert.equal(calls[0][4].run_id,'original-run');
 assert.equal(calls[0][4].spec.elements.chart.props.spec.encoding.x.timeUnit,'month');
 assert.equal(calls[1][3],7);
 assert.equal(JSON.stringify({document,saved,proposal}),input);
 const bad=structuredClone(proposal);bad.presentation_edits[0].spec.elements.chart.props.resultId='invented';
 calls.length=0;
 await assert.rejects(()=>applyReportDesign(document,bad,bridge),/design could not be applied/);
 assert.deepEqual(calls,[]);
 await assert.rejects(()=>applyReportDesign({...document,version:8},proposal,bridge),/Report changed/);
 console.log('PASS report design v1: overlay uses saved values and run; other pins preserved; invalid bindings and stale versions rejected before writes.');
} finally {await server.close();}
