<script lang="ts">
	import { dispatch, errMsg } from '$lib/commands';
	import { ipc, isTauri, openExternal, pickFolder } from '$lib/ipc';
	import { prefs } from '$lib/prefs.svelte';
	import { session } from '$lib/session.svelte';
	import type { InstalledPack } from '$lib/types';
	import Icon, { type IconName } from './Icon.svelte';

	const PACK_LIBRARY_URL = 'https://github.com/Avijit-Kumar-GIT/fella-extensions';
	const KIND_ORDER = ['theme', 'skill', 'mcp', 'augment'];
	const KIND_LABELS: Record<string, string> = {
		theme: 'Themes',
		skill: 'Skills',
		mcp: 'Connections',
		augment: 'Workspace helpers'
	};

	type PackGroup = { kind: string; label: string; items: InstalledPack[] };

	let busyId = $state<string | null>(null);
	let error = $state<string | null>(null);

	let groups = $derived.by(() => {
		const known = new Set(KIND_ORDER);
		const kinds = [...KIND_ORDER, ...session.packs.map((pack) => pack.kind).filter((kind) => !known.has(kind))];
		return [...new Set(kinds)]
			.map((kind): PackGroup => ({
				kind,
				label: KIND_LABELS[kind] ?? `${kind.charAt(0).toUpperCase()}${kind.slice(1)} packs`,
				items: session.packs.filter((pack) => pack.kind === kind)
			}))
			.filter((group) => group.items.length > 0);
	});
	let enabledCount = $derived(session.packs.filter((pack) => pack.enabled).length);

	function iconFor(kind: string): IconName {
		if (kind === 'theme') return 'asterisk';
		if (kind === 'skill') return 'compose';
		if (kind === 'mcp') return 'file';
		if (kind === 'augment') return 'table';
		return 'asterisk';
	}

	function kindLabel(kind: string): string {
		return KIND_LABELS[kind]?.replace(/s$/, '') ?? kind;
	}

	function sourceLabel(pack: InstalledPack): string {
		return pack.verified ? 'Reviewed pack' : pack.source === 'local' ? 'Added from this computer' : 'Unverified pack';
	}

	async function addPack(): Promise<void> {
		if (!isTauri()) {
			error = 'Adding packs is available in the desktop app.';
			return;
		}
		const path = await pickFolder();
		if (!path) return;
		busyId = '__add__';
		error = null;
		try {
			await ipc.packsAdd(path);
			session.packs = await ipc.packsList();
			await prefs.load();
		} catch (e) {
			error = errMsg(e);
		} finally {
			busyId = null;
		}
	}

	async function togglePack(pack: InstalledPack): Promise<void> {
		if (busyId) return;
		busyId = pack.id;
		error = null;
		try {
			await ipc.packsSetEnabled(pack.id, !pack.enabled);
			session.packs = await ipc.packsList();
			await prefs.load();
		} catch (e) {
			error = errMsg(e);
		} finally {
			busyId = null;
		}
	}

	async function removePack(pack: InstalledPack): Promise<void> {
		if (busyId) return;
		busyId = pack.id;
		error = null;
		try {
			await ipc.packsRemove(pack.id);
			session.packs = await ipc.packsList();
			await prefs.load();
		} catch (e) {
			error = errMsg(e);
		} finally {
			busyId = null;
		}
	}

	function connectPack(pack: InstalledPack): void {
		session.setWorkspaceView('ask');
		void dispatch(`/connect ${pack.id}`);
	}

	function browse(): void {
		void openExternal(PACK_LIBRARY_URL);
	}
</script>

