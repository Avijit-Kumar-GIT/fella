<script lang="ts">
	import { Select as BitsSelect } from 'bits-ui';
	import { cn } from '$lib/utils';
	import Icon from '$lib/components/Icon.svelte';

	type SelectOption = { value: string; label: string; disabled?: boolean };

	const EMPTY_VALUE = '__fella_select_empty__';

	type Props = {
		id?: string;
		value?: string;
		options: SelectOption[];
		placeholder?: string;
		emptyOptionLabel?: string;
		ariaLabel?: string;
		disabled?: boolean;
		class?: string;
		onValueChange?: (value: string) => void;
	};

	let {
		id,
		value = $bindable(''),
		options,
		placeholder = 'Choose an option',
		emptyOptionLabel,
		ariaLabel,
		disabled = false,
		class: className,
		onValueChange
	}: Props = $props();

	let menuOptions = $derived(
		emptyOptionLabel
			? [{ value: EMPTY_VALUE, label: emptyOptionLabel }, ...options]
			: options
	);
	let selectedValue = $derived(value || (emptyOptionLabel ? EMPTY_VALUE : ''));

	function updateValue(next: string): void {
		value = next === EMPTY_VALUE ? '' : next;
		onValueChange?.(value);
	}
</script>

<BitsSelect.Root
	type="single"
	value={selectedValue}
	onValueChange={updateValue}
	items={menuOptions}
	disabled={disabled}
>
	<BitsSelect.Trigger
		{id}
		class={cn('fella-ui-select fella-ui-select-trigger', className)}
		aria-label={ariaLabel}
		data-slot="select"
	>
		<BitsSelect.Value {placeholder} />
		<Icon name="chevron-down" size={14} />
	</BitsSelect.Trigger>
	<BitsSelect.Portal>
		<BitsSelect.Content class="fella-ui-menu-content fella-ui-select-content" side="bottom" align="start" sideOffset={5}>
			<BitsSelect.Viewport>
				{#each menuOptions as option (option.value)}
					<BitsSelect.Item
						class="fella-ui-select-option"
						value={option.value}
						label={option.label}
						disabled={option.disabled}
					>
						{#snippet children({ selected })}
							<span>{option.label}</span>
							{#if selected}<Icon name="check" size={14} />{/if}
						{/snippet}
					</BitsSelect.Item>
				{/each}
			</BitsSelect.Viewport>
		</BitsSelect.Content>
	</BitsSelect.Portal>
</BitsSelect.Root>

<style>
	:global(.fella-ui-select-trigger) {
		display: flex;
		min-width: 0;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		text-align: left;
		cursor: pointer;
	}
	:global(.fella-ui-select-trigger > span:first-child) {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	:global(.fella-ui-select-trigger > svg) { flex: none; color: var(--text-faint); }
	:global(.fella-ui-select-trigger:hover:not(:disabled)),
	:global(.fella-ui-select-trigger[data-state='open']) {
		border-color: var(--border-strong);
		background: var(--sidebar-hover, var(--bg-inset));
	}
	:global(.fella-ui-select-trigger:focus-visible) { outline: none; box-shadow: var(--focus-ring); }
	:global(.fella-ui-select-trigger:disabled) { cursor: not-allowed; opacity: 0.55; }
	:global(.fella-ui-select-content) {
		width: var(--bits-select-anchor-width, 220px);
		max-width: min(440px, calc(100vw - 24px));
		max-height: min(360px, calc(100vh - 32px));
		overflow-y: auto;
		overscroll-behavior: contain;
	}
	:global(.fella-ui-select-option) {
		display: flex;
		min-height: 34px;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		padding: 7px 9px;
		border-radius: calc(var(--radius-sm) - 2px);
		color: var(--text-dim);
		font-size: var(--fs-sm);
		line-height: 1.4;
		cursor: default;
	}
	:global(.fella-ui-select-option[data-highlighted]),
	:global(.fella-ui-select-option[data-selected]) {
		background: var(--bg-inset);
		color: var(--text);
	}
	:global(.fella-ui-select-option[data-disabled]) { color: var(--text-faint); opacity: 0.55; }
	:global(.fella-ui-select-option > span:first-child) { min-width: 0; overflow-wrap: anywhere; }
	:global(.fella-ui-select-option > svg) { flex: none; color: var(--brand-icon); }
</style>
