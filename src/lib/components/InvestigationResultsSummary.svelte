<script lang="ts">
	import type { Investigation, InvestigationQuestion, Message } from '$lib/types';
	import { Button } from '$lib/components/ui/button';
	import Icon from './Icon.svelte';
	let { investigation, messages, onreview, onreport, busy = false, disabledReason = '' }: {
		investigation:Investigation; messages:Message[]; onreview:(id:string)=>void;
		onreport:()=>void; busy?:boolean; disabledReason?:string;
	} = $props();
	let questions = $derived((investigation.questions ?? []).filter(question => question.status === 'active'));
	function savedAnswer(question:InvestigationQuestion):string {
		const message = messages.find(message => message.role === 'assistant' && message.answer?.turn_id === question.latest_analysis_turn_id);
		return (message?.answer?.text ?? '').split(/\n\s*\n/).find(part => part.trim())?.replace(/[#*_`]/g,'').trim() ?? '';
	}
	let headline = $derived(investigation.findings?.[0]?.claim || (questions.find(question => !question.parent_id) ? savedAnswer(questions.find(question => !question.parent_id)!) : ''));
	let answered = $derived(questions.filter(question => question.latest_analysis_turn_id));
	let openCount = $derived(questions.filter(question => question.answer_status !== 'answered').length);
</script>

<div class="results-summary">
	<section class="headline"><h3>Headline</h3>{#if headline}<p>{headline}</p>{:else}<p class="empty">Run the main question to build a summary.</p>{/if}</section>
	{#if investigation.findings?.length}
		<section><header><h3>Findings</h3><span>{investigation.findings.length}</span></header><ul>{#each investigation.findings as finding (finding.id)}<li><button class="finding" onclick={() => onreview(finding.question_id)}>{finding.claim}<Icon name="chevron-right" size={12} /></button>{#if finding.caveats.length}<p>{finding.caveats.join(' ')}</p>{/if}</li>{/each}</ul></section>
	{:else}
		<section><header><h3>Answers</h3><span>{answered.length}</span></header><ul>{#each answered as question (question.id)}<li><button class="finding" onclick={() => onreview(question.id)}>{question.text}<Icon name="chevron-right" size={12} /></button>{#if savedAnswer(question)}<p>{savedAnswer(question)}</p>{/if}<small class:attention={question.answer_status !== 'answered'}>{question.answer_status === 'answered' ? 'Answered' : question.answer_status.replaceAll('_',' ')}</small></li>{/each}</ul>{#if !answered.length}<p class="empty">Saved answers will appear here.</p>{/if}</section>
	{/if}
	<section class="report-action"><Button disabled={busy || !!disabledReason} title={disabledReason || undefined} onclick={onreport}>{busy ? 'Preparing report…' : 'Create report'}</Button>{#if openCount}<p>{openCount} {openCount === 1 ? 'question is' : 'questions are'} still open. The report will include these gaps.</p>{/if}</section>
</div>

<style>
	.results-summary { display:grid; gap:24px; min-width:0; }
	h3 { margin:0 4px 8px; color:var(--text-dim); font-size:12px; font-weight:400; }
	header { display:flex; justify-content:space-between; align-items:baseline; }
	header span { color:var(--text-faint); font-size:12px; }
	.headline > p { margin:0 4px; color:var(--chat-body); font-size:14.5px; line-height:23px; }
	ul { margin:0; padding:0; list-style:none; }
	li { padding:12px 4px; border-bottom:1px solid var(--border); }
	li:last-child { border-bottom:0; }
	.finding { display:flex; width:100%; align-items:flex-start; justify-content:space-between; gap:8px; color:var(--text); text-align:left; font-size:13px; font-weight:400; line-height:19px; }
	.finding :global(svg) { flex:none; margin-top:4px; color:var(--text-faint); }
	li p { margin:4px 0 0; color:var(--text-dim); font-size:12px; line-height:18px; }
	li small { display:block; margin-top:5px; color:var(--text-faint); font-size:11.5px; text-transform:capitalize; }
	li small.attention { color:var(--warn); }
	.report-action :global(button) { display:flex; width:100%; justify-content:center; gap:8px; min-height:40px; border-radius:999px; font-size:13px; font-weight:400; }
	.report-action p { margin:8px 4px 0; color:var(--text-faint); font-size:11.5px; line-height:18px; text-align:center; }
	.empty, .headline > p.empty { color:var(--text-faint); font-size:12.5px; line-height:19px; }
</style>
