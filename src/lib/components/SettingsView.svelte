<script lang="ts">
	import { dispatch, openFolder } from '$lib/commands';
	import { ipc, isTauri } from '$lib/ipc';
	import { prefs, type Appearance } from '$lib/prefs.svelte';
	import { session } from '$lib/session.svelte';
	import Icon from './Icon.svelte';
	import ProviderIcon from './ProviderIcon.svelte';

	const appearances: { id: Appearance; label: string; detail: string }[] = [
		{ id: 'system', label: 'System', detail: 'Follow your computer' },
		{ id: 'light', label: 'Light', detail: 'Bright workspace' },
		{ id: 'dark', label: 'Dark', detail: 'Low light workspace' }
	];

	let currentProvider = $derived(session.settings?.provider ?? '');
	let currentModel = $derived(session.model || session.settings?.model || '');
	let provider = $derived(session.providers.find((item) => item.id === currentProvider));
	let workspace = $derived(session.catalog.workspace);

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
				{#if provider}<ProviderIcon providerId={provider.id} size={22} />{/if}
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
							<ProviderIcon providerId={item.id} size={17} />
							<span>{item.display}</span>
							<small>{item.id === currentProvider ? 'Current' : item.authed ? 'Connected' : 'Connect'}</small>
						</button>
					{/each}
				</div>
			{/if}
			<button class="text-button" type="button" onclick={() => void refreshSettings()}>
				<Icon name="check" size={12} /> Refresh connection status
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
						{#if prefs.appearance === option.id}<Icon name="check" size={14} />{/if}
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
				<Icon name="folder" size={13} /> {workspace ? 'Change folder' : 'Choose a folder'}
			</button>
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
		font-weight: 620;
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
		font-weight: 620;
	}
	.card-head p {
		max-width: 42ch;
		margin: 4px 0 0;
		color: var(--text-faint);
		font-size: var(--fs-xs);
		line-height: 1.45;
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
		font-weight: 560;
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
		font-weight: 560;
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
