// Shared reactive session state (Svelte 5 runes in a .svelte.ts module).
//
// `Session` owns app-level navigation and one active workspace context.
// Conversation tabs retain their own scope; the conversation objects remain in
// a local live cache so in-flight work can finish safely while focus moves.

import { ipc, isDesktop } from './ipc';
import type {
	AskMode,
	Catalog,
	ContextReference,
	EvidenceItem,
	Message,
	ProviderHealth,
	ProviderInfo,
	Project,
	RunStep,
	RuntimeTurnState,
	Settings,
	SourceInfo,
	WorkspaceProgress
} from './types';
import { isRenderableChartEvidence, parseCompanionPane, type CompanionPane } from './workbench';
import { analystStepForTool, progressStepForState } from './run-progress';


function uid(): string {
	return Math.random().toString(36).slice(2, 10);
}

/** Commands are controls for the workspace, not conversational turns. Older
 * archives may contain them as user messages, so every history surface uses
 * this predicate when deciding what counts as a real question. */
export function isActualQuestion(message: Pick<Message, 'role' | 'text'>): boolean {
	return message.role === 'user' && message.text.trim().length > 0 && !message.text.trimStart().startsWith('/');
}

export function hasActualQuestion(messages: readonly Message[]): boolean {
	return messages.some(isActualQuestion);
}

export function firstActualQuestion(messages: readonly Message[]): Message | undefined {
	return messages.find(isActualQuestion);
}

/** The first completed answer is authoritative for a conversation's origin.
 * Older session snapshots were repeatedly stamped with the currently mounted
 * repository, so recover their initial scope from that answer when possible. */
function archivedWorkspace(messages: unknown, fallback: string | null | undefined): string | null {
	if (Array.isArray(messages)) {
		const isQuestion = (message: unknown): message is Message =>
			message !== null && typeof message === 'object' && isActualQuestion(message as Message);
		for (let questionIndex = 0; questionIndex < messages.length; questionIndex++) {
			if (!isQuestion(messages[questionIndex])) continue;
			for (let i = questionIndex + 1; i < messages.length; i++) {
				const message = messages[i];
				if (isQuestion(message)) break;
				if (!message || typeof message !== 'object') continue;
				const candidate = message as Record<string, unknown>;
				if (candidate.role !== 'assistant' || !candidate.answer || typeof candidate.answer !== 'object') continue;
				const answer = candidate.answer as Record<string, unknown>;
				const answerWorkspace = answer.workspace;
				if (Object.hasOwn(answer, 'workspace') && answerWorkspace === null) return null;
				if (answerWorkspace && typeof answerWorkspace === 'object') {
					const path = (answerWorkspace as Record<string, unknown>).path;
					if (typeof path === 'string' && path.length > 0) return path;
				}
				const trace = answer.trace;
				if (trace && typeof trace === 'object') {
					const traceRecord = trace as Record<string, unknown>;
					const revision = traceRecord.workspace_revision;
					if (revision === null || (revision === undefined && (traceRecord.id || traceRecord.turn_id))) {
						return null;
					}
				}
				break;
			}
		}
	}
	return fallback ?? null;
}

const PREFIX = 'fella:conversation:'; // one key per conversation
const LEGACY_KEY = 'fella:conversation'; // the single pre-tabs blob
const INDEX_KEY = 'fella:conversations'; // JSON array of live conversation ids
const SIDEBAR_KEY = 'fella:sidebar-collapsed';
const REPOSITORIES_KEY = 'fella:repositories';
const HIDDEN_REPOSITORIES_KEY = 'fella:hidden-repositories';
const HISTORY_ONLY_REPOSITORIES_KEY = 'fella:history-only-repositories';
const PROJECTS_KEY = 'fella:projects';
const OPEN_WORKSPACES_KEY = 'fella:open-workspaces';
const WORKSPACE_CONTEXTS_KEY = 'fella:workspace-contexts';
const LEGACY_ENVIRONMENTS_KEY = 'fella:environments';
export const GENERAL_WORKSPACE_ID = 'general';

export interface WorkspaceWindow {
	id: string;
	kind: 'general' | 'repository';
	/** Null for General. It is never a fake filesystem path. */
	path: string | null;
	catalog: Catalog;
	conversationId: string;
	historyOnly?: boolean;
}

interface SavedWorkspaceState {
	ids: string[];
	active: string | null;
}

export interface WorkspaceContext {
	/** Null is the unbound General workspace; repository IDs are resolved paths. */
	workspaceId: string | null;
	conversationId: string;
	workspacePane?: WorkspacePane;
	/** Whether the optional workspace tools pane is open beside the conversation. */
	workspacePaneOpen?: boolean;
	investigationId?: string | null;
	/** Tool surfaces opened in this workspace pane. Investigation tabs own chats. */
	workspaceToolTabs?: WorkspaceToolTab[];
	activeWorkspaceToolTabId?: string | null;
}

export interface WorkspaceToolTab {
	id: string;
	kind: WorkspaceToolKind;
	/** Present for an Investigation tab once its durable record is known. */
	investigationId?: string | null;
	/** The canonical conversation associated with this Investigation tab. */
	conversationId?: string | null;
	/** Catalog identity for a read-only source preview tab. */
	sourcePath?: string | null;
	sourceName?: string | null;
	/** An immutable reference to an existing answer artifact or older source snapshot. */
	preview?: CompanionPane | null;
	/** Studio documents are workspace-owned, never owned by the active chat. */
	documentId?: string | null;
	title?: string;
	initialArtifactId?: string | null;
	artifactId?: string | null;
	artifactRevision?: number | null;
	newReport?: boolean;
}

interface SavedWorkspaceContextState {
	contexts: WorkspaceContext[];
	activeWorkspaceId: string | null;
}

function readWorkspaceState(): SavedWorkspaceState {
	try {
		const raw = localStorage.getItem(OPEN_WORKSPACES_KEY);
		if (raw === null) return { ids: [], active: null };
		const parsed: unknown = JSON.parse(raw);
		if (!parsed || typeof parsed !== 'object') return { ids: [], active: null };
		const record = parsed as Record<string, unknown>;
		const legacyPaths = Array.isArray(record.paths)
			? [...new Set(record.paths.filter((path): path is string => typeof path === 'string' && path.trim().length > 0))]
			: [];
		// These paths are read only as migration input. The new app restores one
		// active workspace instead of a multi-workspace board.
		const ids = Array.isArray(record.ids)
			? [...new Set(record.ids.filter((id): id is string => typeof id === 'string' && id.trim().length > 0 && id !== GENERAL_WORKSPACE_ID))]
			: legacyPaths;
		const normalizedIds = ids;
		const active = typeof record.active === 'string' && normalizedIds.includes(record.active)
			? record.active
			: null;
		return { ids: normalizedIds, active };
	} catch {
		return { ids: [], active: null };
	}
}

const SAVED_WORKSPACE_STATE = readWorkspaceState();

function workspaceContextKey(workspaceId: string | null): string {
	return workspaceId === null ? '\u0000general' : workspaceId;
}

function normalizeWorkspaceContext(value: unknown): WorkspaceContext | null {
	if (!value || typeof value !== 'object') return null;
	const source = value as Record<string, unknown>;
	if (source.workspaceId !== null && typeof source.workspaceId !== 'string') return null;
	const workspaceId = source.workspaceId as string | null;
	const legacyKind: WorkspacePane = source.workspacePane === 'context' ? 'context' : 'investigation';
	const legacyInvestigationId = typeof source.investigationId === 'string' ? source.investigationId : null;
	const savedTabs = Array.isArray(source.workspaceToolTabs)
		? source.workspaceToolTabs.flatMap((rawTab): WorkspaceToolTab[] => {
			if (!rawTab || typeof rawTab !== 'object') return [];
			const tab = rawTab as Record<string, unknown>;
			if (typeof tab.id !== 'string' || !tab.id || !['context', 'investigation', 'source', 'terminal', 'preview', 'studio', 'result'].includes(String(tab.kind))) return [];
			if (tab.kind === 'result' && (typeof tab.artifactId !== 'string' || !Number.isSafeInteger(tab.artifactRevision) || Number(tab.artifactRevision) < 1)) return [];
			const preview = parseCompanionPane(tab.preview);
			if (tab.kind === 'preview' && !preview) return [];
			return [{
				id: tab.id,
				kind: tab.kind as WorkspaceToolKind,
				investigationId: typeof tab.investigationId === 'string' ? tab.investigationId : null,
				conversationId: typeof tab.conversationId === 'string' ? tab.conversationId : null,
				sourcePath: typeof tab.sourcePath === 'string' ? tab.sourcePath : null,
				sourceName: typeof tab.sourceName === 'string' ? tab.sourceName : null,
				documentId: typeof tab.documentId === 'string' ? tab.documentId : null,
				title: typeof tab.title === 'string' ? tab.title : undefined,
				initialArtifactId: typeof tab.initialArtifactId === 'string' ? tab.initialArtifactId : null,
				artifactId: typeof tab.artifactId === 'string' ? tab.artifactId : null,
				artifactRevision: typeof tab.artifactRevision === 'number' ? tab.artifactRevision : null,
				preview
			}];
		}).filter((tab, index, all) => all.findIndex((candidate) => candidate.id === tab.id) === index)
		: [];
	// Upgrade old single-selection state. Keep a closed pane closed; if it was
	// open, retain its selected tool as a tab in the new workspace context.
	if (!savedTabs.length && (
		source.workspacePaneOpen === true ||
		(legacyKind === 'investigation' && legacyInvestigationId !== null)
	)) {
		savedTabs.push({
			id: legacyKind === 'context' ? 'context' : `investigation:${legacyInvestigationId ?? String(source.conversationId ?? 'legacy')}`,
			kind: legacyKind,
			investigationId: legacyKind === 'investigation' ? legacyInvestigationId : null,
			conversationId: legacyKind === 'investigation' && typeof source.conversationId === 'string' ? source.conversationId : null
		});
	}
	const savedActiveId = typeof source.activeWorkspaceToolTabId === 'string' ? source.activeWorkspaceToolTabId : null;
	const activeTabId = savedTabs.some((tab) => tab.id === savedActiveId) ? savedActiveId : savedTabs[0]?.id ?? null;
	return {
		workspaceId,
		conversationId: typeof source.conversationId === 'string' ? source.conversationId : '',
		workspacePane: savedTabs.find((tab) => tab.id === activeTabId)?.kind === 'investigation' ? 'investigation' : legacyKind,
		workspacePaneOpen: source.workspacePaneOpen === true,
		investigationId: typeof source.investigationId === 'string' ? source.investigationId : null,
		workspaceToolTabs: savedTabs,
		activeWorkspaceToolTabId: activeTabId
	};
}

