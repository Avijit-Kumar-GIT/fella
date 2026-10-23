// Focused desktop design review. Run after pnpm build with the prepared Electron engine.
// Uses a disposable profile and saved data; never calls a model or edits user work.
import { mkdtemp, mkdir, writeFile, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { launch, invoke, chooseWorkspace, seedRun, createArtifact, createDocument } from '../tests/e2e/generative-workspace-harness.mjs';
const profile = await mkdtemp(join(tmpdir(), 'fella-reference-redesign-'));
const workspace = join(profile,'Fella Analyst Sample Workspace');
const captures = join(profile,'screenshots');
await mkdir(workspace); await mkdir(captures);
const rows = [['Camp Kitchen',7855.55,9819.26,1963.71,25],['Apparel',7847.87,9294.68,1446.81,18.44],['Footwear',8418.30,9432.72,1014.42,12.05],['Climbing',9740.22,8946.28,-793.94,-8.15],['Packs',13394.63,9354.84,-4039.79,-30.16]];
const columns=['category','q3_revenue','q4_revenue','change','percent'];
await writeFile(join(workspace,'sales.csv'),[columns.join(','),...rows.map(row=>row.join(','))].join('\n'));
for(const [path,body] of [['commerce/orders.csv','order_id,period,total\n1,Q3,10\n2,Q4,20\n'],['catalog/products.csv','sku,category\nP1,Packs\nP2,Camp Kitchen\n'],['finance/refunds.csv','order_id,amount\n1,2\n2,3\n'],['finance/policy.md','# Finance metric policy\nUse net merchandise revenue, exclude cancelled orders, tax and shipping.\n'],['docs/README.md','# Analyst sample workspace\nA deterministic local dataset for reviewing the desktop design.\n']]) { await mkdir(join(workspace,path,'..'),{recursive:true}); await writeFile(join(workspace,path),body); }
let app, page;
const q='Which product categories saw the largest increases or decreases in revenue between Q3 and Q4 2025?';
const text='Between Q3 and Q4 2025, the largest changes in net merchandise revenue by product category were:\n\n**Largest decrease — Packs.** Down $4,039.79 (−30.16%), from $13,394.63 to $9,354.84.\n\n**Largest increase — Camp Kitchen.** Up $1,963.71 (+25.00%), from $7,855.55 to $9,819.26.';
const conversationId='design-reference-chat';
const issues=[]; const measurements=[];
function check(condition,message) { if (!condition) issues.push(message); }
async function capture(name){ await page.evaluate(()=>document.fonts.ready); await page.screenshot({path:join(captures,`${name}.png`)}); }
async function rect(selector){ return page.locator(selector).first().evaluate(element=>{const r=element.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,cx:r.x+r.width/2};}); }
async function noOverflow(name){ const x=await page.evaluate(()=>({scroll:document.documentElement.scrollWidth,width:document.documentElement.clientWidth}));check(x.scroll<=x.width+1,`${name}: document horizontal overflow ${x.scroll}/${x.width}`); }
try {
 ({app,page}=await launch(profile));
 page.on('pageerror',error=>issues.push(`Browser error: ${error.message}`));
 await invoke(page,'archive_conversation',{id:'design-setup-chat',body:JSON.stringify({id:'design-setup-chat',title:'Workspace setup',workspace,saved_at_ms:Date.now(),messages:[{id:'setup-user',role:'user',text:'Open the local design fixture workspace.',ts:1}]})});
 await page.evaluate(()=>localStorage.setItem('fella:appearance','dark'));
 await page.reload(); await chooseWorkspace(page,'Fella Analyst Sample Workspace');
 await invoke(page,'archive_conversation',{id:conversationId,body:JSON.stringify({id:conversationId,title:'Revenue review',workspace,saved_at_ms:Date.now(),messages:[{id:'design-user',role:'user',text:q,ts:1},{id:'design-answer',role:'assistant',text,ts:2}]})});
 const desktop={app,page,profile,workspace};
 const runSeed=await seedRun(desktop,{id:'design-saved-run',conversationId,question:q});
 const run=JSON.parse(await readFile(runSeed.path,'utf8'));
 run.result.text=text;run.result.evidence[0].columns=columns;run.result.evidence[0].rows=rows;run.result.evidence[0].row_count=rows.length;run.result.evidence[0].sql='SELECT category, q3_revenue, q4_revenue, change, percent FROM sales ORDER BY change DESC';run.result.evidence[0].result_summary='Saved category comparison from the local fixture.';
 run.trace.mode='workspace_ask';run.trace.model='gemma4:31b';run.trace.elapsed_ms=9600;run.trace.steps=[{id:'inspect',operation:'inspect_schema',duration_ms:84,success:true,summary:'Reviewed the sales columns and the two quarter definitions.',sources:['sales.csv']},{id:'sales-result',operation:'run_sql',duration_ms:126,success:true,summary:'Compared net merchandise revenue across five categories.',sources:['sales.csv']}];
 await writeFile(runSeed.path,JSON.stringify(run));
 const formats={category:{label:'Category'},q3_revenue:{label:'Q3',format:'currency',unit:'USD'},q4_revenue:{label:'Q4',format:'currency',unit:'USD'},change:{label:'Change',format:'currency',unit:'USD'},percent:{label:'%',format:'number',decimals:2}};
 const spec={root:'doc',elements:{doc:{type:'Document',props:{},children:['intro','table','note']},intro:{type:'Text',props:{content:text},children:[]},table:{type:'Table',props:{resultId:'sales-result',columnFormats:formats},children:[]},note:{type:'Text',props:{content:'Revenue is net merchandise revenue, excluding cancelled orders, per the current finance metric policy.'},children:[]}}};
 const artifact=await createArtifact(desktop,runSeed,'Category revenue changes',spec);
 const output=await invoke(page,'artifact_resolve',{workspaceId:workspace,artifactId:artifact.id});
 const inv=await invoke(page,'investigation_ensure',{workspaceId:workspace,conversationId,initialRequest:q,sourceMessageId:'design-user'});
 await invoke(page,'investigation_rename',{investigationId:inv.id,title:'Revenue review'});
 await invoke(page,'investigation_update_brief',{investigationId:inv.id,brief:{goal:q,audience:'Finance and merchandising',desired_output:'A quarterly revenue comparison with the largest category movements.',constraints:['Use order date in UTC to define quarters.','Exclude cancelled orders, tax and shipping.']}});
 await invoke(page,'investigation_update_questions',{investigationId:inv.id,questions:[{id:'q_design_main',text:q},{id:'q_design_volume',parent_id:'q_design_main',text:'How much of the category movement came from order volume?'},{id:'q_design_returns',parent_id:'q_design_main',text:'Did posted refunds or discounts change between quarters?'},{id:'q_design_products',parent_id:'q_design_main',text:'Which products contributed most to the Packs decline?'}]});
 await invoke(page,'investigation_link_analysis_turns',{investigationId:inv.id,analysisTurnIds:[run.id]});
 await invoke(page,'investigation_set_analysis_turn_questions',{investigationId:inv.id,analysisTurnId:run.id,questionIds:['q_design_main']});
 for(const claim of ['Packs fell by $4,039.79, or 30.16%, between Q3 and Q4 2025.','Camp Kitchen increased by $1,963.71, the largest gain among the five categories.']) await invoke(page,'investigation_create_finding',{investigationId:inv.id,finding:{question_id:'q_design_main',analysis_turn_id:run.id,evidence_ids:['sales-result'],claim,caveats:[],assumptions:[]}});
 const doc=await createDocument(desktop,[artifact],{title:'Quarterly revenue review',body:'## Summary\n\nPacks had the largest decline, while Camp Kitchen showed the largest increase.\n\n## Method and caveats\n\nThis report preserves the saved category comparison. Figures are from a deterministic local design fixture.'});
 await invoke(page,'archive_conversation',{id:conversationId,body:JSON.stringify({id:conversationId,title:'Revenue review',workspace,saved_at_ms:Date.now(),messages:[{id:'design-user',role:'user',text:q,ts:1},{id:'design-answer',role:'assistant',text,ts:2,run_duration_ms:9600,answer:{...run.result,turn_id:run.id,trace:run.trace,workspace:{path:workspace,revision:run.workspace_revision}},artifact_id:artifact.id,artifact_revision:output.revision.revision,presentation_snapshot:{workspace,turn_id:run.id,spec:output.revision.spec,results:output.results}}]})});
 await invoke(page,'archive_conversation',{id:'design-question-chat',body:JSON.stringify({id:'design-question-chat',title:'What kinds of data are available?',workspace,saved_at_ms:Date.now()-1000,messages:[{id:'other-user',role:'user',text:'What kinds of data are available?',ts:1},{id:'other-answer',role:'assistant',text:'Commerce, catalog, and finance sources are available.',ts:2}]})});
 await page.reload(); await chooseWorkspace(page,'Fella Analyst Sample Workspace');
 await page.getByRole('button',{name:'Open conversation: Revenue review',exact:true}).click();
 await page.locator('.investigation-page').waitFor(); await page.locator('.analytical-answer table').waitFor();
 await page.evaluate(()=>document.fonts.ready);
 const loadedPlan=await invoke(page,'investigation_load',{investigationId:inv.id});check(loadedPlan.questions.length===4,'Fresh fixture did not retain its four seeded questions');
 const sidebar=await rect('.sidebar'),primary=await rect('.workspace-conversation-pane'),chip=await rect('.titlebar-workspace'),pane=await rect('.workspace-tools-pane'),mark=await rect('.investigation-mark');
 measurements.push({size:'1440x960',sidebar,primary,chip,pane,mark});
 check(Math.abs(sidebar.width-268)<1,'Sidebar is not 268px');check(Math.abs(chip.cx-primary.cx)<1,'Workspace chip is not centered in Chat');check(Math.abs(pane.width-380)<1,'Investigation pane is not 380px');check(Math.abs(mark.cx-pane.cx)<1,'Investigation mark is not centered');
 const fonts=await page.evaluate(()=>({body:getComputedStyle(document.body).fontFamily,data:getComputedStyle(document.querySelector('.analytical-answer table')).fontFamily}));check(fonts.body.startsWith('Inter'),'Inter is not active');check(fonts.data.includes('JetBrains'),'Data font is not JetBrains Mono');measurements.push({fonts});
 await noOverflow('wide Chat');await capture('01-chat-sources-wide');
 await page.getByRole('tab',{name:/^Brief/}).click();check((await page.locator('.brief-objective').innerText()).includes(q),'Brief does not show the saved Investigation objective');await capture('02-brief');
 await page.getByRole('tab',{name:/^Questions/}).click();
 const field=page.getByRole('textbox',{name:'New investigation question',exact:true});await field.fill('Which sources need reconciliation before a final decision?');await page.getByRole('button',{name:'Add question',exact:true}).click();
 await page.locator('.question-row-summary').filter({hasText:'Which sources need reconciliation before a final decision?'}).waitFor();
 const tabsBefore=await rect('.investigation-nav');
 await page.locator('.investigation-section-scroll').evaluate(element=>{element.scrollTop=element.scrollHeight;});
 const tabsAfter=await rect('.investigation-nav');check(Math.abs(tabsBefore.y-tabsAfter.y)<1,'Investigation section scroll moves its navigation');
 await page.locator('.investigation-section-scroll').evaluate(element=>{element.scrollTop=0;});
 await capture('03-questions');
 await page.getByRole('tab',{name:/^Sources/}).click();check(await page.locator('.source-list li').count()===5,'Sources should show the five-file preview before expansion');await page.getByRole('button',{name:'Show all 6 matching sources'}).click();await page.locator('.source-list').getByText('sales.csv',{exact:true}).waitFor();check(await page.locator('.source-list li').count()===6,'Sources does not reveal the complete catalog when expanded');
 await page.getByRole('tab',{name:/^Questions/}).click();check(await page.locator('.question-row-summary').count()===5,'Questions does not show the four saved questions and the newly added question');
 await page.getByRole('tab',{name:/^Analysis/}).click();await page.locator('.run-overview .steps li').first().waitFor();check(await page.locator('.run-overview .steps li').count()===2,'Analysis does not show the saved run steps');await capture('04-analysis');
 await page.getByRole('tab',{name:/^Results/}).click();await page.locator('.results-summary .headline').waitFor();check((await page.locator('.results-summary').innerText()).includes('Packs fell'),'Results does not show the saved Investigation findings');await capture('05-results');
 await page.getByRole('button',{name:'Files',exact:true}).click();await page.locator('.file-row').filter({hasText:'sales.csv'}).click();await page.locator('.source-view .table-scroll table').waitFor();await capture('06-files');
 await page.getByRole('button',{name:'Reports',exact:true}).click();await page.locator('.report-row').filter({hasText:doc.title}).click();await page.locator('.studio-document h1').waitFor();await capture('07-report');
 await page.getByRole('button',{name:'Chat',exact:true}).click();await page.getByRole('tab',{name:/^Sources/}).click();
 await page.getByRole('combobox',{name:/^Ask about/}).fill('Keep this draft while browsing files and reports');
 await page.getByRole('button',{name:'Files',exact:true}).click();await page.getByRole('button',{name:'Chat',exact:true}).click();check(await page.getByRole('combobox',{name:/^Ask about/}).inputValue()==='Keep this draft while browsing files and reports','Draft was lost on section change');
 await page.setViewportSize({width:1024,height:768});await noOverflow('1024 Chat');await capture('08-chat-1024');
 const invTabs=await page.locator('.investigation-nav button').evaluateAll(elements=>elements.map(element=>{const a=element.getBoundingClientRect(),b=element.parentElement.getBoundingClientRect();return {label:element.textContent.trim(),fits:a.left>=b.left-1&&a.right<=b.right+1};}));check(invTabs.every(tab=>tab.fits),'Investigation tabs clip at 1024');measurements.push({invTabs});
 await page.setViewportSize({width:680,height:800});await noOverflow('680 Chat');await capture('09-chat-680');
 if(await page.getByRole('tab',{name:'Investigation',exact:true}).isVisible()){await page.getByRole('tab',{name:'Investigation',exact:true}).click();await capture('10-investigation-680');await page.getByRole('tab',{name:'Chat',exact:true}).click();}
 await page.locator('.model-trigger').click();await page.getByRole('dialog',{name:'Choose a model',exact:true}).waitFor();await capture('11-model-picker-680');
 const persisted=await invoke(page,'investigation_load',{investigationId:inv.id});check(persisted.questions.some(question=>question.text==='Which sources need reconciliation before a final decision?'),'Added question was not persisted');
 console.log(JSON.stringify({profile,captures,measurements,issues},null,2));
 await writeFile(join(profile,'visual-review.json'),JSON.stringify({measurements,issues},null,2));
 if(issues.length) process.exitCode=1;
} catch(error){console.error(error);if(page&&!page.isClosed()){await capture('failure').catch(()=>{});await writeFile(join(profile,'failure-visible-state.txt'),await page.locator('body').innerText());}console.error('Profile:',profile);process.exitCode=1;}finally{await app?.close();}
