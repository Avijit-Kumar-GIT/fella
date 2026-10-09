<script lang="ts">
	import { onMount } from 'svelte';
	import { dispatch, openFolder } from '$lib/commands';
	import { ipc, isDesktop, openExternal } from '$lib/ipc';
	import { prefs, type Appearance } from '$lib/prefs.svelte';
	import { session } from '$lib/session.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Switch } from '$lib/components/ui/switch';
	import type { AnalysisCapabilities, RunLogEntry } from '$lib/types';
	import Icon from './Icon.svelte';
	import ProviderIcon from './ProviderIcon.svelte';

	const appearances: { id: Appearance; label: string; detail: string }[] = [
		{ id: 'system', label: 'System', detail: 'Follow your computer' },
		{ id: 'light', label: 'Light', detail: 'Bright workspace' },
		{ id: 'dark', label: 'Dark', detail: 'Low light workspace' }
	];
	const defaultCapabilities: AnalysisCapabilities = {
		table_analysis: true,
		document_analysis: true,
		python_analysis: true,
		visualizations: true
	};
	const capabilityOptions: {
		key: keyof AnalysisCapabilities;
		label: string;
		detail: string;
	}[] = [
		{ key: 'table_analysis', label: 'Table analysis', detail: 'Schemas, samples, and SQL queries' },
		{ key: 'document_analysis', label: 'Document analysis', detail: 'Search and read text and PDF files' },
		{ key: 'python_analysis', label: 'Python calculations', detail: 'Sandboxed calculations for advanced statistics' },
		{ key: 'visualizations', label: 'Visualizations', detail: 'Validated charts from table queries' }
	];

	let currentProvider = $derived(session.settings?.provider ?? '');
	let currentModel = $derived.by(() => {
		const conversationModel = session.activeChat?.model.trim() ?? '';
		const defaultModel = session.settings?.model?.trim() ?? '';
		return conversationModel || defaultModel;
	});
	let provider = $derived(session.providers.find((item) => item.id === currentProvider));
	let workspace = $derived(session.catalog.workspace);
	let capabilities = $derived(session.settings?.capabilities ?? defaultCapabilities);
	let capabilityError = $state('');
	let runLog = $state<RunLogEntry[]>([]);
	let runLogLoading = $state(false);
	let runLogError = $state('');
	let runMetrics = $derived.by(() => {
		const turns = runLog.filter((entry) => entry.kind === 'turn');
		const durations = turns
			.flatMap((entry) => entry.elapsed_ms === undefined ? [] : [entry.elapsed_ms])
			.sort((a, b) => a - b);
		const middle = Math.floor(durations.length / 2);
		const medianMs = durations.length === 0
			? undefined
			: durations.length % 2 === 0
				? Math.round((durations[middle - 1] + durations[middle]) / 2)
				: durations[middle];
		const operations = turns.flatMap((entry) => entry.operations);
		return {
			turns: turns.length,
			medianMs,
			toolSteps: operations.length,
			toolErrors: operations.filter((operation) => !operation.success).length,
			triggers: runLog.filter((entry) => entry.kind === 'trigger').length
		};
	});

	async function toggleCapability(key: keyof AnalysisCapabilities): Promise<void> {
		if (!isDesktop()) return;
		const current = session.settings?.capabilities ?? defaultCapabilities;
		const enabled = !current[key];
		const next: AnalysisCapabilities = { ...current, [key]: enabled };
		if (key === 'table_analysis' && !enabled) next.visualizations = false;
		capabilityError = '';
		try {
			session.settings = await ipc.setSettings({ capabilities: next });
		} catch (error) {
			capabilityError = error instanceof Error ? error.message : String(error);
		}
	}

	function command(commandText: string): void {
		session.setWorkspaceView('ask');
		void dispatch(commandText);
	}

	async function refreshSettings(): Promise<void> {
		if (!isDesktop()) return;
		try {
			session.settings = await ipc.getSettings();
			session.providers = await ipc.listProviders();
		} catch {
			/* Ask remains the fallback if the engine is unavailable. */
		}
	}

	async function refreshRunLog(): Promise<void> {
		if (!isDesktop()) return;
		runLogLoading = true;
		runLogError = '';
		try {
			runLog = await ipc.runLogRecent(100);
		} catch (error) {
			runLog = [];
			runLogError = error instanceof Error ? error.message : String(error);
		} finally {
			runLogLoading = false;
		}
	}

	function modeLabel(mode: RunLogEntry['mode']): string {
		if (mode === 'workspace_ask') return 'Workspace ask';
		if (mode === 'workspace_inspect') return 'Inspect';
		if (mode === 'model_only') return 'General';
		return 'Analysis';
	}

	function outcomeLabel(outcome: string): string {
		const labels: Record<string, string> = {
			completed: 'Completed',
			clarification_requested: 'Clarification requested',
			returned_with_limits: 'Returned with limits',
			failed: 'Failed',
			unsupported: 'Unsupported',
			cancelled: 'Cancelled',
			received: 'Received',
			interpreting: 'Interpreting',
			grounding: 'Grounding',
			planning: 'Planning',
			executing: 'Executing',
			verifying: 'Verifying',
			retry: 'Retry',
			triggered: 'Trigger recorded'
		};
		return labels[outcome] ?? outcome.replaceAll('_', ' ');
	}

	function triggerLabel(trigger: string | undefined): string {
		if (trigger === 'hard_fail_unresolved') return 'Unresolved hard failure';
		if (trigger === 'repeated_tool_errors') return 'Repeated tool errors';
		return trigger?.replaceAll('_', ' ') ?? 'Runtime trigger';
	}

	function formatTimestamp(timestamp: number): string {
		const date = new Date(timestamp);
		if (!Number.isFinite(date.getTime())) return 'Unknown time';
		return date.toLocaleString(undefined, {
			month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit'
		});
	}

	function timestampIso(timestamp: number): string | undefined {
		const date = new Date(timestamp);
		return Number.isFinite(date.getTime()) ? date.toISOString() : undefined;
	}

	function formatDuration(milliseconds: number | undefined): string {
		if (milliseconds === undefined) return '—';
		return milliseconds < 1000 ? `${milliseconds} ms` : `${(milliseconds / 1000).toFixed(1)} s`;
	}

	function runCounts(entry: RunLogEntry): string {
		const models = entry.model_calls.length;
		const tools = entry.operations.length;
		return `${models} model call${models === 1 ? '' : 's'} · ${tools} tool step${tools === 1 ? '' : 's'} · ${formatDuration(entry.elapsed_ms)}`;
	}

	function tokenSummary(entry: RunLogEntry): string | null {
		const prompt = entry.model_calls.flatMap((call) => call.prompt_tokens === undefined ? [] : [call.prompt_tokens]);
		const completion = entry.model_calls.flatMap((call) => call.completion_tokens === undefined ? [] : [call.completion_tokens]);
		if (prompt.length === 0 && completion.length === 0) return null;
		const total = (values: number[]) => values.reduce((sum, value) => sum + value, 0).toLocaleString();
		return `${prompt.length ? total(prompt) : '—'} in · ${completion.length ? total(completion) : '—'} out`;
	}

	onMount(() => {
		if (!session.settings || session.providers.length === 0) void refreshSettings();
		void refreshRunLog();
	});