function readWorkspaceContextState(): SavedWorkspaceContextState {
	try {
		const currentRaw = localStorage.getItem(WORKSPACE_CONTEXTS_KEY);
		if (currentRaw) {
			const parsed: unknown = JSON.parse(currentRaw);
			if (parsed && typeof parsed === 'object') {
				const record = parsed as Record<string, unknown>;
				const contexts = Array.isArray(record.contexts)
					? record.contexts.flatMap((item): WorkspaceContext[] => {
						const context = normalizeWorkspaceContext(item);
						return context ? [context] : [];
					}).filter((context, index, all) => all.findIndex((item) => workspaceContextKey(item.workspaceId) === workspaceContextKey(context.workspaceId)) === index)
					: [];
				if (contexts.length) {
					const activeWorkspaceId = record.activeWorkspaceId === null || typeof record.activeWorkspaceId === 'string'
						? record.activeWorkspaceId as string | null
						: null;
					if (!contexts.some((context) => context.workspaceId === activeWorkspaceId)) {
						contexts.unshift(normalizeWorkspaceContext({ workspaceId: null, conversationId: '' })!);
					}
					return { contexts, activeWorkspaceId };
				}
			}
		}

		// One-time migration from environment compositions. The active environment
		// wins when older copies contain different state for the same workspace;
		// tool tabs from other copies are retained when they have distinct owners.
		const raw = localStorage.getItem(LEGACY_ENVIRONMENTS_KEY);
		if (raw) {
			const parsed: unknown = JSON.parse(raw);
			if (parsed && typeof parsed === 'object') {
				const record = parsed as Record<string, unknown>;
				if (Array.isArray(record.environments)) {
					const environments = record.environments.filter((item): item is Record<string, unknown> =>
						Boolean(item && typeof item === 'object' && Array.isArray((item as Record<string, unknown>).panes))
					);
					const activeEnvironment = environments.find((item) => item.id === record.active);
					const ordered = [...(activeEnvironment ? [activeEnvironment] : []), ...environments.filter((item) => item !== activeEnvironment)];
					const byWorkspace = new Map<string, WorkspaceContext>();
					for (const environment of ordered) {
						const panes = environment.panes as unknown[];
						for (const rawPane of panes) {
							const candidate = normalizeWorkspaceContext(rawPane);
							if (!candidate) continue;
							const key = workspaceContextKey(candidate.workspaceId);
							const existing = byWorkspace.get(key);
							if (!existing) {
								byWorkspace.set(key, candidate);
								continue;
							}
							const knownTabs = new Set(existing.workspaceToolTabs?.map((tab) =>
								tab.kind === 'context' ? 'context' : tab.investigationId ? `investigation:${tab.investigationId}` : tab.conversationId ? `conversation:${tab.conversationId}` : `id:${tab.id}`
							));
							const usedIds = new Set(existing.workspaceToolTabs?.map((tab) => tab.id));
							for (const incoming of candidate.workspaceToolTabs ?? []) {
								const identity = incoming.kind === 'context' ? 'context' : incoming.investigationId ? `investigation:${incoming.investigationId}` : incoming.conversationId ? `conversation:${incoming.conversationId}` : `id:${incoming.id}`;
								if (knownTabs.has(identity)) continue;
								let id = incoming.id;
								let suffix = 2;
								while (usedIds.has(id)) id = `${incoming.id}:migrated-${suffix++}`;
								usedIds.add(id);
								knownTabs.add(identity);
								existing.workspaceToolTabs ??= [];
								existing.workspaceToolTabs.push({ ...incoming, id });
							}
						}
					}
					const contexts = [...byWorkspace.values()];
					if (!contexts.some((context) => context.workspaceId === null)) {
						contexts.push(normalizeWorkspaceContext({ workspaceId: null, conversationId: '' })!);
					}
					const activePane = activeEnvironment && Array.isArray(activeEnvironment.panes)
						? (activeEnvironment.panes as unknown[]).find((item) => item && typeof item === 'object' && (item as Record<string, unknown>).id === activeEnvironment.activePaneId)
						: null;
					const activePaneRecord = activePane && typeof activePane === 'object' ? activePane as Record<string, unknown> : null;
					const legacyActive = activePaneRecord && (activePaneRecord.workspaceId === null || typeof activePaneRecord.workspaceId === 'string')
						? activePaneRecord.workspaceId as string | null
						: SAVED_WORKSPACE_STATE.active;
					const activeWorkspaceId = contexts.some((context) => context.workspaceId === legacyActive) ? legacyActive : null;
					return { contexts, activeWorkspaceId };
				}
			}
		}
	} catch {
		/* fall through to the older workspace list */
	}
	const contexts = SAVED_WORKSPACE_STATE.ids.map((workspaceId) => normalizeWorkspaceContext({ workspaceId, conversationId: '' })!);
	contexts.push(normalizeWorkspaceContext({ workspaceId: null, conversationId: '' })!);
	return {
		contexts,
		activeWorkspaceId: contexts.some((context) => context.workspaceId === SAVED_WORKSPACE_STATE.active)
			? SAVED_WORKSPACE_STATE.active
			: null
	};
}

const SAVED_WORKSPACE_CONTEXT_STATE = readWorkspaceContextState();

function readSidebarCollapsed(): boolean {
	try {
		const v = localStorage.getItem(SIDEBAR_KEY);
		return v === null ? false : v === '1';
	} catch {
		return false;
	}
}

function readStringList(key: string): string[] {
	try {
		const parsed: unknown = JSON.parse(localStorage.getItem(key) ?? '[]');
		return Array.isArray(parsed)
			? parsed.filter((item): item is string => typeof item === 'string' && item.trim().length > 0)
			: [];
	} catch {
		return [];
	}
}

function readProjects(): Project[] {
	try {
		const parsed: unknown = JSON.parse(localStorage.getItem(PROJECTS_KEY) ?? '[]');
		if (!Array.isArray(parsed)) return [];
		return parsed.filter((item): item is Project => {
			if (!item || typeof item !== 'object') return false;
			const candidate = item as Record<string, unknown>;
			return (
				typeof candidate.id === 'string' &&
				typeof candidate.name === 'string' &&
				typeof candidate.workspace === 'string' &&
				typeof candidate.body === 'string' &&
				typeof candidate.created_at_ms === 'number' &&
				typeof candidate.updated_at_ms === 'number'
			);
		});
	} catch {
		return [];
	}
}


/** One conversation: its transcript, its in-flight run, and its input history. */
export class Conversation {
	readonly kind = 'chat' as const;
	readonly id: string;
	messages = $state<Message[]>([]);
	busy = $state<boolean>(false);
	/** Transient one-line status shown while this conversation's agent is working. */
	activity = $state<string>('');
	/** Set by `/login <provider>`: the next composer line is taken as the API
	 *  key for this provider not echoed to the transcript, not persisted. */
	pendingKey = $state<{ provider: string; display: string } | null>(null);
	/** ↑-recall history for the composer while this conversation is focused. */
	history: string[] = [];
	/** The model this conversation answers with. Empty = use the saved default. */
	model = $state<string>('');
	/** Folder-backed analytics scope, pinned when the first real question is
	 *  asked. Null means this conversation belongs to General; undefined is an
	 *  unassigned, pristine conversation awaiting an owner. */
	workspaceScope = $state<string | null | undefined>(undefined);
	/** A user-given name. null = derive one from the folder + first actual
	 *  question, the same as an un-renamed conversation always has. */
	title = $state<string | null>(null);
	/** The current question's intent. Inspect selects the stricter read-only
	 *  tool registry and makes the source-first workflow explicit to the model. */
	mode = $state<AskMode>('ask');
	/** Sources and fields chosen from the workspace for this conversation. */
	contextRefs = $state<ContextReference[]>([]);
	/** Optional artifact surface beside this conversation's transcript. */
	companionPane = $state<CompanionPane | null>(null);
	/** The compact, local run trace shown beneath the transcript. */
	runSteps = $state<RunStep[]>([]);
	runQuestion = $state('');
	runQuestionId = $state<string | null>(null);
	runState = $state<RuntimeTurnState | null>(null);
	runStartedAt = $state<number | null>(null);
	runDurationMs = $state<number | null>(null);
	runMessageId = $state<string | null>(null);

	/** `id` defaults to a fresh one (a genuinely new conversation). Reopening
	 *  an archived conversation passes its real id back in, so re-archiving
	 *  it (on the next settle, or on close) overwrites the same file instead
	 *  of forking a duplicate under a new one. */
	constructor(id?: string) {
		this.id = id ?? uid();
	}

	#persistTimer: ReturnType<typeof setTimeout> | undefined;

	// NB: return the element *after* pushing `messages` is a `$state` proxy, so
	// the pushed plain object is only reactive when reached through the array.
	addUser(text: string): Message {
		this.messages.push({ id: uid(), role: 'user', text, ts: Date.now() });
		return this.messages[this.messages.length - 1];
	}
	addAssistant(text = ''): Message {
		this.messages.push({ id: uid(), role: 'assistant', text, pending: true, ts: Date.now() });
		return this.messages[this.messages.length - 1];
	}
	addSystem(text: string): Message {
		this.messages.push({ id: uid(), role: 'system', text, ts: Date.now() });
		return this.messages[this.messages.length - 1];
	}

	setMode(mode: AskMode): void {
		this.mode = mode;
	}

	bindWorkspaceScope(workspace: string | null): void {
		if (this.workspaceScope === undefined) this.workspaceScope = workspace;
	}

	addContext(ref: ContextReference): void {
		if (this.contextRefs.some((item) => item.kind === ref.kind && item.key === ref.key)) return;
		this.contextRefs = [...this.contextRefs, ref];
	}

