// Shared reactive session state (Svelte 5 runes in a .svelte.ts module).
//
// `Session` owns app-level navigation and up to four open workspace windows.
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
	Settings,
	SourceInfo,
	WorkspaceProgress
} from './types';
import { isRenderableChartEvidence, parseCompanionPane, type CompanionPane } from './workbench';
import type { WorkspaceTilePreference } from './workspace-layout';


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

function humanToolName(tool: string): string {
	const labels: Record<string, string> = {
		list_files: 'Looking through the workspace',
		inspect_file: 'Inspecting a source',
		read_file: 'Reading a source',
		search_files: 'Searching the workspace',
		query: 'Calculating from the data',
		make_chart: 'Preparing a chart',
		memory: 'Checking workspace notes'
	};
	return labels[tool] ?? tool.replace(/[_-]+/g, ' ').replace(/^./, (c) => c.toUpperCase());
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
const ENVIRONMENTS_KEY = 'fella:environments';
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
	layout: WorkspaceTilePreference;
}

export interface EnvironmentPane {
	id: string;
	/** Null is the unbound General workspace; repository IDs are resolved paths. */
	workspaceId: string | null;
	conversationId: string;
}

export interface WorkspaceEnvironment {
	id: string;
	panes: EnvironmentPane[];
	activePaneId: string;
	layout: WorkspaceTilePreference;
}

interface SavedEnvironmentState {
	environments: WorkspaceEnvironment[];
	active: string;
}

function readWorkspaceState(): SavedWorkspaceState {
	try {
		const raw = localStorage.getItem(OPEN_WORKSPACES_KEY);
		if (raw === null) return { ids: [], active: null, layout: {} };
		const parsed: unknown = JSON.parse(raw);
		if (!parsed || typeof parsed !== 'object') return { ids: [], active: null, layout: {} };
		const record = parsed as Record<string, unknown>;
		const legacyPaths = Array.isArray(record.paths)
			? [...new Set(record.paths.filter((path): path is string => typeof path === 'string' && path.trim().length > 0))].slice(0, 4)
			: [];
		// General is a conversation group, not an automatically opened window.
		// Keep old repository windows, but migrate the former implicit General tile
		// out of the visible board.
		const ids = Array.isArray(record.ids)
			? [...new Set(record.ids.filter((id): id is string => typeof id === 'string' && id.trim().length > 0 && id !== GENERAL_WORKSPACE_ID))].slice(0, 4)
			: legacyPaths;
		const normalizedIds = ids;
		const active = typeof record.active === 'string' && normalizedIds.includes(record.active)
			? record.active
			: null;
		const layoutRecord = record.layout && typeof record.layout === 'object'
			? (record.layout as Record<string, unknown>)
			: {};
		const layout: WorkspaceTilePreference = {
			two: layoutRecord.two === 'stacked' ? 'stacked' : 'side-by-side',
			three: ['two-top-one-bottom', 'one-top-two-bottom', 'two-left-one-right', 'one-left-two-right'].includes(String(layoutRecord.three))
				? (layoutRecord.three as WorkspaceTilePreference['three'])
				: 'two-top-one-bottom'
		};
		return { ids: normalizedIds, active, layout };
	} catch {
		return { ids: [], active: null, layout: {} };
	}
}

const SAVED_WORKSPACE_STATE = readWorkspaceState();

