// Revision-state contract v1, frozen before running: unchanged defaults preserve
// viewer selections; changed defaults apply, invalid/removed selections disappear,
// and source JSON/state/results are immutable. Authoring advertises one filter
// mechanism while canonical legacy components remain readable.
import assert from 'node:assert/strict';
import { createServer } from 'vite';
const server = await createServer({ configFile:'vite.config.ts', appType:'custom', logLevel:'error', optimizeDeps:{noDiscovery:true,entries:[]}, server:{middlewareMode:true,hmr:false,ws:false} });
try {
 const { rebasePresentationState } = await server.ssrLoadModule('/src/lib/presentation/results.ts');
 const { presentationManifest } = await server.ssrLoadModule('/src/lib/presentation/manifest.ts');
 const options = [{label:'All',value:'$all'},{label:'East',value:'East'},{label:'West',value:'West'}];
 const spec = {root:'doc',elements:{doc:{type:'Document',props:{contract:{version:'fella-ui-1',capabilities:['display'],state:{region:{type:'enum',options,default:'$all',persistence:'viewer'}}}},children:['region']},region:{type:'Select',props:{name:'Region',stateKey:'region',options,defaultValue:'$all'},children:[]}}};
 const original = structuredClone(spec);
 const state = {'control:region':'West'};
 const next = structuredClone(spec);
 next.elements.doc.children.push('view');
 next.elements.doc.props.contract.state.view={type:'boolean',default:false,persistence:'viewer'};
 next.elements.view={type:'Checkbox',props:{label:'Exact values',stateKey:'view',defaultChecked:false},children:[]};
 assert.equal(rebasePresentationState(spec,next,state,[])['control:region'],'West');
 assert.equal(rebasePresentationState(spec,next,state,[])['control:view'],false);
 next.elements.doc.props.contract.state.region.default='East';
 next.elements.region.props.defaultValue='East';
 assert.equal(rebasePresentationState(spec,next,state,[])['control:region'],'East');
 assert.equal(rebasePresentationState(spec,spec,{'control:region':'Unknown'},[])['control:region'],'$all');
 const removed={root:'doc',elements:{doc:{type:'Document',props:{},children:[]}}};
 assert.equal(rebasePresentationState(spec,removed,state,[])['control:region'],undefined);
 assert.deepEqual(spec,original); assert.deepEqual(state,{'control:region':'West'});
 const manifest=presentationManifest();
 assert.ok(manifest.componentNames.includes('ResultFilter'),'legacy renderer remains compatible');
 assert.equal(manifest.authorSchema.$defs.props_ResultFilter,undefined,'new authoring has one filtering mechanism');
 assert.ok(manifest.authorSchema.$defs.props_Select);
 assert.match(manifest.prompt,/Row\.gap is a number/);
 console.log('PASS revision state preservation/default replacement/removal/immutability and single authored filter vocabulary');
} finally { await server.close(); }