	removeContext(kind: ContextReference['kind'], key: string): void {
		this.contextRefs = this.contextRefs.filter((item) => item.kind !== kind || item.key !== key);
	}

	clearContext(): void {
		this.contextRefs = [];
	}

	startRun(question = '', questionId: string | null = null, messageId: string | null = null): void {
		const now = Date.now();
		this.runStartedAt = now;
		this.runDurationMs = null;
		this.runQuestion = question.trim();
		this.runQuestionId = questionId;
		this.runMessageId = messageId;
		this.runState = 'received';
		this.runSteps = [
			{
				id: uid(),
				label: 'Understanding the question',
				state: 'running',
				started_at_ms: now
			}
		];
	}

	updateRunState(state: RuntimeTurnState): void {
		this.runState = state;
		const label = progressStepForState(state);
		if (label) this.beginProgressStep(label);
	}

	private beginProgressStep(label: string): void {
		const current = this.runSteps.at(-1);
		if (current?.state === 'running' && current.label === label) return;
		const now = Date.now();
		this.runSteps = [
			...this.runSteps.map((step) =>
				step.state === 'running'
					? { ...step, state: 'complete' as const, finished_at_ms: now }
					: step
			),
			{ id: uid(), label, state: 'running', started_at_ms: now }
		];
	}

	beginRunStep(tool: string): string {
		const now = Date.now();
		const label = analystStepForTool(tool);
		const steps = this.runSteps.map((step) =>
			step.state === 'running' && !step.tool
				? { ...step, state: 'complete' as const, finished_at_ms: now }
				: step
		);
		this.runSteps = [
			...steps,
			{
				id: uid(),
				label,
				state: 'running',
				started_at_ms: now,
				tool
			}
		];
		return label;
	}

	completeRunStep(item: EvidenceItem): void {
		const now = Date.now();
		const index = this.runSteps.findLastIndex(
			(step) => step.state === 'running' && (!step.tool || step.tool === item.tool)
		);
		if (index < 0) return;
		this.runSteps = this.runSteps.map((step, i) =>
			i === index
				? {
						...step,
						state: item.error ? ('error' as const) : ('complete' as const),
						finished_at_ms: now,
						summary: item.error ? undefined : item.result_summary,
						sources: item.sources?.map((source) =>
							source.table ? `${source.source} · ${source.table}` : source.source
						),
						evidence_id: item.id
					}
				: step
		);
	}

	finishRun(error = false): void {
		const now = Date.now();
		this.runSteps = this.runSteps.map((step) =>
			step.state === 'running'
				? { ...step, state: error ? ('error' as const) : ('complete' as const), finished_at_ms: now }
				: step
		);
		const message = this.runMessageId
			? this.messages.find((item) => item.id === this.runMessageId)
			: undefined;
		if (error || message?.answer?.status === 'failed') {
			this.runState = 'failed';
		} else if (!['accepted', 'clarify', 'needs_review', 'unsupported', 'failed', 'cancelled'].includes(this.runState ?? '')) {
			if (message?.answer?.clarification) this.runState = 'clarify';
			else if (message?.answer?.status === 'needs_review' || message?.answer?.status === 'insufficient_data') this.runState = 'needs_review';
			else this.runState = 'accepted';
		}
		if (this.runStartedAt !== null) this.runDurationMs = Math.max(0, now - this.runStartedAt);
		if (message) {
			message.run_steps = this.runSteps.map((step) => ({ ...step }));
			if (this.runState) message.run_state = this.runState;
			if (this.runQuestionId) message.run_question_id = this.runQuestionId;
			if (this.runDurationMs !== null) message.run_duration_ms = this.runDurationMs;
		}
		this.runStartedAt = null;
		this.runMessageId = null;
	}

	/** Coalesce writes while a run streams; flush immediately once it settles. */
	persist(): void {
		if (!hasActualQuestion(this.messages)) {
			this.dropSnapshot();
			return;
		}
		clearTimeout(this.#persistTimer);
		if (this.busy) {
			this.#persistTimer = setTimeout(() => this.#writeSnapshot(this.workspaceScope ?? null), 250);
		} else {
			this.#writeSnapshot(this.workspaceScope ?? null);
		}
	}
	#writeSnapshot(workspace: string | null): void {
		try {
			localStorage.setItem(
				PREFIX + this.id,
				JSON.stringify({
					id: this.id,
					workspace,
					model: this.model,
					messages: this.messages.slice(-200),
					companionPane: this.companionPane
				})
			);
		} catch {
			/* ignore */
		}
	}
	dropSnapshot(): void {
		clearTimeout(this.#persistTimer);
		try {
			localStorage.removeItem(PREFIX + this.id);
		} catch {
			/* ignore */
		}
	}
}


export type WorkspaceView = 'ask' | 'workspace' | 'settings' | 'project';
export type WorkspacePane = 'context' | 'investigation';
export type WorkspaceToolKind = WorkspacePane | 'source' | 'terminal' | 'preview' | 'studio' | 'result';

export interface InvestigationNavigationTarget {
	workspaceId: string;
	conversationId: string;
	turnId: string;
}

export interface ConversationAnalysisFocus {
	conversationId: string;
	turnId: string;
}

class Session {
	#deletingWorkspacePaths = new Set<string>();
	#pendingArchivesByWorkspace = new Map<string, Set<Promise<void>>>();

	catalog = $state<Catalog>({ workspace: null, sources: [] });
	/** Repository catalogs currently mounted by the analysis engine. */
	workspaceWindows = $state<WorkspaceWindow[]>([]);
	workspaceContexts = $state<WorkspaceContext[]>(SAVED_WORKSPACE_CONTEXT_STATE.contexts);
	activeWorkspaceId = $state<string | null>(SAVED_WORKSPACE_CONTEXT_STATE.activeWorkspaceId);
	mountProgress = $state<WorkspaceProgress | null>(null);
	/** The app-level page; an unbound conversation is the initial destination. */
	workspaceView = $state<WorkspaceView>('ask');
	workspaceSection = $state<'chat' | 'files' | 'reports'>('chat');
	newInvestigationRequested = $state(false);
	requestNewInvestigation(): void {
		if (!this.catalog.workspace) return;
		this.workspaceSection = 'chat';
		this.setWorkspacePaneOpen(true);
		this.newInvestigationRequested = true;
	}
	/** Window organization only; never a mounted Workspace or an execution mode. */
	contentLayout = $state<'chat' | 'notebook'>('chat');
	setContentLayout(layout: 'chat' | 'notebook'): void {
		this.contentLayout = layout;
		if (layout === 'chat') { this.setWorkspacePaneOpen(false); return; }
		if (layout !== 'notebook' || !this.catalog.workspace) return;
		// Reuse the current tab/chat identity. Opening a layout is not permission
		// to create an Investigation or start an analytical run.
		const tab = this.activeWorkspaceToolTab;
		if (!tab || tab.kind === 'investigation' && tab.conversationId !== this.activeChat.id) {
			this.openWorkspaceToolTab('investigation', null, this.activeChat.id);
		} else this.setWorkspacePaneOpen(true);
	}
	/** The active pane inside Workspace. */
	workspacePane = $state<WorkspacePane>('investigation');
	/** The tool chooser is a transient entry state, not a persisted workspace tool. */
	workspaceToolChooserOpen = $state(false);
	/** Ephemeral cross-navigation between a transcript answer and its canonical run. */
	investigationNavigationTarget = $state<InvestigationNavigationTarget | null>(null);
	conversationAnalysisFocus = $state<ConversationAnalysisFocus | null>(null);
	settings = $state<Settings | null>(null);
	health = $state<ProviderHealth | null>(null);
	/** Built-in providers from the engine, cached so the composer can hint
	 *  valid `/login` / `/logout` names without an await. */
	providers = $state<ProviderInfo[]>([]);
	/** Live conversations retain their workspace scope. */
	conversations = $state<Conversation[]>([new Conversation()]);
	activeConversationIndex = $state<number>(0);
	/** Focus mode: hide the shell header for a plain, single-conversation view. */
	focus = $state<boolean>(false);
	/** History sidebar visibility. Unlike `focus`, this is remembered across
	 *  launches (open by default) it's a layout preference, not a
	 *  per-session display mode. Toggled by the titlebar button or Ctrl+B. */
	sidebarCollapsed = $state<boolean>(readSidebarCollapsed());
	/** Bumped whenever a conversation is archived, so the sidebar's list
	 *  knows to refetch without polling. */
	historyVersion = $state<number>(0);
	/** Bumped when the workspace's Investigation records or links change. */
	investigationsVersion = $state<number>(0);
	reportsVersion = $state(0);
	/** Presentation labels, hydrated from durable Investigations; never transcript edits. */
	investigationNames = $state<Record<string, string>>({});
	rememberInvestigationNames(records: readonly { title: string; conversation_id?: string | null }[]): void {
		for (const record of records) {
			if (record.conversation_id && record.title.trim()) this.investigationNames[record.conversation_id] = record.title.trim();
		}
	}
	/** Bumped for Investigation record edits that update the workspace picker summary. */
	workspaceInvestigationSummaryVersion = $state<number>(0);
	/** Investigation currently selected in the Investigation view, scoped to the active Workspace. */
	activeInvestigationId = $state<string | null>(null);
	/** Source selected by global search for the contextual Investigation catalog. */
	selectedSourcePath = $state<string | null>(null);
	/** Repositories shown in the local sidebar, in user-defined order. */
	repositoryPaths = $state<string[]>(readStringList(REPOSITORIES_KEY));
	/** Repositories intentionally hidden from the local sidebar. */
	hiddenRepositoryPaths = $state<string[]>(readStringList(HIDDEN_REPOSITORIES_KEY));
	/** Saved conversation groups whose workspace path could not be mounted. */
	historyOnlyRepositoryPaths = $state<string[]>(readStringList(HISTORY_ONLY_REPOSITORIES_KEY));
	/** User-created repository wikis, kept entirely on this computer. */
	projects = $state<Project[]>(readProjects());
	activeProjectId = $state<string | null>(null);

