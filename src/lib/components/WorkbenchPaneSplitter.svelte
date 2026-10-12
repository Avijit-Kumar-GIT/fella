<script lang="ts">
	interface Props {
		value?: number;
		oncommit: (value: number) => void;
	}

	let { value = $bindable(46), oncommit }: Props = $props();
	let drag = $state<{ pointerId: number; startX: number; startValue: number; width: number } | null>(null);
	const minimum = 32;
	const maximum = 60;

	function clamp(value: number): number {
		return Math.max(minimum, Math.min(maximum, value));
	}

	function update(nextValue: number, commit = false): void {
		value = clamp(nextValue);
		if (commit) oncommit(value);
	}

	function startPointer(event: PointerEvent): void {
		if (event.button !== 0) return;
		const element = event.currentTarget as HTMLElement;
		const width = element.parentElement?.getBoundingClientRect().width ?? 0;
		if (width <= 0) return;
		drag = { pointerId: event.pointerId, startX: event.clientX, startValue: value, width };
		element.setPointerCapture(event.pointerId);
		event.preventDefault();
	}

	function movePointer(event: PointerEvent): void {
		if (!drag || drag.pointerId !== event.pointerId) return;
		update(drag.startValue + ((event.clientX - drag.startX) / drag.width) * 100);
	}

	function finishPointer(event: PointerEvent): void {
		if (!drag || drag.pointerId !== event.pointerId) return;
		drag = null;
		oncommit(value);
	}

	function handleKeydown(event: KeyboardEvent): void {
		let next: number | null = null;
		if (event.key === 'ArrowLeft') next = value - 2;
		else if (event.key === 'ArrowRight') next = value + 2;
		else if (event.key === 'Home') next = minimum;
		else if (event.key === 'End') next = maximum;
		if (next === null) return;
		event.preventDefault();
		update(next, true);
	}
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions (A focusable ARIA separator exposes pointer and arrow-key resizing.) -->
<div
	class="workbench-pane-splitter"
	role="separator"
	tabindex="0"
	aria-label="Resize Conversation and Analysis panes"
	aria-orientation="vertical"
	aria-valuemin={minimum}
	aria-valuemax={maximum}
	aria-valuenow={Math.round(value)}
	aria-valuetext={`Conversation ${Math.round(value)}%, Analysis ${100 - Math.round(value)}%`}
	title="Drag to resize; use the arrow keys"
	data-testid="workbench-pane-splitter"
	onpointerdown={startPointer}
	onpointermove={movePointer}
	onpointerup={finishPointer}
	onpointercancel={finishPointer}
	onkeydown={handleKeydown}
></div>

<style>
	.workbench-pane-splitter {
		position: relative;
		z-index: 2;
		display: flex;
		grid-column: 2;
		grid-row: 1 / -1;
		align-items: stretch;
		justify-content: center;
		min-width: 10px;
		cursor: col-resize;
		touch-action: none;
		outline: none;
	}
	.workbench-pane-splitter::before {
		width: 1px;
		background: var(--border);
		content: '';
		transition: background-color 120ms ease, width 120ms ease;
	}
	.workbench-pane-splitter::after {
		position: absolute;
		top: 50%;
		left: 50%;
		width: 5px;
		height: 34px;
		border: 1px solid var(--border);
		border-radius: 999px;
		background: var(--bg-raised);
		content: '';
		opacity: 0;
		transform: translate(-50%, -50%);
		transition: opacity 120ms ease;
	}
	.workbench-pane-splitter:hover::before,
	.workbench-pane-splitter:focus-visible::before {
		width: 2px;
		background: var(--link);
	}
	.workbench-pane-splitter:hover::after,
	.workbench-pane-splitter:focus-visible::after {
		opacity: 1;
	}
	@media (prefers-reduced-motion: reduce) {
		.workbench-pane-splitter::before,
		.workbench-pane-splitter::after { transition: none; }
	}
</style>
