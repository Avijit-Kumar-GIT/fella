export type WorkbenchPanelId = 'conversation' | 'investigation' | 'companion';
export type WorkbenchPanelGroup = 'conversation' | 'analysis';

export interface WorkbenchPanelPreference {
	id: WorkbenchPanelId;
	visible: boolean | null;
	group: WorkbenchPanelGroup;
	order: number;
}

export interface WorkbenchPanelPlacement extends WorkbenchPanelPreference {
	visibleNow: boolean;
	groupPosition: number;
	groupCount: number;
	groupRowSpan: number;
	stackPosition: number;
	firstInGroup: boolean;
}

const STORAGE_KEY = 'fella:workbench-layout:v1';
const DEFAULT_CONVERSATION_SHARE = 46;
const MIN_CONVERSATION_SHARE = 32;
const MAX_CONVERSATION_SHARE = 60;

export const WORKBENCH_GROUPS: { id: WorkbenchPanelGroup; label: string }[] = [
	{ id: 'conversation', label: 'Conversation' },
	{ id: 'analysis', label: 'Analysis' }
];

export const WORKBENCH_PANELS: {
	id: WorkbenchPanelId;
	label: string;
	description: string;
	default: WorkbenchPanelPreference;
}[] = [
	{
		id: 'conversation',
		label: 'Conversation',
		description: 'The request, discussion, and analysis handoff.',
		default: { id: 'conversation', visible: true, group: 'conversation', order: 0 }
	},
	{
		id: 'investigation',
		label: 'Investigation',
		description: 'Brief, Questions, coverage, runs, Findings, and results.',
		default: { id: 'investigation', visible: true, group: 'analysis', order: 0 }
	},
	{
		id: 'companion',
		label: 'Chart or source preview',
		description: 'A chart or file opened from the conversation.',
		default: { id: 'companion', visible: null, group: 'analysis', order: 1 }
	}
];

function copyDefaults(): WorkbenchPanelPreference[] {
	return WORKBENCH_PANELS.map(({ default: preference }) => ({ ...preference }));
}

function readLayout(): WorkbenchPanelPreference[] {
	if (typeof localStorage === 'undefined') return copyDefaults();
	try {
		const parsed: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null');
		if (!parsed || typeof parsed !== 'object') return copyDefaults();
		const saved = (parsed as { panels?: unknown }).panels;
		if (!Array.isArray(saved)) return copyDefaults();
		const preferences = copyDefaults().map((fallback) => {
			const candidate = saved.find((item) => item && typeof item === 'object' && (item as { id?: unknown }).id === fallback.id);
			if (!candidate || typeof candidate !== 'object') return fallback;
			const value = candidate as Record<string, unknown>;
			return {
				...fallback,
				visible: typeof value.visible === 'boolean' ? value.visible : fallback.visible,
				group: value.group === 'conversation' || value.group === 'analysis' ? value.group : fallback.group,
				order: typeof value.order === 'number' && Number.isFinite(value.order) ? value.order : fallback.order
			};
		});
		return normalizeLayout(preferences);
	} catch {
		return copyDefaults();
	}
}

function readConversationShare(): number {
	if (typeof localStorage === 'undefined') return DEFAULT_CONVERSATION_SHARE;
	try {
		const parsed: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null');
		if (!parsed || typeof parsed !== 'object') return DEFAULT_CONVERSATION_SHARE;
		const value = (parsed as { conversationShare?: unknown }).conversationShare;
		if (typeof value !== 'number' || !Number.isFinite(value)) return DEFAULT_CONVERSATION_SHARE;
		return Math.max(MIN_CONVERSATION_SHARE, Math.min(MAX_CONVERSATION_SHARE, value));
	} catch {
		return DEFAULT_CONVERSATION_SHARE;
	}
}

function panelIsVisible(panel: WorkbenchPanelPreference, hasCompanion: boolean): boolean {
	return panel.visible ?? (panel.id === 'companion' && hasCompanion);
}

function normalizeLayout(preferences: WorkbenchPanelPreference[]): WorkbenchPanelPreference[] {
	const normalized = [...preferences];
	const primaryVisible = normalized.some((panel) =>
		(panel.id === 'conversation' || panel.id === 'investigation') && panel.visible === true
	);
	if (!primaryVisible) {
		const investigation = normalized.find((panel) => panel.id === 'investigation');
		if (investigation) investigation.visible = true;
	}
	for (const group of WORKBENCH_GROUPS) {
		normalized
			.filter((panel) => panel.group === group.id)
			.sort((a, b) => a.order - b.order || WORKBENCH_PANELS.findIndex((item) => item.id === a.id) - WORKBENCH_PANELS.findIndex((item) => item.id === b.id))
			.forEach((panel, order) => { panel.order = order; });
	}
	return normalized;
}

class WorkbenchLayout {
	panels = $state<WorkbenchPanelPreference[]>(readLayout());
	conversationShare = $state<number>(readConversationShare());

