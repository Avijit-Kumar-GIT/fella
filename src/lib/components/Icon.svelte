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
		minus: 'M5 12h14',
		square: 'M5 5h14v14H5z',
		folder:
			'M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z',
		search: 'M11 3a8 8 0 1 0 0 16 8 8 0 0 0 0-16ZM21 21l-4.35-4.35',
		'corner-down-left': 'M9 10 4 15l5 5M20 4v7a4 4 0 0 1-4 4H4',
		stop: 'M7 6h10a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1Z',
		asterisk: 'M12 6v12M17.196 9 6.804 15M6.804 9l10.392 6',
		info: 'M2 12a10 10 0 1 0 20 0 10 10 0 1 0-20 0M12 16v-4M12 8h.01',
		sun: 'M12 3v2M12 19v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M3 12h2M19 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42M12 7a5 5 0 1 0 0 10 5 5 0 0 0 0-10Z',
		moon: 'M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79Z',
		monitor: 'M4 5h16v11H4zM8 21h8M12 16v5',
		panel: 'M3 4h18v16H3zM9 4v16',
		settings:
			'M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8ZM4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M3 12h2M19 12h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41M12 3v2M12 19v2',
		 table: 'M3 5h18v14H3zM3 10h18M9 5v14M16 5v14',
		file: 'M6 3h8l4 4v14H6zM14 3v5h5M8 12h8M8 16h6',
		home: 'm3 10 9-7 9 7v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1Z M9 21v-7h6v7',
		bookmark: 'M6 3h12v18l-6-4-6 4Z',
		compose:
			'M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7M18.375 2.625a2.121 2.121 0 1 1 3 3L12 15l-4 1 1-4Z',
		pencil: 'M12 20h9M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4Z'
	} as const;

	export type IconName = keyof typeof ICONS;
	/** 12px compact detail, 16px standard UI, 20px display mark. */
	export type IconSize = 12 | 16 | 20;
</script>

<script lang="ts">
	let {
		name,
		size = 16,
		fill = false
	}: { name: IconName; size?: IconSize; fill?: boolean } = $props();
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
		display: inline-block;
		flex: none;
		vertical-align: middle;
	}
</style>
