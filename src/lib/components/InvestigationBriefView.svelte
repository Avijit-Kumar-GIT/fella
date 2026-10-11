<script lang="ts">
	import { tick } from 'svelte';
	import { firstActualQuestion, session } from '$lib/session.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Badge } from '$lib/components/ui/badge';
	import { analyzeInvestigationQuestion, openConversation } from '$lib/commands';
	import { Textarea } from '$lib/components/ui/textarea';
	import { ipc, isDesktop } from '$lib/ipc';
	import { analysisTurnIdsAfterSourceMessage } from '$lib/investigation-runs';
	import { buildInvestigationReport, investigationReportFilename } from '$lib/investigation-report';
	import { runStateLabel } from '$lib/run-progress';
	import type {
		AnalysisTurn,
		Answer,
		ConversationSummary,
		Investigation,
		InvestigationBrief,
		InvestigationDefinitionInput,
		InvestigationFinding,
		InvestigationFindingRevision,
		InvestigationFindingUpdate,
		InvestigationQuestion,
		InvestigationQuestionInput,
		InvestigationQuestionAnswerStatus,
		InvestigationSourceSelection
	} from '$lib/types';
	import { findBriefProposal, type BriefProposal, type BriefProposalTarget } from '$lib/investigation-brief-proposal';
	import Icon from './Icon.svelte';
	import EvidenceBlock from './EvidenceBlock.svelte';
	import InvestigationSourceReadiness from './InvestigationSourceReadiness.svelte';
	import InvestigationQuestionSourceScope from './InvestigationQuestionSourceScope.svelte';
	import QuestionAnswerPreview from './QuestionAnswerPreview.svelte';
	import InvestigationResultsView from './InvestigationResultsView.svelte';

	let investigations = $state<Investigation[]>([]);
	let selectedId = $state<string | null>(null);
	let activeView = $state<'brief' | 'results'>('brief');
	let workspacePath = $state<string | null>(null);
	let loadedWorkspace = $state<string | null>(null);
	let loadedInvestigationsVersion = -1;
	let goal = $state('');
	let audience = $state('');
	let desiredOutput = $state('');
	let constraintsText = $state('');
	let questions = $state<InvestigationQuestionInput[]>([]);
	let definitions = $state<InvestigationDefinitionInput[]>([]);
	let savedBrief = $state('');
	let savedQuestions = $state('');
	let savedDefinitions = $state('');
	let loadedBriefKey = '';
	let loadedQuestionsKey = '';
	let loadedDefinitionsKey = '';
	let proposalMessageId = $state<string | null>(null);
	let proposalText = $state('');
	let proposalTarget = $state<BriefProposalTarget>('goal');
	let pendingAnalysisQuestionId = $state<string | null>(null);
	let loading = $state(false);
	let creating = $state(false);
	let openingConversation = $state(false);
	let saving = $state(false);
	let proposingQuestions = $state(false);
	let startingQuestionAnalysis = $state(false);
	let analysisTurns = $state<Record<string, AnalysisTurn>>({});
	let openAnalysisTurnId = $state<string | null>(null);
	let analysisTurnLoadingId = $state<string | null>(null);
	let analysisTurnError = $state('');
	let questionLinksDraft = $state<string[]>([]);
	let savingQuestionLinks = $state(false);
	let findingClaimDraft = $state('');
	let findingCaveatsDraft = $state('');
	let findingAssumptionsDraft = $state('');
	let findingRevisionTargetDraft = $state('');
	let findingRevisionNoteDraft = $state('');
	let findingQuestionDraft = $state('');
	let findingEvidenceDraft = $state<string[]>([]);
	let savingFinding = $state(false);
	let findingError = $state('');
	let editingFindingId = $state<string | null>(null);
	let findingClaimEditDraft = $state('');
	let findingCaveatsEditDraft = $state('');
	let findingAssumptionsEditDraft = $state('');
	let findingRevisionNoteEditDraft = $state('');
	let updatingFindingId = $state<string | null>(null);
	let findingComparisonSelection = $state<{ findingId: string; revision: number } | null>(null);
	let findingHistoryLoadingId = $state<string | null>(null);
	let findingHistoryError = $state('');
	let exportingReport = $state(false);
	let reportExportStatus = $state('');
	let reportExportFailed = $state(false);
	let error = $state('');
	let requestSerial = 0;
	const syncingInvestigations = new Set<string>();

	let selected = $derived(investigations.find((item) => item.id === selectedId) ?? null);
	let openAnalysisTurn = $derived(openAnalysisTurnId ? analysisTurns[openAnalysisTurnId] ?? null : null);
	let questionLinksDirty = $derived.by(() => {
		if (!openAnalysisTurnId) return false;
		const saved = questionIdsForRun(openAnalysisTurnId, selected).sort();
		return JSON.stringify([...questionLinksDraft].sort()) !== JSON.stringify(saved);
	});
	let rootQuestion = $derived(questions.find((question) => !question.parent_id) ?? null);
	let pendingAnalysisQuestion = $derived(
		questions.find((question) => question.id === pendingAnalysisQuestionId) ?? null
	);
	let analysisSetupCatalogAvailable = $derived(
		Boolean(selected && session.catalog.workspace === selected.workspace_id)
	);
	let analysisSetupIsNarrowed = $derived(
		Boolean(pendingAnalysisQuestion?.source_selection || selected?.source_selection.mode === 'selected_sources')
	);
	let analysisSetupSourcePaths = $derived.by(() => {
		const item = selected;
		const question = pendingAnalysisQuestion;
		if (!item || !question || !analysisSetupCatalogAvailable) return [];
		const paths = [...new Set(session.catalog.sources.map((source) => relativeSourcePath(source.path, item.workspace_id)))];
		const investigationPaths = item.source_selection.mode === 'all_sources'
			? paths
			: paths.filter((path) => item.source_selection.included_paths.includes(path));
		if (!question.source_selection) return investigationPaths;
		const allowed = new Set(investigationPaths);
		return question.source_selection.included_paths.filter((path) => allowed.has(path));
	});
	let analysisSetupExcludedPaths = $derived.by(() => {
		const question = pendingAnalysisQuestion;
		if (!question?.source_selection) return [];
		const effective = new Set(analysisSetupSourcePaths);
		return question.source_selection.included_paths.filter((path) => !effective.has(path));
	});
	let analysisSetupScopeLabel = $derived.by(() => {
		const item = selected;
		const question = pendingAnalysisQuestion;
		if (!item || !question) return '';
		if (!analysisSetupCatalogAvailable) return 'Current workspace catalog is not loaded';
		if (question.source_selection) {
			const count = analysisSetupSourcePaths.length;
			return analysisSetupExcludedPaths.length
				? `Question-specific scope · ${count} effective of ${question.source_selection.included_paths.length} mapped files`
				: `Question-specific scope · ${count} file${count === 1 ? '' : 's'}`;
		}
		if (item.source_selection.mode === 'all_sources') {
			return `Inherit Investigation · all ${analysisSetupSourcePaths.length} available file${analysisSetupSourcePaths.length === 1 ? '' : 's'}`;
		}
		return `Inherit Investigation · ${analysisSetupSourcePaths.length} selected file${analysisSetupSourcePaths.length === 1 ? '' : 's'}`;
	});
	let subquestions = $derived(rootQuestion
		? questions.filter((question) => question.parent_id === rootQuestion.id)
		: []);
	let currentConversationSource = $derived.by(() => {
		const conversation = session.activeChat;
		if (!workspacePath || conversation.workspaceScope !== workspacePath) return undefined;
		return firstActualQuestion(conversation.messages);
	});
	let currentConversationInvestigation = $derived(
		currentConversationSource
			? investigations.find((item) => item.workspace_id === workspacePath && (item.conversation_id === session.activeChat.id || item.brief_source_message_id === currentConversationSource?.id)) ?? null
			: null
	);
	let unlinkedAnalysisTurnIds = $derived.by(() => {
		const item = selected;
		const conversation = session.activeChat;
		if (!item || conversation.workspaceScope !== item.workspace_id) return [];
		const linked = new Set(item.analysis_turn_ids ?? []);
		return analysisTurnIdsAfterSourceMessage(
			conversation.messages,
			item.brief_source_message_id,
			item.workspace_id
		).filter((turnId) => !linked.has(turnId));
	});
	let pendingProposal: BriefProposal | null = $derived.by(() => {
		const item = selected;
		const conversation = session.activeChat;
		if (!item?.brief_source_message_id || conversation.workspaceScope !== item.workspace_id) return null;
		return findBriefProposal(
			conversation.messages,
			item.brief_source_message_id,
			item.brief_reviewed_message_id
		);
	});
	let currentBrief: InvestigationBrief = $derived({
		goal,
		audience,
		desired_output: desiredOutput,
		constraints: constraintsText.split('\n').map((item) => item.trim()).filter(Boolean)
	});
	let briefKey = $derived(JSON.stringify(currentBrief));
	let dirty = $derived(Boolean(selected && briefKey !== savedBrief));
	let questionsKey = $derived(JSON.stringify(questions));
	let questionsDirty = $derived(Boolean(selected && questionsKey !== savedQuestions));
	let definitionsKey = $derived(JSON.stringify(definitions));
	let definitionsDirty = $derived(Boolean(selected && definitionsKey !== savedDefinitions));
	let coverage = $derived.by(() => {
		const activeQuestions = (selected?.questions ?? []).filter((question) => question.status === 'active');
		const answered = activeQuestions.filter((question) => question.answer_status === 'answered').length;
		const partial = activeQuestions.filter((question) => question.answer_status === 'incomplete').length;
		const unsupported = activeQuestions.filter((question) => question.answer_status === 'unsupported').length;
		const unanswered = activeQuestions.length - answered - partial - unsupported;
		const documentedGaps = activeQuestions.filter((question) =>
			(question.answer_status === 'incomplete' || question.answer_status === 'unsupported')
			&& Boolean(question.answer_status_reason?.trim())
		);
		return {
			activeQuestions,
			answered,
			partial,
			unsupported,
			unanswered,
			documentedGaps,
			needsReview: activeQuestions.filter((question) => question.answer_status !== 'answered'),
			canComplete: activeQuestions.length > 0
				&& answered + documentedGaps.length === activeQuestions.length
				&& !dirty
				&& !questionsDirty
				&& !definitionsDirty
				&& !saving
		};
	});
	let hasBlankQuestion = $derived(questions.some((question) => !question.text.trim()));
	let hasBlankDefinition = $derived(definitions.some((definition) => !definition.text.trim()));
	let canSuggestBreakdown = $derived(Boolean(rootQuestion?.text.trim()) && subquestions.length === 0);
	let reportExportDisabledReason = $derived.by(() => {
		if (!selected) return 'Select an Investigation before exporting.';
		if (dirty || questionsDirty || definitionsDirty || saving) return 'Save the brief, Question plan, and definitions before exporting.';
		if (selected.questions?.some((question) => question.status === 'active' && question.answer_status === 'running')) {
			return 'Wait for active analysis runs to finish before exporting.';
		}
		return '';
	});

	async function loadWorkspace(path: string): Promise<void> {
		const serial = ++requestSerial;
		loading = true;
		error = '';
		investigations = [];
		selectedId = null;
		analysisTurns = {};
		openAnalysisTurnId = null;
		questionLinksDraft = [];
		try {
			const all = await ipc.investigationsList();
			if (serial !== requestSerial) return;
			investigations = all.filter((item) => item.workspace_id === path);
			const activeConversation = session.activeChat;
			const activeQuestion = firstActualQuestion(activeConversation.messages);
			const linkedToConversation = investigations.find((item) =>
				item.conversation_id === activeConversation.id ||
				(!!activeQuestion && item.brief_source_message_id === activeQuestion.id)
			);
			selectedId = linkedToConversation?.id ?? investigations[0]?.id ?? null;
		} catch (cause) {
			if (serial === requestSerial) error = cause instanceof Error ? cause.message : String(cause);
		} finally {
			if (serial === requestSerial) loading = false;
		}
	}

	async function syncAnalysisTurns(investigationId: string): Promise<void> {
		if (syncingInvestigations.has(investigationId)) return;
		syncingInvestigations.add(investigationId);
		try {
			while (true) {
				const item = investigations.find((record) => record.id === investigationId);
				const conversation = session.activeChat;
				if (!item || conversation.workspaceScope !== item.workspace_id) return;
				const linked = new Set(item.analysis_turn_ids ?? []);
				const missing = analysisTurnIdsAfterSourceMessage(
					conversation.messages,
					item.brief_source_message_id,
					item.workspace_id
				).filter((turnId) => !linked.has(turnId));
				if (!missing.length) return;
				const updated = await ipc.investigationLinkAnalysisTurns(item.id, missing);
				const confirmed = new Set(updated.analysis_turn_ids ?? []);
				if (missing.some((turnId) => !confirmed.has(turnId))) {
					error = 'The saved analysis-run links could not be confirmed. Reopen this Investigation to try again.';
					return;
				}
				investigations = investigations.map((record) => record.id === updated.id ? updated : record);
			}
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
		} finally {
			syncingInvestigations.delete(investigationId);
		}
	}

	$effect(() => {
		const path = session.catalog.workspace;
		const version = session.investigationsVersion;
		workspacePath = path;
		if (!path) {
			requestSerial += 1;
			loadedWorkspace = null;
			loadedInvestigationsVersion = version;
			investigations = [];
			selectedId = null;
			loading = false;
			return;
		}
		if (path === loadedWorkspace && version === loadedInvestigationsVersion) return;
		loadedWorkspace = path;
		loadedInvestigationsVersion = version;
		void loadWorkspace(path);
	});

	$effect(() => {
		const item = selected;
		const missing = unlinkedAnalysisTurnIds;
		if (item && missing.length && isDesktop()) void syncAnalysisTurns(item.id);
	});

	$effect(() => {
		const item = selected;
		const key = item ? `${item.id}:${JSON.stringify(item.brief)}` : '';
		if (key === loadedBriefKey) return;
		loadedBriefKey = key;
		if (!item) {
			goal = '';
			audience = '';
			desiredOutput = '';
			constraintsText = '';
			savedBrief = '';
			return;
		}
		goal = item.brief.goal;
		audience = item.brief.audience;
		desiredOutput = item.brief.desired_output;
		constraintsText = item.brief.constraints.join('\n');
		savedBrief = JSON.stringify(item.brief);
	});

	$effect(() => {
		const item = selected;
		const key = item ? `${item.id}:${JSON.stringify(item.questions ?? null)}` : '';
		if (key === loadedQuestionsKey) return;
		loadedQuestionsKey = key;
		if (!item) {
			questions = [];
			savedQuestions = '';
			return;
		}
		questions = (item.questions ?? (item.brief.goal.trim()
			? [{ id: `legacy-${item.id}`, text: item.brief.goal, parent_id: undefined, source_selection: undefined }]
			: [])).map((question) => ({
			id: question.id,
			text: question.text,
			parent_id: question.parent_id,
			source_selection: question.source_selection
		}));
		savedQuestions = JSON.stringify(questions);
	});

	$effect(() => {
		const item = selected;
		const key = item ? `${item.id}:${JSON.stringify(item.definitions ?? null)}` : '';
		if (key === loadedDefinitionsKey) return;
		loadedDefinitionsKey = key;
		definitions = (item?.definitions ?? []).map((definition) => ({
			id: definition.id,
			kind: definition.kind,
			text: definition.text
		}));
		savedDefinitions = JSON.stringify(definitions);
	});

	$effect(() => {
		const proposal = pendingProposal;
		if ((proposal?.messageId ?? null) === proposalMessageId) return;
		proposalMessageId = proposal?.messageId ?? null;
		proposalText = proposal?.text ?? '';
		proposalTarget = proposal?.target ?? 'goal';
	});

	async function createInvestigation(): Promise<void> {
		if (!workspacePath || creating || !isDesktop()) return;
		if (questionsDirty) {
			error = 'Save or discard the current Question plan before opening another Investigation.';
			return;
		}
		if (definitionsDirty) {
			error = 'Save or discard the current definitions before opening another Investigation.';
			return;
		}
		creating = true;
		error = '';
		const request = currentConversationSource;
		try {
			const record = request
				? await ipc.investigationEnsure(workspacePath, session.activeChat.id, request.text, request.id)
			: await ipc.investigationCreate(
					workspacePath,
					'New investigation',
					undefined,
					undefined,
					session.activeChat.workspaceScope === workspacePath ? session.activeChat.id : undefined
				);
			investigations = [record, ...investigations.filter((item) => item.id !== record.id)];
			selectedId = record.id;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
		} finally {
			creating = false;
		}
	}

	async function openSelectedConversation(): Promise<boolean> {
		const item = selected;
		if (!item || openingConversation || !isDesktop()) return false;
		if (questionsDirty || definitionsDirty) {
			error = 'Save or discard the current Question plan and definitions before opening the conversation.';
			return false;
		}
		openingConversation = true;
		error = '';
		try {
			let conversationId = item.conversation_id;
			let summary: ConversationSummary | null = null;
			if (!conversationId && item.brief_source_message_id) {
				const summaries = await ipc.conversationsList();
				summary = summaries.find((candidate) =>
					candidate.workspace === item.workspace_id &&
					candidate.first_question_message_id === item.brief_source_message_id
				) ?? null;
				conversationId = summary?.id;
				if (conversationId) {
					const linked = await ipc.investigationEnsure(
						item.workspace_id,
						conversationId,
						item.brief.goal,
						item.brief_source_message_id
					);
					investigations = investigations.map((record) => record.id === linked.id ? linked : record);
				}
			}
			if (!conversationId) throw new Error('The source conversation is not available in local history.');

			const loadedIndex = session.conversations.findIndex((conversation) =>
				conversation.id === conversationId && conversation.workspaceScope === item.workspace_id
			);
			if (loadedIndex >= 0) {
				if (!session.activateConversation(loadedIndex)) throw new Error('Fella could not focus the linked conversation.');
			} else {
				summary ??= (await ipc.conversationsList()).find((candidate) =>
					candidate.id === conversationId && candidate.workspace === item.workspace_id
				) ?? null;
				if (!summary) throw new Error('The linked conversation is not available in local history.');
				if (!(await openConversation(summary))) throw new Error('Fella could not open the linked conversation.');
			}
			session.setWorkspaceView('ask');
			return true;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
			return false;
		} finally {
			openingConversation = false;
		}
	}

	async function saveBrief(): Promise<boolean> {
		if (!selected || saving || !dirty) return !dirty;
		saving = true;
		error = '';
		try {
			const updated = await ipc.investigationUpdateBrief(selected.id, currentBrief);
			investigations = investigations.map((item) => item.id === updated.id ? updated : item);
			savedBrief = JSON.stringify(updated.brief);
			return true;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
			return false;
		} finally {
			saving = false;
		}
	}

	async function markInvestigationComplete(): Promise<void> {
		const item = selected;
		if (!item || saving || !coverage.canComplete) return;
		saving = true;
		error = '';
		try {
			const updated = await ipc.investigationMarkComplete(item.id);
			acceptInvestigationUpdate(updated);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
		} finally {
			saving = false;
		}
	}

	async function reopenInvestigation(): Promise<void> {
		const item = selected;
		if (!item || saving || item.status !== 'complete') return;
		saving = true;
		error = '';
		try {
			const updated = await ipc.investigationReopen(item.id);
			acceptInvestigationUpdate(updated);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
		} finally {
			saving = false;
		}
	}

	function acceptInvestigationUpdate(updated: Investigation): void {
		investigations = investigations.map((item) => item.id === updated.id ? updated : item);
	}

	function relativeSourcePath(path: string, rootPath: string): string {
		const root = rootPath.replace(/[/\\]+$/, '');
		if (path === root) return path;
		if (path.startsWith(`${root}/`) || path.startsWith(`${root}\\`)) {
			return path.slice(root.length + 1).replace(/\\/g, '/');
		}
		return path.replace(/\\/g, '/');
	}

	function addQuestion(): void {
		const id = globalThis.crypto?.randomUUID?.().replaceAll('-', '') ?? `${Date.now()}${Math.random().toString(16).slice(2)}`;
		const question: InvestigationQuestionInput = {
			id: `q_${id}`,
			text: '',
			parent_id: rootQuestion?.id
		};
		questions = [...questions, question];
	}

	function updateQuestionSourceSelection(
		questionId: string,
		sourceSelection: InvestigationQuestionInput['source_selection']
	): void {
		questions = questions.map((question) => question.id === questionId
			? { ...question, source_selection: sourceSelection }
			: question);
	}

	function questionAnswerStatus(questionId: string): InvestigationQuestionAnswerStatus {
		return selected?.questions?.find((question) => question.id === questionId)?.answer_status ?? 'not_started';
	}

	function questionAnswerStatusLabel(status: InvestigationQuestionAnswerStatus): string {
		return {
			not_started: 'Not started',
			running: 'Running',
			answered: 'Complete answer',
			incomplete: 'Partial answer',
			unsupported: 'Unsupported',
			blocked: 'Blocked',
			outdated: 'Outdated'
		}[status];
	}

	function savedQuestion(questionId: string): InvestigationQuestion | undefined {
		return selected?.questions?.find((question) => question.id === questionId);
	}

	function questionAnswerStatusVariant(status: InvestigationQuestionAnswerStatus): 'default' | 'success' | 'warning' | 'destructive' {
		if (status === 'answered') return 'success';
		if (status === 'blocked') return 'destructive';
		if (status === 'incomplete' || status === 'unsupported' || status === 'outdated') return 'warning';
		return 'default';
	}

	function questionCoverageReason(question: InvestigationQuestion): string {
		if (question.answer_status_reason) return question.answer_status_reason;
		return {
			not_started: 'No analysis run is linked to this Question yet.',
			running: 'Analysis is still running.',
			answered: 'This Question has a complete answer.',
			incomplete: 'The latest run returned a partial answer.',
			unsupported: 'The available data or analysis tools do not support this Question.',
			blocked: 'The latest run could not produce a complete answer.',
			outdated: 'The Question or its analysis context changed after the latest run.'
		}[question.answer_status];
	}

	function focusQuestion(questionId: string): void {
		const field = document.getElementById(`question-${questionId}`);
		field?.scrollIntoView({ behavior: 'smooth', block: 'center' });
		field?.focus({ preventScroll: true });
	}

	function questionAnswerReason(questionId: string): string | undefined {
		return selected?.questions?.find((question) => question.id === questionId)?.answer_status_reason;
	}

	function questionRunIdsFor(question: InvestigationQuestion): string[] {
		return [...new Set(question.analysis_turn_ids ?? [])];
	}

	function questionHistoricalRunIds(questionId: string): string[] {
		const question = selected?.questions?.find((item) => item.id === questionId);
		return question
			? questionRunIdsFor(question).filter((turnId) => turnId !== question.latest_analysis_turn_id)
			: [];
	}

	function analysisRunIds(investigation: Investigation | null = selected): string[] {
		if (!investigation) return [];
		const ids = new Set(investigation.analysis_turn_ids ?? []);
		for (const question of investigation.questions ?? []) {
			for (const turnId of questionRunIdsFor(question)) ids.add(turnId);
		}
		return [...ids];
	}

	async function exportInvestigationReport(): Promise<void> {
		const item = selected;
		if (!item || exportingReport || reportExportDisabledReason) return;
		exportingReport = true;
		reportExportStatus = '';
		reportExportFailed = false;
		try {
			const runIds = new Set<string>();
			for (const question of item.questions ?? []) {
				if (question.status === 'active' && question.latest_analysis_turn_id) runIds.add(question.latest_analysis_turn_id);
			}
			for (const finding of item.findings ?? []) runIds.add(finding.analysis_turn_id);
			const loadedRuns = await Promise.all([...runIds].map(async (turnId) => {
				try {
					return [turnId, await ipc.analysisTurnLoad(turnId)] as const;
				} catch {
					return [turnId, null] as const;
				}
			}));
			const runs = Object.fromEntries(loadedRuns);
			const report = buildInvestigationReport(item, runs, new Date());
			const filename = investigationReportFilename(item.title || item.brief.goal);
			const url = URL.createObjectURL(new Blob([report], { type: 'text/markdown;charset=utf-8' }));
			const link = document.createElement('a');
			link.href = url;
			link.download = filename;
			document.body.append(link);
			link.click();
			link.remove();
			setTimeout(() => URL.revokeObjectURL(url), 1000);
			reportExportStatus = `Downloaded ${filename}.`;
		} catch (cause) {
			reportExportFailed = true;
			reportExportStatus = cause instanceof Error ? cause.message : String(cause);
		} finally {
			exportingReport = false;
		}
	}

	function analysisRunOrdinal(turnId: string): number | undefined {
		const index = analysisRunIds().indexOf(turnId);
		return index < 0 ? undefined : index + 1;
	}

	function analysisRunStatusLine(turnId: string): string {
		const run = analysisTurns[turnId];
		if (!run) return 'Open saved record';
		const state = runStateLabel(run.state) ?? 'Unknown state';
		const parentId = run.rerun_of;
		if (typeof parentId !== 'string' || !parentId) return state;
		const parentOrdinal = analysisRunOrdinal(parentId);
		return `${state} · rerun of ${parentOrdinal ? `Run ${parentOrdinal}` : 'saved run'}`;
	}

	function questionIdsForRun(
		turnId: string,
		investigation: Investigation | null = selected
	): string[] {
		return (investigation?.questions ?? [])
			.filter((question) => questionRunIdsFor(question).includes(turnId))
			.map((question) => question.id);
	}

	function linkedQuestionsForRun(turnId: string): InvestigationQuestion[] {
		const ids = new Set(questionIdsForRun(turnId));
		return (selected?.questions ?? []).filter((question) => ids.has(question.id));
	}

	function questionHeading(questionId: string): string {
		const investigation = selected;
		const question = investigation?.questions?.find((item) => item.id === questionId);
		if (!question) return 'Question';
		if (!question.parent_id) return 'Main Question';
		const siblings = (investigation?.questions ?? []).filter((item) => item.parent_id === question.parent_id);
		const index = siblings.findIndex((item) => item.id === questionId);
		return `Subquestion ${index + 1}`;
	}

	function analysisRunSummary(turnId: string): string {
		const run = analysisTurns[turnId];
		const savedQuestion = run?.investigation_context?.question_text?.trim();
		if (savedQuestion) return savedQuestion;
		const linked = linkedQuestionsForRun(turnId)
			.map((question) => question.text.trim())
			.filter(Boolean);
		if (linked.length) return linked.join(' · ');
		return run?.question ?? 'Conversation analysis';
	}

	function answerForAnalysisRun(run: AnalysisTurn): Answer {
		return {
			text: run.result.text,
			evidence: run.result.evidence as Answer['evidence'],
			verification: run.verification?.checks ?? run.result.verification,
			trace: run.trace,
			plan: run.plan,
			contract: run.contract,
			provenance: run.provenance,
			status: run.result.status,
			workspace: run.workspace && run.workspace_revision
				? { path: run.workspace, revision: run.workspace_revision }
				: undefined,
			usage: run.result.usage
		};
	}

	function sourceSelectionSummary(selection: InvestigationSourceSelection | undefined): string {
		if (!selection) return 'No source-scope snapshot was recorded for this run.';
		if (selection.mode === 'all_sources') return 'All sources in the Question’s Investigation scope';
		return `${selection.included_paths.length} selected source${selection.included_paths.length === 1 ? '' : 's'}`;
	}

	async function openAnalysisRun(turnId: string): Promise<void> {
		openAnalysisTurnId = turnId;
		analysisTurnError = '';
		questionLinksDraft = questionIdsForRun(turnId);
		if (!analysisTurns[turnId]) {
			analysisTurnLoadingId = turnId;
			try {
				const run = await ipc.analysisTurnLoad(turnId);
				analysisTurns = { ...analysisTurns, [turnId]: run };
				prepareFindingDraft(run);
			} catch (cause) {
				analysisTurnError = cause instanceof Error ? cause.message : String(cause);
			} finally {
				if (analysisTurnLoadingId === turnId) analysisTurnLoadingId = null;
			}
		} else {
			prepareFindingDraft(analysisTurns[turnId]);
		}
		await tick();
		const detail = document.getElementById('saved-analysis-run-detail');
		detail?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
		detail?.focus({ preventScroll: true });
	}

	async function openRunFromResults(turnId: string): Promise<void> {
		activeView = 'brief';
		await tick();
		await openAnalysisRun(turnId);
	}

	async function reviewQuestionFromResults(questionId: string): Promise<void> {
		activeView = 'brief';
		await tick();
		focusQuestion(questionId);
	}

	function findingEvidenceOptions(run: AnalysisTurn): { id: string; label: string }[] {
		const seen = new Set<string>();
		return run.result.evidence.flatMap((item, index) => {
			if (!item || typeof item !== 'object' || Array.isArray(item)) return [];
			const evidence = item as Record<string, unknown>;
			const id = typeof evidence.id === 'string' ? evidence.id.trim() : '';
			if (!id || seen.has(id)) return [];
			seen.add(id);
			const tool = typeof evidence.tool === 'string' ? evidence.tool : `Evidence ${index + 1}`;
			const summary = typeof evidence.result_summary === 'string' ? evidence.result_summary : '';
			return [{ id, label: summary ? `${tool} · ${summary}` : tool }];
		});
	}

	function prepareFindingDraft(run: AnalysisTurn): void {
		const lead = run.result.text.split(/\n\s*\n/).find((part) => part.trim())?.trim() ?? '';
		findingClaimDraft = lead.slice(0, 500);
		findingCaveatsDraft = '';
		findingAssumptionsDraft = '';
		findingRevisionTargetDraft = '';
		findingRevisionNoteDraft = '';
		const activeQuestions = selected?.questions ?? [];
		const preferredQuestion = questionIdsForRun(run.id)[0]
			?? run.investigation_context?.question_id
			?? activeQuestions[0]?.id;
		findingQuestionDraft = activeQuestions.some((question) => question.id === preferredQuestion)
			? preferredQuestion
			: activeQuestions[0]?.id ?? '';
		findingEvidenceDraft = findingEvidenceOptions(run).slice(0, 100).map((item) => item.id);
		findingError = '';
	}

	function findingNotesFromDraft(draft: string): string[] {
		return draft.split(/\r?\n/).map((note) => note.trim()).filter(Boolean);
	}

	function selectFindingRevisionTarget(findingId: string): void {
		findingRevisionTargetDraft = findingId;
		findingRevisionNoteDraft = '';
		const finding = selected?.findings?.find((item) => item.id === findingId);
		if (!finding || !openAnalysisTurn) return;
		findingQuestionDraft = finding.question_id;
		findingClaimDraft = finding.claim;
		findingCaveatsDraft = (finding.caveats ?? []).join('\n');
		findingAssumptionsDraft = (finding.assumptions ?? []).join('\n');
		const currentEvidence = new Set(findingEvidenceOptions(openAnalysisTurn).map((item) => item.id));
		findingEvidenceDraft = finding.evidence_ids.filter((id) => currentEvidence.has(id));
	}

	function toggleFindingEvidence(evidenceId: string, checked: boolean): void {
		const evidence = new Set(findingEvidenceDraft);
		if (checked) evidence.add(evidenceId);
		else evidence.delete(evidenceId);
		findingEvidenceDraft = [...evidence];
	}

	async function saveFinding(): Promise<void> {
		if (!selected || !openAnalysisTurn || savingFinding) return;
		if (!findingQuestionDraft || !findingEvidenceDraft.length || !findingClaimDraft.trim()) {
			findingError = 'Add a concise claim, choose a Question, and select supporting evidence.';
			return;
		}
		savingFinding = true;
		findingError = '';
		try {
			const caveats = findingNotesFromDraft(findingCaveatsDraft);
			const assumptions = findingNotesFromDraft(findingAssumptionsDraft);
			const target = selected.findings?.find((item) => item.id === findingRevisionTargetDraft);
			if (findingRevisionTargetDraft && !target) {
				throw new Error('The Finding selected for revision is no longer available.');
			}
			const updated = target
				? await ipc.investigationUpdateFinding(selected.id, target.id, {
						question_id: findingQuestionDraft,
						analysis_turn_id: openAnalysisTurn.id,
						evidence_ids: findingEvidenceDraft,
						claim: findingClaimDraft,
						caveats,
						assumptions,
						revision_note: findingRevisionNoteDraft
					})
				: await ipc.investigationCreateFinding(selected.id, {
						question_id: findingQuestionDraft,
						analysis_turn_id: openAnalysisTurn.id,
						evidence_ids: findingEvidenceDraft,
						claim: findingClaimDraft,
						caveats,
						assumptions
					});
			acceptInvestigationUpdate(updated);
			findingRevisionTargetDraft = '';
			findingRevisionNoteDraft = '';
			findingComparisonSelection = null;
			findingError = '';
		} catch (cause) {
			findingError = cause instanceof Error ? cause.message : String(cause);
		} finally {
			savingFinding = false;
		}
	}

	function findingQuestionLabel(finding: InvestigationFinding): string {
		const question = selected?.questions?.find((item) => item.id === finding.question_id);
		if (question) return question.text;
		return analysisTurns[finding.analysis_turn_id]?.investigation_context?.question_text
			?? `Question ${finding.question_id}`;
	}

	function editFinding(finding: InvestigationFinding): void {
		editingFindingId = finding.id;
		findingClaimEditDraft = finding.claim;
		findingCaveatsEditDraft = (finding.caveats ?? []).join('\n');
		findingAssumptionsEditDraft = (finding.assumptions ?? []).join('\n');
		findingRevisionNoteEditDraft = '';
	}

	async function saveFindingEdit(finding: InvestigationFinding): Promise<void> {
		if (!selected || updatingFindingId) return;
		updatingFindingId = finding.id;
		findingError = '';
		try {
			const update: InvestigationFindingUpdate = {
				question_id: finding.question_id,
				analysis_turn_id: finding.analysis_turn_id,
				evidence_ids: finding.evidence_ids,
				claim: findingClaimEditDraft,
				caveats: findingNotesFromDraft(findingCaveatsEditDraft),
				assumptions: findingNotesFromDraft(findingAssumptionsEditDraft),
				revision_note: findingRevisionNoteEditDraft
			};
			const updated = await ipc.investigationUpdateFinding(
				selected.id,
				finding.id,
				update
			);
			acceptInvestigationUpdate(updated);
			editingFindingId = null;
			findingComparisonSelection = null;
		} catch (cause) {
			findingError = cause instanceof Error ? cause.message : String(cause);
		} finally {
			updatingFindingId = null;
		}
	}

	function findingRunMethod(run: AnalysisTurn | undefined): string {
		if (!run) return 'Method not recorded';
		const strategy = run.plan.strategy.replaceAll('_', ' ');
		return run.plan.steps.length ? `${strategy} · ${run.plan.steps.join(' → ')}` : strategy;
	}

	function findingRunScope(run: AnalysisTurn | undefined): string {
		if (!run) return 'Source scope not recorded';
		const selection = run.investigation_context?.source_selection;
		const selectionText = !selection || selection.mode === 'all_sources'
			? 'All available sources'
			: `Selected: ${selection.included_paths.join(', ') || 'none'}`;
		const used = run.source_scope?.used_sources.map((source) =>
			`${source.source}${source.table ? ` · ${source.table}` : ''}`
		) ?? [];
		return `${selectionText} · Used: ${used.length ? used.join(', ') : run.source_scope ? 'none' : 'not recorded'}`;
	}

	function findingRunDefinitions(run: AnalysisTurn | undefined): string {
		if (!run?.investigation_context) return 'Definitions not recorded';
		const definitions = run.investigation_context.definitions ?? [];
		if (!definitions.length) return 'None recorded';
		return definitions.map((definition) =>
			`${definition.kind} rev ${definition.revision}: ${definition.text}`
		).join(' · ');
	}

	function findingRevisionChanges(
		previous: InvestigationFindingRevision,
		current: InvestigationFinding
	): string[] {
		const changes: string[] = [];
		if (previous.claim !== current.claim) changes.push('Claim changed.');
		if (previous.question_id !== current.question_id) changes.push('Question changed.');
		if (JSON.stringify(previous.evidence_ids) !== JSON.stringify(current.evidence_ids)) {
			changes.push('Supporting evidence changed.');
		}
		if (previous.analysis_turn_id !== current.analysis_turn_id) changes.push('Supporting run changed.');
		if (JSON.stringify(previous.caveats) !== JSON.stringify(current.caveats)
			|| JSON.stringify(previous.assumptions) !== JSON.stringify(current.assumptions)) {
			changes.push('Caveats or assumptions changed.');
		}
		if (previous.revision_note !== current.revision_note) changes.push('Revision note changed.');
		const previousRun = analysisTurns[previous.analysis_turn_id];
		const currentRun = analysisTurns[current.analysis_turn_id];
		if (previousRun && currentRun) {
			if (findingRunMethod(previousRun) !== findingRunMethod(currentRun)) changes.push('Analysis method changed.');
			if (findingRunScopeSignature(previousRun) !== findingRunScopeSignature(currentRun)) changes.push('Source scope changed.');
			if (findingRunDefinitionSignature(previousRun) !== findingRunDefinitionSignature(currentRun)) changes.push('Definitions or assumptions changed.');
		} else {
			changes.push('Run context comparison is incomplete because a saved run could not be loaded.');
		}
		return changes.length ? changes : ['No recorded claim, Question, evidence, method, source-scope, or definition changes.'];
	}

	function findingRunScopeSignature(run: AnalysisTurn): string {
		const selection = run.investigation_context?.source_selection;
		const used = run.source_scope?.used_sources.map((source) => `${source.source}\t${source.table ?? ''}`).sort() ?? null;
		const skipped = run.source_scope?.skipped_sources.slice().sort() ?? null;
		return JSON.stringify({
			selection: selection ? { mode: selection.mode, paths: [...selection.included_paths].sort() } : null,
			used,
			skipped
		});
	}

	function findingRunDefinitionSignature(run: AnalysisTurn): string {
		const context = run.investigation_context;
		return JSON.stringify(context
			? context.definitions.map((definition) => ({
				id: definition.id,
				revision: definition.revision,
				kind: definition.kind,
				text: definition.text
			})).sort((left, right) => left.id.localeCompare(right.id))
			: null);
	}

	async function compareFindingRevision(
		finding: InvestigationFinding,
		revision: InvestigationFindingRevision
	): Promise<void> {
		findingComparisonSelection = { findingId: finding.id, revision: revision.revision };
		findingHistoryError = '';
		findingHistoryLoadingId = finding.id;
		try {
			for (const turnId of new Set([revision.analysis_turn_id, finding.analysis_turn_id])) {
				if (analysisTurns[turnId]) continue;
				const run = await ipc.analysisTurnLoad(turnId);
				analysisTurns = { ...analysisTurns, [turnId]: run };
			}
		} catch (cause) {
			findingHistoryError = cause instanceof Error ? cause.message : String(cause);
		} finally {
			findingHistoryLoadingId = null;
		}
	}

	function setQuestionRunLink(questionId: string, checked: boolean): void {
		const links = new Set(questionLinksDraft);
		if (checked) links.add(questionId);
		else links.delete(questionId);
		questionLinksDraft = [...links];
	}

	async function saveAnalysisRunQuestions(): Promise<void> {
		if (!selected || !openAnalysisTurnId || savingQuestionLinks || !questionLinksDirty) return;
		savingQuestionLinks = true;
		analysisTurnError = '';
		try {
			const updated = await ipc.investigationSetAnalysisTurnQuestions(
				selected.id,
				openAnalysisTurnId,
				questionLinksDraft
			);
			investigations = investigations.map((item) => item.id === updated.id ? updated : item);
			questionLinksDraft = questionIdsForRun(openAnalysisTurnId, updated);
		} catch (cause) {
			analysisTurnError = cause instanceof Error ? cause.message : String(cause);
		} finally {
			savingQuestionLinks = false;
		}
	}

	function reviewQuestion(questionId: string): void {
		if (!selected || saving || proposingQuestions || openingConversation || session.activeChat.busy) return;
		if (hasBlankQuestion) {
			error = 'Fill in or remove each blank Question before analyzing the plan.';
			return;
		}
		pendingAnalysisQuestionId = questionId;
		error = '';
	}

	async function analyzeQuestion(questionId: string): Promise<void> {
		const item = selected;
		if (
			!item ||
			pendingAnalysisQuestionId !== questionId ||
			saving ||
			proposingQuestions ||
			openingConversation ||
			startingQuestionAnalysis
		) return;
		if (hasBlankQuestion) {
			error = 'Fill in or remove each blank Question before analyzing the plan.';
			return;
		}
		if (hasBlankDefinition) {
			error = 'Complete or remove each blank definition before running this Question.';
			return;
		}
		startingQuestionAnalysis = true;
		try {
			if (item.status === 'complete') {
				acceptInvestigationUpdate(await ipc.investigationReopen(item.id));
			}
			if (questionsDirty && !(await saveQuestions())) return;
			if (definitionsDirty && !(await saveDefinitions())) return;
			if (!(await openSelectedConversation())) return;
			pendingAnalysisQuestionId = null;
			await analyzeInvestigationQuestion(item.id, questionId);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
		} finally {
			startingQuestionAnalysis = false;
		}
	}

	function moveQuestion(index: number, offset: -1 | 1): void {
		const destination = index + offset;
		if (destination < 0 || destination >= subquestions.length || !rootQuestion) return;
		const next = [...subquestions];
		[next[index], next[destination]] = [next[destination], next[index]];
		questions = [rootQuestion, ...next];
	}

	function removeQuestion(id: string): void {
		if (id === rootQuestion?.id) return;
		questions = questions.filter((question) => question.id !== id);
		if (pendingAnalysisQuestionId === id) pendingAnalysisQuestionId = null;
	}

	function discardQuestions(): void {
		const item = selected;
		if (!item || saving || proposingQuestions) return;
		questions = (item.questions ?? (item.brief.goal.trim()
			? [{ id: `legacy-${item.id}`, text: item.brief.goal, parent_id: undefined, source_selection: undefined }]
			: [])).map((question) => ({
			id: question.id,
			text: question.text,
			parent_id: question.parent_id,
			source_selection: question.source_selection
		}));
		savedQuestions = JSON.stringify(questions);
		pendingAnalysisQuestionId = null;
		error = '';
	}

	async function suggestBreakdown(): Promise<void> {
		const item = selected;
		const root = rootQuestion;
		if (!item || !root || !canSuggestBreakdown || proposingQuestions || saving) return;
		proposingQuestions = true;
		error = '';
		try {
			const proposal = await ipc.investigationProposeQuestions(item.id, root.text.trim());
			if (selected?.id !== item.id) return;
			questions = [
				{ ...root },
				...proposal.subquestions.map((text) => {
					const id = globalThis.crypto?.randomUUID?.().replaceAll('-', '') ?? `${Date.now()}${Math.random().toString(16).slice(2)}`;
					return { id: `q_${id}`, parent_id: root.id, text };
				})
			];
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
		} finally {
			proposingQuestions = false;
		}
	}

	async function saveQuestions(): Promise<boolean> {
		if (!selected || saving || !questionsDirty) return !questionsDirty;
		if (hasBlankQuestion) {
			error = 'Fill in or remove each blank Question before saving the plan.';
			return false;
		}
		saving = true;
		error = '';
		try {
			const updated = await ipc.investigationUpdateQuestions(
				selected.id,
				questions.map((question) => ({
					id: question.id,
					parent_id: question.parent_id,
					text: question.text.trim(),
					source_selection: question.source_selection
				}))
			);
			investigations = investigations.map((item) => item.id === updated.id ? updated : item);
			questions = (updated.questions ?? []).map((question) => ({
				id: question.id,
				text: question.text,
				parent_id: question.parent_id,
				source_selection: question.source_selection
			}));
			savedQuestions = JSON.stringify(questions);
			return true;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
			return false;
		} finally {
			saving = false;
		}
	}

	function addDefinition(kind: InvestigationDefinitionInput['kind'] = 'definition'): void {
		const id = globalThis.crypto?.randomUUID?.().replaceAll('-', '') ?? `${Date.now()}${Math.random().toString(16).slice(2)}`;
		definitions = [...definitions, { id: `d_${id}`, kind, text: '' }];
	}

	function runAssumptionInContext(text: string): boolean {
		const normalized = text.trim().toLowerCase();
		return Boolean(normalized && definitions.some((definition) =>
			definition.kind === 'assumption' && definition.text.trim().toLowerCase() === normalized
		));
	}

	async function addRunAssumptionToContext(text: string): Promise<void> {
		const normalized = text.trim();
		if (!normalized || saving || runAssumptionInContext(normalized)) return;
		const id = globalThis.crypto?.randomUUID?.().replaceAll('-', '') ?? `${Date.now()}${Math.random().toString(16).slice(2)}`;
		const definitionId = `d_${id}`;
		definitions = [...definitions, { id: definitionId, kind: 'assumption', text: normalized }];
		await tick();
		const field = document.getElementById(`definition-text-${definitionId}`);
		field?.scrollIntoView({ behavior: 'smooth', block: 'center' });
		field?.focus({ preventScroll: true });
	}

	function removeDefinition(id: string): void {
		definitions = definitions.filter((definition) => definition.id !== id);
	}

	function discardDefinitions(): void {
		const item = selected;
		if (!item || saving) return;
		definitions = (item.definitions ?? []).map((definition) => ({
			id: definition.id,
			kind: definition.kind,
			text: definition.text
		}));
		savedDefinitions = JSON.stringify(definitions);
		error = '';
	}

	async function saveDefinitions(): Promise<boolean> {
		if (!selected || saving || !definitionsDirty) return !definitionsDirty;
		if (hasBlankDefinition) {
			error = 'Fill in or remove each blank definition before saving.';
			return false;
		}
		saving = true;
		error = '';
		try {
			const updated = await ipc.investigationUpdateDefinitions(
				selected.id,
				definitions.map((definition) => ({
					id: definition.id,
					kind: definition.kind,
					text: definition.text.trim()
				}))
			);
			investigations = investigations.map((item) => item.id === updated.id ? updated : item);
			definitions = (updated.definitions ?? []).map((definition) => ({
				id: definition.id,
				kind: definition.kind,
				text: definition.text
			}));
			savedDefinitions = JSON.stringify(definitions);
			return true;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
			return false;
		} finally {
			saving = false;
		}
	}

	async function reviewProposal(accept: boolean): Promise<void> {
		const item = selected;
		const proposal = pendingProposal;
		if (!item || !proposal || saving || (accept && !proposalText.trim())) return;
		saving = true;
		error = '';
		let nextBrief = item.brief;
		if (accept) {
			nextBrief = { ...currentBrief };
			if (proposalTarget === 'goal') nextBrief.goal = proposalText.trim();
			else if (proposalTarget === 'desired_output') nextBrief.desired_output = proposalText.trim();
			else nextBrief.constraints = [...nextBrief.constraints, proposalText.trim()];
		}
		try {
			const updated = await ipc.investigationUpdateBrief(item.id, nextBrief, proposal.messageId);
			investigations = investigations.map((record) => record.id === updated.id ? updated : record);
			if (accept) savedBrief = JSON.stringify(updated.brief);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
		} finally {
			saving = false;
		}
	}

	async function selectInvestigation(id: string): Promise<void> {
		if (id === selectedId || saving || proposingQuestions) return;
		if (questionsDirty) {
			error = 'Save or discard the current Question plan before switching Investigations.';
			return;
		}
		if (definitionsDirty) {
			error = 'Save or discard the current definitions before switching Investigations.';
			return;
		}
		if (dirty && !(await saveBrief())) return;
		pendingAnalysisQuestionId = null;
		analysisTurnError = '';
		openAnalysisTurnId = null;
		questionLinksDraft = [];
		selectedId = id;
	}
