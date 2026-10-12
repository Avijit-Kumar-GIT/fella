<script lang="ts">
	import { workbenchLayout, WORKBENCH_GROUPS, WORKBENCH_PANELS } from '$lib/workbench-layout.svelte';
	import { session } from '$lib/session.svelte';
	import Icon from './Icon.svelte';

	let hasCompanion = $derived(Boolean(session.activeChat.companionPane));
	let placements = $derived(workbenchLayout.placements(hasCompanion));

	function changeGroup(id: (typeof WORKBENCH_PANELS)[number]['id'], event: Event): void {
		const group = (event.currentTarget as HTMLSelectElement).value;
		if (group === 'analysis' || group === 'conversation') workbenchLayout.setGroup(id, group);
	}
</script>

<details class="workbench-panel-picker">
	<summary aria-label="Configure Workbench panels" title="Show, order, and group Workbench panels">
		<Icon name="sliders" size={14} />
		<span>Panels</span>
	</summary>
	<div class="panel-picker-popover" role="region" aria-label="Workbench panel settings">
		<div class="panel-picker-heading">
			<div>
				<strong>Workbench panels</strong>
				<p>Arrange Focus, the Investigation, and an optional chart or source preview for this Workspace.</p>
			</div>
			<button type="button" class="reset-layout" onclick={() => workbenchLayout.reset()}>Reset</button>
		</div>
		<div class="panel-picker-list">
			{#each placements as placement (placement.id)}
				{@const definition = WORKBENCH_PANELS.find((panel) => panel.id === placement.id)!}
				<div class="panel-setting" data-panel-setting={placement.id}>
					<label class="panel-setting-toggle">
						<input
							type="checkbox"
							checked={placement.visibleNow}
							aria-label={`Show ${definition.label} panel`}
							disabled={placement.visibleNow && !workbenchLayout.canHide(placement.id, hasCompanion)}
							onchange={() => workbenchLayout.toggle(placement.id, hasCompanion)}
						/>
						<span class="panel-setting-copy">
							<strong>{definition.label}</strong>
							<small>{definition.description}</small>
						</span>
					</label>
					<div class="panel-setting-actions">
						<select aria-label={`Group ${definition.label}`} value={placement.group} onchange={(event) => changeGroup(placement.id, event)}>
							{#each WORKBENCH_GROUPS as group (group.id)}
								<option value={group.id}>{group.label}</option>
							{/each}
						</select>
						<button
							type="button"
							aria-label={`Move ${definition.label} up`}
							title="Move up in this group"
							disabled={!workbenchLayout.canMove(placement.id, -1)}
							onclick={() => workbenchLayout.move(placement.id, -1)}
						><Icon name="chevron-up" size={14} /></button>
						<button
							type="button"
							aria-label={`Move ${definition.label} down`}
							title="Move down in this group"
							disabled={!workbenchLayout.canMove(placement.id, 1)}
							onclick={() => workbenchLayout.move(placement.id, 1)}
						><Icon name="chevron-down" size={14} /></button>
					</div>
				</div>
			{/each}
		</div>
		<p class="panel-picker-footnote">Questions, Data &amp; context, runs, evidence, and visualizations stay linked inside the Investigation; the preview opens one chart or source from the same Workspace.</p>
	</div>
</details>

<style>
	.workbench-panel-picker {
		position: relative;
		flex: none;
		font-size: var(--fs-xs);
	}
	.workbench-panel-picker > summary {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		min-height: 32px;
		padding: 0 9px;
		border: 1px solid var(--border);
		border-radius: var(--radius);
		background: var(--bg);
		color: var(--text-dim);
		font-weight: 550;
		list-style: none;
		cursor: pointer;
	}
	.workbench-panel-picker > summary::-webkit-details-marker { display: none; }
	.workbench-panel-picker > summary:hover,
	.workbench-panel-picker[open] > summary {
		background: var(--bg-raised);
		color: var(--text);
	}
	.workbench-panel-picker > summary:focus-visible,
	.panel-picker-popover button:focus-visible,
	.panel-picker-popover select:focus-visible,
	.panel-setting-toggle input:focus-visible {
		outline: 2px solid var(--link);
		outline-offset: 2px;
	}
	.panel-picker-popover {
		position: absolute;
		z-index: 30;
		top: calc(100% + 6px);
		right: 0;
		width: min(360px, calc(100vw - 24px));
		padding: var(--space-3);
		border: 1px solid var(--border);
		border-radius: 12px;
		background: var(--bg-raised);
		box-shadow: var(--shadow-md);
	}
	.panel-picker-heading,
	.panel-setting,
	.panel-setting-toggle,
	.panel-setting-actions {
		display: flex;
		align-items: center;
	}
	.panel-picker-heading {
		justify-content: space-between;
		gap: var(--space-3);
		padding: 0 2px var(--space-2);
		border-bottom: 1px solid var(--border);
	}
	.panel-picker-heading strong { font-size: var(--fs-sm); }
	.panel-picker-heading p,
	.panel-picker-footnote {
		margin: 3px 0 0;
		color: var(--text-faint);
		line-height: 1.4;
	}
	.reset-layout {
		flex: none;
		border: 0;
		background: transparent;
		color: var(--link);
		font: inherit;
		cursor: pointer;
	}
	.panel-picker-list { display: grid; }
	.panel-setting {
		justify-content: space-between;
		gap: var(--space-2);
		padding: var(--space-2) 2px;
		border-bottom: 1px solid var(--border);
	}
	.panel-setting-toggle { min-width: 0; gap: 9px; cursor: pointer; }
	.panel-setting-toggle input {
		flex: none;
		width: 14px;
		height: 14px;
		margin: 0;
		accent-color: var(--brand);
	}
	.panel-setting-copy { display: grid; gap: 2px; min-width: 0; }
	.panel-setting-copy strong { font-weight: 550; }
	.panel-setting-copy small {
		color: var(--text-faint);
		font-size: 11px;
		line-height: 1.35;
	}
	.panel-setting-actions { flex: none; gap: 2px; }
	.panel-setting-actions select {
		max-width: 108px;
		min-height: 26px;
		padding: 0 4px;
		border: 1px solid var(--border);
		border-radius: 5px;
		background: var(--bg);
		color: var(--text-dim);
		font: inherit;
	}
	.panel-setting-actions button {
		display: grid;
		place-items: center;
		width: 26px;
		height: 26px;
		padding: 0;
		border: 0;
		border-radius: 5px;
		background: transparent;
		color: var(--text-dim);
		cursor: pointer;
	}
	.panel-setting-actions button:hover:not(:disabled) { background: var(--bg-inset); color: var(--text); }
	.panel-setting-actions button:disabled { color: var(--text-faint); opacity: 0.5; cursor: default; }
	.panel-picker-footnote { margin-top: var(--space-2); padding-inline: 2px; }
	@media (max-width: 620px) {
		.workbench-panel-picker > summary span { display: none; }
		.workbench-panel-picker > summary { width: 32px; justify-content: center; padding: 0; }
	}
</style>
