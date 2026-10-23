<script lang="ts">
	import type { ImportedReportPackage } from '$lib/analytics-artifacts';
	import MarkdownContent from './MarkdownContent.svelte';
	let { report }: { report: ImportedReportPackage } = $props();
	const runFor = (artifactId: string) => {
		const artifact = report.artifacts.find(item => item.id === artifactId);
		return artifact ? { artifact, run: report.runs.find(item => item.id === artifact.run_id) ?? null } : null;
	};
</script>

<article class="imported-report" aria-label={`Read-only report: ${report.document.title}`}>
	<header>
		<p class="eyebrow">Imported report · read-only snapshot</p>
		<h2>{report.document.title}</h2>
		<p class="report-scope">{report.artifacts.length} saved {report.artifacts.length === 1 ? 'output' : 'outputs'} · {report.runs.length} retained {report.runs.length === 1 ? 'analysis run' : 'analysis runs'}</p>
	</header>
	{#if report.document.body.trim()}<section class="report-notes" aria-label="Report notes"><MarkdownContent text={report.document.body} /></section>{/if}
	<ol class="report-outputs" aria-label="Report outputs">
		{#each report.document.placements as placement (placement.id)}
			{@const resolved = runFor(placement.artifact_id)}
			<li>
				{#if resolved?.run}
					<article class="report-output">
						<h3>{resolved.artifact.title}</h3>
						<p class="question">{resolved.run.original_question}</p>
						{#if resolved.run.answer}<MarkdownContent text={resolved.run.answer} />{/if}
						{#each resolved.run.results as result (result.id)}
							<section class="saved-result" aria-label={`Exact values for ${result.id}`}>
								<h4>{result.id}</h4>
								<p class="result-scope">{result.complete ? `${result.row_count} rows · saved result` : `Incomplete saved result · ${result.exact_rows.length} of ${result.row_count} rows retained`}</p>
								{#if result.exact_rows.length || result.row_count === 0}
									<div class="table-scroll"><table><thead><tr>{#each result.columns as column}<th scope="col">{column}</th>{/each}</tr></thead><tbody>{#each result.exact_rows as row, rowIndex (`${result.id}:${rowIndex}`)}<tr>{#each result.columns as _, columnIndex (`${result.id}:${rowIndex}:${columnIndex}`)}<td>{row[columnIndex] ?? '[exact value unavailable]'}</td>{/each}</tr>{/each}</tbody></table></div>
								{:else}<p>Exact values were not retained. Rounded values are not substituted.</p>{/if}
							</section>
						{/each}
						<details class="provenance">
							<summary>Sources, checks and method</summary>
							<p>Recorded status: {resolved.run.result_status}{resolved.run.source_revision ? ` · source revision ${resolved.run.source_revision}` : ''}</p>
							{#if resolved.run.source_labels.length}<p>Sources: {resolved.run.source_labels.join(', ')}</p>{/if}
							{#if resolved.run.definitions.length}<h4>Definitions</h4><ul>{#each resolved.run.definitions as definition}<li>{definition}</li>{/each}</ul>{/if}
							{#if resolved.run.checks.length || resolved.run.result_checks.length}<h4>Recorded checks</h4><ul>{#each [...resolved.run.checks, ...resolved.run.result_checks] as check, index (`${resolved.run.id}:check:${index}`)}<li>{JSON.stringify(check)}</li>{/each}</ul>{/if}
							{#if resolved.run.methods.length}<h4>Methods</h4>{#each resolved.run.methods as method (method.evidence_id)}<details><summary>{method.tool} · {method.evidence_id}</summary>{#if method.sql}<pre>{method.sql}</pre>{/if}</details>{/each}{/if}
						</details>
					</article>
				{:else}<p>Saved output {placement.artifact_id} is missing from this snapshot.</p>{/if}
			</li>
		{/each}
	</ol>
	<p class="warning">This imported report cannot query its original files. Saved checks are historical and have not been rerun. Its presentation is a read-only snapshot and cannot access data.</p>
</article>

<style>
	.imported-report { display:grid; gap:var(--space-5); min-width:0; color:var(--text-primary); }
	header h2 { margin:2px 0 4px; font-size:var(--fs-xl); font-weight:600; letter-spacing:-.02em; }
	.eyebrow,.report-scope,.result-scope { margin:0; color:var(--text-faint); font-size:var(--fs-xs); }
	.report-notes { padding-bottom:var(--space-4); border-bottom:1px solid var(--border-subtle); }
	.report-outputs { margin:0; padding:0; list-style:none; }
	.report-outputs > li + li { border-top:1px solid var(--border-subtle); }
	.report-output { display:grid; gap:var(--space-3); padding:var(--space-5) 0; }
	.report-output h3,.saved-result h4 { margin:0; font-size:var(--fs-md); font-weight:600; }
	.question { margin:0; color:var(--text-dim); font-size:var(--fs-sm); }
	.saved-result { display:grid; gap:var(--space-2); min-width:0; }
	.table-scroll { max-width:100%; overflow:auto; border:1px solid var(--border-subtle); border-radius:var(--radius); }
	table { border-collapse:collapse; font-size:var(--fs-xs); white-space:nowrap; }
	th,td { padding:7px 10px; border-bottom:1px solid var(--border-subtle); text-align:left; }
	thead th { color:var(--text-faint); font-weight:500; }
	.provenance { color:var(--text-dim); font-size:var(--fs-xs); }
	.provenance summary { cursor:pointer; color:var(--text-primary); }
	.provenance pre { overflow:auto; white-space:pre-wrap; overflow-wrap:anywhere; }
	.warning { margin:0; padding:var(--space-3); border-radius:var(--radius); background:var(--bg-muted); color:var(--text-dim); font-size:var(--fs-xs); line-height:1.55; }
</style>