function readEnvironmentState(): SavedEnvironmentState {
	const normalizeLayout = (value: unknown): WorkspaceTilePreference => {
		const record = value && typeof value === 'object' ? value as Record<string, unknown> : {};
		return {
			two: record.two === 'stacked' ? 'stacked' : 'side-by-side',
			three: ['two-top-one-bottom', 'one-top-two-bottom', 'two-left-one-right', 'one-left-two-right'].includes(String(record.three))
				? record.three as WorkspaceTilePreference['three']
				: 'two-top-one-bottom'
		};
	};
	try {
		const raw = localStorage.getItem(ENVIRONMENTS_KEY);
		if (raw) {
			const parsed: unknown = JSON.parse(raw);
			if (parsed && typeof parsed === 'object') {
				const record = parsed as Record<string, unknown>;
				if (Array.isArray(record.environments)) {
					const environments = record.environments.flatMap((value, index): WorkspaceEnvironment[] => {
						if (!value || typeof value !== 'object') return [];
						const env = value as Record<string, unknown>;
						if (typeof env.id !== 'string' || !Array.isArray(env.panes)) return [];
						const panes = env.panes.flatMap((item, paneIndex): EnvironmentPane[] => {
							if (!item || typeof item !== 'object') return [];
							const pane = item as Record<string, unknown>;
						if (pane.workspaceId !== null && typeof pane.workspaceId !== 'string') return [];
						return [{
							id: typeof pane.id === 'string' ? pane.id : `${env.id}:pane:${paneIndex}`,
							workspaceId: pane.workspaceId as string | null,
							conversationId: typeof pane.conversationId === 'string' ? pane.conversationId : ''
						}];
						}).slice(0, 4);
						if (!panes.length) panes.push({ id: `${env.id}:general`, workspaceId: null, conversationId: '' });
						const activePaneId = typeof env.activePaneId === 'string' && panes.some((pane) => pane.id === env.activePaneId)
							? env.activePaneId
							: panes[0].id;
						return [{ id: env.id, panes, activePaneId, layout: normalizeLayout(env.layout) }];
					}).slice(0, 24);
					if (environments.length) {
						const active = typeof record.active === 'string' && environments.some((env) => env.id === record.active)
							? record.active
							: environments[0].id;
						return { environments, active };
					}
				}
			}
		}
	} catch {
		/* fall through to migration from the single-board layout */
	}
	const id = 'environment-1';
	const panes: EnvironmentPane[] = SAVED_WORKSPACE_STATE.ids.map((workspaceId, index) => ({
		id: `${id}:pane:${index + 1}`,
		workspaceId,
		conversationId: ''
	}));
	if (!panes.length) panes.push({ id: `${id}:general`, workspaceId: null, conversationId: '' });
	return {
		environments: [{
			id,
			panes,
			activePaneId: panes.find((pane) => pane.workspaceId === SAVED_WORKSPACE_STATE.active)?.id ?? panes[0].id,
			layout: SAVED_WORKSPACE_STATE.layout
		}],
		active: id
	};
}

