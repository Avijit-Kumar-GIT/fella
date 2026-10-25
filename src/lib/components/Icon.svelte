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
		'columns-2',
		'panel',
		'panel-right',
		'layout-grid',
		'settings',
		'table',
		'file',
		'bookmark',
		'globe',
		'notebook-pen',
		'arrow-up-down',
		'pencil',
		'download',
		'arrow-up', 'arrow-down', 'user', 'database', 'flask', 'copy'
	] as const;

	export type IconName = (typeof ICON_NAMES)[number];
	/** 12px metadata, 14px compact controls, 16px standard UI, 20px display. */
	export type IconSize = 9 | 10 | 11 | 12 | 13 | 14 | 15 | 16 | 18 | 19 | 20 | 24;
</script>

<script lang="ts">
	import {
		ArrowUp, ArrowDown, UserRound, Database, FlaskConical, Copy,
		Asterisk,
		ArrowUpDown,
		ArrowUpRight,
		Bookmark,
		Check,
		Columns2,
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
		Globe,
		Info,
		LayoutGrid,
		MessageSquare,
		Minus,
		Moon,
		NotebookPen,
		PanelLeft,
		PanelRight,
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
		'panel-right': PanelRight,
		'columns-2': Columns2,
		'layout-grid': LayoutGrid,
		settings: Settings,
		table: Table,
		file: FileText,
		bookmark: Bookmark,
		globe: Globe,
		'notebook-pen': NotebookPen,
		'arrow-up-down': ArrowUpDown,
		pencil: Pencil,
		download: Download,
		'arrow-up': ArrowUp, 'arrow-down': ArrowDown, user: UserRound, database: Database, flask: FlaskConical, copy: Copy
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