</script>

<section class="investigation-page" aria-labelledby="investigation-title">
	<header class="page-head">
		<div>
				<h1 id="investigation-title">{activeView === 'results' ? 'Investigation results' : 'Investigation brief'}</h1>
				<p>{activeView === 'results' ? 'Review current conclusions, visual evidence, and Questions that still need attention.' : 'Keep the goal, audience, deliverable, and constraints clear as the analysis develops.'}</p>
		</div>
		<div class="page-head-actions">
			{#if reportExportStatus}
				<span class="report-export-status" role={reportExportFailed ? 'alert' : 'status'}>{reportExportStatus}</span>
			{/if}
			{#if selected}
				<Button
					type="button"
					variant="outline"
					disabled={exportingReport || Boolean(reportExportDisabledReason)}
					title={reportExportDisabledReason || undefined}
					onclick={() => void exportInvestigationReport()}
				>
					<Icon name="download" size={16} />{exportingReport ? 'Preparing report…' : 'Export report'}
				</Button>
			{/if}
			{#if workspacePath}
				<Button type="button" disabled={loading || creating} onclick={() => void createInvestigation()}>
					<Icon name="plus" size={16} />{creating ? 'Opening…' : currentConversationInvestigation ? 'Open current Investigation' : currentConversationSource ? 'Start from conversation' : 'New investigation'}
				</Button>
			{/if}
		</div>
	</header>
	{#if error && !selected}
		<p class="error" role="alert">{error}</p>
	{/if}
	{#if workspacePath && !loading}
		<InvestigationSourceReadiness
			{workspacePath}
			investigation={selected}
			onUpdated={acceptInvestigationUpdate}
		/>
	{/if}

	{#if !workspacePath}
		<div class="empty-state">
			<Icon name="folder" size={20} />
			<h2>Open a workspace to start an Investigation</h2>
			<p>Investigations are scoped to a folder so their intent stays alongside the data being analyzed.</p>
		</div>
	{:else if loading}
		<p class="status" role="status">Loading investigations…</p>
	{:else if !investigations.length}
		<div class="empty-state">
			<Icon name="bookmark" size={20} />
			<h2>No investigations yet</h2>
			<p>Workspace questions start an Investigation automatically. You can also create one here before beginning.</p>
			<Button type="button" disabled={creating} onclick={() => void createInvestigation()}>
				<Icon name="plus" size={16} />{creating ? 'Opening…' : currentConversationSource ? 'Start from conversation' : 'Start from this workspace'}
			</Button>
		</div>
	{:else}
		<div class="investigation-layout">
			<nav class="investigation-list" aria-label="Workspace investigations">
				{#each investigations as item (item.id)}
					<button
						class="investigation-choice"
						class:selected={item.id === selectedId}
						type="button"
						aria-current={item.id === selectedId ? 'true' : undefined}
						disabled={saving}
						onclick={() => void selectInvestigation(item.id)}
					>
						<span>{item.title}</span>
						<small>{new Date(item.updated_at_ms).toLocaleDateString()}</small>
					</button>
				{/each}
			</nav>

			{#if selected}
				<div class="brief-editor">
					<div class="editor-heading">
						<div>
							<h2>{selected.title}</h2>
							{#if selected.brief_source_message_id}
								<p>Goal seeded from the conversation’s first request. Editing this brief does not change that message.</p>
							{/if}
						</div>
						<div class="editor-actions">
							<Button type="button" variant="outline" disabled={openingConversation || (!selected.conversation_id && !selected.brief_source_message_id)} onclick={() => void openSelectedConversation()}>
								<Icon name="ask" size={16} />{openingConversation ? 'Opening…' : 'Open conversation'}
							</Button>
							{#if activeView === 'brief'}
								<Button type="button" disabled={!dirty || saving} onclick={() => void saveBrief()}>
									<Icon name="check" size={16} />{saving ? 'Saving…' : 'Save brief'}
								</Button>
							{/if}
						</div>
					</div>
					<div class="investigation-view-switch" role="group" aria-label="Investigation view">
						<Button type="button" variant={activeView === 'brief' ? 'secondary' : 'ghost'} size="sm" aria-pressed={activeView === 'brief'} onclick={() => (activeView = 'brief')}>Brief</Button>
						<Button type="button" variant={activeView === 'results' ? 'secondary' : 'ghost'} size="sm" aria-pressed={activeView === 'results'} onclick={() => (activeView = 'results')}>Results</Button>
					</div>
					{#if activeView === 'results'}
						<InvestigationResultsView
							investigation={selected}
							hasUnsavedEdits={dirty || questionsDirty || definitionsDirty}
							onopenrun={openRunFromResults}
							onreviewquestion={reviewQuestionFromResults}
						/>
					{:else}
					{#if pendingProposal}
						<section class="brief-proposal" aria-labelledby="brief-proposal-title">
							<div class="proposal-copy">
								<h3 id="brief-proposal-title">Possible change to the brief</h3>
								<p>A message uses explicit revision or source-scope language. Review it before updating the brief.</p>
							</div>
							<label for="proposal-target">Apply as</label>
							<select id="proposal-target" bind:value={proposalTarget} disabled={saving}>
								<option value="goal">New goal</option>
								<option value="desired_output">Desired output</option>
								<option value="constraint">Additional constraint</option>
							</select>
							<Textarea id="proposal-text" class="brief-field proposal-field" bind:value={proposalText} disabled={saving} aria-label="Proposed brief update" />
							<div class="proposal-actions">
								<Button type="button" disabled={saving || !proposalText.trim()} onclick={() => void reviewProposal(true)}>
									<Icon name="check" size={16} />{saving ? 'Saving…' : 'Apply update'}
								</Button>
								<Button type="button" variant="ghost" disabled={saving} onclick={() => void reviewProposal(false)}>Dismiss</Button>
							</div>
						</section>
					{/if}
					<label for="investigation-goal">Goal</label>
					<Textarea id="investigation-goal" class="brief-field goal-field" bind:value={goal} disabled={saving} placeholder="What question should this Investigation answer?" />
					<label for="investigation-audience">Audience</label>
					<Textarea id="investigation-audience" class="brief-field audience-field" bind:value={audience} disabled={saving} placeholder="Who needs this analysis?" />
					<label for="investigation-output">Desired output</label>
					<Textarea id="investigation-output" class="brief-field" bind:value={desiredOutput} disabled={saving} placeholder="What should the analyst deliver?" />
					<label for="investigation-constraints">Constraints</label>
					<Textarea id="investigation-constraints" class="brief-field constraints-field" bind:value={constraintsText} disabled={saving} placeholder="One constraint per line" />
					<section class="definition-editor" aria-labelledby="definition-context-title">
						<header class="question-heading">
							<div>
								<h3 id="definition-context-title">Definitions &amp; assumptions</h3>
								<p>Save business meanings that should guide future runs. Each run keeps the exact versions it used.</p>
							</div>
							<div class="question-save">
								<span role={error && definitionsDirty ? 'alert' : 'status'}>{error && definitionsDirty ? error : definitionsDirty ? 'Unsaved context' : 'Context saved'}</span>
								{#if definitionsDirty}
									<Button type="button" variant="ghost" disabled={saving} onclick={discardDefinitions}>Discard</Button>
								{/if}
								<Button type="button" disabled={!definitionsDirty || saving || hasBlankDefinition} onclick={() => void saveDefinitions()}>
									<Icon name="check" size={16} />{saving ? 'Saving…' : 'Save context'}
								</Button>
							</div>
						</header>
						{#if definitions.length}
							<ol class="definition-list">
								{#each definitions as definition, index (definition.id)}
									<li class="definition-item">
										<div class="definition-item-heading">
											<label for="definition-kind-{definition.id}">Context type</label>
											<div class="definition-item-meta">
												{#if selected?.definitions?.find((saved) => saved.id === definition.id)}
													<span>Saved revision {selected.definitions.find((saved) => saved.id === definition.id)?.revision}</span>
												{/if}
												<Button type="button" variant="ghost" aria-label="Remove context note {index + 1}" disabled={saving} onclick={() => removeDefinition(definition.id)}>Remove</Button>
											</div>
										</div>
										<select id="definition-kind-{definition.id}" aria-label="Context type {index + 1}" bind:value={definition.kind} disabled={saving}>
											<option value="definition">Definition</option>
											<option value="assumption">Assumption</option>
										</select>
										<Textarea
											id="definition-text-{definition.id}"
											class="definition-field"
											bind:value={definition.text}
											disabled={saving}
											aria-label="Context note {index + 1}"
											placeholder={definition.kind === 'definition' ? 'e.g. An active customer has placed a paid order in the past 90 days.' : 'e.g. Treat missing cancellation dates as still active.'}
										/>
									</li>
								{/each}
							</ol>
						{:else}
							<p class="definition-empty">No saved definitions or assumptions. Runs use no Investigation-specific semantic context.</p>
						{/if}
						<Button type="button" variant="outline" disabled={saving} onclick={() => addDefinition()}>
							<Icon name="plus" size={16} />Add definition or assumption
						</Button>
					</section>
					<section class="question-editor" aria-labelledby="question-plan-title">
						<header class="question-heading">
							<div>
								<h3 id="question-plan-title">Questions</h3>
								<p>The main request is the parent. Its subquestions are durable linked records; suggestions stay drafts until saved.</p>
							</div>
							<div class="question-save">
								<span role={error ? 'alert' : 'status'}>{error || (questionsDirty ? 'Unsaved plan' : 'Plan saved')}</span>
								{#if selected.status === 'complete'}
									<Button type="button" variant="outline" disabled={saving} onclick={() => void reopenInvestigation()}>
										Reopen Investigation
									</Button>
								{:else}
									<Button
										type="button"
										variant="outline"
										disabled={!coverage.canComplete || saving}
						title="Answer each active Question or record a reason for an incomplete or unsupported result; save current edits before completing."
										onclick={() => void markInvestigationComplete()}
									>
										Mark complete
									</Button>
								{/if}
								{#if questionsDirty}
									<Button type="button" variant="ghost" disabled={saving || proposingQuestions} onclick={discardQuestions}>Discard plan</Button>
								{/if}
								<Button
									type="button"
									variant="outline"
									size="sm"
									title={subquestions.length ? 'Remove current subquestions before drafting a replacement breakdown.' : undefined}
									disabled={!canSuggestBreakdown || saving || proposingQuestions}
									onclick={() => void suggestBreakdown()}
								>
									{proposingQuestions ? 'Suggesting…' : 'Suggest breakdown'}
								</Button>
								<Button type="button" disabled={!questionsDirty || saving || proposingQuestions || hasBlankQuestion} onclick={() => void saveQuestions()}>
									<Icon name="check" size={16} />{saving ? 'Saving…' : 'Save plan'}
								</Button>
							</div>
						</header>
						<section class="question-coverage" aria-label="Question coverage" data-investigation-status={selected.status}>
							<header>
								<div>
									<h4>Coverage</h4>
									<p>Answer each active Question or record a data-coverage gap. Runtime failures and outdated results still need review.</p>
								</div>
								<dl class="question-coverage-counts">
									<div data-coverage="answered"><dt>Answered</dt><dd>{coverage.answered}</dd></div>
									<div data-coverage="partial"><dt>Partial</dt><dd>{coverage.partial}</dd></div>
									<div data-coverage="unanswered"><dt>Unanswered</dt><dd>{coverage.unanswered}</dd></div>
									<div data-coverage="unsupported"><dt>Unsupported</dt><dd>{coverage.unsupported}</dd></div>
								</dl>
							</header>
							{#if selected.status === 'complete'}
								{#if coverage.documentedGaps.length}
									<p class="question-coverage-message" role="status">Investigation closed with documented gaps. It is not fully answered; reopen it to continue analysis.</p>
									<details class="question-coverage-review" open>
										<summary>{coverage.documentedGaps.length} documented Question gap{coverage.documentedGaps.length === 1 ? '' : 's'}</summary>
										<ul>
											{#each coverage.documentedGaps as question (question.id)}
												<li>
													<span class="coverage-question-status" data-question-status={question.answer_status}>{questionAnswerStatusLabel(question.answer_status)}</span>
													<strong>{question.text}</strong>
													<small>{questionCoverageReason(question)}</small>
												</li>
											{/each}
										</ul>
									</details>
								{:else}
									<p class="question-coverage-message" role="status">Investigation fully answered. Reopen it to continue editing or analysis.</p>
								{/if}
							{:else if !coverage.activeQuestions.length}
								<p class="question-coverage-message">Add at least one Question before completing this Investigation.</p>
							{:else if coverage.needsReview.length}
								{#if coverage.canComplete && coverage.documentedGaps.length}
									<p class="question-coverage-message" role="status">Ready to close with {coverage.documentedGaps.length} documented gap{coverage.documentedGaps.length === 1 ? '' : 's'}; this does not mean every Question was fully answered.</p>
								{/if}
								<details class="question-coverage-review" open>
									<summary>{coverage.needsReview.length} Question{coverage.needsReview.length === 1 ? '' : 's'} need an answer or review</summary>
									<ul>
										{#each coverage.needsReview as question (question.id)}
											<li>
												<button type="button" aria-label="Review {questionAnswerStatusLabel(question.answer_status)} Question: {question.text}" onclick={() => focusQuestion(question.id)}>
													<span class="coverage-question-status" data-question-status={question.answer_status}>{questionAnswerStatusLabel(question.answer_status)}</span>
													<strong>{question.text}</strong>
													<small>{questionCoverageReason(question)}</small>
												</button>
											</li>
										{/each}
									</ul>
								</details>
							{:else if dirty || questionsDirty || definitionsDirty}
								<p class="question-coverage-message">Save your current brief, Question plan, and definitions before completing.</p>
							{:else}
								<p class="question-coverage-message" role="status">All active Questions have complete, current answers and are ready to close.</p>
							{/if}
						</section>
						{#if questions.length}
							{#if rootQuestion}
								<div class="question-item root-question-item">
									<div class="question-item-heading">
										<label for="question-{rootQuestion.id}">Main question</label>
										<span class="question-parent-label">Parent request</span>
										<Badge
											variant={questionAnswerStatusVariant(questionAnswerStatus(rootQuestion.id))}
											aria-label="Main Question status: {questionAnswerStatusLabel(questionAnswerStatus(rootQuestion.id))}"
											data-question-status={questionAnswerStatus(rootQuestion.id)}
										>{questionAnswerStatusLabel(questionAnswerStatus(rootQuestion.id))}</Badge>
										<Button
											type="button"
											variant="outline"
											size="sm"
											aria-label="Analyze main Question"
											disabled={saving || proposingQuestions || openingConversation || session.activeChat.busy || hasBlankQuestion}
											onclick={() => reviewQuestion(rootQuestion.id)}
										>
											Analyze
										</Button>
									</div>
									{#if questionAnswerReason(rootQuestion.id)}
										<p class="question-status-reason" data-question-status={questionAnswerStatus(rootQuestion.id)} role="status">{questionAnswerReason(rootQuestion.id)}</p>
									{/if}
									{#if savedQuestion(rootQuestion.id)}
										<QuestionAnswerPreview
											question={savedQuestion(rootQuestion.id)!}
											questionLabel="Main Question"
											onopenrun={openAnalysisRun}
										/>
									{/if}
									{#if questionHistoricalRunIds(rootQuestion.id).length}
										<div class="question-run-links" role="group" aria-label="Earlier linked analysis runs for Main Question">
											<span>Earlier runs</span>
											{#each questionHistoricalRunIds(rootQuestion.id) as turnId, index (turnId)}
												<Button
													type="button"
													variant="ghost"
													size="sm"
													aria-label="Open earlier analysis run {index + 1} for Main Question"
													onclick={() => void openAnalysisRun(turnId)}
												>
													Run {index + 1}
												</Button>
											{/each}
										</div>
									{/if}
									<Textarea
										id="question-{rootQuestion.id}"
										class="question-field"
										bind:value={rootQuestion.text}
										disabled={saving || proposingQuestions}
										aria-label="Main question"
									/>
									<InvestigationQuestionSourceScope
										workspacePath={selected.workspace_id}
										investigation={selected}
										questionId={rootQuestion.id}
										selection={rootQuestion.source_selection}
										disabled={saving || proposingQuestions}
										onChange={(selection) => updateQuestionSourceSelection(rootQuestion.id, selection)}
									/>
								</div>
							{/if}
							{#if subquestions.length}
								<h4 class="subquestion-title">Breakdown</h4>
								<ol class="question-list">
									{#each subquestions as question, index (question.id)}
										<li class="question-item">
											<div class="question-item-heading">
												<label for="question-{question.id}">Subquestion {index + 1}</label>
												<Badge
													variant={questionAnswerStatusVariant(questionAnswerStatus(question.id))}
													aria-label="Subquestion {index + 1} status: {questionAnswerStatusLabel(questionAnswerStatus(question.id))}"
													data-question-status={questionAnswerStatus(question.id)}
												>{questionAnswerStatusLabel(questionAnswerStatus(question.id))}</Badge>
												<div class="question-item-actions">
													<Button
														type="button"
														variant="outline"
														size="sm"
														aria-label="Analyze Subquestion {index + 1}"
													disabled={saving || proposingQuestions || openingConversation || session.activeChat.busy || hasBlankQuestion}
													onclick={() => reviewQuestion(question.id)}
													>
														Analyze
													</Button>
													<Button type="button" variant="ghost" aria-label="Move Subquestion {index + 1} up" disabled={saving || proposingQuestions || index === 0} onclick={() => moveQuestion(index, -1)}>↑</Button>
													<Button type="button" variant="ghost" aria-label="Move Subquestion {index + 1} down" disabled={saving || proposingQuestions || index === subquestions.length - 1} onclick={() => moveQuestion(index, 1)}>↓</Button>
													<Button type="button" variant="ghost" aria-label="Remove Subquestion {index + 1}" disabled={saving || proposingQuestions} onclick={() => removeQuestion(question.id)}>Remove</Button>
												</div>
											</div>
											{#if questionAnswerReason(question.id)}
												<p class="question-status-reason" data-question-status={questionAnswerStatus(question.id)} role="status">{questionAnswerReason(question.id)}</p>
											{/if}
											{#if savedQuestion(question.id)}
												<QuestionAnswerPreview
													question={savedQuestion(question.id)!}
													questionLabel="Subquestion {index + 1}"
													onopenrun={openAnalysisRun}
												/>
											{/if}
											{#if questionHistoricalRunIds(question.id).length}
												<div class="question-run-links" role="group" aria-label="Linked analysis runs for Subquestion {index + 1}">
													<span>Linked runs</span>
													{#each questionHistoricalRunIds(question.id) as turnId, runIndex (turnId)}
														<Button
															type="button"
															variant="ghost"
															size="sm"
															aria-label="Open linked analysis run {runIndex + 1} for Subquestion {index + 1}"
															onclick={() => void openAnalysisRun(turnId)}
														>
															Run {runIndex + 1}
														</Button>
													{/each}
												</div>
											{/if}
											<Textarea
												id="question-{question.id}"
												class="question-field"
												bind:value={question.text}
												disabled={saving || proposingQuestions}
												aria-label="Subquestion {index + 1}"
											/>
											<InvestigationQuestionSourceScope
												workspacePath={selected.workspace_id}
												investigation={selected}
												questionId={question.id}
												selection={question.source_selection}
												disabled={saving || proposingQuestions}
												onChange={(selection) => updateQuestionSourceSelection(question.id, selection)}
											/>
										</li>
									{/each}
								</ol>
							{/if}
						{:else}
							<p class="question-empty">No Questions in this plan yet.</p>
						{/if}
						{#if pendingAnalysisQuestion}
							<section class="run-preview" aria-labelledby="run-preview-title">
								<header>
									<h4 id="run-preview-title">Review this run</h4>
									<p>No analysis has started. Confirm the Question, source scope, and definitions Fella will use.</p>
								</header>
								<dl class="run-preview-facts">
									<div><dt>Question</dt><dd>{pendingAnalysisQuestion.text.trim() || 'No Question text'}</dd></div>
									<div><dt>Source scope</dt><dd>{analysisSetupScopeLabel}</dd></div>
								</dl>
								<details class="run-preview-plan">
									<summary>Review the broader Question plan ({questions.length})</summary>
									<p>Fella receives this current plan as context alongside the Question above.</p>
									<ol>
										{#each questions as question (question.id)}
											<li>
												{question.text}
												{#if question.id === pendingAnalysisQuestion.id}<span> · running now</span>{/if}
											</li>
										{/each}
									</ol>
								</details>
								{#if analysisSetupExcludedPaths.length}
									<p class="run-preview-warning" role="status">
										{analysisSetupExcludedPaths.length} mapped file{analysisSetupExcludedPaths.length === 1 ? ' is' : 's are'} outside the Investigation scope and will not be used: {analysisSetupExcludedPaths.join(', ')}.
									</p>
								{/if}
								{#if analysisSetupCatalogAvailable && analysisSetupSourcePaths.length}
									<details class="run-preview-files">
										<summary>Review included files ({analysisSetupSourcePaths.length})</summary>
										<ul>{#each analysisSetupSourcePaths as path (path)}<li>{path}</li>{/each}</ul>
									</details>
								{:else if analysisSetupCatalogAvailable && analysisSetupIsNarrowed}
									<p class="run-preview-warning" role="alert">No files are included in this Question's effective source scope. Adjust the source set before running.</p>
								{:else if analysisSetupCatalogAvailable}
									<p class="run-preview-empty">No sources are currently cataloged; Fella will use all files available to this Investigation.</p>
								{:else}
									<p class="run-preview-warning" role="status">The workspace catalog is not loaded, so the included file list cannot be previewed.</p>
								{/if}
								<div class="run-preview-definitions">
									<strong>Definitions and assumptions</strong>
									{#if definitions.length}
										<ul>
											{#each definitions as definition (definition.id)}
												<li><span>{definition.kind === 'definition' ? 'Definition' : 'Assumption'}</span>{definition.text.trim() || 'Text is blank'}</li>
											{/each}
										</ul>
									{:else}
										<p>No Investigation-specific definitions or assumptions will be applied.</p>
									{/if}
								</div>
								{#if hasBlankDefinition}
									<p class="run-preview-warning" role="alert">Complete or remove each blank definition before running this Question.</p>
								{/if}
								{#if questionsDirty || definitionsDirty}
									<p class="run-preview-dirty">Your unsaved Question and context edits will be saved before this run starts.</p>
								{/if}
								<div class="run-preview-actions">
									<Button type="button" variant="ghost" disabled={startingQuestionAnalysis} onclick={() => (pendingAnalysisQuestionId = null)}>Cancel</Button>
									<Button
										type="button"
										disabled={saving || proposingQuestions || openingConversation || startingQuestionAnalysis || session.activeChat.busy || hasBlankQuestion || hasBlankDefinition || (analysisSetupCatalogAvailable && analysisSetupIsNarrowed && !analysisSetupSourcePaths.length)}
										onclick={() => void analyzeQuestion(pendingAnalysisQuestion.id)}
									>
										{startingQuestionAnalysis ? 'Preparing run…' : questionsDirty || definitionsDirty ? 'Save changes and run' : 'Run this Question'}
									</Button>
								</div>
							</section>
						{/if}
						<Button type="button" variant="outline" disabled={saving || proposingQuestions} onclick={addQuestion}>
							<Icon name="plus" size={16} />{rootQuestion ? 'Add subquestion' : 'Add main Question'}
						</Button>
					</section>
					<section class="findings" aria-labelledby="findings-title">
						<header class="findings-heading">
							<div>
								<h3 id="findings-title">Findings</h3>
								<p>Keep the conclusions you’ll reuse, each tied to the Question, run, and evidence that supports it.</p>
							</div>
							<span>{selected.findings?.length ?? 0} saved</span>
						</header>
						{#if findingError}
							<p class="finding-error" role="alert">{findingError}</p>
						{/if}
						{#if selected.findings?.length}
							<ol class="finding-list">
								{#each selected.findings as finding, index (finding.id)}
									<li class="finding-card">
										<div class="finding-card-heading">
											<div>
												<h4>Finding {index + 1} · Revision {finding.revision ?? 1}</h4>
												{#if editingFindingId !== finding.id}<p>{finding.claim}</p>{/if}
											</div>
											{#if editingFindingId !== finding.id}
												<Button type="button" variant="ghost" disabled={Boolean(updatingFindingId)} onclick={() => editFinding(finding)}>Edit finding</Button>
											{/if}
										</div>
										{#if editingFindingId === finding.id}
											<label for="finding-claim-edit-{finding.id}">Claim</label>
											<Textarea id="finding-claim-edit-{finding.id}" bind:value={findingClaimEditDraft} aria-label="Edit Finding claim" class="finding-edit-field" disabled={updatingFindingId === finding.id} />
											<label for="finding-caveats-edit-{finding.id}">Material caveats <span>(one per line)</span></label>
											<Textarea id="finding-caveats-edit-{finding.id}" bind:value={findingCaveatsEditDraft} aria-label="Edit Finding caveats" class="finding-edit-field" disabled={updatingFindingId === finding.id} />
											<label for="finding-assumptions-edit-{finding.id}">Assumptions <span>(one per line)</span></label>
											<Textarea id="finding-assumptions-edit-{finding.id}" bind:value={findingAssumptionsEditDraft} aria-label="Edit Finding assumptions" class="finding-edit-field" disabled={updatingFindingId === finding.id} />
												<label for="finding-revision-note-edit-{finding.id}">Why this changed <span>(optional)</span></label>
												<Textarea id="finding-revision-note-edit-{finding.id}" bind:value={findingRevisionNoteEditDraft} aria-label="Edit Finding revision note" class="finding-edit-field finding-revision-note" disabled={updatingFindingId === finding.id} />
											<div class="finding-edit-actions">
												<Button type="button" variant="ghost" disabled={updatingFindingId === finding.id} onclick={() => (editingFindingId = null)}>Cancel</Button>
												<Button type="button" disabled={updatingFindingId === finding.id || !findingClaimEditDraft.trim()} onclick={() => void saveFindingEdit(finding)}>{updatingFindingId === finding.id ? 'Saving…' : 'Save finding'}</Button>
											</div>
										{/if}
										{#if editingFindingId !== finding.id && finding.assumptions?.length}
											<section class="finding-interpretation" aria-label="Assumptions">
												<h5>Assumptions</h5>
												<ul>{#each finding.assumptions as assumption (`${finding.id}-assumption-${assumption}`)}<li>{assumption}</li>{/each}</ul>
											</section>
										{/if}
										{#if editingFindingId !== finding.id && finding.caveats?.length}
											<section class="finding-interpretation finding-caveats" aria-label="Interpretation caveats">
												<h5>Interpretation caveats</h5>
												<ul>{#each finding.caveats as caveat (`${finding.id}-caveat-${caveat}`)}<li>{caveat}</li>{/each}</ul>
											</section>
										{/if}
										<dl class="finding-provenance">
											<div><dt>Question</dt><dd>{findingQuestionLabel(finding)}</dd></div>
											<div>
												<dt>Analysis run</dt>
												<dd><button class="finding-run-link" type="button" onclick={() => void openAnalysisRun(finding.analysis_turn_id)}>{analysisRunSummary(finding.analysis_turn_id)} · {finding.analysis_turn_id}</button></dd>
											</div>
											<div><dt>Supporting evidence IDs</dt><dd>{#each finding.evidence_ids as evidenceId, evidenceIndex (`${finding.id}-${evidenceId}`)}<code>{evidenceId}</code>{#if evidenceIndex < finding.evidence_ids.length - 1}<span>, </span>{/if}{/each}</dd></div>
										</dl>
										<p class="finding-edit-note">This edit keeps the linked Question, run, and evidence fixed. Use “Revise an existing Finding” from a new run to update its provenance.</p>
										{#if finding.revisions?.length}
											<section class="finding-history" aria-label="Finding revision history">
												<header><h5>History</h5><span>{finding.revisions.length} prior version{finding.revisions.length === 1 ? '' : 's'}</span></header>
												<ol>
													{#each finding.revisions as revision (`${finding.id}-revision-${revision.revision}`)}
														<li>
															<span>Revision {revision.revision}</span>
															<Button type="button" variant="ghost" size="sm" aria-label="Compare revision {revision.revision} with current" onclick={() => void compareFindingRevision(finding, revision)}>Compare</Button>
														</li>
													{/each}
												</ol>
												{#if findingComparisonSelection?.findingId === finding.id}
													{@const comparedRevision = finding.revisions.find((item) => item.revision === findingComparisonSelection?.revision)}
													{#if comparedRevision}
														<div class="finding-comparison" role="region" aria-label="Finding revision comparison">
															<header>
																<div><h6>Revision {comparedRevision.revision} → Revision {finding.revision ?? 1}</h6><p>Why this changed: {finding.revision_note || 'No reason recorded.'}</p></div>
																<Button type="button" variant="ghost" size="sm" aria-label="Close Finding comparison" onclick={() => (findingComparisonSelection = null)}>Close</Button>
															</header>
															{#if findingHistoryLoadingId === finding.id}
																<p role="status">Loading saved run context…</p>
															{:else}
																{#if findingHistoryError}<p class="finding-error" role="alert">{findingHistoryError}</p>{/if}
																<ul class="finding-change-list">{#each findingRevisionChanges(comparedRevision, finding) as change (`${finding.id}-${comparedRevision.revision}-${change}`)}<li>{change}</li>{/each}</ul>
																<div class="finding-comparison-grid">
																	<article aria-label="Previous Finding version">
																		<h6>Revision {comparedRevision.revision}</h6><p class="finding-comparison-claim">{comparedRevision.claim}</p>
																		<dl>
																			<div><dt>Caveats</dt><dd>{comparedRevision.caveats.join('; ') || 'None recorded'}</dd></div>
																			<div><dt>Assumptions</dt><dd>{comparedRevision.assumptions.join('; ') || 'None recorded'}</dd></div>
																			<div><dt>Revision note</dt><dd>{comparedRevision.revision_note || 'None recorded'}</dd></div>
																			<div><dt>Supporting run</dt><dd>{analysisRunSummary(comparedRevision.analysis_turn_id)} · {comparedRevision.analysis_turn_id}</dd></div>
																			<div><dt>Evidence</dt><dd>{comparedRevision.evidence_ids.join(', ')}</dd></div>
																			<div><dt>Method</dt><dd>{findingRunMethod(analysisTurns[comparedRevision.analysis_turn_id])}</dd></div>
																			<div><dt>Source scope</dt><dd>{findingRunScope(analysisTurns[comparedRevision.analysis_turn_id])}</dd></div>
																			<div><dt>Definitions</dt><dd>{findingRunDefinitions(analysisTurns[comparedRevision.analysis_turn_id])}</dd></div>
																		</dl>
																	</article>
																	<article aria-label="Current Finding version">
																		<h6>Revision {finding.revision ?? 1}</h6><p class="finding-comparison-claim">{finding.claim}</p>
																		<dl>
																			<div><dt>Caveats</dt><dd>{finding.caveats.join('; ') || 'None recorded'}</dd></div>
																			<div><dt>Assumptions</dt><dd>{finding.assumptions.join('; ') || 'None recorded'}</dd></div>
																			<div><dt>Revision note</dt><dd>{finding.revision_note || 'None recorded'}</dd></div>
																			<div><dt>Supporting run</dt><dd>{analysisRunSummary(finding.analysis_turn_id)} · {finding.analysis_turn_id}</dd></div>
																			<div><dt>Evidence</dt><dd>{finding.evidence_ids.join(', ')}</dd></div>
																			<div><dt>Method</dt><dd>{findingRunMethod(analysisTurns[finding.analysis_turn_id])}</dd></div>
																			<div><dt>Source scope</dt><dd>{findingRunScope(analysisTurns[finding.analysis_turn_id])}</dd></div>
																			<div><dt>Definitions</dt><dd>{findingRunDefinitions(analysisTurns[finding.analysis_turn_id])}</dd></div>
																		</dl>
																	</article>
																</div>
														{/if}
													</div>
												{/if}
										{/if}
											</section>
										{/if}
									</li>
								{/each}
							</ol>
						{:else}
							<p class="findings-empty">No Findings saved yet. Capture a concise conclusion from a reviewed run.</p>
						{/if}
					</section>
					<section class="analysis-runs" aria-labelledby="analysis-runs-title">
						<header class="analysis-runs-heading">
							<div>
								<h3 id="analysis-runs-title">Analysis runs</h3>
								<p>Canonical run records linked to this Investigation and its Questions.</p>
							</div>
							<span>{analysisRunIds(selected).length} linked</span>
						</header>
						{#if analysisRunIds(selected).length}
							<ol class="analysis-run-list">
								{#each analysisRunIds(selected) as turnId, index (turnId)}
									<li>
										<button
											class="analysis-run-link"
											type="button"
											aria-label="Open saved analysis run {index + 1}: {analysisRunSummary(turnId)}"
											aria-current={openAnalysisTurnId === turnId ? 'true' : undefined}
											onclick={() => void openAnalysisRun(turnId)}
										>
											<span class="analysis-run-number">Run {index + 1}</span>
											<span class="analysis-run-copy">
												<strong>{analysisRunSummary(turnId)}</strong>
								<small>{analysisRunStatusLine(turnId)}</small>
											</span>
											<Icon name="chevron-right" size={14} />
										</button>
									</li>
								{/each}
							</ol>
						{:else}
							<p class="analysis-runs-empty">No saved analysis runs are linked yet.</p>
						{/if}

						{#if openAnalysisTurnId}
							<section id="saved-analysis-run-detail" class="analysis-run-detail" aria-label="Saved analysis run" tabindex="-1">
								<header>
									<div>
										<h4>Saved analysis run</h4>
										<p>{analysisRunSummary(openAnalysisTurnId)}</p>
									</div>
									<Button type="button" variant="ghost" aria-label="Close saved analysis run" onclick={() => (openAnalysisTurnId = null)}>Close</Button>
								</header>
								{#if analysisTurnLoadingId === openAnalysisTurnId}
									<p role="status">Loading the saved run…</p>
								{:else if analysisTurnError}
									<p class="analysis-run-error" role="alert">Couldn't load or update this run: {analysisTurnError}</p>
								{:else if openAnalysisTurn}
									<dl class="analysis-run-facts">
										<div><dt>State</dt><dd>{runStateLabel(openAnalysisTurn.state)}</dd></div>
										<div><dt>Method</dt><dd>{openAnalysisTurn.plan.strategy.replaceAll('_', ' ')}</dd></div>
										<div><dt>Execution steps</dt><dd>{openAnalysisTurn.trace.steps.length}</dd></div>
										<div><dt>Recorded evidence</dt><dd>{openAnalysisTurn.result.evidence.length} items</dd></div>
										<div><dt>Run ID</dt><dd><code>{openAnalysisTurn.id}</code></dd></div>
										<div>
											<dt>Attempt</dt>
											<dd>
												{#if openAnalysisTurn.rerun_of}
													{@const parentOrdinal = analysisRunOrdinal(openAnalysisTurn.rerun_of)}
													{#if parentOrdinal}
														<Button
															type="button"
															variant="ghost"
															size="sm"
															class="analysis-run-parent-link"
															aria-label="Open parent run {parentOrdinal}"
															onclick={() => void openAnalysisRun(openAnalysisTurn.rerun_of!)}
														>
															Rerun of Run {parentOrdinal}
														</Button>
													{:else}
														Rerun of saved run <code>{openAnalysisTurn.rerun_of}</code>
													{/if}
												{:else}
													Original saved attempt
												{/if}
											</dd>
										</div>
									</dl>
									<dl class="analysis-run-question-snapshot">
										<div><dt>Question at run</dt><dd>{openAnalysisTurn.investigation_context?.question_text ?? openAnalysisTurn.question}</dd></div>
									</dl>
									{#if openAnalysisTurn.contract?.assumptions.length}
										<section class="analysis-run-assumptions" aria-label="Assumptions recorded for this run">
											<div>
												<h5>Assumptions recorded for this run</h5>
												<p>Interpretive choices, not observed facts. Add one to editable context to review or correct it for future runs; this saved run stays unchanged.</p>
											</div>
											<ul>
												{#each openAnalysisTurn.contract.assumptions as assumption, index (`${openAnalysisTurn.id}-assumption-${index}`)}
													<li>
														<span>{assumption}</span>
														<Button
															type="button"
															variant="outline"
															size="sm"
															disabled={saving || runAssumptionInContext(assumption)}
															aria-label="Add assumption to editable context: {assumption}"
															onclick={() => void addRunAssumptionToContext(assumption)}
														>
															{runAssumptionInContext(assumption) ? 'In context' : 'Add to editable context'}
														</Button>
													</li>
												{/each}
											</ul>
										</section>
									{/if}
									<section class="analysis-run-context" aria-label="Definitions and source scope used">
										<header>
							<h5>Inputs captured for this run</h5>
							<p>This snapshot stays fixed even if the Investigation changes later.</p>
										</header>
										<dl class="analysis-run-context-facts">
											<div>
												<dt>Requested source scope</dt>
												<dd>{sourceSelectionSummary(openAnalysisTurn.investigation_context?.source_selection)}</dd>
											</div>
							{#if openAnalysisTurn.investigation_context?.source_selection?.mode === 'selected_sources'}
												<div>
													<dt>Selected paths</dt>
									<dd>{openAnalysisTurn.investigation_context.source_selection?.included_paths.join(', ') || 'None'}</dd>
												</div>
											{/if}
											<div>
												<dt>Sources actually used</dt>
												<dd>
													{#if openAnalysisTurn.source_scope}
														{#if openAnalysisTurn.source_scope.used_sources.length}
															<ul class="analysis-run-context-list">
																{#each openAnalysisTurn.source_scope.used_sources as source (`${source.source}-${source.table ?? ''}`)}
																	<li>
													<span>{source.source}{#if source.table}{' · '}{source.table}{/if}</span>
																		{#if source.evidence_ids.length}<small>Evidence: {source.evidence_ids.join(', ')}</small>{/if}
																	</li>
										{/each}
															</ul>
														{:else}
															No source was linked to the recorded evidence.
														{/if}
													{:else}
														Not recorded for this run.
													{/if}
												</dd>
											</div>
											<div>
												<dt>Skipped sources</dt>
												<dd>
													{#if openAnalysisTurn.source_scope}
														{#if openAnalysisTurn.source_scope.skipped_sources.length}
															<ul class="analysis-run-context-list">
																{#each openAnalysisTurn.source_scope.skipped_sources as source (source)}<li>{source}</li>{/each}
															</ul>
														{:else}
															None recorded.
														{/if}
													{:else}
														Not recorded for this run.
													{/if}
												</dd>
											</div>
										</dl>
										<section class="analysis-run-context-definitions" aria-label="Definitions and assumptions at run time">
											<h6>Definitions and assumptions</h6>
											{#if openAnalysisTurn.investigation_context}
												{#if openAnalysisTurn.investigation_context.definitions.length}
													<ul>
														{#each openAnalysisTurn.investigation_context.definitions as definition (definition.id)}
															<li>
																<strong>{definition.kind === 'definition' ? 'Definition' : 'Assumption'} · revision {definition.revision}</strong>
																<span>{definition.text}</span>
															</li>
														{/each}
													</ul>
												{:else}
													<p>No saved Investigation definitions or assumptions were active.</p>
												{/if}
											{:else}
												<p>No Investigation context snapshot was recorded.</p>
											{/if}
										</section>
									</section>
									<div class="analysis-run-result" aria-label="Recorded answer">{openAnalysisTurn.result.text}</div>
									<section class="analysis-run-evidence" aria-label="Query and output evidence">
										<header>
											<h5>Query and output evidence</h5>
											<p>Inspect the recorded queries, results, and checks beside the run’s Question and source scope.</p>
										</header>
										<EvidenceBlock
											answer={answerForAnalysisRun(openAnalysisTurn)}
											bodyId={`investigation-run-evidence-${openAnalysisTurn.id}`}
											expanded={true}
										/>
									</section>
									<details class="analysis-run-trace">
										<summary>Planned method and recorded execution</summary>
										{#if openAnalysisTurn.plan.steps.length}
											<h5>Planned steps</h5>
											<ol>{#each openAnalysisTurn.plan.steps as step, index (`${openAnalysisTurn.id}-planned-${index}`)}<li>{step.replaceAll('_', ' ')}</li>{/each}</ol>
										{/if}
										<ol>
											{#each openAnalysisTurn.trace.steps as step, index (`${openAnalysisTurn.id}-${index}`)}
												<li>
													<strong>{step.operation.replaceAll('_', ' ')}</strong>
													<small>{step.success ? 'Succeeded' : 'Failed'}{#if step.duration_ms != null} · {step.duration_ms} ms{/if}</small>
													{#if step.summary}<p>{step.summary}</p>{/if}
													{#if step.sources.length}<p>Sources: {step.sources.join(', ')}</p>{/if}
												</li>
											{/each}
										</ol>
									</details>
									<fieldset class="analysis-run-question-links" disabled={savingQuestionLinks || questionsDirty}>
										<legend>Questions this run addresses</legend>
										{#if selected.questions?.length}
											{#each selected.questions as question (question.id)}
												<label>
													<input
														type="checkbox"
														checked={questionLinksDraft.includes(question.id)}
														disabled={savingQuestionLinks || questionsDirty || question.latest_analysis_turn_id === openAnalysisTurn.id}
														onchange={(event) => setQuestionRunLink(question.id, event.currentTarget.checked)}
													/>
													<span><strong>{questionHeading(question.id)}</strong><small>{question.text}</small></span>
												</label>
											{/each}
											<Button type="button" variant="outline" size="sm" disabled={!questionLinksDirty || savingQuestionLinks || questionsDirty} onclick={() => void saveAnalysisRunQuestions()}>
												{savingQuestionLinks ? 'Saving links…' : 'Save Question links'}
											</Button>
										{:else}
											<p>This Investigation has no active Questions to link.</p>
										{/if}
									</fieldset>
					<section class="finding-capture" aria-labelledby="finding-capture-title">
						<header>
							<h5 id="finding-capture-title">Save or revise a Finding</h5>
							<p>Capture material limits and assumptions so the conclusion stays interpretable when shared.</p>
						</header>
						<label for="finding-claim-{openAnalysisTurn.id}">Claim</label>
						<Textarea id="finding-claim-{openAnalysisTurn.id}" bind:value={findingClaimDraft} aria-label="Finding claim" class="finding-capture-field" disabled={savingFinding} maxlength={2000} />
						<label for="finding-caveats-{openAnalysisTurn.id}">Material caveats <span>(optional, one per line)</span></label>
						<Textarea id="finding-caveats-{openAnalysisTurn.id}" bind:value={findingCaveatsDraft} aria-label="Finding caveats" class="finding-capture-field finding-notes-field" disabled={savingFinding} />
						<label for="finding-assumptions-{openAnalysisTurn.id}">Assumptions <span>(optional, one per line)</span></label>
						<Textarea id="finding-assumptions-{openAnalysisTurn.id}" bind:value={findingAssumptionsDraft} aria-label="Finding assumptions" class="finding-capture-field finding-notes-field" disabled={savingFinding} />
						<label for="finding-question-{openAnalysisTurn.id}">Question this Finding answers</label>
						<select id="finding-question-{openAnalysisTurn.id}" aria-label="Question for Finding" bind:value={findingQuestionDraft} onchange={() => { findingRevisionTargetDraft = ''; findingRevisionNoteDraft = ''; }} disabled={savingFinding || questionsDirty || !(selected.questions?.length)}>
							<option value="">Select a Question</option>
							{#each selected.questions ?? [] as question (question.id)}
								<option value={question.id}>{question.text}</option>
							{/each}
						</select>
						{#if (selected.findings ?? []).some((finding) => finding.question_id === findingQuestionDraft)}
							<label for="finding-revision-target-{openAnalysisTurn.id}">Revise an existing Finding <span>(optional)</span></label>
							<select
								id="finding-revision-target-{openAnalysisTurn.id}"
								aria-label="Revise existing Finding"
								value={findingRevisionTargetDraft}
								onchange={(event) => selectFindingRevisionTarget(event.currentTarget.value)}
								disabled={savingFinding || questionsDirty}
							>
								<option value="">Save as a new Finding</option>
								{#each (selected.findings ?? []).filter((finding) => finding.question_id === findingQuestionDraft) as finding (finding.id)}
									<option value={finding.id}>Finding {finding.revision ?? 1} · {finding.claim.slice(0, 90)}</option>
								{/each}
							</select>
						{/if}
						{#if findingRevisionTargetDraft}
							<label for="finding-revision-note-{openAnalysisTurn.id}">Why this changed <span>(optional)</span></label>
							<Textarea id="finding-revision-note-{openAnalysisTurn.id}" bind:value={findingRevisionNoteDraft} aria-label="Finding revision note" class="finding-capture-field finding-notes-field" disabled={savingFinding} />
						{/if}
						<fieldset class="finding-evidence-select" disabled={savingFinding || questionsDirty}>
							<legend>Supporting evidence from this run</legend>
							{#if findingEvidenceOptions(openAnalysisTurn).length}
								{#each findingEvidenceOptions(openAnalysisTurn) as item (item.id)}
									<label><input type="checkbox" checked={findingEvidenceDraft.includes(item.id)} onchange={(event) => toggleFindingEvidence(item.id, event.currentTarget.checked)} /><span>{item.label}</span></label>
								{/each}
							{:else}
								<p>This run has no identified evidence items to attach.</p>
							{/if}
						</fieldset>
						<div class="finding-capture-actions">
							<span>{findingEvidenceDraft.length} evidence item{findingEvidenceDraft.length === 1 ? '' : 's'} selected</span>
							<Button type="button" disabled={savingFinding || questionsDirty || !findingClaimDraft.trim() || !findingQuestionDraft || !findingEvidenceDraft.length} onclick={() => void saveFinding()}>{savingFinding ? 'Saving…' : findingRevisionTargetDraft ? 'Save Finding revision' : 'Save Finding'}</Button>
						</div>
					</section>
								{:else}
									<p role="status">No saved record was returned for this run.</p>
								{/if}
							</section>
						{/if}
					</section>
					<div class="editor-foot">
						<span role={error ? 'alert' : 'status'}>{error || (saving ? 'Saving…' : dirty ? 'Unsaved changes' : 'Saved locally')}</span>
						{#if selected.brief_source_message_id}<span>Original request preserved</span>{/if}
						<span>{selected.analysis_turn_ids?.length ?? 0} saved run{(selected.analysis_turn_ids?.length ?? 0) === 1 ? '' : 's'} linked</span>
					</div>
					{/if}
				</div>
			{/if}
		</div>
	{/if}
</section>

<style>
	.investigation-page {
		flex: 1;
		min-height: 0;
		width: 100%;
		max-width: var(--content-max);
		margin: 0 auto;
		padding: var(--space-6) var(--pad);
		overflow: auto;
	}
	.page-head,
	.editor-heading,
	.editor-foot {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-4);
	}
	.page-head { margin-bottom: var(--space-5); }
	.page-head-actions { display: flex; min-width: 0; align-items: center; justify-content: flex-end; gap: var(--space-2); }
	.report-export-status { max-width: 300px; color: var(--text-dim); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.report-export-status[role='alert'] { color: var(--danger, #b42318); }
	h1, h2, p { margin: 0; }
	h1 { font-size: clamp(24px, 3vw, 32px); font-weight: 650; letter-spacing: -0.03em; }
	.page-head p, .editor-heading p { margin-top: var(--space-1); color: var(--text-dim); }
	.investigation-layout { display: grid; grid-template-columns: minmax(180px, 240px) minmax(0, 1fr); gap: var(--space-6); }
	.investigation-list { display: flex; flex-direction: column; gap: 4px; }
	.investigation-choice {
		display: flex; flex-direction: column; align-items: flex-start; gap: 3px;
		padding: 9px 11px; border: 1px solid transparent; border-radius: var(--radius-md);
		background: transparent; color: var(--text); text-align: left; cursor: pointer;
	}
	.investigation-choice:hover { background: var(--bg-inset); }
	.investigation-choice.selected { border-color: var(--border); background: var(--bg-inset); }
	.investigation-choice span { max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-sm); font-weight: 600; }
	.investigation-choice small, .status, .editor-foot { color: var(--text-faint); font-size: var(--fs-xs); }
	.error { margin: 0 0 var(--space-3); color: var(--danger, #b42318); font-size: var(--fs-sm); overflow-wrap: anywhere; }
	.brief-editor { min-width: 0; border-block: 1px solid var(--border); }
	.editor-heading { display: flex; align-items: center; justify-content: space-between; gap: var(--space-4); padding: var(--space-3) 0; border-bottom: 1px solid var(--border); }
	.editor-actions { display: flex; flex: none; align-items: center; gap: var(--space-2); }
	.investigation-view-switch { display: flex; width: fit-content; align-items: center; gap: 2px; padding: 3px; margin-top: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg-inset); }
	.investigation-view-switch :global(button[aria-pressed='true']) { box-shadow: 0 0 0 1px var(--border-strong, var(--border)); }
	.brief-proposal { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: center; gap: var(--space-3); padding: var(--space-3); margin-top: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg-inset); }
	.proposal-copy { grid-column: 1 / -1; }
	.proposal-copy h3 { margin: 0; font-size: var(--fs-sm); font-weight: 650; }
	.proposal-copy p { margin-top: 3px; color: var(--text-dim); font-size: var(--fs-xs); }
	.brief-proposal label { margin: 0; font-size: var(--fs-xs); }
	.brief-proposal select { min-width: 190px; padding: 6px 9px; border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg); color: var(--text); font: inherit; }
	.brief-proposal :global(.proposal-field) { grid-column: 1 / -1; min-height: 78px; }
	.proposal-actions { grid-column: 1 / -1; display: flex; justify-content: flex-end; gap: var(--space-2); }
	h2 { font-size: var(--fs-lg); font-weight: 650; }
	.brief-editor label { display: block; margin: var(--space-4) 0 var(--space-2); font-size: var(--fs-sm); font-weight: 600; }
	.brief-editor :global(.brief-field) { width: 100%; min-height: 74px; resize: vertical; }
	.brief-editor :global(.goal-field) { min-height: 112px; }
	.brief-editor :global(.constraints-field) { min-height: 96px; }
	.definition-editor { display: grid; gap: var(--space-3); padding: var(--space-4) 0; margin-top: var(--space-4); border-top: 1px solid var(--border); }
	.definition-editor h3 { margin: 0; font-size: var(--fs-md); font-weight: 650; }
	.definition-editor .question-heading p, .definition-empty { margin-top: var(--space-1); color: var(--text-dim); font-size: var(--fs-xs); }
	.definition-list { display: grid; gap: var(--space-3); margin: 0; padding: 0; list-style: none; }
	.definition-item { display: grid; gap: var(--space-2); min-width: 0; padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); }
	.definition-item-heading, .definition-item-meta { display: flex; align-items: center; justify-content: space-between; gap: var(--space-2); }
	.definition-item-heading label { margin: 0; font-size: var(--fs-sm); font-weight: 600; }
	.definition-item-meta { color: var(--text-faint); font-size: var(--fs-xs); }
	.definition-item-meta :global(button) { min-height: 30px; }
	.definition-item select { width: max-content; min-width: 140px; padding: 6px 9px; border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg); color: var(--text); font: inherit; }
	.brief-editor :global(.definition-field) { width: 100%; min-height: 68px; resize: vertical; }
	.definition-empty { margin: 0; }
	.question-editor { display: grid; gap: var(--space-3); padding: var(--space-4) 0; margin-top: var(--space-4); border-top: 1px solid var(--border); }
	.question-heading, .question-save, .question-item-heading, .question-item-actions { display: flex; align-items: center; justify-content: space-between; gap: var(--space-2); }
	.question-heading { align-items: flex-start; gap: var(--space-4); }
	.question-heading h3 { margin: 0; font-size: var(--fs-md); font-weight: 650; }
	.question-heading p, .question-empty { margin-top: var(--space-1); color: var(--text-dim); font-size: var(--fs-xs); }
	.question-save { flex: none; flex-wrap: wrap; justify-content: flex-end; }
	.question-save > span { color: var(--text-faint); font-size: var(--fs-xs); }
	.question-coverage { display: grid; gap: var(--space-3); padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg-inset); }
	.question-coverage > header { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--space-4); }
	.question-coverage h4 { margin: 0; font-size: var(--fs-sm); font-weight: 650; }
	.question-coverage > header p, .question-coverage-message { margin: 3px 0 0; color: var(--text-dim); font-size: var(--fs-xs); }
	.question-coverage-counts { display: grid; grid-template-columns: repeat(4, minmax(64px, auto)); gap: var(--space-2); margin: 0; }
	.question-coverage-counts > div { display: grid; gap: 2px; }
	.question-coverage-counts dt { color: var(--text-faint); font-size: 10px; }
	.question-coverage-counts dd { margin: 0; font-size: var(--fs-sm); font-weight: 650; font-variant-numeric: tabular-nums; }
	.question-coverage-message { margin: 0; }
	.question-coverage-review { min-width: 0; }
	.question-coverage-review > summary { width: fit-content; color: var(--text-dim); font-size: var(--fs-xs); cursor: pointer; }
	.question-coverage-review ul { display: grid; max-height: 240px; gap: var(--space-1); overflow: auto; margin: var(--space-2) 0 0; padding: 0; list-style: none; }
	.question-coverage-review button { display: grid; width: 100%; min-width: 0; justify-items: start; gap: 2px; padding: var(--space-2); border: 1px solid transparent; border-radius: var(--radius-sm); background: transparent; color: var(--text); text-align: left; cursor: pointer; }
	.question-coverage-review button:hover { border-color: var(--border); background: var(--bg); }
	.coverage-question-status { color: var(--warn); font-size: 10px; font-weight: 650; }
	.coverage-question-status[data-question-status='unsupported'], .coverage-question-status[data-question-status='blocked'] { color: var(--err); }
	.question-coverage-review strong { min-width: 0; font-size: var(--fs-xs); font-weight: 600; overflow-wrap: anywhere; }
	.question-coverage-review small { color: var(--text-faint); font-size: 11px; overflow-wrap: anywhere; }
	.subquestion-title { margin: 0; font-size: var(--fs-sm); font-weight: 650; }
	.question-list { display: grid; gap: var(--space-3); margin: 0; padding: 0; list-style: none; }
	.question-item { min-width: 0; padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); }
	.question-item-heading { margin-bottom: var(--space-2); }
	.question-item-heading label { margin: 0; font-size: var(--fs-sm); font-weight: 600; }
	.question-parent-label { color: var(--text-faint); font-size: var(--fs-xs); }
	.question-status-reason { margin: 0 0 var(--space-2); color: var(--warn); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.question-status-reason[data-question-status='blocked'] { color: var(--err); }
	.question-item-actions { gap: 0; }
	.question-item-actions :global(button) { min-width: 34px; min-height: 32px; padding: 4px 8px; }
	.brief-editor :global(.question-field) { width: 100%; min-height: 68px; resize: vertical; }
	.question-empty { margin: 0; }
	.question-run-links { display: flex; align-items: center; gap: var(--space-1); margin: 0 0 var(--space-2); color: var(--text-faint); font-size: var(--fs-xs); }
	.question-run-links > span { margin-right: var(--space-1); }
	.question-run-links :global(button) { min-height: 28px; padding-inline: var(--space-2); }
	.findings { display: grid; gap: var(--space-3); padding: var(--space-4) 0; margin-top: var(--space-4); border-top: 1px solid var(--border); }
	.findings-heading, .finding-card-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--space-3); }
	.findings-heading h3, .finding-card h4 { margin: 0; font-size: var(--fs-md); font-weight: 650; }
	.findings-heading p { margin: var(--space-1) 0 0; color: var(--text-dim); font-size: var(--fs-xs); }
	.findings-heading > span { color: var(--text-faint); font-size: var(--fs-xs); white-space: nowrap; }
	.finding-list { display: grid; gap: var(--space-2); margin: 0; padding: 0; list-style: none; }
	.finding-card { display: grid; gap: var(--space-2); min-width: 0; padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg); }
	.finding-card-heading > div { min-width: 0; }
	.finding-card-heading h4 { font-size: var(--fs-sm); }
	.finding-card-heading p { margin: var(--space-1) 0 0; font-size: var(--fs-sm); line-height: 1.5; white-space: pre-wrap; overflow-wrap: anywhere; }
	.finding-card > label { margin: var(--space-1) 0 0; color: var(--text-faint); font-size: var(--fs-xs); font-weight: 600; }
	.finding-card > label span, .finding-capture > label span { color: var(--text-faint); font-weight: 400; }
	.finding-interpretation { display: grid; gap: var(--space-1); padding: var(--space-2) var(--space-3); border-left: 2px solid var(--border-strong, var(--border)); border-radius: 0 var(--radius-sm) var(--radius-sm) 0; background: var(--bg-inset); }
	.finding-interpretation h5 { margin: 0; font-size: var(--fs-xs); font-weight: 650; }
	.finding-interpretation ul { display: grid; gap: 2px; margin: 0; padding-left: 1.1rem; color: var(--text-dim); font-size: var(--fs-xs); line-height: 1.45; }
	.finding-caveats { border-left-color: var(--warning, #9b6500); }
	.finding-caveats h5 { color: var(--warning, #9b6500); }
	.finding-history { display: grid; gap: var(--space-2); padding-top: var(--space-2); border-top: 1px solid var(--border); }
	.finding-history > header, .finding-comparison > header { display: flex; align-items: center; justify-content: space-between; gap: var(--space-2); }
	.finding-history > header h5, .finding-comparison h6 { margin: 0; font-size: var(--fs-xs); font-weight: 650; }
	.finding-history > header span { color: var(--text-faint); font-size: var(--fs-xs); }
	.finding-history ol { display: grid; gap: var(--space-1); margin: 0; padding: 0; list-style: none; }
	.finding-history ol li { display: flex; align-items: center; justify-content: space-between; gap: var(--space-2); color: var(--text-dim); font-size: var(--fs-xs); }
	.finding-history ol :global(button) { min-height: 26px; padding-inline: var(--space-2); }
	.finding-comparison { display: grid; gap: var(--space-2); padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg-inset); }
	.finding-comparison > header > div { min-width: 0; }
	.finding-comparison > header p { margin: 3px 0 0; color: var(--text-dim); font-size: var(--fs-xs); }
	.finding-change-list { display: flex; flex-wrap: wrap; gap: var(--space-1); margin: 0; padding: 0; list-style: none; }
	.finding-change-list li { padding: 2px var(--space-2); border: 1px solid var(--border); border-radius: 999px; color: var(--text-dim); font-size: var(--fs-xs); }
	.finding-comparison-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-2); }
	.finding-comparison-grid article { display: grid; align-content: start; gap: var(--space-2); min-width: 0; padding: var(--space-2); border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg); }
	.finding-comparison-grid article h6 { font-size: var(--fs-xs); }
	.finding-comparison-claim { margin: 0; font-size: var(--fs-sm); line-height: 1.45; overflow-wrap: anywhere; }
	.finding-comparison-grid dl { display: grid; gap: var(--space-2); margin: 0; }
	.finding-comparison-grid dl > div { display: grid; gap: 2px; min-width: 0; }
	.finding-comparison-grid dt { color: var(--text-faint); font-size: var(--fs-xs); }
	.finding-comparison-grid dd { margin: 0; font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.finding-provenance { display: grid; gap: var(--space-2); margin: 0; padding-top: var(--space-2); border-top: 1px solid var(--border); }
	.finding-provenance > div { display: grid; grid-template-columns: 132px minmax(0, 1fr); gap: var(--space-2); }
	.finding-provenance dt { color: var(--text-faint); font-size: var(--fs-xs); }
	.finding-provenance dd { min-width: 0; margin: 0; font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.finding-provenance code { padding: 1px 4px; border-radius: var(--radius-sm); background: var(--bg-inset); font-size: 11px; }
	.finding-run-link { padding: 0; border: 0; background: transparent; color: var(--brand); font: inherit; text-align: left; cursor: pointer; overflow-wrap: anywhere; }
	.finding-run-link:hover { text-decoration: underline; }
	.finding-edit-note, .findings-empty { margin: 0; color: var(--text-faint); font-size: var(--fs-xs); }
	:global(.finding-edit-field) { width: 100%; min-height: 72px; resize: vertical; }
	.finding-edit-actions, .finding-capture-actions { display: flex; align-items: center; justify-content: flex-end; gap: var(--space-2); }
	.finding-error { margin: 0; color: var(--danger, #b42318); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.analysis-runs { display: grid; gap: var(--space-3); padding: var(--space-4) 0; margin-top: var(--space-4); border-top: 1px solid var(--border); }
	.analysis-runs-heading, .analysis-run-detail > header { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--space-3); }
	.analysis-runs-heading h3, .analysis-run-detail h4 { margin: 0; font-size: var(--fs-md); font-weight: 650; }
	.analysis-runs-heading p, .analysis-run-detail > header p { margin: var(--space-1) 0 0; color: var(--text-dim); font-size: var(--fs-xs); }
	.analysis-runs-heading > span { color: var(--text-faint); font-size: var(--fs-xs); white-space: nowrap; }
	.analysis-run-list { display: grid; gap: var(--space-1); margin: 0; padding: 0; list-style: none; }
	.analysis-run-link { display: flex; width: 100%; min-width: 0; align-items: center; gap: var(--space-3); padding: var(--space-2) var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg); color: var(--text); text-align: left; cursor: pointer; }
	.analysis-run-link:hover, .analysis-run-link[aria-current='true'] { background: var(--bg-inset); }
	.analysis-run-number { flex: none; color: var(--text-faint); font-size: var(--fs-xs); }
	.analysis-run-copy { display: grid; min-width: 0; flex: 1; gap: 2px; }
	.analysis-run-copy strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--fs-sm); font-weight: 600; }
	.analysis-run-copy small { color: var(--text-faint); font-size: var(--fs-xs); }
	.analysis-run-link > :global(svg) { flex: none; color: var(--text-faint); }
	.analysis-runs-empty { margin: 0; color: var(--text-faint); font-size: var(--fs-xs); }
	.analysis-run-detail { display: grid; gap: var(--space-3); min-width: 0; padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg-inset); }
	.analysis-run-facts { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-2) var(--space-4); margin: 0; }
	.analysis-run-facts > div, .analysis-run-question-snapshot > div { min-width: 0; }
	.analysis-run-facts dt, .analysis-run-question-snapshot dt { color: var(--text-faint); font-size: var(--fs-xs); }
	.analysis-run-facts dd, .analysis-run-question-snapshot dd { margin: 2px 0 0; font-size: var(--fs-sm); overflow-wrap: anywhere; }
	.analysis-run-facts code { font-size: var(--fs-xs); }
	.analysis-run-facts :global(.analysis-run-parent-link) { min-height: 0; padding: 0; font-size: var(--fs-xs); }
	.analysis-run-question-snapshot { padding-top: var(--space-2); margin: 0; border-top: 1px solid var(--border); }
	.analysis-run-context { display: grid; gap: var(--space-2); padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg); }
	.analysis-run-context > header h5, .analysis-run-context-definitions h6, .analysis-run-evidence h5 { margin: 0; color: var(--text); font-size: var(--fs-sm); font-weight: 650; }
	.analysis-run-context > header p, .analysis-run-evidence header p { margin: 3px 0 0; color: var(--text-dim); font-size: var(--fs-xs); }
	.analysis-run-context-facts { display: grid; gap: var(--space-2); margin: 0; }
	.analysis-run-context-facts > div { display: grid; grid-template-columns: 150px minmax(0, 1fr); gap: var(--space-2); }
	.analysis-run-context-facts dt { color: var(--text-faint); font-size: var(--fs-xs); }
	.analysis-run-context-facts dd { min-width: 0; margin: 0; font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.analysis-run-context-list, .analysis-run-context-definitions ul { display: grid; gap: var(--space-1); margin: 0; padding-left: var(--space-4); }
	.analysis-run-context-list li { display: grid; gap: 2px; }
	.analysis-run-context-list small { color: var(--text-faint); font-size: 11px; }
	.analysis-run-context-definitions { display: grid; gap: var(--space-2); padding-top: var(--space-2); border-top: 1px solid var(--border); }
	.analysis-run-context-definitions li { display: grid; gap: 2px; }
	.analysis-run-context-definitions strong { font-size: var(--fs-xs); font-weight: 600; }
	.analysis-run-context-definitions li span, .analysis-run-context-definitions p { margin: 0; color: var(--text-dim); font-size: var(--fs-xs); white-space: pre-wrap; overflow-wrap: anywhere; }
	.analysis-run-evidence { display: grid; gap: var(--space-2); min-width: 0; }
	.analysis-run-evidence :global(.evidence-body) { margin-top: 0; }
	.analysis-run-assumptions { display: grid; gap: var(--space-2); padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg); }
	.analysis-run-assumptions h5 { margin: 0; color: var(--text); font-size: var(--fs-xs); font-weight: 650; }
	.analysis-run-assumptions p { margin: 3px 0 0; color: var(--text-dim); font-size: var(--fs-xs); }
	.analysis-run-assumptions ul { display: grid; gap: var(--space-2); margin: 0; padding: 0; list-style: none; }
	.analysis-run-assumptions li { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--space-3); }
	.analysis-run-assumptions li > span { min-width: 0; flex: 1; font-size: var(--fs-sm); overflow-wrap: anywhere; }
	.analysis-run-assumptions li :global(button) { flex: none; }
	.analysis-run-result { max-height: 320px; overflow: auto; padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg); font-size: var(--fs-sm); line-height: 1.55; white-space: pre-wrap; overflow-wrap: anywhere; }
	.analysis-run-trace { color: var(--text-dim); font-size: var(--fs-xs); }
	.analysis-run-trace summary { width: fit-content; cursor: pointer; }
	.analysis-run-trace ol { display: grid; gap: var(--space-1); margin: var(--space-2) 0 0; padding-left: var(--space-5); }
	.analysis-run-trace h5 { margin: var(--space-3) 0 0; color: var(--text); font-size: var(--fs-xs); }
	.analysis-run-trace li small { color: var(--text-faint); }
	.analysis-run-trace li p { margin: 2px 0 0; color: var(--text-dim); }
	.analysis-run-question-links { display: grid; gap: var(--space-2); min-width: 0; padding: var(--space-3); margin: 0; border: 1px solid var(--border); border-radius: var(--radius-sm); }
	.analysis-run-question-links legend { padding: 0 var(--space-1); color: var(--text-faint); font-size: var(--fs-xs); font-weight: 600; }
	.analysis-run-question-links label { display: flex; align-items: flex-start; gap: var(--space-2); margin: 0; font-size: var(--fs-sm); cursor: pointer; }
	.analysis-run-question-links input { flex: none; margin-top: 3px; accent-color: var(--brand); }
	.analysis-run-question-links label span { display: grid; min-width: 0; gap: 2px; }
	.analysis-run-question-links label strong { font-size: var(--fs-xs); font-weight: 600; }
	.analysis-run-question-links label small { color: var(--text-dim); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.finding-capture { display: grid; gap: var(--space-2); min-width: 0; padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg-inset); }
	.finding-capture header h5 { margin: 0; color: var(--text); font-size: var(--fs-sm); font-weight: 650; }
	.finding-capture header p { margin: 3px 0 0; color: var(--text-dim); font-size: var(--fs-xs); }
	.finding-capture > label { margin: var(--space-1) 0 0; color: var(--text-faint); font-size: var(--fs-xs); font-weight: 600; }
	.finding-capture :global(.finding-capture-field) { width: 100%; min-height: 66px; resize: vertical; }
	.finding-capture :global(.finding-notes-field) { min-height: 52px; }
	.finding-card :global(.finding-revision-note) { min-height: 52px; }
	.finding-capture select { width: 100%; min-width: 0; padding: 7px 9px; border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg); color: var(--text); font: inherit; font-size: var(--fs-sm); }
	.finding-evidence-select { display: grid; gap: var(--space-2); min-width: 0; padding: var(--space-2) var(--space-3); margin: var(--space-1) 0 0; border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg); }
	.finding-evidence-select legend { padding: 0 var(--space-1); color: var(--text-faint); font-size: var(--fs-xs); font-weight: 600; }
	.finding-evidence-select label { display: flex; align-items: flex-start; gap: var(--space-2); margin: 0; font-size: var(--fs-xs); cursor: pointer; }
	.finding-evidence-select input { flex: none; margin-top: 2px; accent-color: var(--brand); }
	.finding-evidence-select label span { min-width: 0; overflow-wrap: anywhere; }
	.finding-evidence-select p { margin: 0; color: var(--text-faint); font-size: var(--fs-xs); }
	.finding-capture-actions { margin-top: var(--space-1); }
	.finding-capture-actions > span { margin-right: auto; color: var(--text-faint); font-size: var(--fs-xs); }
	.analysis-run-error { margin: 0; color: var(--danger, #b42318); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.run-preview { display: grid; gap: var(--space-3); padding: var(--space-3); border: 1px solid var(--border); border-left: 3px solid var(--brand); border-radius: var(--radius-md); background: var(--bg-inset); }
	.run-preview header h4 { margin: 0; font-size: var(--fs-sm); font-weight: 650; }
	.run-preview header p, .run-preview-definitions p { margin: 3px 0 0; color: var(--text-dim); font-size: var(--fs-xs); }
	.run-preview-facts { display: grid; gap: var(--space-2); margin: 0; }
	.run-preview-facts > div { display: grid; grid-template-columns: 120px minmax(0, 1fr); gap: var(--space-2); }
	.run-preview-facts dt, .run-preview-definitions strong { color: var(--text-faint); font-size: var(--fs-xs); font-weight: 600; }
	.run-preview-facts dd { min-width: 0; margin: 0; font-size: var(--fs-sm); overflow-wrap: anywhere; }
	.run-preview-files { color: var(--text-dim); font-size: var(--fs-xs); }
	.run-preview-files summary { width: fit-content; cursor: pointer; }
	.run-preview-plan { color: var(--text-dim); font-size: var(--fs-xs); }
	.run-preview-plan summary { width: fit-content; cursor: pointer; }
	.run-preview-plan p { margin: var(--space-1) 0; color: var(--text-faint); }
	.run-preview-plan ol { display: grid; gap: var(--space-1); margin: 0; padding-left: var(--space-5); }
	.run-preview-plan li span { color: var(--brand); font-weight: 600; }
	.run-preview-files ul, .run-preview-definitions ul { display: grid; gap: var(--space-1); margin: var(--space-2) 0 0; padding-left: var(--space-5); }
	.run-preview-files li { overflow-wrap: anywhere; }
	.run-preview-definitions li { display: flex; gap: var(--space-2); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.run-preview-definitions li span { flex: none; color: var(--text-faint); }
	.run-preview-warning { margin: 0; color: var(--warn); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.run-preview-empty { margin: 0; color: var(--text-faint); font-size: var(--fs-xs); }
	.run-preview-dirty { margin: 0; color: var(--text-faint); font-size: var(--fs-xs); }
	.run-preview-actions { display: flex; justify-content: flex-end; gap: var(--space-2); }
	.editor-foot { padding: var(--space-3) 0; }
	.empty-state { max-width: 48ch; margin: 12vh auto 0; display: grid; justify-items: center; gap: var(--space-3); text-align: center; }
	.empty-state > :global(svg) { color: var(--text-faint); }
	.empty-state h2 { font-size: var(--fs-lg); }
	.empty-state p { color: var(--text-dim); }
	@media (max-width: 720px) {
		.investigation-layout { grid-template-columns: 1fr; gap: var(--space-4); }
		.investigation-list { max-height: 180px; overflow: auto; }
		.question-coverage > header { flex-direction: column; }
		.question-coverage-counts { width: 100%; grid-template-columns: repeat(4, minmax(0, 1fr)); }
	}
</style>