</script>

<section class="settings-page" aria-labelledby="settings-title">
	<header class="page-head">
		<div>
			<h1 id="settings-title">Settings</h1>
		</div>
	</header>

	<div class="settings-grid">
		<section class="settings-card" aria-labelledby="model-title">
			<div class="card-head">
				<div>
					<h2 id="model-title">Model</h2>
					<p>Fella reads and computes locally. Your chosen provider receives prompts, relevant context, and tool results—which may include document text or data rows.</p>
				</div>
				{#if provider}<ProviderIcon providerId={provider.id} size={20} />{/if}
			</div>
			<div class="current-row">
				<div>
					<span class="label">Current provider</span>
					<strong>{provider?.display ?? (currentProvider || 'Not connected')}</strong>
				</div>
				<Button variant="ghost" size="sm" class="settings-action" onclick={() => command('/login')}>Change</Button>
			</div>
			<div class="current-row">
				<div>
					<span class="label">{session.activeChat?.model.trim() ? 'Conversation model' : 'Default model'}</span>
					<strong title={currentModel}>{currentModel || 'Choose a model'}</strong>
				</div>
				<Button variant="ghost" size="sm" class="settings-action" onclick={() => command('/model')}>Choose</Button>
			</div>
			{#if session.providers.length}
				<div class="provider-list">
					<p class="section-label">Available providers</p>
					{#each session.providers.filter((item) => item.auth === 'key') as item (item.id)}
						<button
							class="provider-row"
							class:current={item.id === currentProvider}
							type="button"
							onclick={() => command(`/login ${item.id}`)}
						>
							<ProviderIcon providerId={item.id} size={16} />
							<span>{item.display}</span>
							<small>{item.id === currentProvider ? 'Current' : item.authed ? 'Connected' : 'Connect'}</small>
						</button>
					{/each}
				</div>
			{/if}
			<Button
				variant="link"
				size="sm"
				class="text-button"
				onclick={() => void openExternal('https://docs.lilfella.app/developer-platform/using-fella/privacy')}
			>
				Privacy and security <Icon name="arrow-up-right" size={12} />
			</Button>
			<Button variant="link" size="sm" class="text-button" onclick={() => void refreshSettings()}>
				<Icon name="check" size={16} /> Refresh connection status
			</Button>
		</section>

		<section class="settings-card" aria-labelledby="appearance-title">
			<div class="card-head">
				<div>
					<h2 id="appearance-title">Appearance</h2>
					<p>Choose the contrast that feels right for your workspace.</p>
				</div>
				<Icon name={prefs.isDark ? 'moon' : 'sun'} size={20} />
			</div>
			<div class="appearance-list">
				{#each appearances as option (option.id)}
					<button
						class="appearance-row"
						class:selected={prefs.appearance === option.id}
						type="button"
						onclick={() => prefs.setAppearance(option.id)}
					>
						<span><strong>{option.label}</strong><small>{option.detail}</small></span>
						{#if prefs.appearance === option.id}<Icon name="check" size={16} />{/if}
					</button>
				{/each}
			</div>
		</section>

		<section class="settings-card workspace-card" aria-labelledby="folder-title">
			<div class="card-head">
				<div>
					<h2 id="folder-title">Workspace</h2>
					<p>Your mounted folder is the only data source Fella can analyze.</p>
				</div>
				<Icon name="folder" size={20} />
			</div>
			{#if workspace}
				<code title={workspace}>{workspace}</code>
			{:else}
				<p class="muted">No folder mounted yet.</p>
			{/if}
			<Button variant="ghost" size="sm" class="settings-action" onclick={() => void openFolder()}>
				<Icon name="folder" size={16} /> {workspace ? 'Change folder' : 'Choose a folder'}
			</Button>
		</section>

		<section class="settings-card capability-card" aria-labelledby="capability-title">
			<div class="card-head">
				<div>
					<div class="title-line">
						<h2 id="capability-title">Analysis capabilities</h2>
						<span class="experimental-badge">Experimental</span>
					</div>
					<p>Choose which analysis paths the model may use on this computer.</p>
				</div>
				<Icon name="settings" size={20} />
			</div>
			<div class="capability-list">
				{#each capabilityOptions as item (item.key)}
					<div class="capability-row">
						<div class="capability-copy">
							<strong>{item.label}</strong>
							<small>{item.detail}</small>
						</div>
						<Switch.Root
							class="fella-ui-switch"
							checked={capabilities[item.key]}
							aria-label={`${item.label}: ${capabilities[item.key] ? 'on' : 'off'}`}
							disabled={!capabilities.table_analysis && item.key === 'visualizations'}
							onCheckedChange={() => void toggleCapability(item.key)}
						>
							<Switch.Thumb class="fella-ui-switch-thumb" />
						</Switch.Root>
					</div>
				{/each}
			</div>
			<p class="capability-note">Evidence, verification, and the read-only boundary always stay on.</p>
			{#if capabilityError}<p class="error-note">{capabilityError}</p>{/if}
		</section>

		<section class="settings-card run-log-card" aria-labelledby="run-log-title">
			<div class="card-head">
				<div>
					<h2 id="run-log-title">Run log</h2>
					<p>Recent run timings and tool activity, kept on this computer. This is operational detail, not a correctness score.</p>
				</div>
				<button class="text-button refresh-log" type="button" disabled={runLogLoading} onclick={() => void refreshRunLog()}>
					<Icon name="refresh" size={16} /> {runLogLoading ? 'Refreshing' : 'Refresh'}
				</button>
			</div>
			<p class="run-log-note">Transcript and workspace contents aren’t copied into this view. Fella doesn’t transmit this log.</p>
			{#if runLog.length > 0}
				<div class="run-log-summary" aria-label="Summary of the visible run log">
					<span>{runMetrics.turns} turns</span>
					<span>median {formatDuration(runMetrics.medianMs)}</span>
					<span>{runMetrics.toolErrors} tool errors / {runMetrics.toolSteps} steps</span>
					<span>{runMetrics.triggers} triggers</span>
				</div>
			{/if}
			{#if runLogError}
				<p class="error-note">Couldn't load the run log: {runLogError}</p>
			{:else if runLog.length === 0}
				<p class="muted">No saved runs or friction triggers yet.</p>
			{:else}
				<div class="run-log-list" role="log" aria-label="Recent Fella runs">
					{#each runLog as entry (entry.id)}
						<details class="run-log-entry">
							<summary>
								<time datetime={timestampIso(entry.at_ms)}>{formatTimestamp(entry.at_ms)}</time>
								<span class="run-kind" class:trigger-kind={entry.kind === 'trigger'}>{entry.kind === 'trigger' ? 'Trigger' : modeLabel(entry.mode)}</span>
								<span class="run-outcome">{entry.kind === 'trigger' ? triggerLabel(entry.trigger) : outcomeLabel(entry.outcome)}</span>
								<span class="run-counts">
									{#if entry.kind === 'trigger'}
										{entry.trigger_steps ?? 0} steps · {entry.trigger_errors ?? 0} errors
									{:else}
										{runCounts(entry)}
									{/if}
								</span>
							</summary>
							<div class="run-log-detail">
								{#if entry.kind === 'turn'}
									<div class="run-log-meta">
										<span>{entry.model ?? 'Model not recorded'}</span>
										{#if tokenSummary(entry)}<span>{tokenSummary(entry)}</span>{/if}
										{#if entry.prior_analysis_count > 0}<span>{entry.prior_analysis_count} earlier {entry.prior_analysis_count === 1 ? 'analysis' : 'analyses'} reused</span>{/if}
										{#if entry.context_reference_count > 0}<span>{entry.context_reference_count} attached context reference{entry.context_reference_count === 1 ? '' : 's'}</span>{/if}
										{#if entry.clarification_continuation}<span>Clarification continuation</span>{/if}
										{#if entry.rerun}<span>Rerun</span>{/if}
										<code>{entry.id}</code>
									</div>
									{#if entry.model_calls.length}
										<p class="run-log-group-label">Model calls</p>
										<div class="run-log-operations">
											{#each entry.model_calls as call, index (`${entry.id}-model-${index}`)}
												<div><span>Model call · {call.model}</span><small class:call-failed={!call.success}>{formatDuration(call.duration_ms)}{call.success ? '' : ' · failed'}</small></div>
											{/each}
										</div>
									{/if}
									{#if entry.operations.length}
										<p class="run-log-group-label">Tool steps</p>
										<div class="run-log-operations">
											{#each entry.operations as operation, index (`${entry.id}-operation-${index}`)}
												<div><span>{operation.operation}</span><small class:call-failed={!operation.success}>{formatDuration(operation.duration_ms)}{operation.success ? '' : ' · failed'}</small></div>
											{/each}
										</div>
									{:else}<p class="run-log-empty-detail">No local analysis tool calls were recorded for this turn.</p>{/if}
								{:else}
									<p class="run-log-empty-detail">{entry.trigger_steps ?? 0} tool steps, {entry.trigger_errors ?? 0} errors. The signal contains no prompt, answer, tool arguments, or file data.</p>
								{/if}
							</div>
						</details>
					{/each}
				</div>
			{/if}
		</section>
	</div>
</section>

<style>
	.settings-page {
		flex: 1;
		min-height: 0;
		width: 100%;
		max-width: var(--content-max);
		margin: 0 auto;
		padding: var(--space-6) var(--pad) var(--space-6);
		overflow: auto;
	}
	.page-head {
		margin-bottom: var(--space-5);
	}
	h1 {
		margin: 0;
		font-size: clamp(24px, 3vw, 32px);
		font-weight: 650;
		letter-spacing: -0.03em;
	}
	.settings-grid {
		display: flex;
		flex-direction: column;
		gap: 0;
		max-width: 760px;
	}
	.settings-card {
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		padding: var(--space-5) 0;
		border: 0;
		border-top: 1px solid var(--border);
		border-radius: 0;
		background: transparent;
	}
	.settings-card:first-child {
		padding-top: 0;
		border-top: 0;
	}
	.workspace-card,
	.capability-card {
		grid-column: auto;
	}
	.card-head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-3);
	}
	.card-head > :global(svg) {
		flex: none;
		color: var(--text-faint);
	}
	h2 {
		margin: 0;
		font-size: var(--fs-md);
		font-weight: 650;
	}
	.card-head p {
		max-width: 42ch;
		margin: 4px 0 0;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		line-height: 1.45;
	}
	.title-line {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 8px;
	}
	.experimental-badge {
		padding: 3px 7px;
		border: 1px solid var(--brand);
		border-radius: 999px;
		color: var(--brand);
		font-size: var(--fs-xs);
		font-weight: 650;
		letter-spacing: 0.02em;
		white-space: nowrap;
	}
	.current-row,
	.provider-row,
	.appearance-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
	}
	.current-row {
		padding-top: var(--space-3);
		border-top: 1px solid var(--border);
	}
	.current-row > div {
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 3px;
	}
	.label,
	.section-label,
	.muted {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.current-row strong {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: var(--fs-sm);
		font-weight: 600;
	}
	.current-row :global(.settings-action),
	.workspace-card :global(.settings-action) {
		flex: none;
	}
	.provider-list,
	.appearance-list {
		border-top: 1px solid var(--border);
	}
	.capability-list {
		border-top: 1px solid var(--border);
	}
	.capability-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-4);
		padding: 10px 0;
	}
	.capability-row + .capability-row {
		border-top: 1px solid var(--border);
	}
	.capability-copy {
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 3px;
	}
	.capability-copy strong {
		font-size: var(--fs-sm);
		font-weight: 600;
	}
	.capability-copy small,
	.capability-note,
	.error-note {
		color: var(--text-faint);
		font-size: var(--fs-xs);
		line-height: 1.45;
	}
	.capability-note,
	.error-note {
		margin: 0;
	}
	.error-note {
		color: var(--danger, #c15d5d);
	}
	.section-label {
		margin: var(--space-3) 0 var(--space-1);
	}
	.provider-row,
	.appearance-row {
		width: 100%;
		padding: 8px 0;
		color: var(--text-dim);
		text-align: left;
	}
	.provider-row + .provider-row,
	.appearance-row + .appearance-row {
		border-top: 1px solid var(--border);
	}
	.provider-row:hover,
	.provider-row.current,
	.appearance-row:hover,
	.appearance-row.selected {
		color: var(--text);
	}
	.provider-row > :global(.provider-icon) {
		margin-right: 2px;
	}
	.provider-row span:not(:global(.provider-icon)) {
		flex: 1;
	}
	.provider-row small {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	:global(.text-button) {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		align-self: flex-start;
		padding: 0;
		font-size: var(--fs-xs);
	}
	:global(.text-button:hover) {
		text-decoration: underline;
	}
	.appearance-row > span {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.appearance-row strong {
		font-size: var(--fs-sm);
		font-weight: 600;
	}
	.appearance-row small {
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.appearance-row :global(svg:last-child) {
		color: var(--text-dim);
	}
	.workspace-card code {
		padding: 8px 10px;
		border-radius: var(--radius-sm);
		background: var(--bg-inset);
		color: var(--text-dim);
		font-family: var(--mono);
		font-size: 11px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.run-log-note {
		margin: -6px 0 0;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		line-height: 1.45;
	}
	.run-log-summary {
		display: flex;
		flex-wrap: wrap;
		gap: 5px 14px;
		color: var(--text-faint);
		font-family: var(--mono);
		font-size: 10px;
		font-variant-numeric: tabular-nums;
	}
	.refresh-log {
		flex: none;
		padding-top: 3px;
	}
	.refresh-log:disabled {
		cursor: wait;
		opacity: 0.55;
	}
	.run-log-list {
		max-height: 440px;
		overflow: auto;
		border-top: 1px solid var(--border);
	}
	.run-log-entry {
		border-bottom: 1px solid var(--border);
	}
	.run-log-entry summary {
		display: grid;
		grid-template-columns: 116px 112px minmax(110px, 1fr) auto;
		align-items: center;
		gap: 10px;
		min-height: 38px;
		cursor: pointer;
		list-style: none;
		color: var(--text-dim);
		font-size: var(--fs-xs);
	}
	.run-log-entry summary::-webkit-details-marker {
		display: none;
	}
	.run-log-entry summary:hover,
	.run-log-entry[open] summary {
		color: var(--text);
	}
	.run-log-entry time,
	.run-log-entry code,
	.run-counts,
	.run-log-operations small {
		font-family: var(--mono);
		font-size: 10px;
		font-variant-numeric: tabular-nums;
	}
	.run-log-entry time,
	.run-counts {
		color: var(--text-faint);
		white-space: nowrap;
	}
	.run-kind {
		color: var(--text-faint);
		white-space: nowrap;
	}
	.run-kind.trigger-kind {
		color: var(--warning, #b78430);
	}
	.run-outcome {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.run-counts {
		text-align: right;
	}
	.run-log-detail {
		padding: 2px 0 12px 126px;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.run-log-meta {
		display: flex;
		flex-wrap: wrap;
		gap: 5px 14px;
		margin-bottom: 8px;
	}
	.run-log-meta code {
		color: var(--text-faint);
		overflow-wrap: anywhere;
	}
	.run-log-operations {
		display: grid;
		gap: 3px;
	}
	.run-log-group-label {
		margin: 7px 0 4px;
		color: var(--text-faint);
		font-size: 10px;
		font-weight: 600;
	}
	.run-log-operations > div {
		display: flex;
		justify-content: space-between;
		gap: 12px;
	}
	.run-log-operations span {
		color: var(--text-dim);
		font-family: var(--mono);
		font-size: 10px;
	}
	.run-log-operations small {
		flex: none;
		color: var(--text-faint);
	}
	.run-log-operations small.call-failed {
		color: var(--danger, #c15d5d);
	}
	.run-log-empty-detail {
		margin: 0;
		line-height: 1.45;
	}
	@media (max-width: 680px) {
		.settings-grid {
			width: 100%;
		}
		.run-log-entry summary {
			grid-template-columns: 94px 76px minmax(60px, 1fr);
			gap: 6px;
		}
		.run-counts {
			grid-column: 1 / -1;
			grid-row: 2;
			padding-bottom: 7px;
			text-align: left;
		}
		.run-log-detail {
			padding-left: 0;
		}
	}
</style>
