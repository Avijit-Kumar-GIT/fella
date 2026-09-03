<script lang="ts" module>
	// Hand-picked line icons (Lucide geometry, 24x24, currentColor). No package
	// re-inline a `d` string here when a new one is needed.
	export const ICONS = {
		'chevron-right': 'm9 18 6-6-6-6',
		x: 'M18 6 6 18M6 6l12 12',
		plus: 'M5 12h14M12 5v14',
		check: 'M20 6 9 17l-5-5',
		alert:
			'm10.24 4.5-8 14A2 2 0 0 0 4 21.5h16a2 2 0 0 0 1.76-3l-8-14a2 2 0 0 0-3.52 0ZM12 9.5v4M12 17.5h.01',
		'arrow-up-right': 'M7 17 17 7M7 7h10v10',
		folder:
			'M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z',
		search: 'M11 3a8 8 0 1 0 0 16 8 8 0 0 0 0-16ZM21 21l-4.35-4.35',
		'corner-down-left': 'M9 10 4 15l5 5M20 4v7a4 4 0 0 1-4 4H4',
		stop: 'M7 6h10a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1Z'
	} as const;

	export type IconName = keyof typeof ICONS;
</script>

<script lang="ts">
	let {
		name,
		size = 16,
		fill = false
	}: { name: IconName; size?: number; fill?: boolean } = $props();
</script>

<svg
	class="icon"
	width={size}
	height={size}
	viewBox="0 0 24 24"
	fill={fill ? 'currentColor' : 'none'}
	stroke="currentColor"
	stroke-width="1.75"
	stroke-linecap="round"
	stroke-linejoin="round"
	aria-hidden="true"
	focusable="false"
>
	<path d={ICONS[name]} />
</svg>

<style>
	.icon {
		display: block;
		flex: none;
	}
</style>