const SAVED_ENVIRONMENT_STATE = readEnvironmentState();

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
	runStartedAt = $state<number | null>(null);
	runDurationMs = $state<number | null>(null);

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

	startRun(): void {
		const now = Date.now();
		this.runStartedAt = now;
		this.runDurationMs = null;
		this.runSteps = [
			{
				id: uid(),
				label: 'Planning the question',
				state: 'running',
				started_at_ms: now
			}
		];
	}

	beginRunStep(tool: string, note?: string): void {
		const now = Date.now();
		const steps = this.runSteps.map((step) =>
			step.state === 'running' && !step.tool
				? { ...step, state: 'complete' as const, finished_at_ms: now }
				: step
		);
		this.runSteps = [
			...steps,
			{
				id: uid(),
				label: note || humanToolName(tool),
				state: 'running',
				started_at_ms: now,
				tool,
				note
			}
		];
	}

	completeRunStep(item: EvidenceItem): void {
		const now = Date.now();
		const index = this.runSteps.findLastIndex(
			(step) => step.state === 'running' && (!step.tool || step.tool === item.tool)
		);
		if (index < 0) return;
		this.runSteps = this.runSteps.map((step, i) =>
			i === index
				? { ...step, state: item.error ? ('error' as const) : ('complete' as const), finished_at_ms: now, evidence: item }
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
		if (this.runStartedAt !== null) this.runDurationMs = Math.max(0, now - this.runStartedAt);
		this.runStartedAt = null;
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


export type WorkspaceView = 'ask' | 'workspace' | 'board' | 'settings' | 'project';
export type WorkspacePane = 'sources' | 'context';

class Session {
	#deletingWorkspacePaths = new Set<string>();
	#pendingArchivesByWorkspace = new Map<string, Set<Promise<void>>>();
	catalog = $state<Catalog>({ workspace: null, sources: [] });
	/** Live repository mounts shared by environments. Panes reference these IDs. */
	workspaceWindows = $state<WorkspaceWindow[]>([]);
	activeWorkspaceId = $state<string | null>(null);
	environments = $state<WorkspaceEnvironment[]>(SAVED_ENVIRONMENT_STATE.environments);
	activeEnvironmentId = $state<string>(SAVED_ENVIRONMENT_STATE.active);
	workspaceLayout = $state<WorkspaceTilePreference>(
		SAVED_ENVIRONMENT_STATE.environments.find((env) => env.id === SAVED_ENVIRONMENT_STATE.active)?.layout ?? {}
	);
	mountProgress = $state<WorkspaceProgress | null>(null);
	/** The app-level page; an unbound conversation is the initial destination. */
	workspaceView = $state<WorkspaceView>('ask');
	/** The active pane inside Workspace. */
	workspacePane = $state<WorkspacePane>('sources');
	settings = $state<Settings | null>(null);
	health = $state<ProviderHealth | null>(null);
	/** Built-in providers from the engine, cached so the composer can hint
	 *  valid `/login` / `/logout` names without an await. */
	providers = $state<ProviderInfo[]>([]);
	/** Live conversations retain their scope; environments select them through panes. */
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
	/** Source selected by global search for the Sources master/detail view. */
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
		// Every environment has an explicit General pane available, but it never
		// implies a mounted folder. The first blank conversation belongs to that
		// pane unless a saved repository pane is restored at startup.
		const env = this.activeEnvironment;
		const pane = env?.panes.find((item) => item.id === env.activePaneId) ?? env?.panes[0];
		if (pane) {
			pane.conversationId = this.conversations[0].id;
			this.conversations[0].workspaceScope = pane.workspaceId;
			this.activeWorkspaceId = pane.workspaceId;
		}
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
			localStorage.setItem(ENVIRONMENTS_KEY, JSON.stringify({
				environments: this.environments,
				active: this.activeEnvironmentId
			}));
			const active = this.activeEnvironment;
			localStorage.setItem(OPEN_WORKSPACES_KEY, JSON.stringify({
				ids: active?.panes.map((pane) => pane.workspaceId).filter((id): id is string => !!id) ?? [],
				active: this.activeWorkspaceId,
				layout: this.workspaceLayout
			}));
		} catch {
			/* workspace state is a convenience; a full localStorage must not block analysis */
		}
	}

	get pendingWorkspacePaths(): string[] {
		return (this.activeEnvironment?.panes ?? [])
			.map((pane) => pane.workspaceId)
			.filter((id): id is string => !!id && !this.workspaceAt(id));
	}

	get savedActiveWorkspaceId(): string | null {
		return this.activePane?.workspaceId ?? null;
	}

	get activeEnvironment(): WorkspaceEnvironment | null {
		return this.environments.find((environment) => environment.id === this.activeEnvironmentId) ?? this.environments[0] ?? null;
	}

	get activeEnvironmentPanes(): EnvironmentPane[] {
		return this.activeEnvironment?.panes ?? [];
	}

	get activePane(): EnvironmentPane | null {
		const environment = this.activeEnvironment;
		return environment?.panes.find((pane) => pane.id === environment.activePaneId) ?? environment?.panes[0] ?? null;
	}

	paneAt(id: string): EnvironmentPane | null {
		return this.activeEnvironment?.panes.find((pane) => pane.id === id) ?? null;
	}

	paneForWorkspace(workspaceId: string | null): EnvironmentPane | null {
		return this.activeEnvironment?.panes.find((pane) => pane.workspaceId === workspaceId) ?? null;
	}

	environmentLabel(environment: WorkspaceEnvironment): string {
		const names = environment.panes.map((pane) => {
			if (!pane.workspaceId) return 'General';
			const workspace = this.workspaceAt(pane.workspaceId);
			return (workspace?.path ?? pane.workspaceId).replace(/[/\\]+$/, '').split(/[/\\]/).at(-1) || 'Workspace';
		});
		if (!names.length) return 'General';
		if (names.length === 1) {
			const sameName = this.environments.filter((item) => item.panes.length === 1 && this.environmentLabelBase(item) === names[0]);
			const index = sameName.findIndex((item) => item.id === environment.id);
			return sameName.length > 1 && index > 0 ? `${names[0]} ${index + 1}` : names[0];
		}
		return `${names[0]} +${names.length - 1}`;
	}

	private environmentLabelBase(environment: WorkspaceEnvironment): string {
		const pane = environment.panes[0];
		if (!pane?.workspaceId) return 'General';
		const workspace = this.workspaceAt(pane.workspaceId);
		return (workspace?.path ?? pane.workspaceId).replace(/[/\\]+$/, '').split(/[/\\]/).at(-1) || 'Workspace';
	}

	get activeWorkspace(): WorkspaceWindow | null {
		const workspaceId = this.activePane?.workspaceId;
		return workspaceId ? this.workspaceAt(workspaceId) : null;
	}

	/** The path to pass to the analytical runtime; General always maps to null. */
	get activeRepositoryPath(): string | null {
		const workspace = this.activeWorkspace;
		return workspace?.kind === 'repository' && !workspace.historyOnly ? workspace.path : null;
	}

	workspaceAt(id: string): WorkspaceWindow | null {
		return this.workspaceWindows.find((workspace) => workspace.id === id) ?? null;
	}

	setWorkspaceLayout(layout: WorkspaceTilePreference): void {
		this.workspaceLayout = { ...layout };
		if (this.activeEnvironment) this.activeEnvironment.layout = { ...layout };
		this.#writeWorkspaceState();
	}

	/** Place an environment pane in a fixed board slot; users never resize tiles. */
	placeWorkspacePane(id: string, index: number): boolean {
		const environment = this.activeEnvironment;
		if (!environment) return false;
		const currentIndex = environment.panes.findIndex((pane) => pane.id === id);
		if (currentIndex < 0) return false;
		const next = [...environment.panes];
		const [pane] = next.splice(currentIndex, 1);
		const targetIndex = Math.max(0, Math.min(Math.trunc(index), next.length));
		next.splice(targetIndex, 0, pane);
		environment.panes = next;
		this.#writeWorkspaceState();
		return true;
	}

	placeWorkspaceWindow(id: string, index: number): boolean {
		const pane = this.paneForWorkspace(id);
		return pane ? this.placeWorkspacePane(pane.id, index) : false;
	}

	createEnvironment(): boolean {
		const id = `environment-${uid()}`;
		const conversation = new Conversation();
		conversation.model = this.model;
		conversation.workspaceScope = null;
		this.conversations.push(conversation);
		const pane: EnvironmentPane = { id: `${id}:general`, workspaceId: null, conversationId: conversation.id };
		const environment: WorkspaceEnvironment = { id, panes: [pane], activePaneId: pane.id, layout: {} };
		this.environments = [...this.environments, environment];
		this.activeEnvironmentId = id;
		this.workspaceLayout = { ...environment.layout };
		this.activeWorkspaceId = null;
		this.activeConversationIndex = this.conversations.length - 1;
		this.catalog = { workspace: null, sources: [] };
		this.workspaceView = 'ask';
		this.#writeIndex();
		this.#writeWorkspaceState();
		return true;
	}

	activateEnvironment(id: string): boolean {
		const environment = this.environments.find((item) => item.id === id);
		if (!environment) return false;
		this.activeEnvironmentId = id;
		this.workspaceLayout = { ...environment.layout };
		const pane = environment.panes.find((item) => item.id === environment.activePaneId) ?? environment.panes[0];
		if (pane) this.focusEnvironmentPane(pane.id);
		else this.workspaceView = 'ask';
		this.#writeWorkspaceState();
		return true;
	}

	async closeEnvironment(id: string): Promise<boolean> {
		if (this.environments.length <= 1) return false;
		const closing = this.environments.find((item) => item.id === id);
		if (!closing) return true;
		if (closing.panes.some((pane) => this.conversations.find((item) => item.id === pane.conversationId)?.busy)) return false;
		this.environments = this.environments.filter((item) => item.id !== id);
		if (this.activeEnvironmentId === id) this.activateEnvironment(this.environments[Math.max(0, this.environments.length - 1)].id);
		const used = new Set(this.environments.flatMap((environment) => environment.panes.map((pane) => pane.workspaceId).filter((path): path is string => !!path)));
		const unused = this.workspaceWindows.filter((workspace) => !used.has(workspace.id));
		this.workspaceWindows = this.workspaceWindows.filter((workspace) => used.has(workspace.id));
		for (const workspace of unused) {
			if (workspace.path && isDesktop()) {
				try { await ipc.closeWorkspace(workspace.path); } catch (error) { console.warn('workspace close failed', error); }
			}
		}
		this.#writeWorkspaceState();
		return true;
	}

	/** Focus a pane in the current environment and restore its conversation scope. */
	focusEnvironmentPane(paneId: string): boolean {
		const environment = this.activeEnvironment;
		const pane = environment?.panes.find((item) => item.id === paneId);
		if (!environment || !pane) return false;
		environment.activePaneId = pane.id;
		const scope = pane.workspaceId;
		const workspace = scope ? this.workspaceAt(scope) : null;
		let conversationIndex = pane.conversationId
			? this.conversations.findIndex((item) => item.id === pane.conversationId && item.workspaceScope === scope)
			: -1;
		if (conversationIndex < 0) {
			const conversation = new Conversation();
			conversation.model = this.model;
			conversation.workspaceScope = scope;
			this.conversations.push(conversation);
			conversationIndex = this.conversations.length - 1;
			this.#writeIndex();
		}
		pane.conversationId = this.conversations[conversationIndex].id;
		if (workspace) workspace.conversationId = pane.conversationId;
		this.activeConversationIndex = conversationIndex;
		this.activeWorkspaceId = scope;
		this.catalog = workspace && !workspace.historyOnly ? workspace.catalog : { workspace: null, sources: [] };
		this.workspaceView = scope || environment.panes.length > 1 ? 'board' : 'ask';
		this.#writeWorkspaceState();
		return true;
	}

	/** Restore the persisted active pane after its mount has been re-established. */
	restoreActiveEnvironment(): void {
		const pane = this.activePane;
		if (pane) this.focusEnvironmentPane(pane.id);
	}

	/** Register a mounted catalog and focus its repository-owned conversation.
	 *  General conversations are never silently reassigned to a folder. */
	registerWorkspace(catalog: Catalog): boolean {
		const path = catalog.workspace;
		if (!path) return false;
		if (!this.paneForWorkspace(path) && this.activeEnvironmentPanes.length >= 4) return false;
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
		const environment = this.activeEnvironment;
		if (!environment) return false;
		let pane = environment.panes.find((item) => item.workspaceId === scope);
		if (!pane) {
			if (environment.panes.length >= 4) return false;
			pane = {
				id: `${environment.id}:pane:${uid()}`,
				workspaceId: scope,
				conversationId: conversationId ?? workspace?.conversationId ?? this.conversations.findLast((item) => item.workspaceScope === scope)?.id ?? ''
			};
			environment.panes = [...environment.panes, pane];
		}
		if (conversationId && this.conversations.some((item) => item.id === conversationId && item.workspaceScope === scope)) {
			pane.conversationId = conversationId;
		}
		return this.focusEnvironmentPane(pane.id);
	}

	focusWorkspaceConversation(id: string): void {
		this.focusWorkspace(id);
	}

	setWorkspaceConversation(id: string, conversationId: string): void {
		const workspace = this.workspaceAt(id);
		if (!workspace) return;
		workspace.conversationId = conversationId;
		const pane = this.paneForWorkspace(id);
		if (pane) pane.conversationId = conversationId;
		this.#writeWorkspaceState();
	}

	async closeWorkspaceWindow(id: string): Promise<void> {
		const environment = this.activeEnvironment;
		if (!environment) return;
		const pane = environment.panes.find((item) => item.id === id || item.workspaceId === id);
		if (!pane) return;
		const wasActive = environment.activePaneId === pane.id;
		environment.panes = environment.panes.filter((item) => item.id !== pane.id);
		if (!environment.panes.length) {
			const conversation = new Conversation();
			conversation.model = this.model;
			conversation.workspaceScope = null;
			this.conversations.push(conversation);
			const general: EnvironmentPane = { id: `${environment.id}:general`, workspaceId: null, conversationId: conversation.id };
			environment.panes = [general];
			environment.activePaneId = general.id;
		} else if (wasActive) {
			environment.activePaneId = environment.panes[0].id;
		}
		if (wasActive) this.focusEnvironmentPane(environment.activePaneId);
		const workspaceId = pane.workspaceId;
		const stillUsed = workspaceId && this.environments.some((item) => item.panes.some((entry) => entry.workspaceId === workspaceId));
		const workspace = workspaceId && !stillUsed ? this.workspaceAt(workspaceId) : null;
		if (workspace) {
			this.workspaceWindows = this.workspaceWindows.filter((item) => item.id !== workspaceId);
			if (workspace.path && isDesktop()) {
				try { await ipc.closeWorkspace(workspace.path); } catch (error) { console.warn('workspace close failed', error); }
			}
		}
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
		if (this.activeEnvironmentPanes.length >= 4 && !this.paneForWorkspace(path)) return false;
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

	/** Remove a workspace from Fella and clear every pane/conversation reference
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

		for (const environment of this.environments) {
			const panes = environment.panes.filter((pane) => pane.workspaceId !== path);
			for (const pane of panes) {
				if (!removedIds.has(pane.conversationId)) continue;
				let replacement = this.conversations.findLast((conversation) => conversation.workspaceScope === pane.workspaceId);
				if (!replacement) {
					replacement = new Conversation();
					replacement.model = this.model;
					replacement.workspaceScope = pane.workspaceId;
					this.conversations.push(replacement);
				}
				pane.conversationId = replacement.id;
			}
			if (!panes.length) {
				let general = this.conversations.findLast((conversation) => conversation.workspaceScope === null);
				if (!general) {
					general = new Conversation();
					general.model = this.model;
					general.workspaceScope = null;
					this.conversations.push(general);
				}
				const pane = { id: `${environment.id}:general`, workspaceId: null, conversationId: general.id };
				panes.push(pane);
			}
			environment.panes = panes;
			if (!panes.some((pane) => pane.id === environment.activePaneId)) environment.activePaneId = panes[0].id;
		}

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
		this.#writeWorkspaceState();
		const activePane = this.activePane;
		if (activePane) this.focusEnvironmentPane(activePane.id);
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

	createProject(name: string, workspace: string): Project {
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
			created_at_ms: now,
			updated_at_ms: now
		};
		this.projects = [project, ...this.projects];
		this.#writeProjects();
		this.activeProjectId = project.id;
		this.workspaceView = 'project';
		return project;
	}

	updateProject(id: string, patch: Partial<Pick<Project, 'name' | 'body'>>): void {
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
		if (view === 'workspace' && !this.catalog.workspace) {
			this.workspaceView = 'ask';
			return;
		}
		if (view === 'board' && !this.activeEnvironmentPanes.length) {
			this.workspaceView = 'ask';
			return;
		}
		this.workspaceView = view;
	}

	setWorkspacePane(pane: WorkspacePane): void {
		if (!this.catalog.workspace) {
			this.workspaceView = 'ask';
			return;
		}
		this.workspacePane = pane;
		this.workspaceView = 'workspace';
	}

	selectSource(path: string | null): void {
		this.selectedSourcePath = path;
	}

	/** Open a catalog-backed source without changing the conversation's data
	 * scope. The pane is a user interface choice, not analysis context. */
	openSourcePane(source: SourceInfo): void {
		const workspace = this.catalog.workspace;
		if (!workspace || !this.catalog.sources.some((item) => item.path === source.path)) return;
		this.activeChat.companionPane = {
			kind: 'source',
			path: source.path,
			name: source.name,
			workspacePath: workspace,
			revision: this.catalog.revision ?? null
		};
		this.workspaceView = 'ask';
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
		this.activeChat.companionPane = {
			kind: 'chart',
			messageId,
			evidenceIndex,
			...(evidence.id ? { evidenceId: evidence.id } : {}),
			workspacePath: message.answer?.workspace?.path ?? this.activeChat.workspaceScope ?? null,
			revision: message.answer?.workspace?.revision ?? null
		};
		this.workspaceView = 'ask';
		this.persist();
	}

	closeCompanionPane(): void {
		this.activeChat.companionPane = null;
		this.persist();
	}

	/** Select a conversation inside the current environment, preserving its owner. */
	activateConversation(index: number): boolean {
		const conversation = this.conversations[index];
		if (!conversation) return false;
		const scope = conversation.workspaceScope ?? null;
		const workspaceId = scope;
		if (workspaceId && !this.workspaceAt(workspaceId)) return false;
		const environment = this.activeEnvironment;
		if (!environment) return false;
		let pane = environment.panes.find((item) => item.workspaceId === workspaceId);
		if (!pane) {
			if (environment.panes.length >= 4) {
				this.addSystem('This environment already has four workspaces. Open another environment to continue.');
				return false;
			}
			pane = { id: `${environment.id}:pane:${uid()}`, workspaceId, conversationId: conversation.id };
			environment.panes = [...environment.panes, pane];
		}
		pane.conversationId = conversation.id;
		return this.focusEnvironmentPane(pane.id);
	}


	setAskMode(mode: AskMode): void {
		this.ensureChat().setMode(mode);
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

	// --- conversation creation inside the focused environment pane ----------
	newConversation(workspaceId: string | null = this.activeWorkspaceId): boolean {
		const workspace = workspaceId ? this.workspaceAt(workspaceId) : null;
		if (workspaceId && !workspace) return false;
		const scope = workspace?.path ?? null;
		const environment = this.activeEnvironment;
		if (!environment) return false;
		let pane = environment.panes.find((item) => item.workspaceId === scope);
		if (!pane) {
			if (environment.panes.length >= 4) return false;
			pane = { id: `${environment.id}:pane:${uid()}`, workspaceId: scope, conversationId: '' };
			environment.panes = [...environment.panes, pane];
		}
		const conversation = new Conversation();
		conversation.model = this.model;
		conversation.workspaceScope = scope;
		this.conversations.push(conversation);
		pane.conversationId = conversation.id;
		this.focusEnvironmentPane(pane.id);
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
		companionPane: unknown = null
	): void {
		const existing = this.conversations.findIndex((conversation) => conversation.id === id);
		if (existing >= 0) {
			const conversation = this.conversations[existing];
			if (conversation.workspaceScope === undefined) conversation.workspaceScope = workspace;
			conversation.companionPane = parseCompanionPane(companionPane) ?? conversation.companionPane;
			const pane = this.paneForWorkspace(workspace ?? null);
			if (pane) pane.conversationId = id;
			const owner = workspace ? this.workspaceAt(workspace) : null;
			if (owner) owner.conversationId = id;
			this.activeConversationIndex = existing;
			return;
		}
		const conversation = new Conversation(id);
		conversation.model = this.model;
		conversation.title = title;
		conversation.workspaceScope = workspace;
		conversation.companionPane = parseCompanionPane(companionPane);
		// A reloaded transcript never has a run in flight.
		conversation.messages = messages.map((m) => (m.pending ? { ...m, pending: false } : m));
		this.conversations.push(conversation);
		const pane = this.paneForWorkspace(workspace ?? null);
		if (pane) pane.conversationId = id;
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


	/** Remove a conversation from the live cache WITHOUT archiving it -- for
	 *  when its history entry was just deleted from the sidebar. Deleting only
	 *  the archived file would let a later settle re-create it from the live
	 *  transcript. No-op if the conversation isn't currently live. */
	removeConversationWithoutArchiving(id: string): void {
		const i = this.conversations.findIndex((conversation) => conversation.id === id);
		if (i < 0) return;
		const conversation = this.conversations[i];
		const ownerId = conversation.workspaceScope ?? null;
		const wasActive = i === this.activeConversationIndex;
		conversation.dropSnapshot();
		if (isDesktop()) void ipc.forgetConversation(conversation.id).catch(() => {});
		this.conversations.splice(i, 1);
		const owner = ownerId ? this.workspaceAt(ownerId) : null;
		for (const environment of this.environments) {
			for (const pane of environment.panes) {
				if (pane.conversationId !== id) continue;
				const replacementIndex = this.conversations.findLastIndex((item) => item.workspaceScope === pane.workspaceId);
				if (replacementIndex >= 0) pane.conversationId = this.conversations[replacementIndex].id;
				else {
					const replacement = new Conversation();
					replacement.workspaceScope = pane.workspaceId;
					this.conversations.push(replacement);
					pane.conversationId = replacement.id;
				}
			}
		}
		if (owner?.conversationId === id) {
			const replacement = this.conversations.findLast((item) => item.workspaceScope === owner.path);
			if (replacement) owner.conversationId = replacement.id;
		}
		if (this.conversations.length === 0) {
			const replacement = new Conversation();
			replacement.workspaceScope = ownerId;
			this.conversations.push(replacement);
			if (owner) owner.conversationId = replacement.id;
		}
		if (wasActive) {
			this.activateConversation(this.conversations.findIndex((item) => item.id === (owner?.conversationId ?? this.activePane?.conversationId)));
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
		if (this.activePane) this.activePane.conversationId = replacement.id;
		this.activateConversation(this.activeConversationIndex);
		this.#writeIndex();
	}

	/** Called once on launch. Archives every transcript a previous run left
	 *  behind (one blob per conversation, plus the legacy single blob) and starts
	 *  each active environment pane with an empty conversation; transcripts are
	 *  loaded from history on demand. */
	async rollOver(): Promise<void> {
		let keys: string[];
		try {
			keys = Object.keys(localStorage).filter((k) => k === LEGACY_KEY || k.startsWith(PREFIX));
		} catch {
			const replacement = new Conversation();
			replacement.workspaceScope = this.activePane?.workspaceId ?? null;
			this.conversations = [replacement];
			this.activeConversationIndex = 0;
			for (const environment of this.environments) for (const pane of environment.panes) pane.conversationId = '';
			if (this.activePane) this.activePane.conversationId = replacement.id;
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
		replacement.workspaceScope = this.activePane?.workspaceId ?? null;
		this.conversations = [replacement];
		this.activeConversationIndex = 0;
		for (const environment of this.environments) for (const pane of environment.panes) pane.conversationId = '';
		if (this.activePane) this.activePane.conversationId = replacement.id;
		if (this.activePane?.workspaceId) {
			const workspace = this.workspaceAt(this.activePane.workspaceId);
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
		let saved: { id?: string; workspace?: string | null; messages?: unknown; companionPane?: unknown };
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
