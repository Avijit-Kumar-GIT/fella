<script lang="ts" module>
	export const ICON_NAMES = [
		'chevron-right',
		'chevron-up',
		'chevron-down',
		'sliders',
		'x',
		'plus',
		'more-horizontal',
		'check',
		'alert',
		'arrow-up-right',
		'refresh',
		'minus',
		'square',
		'folder',
		'search',
		'corner-down-left',
		'ask',
		'repository',
		'project',
		'stop',
		'asterisk',
		'info',
		'sun',
		'moon',
		'panel',
		'settings',
		'table',
		'file',
		'bookmark',
		'pencil',
		'download'
	] as const;

	export type IconName = (typeof ICON_NAMES)[number];
	/** 12px metadata, 14px compact controls, 16px standard UI, 20px display. */
	export type IconSize = 12 | 14 | 16 | 20;
</script>

<script lang="ts">
	import {
		Asterisk,
		ArrowUpRight,
		Bookmark,
		Check,
		ChevronRight,
		ChevronUp,
		ChevronDown,
		CornerDownLeft,
		Download,
		Ellipsis,
		FileText,
		Folder,
		FolderGit2,
		FolderKanban,
		Info,
		MessageSquare,
		Minus,
		Moon,
		PanelLeft,
		Pencil,
		Plus,
		RefreshCw,
		Search,
		SlidersHorizontal,
		Settings,
		Square,
		SquareStop,
		Sun,
		Table,
		TriangleAlert,
		X,
		type LucideIcon
	} from '@lucide/svelte';

	const ICONS: Record<IconName, LucideIcon> = {
		'chevron-right': ChevronRight,
		'chevron-up': ChevronUp,
		'chevron-down': ChevronDown,
		sliders: SlidersHorizontal,
		x: X,
		plus: Plus,
		'more-horizontal': Ellipsis,
		check: Check,
		alert: TriangleAlert,
		'arrow-up-right': ArrowUpRight,
		refresh: RefreshCw,
		minus: Minus,
		square: Square,
		folder: Folder,
		search: Search,
		'corner-down-left': CornerDownLeft,
		ask: MessageSquare,
		repository: FolderGit2,
		project: FolderKanban,
		stop: SquareStop,
		asterisk: Asterisk,
		info: Info,
		sun: Sun,
		moon: Moon,
		panel: PanelLeft,
		settings: Settings,
		table: Table,
		file: FileText,
		bookmark: Bookmark,
		pencil: Pencil,
		download: Download
	};

	let { name, size = 16 }: { name: IconName; size?: IconSize } = $props();
	const IconComponent = $derived(ICONS[name]);
</script>

<IconComponent
	class="icon"
	{size}
	strokeWidth={size === 12 ? 1.8 : 2}
	aria-hidden="true"
	focusable="false"
/>

<style>
	:global(svg.icon) {
		display: inline-block;
		flex: none;
		vertical-align: middle;
	}
</style>
