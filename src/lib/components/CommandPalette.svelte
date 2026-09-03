<script lang="ts">
	import { COMMAND_DESCRIPTIONS, SLASH_COMMANDS } from '$lib/commands';
	import { fadeQuick, pop } from '$lib/motion';
	import Icon from './Icon.svelte';

	let { open = $bindable(false), onpick }: { open?: boolean; onpick: (cmd: string) => void } =
		$props();

	let query = $state('');
	let sel = $state(0);
	let input: HTMLInputElement | undefined = $state();
	let dialog: HTMLDivElement | undefined = $state();
	let returnFocus: HTMLElement | null = null;

	let matches = $derived(
		SLASH_COMMANDS.filter((c) => c.includes(query.replace(/^\//, '').toLowerCase()))
	);

	$effect(() => {
		if (open) {
			returnFocus = document.activeElement as HTMLElement | null;
			query = '';
			sel = 0;
			queueMicrotask(() => input?.focus());
		} else if (returnFocus) {
			// Put focus back where it was when the palette opened.
			returnFocus.focus();
			returnFocus = null;
		}
	});

	function close() {
		open = false;
	}

	function key(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.stopPropagation();
			close();
		} else if (e.key === 'ArrowDown') {
			sel = (sel + 1) % Math.max(matches.length, 1);
			e.preventDefault();
		} else if (e.key === 'ArrowUp') {
			sel = (sel - 1 + matches.length) % Math.max(matches.length, 1);
			e.preventDefault();
		} else if (e.key === 'Enter' && matches[sel]) {
			onpick(matches[sel]);
			close();
		}
	}

	// Keep Tab inside the dialog while it's open; Escape closes from anywhere in it.
	function trap(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.stopPropagation();
			close();
			return;
		}
		if (e.key !== 'Tab' || !dialog) return;
		const focusable = dialog.querySelectorAll<HTMLElement>(
			'a[href], button:not([disabled]), input, [tabindex]:not([tabindex="-1"])'
		);
		if (focusable.length === 0) return;
		const first = focusable[0];
		const last = focusable[focusable.length - 1];
		const active = document.activeElement;
		if (e.shiftKey && active === first) {
			e.preventDefault();
			last.focus();
		} else if (!e.shiftKey && active === last) {
			e.preventDefault();
			first.focus();
		}
	}
</script>

{#if open}
	<div class="scrim">
		<button
			type="button"
			class="backdrop"
			aria-label="Close commands"
			onclick={close}
			transition:fadeQuick
		></button>
		<div
			class="palette"
			bind:this={dialog}
			role="dialog"
			aria-modal="true"
			aria-label="Commands"
			tabindex="-1"
			onkeydown={trap}
			transition:pop
		>
			<div class="search">
				<Icon name="search" size={15} />
				<input
					bind:this={input}
					bind:value={query}
					onkeydown={key}
					placeholder="Search commands…"
					spellcheck="false"
					aria-label="Search commands"
				/>
			</div>
			<ul>
				{#each matches as m, i (m)}
					<li class:sel={i === sel}>
						<button class="rowbtn" type="button" onclick={() => { onpick(m); close(); }}>
							<span class="name">{m}</span>
							<span class="desc">{COMMAND_DESCRIPTIONS[m] ?? ''}</span>
						</button>
					</li>
				{/each}
				{#if matches.length === 0}
					<li class="empty">no match</li>
				{/if}
			</ul>
		</div>
	</div>
{/if}

<style>
	.scrim {
		position: fixed;
		inset: 0;
		display: flex;
		justify-content: center;
		align-items: flex-start;
		padding-top: 12vh;
		z-index: 50;
	}
	.backdrop {
		position: fixed;
		inset: 0;
		border: none;
		padding: 0;
		background: color-mix(in srgb, var(--bg) 55%, transparent);
		cursor: default;
	}
	.palette {
		position: relative;
		width: min(480px, 92vw);
		background: var(--bg-raised);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		box-shadow: var(--shadow-pop);
		overflow: hidden;
	}
	.search {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: 0 var(--space-3);
		border-bottom: 1px solid var(--border);
		color: var(--text-faint);
	}
	input {
		flex: 1;
		border: none;
		background: transparent;
		color: var(--text);
		font: inherit;
		padding: var(--space-3) 0;
		outline: none;
	}
	input::placeholder {
		color: var(--text-faint);
	}
	input:focus-visible {
		box-shadow: none;
	}
	ul {
		list-style: none;
		margin: 0;
		padding: var(--space-1);
		max-height: 50vh;
		overflow-y: auto;
	}
	/* rows use the global .rowbtn primitive; selection is marked on the <li> */
	li.sel :global(.rowbtn) {
		background: var(--bg-inset);
	}
	.name {
		color: var(--accent);
	}
	.desc {
		color: var(--text-faint);
		font-size: var(--fs-sm);
	}
	.empty {
		padding: var(--space-2) var(--space-3);
		color: var(--text-faint);
	}
</style>
