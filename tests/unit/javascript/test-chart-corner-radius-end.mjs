// Frozen criteria: numeric value-end radii 0, 4 and 1600 are accepted by the
// native and browser contract; negative, oversized, expression and unknown
// properties remain rejected. An accepted rounded bar reaches Vega and renders.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createServer } from 'vite';
const server = await createServer({configFile:'vite.config.ts',appType:'custom',logLevel:'error',optimizeDeps:{noDiscovery:true,entries:[]},server:{middlewareMode:true,hmr:false,ws:false}});
function nativeAccept(input) {
 return new Promise((resolve,reject)=>{
  const child=spawn('backend/target/debug/examples/contract_accept'); let output='', error='';
  const timer=setTimeout(()=>{child.kill();reject(new Error('Native validator timed out'));},10000);
  child.stdout.on('data',chunk=>{output+=chunk;}); child.stderr.on('data',chunk=>{error+=chunk;});
  child.on('error',reject); child.on('close',code=>{clearTimeout(timer);if(code!==0)reject(new Error(error));else resolve(JSON.parse(output));});
  child.stdin.end(JSON.stringify(input)+'\n');
 });
}
try {
 const {acceptContract} = await server.ssrLoadModule('/src/lib/presentation/contract.ts');
 const {renderBoundChart} = await server.ssrLoadModule('/src/lib/presentation/vega-runtime.ts');
 const result={id:'sales',columns:['category','value'],rows:[['A',10],['B',-4]],exact_rows:[['A','10'],['B','-4']],row_count:2,complete:true};
 const chart=radius=>({mark:{type:'bar',cornerRadiusEnd:radius},encoding:{x:{field:'category',type:'nominal'},y:{field:'value',type:'quantitative'}}});
 const document=spec=>({root:'doc',elements:{doc:{type:'Document',props:{},children:['chart']},chart:{type:'Chart',props:{resultId:'sales',spec},children:[]}}});
 for(const [label,spec,expected] of [
  ['zero',chart(0),true],['rounded',chart(4),true],['maximum',chart(1600),true],
  ['negative',chart(-1),false],['oversized',chart(1601),false],['expression',chart({expr:'1+1'}),false],
  ['unknown mark key',{...chart(4),mark:{type:'bar',cornerRadiusEnd:4,markKeys:'value'}},false]
 ]) {
  const input={spec:document(spec),results:[result]};
  const wasm=acceptContract(input.spec,input.results);
  const parsed=await nativeAccept(input);
  assert.equal(wasm.valid,expected,`${label}: browser`); assert.equal(parsed.valid,expected,`${label}: native`);
  assert.deepEqual(parsed,wasm,`${label}: native/browser parity`);
 }
 const colors={text:'#dddddd',grid:'#333333',accent:'#008080'};
 const plain=await renderBoundChart(null,chart(0),result,[0,1],colors);
 const rounded=await renderBoundChart(null,chart(4),result,[0,1],colors);
 const plainSvg=await plain.toSVG(), roundedSvg=await rounded.toSVG();
 assert.match(roundedSvg,/<svg/); assert.notEqual(roundedSvg,plainSvg,'rounded bars must affect rendered output');
 plain.finalize(); rounded.finalize();
 console.log('PASS: native/browser mark validation parity, bounds, unsafe keys, and actual Vega rounded-bar rendering');
} finally { await server.close(); }