	placements(hasCompanion: boolean): WorkbenchPanelPlacement[] {
		const sorted = [...this.panels].sort((a, b) =>
			WORKBENCH_GROUPS.findIndex((group) => group.id === a.group) - WORKBENCH_GROUPS.findIndex((group) => group.id === b.group)
			|| a.order - b.order
		);
		let stackPosition = 0;
		const rowCount = this.rowCount(hasCompanion);
		return sorted.map((panel, index) => {
			const visibleNow = panelIsVisible(panel, hasCompanion);
			if (visibleNow) stackPosition += 1;
			const before = sorted.slice(0, index).filter((item) => item.group === panel.group && panelIsVisible(item, hasCompanion));
			const groupPosition = panelIsVisible(panel, hasCompanion) ? before.length + 1 : 1;
			const groupCount = sorted.filter((item) => item.group === panel.group && panelIsVisible(item, hasCompanion)).length;
			const firstInGroup = visibleNow && !before.length;
			const groupRowSpan = visibleNow && groupPosition === groupCount ? rowCount - groupCount + 1 : 1;
			return { ...panel, visibleNow, groupPosition, groupCount, groupRowSpan, stackPosition: visibleNow ? stackPosition : 1, firstInGroup };
		});
	}

	visibleCount(hasCompanion: boolean): number {
		return this.panels.filter((panel) => panelIsVisible(panel, hasCompanion)).length;
	}

	visibleGroupCount(hasCompanion: boolean): number {
		return WORKBENCH_GROUPS.filter((group) =>
			this.panels.some((panel) => panel.group === group.id && panelIsVisible(panel, hasCompanion))
		).length;
	}

	gridColumn(group: WorkbenchPanelGroup, hasCompanion: boolean): number {
		if (group === 'conversation') return 1;
		return this.panels.some((panel) => panel.group === 'conversation' && panelIsVisible(panel, hasCompanion)) ? 3 : 1;
	}

	rowCount(hasCompanion: boolean): number {
		return Math.max(...WORKBENCH_GROUPS.map((group) =>
			this.panels.filter((panel) => panel.group === group.id && panelIsVisible(panel, hasCompanion)).length
		), 1);
	}

	canHide(id: WorkbenchPanelId, hasCompanion: boolean): boolean {
		const panel = this.panels.find((item) => item.id === id);
		if (!panel || !panelIsVisible(panel, hasCompanion)) return true;
		if (id !== 'conversation' && id !== 'investigation') return true;
		return this.panels.some((other) =>
			other.id !== id
			&& (other.id === 'conversation' || other.id === 'investigation')
			&& panelIsVisible(other, hasCompanion)
		);
	}

	toggle(id: WorkbenchPanelId, hasCompanion: boolean): void {
		const current = this.panels.find((panel) => panel.id === id);
		if (!current) return;
		const visible = panelIsVisible(current, hasCompanion);
		if (visible && !this.canHide(id, hasCompanion)) return;
		this.update(id, { visible: !visible });
	}

	setGroup(id: WorkbenchPanelId, group: WorkbenchPanelGroup): void {
		this.update(id, { group });
	}

	canMove(id: WorkbenchPanelId, direction: -1 | 1): boolean {
		const panel = this.panels.find((item) => item.id === id);
		if (!panel) return false;
		const peers = this.panels.filter((item) => item.group === panel.group).sort((a, b) => a.order - b.order);
		const index = peers.findIndex((item) => item.id === id);
		return direction < 0 ? index > 0 : index >= 0 && index < peers.length - 1;
	}

	move(id: WorkbenchPanelId, direction: -1 | 1): void {
		if (!this.canMove(id, direction)) return;
		const current = this.panels.find((item) => item.id === id);
		if (!current) return;
		const peers = this.panels.filter((item) => item.group === current.group).sort((a, b) => a.order - b.order);
		const index = peers.findIndex((item) => item.id === id);
		const other = peers[index + direction];
		if (!other) return;
		this.panels = normalizeLayout(this.panels.map((item) => {
			if (item.id === current.id) return { ...item, order: other.order };
			if (item.id === other.id) return { ...item, order: current.order };
			return item;
		}));
		this.persist();
	}

	reset(): void {
		this.panels = copyDefaults();
		this.conversationShare = DEFAULT_CONVERSATION_SHARE;
		this.persist();
	}

	setConversationShare(value: number, persist = true): void {
		if (!Number.isFinite(value)) return;
		this.conversationShare = Math.max(MIN_CONVERSATION_SHARE, Math.min(MAX_CONVERSATION_SHARE, value));
		if (persist) this.persist();
	}

	private update(id: WorkbenchPanelId, patch: Partial<WorkbenchPanelPreference>): void {
		this.panels = normalizeLayout(this.panels.map((panel) => panel.id === id ? { ...panel, ...patch } : panel));
		this.persist();
	}

	private persist(): void {
		try {
			localStorage.setItem(STORAGE_KEY, JSON.stringify({
				version: 1,
				panels: this.panels,
				conversationShare: this.conversationShare
			}));
		} catch {
			/* The layout remains usable for this session if storage is unavailable. */
		}
	}
}

export const workbenchLayout = new WorkbenchLayout();
