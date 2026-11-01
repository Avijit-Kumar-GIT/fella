import { spawnSync } from 'node:child_process';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
const target='wasm32-unknown-unknown';
function run(command,args) {const result=spawnSync(command,args,{stdio:'inherit',windowsHide:true});if(result.error)throw result.error;if(result.status!==0)process.exit(result.status ?? 1);}
const installed=spawnSync('rustup',['target','list','--installed'],{encoding:'utf8',windowsHide:true});
if(installed.status!==0)throw new Error('Rustup is required to build the shared UI contract.');
if(!installed.stdout.split(/\s+/).includes(target))run('rustup',['target','add',target]);
run('cargo',['build','--manifest-path','backend/Cargo.toml','--locked','--profile','release-min','--target',target,'--package','fella-presentation-contract','--lib']);
const bytes=await readFile(`backend/target/${target}/release-min/fella_presentation_contract.wasm`);
const output='src/lib/presentation/generated/contract.wasm';await mkdir('src/lib/presentation/generated',{recursive:true});
const prior=await readFile(output).catch(()=>null);if(!prior || !prior.equals(bytes))await writeFile(output,bytes);
console.log(`Shared UI acceptance module: ${bytes.length} bytes`);