<section class="packs-page" aria-labelledby="packs-title">
	<header class="page-head">
		<div>
			<p class="eyebrow">Customize</p>
			<h1 id="packs-title">Packs</h1>
			<p class="lede">Small additions that change how Fella looks, thinks, or connects.</p>
		</div>
		<div class="head-actions">
			<button class="pill ghost" type="button" onclick={browse}>
				Browse pack library <Icon name="arrow-up-right" size={13} />
			</button>
			<button class="pill primary" type="button" disabled={busyId !== null} onclick={() => void addPack()}>
				<Icon name="plus" size={14} /> {busyId === '__add__' ? 'Adding…' : 'Add pack'}
			</button>
		</div>
	</header>

	<div class="summary" aria-label="Pack summary">
		<span><strong>{session.packs.length}</strong> installed</span>
		<span><strong>{enabledCount}</strong> enabled</span>
		<span>Optional and reversible</span>
	</div>

	{#if error}
		<div class="error" role="alert"><Icon name="alert" size={14} /> {error}</div>
	{/if}

	{#if !groups.length}
		<div class="empty-state">
			<div class="empty-icon"><Icon name="asterisk" size={21} /></div>
			<h2>Make Fella yours</h2>
			<p>
				Packs are optional additions for the way you work. Add a theme, a skill, a workspace helper,
				or a connection from a local folder.
			</p>
			<div class="empty-actions">
				<button class="pill primary" type="button" disabled={busyId !== null} onclick={() => void addPack()}>
					<Icon name="plus" size={14} /> Add a local pack
				</button>
				<button class="pill ghost" type="button" onclick={browse}>Browse the pack library</button>
			</div>
		</div>
	{:else}
		<div class="pack-groups">
			{#each groups as group (group.kind)}
				<section class="pack-group" aria-labelledby={`pack-group-${group.kind}`}>
					<header class="group-head">
						<div>
							<h2 id={`pack-group-${group.kind}`}>{group.label}</h2>
							<p>{group.items.length} installed</p>
						</div>
						<span>{group.items.filter((pack) => pack.enabled).length} enabled</span>
					</header>
					<div class="pack-list">
						{#each group.items as pack (pack.id)}
							<article class="pack-row" class:off={!pack.enabled}>
								<div class="pack-mark"><Icon name={iconFor(pack.kind)} size={16} /></div>
								<div class="pack-copy">
									<div class="pack-title">
										<strong>{pack.name}</strong>
										<span class="kind">{kindLabel(pack.kind)}</span>
										<span class:verified={pack.verified} class="trust">{pack.verified ? 'Reviewed' : 'Local'}</span>
									</div>
									<p>{pack.description || 'No description provided.'}</p>
									<div class="pack-meta">
										<code>{pack.id}</code><span>v{pack.version}</span><span>{sourceLabel(pack)}</span>
										{#if pack.needs_token}<span class="needs">Needs connection</span>{/if}
									</div>
								</div>
								<div class="pack-actions">
									{#if pack.kind === 'mcp' && pack.needs_token}
										<button class="text-action" type="button" onclick={() => connectPack(pack)}>Connect</button>
									{/if}
									<button
										class="state-action"
										class:on={pack.enabled}
										type="button"
										disabled={busyId !== null}
										onclick={() => void togglePack(pack)}
									>
										{pack.enabled ? 'Enabled' : 'Enable'}
									</button>
									<button
										class="icon-action danger"
										type="button"
										aria-label={`Remove ${pack.name}`}
										title="Remove pack"
										disabled={busyId !== null}
										onclick={() => void removePack(pack)}
									>
										<Icon name="x" size={14} />
									</button>
								</div>
							</article>
						{/each}
					</div>
				</section>
			{/each}
		</div>
	{/if}
</section>

<style>
	.packs-page {
		flex: 1;
		min-height: 0;
		width: 100%;
		max-width: var(--content-max);
		margin: 0 auto;
		padding: var(--space-6) var(--pad) var(--space-6);
		overflow: auto;
	}
	.page-head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-5);
		margin-bottom: var(--space-5);
	}
	.eyebrow {
		margin: 0 0 var(--space-1);
		color: var(--text-faint);
		font-size: var(--fs-xs);
		font-weight: 650;
		letter-spacing: 0.01em;
	}
	h1,
	h2 {
		margin: 0;
		font-weight: 620;
		letter-spacing: -0.03em;
	}
	h1 {
		font-size: clamp(24px, 3vw, 32px);
		line-height: 1.15;
	}
	h2 {
		font-size: var(--fs-lg);
		line-height: 1.3;
	}
	.lede {
		margin: var(--space-2) 0 0;
		color: var(--text-dim);
	}
	.head-actions,
	.empty-actions,
	.pack-actions {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: var(--space-2);
	}
	.head-actions {
		justify-content: flex-end;
	}
	.summary {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: var(--space-4);
		margin-bottom: var(--space-5);
		padding-bottom: var(--space-3);
		border-bottom: 1px solid var(--border);
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.summary span {
		display: inline-flex;
		align-items: baseline;
		gap: 5px;
	}
	.summary strong {
		color: var(--text);
		font-size: var(--fs-sm);
		font-weight: 600;
	}
	.error {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin-bottom: var(--space-4);
		padding: var(--space-2) var(--space-3);
		border-left: 2px solid var(--err);
		background: color-mix(in srgb, var(--err) 7%, transparent);
		color: var(--err);
		font-size: var(--fs-sm);
	}
	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		min-height: 360px;
		padding: var(--space-6);
		text-align: center;
	}
	.empty-icon {
		display: grid;
		place-items: center;
		width: 42px;
		height: 42px;
		margin-bottom: var(--space-3);
		border-radius: 50%;
		background: color-mix(in srgb, var(--brand) 10%, transparent);
		color: var(--brand);
	}
	.empty-state h2 {
		font-size: var(--fs-xl);
	}
	.empty-state p {
		max-width: 48ch;
		margin: var(--space-2) 0 var(--space-4);
		color: var(--text-dim);
	}
	.pack-groups {
		display: grid;
		gap: var(--space-6);
	}
	.pack-group {
		min-width: 0;
	}
	.group-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: var(--space-3);
		margin-bottom: var(--space-2);
	}
	.group-head p,
	.group-head > span {
		margin: 2px 0 0;
		color: var(--text-faint);
		font-size: var(--fs-xs);
	}
	.group-head > span {
		font-family: var(--mono);
		font-size: 10px;
	}
	.pack-list {
		border-top: 1px solid var(--border);
	}
	.pack-row {
		display: grid;
		grid-template-columns: 32px minmax(0, 1fr) auto;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-3) 0;
		border-bottom: 1px solid var(--border);
	}
	.pack-mark {
		display: grid;
		place-items: center;
		width: 32px;
		height: 32px;
		border-radius: var(--radius-sm);
		background: color-mix(in srgb, var(--brand) 10%, transparent);
		color: var(--brand);
	}
	.pack-row.off .pack-mark {
		background: var(--bg-inset);
		color: var(--text-faint);
	}
	.pack-copy {
		min-width: 0;
	}
	.pack-title {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 6px;
	}
	.pack-title strong {
		font-size: var(--fs-sm);
		font-weight: 600;
	}
	.kind,
	.trust {
		padding: 1px 5px;
		border: 1px solid var(--border);
		border-radius: var(--radius-chip);
		color: var(--text-faint);
		font-size: 10px;
	}
	.trust.verified {
		border-color: color-mix(in srgb, var(--ok) 30%, var(--border));
		color: var(--ok);
	}
	.pack-copy p {
		margin: 3px 0 0;
		color: var(--text-dim);
		font-size: var(--fs-sm);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.pack-meta {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 7px;
		margin-top: 5px;
		color: var(--text-faint);
		font-size: 10px;
	}
	.pack-meta code {
		font-family: var(--mono);
	}
	.pack-meta span + span::before {
		content: '·';
		margin-right: 7px;
	}
	.pack-meta .needs {
		color: var(--warn);
	}
	.text-action,
	.state-action,
	.icon-action {
		font-size: var(--fs-xs);
	}
	.text-action {
		padding: 5px 4px;
		color: var(--link);
	}
	.text-action:hover {
		text-decoration: underline;
	}
	.state-action {
		padding: 5px 8px;
		border: 1px solid var(--border);
		border-radius: var(--radius-chip);
		color: var(--text-dim);
	}
	.state-action:hover:not(:disabled) {
		border-color: var(--border-strong);
		background: var(--bg-inset);
		color: var(--text);
	}
	.state-action.on {
		border-color: color-mix(in srgb, var(--brand) 30%, var(--border));
		color: var(--brand);
	}
	.icon-action {
		display: grid;
		place-items: center;
		width: 26px;
		height: 26px;
		border-radius: var(--radius-chip);
		color: var(--text-faint);
	}
	.icon-action:hover:not(:disabled) {
		background: var(--bg-inset);
		color: var(--text);
	}
	.icon-action.danger:hover:not(:disabled) {
		color: var(--err);
	}
	button:disabled {
		cursor: default;
		opacity: 0.55;
	}
	@media (max-width: 760px) {
		.packs-page {
			padding-inline: var(--space-4);
		}
		.page-head {
			flex-direction: column;
		}
		.head-actions {
			justify-content: flex-start;
		}
		.pack-row {
			grid-template-columns: 32px minmax(0, 1fr);
			align-items: start;
		}
		.pack-actions {
			grid-column: 2;
			justify-content: flex-start;
		}
	}
</style>
