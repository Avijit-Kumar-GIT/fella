<script lang="ts">
	import { dispatch, openFolder } from '$lib/commands';
	import { ipc, isTauri } from '$lib/ipc';
	import { prefs, type Appearance } from '$lib/prefs.svelte';
	import { session } from '$lib/session.svelte';
	import type { AnalysisCapabilities } from '$lib/types';
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
	let currentModel = $derived(session.model || session.settings?.model || '');
	let provider = $derived(session.providers.find((item) => item.id === currentProvider));
	let workspace = $derived(session.catalog.workspace);
	let capabilities = $derived(session.settings?.capabilities ?? defaultCapabilities);
	let capabilityError = $state('');

	async function toggleCapability(key: keyof AnalysisCapabilities): Promise<void> {
		if (!isTauri()) return;
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
		if (!isTauri()) return;
		try {
			session.settings = await ipc.getSettings();
			session.providers = await ipc.listProviders();
		} catch {
			/* Ask remains the fallback if the engine is unavailable. */
		}
	}
</script>

<section class="settings-page" aria-labelledby="settings-title">
	<header class="page-head">
		<div>
			<p class="eyebrow">Fella</p>
			<h1 id="settings-title">Settings</h1>
			<p class="lede">Choose how Fella connects and how it looks on this computer.</p>
		</div>
	</header>

	<div class="settings-grid">
		<section class="settings-card" aria-labelledby="model-title">
			<div class="card-head">
				<div>
					<h2 id="model-title">Model</h2>
					<p>Fella uses your provider key and sends only the question and requested evidence.</p>
				</div>
				{#if provider}<ProviderIcon providerId={provider.id} size={20} />{/if}
			</div>
			<div class="current-row">
				<div>
					<span class="label">Current provider</span>
					<strong>{provider?.display ?? (currentProvider || 'Not connected')}</strong>
				</div>
				<button class="pill ghost" type="button" onclick={() => command('/login')}>Change</button>
			</div>
			<div class="current-row">
				<div>
					<span class="label">Default model</span>
					<strong>{currentModel || 'Choose a model'}</strong>
				</div>
				<button class="pill ghost" type="button" onclick={() => command('/model')}>Choose</button>
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
			<button class="text-button" type="button" onclick={() => void refreshSettings()}>
				<Icon name="check" size={16} /> Refresh connection status
			</button>
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
			<button class="pill ghost" type="button" onclick={() => void openFolder()}>
					<Icon name="folder" size={16} /> {workspace ? 'Change folder' : 'Choose a folder'}
			</button>
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
						<button
							class="capability-toggle"
							class:on={capabilities[item.key]}
							type="button"
							role="switch"
							aria-checked={capabilities[item.key]}
							aria-label={`${item.label}: ${capabilities[item.key] ? 'on' : 'off'}`}
							disabled={!capabilities.table_analysis && item.key === 'visualizations'}
							onclick={() => void toggleCapability(item.key)}
						>
							<span></span>
						</button>
					</div>
				{/each}
			</div>
			<p class="capability-note">Evidence, verification, and the read-only boundary always stay on.</p>
			{#if capabilityError}<p class="error-note">{capabilityError}</p>{/if}
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
	.eyebrow {
		margin: 0 0 var(--space-1);
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 650;
	}
	h1 {
		margin: 0;
		font-size: clamp(24px, 3vw, 32px);
		font-weight: 650;
		letter-spacing: -0.03em;
	}
	.lede {
		margin: var(--space-2) 0 0;
		color: var(--text-dim);
	}
	.settings-grid {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: var(--space-3);
		max-width: 760px;
	}
	.settings-card {
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		padding: var(--space-4);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		background: var(--bg-raised);
	}
	.workspace-card {
		grid-column: 1 / -1;
	}
	.capability-card {
		grid-column: 1 / -1;
	}
	.card-head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-3);
	}
	.card-head > :global(svg) {
		flex: none;
		color: var(--brand);
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
		font-size: 10px;
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
	.current-row .pill,
	.workspace-card .pill {
		flex: none;
		padding: 6px 9px;
		font-size: var(--fs-xs);
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
	.capability-toggle {
		position: relative;
		flex: none;
		width: 38px;
		height: 22px;
		padding: 2px;
		border: 1px solid var(--border-strong, var(--border));
		border-radius: 999px;
		background: var(--bg-inset);
		transition: background 120ms ease, border-color 120ms ease;
	}
	.capability-toggle span {
		display: block;
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: var(--text-faint);
		transition: transform 120ms ease, background 120ms ease;
	}
	.capability-toggle.on {
		border-color: var(--brand);
		background: var(--brand);
	}
	.capability-toggle.on span {
		background: var(--on-brand, #fff);
		transform: translateX(16px);
	}
	.capability-toggle:disabled {
		cursor: not-allowed;
		opacity: 0.45;
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
		font-size: 10px;
	}
	.text-button {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		align-self: flex-start;
		color: var(--link);
		font-size: var(--fs-xs);
	}
	.text-button:hover {
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
		color: var(--brand);
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
	@media (max-width: 680px) {
		.settings-grid {
			grid-template-columns: 1fr;
		}
		.workspace-card {
			grid-column: auto;
		}
	}
</style>