	constructor() {
		// A workspace is the scope owner. General is always available without
		// pretending that it is a mounted folder.
		let context = this.workspaceContextFor(this.activeWorkspaceId);
		if (!context) {
			context = { workspaceId: this.activeWorkspaceId, conversationId: this.conversations[0].id };
			this.workspaceContexts.push(context);
		}
		context.conversationId = this.conversations[0].id;
		this.conversations[0].workspaceScope = context.workspaceId;
	}

	#writeStringList(key: string, values: string[]): void {
		try {
			localStorage.setItem(key, JSON.stringify(values));
		} catch {
			/* ignore */
		}
	}

	#writeProjects(): void {
		try {
			localStorage.setItem(PROJECTS_KEY, JSON.stringify(this.projects));
		} catch {
			/* ignore */
		}
	}

	#writeWorkspaceState(): void {
		try {
			localStorage.setItem(WORKSPACE_CONTEXTS_KEY, JSON.stringify({
				contexts: this.workspaceContexts,
				activeWorkspaceId: this.activeWorkspaceId
			}));
			// Remove obsolete composition snapshots only after the new representation
			// has been saved successfully.
			localStorage.removeItem(OPEN_WORKSPACES_KEY);
			localStorage.removeItem(LEGACY_ENVIRONMENTS_KEY);
		} catch {
			/* workspace state is a convenience; a full localStorage must not block analysis */
		}
	}

	setActiveInvestigation(id: string | null): void {
		const pane = this.activeWorkspaceContext;
		if (pane) {
			const activeTab = pane.workspaceToolTabs?.find((item) => item.id === pane.activeWorkspaceToolTabId);
			if (id) {
				const conversationId = this.activeChat.id;
				const tabByInvestigation = pane.workspaceToolTabs?.find((item) =>
					item.kind === 'investigation' && item.investigationId === id
				);
				// Ignore a late selection that no longer belongs to the focused chat
				// or to the selected Investigation tab. Never let an old view refresh
				// transfer a durable tab to whichever conversation happens to be active.
				if (
					(activeTab?.kind === 'investigation' && (
						(activeTab.investigationId && activeTab.investigationId !== id) ||
						(activeTab.conversationId && activeTab.conversationId !== conversationId)
					)) ||
					(tabByInvestigation?.conversationId && tabByInvestigation.conversationId !== conversationId)
				) return;

				let tab = activeTab?.kind === 'investigation'
					? activeTab
					: tabByInvestigation ?? pane.workspaceToolTabs?.find((item) =>
						item.kind === 'investigation' &&
						item.conversationId === conversationId &&
						!item.investigationId
					);
				if (tab) {
					if (!tab.investigationId) tab.investigationId = id;
					if (!tab.conversationId) tab.conversationId = conversationId;
				}
				this.activeInvestigationId = id;
				pane.investigationId = id;
				// A new chat must not inherit the old Investigation's selected tab.
				// A workspace-wide Context tab also stays selected while its chat is active.
				if (tab && (!activeTab || activeTab === tab)) {
					pane.activeWorkspaceToolTabId = tab.id;
					pane.workspacePane = 'investigation';
					this.workspacePane = 'investigation';
					if (pane.workspacePaneOpen) this.workspaceToolChooserOpen = false;
				}
			} else {
				this.activeInvestigationId = null;
				pane.investigationId = null;
			}
		} else {
			this.activeInvestigationId = id;
		}
		this.#writeWorkspaceState();
	}

	/** Repair a saved tab label from its canonical conversation when that link is
	 *  unambiguous. Conversation ownership is the source of truth if an older
	 *  stale selection overwrote the tab's Investigation ID. */
	reconcileWorkspaceToolTabs(records: readonly { id: string; conversation_id?: string | null }[]): void {
		const pane = this.activeWorkspaceContext;
		if (!pane?.workspaceToolTabs?.length) return;
		const investigationByConversation = new Map<string, string | null>();
		for (const record of records) {
			const conversationId = record.conversation_id;
			if (!conversationId) continue;
			if (investigationByConversation.has(conversationId)) {
				investigationByConversation.set(conversationId, null);
			} else {
				investigationByConversation.set(conversationId, record.id);
			}
		}

		let changed = false;
		for (const tab of pane.workspaceToolTabs) {
			if (tab.kind !== 'investigation' || !tab.conversationId) continue;
			const investigationId = investigationByConversation.get(tab.conversationId);
			if (investigationId && tab.investigationId !== investigationId) {
				tab.investigationId = investigationId;
				changed = true;
			}
		}
		if (!changed) return;

		const activeTab = pane.workspaceToolTabs.find((tab) => tab.id === pane.activeWorkspaceToolTabId);
		if (activeTab?.kind === 'investigation') {
			pane.investigationId = activeTab.investigationId ?? null;
			this.activeInvestigationId = pane.investigationId;
		}
		this.#writeWorkspaceState();
	}

	get pendingWorkspacePaths(): string[] {
		const id = this.activeWorkspaceId;
		return id && !this.workspaceAt(id) ? [id] : [];
	}

	get savedActiveWorkspaceId(): string | null {
		return this.activeWorkspaceId;
	}

	get activeWorkspaceContext(): WorkspaceContext | null {
		return this.workspaceContextFor(this.activeWorkspaceId);
	}

	workspaceContextFor(workspaceId: string | null): WorkspaceContext | null {
		return this.workspaceContexts.find((context) => context.workspaceId === workspaceId) ?? null;
	}

	#ensureWorkspaceContext(workspaceId: string | null): WorkspaceContext {
		let context = this.workspaceContextFor(workspaceId);
		if (context) return context;
		const conversation = this.conversations.findLast((item) => item.workspaceScope === workspaceId);
		context = {
			workspaceId,
			conversationId: conversation?.id ?? '',
			workspacePane: 'investigation',
			workspacePaneOpen: false,
			workspaceToolTabs: [],
			activeWorkspaceToolTabId: null
		};
		this.workspaceContexts.push(context);
		return context;
	}

	get workspaceToolTabs(): WorkspaceToolTab[] {
		return this.activeWorkspaceContext?.workspaceToolTabs ?? [];
	}

	get activeWorkspaceToolTab(): WorkspaceToolTab | null {
		const context = this.activeWorkspaceContext;
		return context?.workspaceToolTabs?.find((tab) => tab.id === context.activeWorkspaceToolTabId) ?? null;
	}

	workspaceLabel(workspaceId: string | null): string {
		if (!workspaceId) return 'General';
		const workspace = this.workspaceAt(workspaceId);
		return (workspace?.path ?? workspaceId).replace(/[/\\]+$/, '').split(/[/\\]/).at(-1) || 'Workspace';
	}

	get activeWorkspace(): WorkspaceWindow | null {
		return this.activeWorkspaceId ? this.workspaceAt(this.activeWorkspaceId) : null;
	}

	/** The path to pass to the analytical runtime; General always maps to null. */
	get activeRepositoryPath(): string | null {
		const workspace = this.activeWorkspace;
		return workspace?.kind === 'repository' && !workspace.historyOnly ? workspace.path : null;
	}

	workspaceAt(id: string): WorkspaceWindow | null {
		return this.workspaceWindows.find((workspace) => workspace.id === id) ?? null;
	}

	selectWorkspace(id: string): boolean {
		return this.focusWorkspace(id);
	}

	/** Restore the persisted workspace after its catalog has been mounted. */
	restoreActiveWorkspace(): void {
		this.focusWorkspace(this.activeWorkspaceId ?? GENERAL_WORKSPACE_ID);
	}

	/** Register a mounted catalog and focus its repository-owned conversation.
	 *  General conversations are never silently reassigned to a folder. */
	registerWorkspace(catalog: Catalog): boolean {
		const path = catalog.workspace;
		if (!path) return false;
		const existing = this.workspaceAt(path);
		if (existing) {
			existing.catalog = catalog;
			existing.historyOnly = false;
		}
		else {
			this.workspaceWindows = [...this.workspaceWindows, {
				id: path,
				kind: 'repository',
				path,
				catalog,
				conversationId: '',
				historyOnly: false
			}];
		}
		this.catalog = catalog;
		this.rememberRepository(path);
		this.markRepositoryAvailable(path);
		if (!this.focusWorkspace(path)) return false;
		this.#writeWorkspaceState();
		return true;
	}

	/** Focus a workspace and its owned conversation without borrowing another
	 *  workspace's catalog. General has no repository runtime or catalog. */
	focusWorkspace(id: string, conversationId?: string): boolean {
		const scope = id === GENERAL_WORKSPACE_ID ? null : id;
		const workspace = scope ? this.workspaceAt(scope) : null;
		if (scope && !workspace) return false;
		const context = this.#ensureWorkspaceContext(scope);
		const requestedConversation = conversationId
			? this.conversations.find((item) => item.id === conversationId && item.workspaceScope === scope)
			: null;
		let conversationIndex = requestedConversation
			? this.conversations.indexOf(requestedConversation)
			: this.conversations.findIndex((item) => item.id === context.conversationId && item.workspaceScope === scope);
		if (conversationIndex < 0 && workspace?.conversationId) {
			conversationIndex = this.conversations.findIndex((item) => item.id === workspace.conversationId && item.workspaceScope === scope);
		}
		if (conversationIndex < 0) conversationIndex = this.conversations.findLastIndex((item) => item.workspaceScope === scope);
		if (conversationIndex < 0) {
			const conversation = new Conversation();
			conversation.model = this.model;
			conversation.workspaceScope = scope;
			this.conversations.push(conversation);
			conversationIndex = this.conversations.length - 1;
			this.#writeIndex();
		}
		const conversation = this.conversations[conversationIndex];
		if (conversationId && requestedConversation && context.conversationId !== requestedConversation.id) {
			// Selecting another saved conversation must not leave a different
			// Investigation's transcript selected in the Tools pane.
			const activeTab = context.workspaceToolTabs?.find((tab) => tab.id === context.activeWorkspaceToolTabId);
			if (activeTab?.kind === 'investigation' && activeTab.conversationId !== requestedConversation.id) {
				const matchingTab = context.workspaceToolTabs?.find((tab) =>
					tab.kind === 'investigation' && tab.conversationId === requestedConversation.id
				);
				context.activeWorkspaceToolTabId = matchingTab?.id ?? null;
			}
			context.investigationId = null;
		}
		context.conversationId = conversation.id;
		if (workspace) workspace.conversationId = conversation.id;
		this.activeConversationIndex = conversationIndex;
		this.activeWorkspaceId = scope;
		const activeToolTab = context.workspaceToolTabs?.find((tab) => tab.id === context.activeWorkspaceToolTabId) ?? null;
		const conversationToolTab = activeToolTab?.kind === 'investigation' && activeToolTab.conversationId === conversation.id
			? activeToolTab
			: null;
		this.activeInvestigationId = conversationToolTab?.investigationId ?? context.investigationId ?? null;
		this.catalog = workspace && !workspace.historyOnly ? workspace.catalog : { workspace: null, sources: [] };
		this.workspacePane = activeToolTab?.kind === 'investigation' ? 'investigation' : activeToolTab ? 'context' : context.workspacePane ?? 'investigation';
		this.workspaceToolChooserOpen = context.workspacePaneOpen === true && !activeToolTab;
		this.#writeWorkspaceState();
		return true;
	}

	focusWorkspaceConversation(id: string): void {
		this.focusWorkspace(id);
	}

	setWorkspaceConversation(id: string, conversationId: string): void {
		const workspace = this.workspaceAt(id);
		if (!workspace) return;
		workspace.conversationId = conversationId;
		const context = this.workspaceContextFor(id);
		if (context) context.conversationId = conversationId;
		this.#writeWorkspaceState();
	}

	/** Remember a folder explicitly opened by the user and unhide it if needed. */
	rememberRepository(path: string | null | undefined): void {
		const normalized = path?.trim();
		if (!normalized) return;
		if (this.hiddenRepositoryPaths.includes(normalized)) {
			this.hiddenRepositoryPaths = this.hiddenRepositoryPaths.filter((item) => item !== normalized);
			this.#writeStringList(HIDDEN_REPOSITORIES_KEY, this.hiddenRepositoryPaths);
		}
		if (this.repositoryPaths.includes(normalized)) return;
		this.repositoryPaths = [...this.repositoryPaths, normalized];
		this.#writeStringList(REPOSITORIES_KEY, this.repositoryPaths);
	}

	/** Migrate workspace paths found in archived conversations without unhiding one. */
	rememberRepositories(paths: (string | null | undefined)[]): void {
		const hidden = new Set(this.hiddenRepositoryPaths);
		const additions = paths
			.map((path) => path?.trim())
			.filter((path): path is string => !!path && !hidden.has(path) && !this.repositoryPaths.includes(path));
		if (!additions.length) return;
		this.repositoryPaths = [...this.repositoryPaths, ...new Set(additions)];
		this.#writeStringList(REPOSITORIES_KEY, this.repositoryPaths);
	}

	/** Preserve a conversation group when its original folder cannot be mounted. */
	markRepositoryHistoryOnly(path: string): void {
		const normalized = path.trim();
		if (!normalized) return;
		if (this.historyOnlyRepositoryPaths.includes(normalized)) return;
		this.historyOnlyRepositoryPaths = [...this.historyOnlyRepositoryPaths, normalized];
		this.#writeStringList(HISTORY_ONLY_REPOSITORIES_KEY, this.historyOnlyRepositoryPaths);
	}

	/** Open a missing repository's archived conversations in a history-only
	 *  workspace window. Its owner stays the original folder; no other catalog
	 *  can be borrowed for analysis. */
	openHistoryOnlyWorkspace(path: string, conversationId = ''): boolean {
		const existing = this.workspaceAt(path);
		if (existing) {
			if (conversationId) existing.conversationId = conversationId;
			return this.focusWorkspace(path, conversationId || undefined);
		}
		this.markRepositoryHistoryOnly(path);
		this.rememberRepository(path);
		const workspace: WorkspaceWindow = {
			id: path,
			kind: 'repository',
			path,
			catalog: { workspace: null, sources: [] },
			conversationId,
			historyOnly: true
		};
		this.workspaceWindows = [...this.workspaceWindows, workspace];
		if (!this.focusWorkspace(path, conversationId || undefined)) return false;
		this.#writeWorkspaceState();
		return true;
	}

	/** A successful mount restores the repository's normal, live state. */
	markRepositoryAvailable(path: string): void {
		const normalized = path.trim();
		if (!normalized) return;
		const workspace = this.workspaceAt(normalized);
		if (workspace) workspace.historyOnly = false;
		if (this.historyOnlyRepositoryPaths.includes(normalized)) {
			this.historyOnlyRepositoryPaths = this.historyOnlyRepositoryPaths.filter((item) => item !== normalized);
			this.#writeStringList(HISTORY_ONLY_REPOSITORIES_KEY, this.historyOnlyRepositoryPaths);
		}
	}

	/** Hide a repository from navigation without deleting its conversations or files. */
	forgetRepository(path: string): void {
		this.repositoryPaths = this.repositoryPaths.filter((item) => item !== path);
		this.historyOnlyRepositoryPaths = this.historyOnlyRepositoryPaths.filter((item) => item !== path);
		if (!this.hiddenRepositoryPaths.includes(path)) {
			this.hiddenRepositoryPaths = [...this.hiddenRepositoryPaths, path];
		}
		this.#writeStringList(REPOSITORIES_KEY, this.repositoryPaths);
		this.#writeStringList(HIDDEN_REPOSITORIES_KEY, this.hiddenRepositoryPaths);
		this.#writeStringList(HISTORY_ONLY_REPOSITORIES_KEY, this.historyOnlyRepositoryPaths);
	}

	/** Prevent a workspace's live transcripts from being re-archived while its
	 *  saved conversations are being deleted. Wait for any archive already in
	 *  flight before the caller takes its final archive list. */
	async beginWorkspaceDeletion(path: string): Promise<boolean> {
		if (this.#deletingWorkspacePaths.has(path)) return false;
		if (this.conversations.some((conversation) => conversation.workspaceScope === path && conversation.busy)) return false;
		this.#deletingWorkspacePaths.add(path);
		await Promise.all(this.#pendingArchivesByWorkspace.get(path) ?? []);
		return true;
	}

	cancelWorkspaceDeletion(path: string): void {
		this.#deletingWorkspacePaths.delete(path);
	}

	/** Remove a workspace from Fella and clear its conversation/context references
	 *  after its archived conversations have been deleted from the backend. */
	async deleteWorkspaceAndConversations(path: string): Promise<void> {
		this.#deletingWorkspacePaths.delete(path);
		const removed = this.conversations.filter((conversation) => conversation.workspaceScope === path);
		const removedIds = new Set(removed.map((conversation) => conversation.id));
		for (const conversation of removed) {
			conversation.dropSnapshot();
			if (isDesktop()) void ipc.forgetConversation(conversation.id).catch(() => {});
		}
		this.conversations = this.conversations.filter((conversation) => conversation.workspaceScope !== path);
		this.workspaceContexts = this.workspaceContexts.filter((context) => context.workspaceId !== path);
		const generalContext = this.#ensureWorkspaceContext(null);
		if (removedIds.has(generalContext.conversationId)) generalContext.conversationId = '';
		if (this.activeWorkspaceId === path) this.activeWorkspaceId = null;

		this.workspaceWindows = this.workspaceWindows.filter((workspace) => workspace.id !== path);
		this.repositoryPaths = this.repositoryPaths.filter((repository) => repository !== path);
		if (!this.hiddenRepositoryPaths.includes(path)) this.hiddenRepositoryPaths = [...this.hiddenRepositoryPaths, path];
		this.historyOnlyRepositoryPaths = this.historyOnlyRepositoryPaths.filter((repository) => repository !== path);
		this.#writeStringList(REPOSITORIES_KEY, this.repositoryPaths);
		this.#writeStringList(HIDDEN_REPOSITORIES_KEY, this.hiddenRepositoryPaths);
		this.#writeStringList(HISTORY_ONLY_REPOSITORIES_KEY, this.historyOnlyRepositoryPaths);

		if (this.catalog.workspace === path) this.catalog = { workspace: null, sources: [] };
		if (this.activeProject?.workspace === path) {
			this.activeProjectId = null;
			this.workspaceView = 'ask';
		}
		this.historyVersion++;
		this.#writeIndex();
		this.focusWorkspace(this.activeWorkspaceId ?? GENERAL_WORKSPACE_ID);
		this.#writeWorkspaceState();
		if (isDesktop()) {
			try { await ipc.closeWorkspace(path); } catch (error) { console.warn('workspace close failed after deletion', error); }
		}
	}

	get activeProject(): Project | null {
		return this.projects.find((project) => project.id === this.activeProjectId) ?? null;
	}

	projectForWorkspace(workspace: string): Project | null {
		return this.projects.find((project) => project.workspace === workspace) ?? null;
	}

	createProject(name: string, workspace: string): Project | null {
		const existing = this.projectForWorkspace(workspace);
		if (existing) {
			this.activeProjectId = existing.id;
			this.workspaceView = 'project';
			return existing;
		}

		const now = Date.now();
		const project: Project = {
			id: `project-${uid()}`,
			name: name.trim() || 'Untitled project',
			workspace,
			body: '',
			references: [],
			created_at_ms: now,
			updated_at_ms: now
		};
		this.projects = [project, ...this.projects];
		this.#writeProjects();
		this.activeProjectId = project.id;
		this.workspaceView = 'project';
		return project;
	}

	updateProject(id: string, patch: Partial<Pick<Project, 'name' | 'body' | 'references'>>): void {
		this.projects = this.projects.map((project) =>
			project.id === id ? { ...project, ...patch, updated_at_ms: Date.now() } : project
		);
		this.#writeProjects();
	}

	openProject(id: string): void {
		if (!this.projects.some((project) => project.id === id)) return;
		this.activeProjectId = id;
		this.workspaceView = 'project';
	}

	deleteProject(id: string): void {
		this.projects = this.projects.filter((project) => project.id !== id);
		this.#writeProjects();
		if (this.activeProjectId === id) {
			this.activeProjectId = null;
			this.workspaceView = 'ask';
		}
	}

	toggleSidebar(): void {
		this.sidebarCollapsed = !this.sidebarCollapsed;
		try {
			localStorage.setItem(SIDEBAR_KEY, this.sidebarCollapsed ? '1' : '0');
		} catch {
			/* ignore */
		}
	}

	setWorkspaceView(view: WorkspaceView): void {
		if (view !== 'ask') this.workspaceToolChooserOpen = false;
		if (view === 'workspace' && !this.catalog.workspace) {
			this.workspaceView = 'ask';
			return;
		}
		if (view === 'workspace') {
			this.setWorkspacePane('investigation');
			return;
		}
		this.workspaceView = view;
	}

	setWorkspacePane(pane: WorkspacePane): void {
		if (!this.catalog.workspace) {
			this.workspaceView = 'ask';
			return;
		}
		const tab = this.openWorkspaceToolTab(
			pane,
			pane === 'investigation' ? this.activeInvestigationId : null,
			pane === 'investigation' ? this.activeChat.id : null
		);
		if (tab) this.activateWorkspaceToolTab(tab.id);
	}

	/** Open a workspace tool as a durable pane-level tab. Reusable navigation
	 *  selects an existing workspace-wide tool; the Tools add menu can request an
	 *  independent second instance. Each Investigation remains tied to its own
	 *  canonical conversation. */
	openWorkspaceToolTab(
		kind: WorkspaceToolKind,
		investigationId: string | null = null,
		conversationId: string | null = kind === 'investigation' ? this.activeChat.id : null,
		newInstance = false
	): WorkspaceToolTab | null {
		if (kind === 'studio') return this.openStudioToolTab();
		const context = this.activeWorkspaceContext;
		if (!context || !this.catalog.workspace) return null;
		if (kind === 'investigation' && conversationId && conversationId !== this.activeChat.id) return null;
		context.workspaceToolTabs ??= [];
		const activeTab = context.workspaceToolTabs.find((item) => item.id === context.activeWorkspaceToolTabId);
		const tabByInvestigation = investigationId
			? context.workspaceToolTabs.find((item) => item.kind === 'investigation' && item.investigationId === investigationId)
			: null;
		const tabByConversation = conversationId
			? context.workspaceToolTabs.find((item) => item.kind === 'investigation' && item.conversationId === conversationId)
			: null;
		// A blank Investigation tab may acquire its durable record from the chat.
		// A tab already owned by a different Investigation is never a fallback.
		let tab: WorkspaceToolTab | null | undefined;
		if (kind === 'context') {
			tab = newInstance ? null : context.workspaceToolTabs.find((item) => item.kind === 'context');
		} else if (kind === 'terminal') {
			tab = newInstance ? null : context.workspaceToolTabs.find((item) => item.kind === 'terminal');
		} else {
			tab = tabByInvestigation;
			if (!tab && tabByConversation && (!investigationId || !tabByConversation.investigationId)) {
				tab = tabByConversation;
			}
		}
		if (
			kind === 'investigation' && investigationId &&
			activeTab?.kind === 'investigation' &&
			activeTab.conversationId === conversationId &&
			activeTab.investigationId &&
			activeTab.investigationId !== investigationId
		) return null;
		if (!tab) {
			const hasSameKind = context.workspaceToolTabs.some((item) => item.kind === kind);
			tab = {
				id: kind === 'context'
					? hasSameKind ? `context:${uid()}` : 'context'
					: kind === 'terminal'
						? hasSameKind ? `terminal:${uid()}` : 'terminal'
						: `investigation:${uid()}`,
				kind,
				investigationId: kind === 'investigation' ? investigationId : null,
				conversationId: kind === 'investigation' ? conversationId : null
			};
			context.workspaceToolTabs.push(tab);
		}
		if (kind === 'investigation') {
			if (investigationId && !tab.investigationId) tab.investigationId = investigationId;
			if (conversationId) tab.conversationId = conversationId;
		}
		context.activeWorkspaceToolTabId = tab.id;
		context.workspacePane = kind === 'investigation' ? 'investigation' : 'context';
		context.investigationId = kind === 'investigation' ? tab.investigationId ?? null : null;
		context.workspacePaneOpen = true;
		this.workspacePane = context.workspacePane;
		this.workspaceToolChooserOpen = false;
		this.workspaceView = 'ask';
		this.#writeWorkspaceState();
		return tab;
	}

	openStudioToolTab(documentId: string | null = null, artifactId: string | null = null): WorkspaceToolTab | null {
		this.workspaceSection = 'reports';
		const context = this.activeWorkspaceContext;
		if (!context || !this.catalog.workspace) return null;
		context.workspaceToolTabs ??= [];
		let tab = documentId ? context.workspaceToolTabs.find((item) => item.kind === 'studio' && item.documentId === documentId) : undefined;
		if (!tab) {
			tab = { id: `studio:${uid()}`, kind: 'studio', documentId, initialArtifactId: artifactId };
			context.workspaceToolTabs.push(tab);
		} else if (artifactId) tab.initialArtifactId = artifactId;
		this.activateWorkspaceToolTab(tab.id);
		return tab;
	}

	openNewReportTab(): void {
		const tab = this.openStudioToolTab();
		if (tab) tab.newReport = true;
	}

	openResultToolTab(artifactId: string, revision: number, title: string): WorkspaceToolTab | null {
		const context = this.activeWorkspaceContext;
		if (!context || !this.catalog.workspace || !artifactId || !Number.isSafeInteger(revision) || revision < 1) return null;
		context.workspaceToolTabs ??= [];
		let tab = context.workspaceToolTabs.find(item => item.kind === 'result' && item.artifactId === artifactId && item.artifactRevision === revision);
		if (!tab) {
			tab = { id: `result:${uid()}`, kind: 'result', artifactId, artifactRevision: revision, title };
			context.workspaceToolTabs.push(tab);
		}
		this.activateWorkspaceToolTab(tab.id);
		return tab;
	}

	setStudioTabDocument(tabId: string, documentId: string, title?: string): void {
		const tab = this.workspaceToolTabs.find((item) => item.id === tabId && item.kind === 'studio');
		if (!tab) return;
		tab.documentId = documentId;
		tab.newReport = false;
		if (title) tab.title = title;
		tab.initialArtifactId = null;
		this.reportsVersion++;
		this.#writeWorkspaceState();
	}

	/** Open a read-only catalog source preview in Tools without changing the
	 *  active Conversation, Investigation scope, or next-message context. */
	openSourceToolTab(source: SourceInfo): WorkspaceToolTab | null {
		const context = this.activeWorkspaceContext;
		const workspace = this.catalog.workspace;
		if (!context || !workspace || !this.catalog.sources.some((item) => item.path === source.path && item.name === source.name)) return null;
		context.workspaceToolTabs ??= [];
		const existing = context.workspaceToolTabs.find((tab) =>
			tab.kind === 'source' && tab.sourcePath === source.path && tab.sourceName === source.name
		);
		const tab = existing ?? {
			id: `source:${uid()}`,
			kind: 'source' as const,
			sourcePath: source.path,
			sourceName: source.name
		};
		if (!existing) context.workspaceToolTabs.push(tab);
		context.activeWorkspaceToolTabId = tab.id;
		context.workspacePane = 'context';
		context.investigationId = null;
		context.workspacePaneOpen = true;
		this.workspacePane = 'context';
		this.workspaceToolChooserOpen = false;
		this.workspaceView = 'ask';
		this.#writeWorkspaceState();
		return tab;
	}

	activateWorkspaceToolTab(tabId: string): boolean {
		const context = this.activeWorkspaceContext;
		const tab = context?.workspaceToolTabs?.find((item) => item.id === tabId);
		if (!context || !tab) return false;
		context.activeWorkspaceToolTabId = tab.id;
		context.workspacePane = tab.kind === 'investigation' ? 'investigation' : 'context';
		context.investigationId = tab.kind === 'investigation' ? tab.investigationId ?? null : null;
		context.workspacePaneOpen = true;
		this.workspacePane = tab.kind === 'investigation' ? 'investigation' : 'context';
		this.workspaceToolChooserOpen = false;
		this.workspaceView = 'ask';
		if (tab.kind === 'investigation') this.activeInvestigationId = tab.investigationId ?? null;
		this.#writeWorkspaceState();
		return true;
	}

	/** Display an existing artifact in the same tabbed Tools frame. References
	 * retain their original workspace revision; opening a tab never reruns it. */
	openPreviewToolTab(preview: CompanionPane): WorkspaceToolTab | null {
		const context = this.activeWorkspaceContext;
		if (!context || !this.catalog.workspace) return null;
		context.workspaceToolTabs ??= [];
		const identity = JSON.stringify(preview);
		let tab = context.workspaceToolTabs.find((item) => item.kind === 'preview' && item.conversationId === this.activeChat.id && JSON.stringify(item.preview) === identity);
		if (!tab) {
			tab = { id: `preview:${uid()}`, kind: 'preview', conversationId: this.activeChat.id, preview };
			context.workspaceToolTabs.push(tab);
		}
		this.activateWorkspaceToolTab(tab.id);
		return tab;
	}

	closeWorkspaceToolTab(tabId: string): boolean {
		const context = this.activeWorkspaceContext;
		if (!context?.workspaceToolTabs) return false;
		const index = context.workspaceToolTabs.findIndex((item) => item.id === tabId);
		if (index < 0) return false;
		const wasActive = context.activeWorkspaceToolTabId === tabId;
		context.workspaceToolTabs.splice(index, 1);
		if (wasActive) {
			const next = context.workspaceToolTabs[Math.min(index, context.workspaceToolTabs.length - 1)] ?? null;
			context.activeWorkspaceToolTabId = next?.id ?? null;
			context.workspacePane = next?.kind === 'investigation' ? 'investigation' : next ? 'context' : 'investigation';
			context.investigationId = next?.kind === 'investigation' ? next.investigationId ?? null : null;
			this.workspacePane = context.workspacePane;
			this.activeInvestigationId = context.investigationId ?? null;
		}
		context.workspacePaneOpen = true;
		this.workspaceToolChooserOpen = context.workspaceToolTabs.length === 0;
		this.#writeWorkspaceState();
		return true;
	}

	openWorkspaceToolChooser(): void {
		if (!this.catalog.workspace || !this.activeWorkspaceContext) return;
		this.setWorkspacePaneOpen(true);
		this.workspaceToolChooserOpen = true;
	}

	setWorkspacePaneOpen(open: boolean): void {
		if (open && !this.catalog.workspace) return;
		const context = this.activeWorkspaceContext;
		if (!context) return;
		context.workspacePaneOpen = open;
		this.workspaceView = 'ask';
		this.workspaceToolChooserOpen = open && !this.activeWorkspaceToolTab;
		this.#writeWorkspaceState();
	}

	toggleWorkspacePane(): void {
		if (!this.catalog.workspace) return;
		this.setWorkspacePaneOpen(!this.workspacePaneOpen);
	}

	openInvestigationRun(turnId: string): boolean {
		const conversation = this.activeChat;
		const workspaceId = conversation.workspaceScope;
		if (!workspaceId || this.catalog.workspace !== workspaceId || !turnId.trim()) return false;
		this.investigationNavigationTarget = {
			workspaceId,
			conversationId: conversation.id,
			turnId
		};
		this.setWorkspacePane('investigation');
		return this.workspacePaneOpen;
	}

	clearInvestigationNavigationTarget(target: InvestigationNavigationTarget): void {
		if (this.investigationNavigationTarget === target) this.investigationNavigationTarget = null;
	}

	focusConversationAnalysis(conversationId: string, turnId: string): void {
		this.conversationAnalysisFocus = { conversationId, turnId };
	}

	clearConversationAnalysisFocus(target: ConversationAnalysisFocus): void {
		if (this.conversationAnalysisFocus === target) this.conversationAnalysisFocus = null;
	}

	selectSource(path: string | null): void {
		this.selectedSourcePath = path;
	}

	/** Open a catalog-backed source without changing the conversation's data
	 * scope. The pane is a user interface choice, not analysis context. */
	openSourcePane(source: SourceInfo): void {
		const workspace = this.catalog.workspace;
		if (!workspace || !this.catalog.sources.some((item) => item.path === source.path)) return;
		if (this.activeChat.workspaceScope === workspace) {
			this.activeChat.companionPane = null;
			this.openSourceToolTab(source);
			this.persist();
			return;
		}
		this.setWorkspacePaneOpen(false);
		this.activeChat.companionPane = {
			kind: 'source',
			path: source.path,
			name: source.name,
			workspacePath: workspace,
			revision: this.catalog.revision ?? null
		};
		this.workspaceView = 'ask';
		this.#writeWorkspaceState();
		this.persist();
	}

	/** Open an already-produced chart by its transcript/evidence identity. */
	openChartPane(messageId: string, evidenceIndex: number): void {
		const message = this.activeChat.messages.find((item) => item.id === messageId);
		const evidence = message?.answer?.evidence[evidenceIndex];
		if (
			message?.role !== 'assistant' ||
			!message.answer ||
			!evidence ||
			!isRenderableChartEvidence(evidence, message.answer.verification ?? [])
		) return;
		const preview: CompanionPane = {
			kind: 'chart',
			messageId,
			evidenceIndex,
			...(evidence.id ? { evidenceId: evidence.id } : {}),
			workspacePath: message.answer?.workspace?.path ?? this.activeChat.workspaceScope ?? null,
			revision: message.answer?.workspace?.revision ?? null
		};
		if (this.catalog.workspace && this.activeChat.workspaceScope === this.catalog.workspace) {
			this.activeChat.companionPane = null;
			this.openPreviewToolTab(preview);
		} else {
			this.setWorkspacePaneOpen(false);
			this.activeChat.companionPane = preview;
		}
		this.workspaceView = 'ask';
		this.#writeWorkspaceState();
		this.persist();
	}

	closeCompanionPane(): void {
		this.activeChat.companionPane = null;
		this.persist();
	}

	/** Select a conversation inside its owning workspace. */
	activateConversation(index: number): boolean {
		const conversation = this.conversations[index];
		if (!conversation) return false;
		const scope = conversation.workspaceScope ?? null;
		if (scope && !this.workspaceAt(scope)) return false;
		return this.focusWorkspace(scope ?? GENERAL_WORKSPACE_ID, conversation.id);
	}


	addContextReference(ref: ContextReference): void {
		this.ensureChat().addContext(ref);
	}

	removeContextReference(kind: ContextReference['kind'], key: string): void {
		this.activeChat?.removeContext(kind, key);
	}

	clearContextReferences(): void {
		this.activeChat?.clearContext();
	}

	get activeConversation(): Conversation {
		return this.conversations[this.activeConversationIndex] ?? this.conversations[0];
	}

	/** The focused conversation used by the command and ask paths. */
	get activeChat(): Conversation {
		return this.activeConversation;
	}

	/** The optional right-side tools pane belongs to the active workspace pane. */
	get workspacePaneOpen(): boolean {
		return this.activeWorkspaceContext?.workspacePaneOpen === true;
	}

	/** The active conversation's model, or the saved default when unset. */
	get model(): string {
		const m = this.activeConversation.model;
		return m || this.settings?.model || '';
	}

	// --- per-conversation facade -> the active conversation -----------------
	get messages(): Message[] {
		return this.activeChat?.messages ?? [];
	}
	set messages(v: Message[]) {
		if (this.activeChat) this.activeChat.messages = v;
	}
	get conversationId(): string {
		return this.activeChat?.id ?? this.activeConversation.id;
	}
	get busy(): boolean {
		return this.activeChat?.busy ?? false;
	}
	set busy(v: boolean) {
		if (this.activeChat) this.activeChat.busy = v;
	}
	get activity(): string {
		return this.activeChat?.activity ?? '';
	}
	set activity(v: string) {
		if (this.activeChat) this.activeChat.activity = v;
	}
	get pendingKey(): { provider: string; display: string } | null {
		return this.activeChat?.pendingKey ?? null;
	}
	set pendingKey(v: { provider: string; display: string } | null) {
		if (this.activeChat) this.activeChat.pendingKey = v;
	}
	/** The conversation to act on for a slash command. */
	ensureChat(): Conversation {
		return this.activeChat;
	}

	addUser(text: string): Message {
		return this.ensureChat().addUser(text);
	}
	addAssistant(text = ''): Message {
		return this.ensureChat().addAssistant(text);
	}
	addSystem(text: string): Message {
		return this.ensureChat().addSystem(text);
	}

	// --- conversation creation inside the focused workspace -----------------
	newConversation(workspaceId: string | null = this.activeWorkspaceId): boolean {
		const scope = workspaceId === GENERAL_WORKSPACE_ID ? null : workspaceId;
		const workspace = scope ? this.workspaceAt(scope) : null;
		if (scope && !workspace) return false;
		const context = this.#ensureWorkspaceContext(scope);
		const conversation = new Conversation();
		conversation.model = this.model;
		conversation.workspaceScope = scope;
		this.conversations.push(conversation);
		context.conversationId = conversation.id;
		context.investigationId = null;
		context.activeWorkspaceToolTabId = null;
		this.activeInvestigationId = null;
		// A deliberate New conversation action starts in chat with the optional
		// workspace tools pane closed.
		context.workspacePaneOpen = false;
		this.focusWorkspace(scope ?? GENERAL_WORKSPACE_ID, conversation.id);
		this.workspaceView = 'ask';
		this.#writeIndex();
		this.#writeWorkspaceState();
		return true;
	}

	/** Load an archived conversation into its owner's active conversation slot. */
	loadConversation(
		id: string,
		messages: Message[],
		title: string | null = null,
		workspace: string | null = null,
		companionPane: unknown = null,
		model: string | null = null
	): void {
		const existing = this.conversations.findIndex((conversation) => conversation.id === id);
		if (existing >= 0) {
			const conversation = this.conversations[existing];
			if (typeof model === 'string') conversation.model = model;
			if (conversation.workspaceScope === undefined) conversation.workspaceScope = workspace;
			conversation.companionPane = parseCompanionPane(companionPane) ?? conversation.companionPane;
			const context = this.#ensureWorkspaceContext(workspace ?? null);
			if (context) {
				if (context.conversationId !== id) context.investigationId = null;
				context.conversationId = id;
			}
			const owner = workspace ? this.workspaceAt(workspace) : null;
			if (owner) owner.conversationId = id;
			this.activeConversationIndex = existing;
			return;
		}
		const conversation = new Conversation(id);
		conversation.model = typeof model === 'string' ? model : this.model;
		conversation.title = title;
		conversation.workspaceScope = workspace;
		conversation.companionPane = parseCompanionPane(companionPane);
		// A reloaded transcript never has a run in flight.
		conversation.messages = messages.map((m) => (m.pending ? { ...m, pending: false } : m));
		this.conversations.push(conversation);
		const context = this.#ensureWorkspaceContext(workspace ?? null);
		if (context) {
			if (context.conversationId !== id) context.investigationId = null;
			context.conversationId = id;
		}
		const owner = workspace ? this.workspaceAt(workspace) : null;
		if (owner) owner.conversationId = id;
		this.activeConversationIndex = this.conversations.length - 1;
		this.#writeIndex();
	}

	/** Rename a conversation, live or archived-only. `title` empty/whitespace
	 *  clears a custom name back to the derived folder + first-message one. */
	async renameConversation(id: string, title: string): Promise<void> {
		const trimmed = title.trim();
		const conversation = this.conversations.find((item) => item.id === id);
		if (conversation) {
			conversation.title = trimmed || null;
			await this.#archive(conversation);
			return;
		}
		if (!isDesktop()) return;
		await ipc.renameConversation(id, trimmed);
		this.historyVersion++;
	}


	/** Remove a deleted conversation from live state and remove its
	 *  conversation-owned Investigation/preview tabs. */
	removeConversationWithoutArchiving(id: string): void {
		delete this.investigationNames[id];
		const i = this.conversations.findIndex((conversation) => conversation.id === id);
		const conversation = i >= 0 ? this.conversations[i] : null;
		const ownerId = conversation?.workspaceScope ??
			this.workspaceContexts.find((context) => context.conversationId === id)?.workspaceId ?? null;
		const wasActive = i >= 0 && i === this.activeConversationIndex;
		if (conversation) {
			conversation.dropSnapshot();
			if (isDesktop()) void ipc.forgetConversation(conversation.id).catch(() => {});
			this.conversations.splice(i, 1);
		}

		let removedToolState = false;
		for (const context of this.workspaceContexts) {
			const tabs = context.workspaceToolTabs ?? [];
			const removedTabs = tabs.filter((tab) =>
				tab.conversationId === id ||
				(tab.kind === 'investigation' && !tab.conversationId && context.conversationId === id)
			);
			if (removedTabs.length) {
				const removedIds = new Set(removedTabs.map((tab) => tab.id));
				context.workspaceToolTabs = tabs.filter((tab) => !removedIds.has(tab.id));
				const activeTabWasRemoved = Boolean(context.activeWorkspaceToolTabId && removedIds.has(context.activeWorkspaceToolTabId));
				if (activeTabWasRemoved) {
					const next = context.workspaceToolTabs[Math.min(tabs.findIndex((tab) => removedIds.has(tab.id)), context.workspaceToolTabs.length - 1)] ?? null;
					context.activeWorkspaceToolTabId = next?.id ?? null;
					context.workspacePane = next?.kind === 'investigation' ? 'investigation' : 'context';
					context.investigationId = next?.kind === 'investigation' ? next.investigationId ?? null : null;
					context.workspacePaneOpen = Boolean(next);
				} else if (context.investigationId && removedTabs.some((tab) => tab.investigationId === context.investigationId)) {
					const activeTab = context.workspaceToolTabs.find((tab) => tab.id === context.activeWorkspaceToolTabId);
					context.investigationId = activeTab?.kind === 'investigation' ? activeTab.investigationId ?? null : null;
				}
				removedToolState = true;
			}
			if (context.conversationId === id) {
				const replacement = this.conversations.findLast((item) => item.workspaceScope === context.workspaceId);
				if (replacement) context.conversationId = replacement.id;
				else {
					const blank = new Conversation();
					blank.workspaceScope = context.workspaceId;
					this.conversations.push(blank);
					context.conversationId = blank.id;
				}
				const activeTab = context.workspaceToolTabs?.find((tab) => tab.id === context.activeWorkspaceToolTabId);
				if (activeTab?.kind === 'investigation') {
					context.investigationId = activeTab.investigationId ?? null;
					if (activeTab.conversationId) context.conversationId = activeTab.conversationId;
				} else context.investigationId = null;
				removedToolState = true;
			}
		}
		const owner = ownerId ? this.workspaceAt(ownerId) : null;
		if (owner?.conversationId === id) {
			const replacement = this.conversations.findLast((item) => item.workspaceScope === owner.path);
			if (replacement) owner.conversationId = replacement.id;
		}
		this.investigationsVersion++;
		if (removedToolState) {
			if (this.activeWorkspaceContext) {
				this.activeInvestigationId = this.activeWorkspaceContext.investigationId ?? null;
				this.workspacePane = this.activeWorkspaceContext.workspacePane ?? 'investigation';
				if (!this.activeWorkspaceContext.workspacePaneOpen) this.workspaceToolChooserOpen = false;
			}
			this.#writeWorkspaceState();
		}
		if (i < 0) return;
		if (this.conversations.length === 0) {
			const replacement = new Conversation();
			replacement.workspaceScope = ownerId;
			this.conversations.push(replacement);
			if (owner) owner.conversationId = replacement.id;
			this.#ensureWorkspaceContext(ownerId).conversationId = replacement.id;
		}
		if (wasActive) {
			this.activateConversation(this.conversations.findIndex((item) => item.id === (owner?.conversationId ?? this.activeWorkspaceContext?.conversationId)));
		} else {
			if (this.activeConversationIndex > i) this.activeConversationIndex -= 1;
			this.activeConversationIndex = Math.min(this.activeConversationIndex, this.conversations.length - 1);
		}
		this.#writeIndex();
	}

	/** End the active conversation and start the slot blank. */
	async clear(): Promise<void> {
		const conversation = this.activeChat;
		await this.#archive(conversation);
		conversation.dropSnapshot();
		const replacement = new Conversation();
		replacement.workspaceScope = conversation.workspaceScope ?? null;
		this.conversations[this.activeConversationIndex] = replacement;
		const workspace = conversation.workspaceScope ? this.workspaceAt(conversation.workspaceScope) : null;
		if (workspace) workspace.conversationId = replacement.id;
		if (this.activeWorkspaceContext) this.activeWorkspaceContext.conversationId = replacement.id;
		this.activateConversation(this.activeConversationIndex);
		this.#writeIndex();
	}

	/** Called once on launch. Archives every transcript a previous run left
	 *  behind (one blob per conversation, plus the legacy single blob) and starts
	 *  the active workspace context with an empty conversation; transcripts are
	 *  loaded from history on demand. */
	async rollOver(): Promise<void> {
		let keys: string[];
		try {
			keys = Object.keys(localStorage).filter((k) => k === LEGACY_KEY || k.startsWith(PREFIX));
		} catch {
			const replacement = new Conversation();
			replacement.workspaceScope = this.activeWorkspaceContext?.workspaceId ?? null;
			this.conversations = [replacement];
			this.activeConversationIndex = 0;
			for (const context of this.workspaceContexts) context.conversationId = '';
			for (const workspace of this.workspaceWindows) workspace.conversationId = '';
			if (this.activeWorkspaceContext) this.activeWorkspaceContext.conversationId = replacement.id;
			if (this.activeWorkspaceId) {
				const workspace = this.workspaceAt(this.activeWorkspaceId);
				if (workspace) workspace.conversationId = replacement.id;
			}
			return;
		}
		for (const k of keys) {
			let raw: string | null = null;
			try {
				raw = localStorage.getItem(k);
			} catch {
				continue;
			}
			// Remove the key unless the archive call itself failed (then keep it
			// for the next launch to retry).
			if (await this.#archiveRaw(raw)) {
				try {
					localStorage.removeItem(k);
				} catch {
					/* ignore */
				}
			}
		}
		try {
			localStorage.removeItem(INDEX_KEY);
		} catch {
			/* ignore */
		}
		const replacement = new Conversation();
		replacement.workspaceScope = this.activeWorkspaceContext?.workspaceId ?? null;
		this.conversations = [replacement];
		this.activeConversationIndex = 0;
		for (const context of this.workspaceContexts) context.conversationId = '';
		for (const workspace of this.workspaceWindows) workspace.conversationId = '';
		if (this.activeWorkspaceContext) this.activeWorkspaceContext.conversationId = replacement.id;
		if (this.activeWorkspaceContext?.workspaceId) {
			const workspace = this.workspaceAt(this.activeWorkspaceContext.workspaceId);
			if (workspace) workspace.conversationId = replacement.id;
		}
		this.historyVersion++;
	}

	/** Persist every live conversation's transcript (each debounces its own
	 *  write).
	 *
	 *  Also archives a conversation into history as soon as its first exchange
	 *  settles, not just on close/clear -- otherwise a conversation you're
	 *  still actively having doesn't show up in the sidebar until you're
	 *  done with it, which reads as "it didn't save" rather than "it hasn't
	 *  been archived yet". `#archive` re-runs (and just overwrites the same
	 *  file) on every later settle too, so the sidebar's title/preview
	 *  reflects the real conversation while it continues. */
	persist(): void {
		for (const conversation of this.conversations) {
			if (conversation.workspaceScope && this.#deletingWorkspacePaths.has(conversation.workspaceScope)) continue;
			conversation.persist();
			if (!conversation.busy && conversation.messages.length > 0) void this.#archive(conversation);
		}
		this.#writeIndex();
	}

	#writeIndex(): void {
		try {
			localStorage.setItem(INDEX_KEY, JSON.stringify(this.conversations.map((conversation) => conversation.id)));
		} catch {
			/* ignore */
		}
	}

	async #archive(conversation: Conversation): Promise<void> {
		if (conversation.messages.length === 0 || !isDesktop()) return;
		const workspace = conversation.workspaceScope;
		if (workspace && this.#deletingWorkspacePaths.has(workspace)) return;
		const body = JSON.stringify({
			id: conversation.id,
			saved_at_ms: Date.now(),
			workspace: conversation.workspaceScope ?? null,
			model: conversation.model,
			messages: conversation.messages,
			title: conversation.title ?? undefined,
			companionPane: conversation.companionPane
		});
		const archive = (async () => {
			try {
				await ipc.archiveConversation(conversation.id, body);
				this.historyVersion++;
			} catch (e) {
				console.warn('archive failed', e);
			}
		})();
		if (workspace) {
			const pending = this.#pendingArchivesByWorkspace.get(workspace) ?? new Set<Promise<void>>();
			pending.add(archive);
			this.#pendingArchivesByWorkspace.set(workspace, pending);
			void archive.then(() => {
				pending.delete(archive);
				if (!pending.size) this.#pendingArchivesByWorkspace.delete(workspace);
			});
		}
		await archive;
	}

	/** Returns true when the blob has been dealt with (archived, or not worth
	 *  keeping); false only when the archive IPC threw. */
	async #archiveRaw(raw: string | null): Promise<boolean> {
		if (!raw) return true;
		let saved: { id?: string; workspace?: string | null; model?: unknown; messages?: unknown; companionPane?: unknown };
		try {
			saved = JSON.parse(raw);
		} catch {
			return true;
		}
		if (!Array.isArray(saved.messages) || saved.messages.length === 0 || !isDesktop()) return true;
		// This is a previous run's leftover conversation, recovered on launch --
		// date it by its own last message, not by "now" (the relaunch moment),
		// or it files under today's date no matter how long ago it happened.
		const last = saved.messages[saved.messages.length - 1] as { ts?: number } | undefined;
		const body = JSON.stringify({
			id: saved.id ?? '',
			saved_at_ms: last?.ts ?? Date.now(),
			workspace: archivedWorkspace(saved.messages, saved.workspace),
			...(typeof saved.model === 'string' ? { model: saved.model } : {}),
			messages: saved.messages,
			companionPane: parseCompanionPane(saved.companionPane)
		});
		try {
			await ipc.archiveConversation(String(saved.id ?? ''), body);
			return true;
		} catch (e) {
			console.warn('archive failed keeping the transcript for the next launch', e);
			return false;
		}
	}
}

export const session = new Session();
