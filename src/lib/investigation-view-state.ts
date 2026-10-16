/** Presentation only. Stable tool keys keep navigation out of analytical records. */
export type InvestigationSection = 'brief' | 'data' | 'questions' | 'analysis' | 'results' | 'share';
export interface InvestigationViewState {
	section: InvestigationSection;
	scroll: Partial<Record<InvestigationSection, number>>;
	openRunId: string | null;
	definitionsOpen: boolean;
	historyOpen: boolean;
}
const sections = ['brief', 'data', 'questions', 'analysis', 'results', 'share'] as const;
const cache = new Map<string, InvestigationViewState>();
const prefix = 'fella:tool-view:';

export function readInvestigationView(key: string): InvestigationViewState {
	const fallback: InvestigationViewState = { section: 'brief', scroll: {}, openRunId: null, definitionsOpen: false, historyOpen: true };
	if (!key) return fallback;
	try {
		const saved = cache.get(key) ?? JSON.parse(localStorage.getItem(prefix + key) || 'null');
		if (!saved || !sections.includes(saved.section)) return fallback;
		const scroll = Object.fromEntries(sections.map(section => [section, Math.max(0, Math.min(1_000_000, Number(saved.scroll?.[section]) || 0))]));
		return { section: saved.section, scroll, openRunId: typeof saved.openRunId === 'string' ? saved.openRunId : null, definitionsOpen: saved.definitionsOpen === true, historyOpen: saved.historyOpen !== false };
	} catch { return fallback; }
}

export function saveInvestigationView(key: string, state: InvestigationViewState): void {
	if (!key) return;
	const plain = JSON.parse(JSON.stringify(state)) as InvestigationViewState;
	cache.set(key, plain);
	try { localStorage.setItem(prefix + key, JSON.stringify(plain)); } catch { /* Session navigation still works without storage. */ }
}
