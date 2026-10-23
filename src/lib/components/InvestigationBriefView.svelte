<script lang="ts">
	import { onDestroy, tick, untrack } from 'svelte';
	import { readInvestigationView, saveInvestigationView, type InvestigationSection } from '$lib/investigation-view-state';
	import { firstActualQuestion, session } from '$lib/session.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Badge } from '$lib/components/ui/badge';
	import { Dialog } from '$lib/components/ui/dialog';
	import { DropdownMenu } from '$lib/components/ui/dropdown-menu';
	import { Select } from '$lib/components/ui/select';
	import { analyzeInvestigationQuestion } from '$lib/commands';
	import { Textarea } from '$lib/components/ui/textarea';
	import { Input } from '$lib/components/ui/input';
	import { ipc, isDesktop } from '$lib/ipc';
	import { analysisTurnIdsAfterSourceMessage } from '$lib/investigation-runs';
	import { investigationQuestionStatusDetail } from '$lib/investigation-question-status';
	import { focusInvestigationConversation } from '$lib/investigation-navigation';
	import { buildInvestigationReport, investigationReportFilename, type InvestigationReportRuns } from '$lib/investigation-report';
	import { handleScrollRegionKeydown } from '$lib/scroll-region';
	import { runStateLabel } from '$lib/run-progress';
	import { isRenderableChartEvidence } from '$lib/workbench';
	import type {
		AnalysisTurn,
		Answer,
		EvidenceItem,
		Investigation,
		InvestigationBrief,
		InvestigationDefinitionInput,
		InvestigationFinding,
		InvestigationFindingRevision,
		InvestigationFindingUpdate,
		InvestigationQuestion,
		InvestigationQuestionInput,
		InvestigationQuestionAnswerStatus,
		AnalysisTurnReplayStatus,
		InvestigationSourceSelection,
		Message
	} from '$lib/types';
	import { findBriefProposal, type BriefProposal, type BriefProposalTarget } from '$lib/investigation-brief-proposal';
	import Icon from './Icon.svelte';
	import EvidenceBlock from './EvidenceBlock.svelte';
	import RunTimeline from './RunTimeline.svelte';
	import InvestigationSourceReadiness from './InvestigationSourceReadiness.svelte';
	import InvestigationQuestionSourceScope from './InvestigationQuestionSourceScope.svelte';
	import QuestionAnswerPreview from './QuestionAnswerPreview.svelte';
	import InvestigationResultsView from './InvestigationResultsView.svelte';
	import InvestigationRunOverview from './InvestigationRunOverview.svelte';
	import InvestigationResultsSummary from './InvestigationResultsSummary.svelte';
	import Chart from './Chart.svelte';
	import MarkdownContent from './MarkdownContent.svelte';

	let { viewKey = '', initialInvestigation = null, onclose, onexpand, expanded = false }: { viewKey?: string; initialInvestigation?: Investigation | null; onclose?: () => void; onexpand?: () => void; expanded?: boolean } = $props();
	// Freeze the keyed instance's owner before its parent switches live tab props.
	const stableViewKey = untrack(() => viewKey);
	const initialRecord = untrack(() => initialInvestigation);
	const initialView = readInvestigationView(stableViewKey);
	const sectionScroll = { ...initialView.scroll };
	let scroller = $state<HTMLElement>();
	let restoringScroll = false;
	let viewRestoreGeneration = 0;
	let nameOpen = $state(false);
	let nameDraft = $state('');
	let nameError = $state('');
	let naming = $state(false);

	type HandoffChart = {
		key: string;
		run: AnalysisTurn;
		evidence: EvidenceItem;
		sourceEvidence: EvidenceItem | null;
	};
	type HandoffPreview = {
		investigation: Investigation;
		runs: InvestigationReportRuns;
		markdown: string;
		filename: string;
		charts: HandoffChart[];
	};
	const investigationSections: { id: InvestigationSection; label: string }[] = [
		{ id: 'brief', label: 'Brief' },
		{ id: 'data', label: 'Sources' },
		{ id: 'questions', label: 'Questions' },
		{ id: 'analysis', label: 'Analysis' },
		{ id: 'results', label: 'Results' }
	];

	let investigations = $state<Investigation[]>(initialRecord ? [initialRecord] : []);
	let selectedId = $state<string | null>(initialRecord?.id ?? null);
	let investigationSection = $state<InvestigationSection>(initialView.section);
	let workspacePath = $state<string | null>(null);
	let loadedWorkspace = $state<string | null>(null);
	let loadedConversationId: string | null = null;
	let loadedInvestigationsVersion = -1;
	let goal = $state('');
	let audience = $state('');
	let desiredOutput = $state('');
	let constraintsText = $state('');
	let briefEditing = $state(false);
	let questions = $state<InvestigationQuestionInput[]>([]);
	let definitions = $state<InvestigationDefinitionInput[]>([]);
	let definitionsOpen = $state(false);
	let historyOpen = $state(true);
	let editingQuestionIds = $state<string[]>([]);
	let newQuestionText = $state('');
	let savedBrief = $state('');
	let savedQuestions = $state('');
	let savedDefinitions = $state('');
	let briefDraftId = $state<string | null>(null);
	let questionDraftId = $state<string | null>(null);
	let definitionDraftId = $state<string | null>(null);
	let loadedBriefKey = '';
	let loadedQuestionsKey = '';
	let loadedDefinitionsKey = '';
	let proposalMessageId = $state<string | null>(null);
	let proposalText = $state('');
	let proposalTarget = $state<BriefProposalTarget>('goal');
	let pendingAnalysisQuestionId = $state<string | null>(null);
	let staleResultReview = $state<{ questionId: string; status: AnalysisTurnReplayStatus } | null>(null);
	let loading = $state(!initialRecord);
	let openingConversation = $state(false);
	let saving = $state(false);
	let autosaving = $state(false);
	let proposingQuestions = $state(false);
	let startingQuestionAnalysis = $state(false);
	let analysisTurns = $state<Record<string, AnalysisTurn>>({});
	let openAnalysisTurnId = $state<string | null>(null);
	let analysisTurnLoadingId = $state<string | null>(null);
	let analysisTurnError = $state('');
	let overviewRunId = $state<string | null>(initialView.openRunId);
	let overviewLoadingId = $state<string | null>(null);
	let overviewError = $state('');
	let questionLinksError = $state('');
	let questionLinksStatus = $state('');
	let questionLinksDraft = $state<string[]>([]);
	let savingQuestionLinks = $state(false);
	let findingClaimDraft = $state('');
	let findingCaveatsDraft = $state('');
	let findingAssumptionsDraft = $state('');
	let findingRevisionTargetDraft = $state('');
	let findingRevisionNoteDraft = $state('');
	let findingQuestionDraft = $state('');
	let findingEvidenceDraft = $state<string[]>([]);
	let findingDraftTouched = $state(false);
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
	let reportPreviewOpen = $state(false);
	let reportPreview = $state<HandoffPreview | null>(null);
	let error = $state('');
	let investigationLoadError = $state('');
	let conversationError = $state('');
	let autosaveError = $state('');
	let requestSerial = 0;
	let autosaveTimer: ReturnType<typeof setTimeout> | undefined;
	let autosaveInProgress: Promise<void> | null = null;
	let investigationPaneWasVisible = false;
	let quietRefreshInProgress = false;
	let quietRefreshPending = $state(false);
	const syncingInvestigations = new Set<string>();

	let selected = $derived(investigations.find((item) => item.id === selectedId) ?? null);
	let needsQuestionAttention = $derived((selected?.questions ?? []).some((question) => question.status === 'active' && question.answer_status !== 'answered'));
	let needsResultsReview = $derived((selected?.questions ?? []).some((question) => question.status === 'active' && ['incomplete', 'unsupported', 'insufficient_data', 'needs_review', 'outdated', 'blocked', 'waiting_for_input'].includes(question.answer_status)));
	let investigationConversationMessages = $derived.by(() => {
		if (!selected?.conversation_id) return [];
		return session.conversations.find((conversation) =>
			conversation.id === selected.conversation_id && conversation.workspaceScope === selected.workspace_id
		)?.messages ?? [];
	});
	let disclosureInvestigationId = '';
	$effect(() => {
		const id = selected?.id ?? '';
		if (id === disclosureInvestigationId) return;
		disclosureInvestigationId = id;
		if (!id) return;
		definitionsOpen = initialView.definitionsOpen;
		historyOpen = initialView.historyOpen;
		editingQuestionIds = [];
		if (initialView.openRunId && selected && analysisRunIds(selected).includes(initialView.openRunId)) {
			void untrack(() => openAnalysisRun(initialView.openRunId!, false));
		}
	});

	function rememberView(): void {
		if (!selected) return;
		saveInvestigationView(stableViewKey, { section: investigationSection, scroll: sectionScroll, openRunId: openAnalysisTurnId, definitionsOpen, historyOpen });
	}
	function rememberScroll(): void {
		if (!selected || loading || restoringScroll || !scroller) return;
		sectionScroll[investigationSection] = scroller.scrollTop;
		rememberView();
	}
	$effect(() => {
		const id = selected?.id, section = investigationSection, busy = loading || analysisTurnLoadingId !== null;
		if (!id || busy || !scroller) return;
		const generation = ++viewRestoreGeneration;
		restoringScroll = true;
		void tick().then(() => {
			if (generation !== viewRestoreGeneration || !scroller) return;
			scroller.scrollTop = sectionScroll[section] ?? 0;
			requestAnimationFrame(() => { if (generation === viewRestoreGeneration) restoringScroll = false; });
		});
		untrack(rememberView);
	});

	async function renameInvestigation(): Promise<void> {
		const item = selected;
		if (!item || naming || !nameDraft.trim()) return;
		naming = true; nameError = '';
		try {
			const updated = await ipc.investigationRename(item.id, nameDraft);
			acceptInvestigationUpdate(updated);
			nameOpen = false;
		} catch (cause) { nameError = cause instanceof Error ? cause.message : String(cause); }
		finally { naming = false; }
	}
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
	let analysisSetupCatalogPaths = $derived.by(() => {
		if (!selected || !analysisSetupCatalogAvailable) return [];
		return [...new Set(session.catalog.sources.map((source) => relativeSourcePath(source.path, selected.workspace_id)))];
	});
	let analysisSetupInvestigationPaths = $derived.by(() => {
		const item = selected;
		if (!item || !analysisSetupCatalogAvailable) return [];
		return item.source_selection.mode === 'all_sources'
			? analysisSetupCatalogPaths
			: analysisSetupCatalogPaths.filter((path) => item.source_selection.included_paths.includes(path));
	});
	let analysisSetupIsNarrowed = $derived(
		Boolean(pendingAnalysisQuestion?.source_selection || selected?.source_selection.mode === 'selected_sources')
	);
	let analysisSetupSourcePaths = $derived.by(() => {
		const question = pendingAnalysisQuestion;
		if (!question || !analysisSetupCatalogAvailable) return [];
		if (!question.source_selection) return analysisSetupInvestigationPaths;
		const allowed = new Set(analysisSetupInvestigationPaths);
		return question.source_selection.included_paths.filter((path) => allowed.has(path));
	});
	let analysisSetupRequestedPaths = $derived.by(() => {
		const item = selected;
		const question = pendingAnalysisQuestion;
		if (question?.source_selection) return [...new Set(question.source_selection.included_paths)];
		if (item?.source_selection.mode === 'selected_sources') return [...new Set(item.source_selection.included_paths)];
		return [];
	});
	let analysisSetupMissingPaths = $derived(
		analysisSetupRequestedPaths.filter((path) => !analysisSetupCatalogPaths.includes(path))
	);
	let analysisSetupOutOfScopePaths = $derived.by(() => {
		if (!pendingAnalysisQuestion?.source_selection) return [];
		return [...new Set(pendingAnalysisQuestion.source_selection.included_paths)].filter(
			(path) => analysisSetupCatalogPaths.includes(path) && !analysisSetupInvestigationPaths.includes(path)
		);
	});
	let analysisSetupScopeLabel = $derived.by(() => {
		const item = selected;
		const question = pendingAnalysisQuestion;
		if (!item || !question) return '';
		if (!analysisSetupCatalogAvailable) return 'Current workspace catalog is not loaded';
		if (question.source_selection) {
			return `Question-specific scope · ${analysisSetupSourcePaths.length} effective of ${new Set(question.source_selection.included_paths).size} selected`;
		}
		if (item.source_selection.mode === 'all_sources') {
			return `Inherit Investigation · ${analysisSetupSourcePaths.length} effective of ${analysisSetupCatalogPaths.length} cataloged`;
		}
		return `Inherit Investigation · ${analysisSetupSourcePaths.length} effective of ${new Set(item.source_selection.included_paths).size} selected`;
	});
	let subquestions = $derived(rootQuestion
		? questions.filter((question) => question.parent_id === rootQuestion.id)
		: []);
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
	let dirty = $derived(Boolean(selected && briefDraftId === selected.id && briefKey !== savedBrief));
	let questionsKey = $derived(JSON.stringify(questions));
	let questionsDirty = $derived(Boolean(selected && questionDraftId === selected.id && questionsKey !== savedQuestions));
	let definitionsKey = $derived(JSON.stringify(definitions));
	let definitionsDirty = $derived(Boolean(selected && definitionDraftId === selected.id && definitionsKey !== savedDefinitions));
	let observedEditableDraftKey = '';
	$effect(() => {
		const key = JSON.stringify([selected?.id ?? null, briefKey, questionsKey, definitionsKey]);
		if (key === observedEditableDraftKey) return;
		observedEditableDraftKey = key;
		if (error) error = '';
		if (autosaveError) autosaveError = '';
	});
	$effect(() => {
		const itemId = selected?.id;
		const hasEditableChanges = dirty
			|| (questionsDirty && questions.every((question) => question.text.trim()))
			|| (definitionsDirty && definitions.every((definition) => definition.text.trim()));
		if (!itemId || !hasEditableChanges || saving || autosaving || proposingQuestions || autosaveError) return;
		clearTimeout(autosaveTimer);
		autosaveTimer = setTimeout(() => void saveEditableDrafts(itemId), 700);
		return () => clearTimeout(autosaveTimer);
	});

	onDestroy(() => {
		rememberView();
		clearTimeout(autosaveTimer);
		if (selected && (dirty || questionsDirty || definitionsDirty)) {
			void saveEditableDrafts(selected.id);
		}
	});
	let coverage = $derived.by(() => {
		const activeQuestions = (selected?.questions ?? []).filter((question) => question.status === 'active');
		const answered = activeQuestions.filter((question) => question.answer_status === 'answered').length;
		const partial = activeQuestions.filter((question) => question.answer_status === 'incomplete').length;
		const dataGaps = activeQuestions.filter((question) =>
			question.answer_status === 'unsupported' || question.answer_status === 'insufficient_data'
		).length;
		const needsAttention = activeQuestions.length - answered - partial - dataGaps;
		const documentedGaps = activeQuestions.filter((question) =>
			(['incomplete', 'unsupported', 'insufficient_data'].includes(question.answer_status))
			&& Boolean(question.answer_status_reason?.trim())
		);
		return {
			activeQuestions,
			answered,
			partial,
			dataGaps,
			needsAttention,
			documentedGaps,
			needsReview: activeQuestions.filter((question) => question.answer_status !== 'answered'),
			canComplete: activeQuestions.length > 0
				&& answered + documentedGaps.length === activeQuestions.length
				&& !dirty
				&& !questionsDirty
				&& !definitionsDirty
				&& !saving
				&& !autosaving
		};
	});
	let hasBlankQuestion = $derived(questions.some((question) => !question.text.trim()));
	let hasBlankDefinition = $derived(definitions.some((definition) => !definition.text.trim()));
	let canSuggestBreakdown = $derived(Boolean(rootQuestion?.text.trim()) && subquestions.length === 0);
	let reportExportDisabledReason = $derived.by(() => {
		if (!selected) return 'Select an Investigation before exporting.';
		if (dirty || questionsDirty || definitionsDirty || saving || autosaving) return 'Wait for the Investigation changes to save before exporting.';
		if (selected.questions?.some((question) => question.status === 'active' && question.answer_status === 'running')) {
			return 'Wait for active analysis runs to finish before exporting.';
		}
		return '';
	});

	async function loadWorkspace(path: string, preserveDrafts = false): Promise<void> {
		const serial = ++requestSerial;
		const draft = preserveDrafts ? selected : null;
		if (draft && (dirty || questionsDirty || definitionsDirty)) {
			clearTimeout(autosaveTimer);
			await saveEditableDrafts(draft.id);
			if (serial !== requestSerial) return;
			if (selected?.id === draft.id && (dirty || questionsDirty || definitionsDirty)) return;
		}
		const visibleSelection = selected?.workspace_id === path;
		loading = !visibleSelection;
		error = '';
		investigationLoadError = '';
		conversationError = '';
		if (!visibleSelection) {
			investigations = [];
			selectedId = null;
			analysisTurns = {};
			openAnalysisTurnId = null;
			questionLinksDraft = [];
		}
		try {
			const all = await ipc.investigationsList(session.conversations.map((conversation) => conversation.id));
			if (serial !== requestSerial) return;
			investigations = all.filter((item) => item.workspace_id === path);
			const activeConversation = session.activeChat;
			const activeQuestion = firstActualQuestion(activeConversation.messages);
			const navigationTarget = session.investigationNavigationTarget;
			const navigationInvestigation = navigationTarget?.workspaceId === path
				? investigations.find((item) => item.conversation_id === navigationTarget.conversationId)
					?? investigations.find((item) => (item.analysis_turn_ids ?? []).includes(navigationTarget.turnId)
						|| (item.questions ?? []).some((question) => question.analysis_turn_ids?.includes(navigationTarget.turnId)))
				: null;
			// The active Investigation is persisted on its workspace pane. On
			// restore, chat may briefly be a fresh placeholder while its canonical
			// conversation is being loaded, so the saved selection must recover it.
			const explicitlyActive = investigations.find((item) => item.id === session.activeInvestigationId);
			const linkedToConversation = investigations.find((item) =>
				item.conversation_id === activeConversation.id ||
				(!!activeQuestion && item.brief_source_message_id === activeQuestion.id)
			);
			const seededInvestigation = initialRecord?.workspace_id === path
				? investigations.find((item) => item.id === initialRecord.id)
				: null;
			const conversationInvestigation = navigationInvestigation ?? explicitlyActive ?? seededInvestigation ?? linkedToConversation;
			selectedId = conversationInvestigation?.id ?? null;
			const selectedInvestigation = investigations.find((item) => item.id === selectedId);
			if (isDesktop() && conversationInvestigation && selectedInvestigation && (selectedInvestigation.conversation_id || selectedInvestigation.brief_source_message_id)) {
				try {
					const linked = await focusInvestigationConversation(
						selectedInvestigation,
						'preserve',
						undefined,
						() => serial === requestSerial && path === session.catalog.workspace && selectedId === selectedInvestigation.id
					);
					if (serial !== requestSerial) return;
					if (linked) {
						selectedId = linked.id;
						investigations = investigations.map((record) => record.id === linked.id ? linked : record);
					}
				} catch (cause) {
					if (serial === requestSerial) conversationError = cause instanceof Error ? cause.message : String(cause);
				}
			}
			if (serial === requestSerial) session.setActiveInvestigation(conversationInvestigation ? selectedId : null);
		} catch (cause) {
			if (serial === requestSerial) investigationLoadError = cause instanceof Error ? cause.message : String(cause);
		} finally {
			if (serial === requestSerial) loading = false;
		}
	}

	async function refreshWorkspace(path: string): Promise<void> {
		if (path !== session.catalog.workspace) return;
		if (quietRefreshInProgress) {
			quietRefreshPending = true;
			return;
		}
		if (loading || dirty || questionsDirty || definitionsDirty || saving || autosaving || proposingQuestions) {
			quietRefreshPending = true;
			return;
		}
		quietRefreshInProgress = true;
		const serial = ++requestSerial;
		const conversationId = session.activeChat.id;
		const previousSelectedId = selectedId;
		try {
			const records = await ipc.investigationsList(session.conversations.map((conversation) => conversation.id));
			if (serial !== requestSerial || path !== session.catalog.workspace || conversationId !== session.activeChat.id) return;
			const refreshed = records.filter((item) => item.workspace_id === path);
			const activeConversation = session.activeChat;
			const activeQuestion = firstActualQuestion(activeConversation.messages);
			const retainedSelection = refreshed.find((item) => item.id === previousSelectedId);
			const activeSelection = refreshed.find((item) => item.id === session.activeInvestigationId);
			const linkedToConversation = refreshed.find((item) =>
				item.conversation_id === activeConversation.id ||
				(!!activeQuestion && item.brief_source_message_id === activeQuestion.id)
			);
			const nextSelection = retainedSelection ?? activeSelection ?? linkedToConversation ?? null;
			investigations = refreshed;
			selectedId = nextSelection?.id ?? null;
			if (session.activeInvestigationId !== selectedId) session.setActiveInvestigation(selectedId);
			investigationLoadError = '';
		} catch (cause) {
			if (serial === requestSerial && investigations.length === 0) {
				investigationLoadError = cause instanceof Error ? cause.message : String(cause);
			}
		} finally {
			quietRefreshInProgress = false;
			if (quietRefreshPending) {
				quietRefreshPending = false;
				void refreshWorkspace(path);
			}
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
		const conversationId = session.activeChat.id;
		const paneVisible = session.workspacePaneOpen && session.workspacePane === 'investigation';
		const enteredPane = paneVisible && !investigationPaneWasVisible;
		investigationPaneWasVisible = paneVisible;
		const sameWorkspace = path === loadedWorkspace;
		const conversationChanged = conversationId !== loadedConversationId;
		const versionChanged = version !== loadedInvestigationsVersion;
		if (path !== loadedWorkspace && selected && (dirty || questionsDirty || definitionsDirty)) {
			void saveEditableDrafts(selected.id);
		}
		workspacePath = path;
		if (!path) {
			requestSerial += 1;
			loadedWorkspace = null;
			loadedConversationId = null;
			loadedInvestigationsVersion = version;
			quietRefreshPending = false;
			investigations = [];
			selectedId = null;
			investigationSection = 'brief';
			loading = false;
			return;
		}
		if (sameWorkspace && !conversationChanged && !versionChanged && !enteredPane) return;
		if (sameWorkspace && !conversationChanged) {
			loadedInvestigationsVersion = version;
			quietRefreshPending = true;
			return;
		}
		loadedWorkspace = path;
		loadedConversationId = conversationId;
		loadedInvestigationsVersion = version;
		void loadWorkspace(path, sameWorkspace);
	});

	$effect(() => {
		if (!quietRefreshPending) return;
		const path = workspacePath;
		const blocked = loading || dirty || questionsDirty || definitionsDirty || saving || autosaving || proposingQuestions;
		if (!path || blocked || quietRefreshInProgress) return;
		quietRefreshPending = false;
		void refreshWorkspace(path);
	});

	$effect(() => {
		const item = selected;
		const missing = unlinkedAnalysisTurnIds;
		if (item && missing.length && isDesktop()) void syncAnalysisTurns(item.id);
	});

	$effect(() => {
		const target = session.investigationNavigationTarget;
		const item = selected;
		if (!target || !item || loading || target.workspaceId !== item.workspace_id) return;
		if (item.conversation_id !== target.conversationId
			&& !(item.analysis_turn_ids ?? []).includes(target.turnId)
			&& !(item.questions ?? []).some((question) => question.analysis_turn_ids?.includes(target.turnId))) return;
		session.clearInvestigationNavigationTarget(target);
		void openAnalysisRun(target.turnId);
	});

	$effect(() => {
		const sourcePath = session.selectedSourcePath;
		if (!sourcePath || !workspacePath) return;
		if (!session.catalog.sources.some((source) => source.path === sourcePath)) return;
		investigationSection = 'data';
	});

	$effect(() => {
		const item = selected;
		const key = item ? `${item.id}:${JSON.stringify(item.brief)}` : '';
		if (key === loadedBriefKey) return;
		loadedBriefKey = key;
		if (autosaving && item?.id === selectedId) return;
		briefEditing = false;
		if (!item) {
			goal = '';
			audience = '';
			desiredOutput = '';
			constraintsText = '';
			savedBrief = '';
			briefDraftId = null;
			return;
		}
		goal = item.brief.goal;
		audience = item.brief.audience;
		desiredOutput = item.brief.desired_output;
		constraintsText = item.brief.constraints.join('\n');
		savedBrief = JSON.stringify(item.brief);
		briefDraftId = item.id;
	});

	$effect(() => {
		const item = selected;
		const key = item ? `${item.id}:${JSON.stringify(item.questions ?? null)}` : '';
		if (key === loadedQuestionsKey) return;
		loadedQuestionsKey = key;
		if (autosaving && item?.id === selectedId) return;
		if (!item) {
			questions = [];
			savedQuestions = '';
			questionDraftId = null;
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
		questionDraftId = item.id;
	});

	$effect(() => {
		const item = selected;
		const key = item ? `${item.id}:${JSON.stringify(item.definitions ?? null)}` : '';
		if (key === loadedDefinitionsKey) return;
		loadedDefinitionsKey = key;
		if (autosaving && item?.id === selectedId) return;
		definitions = (item?.definitions ?? []).map((definition) => ({
			id: definition.id,
			kind: definition.kind,
			text: definition.text
		}));
		savedDefinitions = JSON.stringify(definitions);
		definitionDraftId = item?.id ?? null;
	});

	$effect(() => {
		const proposal = pendingProposal;
		if ((proposal?.messageId ?? null) === proposalMessageId) return;
		proposalMessageId = proposal?.messageId ?? null;
		proposalText = proposal?.text ?? '';
		proposalTarget = proposal?.target ?? 'goal';
	});

	async function openSelectedConversation(
		focusTurnId?: string,
		toolsPane: 'closed' | 'open' | 'preserve' = 'preserve'
	): Promise<Investigation | null> {
		const item = selected;
		if (!item || openingConversation || !isDesktop()) return null;
		void saveEditableDrafts(item.id);
		const sourceConversationId = session.activeChat.id;
		openingConversation = true;
		conversationError = '';
		try {
			const linked = await focusInvestigationConversation(
				item,
				toolsPane,
				focusTurnId,
				() => selectedId === item.id &&
					session.catalog.workspace === item.workspace_id &&
					session.activeChat.id === sourceConversationId
			);
			if (linked) investigations = investigations.map((record) => record.id === linked.id ? linked : record);
			return linked;
		} catch (cause) {
			conversationError = cause instanceof Error ? cause.message : String(cause);
			return null;
		} finally {
			openingConversation = false;
		}
	}

	async function saveBrief(closeEditor = true): Promise<boolean> {
		const itemId = selected?.id;
		if (!itemId) return false;
		await waitForEditableAutosave();
		if (selected?.id !== itemId || saving) return false;
		if (!dirty) {
			if (closeEditor) briefEditing = false;
			return true;
		}
		saving = true;
		error = '';
		try {
			const updated = await ipc.investigationUpdateBrief(itemId, currentBrief);
			acceptInvestigationUpdate(updated);
			savedBrief = JSON.stringify(updated.brief);
			if (closeEditor) briefEditing = false;
			return true;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
			return false;
		} finally {
			saving = false;
		}
	}

	function discardBrief(): void {
		const item = selected;
		if (!item || saving) return;
		goal = item.brief.goal;
		audience = item.brief.audience;
		desiredOutput = item.brief.desired_output;
		constraintsText = item.brief.constraints.join('\n');
		savedBrief = JSON.stringify(item.brief);
		briefDraftId = item.id;
		briefEditing = false;
		error = '';
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
		session.rememberInvestigationNames([updated]);
		investigations = investigations.map((item) => item.id === updated.id ? updated : item);
		session.investigationsVersion += 1;
		session.workspaceInvestigationSummaryVersion += 1;
	}

	function relativeSourcePath(path: string, rootPath: string): string {
		const root = rootPath.replace(/[/\\]+$/, '');
		if (path === root) return path;
		if (path.startsWith(`${root}/`) || path.startsWith(`${root}\\`)) {
			return path.slice(root.length + 1).replace(/\\/g, '/');
		}
		return path.replace(/\\/g, '/');
	}

	function submitNewQuestion(event: SubmitEvent): void {
		event.preventDefault();
		const text = newQuestionText.trim();
		if (!text || saving || proposingQuestions) return;
		addQuestion();
		const added = questions.at(-1)!;
		questions = questions.map(question => question.id === added.id ? { ...question, text } : question);
		editingQuestionIds = editingQuestionIds.filter(id => id !== added.id);
		newQuestionText = '';
	}

	function questionExcerpt(questionId: string): string {
		const turnId = savedQuestion(questionId)?.latest_analysis_turn_id;
		const message = investigationConversationMessages.find(item => item.role === 'assistant' && item.answer?.turn_id === turnId);
		return (message?.answer?.text ?? '').split(/\n\s*\n/)[0]?.replace(/[#*_`]/g, '').slice(0, 160) ?? '';
	}

	function addQuestion(): void {
		const id = globalThis.crypto?.randomUUID?.().replaceAll('-', '') ?? `${Date.now()}${Math.random().toString(16).slice(2)}`;
		const question: InvestigationQuestionInput = {
			id: `q_${id}`,
			text: '',
			parent_id: rootQuestion?.id
		};
		questions = [...questions, question];
		editingQuestionIds = [...editingQuestionIds, question.id];
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
			insufficient_data: 'Insufficient data',
			needs_review: 'Needs review',
			waiting_for_input: 'Waiting for your input',
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
		if (status === 'incomplete' || status === 'unsupported' || status === 'insufficient_data' || status === 'outdated') return 'warning';
		return 'default';
	}

	function questionCoverageReason(question: InvestigationQuestion): string {
		return investigationQuestionStatusDetail(question);
	}

	async function focusQuestion(questionId: string): Promise<void> {
		if (!editingQuestionIds.includes(questionId)) editingQuestionIds = [...editingQuestionIds, questionId];
		investigationSection = 'questions';
		await tick();
		const field = document.getElementById(`question-${questionId}`);
		if (!(field instanceof HTMLElement)) return;
		field.focus({ preventScroll: true });

		// Keep the review jump inside this Investigation. scrollIntoView() may
		// scroll every ancestor, including the workspace/tool chrome around this
		// pane, which can leave the section tabs underneath those bars.
		const scroller = field.closest<HTMLElement>('.investigation-section-scroll');
		if (!scroller) return;
		const scrollport = scroller.getBoundingClientRect();
		const scrollportTop = scrollport.top + scroller.clientTop;
		const scrollportBottom = scrollportTop + scroller.clientHeight;
		const stickyTabs = scroller.querySelector<HTMLElement>('.investigation-nav');
		const visibleTop = Math.max(
			scrollportTop,
			stickyTabs?.getBoundingClientRect().bottom ?? scrollportTop
		);
		const visibleHeight = Math.max(0, scrollportBottom - visibleTop);
		const fieldRect = field.getBoundingClientRect();
		const centeredOffset = Math.max(0, (visibleHeight - fieldRect.height) / 2);
		const targetScrollTop = scroller.scrollTop + fieldRect.top - visibleTop - centeredOffset;
		scroller.scrollTo({ top: Math.max(0, targetScrollTop), behavior: 'smooth' });
	}

	function questionAnswerReason(questionId: string): string | undefined {
		const question = selected?.questions?.find((item) => item.id === questionId);
		if (!question || ['answered', 'not_started', 'running'].includes(question.answer_status)) return undefined;
		return investigationQuestionStatusDetail(question);
	}

	function questionRunIdsFor(question: InvestigationQuestion): string[] {
		return [...new Set(question.analysis_turn_ids ?? [])];
	}

	function questionForRun(investigation: Investigation, run: AnalysisTurn): InvestigationQuestion | undefined {
		const questionId = run.investigation_context?.question_id;
		return investigation.questions?.find((question) => question.id === questionId)
			?? investigation.questions?.find((question) => question.latest_analysis_turn_id === run.id || questionRunIdsFor(question).includes(run.id));
	}

	function runFreshness(question: InvestigationQuestion | undefined, runId: string): string {
		if (!question) return 'Question linkage unavailable; freshness could not be checked.';
		if (!question.latest_analysis_turn_id) return 'No latest Question run is recorded; freshness could not be checked.';
		if (question.latest_analysis_turn_id === runId) return 'Linked to the Question’s latest run.';
		return `Earlier than the Question’s latest run (${question.latest_analysis_turn_id}); review before treating as current.`;
	}

	function runRevision(run: AnalysisTurn | null | undefined): string {
		return run?.workspace_revision ?? run?.trace.workspace_revision ?? run?.workspace_snapshot?.revision ?? 'Not recorded for this run';
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

	function isHandoffEvidence(value: unknown): value is EvidenceItem {
		if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
		const item = value as Record<string, unknown>;
		return typeof item.tool === 'string'
			&& Boolean(item.args && typeof item.args === 'object' && !Array.isArray(item.args))
			&& typeof item.result_summary === 'string';
	}

	function handoffCharts(investigation: Investigation, runs: InvestigationReportRuns): HandoffChart[] {
		const runIds = new Set<string>();
		for (const question of investigation.questions ?? []) {
			if (question.status === 'active' && question.latest_analysis_turn_id) runIds.add(question.latest_analysis_turn_id);
		}
		for (const finding of investigation.findings ?? []) runIds.add(finding.analysis_turn_id);
		const charts: HandoffChart[] = [];
		for (const runId of runIds) {
			const run = runs[runId];
			if (!run) continue;
			const evidence = run.result.evidence.filter(isHandoffEvidence);
			for (const [index, item] of evidence.entries()) {
				if (!isRenderableChartEvidence(item, run.verification?.checks ?? run.result.verification)) continue;
				const sourceId = item.chart?.metadata?.source_evidence_id;
				charts.push({
					key: `${run.id}:${item.id ?? index}`,
					run,
					evidence: item,
					sourceEvidence: sourceId ? evidence.find((candidate) => candidate.id === sourceId) ?? null : null
				});
			}
		}
		return charts;
	}

	async function reviewInvestigationHandoff(): Promise<void> {
		const item = selected;
		if (!item || exportingReport || reportExportDisabledReason) return;
		// `selected` is a Svelte reactive proxy; serialize the canonical record
		// into an immutable plain-data snapshot before loading its linked runs.
		const snapshot = JSON.parse(JSON.stringify(item)) as Investigation;
		exportingReport = true;
		reportExportStatus = '';
		reportExportFailed = false;
		try {
			const runIds = new Set<string>();
			for (const question of snapshot.questions ?? []) {
				if (question.status === 'active' && question.latest_analysis_turn_id) runIds.add(question.latest_analysis_turn_id);
			}
			for (const finding of snapshot.findings ?? []) runIds.add(finding.analysis_turn_id);
			const loadedRuns = await Promise.all([...runIds].map(async (turnId) => {
				try {
					return [turnId, await ipc.analysisTurnLoad(turnId)] as const;
				} catch {
					return [turnId, null] as const;
				}
			}));
			const runs = Object.fromEntries(loadedRuns);
			if (selectedId !== snapshot.id) return;
			const report = buildInvestigationReport(snapshot, runs, new Date());
			reportPreview = {
				investigation: snapshot,
				runs,
				markdown: report,
				filename: investigationReportFilename(snapshot.title || snapshot.brief.goal),
				charts: handoffCharts(snapshot, runs)
			};
			reportPreviewOpen = true;
			reportExportStatus = 'Report preview ready.';
		} catch (cause) {
			reportExportFailed = true;
			reportExportStatus = cause instanceof Error ? cause.message : String(cause);
		} finally {
			exportingReport = false;
		}
	}

	function downloadInvestigationHandoff(): void {
		const preview = reportPreview;
		if (!preview || exportingReport) return;
		exportingReport = true;
		reportExportStatus = '';
		reportExportFailed = false;
		try {
			const url = URL.createObjectURL(new Blob([preview.markdown], { type: 'text/markdown;charset=utf-8' }));
			const link = document.createElement('a');
			link.href = url;
			link.download = preview.filename;
			document.body.append(link);
			link.click();
			link.remove();
			setTimeout(() => URL.revokeObjectURL(url), 1000);
			reportExportStatus = `Downloaded ${preview.filename}.`;
		} catch (cause) {
			reportExportFailed = true;
			reportExportStatus = cause instanceof Error ? cause.message : String(cause);
		} finally { exportingReport = false; }
	}

	async function openHandoffReference(turnId: string, evidenceId?: string): Promise<void> {
		reportPreviewOpen = false;
		await tick();
		await openRunFromResults(turnId, evidenceId);
	}

	function analysisRunOrdinal(turnId: string): number | undefined {
		const index = analysisRunIds().indexOf(turnId);
		return index < 0 ? undefined : index + 1;
	}

	function analysisRunStatusLine(turnId: string): string {
		const run = analysisTurns[turnId];
		const message = analysisRunMessage(turnId);
		const state = runStateLabel(run?.state ?? message?.run_state) ?? (run ? 'Unknown state' : 'Saved run');
		const parentId = run?.rerun_of ?? message?.rerun_of;
		const details = [state];
		const elapsedMs = run?.trace.elapsed_ms ?? message?.answer?.trace?.elapsed_ms ?? message?.run_duration_ms;
		if (typeof elapsedMs === 'number' && Number.isFinite(elapsedMs) && elapsedMs >= 0) {
			details.push(`${formatAnalysisRunDuration(elapsedMs)} total`);
		}
		if (typeof parentId === 'string' && parentId) {
			const parentOrdinal = analysisRunOrdinal(parentId);
			details.push(`rerun of ${parentOrdinal ? `Run ${parentOrdinal}` : 'saved run'}`);
		}
		return details.join(' · ');
	}

	function analysisRunMessage(turnId: string): Message | undefined {
		return investigationConversationMessages.find((message) => message.answer?.turn_id === turnId);
	}

	function analysisRunTimestamp(turnId: string): number | undefined {
		const timestamp = analysisRunMessage(turnId)?.ts;
		return typeof timestamp === 'number' && Number.isFinite(timestamp) ? timestamp : undefined;
	}

	function formatAnalysisRunTimestamp(timestamp: number): string {
		return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(timestamp);
	}

	function formatAnalysisRunDuration(ms: number): string {
		if (ms < 1000) return `${ms} ms`;
		return `${(ms / 1000).toFixed(ms < 10_000 ? 1 : 0)} s`;
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
		const count = new Set(selection.included_paths).size;
		return `${count} source${count === 1 ? '' : 's'} selected for this Question`;
	}

	async function loadOverviewRun(turnId: string): Promise<void> {
		const owner = selected?.id;
		overviewRunId = turnId;
		overviewError = '';
		if (analysisTurns[turnId]) return;
		overviewLoadingId = turnId;
		try {
			const run = await ipc.analysisTurnLoad(turnId);
			if (owner === selected?.id) analysisTurns = { ...analysisTurns, [turnId]:run };
		} catch (cause) {
			if (owner === selected?.id && overviewRunId === turnId) overviewError = cause instanceof Error ? cause.message : String(cause);
		} finally { if (overviewLoadingId === turnId) overviewLoadingId = null; }
	}
	$effect(() => {
		const item = selected, section = investigationSection;
		const ids = item ? analysisRunIds(item) : [];
		if (section !== 'analysis' || !ids.length) return;
		if (!overviewRunId || !ids.includes(overviewRunId)) void untrack(() => loadOverviewRun(ids.at(-1)!));
		else if (!analysisTurns[overviewRunId] && overviewLoadingId !== overviewRunId && !overviewError) void untrack(() => loadOverviewRun(overviewRunId!));
	});

	async function openAnalysisRun(turnId: string, navigate = true): Promise<void> {
		if (navigate) investigationSection = 'analysis';
		historyOpen = true;
		openAnalysisTurnId = turnId;
		analysisTurnError = '';
		questionLinksError = ''; questionLinksStatus = '';
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
		if (!navigate) return;
		await tick();
		const detail = document.getElementById('saved-analysis-run-detail');
		detail?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
		detail?.focus({ preventScroll: true });
	}

	async function openRunFromResults(turnId: string, evidenceId?: string): Promise<void> {
		await openAnalysisRun(turnId);
		if (!evidenceId) return;
		await tick();
		const body = document.getElementById(`investigation-run-evidence-${turnId}`);
		const target = [...(body?.querySelectorAll<HTMLElement>('[data-evidence-id]') ?? [])]
			.find((item) => item.dataset.evidenceId === evidenceId);
		target?.scrollIntoView({ behavior: 'smooth', block: 'center' });
		target?.focus({ preventScroll: true });
	}

	async function openLegacyRunFromResults(turnId?: string): Promise<void> {
		await openSelectedConversation(turnId);
	}

	async function reviewQuestionFromResults(
		questionId: string,
		freshness?: AnalysisTurnReplayStatus
	): Promise<void> {
		staleResultReview = freshness?.same_workspace && freshness.revision_changed
			? { questionId, status: freshness }
			: null;
		if (staleResultReview) pendingAnalysisQuestionId = questionId;
		await focusQuestion(questionId);
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
		findingDraftTouched = false;
		findingError = '';
	}

	function findingNotesFromDraft(draft: string): string[] {
		return draft.split(/\r?\n/).map((note) => note.trim()).filter(Boolean);
	}

	function selectFindingRevisionTarget(findingId: string): void {
		findingDraftTouched = true;
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
		findingDraftTouched = true;
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
			findingDraftTouched = false;
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
		questionLinksError = ''; questionLinksStatus = '';
		try {
			const updated = await ipc.investigationSetAnalysisTurnQuestions(
				selected.id,
				openAnalysisTurnId,
				questionLinksDraft
			);
			acceptInvestigationUpdate(updated);
			questionLinksDraft = questionIdsForRun(openAnalysisTurnId, updated);
			questionLinksStatus = 'Question links saved. Answer coverage is unchanged.';
		} catch (cause) {
			questionLinksError = cause instanceof Error ? cause.message : String(cause);
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
		staleResultReview = null;
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
			const linkedInvestigation = await openSelectedConversation(undefined, 'open');
			if (!linkedInvestigation) return;
			pendingAnalysisQuestionId = null;
			await analyzeInvestigationQuestion(linkedInvestigation, questionId);
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
		const itemId = selected?.id;
		if (!itemId) return false;
		await waitForEditableAutosave();
		if (selected?.id !== itemId) return false;
		if (saving || !questionsDirty) return !questionsDirty;
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
			acceptInvestigationUpdate(updated);
			questions = (updated.questions ?? []).map((question) => ({
				id: question.id,
				text: question.text,
				parent_id: question.parent_id,
				source_selection: question.source_selection
			}));
			savedQuestions = JSON.stringify(questions);
			questionDraftId = updated.id;
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

	function startDefinition(): void {
		investigationSection = 'questions';
		definitionsOpen = true;
		addDefinition();
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
		investigationSection = 'questions';
		const id = globalThis.crypto?.randomUUID?.().replaceAll('-', '') ?? `${Date.now()}${Math.random().toString(16).slice(2)}`;
		const definitionId = `d_${id}`;
		definitionsOpen = true;
		definitions = [...definitions, { id: definitionId, kind: 'assumption', text: normalized }];
		await tick();
		const field = document.getElementById(`definition-text-${definitionId}`);
		field?.scrollIntoView({ behavior: 'smooth', block: 'center' });
		field?.focus({ preventScroll: true });
	}

	function removeDefinition(id: string): void {
		definitions = definitions.filter((definition) => definition.id !== id);
	}

	async function saveDefinitions(): Promise<boolean> {
		const itemId = selected?.id;
		if (!itemId) return false;
		await waitForEditableAutosave();
		if (selected?.id !== itemId) return false;
		if (saving || !definitionsDirty) return !definitionsDirty;
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
			acceptInvestigationUpdate(updated);
			definitions = (updated.definitions ?? []).map((definition) => ({
				id: definition.id,
				kind: definition.kind,
				text: definition.text
			}));
			savedDefinitions = JSON.stringify(definitions);
			definitionDraftId = updated.id;
			return true;
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
			return false;
		} finally {
			saving = false;
		}
	}

	async function saveEditableDrafts(investigationId: string): Promise<void> {
		if (autosaveInProgress) {
			const pending = autosaveInProgress;
			await pending;
			if (!autosaveError && selected?.id === investigationId && (dirty || questionsDirty || definitionsDirty)) {
				return saveEditableDrafts(investigationId);
			}
			return;
		}

		const item = investigations.find((record) => record.id === investigationId);
		if (!item || selected?.id !== investigationId) return;
		const brief = { ...currentBrief, constraints: [...currentBrief.constraints] };
		const briefChanged = briefDraftId === investigationId && JSON.stringify(brief) !== savedBrief;
		const questionSnapshot = questions.map((question) => ({ ...question }));
		const questionsChanged = questionDraftId === investigationId && JSON.stringify(questionSnapshot) !== savedQuestions;
		const definitionSnapshot = definitions.map((definition) => ({ ...definition }));
		const definitionsChanged = definitionDraftId === investigationId && JSON.stringify(definitionSnapshot) !== savedDefinitions;
		const saveQuestionsChanged = questionsChanged && questionSnapshot.every((question) => question.text.trim());
		const saveDefinitionsChanged = definitionsChanged && definitionSnapshot.every((definition) => definition.text.trim());
		if (!briefChanged && !saveQuestionsChanged && !saveDefinitionsChanged) return;

		const save = async () => {
			while (saving || proposingQuestions) {
				await new Promise<void>((resolve) => setTimeout(resolve, 16));
			}
			autosaving = true;
			autosaveError = '';
			try {
				if (briefChanged) {
					try {
						const updated = await ipc.investigationUpdateBrief(investigationId, brief);
						if (selected?.id === investigationId) {
							loadedBriefKey = `${investigationId}:${JSON.stringify(updated.brief)}`;
							savedBrief = JSON.stringify(brief);
						}
						acceptInvestigationUpdate(updated);
					} catch (cause) {
						if (selected?.id === investigationId) autosaveError = cause instanceof Error ? cause.message : String(cause);
						return;
					}
				}
				if (saveQuestionsChanged) {
					try {
						const updated = await ipc.investigationUpdateQuestions(investigationId, questionSnapshot.map((question) => ({
							id: question.id,
							parent_id: question.parent_id,
							text: question.text.trim(),
							source_selection: question.source_selection
						})));
						if (selected?.id === investigationId) {
							loadedQuestionsKey = `${investigationId}:${JSON.stringify(updated.questions ?? null)}`;
							savedQuestions = JSON.stringify(questionSnapshot);
						}
						acceptInvestigationUpdate(updated);
					} catch (cause) {
						if (selected?.id === investigationId) autosaveError = cause instanceof Error ? cause.message : String(cause);
						return;
					}
				}
				if (saveDefinitionsChanged) {
					try {
						const updated = await ipc.investigationUpdateDefinitions(investigationId, definitionSnapshot.map((definition) => ({
							id: definition.id,
							kind: definition.kind,
							text: definition.text.trim()
						})));
						if (selected?.id === investigationId) {
							loadedDefinitionsKey = `${investigationId}:${JSON.stringify(updated.definitions ?? null)}`;
							savedDefinitions = JSON.stringify(definitionSnapshot);
						}
						acceptInvestigationUpdate(updated);
					} catch (cause) {
						if (selected?.id === investigationId) autosaveError = cause instanceof Error ? cause.message : String(cause);
					}
				}
			} finally {
				autosaving = false;
			}
		};

		autosaveInProgress = save();
		try {
			await autosaveInProgress;
		} finally {
			autosaveInProgress = null;
		}
	}

	async function waitForEditableAutosave(): Promise<void> {
		clearTimeout(autosaveTimer);
		const pending = autosaveInProgress;
		if (pending) await pending;
		clearTimeout(autosaveTimer);
	}

	function retryEditableAutosave(): void {
		const itemId = selected?.id;
		if (!itemId || autosaving) return;
		autosaveError = '';
		clearTimeout(autosaveTimer);
		void saveEditableDrafts(itemId);
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
			acceptInvestigationUpdate(updated);
			if (accept) savedBrief = JSON.stringify(updated.brief);
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
		} finally {
			saving = false;
		}
	}

	function handleInvestigationTabsKeydown(event: KeyboardEvent): void {
		if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
		const tabList = event.currentTarget;
		if (!(tabList instanceof HTMLElement)) return;
		const tabs = Array.from(tabList.querySelectorAll<HTMLButtonElement>('[role="tab"]'));
		const currentIndex = tabs.findIndex((tab) => tab === document.activeElement);
		if (currentIndex < 0) return;
		event.preventDefault();
		const nextIndex = event.key === 'Home'
			? 0
			: event.key === 'End'
				? tabs.length - 1
				: (currentIndex + (event.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length;
		const nextTab = tabs[nextIndex];
		const nextSection = investigationSections[nextIndex];
		if (!nextTab || !nextSection) return;
		investigationSection = nextSection.id;
		nextTab.focus();
	}
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<section class="investigation-page" aria-labelledby="investigation-title">
	<header class="page-head">
		{#if selected}
			<div class="notebook-panel-bar">
				<Button type="button" variant="ghost" class="notebook-back" onclick={() => onexpand?.()}><Icon name="chevron-right" size={14} />{expanded ? 'Chat' : 'Notebook'}</Button>
				<div class="notebook-panel-actions">
					<Button id="investigation-share-action" type="button" variant="ghost" class="notebook-share" onclick={() => (investigationSection = 'share')}><Icon name="globe" size={14} />Share</Button>
				<Button type="button" variant="ghost" size="icon" class="notebook-close" aria-label="Close Investigation panel" title="Close panel" onclick={() => onclose?.()}><Icon name="x" size={14} /></Button>
				</div>
			</div>
			<div class="investigation-identity">
				<span class="investigation-mark"><Icon name="notebook-pen" size={20} /></span>
				<div class="investigation-title-row">
					<h1 id="investigation-title">{selected.title || 'Untitled Investigation'}</h1>
				<Button variant="ghost" size="icon" aria-label="Rename Investigation" title="Rename Investigation" onclick={() => { nameDraft = selected!.title; nameError = ''; nameOpen = true; }}><Icon name="pencil" size={14} /></Button>
				</div>
				<p>Investigation</p>
			</div>
		{:else}
			<div class="investigation-title-row"><h1 id="investigation-title">Investigation</h1><p>One request, the data behind it, and the results Fella produces.</p></div>
		{/if}
	</header>
	{#if workspacePath && !loading && !investigationLoadError && !selected}
		<section class="investigation-document-section investigation-data-section" aria-labelledby="investigation-data-empty-heading">
			<header class="investigation-document-heading">
				<div><h2 id="investigation-data-empty-heading">Workspace data</h2><p>Files and fields available in this workspace.</p></div>
			</header>
			<InvestigationSourceReadiness
				{workspacePath}
				investigation={selected}
				onUpdated={acceptInvestigationUpdate}
			/>
		</section>
	{/if}

	{#if !workspacePath}
		<div class="empty-state">
			<Icon name="folder" size={20} />
			<h2>Open a workspace to start an Investigation</h2>
			<p>Investigations are scoped to a folder so their intent stays alongside the data being analyzed.</p>
		</div>
	{:else if loading}
		<p class="status" role="status">Loading investigations…</p>
	{:else if investigationLoadError}
		<div class="empty-state" role="alert">
			<Icon name="alert" size={20} />
			<h2>Couldn't load investigations</h2>
			<p>{investigationLoadError}</p>
			<Button type="button" onclick={() => workspacePath && void loadWorkspace(workspacePath)}>Retry</Button>
		</div>
	{:else if !investigations.length}
		<div class="empty-state">
			<Icon name="bookmark" size={20} />
			<h2>No investigations yet</h2>
			<p>Choose “New Investigation” from the workspace menu above to start one. Each Investigation opens its own conversation.</p>
		</div>
	{:else}
		{#if selected}
			<div class="investigation-content">
				<div class="brief-editor">
					{#if conversationError}
						<p class="analysis-run-error" role="alert">{conversationError}</p>
					{/if}
					<div class="investigation-nav" role="tablist" aria-label="Investigation sections" tabindex="-1" onkeydown={handleInvestigationTabsKeydown}>
						{#each investigationSections as section (section.id)}
							<button
								class="fella-pill-nav-item"
								id={`investigation-tab-${section.id}`}
								type="button"
								role="tab"
								aria-selected={investigationSection === section.id}
								aria-controls={`investigation-panel-${section.id}`}
								aria-label={`${section.label}${section.id === 'questions' && needsQuestionAttention ? ', has unanswered questions' : ''}${section.id === 'results' && needsResultsReview ? ', results need review' : ''}`}
								tabindex={investigationSection === section.id ? 0 : -1}
								onclick={() => (investigationSection = section.id)}
							>
								{section.label}
								{#if section.id === 'questions' && needsQuestionAttention}<i class="investigation-tab-indicator question-indicator" aria-hidden="true"></i>{/if}
								{#if section.id === 'results' && needsResultsReview}<i class="investigation-tab-indicator results-indicator" aria-hidden="true"></i>{/if}
							</button>
						{/each}
					</div>
					<div class="investigation-section-scroll" bind:this={scroller} role="region" aria-label="Investigation content" tabindex="0" onkeydown={handleScrollRegionKeydown} onscroll={rememberScroll}>
					<div
						id="investigation-panel-brief"
						class="investigation-tab-panel"
						role="tabpanel"
						aria-labelledby="investigation-tab-brief"
						tabindex="0"
						hidden={investigationSection !== 'brief'}
					>
					<section class="brief-summary" aria-labelledby="brief-summary-title">
						<header class="brief-summary-header">
							<h3 id="brief-summary-title">Objective</h3>
							<Button type="button" variant="outline" disabled={saving} onclick={() => (briefEditing = true)}>
								<Icon name="pencil" size={11} />Edit
							</Button>
						</header>
						<p class="brief-objective">{goal.trim() || rootQuestion?.text.trim() || 'Not set'}</p>
						{#if audience.trim() || desiredOutput.trim() || constraintsText.trim()}
							<section class="brief-scope" aria-label="Investigation scope">
								<h4>Scope</h4>
								<dl>
									{#if audience.trim()}<div><dt>Audience</dt><dd>{audience}</dd></div>{/if}
									{#if desiredOutput.trim()}<div><dt>Deliverable</dt><dd>{desiredOutput}</dd></div>{/if}
									{#if constraintsText.trim()}<div><dt>Constraints</dt><dd>{constraintsText.split('\n').map((line) => line.trim()).filter(Boolean).join(' · ')}</dd></div>{/if}
								</dl>
							</section>
						{/if}
						{#if selected.brief_source_message_id}
							<details class="brief-source-note"><summary>Original request</summary><p>Kept in the conversation. Editing this brief does not change it.</p></details>
						{/if}
					</section>
					{#if pendingProposal}
						<section class="brief-proposal" aria-labelledby="brief-proposal-title">
							<div class="proposal-copy">
								<h3 id="brief-proposal-title">Possible change to the brief</h3>
								<p>A message uses explicit revision or source-scope language. Review it before updating the brief.</p>
							</div>
							<label for="proposal-target">Apply as</label>
							<Select
								id="proposal-target"
								value={proposalTarget}
								options={[
									{ value: 'goal', label: 'New goal' },
									{ value: 'desired_output', label: 'Desired output' },
									{ value: 'constraint', label: 'Additional constraint' }
								]}
								disabled={saving}
								onValueChange={(value) => (proposalTarget = value as BriefProposalTarget)}
							/>
							<Textarea id="proposal-text" class="brief-field proposal-field" bind:value={proposalText} disabled={saving} aria-label="Proposed brief update" />
							<div class="proposal-actions">
								<Button type="button" disabled={saving || !proposalText.trim()} onclick={() => void reviewProposal(true)}>
									<Icon name="check" size={16} />{saving ? 'Saving…' : 'Apply update'}
								</Button>
								<Button type="button" variant="ghost" disabled={saving} onclick={() => void reviewProposal(false)}>Dismiss</Button>
							</div>
						</section>
					{/if}
					{#if briefEditing}
						<section class="brief-edit-panel" aria-label="Edit Investigation brief">
					<label for="investigation-goal">Goal</label>
					<Textarea id="investigation-goal" class="brief-field goal-field" bind:value={goal} disabled={saving} placeholder="What question should this Investigation answer?" />
					<label for="investigation-audience">Audience</label>
					<Textarea id="investigation-audience" class="brief-field audience-field" bind:value={audience} disabled={saving} placeholder="Who needs this analysis?" />
					<label for="investigation-output">Desired output</label>
					<Textarea id="investigation-output" class="brief-field" bind:value={desiredOutput} disabled={saving} placeholder="What should the analyst deliver?" />
					<label for="investigation-constraints">Constraints</label>
					<Textarea id="investigation-constraints" class="brief-field constraints-field" bind:value={constraintsText} disabled={saving} placeholder="One constraint per line" />
						<div class="brief-edit-actions">
							<span role={error || autosaveError ? 'alert' : 'status'}>{error || autosaveError || (saving || autosaving ? 'Saving…' : dirty ? 'Changes save automatically' : 'Brief saved')}</span>
							<Button type="button" variant="ghost" disabled={saving} onclick={discardBrief}>{dirty ? 'Discard changes' : 'Done'}</Button>
							<Button type="button" disabled={!dirty || saving} onclick={() => void saveBrief()}>
								<Icon name="check" size={16} />{saving ? 'Saving…' : 'Save brief'}
							</Button>
						</div>
						</section>
					{/if}
					</div>
					<div
						id="investigation-panel-data"
						class="investigation-tab-panel"
						role="tabpanel"
						aria-labelledby="investigation-tab-data"
						tabindex="0"
						hidden={investigationSection !== 'data'}
					>
					<section class="investigation-document-section investigation-data-section" aria-labelledby="investigation-data-heading">
						<h3 id="investigation-data-heading" class="sr-only">Investigation sources</h3>
						<InvestigationSourceReadiness
							{workspacePath}
							investigation={selected}
							onUpdated={acceptInvestigationUpdate}
						/>
					</section>
					</div>
					<div
						id="investigation-panel-questions"
						class="investigation-tab-panel"
						role="tabpanel"
						aria-labelledby="investigation-tab-questions"
						tabindex="0"
						hidden={investigationSection !== 'questions'}
					>
					<section class="analyst-work-plan" aria-labelledby="question-plan-title">
					<section class="question-editor" aria-label="Investigation questions">
						<form class="question-add" onsubmit={submitNewQuestion}>
							<Input aria-label="New investigation question" bind:value={newQuestionText} placeholder="Add a question to this investigation…" disabled={saving || proposingQuestions} />
							<Button size="icon" type="submit" aria-label="Add question" disabled={!newQuestionText.trim() || saving || proposingQuestions}><Icon name="arrow-up" size={13} /></Button>
						</form>
						<header class="question-heading question-plan-heading">
							<div>
								<h3 id="question-plan-title">Questions</h3>
							</div>
							<div class="question-save">
								<span role={error || autosaveError ? 'alert' : 'status'} title={error || autosaveError || undefined}>
									{error || autosaveError || ((saving || autosaving) && (dirty || questionsDirty || definitionsDirty) ? 'Saving…' : hasBlankQuestion || hasBlankDefinition ? 'Complete blank fields' : dirty || questionsDirty || definitionsDirty ? 'Unsaved changes' : 'Saved')}
								</span>
							</div>
						</header>

						{#if coverage.activeQuestions.length}
						<details class="coverage-disclosure">
							<summary>{coverage.answered} of {coverage.activeQuestions.length} answered{#if coverage.needsReview.length} · {coverage.needsReview.length} need attention{/if}</summary>
						<section class="question-coverage" aria-label="Question coverage" data-investigation-status={selected.status}>
							<header>
								<div>
									<h4>Coverage</h4>
								</div>
								<dl class="question-coverage-counts">
									<div data-coverage="answered"><dt>Answered</dt><dd>{coverage.answered}</dd></div>
									<div data-coverage="partial"><dt>Partial</dt><dd>{coverage.partial}</dd></div>
									<div data-coverage="unanswered"><dt>Needs attention</dt><dd>{coverage.needsAttention}</dd></div>
									<div data-coverage="unsupported"><dt>Data gaps</dt><dd>{coverage.dataGaps}</dd></div>
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
								<p class="question-coverage-message">Add a main Question to see coverage.</p>
							{:else if coverage.needsReview.length}
								{#if coverage.canComplete && coverage.documentedGaps.length}
									<p class="question-coverage-message" role="status">Ready to close with {coverage.documentedGaps.length} documented gap{coverage.documentedGaps.length === 1 ? '' : 's'}; this does not mean every Question was fully answered.</p>
								{/if}
								<details class="question-coverage-review" open>
									<summary>{coverage.needsReview.length} Question{coverage.needsReview.length === 1 ? '' : 's'} need attention</summary>
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
								<p class="question-coverage-message">Wait for your changes to save before completing this Investigation.</p>
							{:else}
								<p class="question-coverage-message" role="status">All active Questions have complete, current answers and are ready to close.</p>
							{/if}
						</section>
						</details>
						{/if}
						{#if questions.length}
							{#if rootQuestion}
								<details class="question-item root-question-item" open={editingQuestionIds.includes(rootQuestion.id) || !rootQuestion.text.trim()}>
									<summary class="question-row-summary"><i class="question-status-mark" data-status={questionAnswerStatus(rootQuestion.id)}></i><span class="question-summary-copy"><span><small>Q1</small>{rootQuestion.text || 'Main question'}</span>{#if questionExcerpt(rootQuestion.id)}<span class="question-excerpt">{questionExcerpt(rootQuestion.id)}</span>{/if}<small class="question-summary-status">{questionAnswerStatusLabel(questionAnswerStatus(rootQuestion.id))}</small></span><Icon name="chevron-right" size={12} /></summary>
									<div class="question-expanded">
									<div class="question-item-heading">
										<label for="question-{rootQuestion.id}">Main question</label>
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
									{#if staleResultReview?.questionId === rootQuestion.id}
										<div class="question-stale-review" role="status">
											<p>This saved answer used catalog {staleResultReview.status.original_revision ?? 'not recorded'}; the current catalog is {staleResultReview.status.current_revision ?? 'different'}. The saved answer and run are unchanged.</p>
											<details>
												<summary>Source scope used by the saved run</summary>
												{#if staleResultReview.status.source_scope?.used_sources.length}
													<ul>{#each staleResultReview.status.source_scope.used_sources as source (`${source.source}-${source.table}`)}<li>{source.table ? `${source.source} · ${source.table}` : source.source}</li>{/each}</ul>
												{:else}
													<p>The saved run did not record its source list.</p>
												{/if}
										</details>
											<p>Review the included files in the run preview before choosing to run this Question again.</p>
										</div>
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
									{#if editingQuestionIds.includes(rootQuestion.id) || !rootQuestion.text.trim()}
									<Textarea
										id="question-{rootQuestion.id}"
										class="question-field"
										bind:value={rootQuestion.text}
										disabled={saving || proposingQuestions}
										aria-label="Main question"
									/>
									<Button variant="ghost" size="sm" disabled={!rootQuestion.text.trim()} onclick={() => (editingQuestionIds = editingQuestionIds.filter((id) => id !== rootQuestion.id))}>Done editing</Button>
									{:else}
									<button class="question-text" id="question-{rootQuestion.id}" type="button" aria-label="Edit main question" onclick={() => void focusQuestion(rootQuestion.id)}>{rootQuestion.text}<span>Edit</span></button>
									{/if}
									{#if !savedQuestion(rootQuestion.id)?.latest_analysis_turn_id && analysisRunIds(selected).length}
									<Button variant="link" size="sm" onclick={() => void openAnalysisRun(analysisRunIds(selected).at(-1)!)}>Review existing analysis</Button>
									{/if}
									<InvestigationQuestionSourceScope
										workspacePath={selected.workspace_id}
										investigation={selected}
										questionId={rootQuestion.id}
										selection={rootQuestion.source_selection}
										disabled={saving || proposingQuestions}
										onChange={(selection) => updateQuestionSourceSelection(rootQuestion.id, selection)}
									/>
								</div>
								</details>
							{/if}
							{#if subquestions.length}
											<h4 class="subquestion-title sr-only">Subquestions</h4>
								<ol class="question-list">
									{#each subquestions as question, index (question.id)}
										<li class="question-row">
										<details class="question-item" open={editingQuestionIds.includes(question.id) || !question.text.trim()}>
											<summary class="question-row-summary"><i class="question-status-mark" data-status={questionAnswerStatus(question.id)}></i><span class="question-summary-copy"><span><small>Q{index + 2}</small>{question.text || 'New question'}</span>{#if questionExcerpt(question.id)}<span class="question-excerpt">{questionExcerpt(question.id)}</span>{/if}<small class="question-summary-status">{questionAnswerStatusLabel(questionAnswerStatus(question.id))}</small></span><Icon name="chevron-right" size={12} /></summary>
											<div class="question-expanded">
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
										<DropdownMenu.Root>
											<DropdownMenu.Trigger type="button" class="question-more" aria-label="More actions for Subquestion {index + 1}" title="More actions"><Icon name="more-horizontal" size={16} /></DropdownMenu.Trigger>
											<DropdownMenu.Portal><DropdownMenu.Content class="fella-ui-menu-content" align="end" sideOffset={4}>
												<DropdownMenu.Item class="fella-ui-menu-item" disabled={saving || proposingQuestions || index === 0} onSelect={() => moveQuestion(index, -1)}>Move up</DropdownMenu.Item>
												<DropdownMenu.Item class="fella-ui-menu-item" disabled={saving || proposingQuestions || index === subquestions.length - 1} onSelect={() => moveQuestion(index, 1)}>Move down</DropdownMenu.Item>
												<DropdownMenu.Item class="fella-ui-menu-item" disabled={saving || proposingQuestions} onSelect={() => removeQuestion(question.id)}>Remove question</DropdownMenu.Item>
											</DropdownMenu.Content></DropdownMenu.Portal>
										</DropdownMenu.Root>
												</div>
											</div>
											{#if questionAnswerReason(question.id)}
												<p class="question-status-reason" data-question-status={questionAnswerStatus(question.id)} role="status">{questionAnswerReason(question.id)}</p>
											{/if}
												{#if staleResultReview?.questionId === question.id}
													<div class="question-stale-review" role="status">
														<p>This saved answer used catalog {staleResultReview.status.original_revision ?? 'not recorded'}; the current catalog is {staleResultReview.status.current_revision ?? 'different'}. The saved answer and run are unchanged.</p>
														<details>
														<summary>Source scope used by the saved run</summary>
														{#if staleResultReview.status.source_scope?.used_sources.length}
															<ul>{#each staleResultReview.status.source_scope.used_sources as source (`${source.source}-${source.table}`)}<li>{source.table ? `${source.source} · ${source.table}` : source.source}</li>{/each}</ul>
														{:else}
															<p>The saved run did not record its source list.</p>
														{/if}
													</details>
													<p>Review the included files in the run preview before choosing to run this Question again.</p>
													</div>
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
											{#if editingQuestionIds.includes(question.id) || !question.text.trim()}
											<Textarea
												id="question-{question.id}"
												class="question-field"
												bind:value={question.text}
												disabled={saving || proposingQuestions}
												aria-label="Subquestion {index + 1}"
											/>
											<Button variant="ghost" size="sm" disabled={!question.text.trim()} onclick={() => (editingQuestionIds = editingQuestionIds.filter((id) => id !== question.id))}>Done editing</Button>
											{:else}
											<button class="question-text" id="question-{question.id}" type="button" aria-label="Edit Subquestion {index + 1}" onclick={() => void focusQuestion(question.id)}>{question.text}<span>Edit</span></button>
											{/if}
											<InvestigationQuestionSourceScope
												workspacePath={selected.workspace_id}
												investigation={selected}
												questionId={question.id}
												selection={question.source_selection}
												disabled={saving || proposingQuestions}
												onChange={(selection) => updateQuestionSourceSelection(question.id, selection)}
											/>
											</div></details>
										</li>
									{/each}
								</ol>
							{/if}
						{:else}
							<div class="question-empty-state">
								<div><strong>No Questions yet</strong></div>
								<Button type="button" onclick={addQuestion} disabled={saving || proposingQuestions}>
									<Icon name="plus" size={16} />Add main Question
								</Button>
							</div>
						{/if}
						<div class="question-plan-actions"><h4>Suggested by Fella</h4>
							{#if rootQuestion}
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
							{/if}
							{#if autosaveError && (dirty || questionsDirty || definitionsDirty)}
								<Button type="button" variant="ghost" disabled={autosaving} onclick={retryEditableAutosave}>Retry save</Button>
							{/if}
						</div>
						{#if pendingAnalysisQuestion}
							<section class="run-preview" aria-labelledby="run-preview-title">
								<header>
									<h4 id="run-preview-title">Review this run</h4>
									<p>Check the Question and included sources before running.</p>
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
								{#if analysisSetupMissingPaths.length}
									<p class="run-preview-warning" role="status">
										{analysisSetupMissingPaths.length} selected path{analysisSetupMissingPaths.length === 1 ? ' is' : 's are'} not present in the current catalog and will not be used: {analysisSetupMissingPaths.join(', ')}. If a file was skipped, its reason appears in the unavailable-file list.
									</p>
								{/if}
								{#if analysisSetupOutOfScopePaths.length}
									<p class="run-preview-warning" role="status">
										{analysisSetupOutOfScopePaths.length} selected source{analysisSetupOutOfScopePaths.length === 1 ? ' is' : 's are'} cataloged but outside the Investigation scope and will not be used: {analysisSetupOutOfScopePaths.join(', ')}.
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
					<p class="run-preview-empty">No sources are currently cataloged, so this preview cannot confirm which files the run can use.</p>
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
											<Button type="button" variant="ghost" disabled={startingQuestionAnalysis} onclick={() => { pendingAnalysisQuestionId = null; staleResultReview = null; }}>Cancel</Button>
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
					</section>
						<div class="optional-context-heading">
							<p>Context</p>
						</div>
						<details class="definition-disclosure" bind:open={definitionsOpen}>
							<summary>
				<span id="definition-context-title">Definitions &amp; assumptions</span>
								<small>{definitions.length ? `${definitions.length} context note${definitions.length === 1 ? '' : 's'}` : 'None saved'}</small>
							</summary>
					<section class="definition-editor" aria-labelledby="definition-context-title">
						<header class="question-heading">
							<div>
								<p>Each run keeps the context version it used.</p>
							</div>
							<Button type="button" variant="ghost" size="sm" disabled={saving} onclick={startDefinition}>
								<Icon name="plus" size={14} />Add context
							</Button>
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
										<Select
											id="definition-kind-{definition.id}"
											ariaLabel="Context type {index + 1}"
											value={definition.kind}
											options={[
												{ value: 'definition', label: 'Definition' },
												{ value: 'assumption', label: 'Assumption' }
											]}
											disabled={saving}
											onValueChange={(value) => (definition.kind = value as typeof definition.kind)}
										/>
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
							<p class="definition-empty">No saved definitions or assumptions.</p>
						{/if}
					</section>
						</details>
					</section>
						</div>
						<div
							id="investigation-panel-analysis"
							class="investigation-tab-panel"
							role="tabpanel"
							aria-labelledby="investigation-tab-analysis"
							tabindex="0"
							hidden={investigationSection !== 'analysis'}
						>
					<section class="investigation-document-section investigation-analysis-section" aria-labelledby="investigation-analysis-heading">
						<h3 id="investigation-analysis-heading" class="sr-only">Analysis runs</h3>
						<InvestigationRunOverview ids={analysisRunIds(selected)} records={analysisTurns} selectedId={overviewRunId} loading={overviewLoadingId === overviewRunId && overviewRunId !== null} error={overviewError} onselect={(id) => void loadOverviewRun(id)} oninspect={(id) => void openAnalysisRun(id)} />
					<details class="investigation-history" bind:open={historyOpen}>
						<summary>
								<span>Manage findings and records</span>
							<small>{analysisRunIds(selected).length} run{analysisRunIds(selected).length === 1 ? '' : 's'} · {selected.findings?.length ?? 0} finding{selected.findings?.length === 1 ? '' : 's'}</small>
						</summary>
						<div class="investigation-history-content">
					<section class="findings" aria-labelledby="findings-title">
						<header class="findings-heading">
							<div>
								<h3 id="findings-title">Findings</h3>
								<p>Reviewed conclusions linked to their Questions and evidence.</p>
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
								<h3 id="analysis-runs-title">Runs</h3>
								<p>Each run keeps its own scope, method, and evidence.</p>
							</div>
							<span>{analysisRunIds(selected).length} linked</span>
						</header>
						{#if analysisRunIds(selected).length}
							<details class="analysis-run-chooser" open={!openAnalysisTurnId}>
								<summary>Saved runs · {analysisRunIds(selected).length}</summary>
							<ol class="analysis-run-list">
								{#each analysisRunIds(selected) as turnId, index (turnId)}
									{@const timestamp = analysisRunTimestamp(turnId)}
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
												{#if timestamp}
													<small class="analysis-run-time">Started <time datetime={new Date(timestamp).toISOString()}>{formatAnalysisRunTimestamp(timestamp)}</time></small>
												{/if}
											</span>
											<Icon name="chevron-right" size={14} />
										</button>
									</li>
								{/each}
							</ol>
							</details>
						{:else}
							<p class="analysis-runs-empty">No saved analysis runs are linked yet.</p>
						{/if}

						{#if openAnalysisTurnId}
							<section id="saved-analysis-run-detail" class="analysis-run-detail" aria-label="Saved analysis run" tabindex="-1">
								<header>
									<div>
										<h4>Saved analysis run</h4>
									</div>
									{#if openAnalysisTurn?.conversation_id && selected.conversation_id === openAnalysisTurn.conversation_id}
										<Button type="button" variant="outline" size="sm" class="analysis-run-open-chat" onclick={() => void openSelectedConversation(openAnalysisTurnId!)}>
											<Icon name="ask" size={14} />Open conversation result
										</Button>
									{/if}
									<Button type="button" variant="ghost" size="icon" class="analysis-run-close" aria-label="Close saved analysis run" onclick={() => (openAnalysisTurnId = null)}><Icon name="x" size={14} /></Button>
								</header>
								{#if analysisTurnLoadingId === openAnalysisTurnId}
									<p role="status">Loading the saved run…</p>
								{:else if analysisTurnError}
									<p class="analysis-run-error" role="alert">Couldn't load or update this run: {analysisTurnError}</p>
								{:else if openAnalysisTurn}
									<section class="analysis-run-answer" aria-label="Recorded answer">
										<h5>Recorded answer</h5>
										<MarkdownContent text={openAnalysisTurn.result.text} />
										<small>Saved model answer · not an analyst-reviewed Finding.</small>
									</section>
									<fieldset class="analysis-run-question-links" disabled={savingQuestionLinks || questionsDirty}>
										<legend>Questions this run addresses</legend>
										{#if selected.questions?.length}
											{#each selected.questions as question (question.id)}
												<label>
													<input type="checkbox" checked={questionLinksDraft.includes(question.id)} disabled={savingQuestionLinks || questionsDirty || question.latest_analysis_turn_id === openAnalysisTurn.id} onchange={(event) => setQuestionRunLink(question.id, event.currentTarget.checked)} />
													<span><strong>{questionHeading(question.id)}</strong><small title={question.text}>{question.text}</small></span>
												</label>
											{/each}
											<Button type="button" variant="outline" size="sm" disabled={!questionLinksDirty || savingQuestionLinks || questionsDirty} onclick={() => void saveAnalysisRunQuestions()}>{savingQuestionLinks ? 'Saving links…' : 'Save Question links'}</Button>
											<small>Linking preserves this run. It does not mark a Question answered.</small>
										{:else}<p>No active Questions to link.</p>{/if}
										{#if questionLinksError}<p class="analysis-run-error" role="alert">Couldn't save Question links: {questionLinksError}. Your saved answer is unchanged. Try saving again.</p>{/if}
										{#if questionLinksStatus}<p role="status">{questionLinksStatus}</p>{/if}
									</fieldset>
									<details class="analysis-run-technical">
									<summary>Run details and source scope</summary>
									<dl class="analysis-run-facts">
										<div><dt>State</dt><dd>{runStateLabel(openAnalysisTurn.state)}</dd></div>
										<div><dt>Method</dt><dd>{openAnalysisTurn.plan.strategy.replaceAll('_', ' ')}</dd></div>
										<div><dt>Execution steps</dt><dd>{openAnalysisTurn.trace.steps.length}</dd></div>
										<div><dt>Recorded evidence</dt><dd>{openAnalysisTurn.result.evidence.length} items</dd></div>
										<div>
											<dt>Source catalog revision</dt>
											<dd>
												{#if openAnalysisTurn.workspace_revision}
													<code>{openAnalysisTurn.workspace_revision}</code>
												{:else}
													Not recorded for this run
												{/if}
											</dd>
										</div>
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
										<div>
											<dt>Question at run</dt>
											<dd>
												{openAnalysisTurn.investigation_context?.question_text ?? openAnalysisTurn.question}
												{#if openAnalysisTurn.investigation_context?.question_id && selected.questions?.some((question) => question.id === openAnalysisTurn.investigation_context?.question_id)}
													<Button type="button" variant="ghost" size="sm" onclick={() => focusQuestion(openAnalysisTurn.investigation_context!.question_id!)}>Go to Question</Button>
												{/if}
											</dd>
										</div>
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
										<div class="analysis-run-source-provenance">
											<section class="analysis-run-source-state" aria-label="Requested source scope">
												<h6>Requested for this run</h6>
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
												</dl>
											</section>
											<section class="analysis-run-source-state" aria-label="Observed source use">
												<h6>Observed in recorded evidence</h6>
												<dl class="analysis-run-context-facts">
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
										</section>
									</div>
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
									</details>
									<RunTimeline
										turn={openAnalysisTurn}
										bodyId={`investigation-run-evidence-${openAnalysisTurn.id}`}
									/>
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
					<section class="finding-capture" aria-labelledby="finding-capture-title">
						<header>
							<h5 id="finding-capture-title">Save or revise a Finding</h5>
							<p>Capture material limits and assumptions so the conclusion stays interpretable when shared.</p>
						</header>
						<label for="finding-claim-{openAnalysisTurn.id}">Claim</label>
						<Textarea id="finding-claim-{openAnalysisTurn.id}" bind:value={findingClaimDraft} oninput={() => (findingDraftTouched = true)} aria-label="Finding claim" class="finding-capture-field" disabled={savingFinding} maxlength={2000} />
						<label for="finding-caveats-{openAnalysisTurn.id}">Material caveats <span>(optional, one per line)</span></label>
						<Textarea id="finding-caveats-{openAnalysisTurn.id}" bind:value={findingCaveatsDraft} oninput={() => (findingDraftTouched = true)} aria-label="Finding caveats" class="finding-capture-field finding-notes-field" disabled={savingFinding} />
						<label for="finding-assumptions-{openAnalysisTurn.id}">Assumptions <span>(optional, one per line)</span></label>
						<Textarea id="finding-assumptions-{openAnalysisTurn.id}" bind:value={findingAssumptionsDraft} oninput={() => (findingDraftTouched = true)} aria-label="Finding assumptions" class="finding-capture-field finding-notes-field" disabled={savingFinding} />
						<label for="finding-question-{openAnalysisTurn.id}">Question this Finding answers</label>
						<Select
							id="finding-question-{openAnalysisTurn.id}"
							ariaLabel="Question for Finding"
							value={findingQuestionDraft}
							emptyOptionLabel="Select a Question"
							options={(selected.questions ?? []).map((question) => ({ value: question.id, label: question.text }))}
							disabled={savingFinding || questionsDirty || !(selected.questions?.length)}
							onValueChange={(value) => {
								findingQuestionDraft = value;
								findingDraftTouched = true;
								findingRevisionTargetDraft = '';
								findingRevisionNoteDraft = '';
							}}
						/>
						{#if (selected.findings ?? []).some((finding) => finding.question_id === findingQuestionDraft)}
							<label for="finding-revision-target-{openAnalysisTurn.id}">Revise an existing Finding <span>(optional)</span></label>
							<Select
								id="finding-revision-target-{openAnalysisTurn.id}"
								ariaLabel="Revise existing Finding"
								value={findingRevisionTargetDraft}
								emptyOptionLabel="Save as a new Finding"
								options={(selected.findings ?? [])
									.filter((finding) => finding.question_id === findingQuestionDraft)
									.map((finding) => ({
										value: finding.id,
										label: `Finding ${finding.revision ?? 1} · ${finding.claim.slice(0, 90)}`
									}))}
								disabled={savingFinding || questionsDirty}
								onValueChange={selectFindingRevisionTarget}
							/>
						{/if}
						{#if findingRevisionTargetDraft}
							<label for="finding-revision-note-{openAnalysisTurn.id}">Why this changed <span>(optional)</span></label>
							<Textarea id="finding-revision-note-{openAnalysisTurn.id}" bind:value={findingRevisionNoteDraft} oninput={() => (findingDraftTouched = true)} aria-label="Finding revision note" class="finding-capture-field finding-notes-field" disabled={savingFinding} />
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
						</div>
					</details>
					</section>
						</div>
						<div
							id="investigation-panel-results"
							class="investigation-tab-panel"
							role="tabpanel"
							aria-labelledby="investigation-tab-results"
							tabindex="0"
							hidden={investigationSection !== 'results'}
						>
					<section class="investigation-document-section investigation-results-section" aria-labelledby="investigation-results-heading">
						<h3 id="investigation-results-heading" class="sr-only">Investigation results</h3>
						<InvestigationResultsSummary investigation={selected} messages={investigationConversationMessages} onreview={(id) => reviewQuestionFromResults(id)} onreport={() => void reviewInvestigationHandoff()} busy={exportingReport} disabledReason={reportExportDisabledReason} />
						{#if reportExportStatus}<p class="report-export-status" role={reportExportFailed ? 'alert' : 'status'}>{reportExportStatus}</p>{/if}
						<details class="results-records"><summary>Evidence and question coverage</summary>
						<InvestigationResultsView
							investigation={selected}
							conversationMessages={investigationConversationMessages}
							hasUnsavedEdits={dirty || questionsDirty || definitionsDirty}
							onopenrun={openRunFromResults}
							onopenlegacy={openLegacyRunFromResults}
							onreviewquestion={reviewQuestionFromResults}
						/>
						</details>
							<footer class="investigation-completion-actions">
							{#if selected.status === 'complete'}
								<Button type="button" variant="outline" disabled={saving} onclick={() => void reopenInvestigation()}>
									Reopen Investigation
								</Button>
							{:else}
								<Button
									type="button"
									variant="outline"
									disabled={!coverage.canComplete || saving}
									title="Answer each active Question or document a data gap. Review flagged results and resolve them before completing."
									onclick={() => void markInvestigationComplete()}
								>
									Mark complete
								</Button>
							{/if}
								{#if selected.status !== 'complete' && !coverage.canComplete}<p>Resolve open questions or document a data gap before completing.</p>{/if}
							</footer>
					</section>
						</div>
						<div
							id="investigation-panel-share"
							class="investigation-tab-panel"
							role="tabpanel"
							aria-labelledby="investigation-share-action"
							tabindex="0"
							hidden={investigationSection !== 'share'}
						>
					<section class="investigation-document-section investigation-share-section" aria-labelledby="investigation-share-heading">
						<header class="investigation-document-heading">
							<div><h3 id="investigation-share-heading">Share</h3><p>Preview the report before exporting.</p></div>
							<Button
								type="button"
								variant="outline"
								disabled={exportingReport || Boolean(reportExportDisabledReason)}
								title={reportExportDisabledReason || undefined}
								onclick={() => void reviewInvestigationHandoff()}
							>
								<Icon name="file" size={16} />{exportingReport ? 'Preparing preview…' : 'Review report'}
							</Button>
						</header>
						{#if reportExportDisabledReason}<p class="report-export-disabled-reason">{reportExportDisabledReason}</p>{/if}
						{#if reportExportStatus}
							<p class="report-export-status" role={reportExportFailed ? 'alert' : 'status'}>{reportExportStatus}</p>
						{/if}
					</section>
						</div>
					</div>
				</div>
			</div>
		{:else}
			<div class="empty-state" data-testid="investigation-not-selected">
				<Icon name="bookmark" size={20} />
				<h2>Choose an Investigation</h2>
				<p>Select one from the workspace menu above to open its conversation and analysis.</p>
			</div>
		{/if}
	{/if}

	<Dialog.Root bind:open={nameOpen}>
		<Dialog.Portal>
			<Dialog.Overlay class="fella-ui-dialog-overlay" />
			<Dialog.Content class="fella-ui-dialog-content investigation-name-dialog">
				<Dialog.Title>Rename Investigation</Dialog.Title>
				<Dialog.Description>A short name for tabs and navigation. Your original request stays unchanged.</Dialog.Description>
				<form onsubmit={(event) => { event.preventDefault(); void renameInvestigation(); }}>
					<label for="investigation-name">Investigation name</label>
					<Input id="investigation-name" bind:value={nameDraft} maxlength={120} disabled={naming} />
					{#if nameError}<p role="alert">{nameError}</p>{/if}
					<div class="name-dialog-actions"><Dialog.Close disabled={naming} class="name-cancel">Cancel</Dialog.Close><Button type="submit" disabled={naming || !nameDraft.trim()}>{naming ? 'Saving…' : 'Save name'}</Button></div>
				</form>
			</Dialog.Content>
		</Dialog.Portal>
	</Dialog.Root>
	<Dialog.Root bind:open={reportPreviewOpen}>
		<Dialog.Portal>
			<Dialog.Overlay class="fella-ui-dialog-overlay" />
			<Dialog.Content class="fella-ui-dialog-content handoff-preview-dialog" aria-label="Review report">
				{#if reportPreview}
					<header class="handoff-preview-header">
						<div>
							<p class="handoff-preview-eyebrow">Investigation report</p>
							<Dialog.Title class="handoff-preview-title" level={2}>Review report</Dialog.Title>
							<Dialog.Description class="handoff-preview-description">
								Review the exact Markdown export generated from the saved brief, Questions, Findings, and analysis runs.
							</Dialog.Description>
						</div>
						<Dialog.Close class="handoff-preview-close" type="button" aria-label="Close report preview" title="Close">
							<Icon name="x" size={16} />
						</Dialog.Close>
					</header>
					{@const activeHandoffQuestions = (reportPreview.investigation.questions ?? []).filter((question) => question.status === 'active')}
					{@const unresolvedHandoffQuestions = activeHandoffQuestions.filter((question) => question.answer_status !== 'answered')}
					<div class="handoff-preview-body">
						<section class="handoff-coverage" aria-label="Report coverage status">
							{#if reportPreview.investigation.status === 'complete' && unresolvedHandoffQuestions.length}
								<Badge variant="warning">Closed with unresolved Questions · not fully answered</Badge>
							{:else if unresolvedHandoffQuestions.length}
								<Badge variant="warning">{unresolvedHandoffQuestions.length} unresolved Question{unresolvedHandoffQuestions.length === 1 ? '' : 's'}</Badge>
							{:else if activeHandoffQuestions.length}
								<Badge variant="success">All active Questions answered</Badge>
							{:else}
								<Badge>No active Questions recorded</Badge>
							{/if}
							<span>{activeHandoffQuestions.length} active Question{activeHandoffQuestions.length === 1 ? '' : 's'} · {reportPreview.investigation.findings?.length ?? 0} saved Finding{(reportPreview.investigation.findings?.length ?? 0) === 1 ? '' : 's'} · {reportPreview.charts.length} visualization{reportPreview.charts.length === 1 ? '' : 's'}</span>
						</section>

						<section class="handoff-markdown" aria-label="Markdown report preview">
							<article class="handoff-markdown-content"><MarkdownContent text={reportPreview.markdown} /></article>
						</section>
						<details class="handoff-records" aria-label="Supporting records in Fella">
							<summary>Supporting records in Fella</summary>
							{#if (reportPreview.investigation.findings ?? []).length}
								<ul>
									{#each reportPreview.investigation.findings ?? [] as finding (finding.id)}
										{@const findingQuestion = reportPreview.investigation.questions?.find((question) => question.id === finding.question_id)}
										{@const findingFreshness = runFreshness(findingQuestion, finding.analysis_turn_id)}
										<li>
											<div>
												<strong>{finding.claim}</strong>
												<small>Finding revision {finding.revision ?? 1} · run {finding.analysis_turn_id} · Source catalog revision {runRevision(reportPreview.runs[finding.analysis_turn_id])}</small>
												{#if findingFreshness.startsWith('Earlier') || findingFreshness.startsWith('Question linkage') || findingFreshness.startsWith('No latest')}
													<small class="handoff-freshness-warning">{findingFreshness}</small>
												{/if}
												</div>
											<div class="handoff-record-actions">
												{#each finding.evidence_ids as evidenceId (evidenceId)}
													<Button type="button" variant="ghost" size="sm" onclick={() => void openHandoffReference(finding.analysis_turn_id, evidenceId)}>Open evidence · {evidenceId}</Button>
												{/each}
												<Button type="button" variant="outline" size="sm" onclick={() => void openHandoffReference(finding.analysis_turn_id)}>Open run</Button>
												{#if !finding.evidence_ids.length}<small>No stable evidence IDs recorded.</small>{/if}
												</div>
										</li>
									{/each}
								</ul>
							{:else}
								<p class="handoff-record-empty">No reviewed Findings are saved. Model-generated answers and charts remain labeled separately below.</p>
							{/if}
							{#if unresolvedHandoffQuestions.length}
								<ul class="handoff-open-questions" aria-label="Unresolved Question links">
									{#each unresolvedHandoffQuestions as question (question.id)}
										<li>
											<div><strong>{question.text}</strong><small>{questionAnswerStatusLabel(question.answer_status)} · {questionCoverageReason(question)}</small></div>
											{#if question.latest_analysis_turn_id}<Button type="button" variant="ghost" size="sm" onclick={() => void openHandoffReference(question.latest_analysis_turn_id!)}>Open supporting run</Button>{/if}
										</li>
									{/each}
								</ul>
							{/if}
						</details>

						{#if reportPreview.charts.length}
							<section class="handoff-visualizations" aria-label="Visualization previews included in the report">
								<header><h3>Visualizations</h3><p>Model-produced artifacts, not analyst-reviewed Findings.</p></header>
								{#each reportPreview.charts as chart (chart.key)}
									{@const chartQuestion = questionForRun(reportPreview.investigation, chart.run)}
									{@const chartFreshness = runFreshness(chartQuestion, chart.run.id)}
									<article class="handoff-chart-card">
										<Chart spec={chart.evidence.chart!} source={chart.sourceEvidence?.result_summary ?? chart.evidence.chart?.metadata?.source_label ?? ''} sourceEvidence={chart.sourceEvidence} sourceRunId={chart.run.id} findingSnapshot={chart.evidence.chart?.metadata?.finding_snapshot} originLabel="Model-produced visualization" />
										<div class="handoff-chart-lineage">
											<span>Run <code>{chart.run.id}</code></span>
											<span>Source catalog revision {runRevision(chart.run)}</span>
											{#if chartFreshness.startsWith('Earlier') || chartFreshness.startsWith('Question linkage') || chartFreshness.startsWith('No latest')}
												<span class="handoff-freshness-warning">{chartFreshness}</span>
											{/if}
											{#if chart.evidence.id}<Button type="button" variant="ghost" size="sm" onclick={() => void openHandoffReference(chart.run.id, chart.evidence.id)}>Open chart evidence</Button>{/if}
											{#if chart.sourceEvidence?.id}<Button type="button" variant="ghost" size="sm" onclick={() => void openHandoffReference(chart.run.id, chart.sourceEvidence!.id)}>Open source evidence</Button>{/if}
										</div>
									</article>
								{/each}
							</section>
						{/if}

					</div>

					<footer class="handoff-preview-footer">
						{#if reportExportStatus}<span role={reportExportFailed ? 'alert' : 'status'}>{reportExportStatus}</span>{/if}
						<div class="handoff-preview-actions"><span class="handoff-preview-filename">Markdown file · <code>{reportPreview.filename}</code></span><Dialog.Close class="handoff-preview-secondary" type="button">Back</Dialog.Close><Button type="button" onclick={downloadInvestigationHandoff} disabled={exportingReport}><Icon name="download" size={16} />{exportingReport ? 'Preparing…' : 'Export report'}</Button></div>
					</footer>
				{/if}
			</Dialog.Content>
		</Dialog.Portal>
	</Dialog.Root>
</section>

<style>
	.investigation-page {
		flex: 1;
		min-height: 0;
		width: 100%;
		max-width: var(--content-max);
		margin: 0 auto;
		padding: 20px;
		overflow: auto;
		overscroll-behavior: contain;
	}
	.page-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-4);
	}
	.page-head { margin-bottom: 16px; }
	.page-head > div { min-width: 0; }
	.investigation-title-row { display: flex; align-items: flex-start; gap: 8px; width: 100%; }
	.investigation-title-row h1 { flex: 1; }
	.investigation-title-row :global(button) { flex: none; color: var(--text-faint); }
	:global(.investigation-name-dialog) { width: min(440px, calc(100vw - 32px)); padding: 24px; }
	:global(.investigation-name-dialog form) { display: grid; gap: 12px; margin-top: 20px; }
	:global(.investigation-name-dialog [data-dialog-description]) { color: var(--text-dim); line-height: 1.5; }
	:global(.name-dialog-actions) { display: flex; justify-content: flex-end; gap: 8px; margin-top: 8px; }
	:global(.name-cancel) { border: 0; border-radius: var(--radius); padding: 8px 12px; background: transparent; color: var(--text-dim); cursor: pointer; }
	.analysis-run-technical { min-width: 0; font-size: var(--fs-sm); }
	.analysis-run-technical > summary { cursor: pointer; color: var(--text-dim); padding: 8px 0; }
	.analysis-run-technical > dl { margin-top: 12px; }
	.analysis-run-question-links label small { display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; }
	.analysis-run-question-links > small, .analysis-run-question-links > p { margin: 0; font-size: var(--fs-xs); color: var(--text-dim); }
	.analysis-run-detail > header { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: center; }
	.analysis-run-detail > header > div { grid-column: 1; grid-row: 1; }
	.analysis-run-detail :global(.analysis-run-close) { grid-column: 2; grid-row: 1; }
	.analysis-run-detail :global(.analysis-run-open-chat) { grid-column: 1 / -1; grid-row: 2; justify-self: start; }
	.analysis-run-chooser > summary { cursor: pointer; color: var(--text-dim); font-size: var(--fs-xs); padding: 6px 0; }
	.page-head h1 { overflow-wrap: anywhere; }
	.investigation-nav {
		position: sticky;
		top: 0;
		z-index: 4;
		display: flex;
		flex-wrap: nowrap;
		min-width: 0;
		width: max-content;
		max-width: 100%;
		margin: 0 0 var(--space-4);
		overflow-x: auto;
		overflow-y: hidden;
		scrollbar-width: thin;
		scrollbar-color: var(--border-strong) transparent;
		overscroll-behavior-x: contain;
		background: var(--bg);
	}
	.investigation-nav::-webkit-scrollbar { height: 4px; }
	.investigation-nav::-webkit-scrollbar-track { background: transparent; }
	.investigation-nav::-webkit-scrollbar-thumb { border-radius: 999px; background: var(--border-strong); }
	@container investigation-pane (max-width: 430px) {
		.investigation-nav {
			width: 100%;
			gap: clamp(0px, 0.4cqi, 2px);
		}
		.investigation-nav > .fella-pill-nav-item {
			flex: 1 1 auto;
			min-width: 0;
			padding-inline: clamp(2px, 0.55cqi, 4px);
			font-size: clamp(11px, 3cqi, var(--fs-xs));
		}
	}
	.investigation-tab-panel { min-width: 0; }
	.investigation-tab-panel[hidden] { display: none; }
	.investigation-tab-panel > .investigation-document-section:first-child,
	.investigation-tab-panel > .analyst-work-plan:first-child { padding-top: var(--space-4); }
	.report-export-status { max-width: 300px; color: var(--text-dim); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.report-export-status[role='alert'] { color: var(--danger, #b42318); }
	.report-export-disabled-reason { margin: var(--space-2) 0 0; color: var(--text-faint); font-size: var(--fs-xs); }
	:global(.handoff-preview-dialog) {
		display: grid;
		grid-template-rows: auto minmax(0, 1fr) auto;
		width: min(1040px, calc(100vw - 40px));
		height: min(900px, calc(100vh - 40px));
		padding: 0;
		overflow: hidden;
	}
	.handoff-preview-header, .handoff-preview-footer { display: flex; align-items: center; justify-content: space-between; gap: var(--space-4); padding: var(--space-4) var(--space-5); }
	.handoff-preview-header { border-bottom: 1px solid var(--border); }
	.handoff-preview-eyebrow { margin: 0 0 3px; color: var(--text-faint); font-size: var(--fs-xs); font-weight: 650; letter-spacing: .04em; text-transform: uppercase; }
	:global(.handoff-preview-title) { margin: 0; font-size: var(--fs-lg); font-weight: 650; letter-spacing: -.02em; }
	:global(.handoff-preview-description) { margin: 3px 0 0; color: var(--text-dim); font-size: var(--fs-sm); }
	:global(.handoff-preview-close) { display: grid; width: 32px; height: 32px; flex: none; place-items: center; border: 1px solid transparent; border-radius: var(--radius-sm); background: transparent; color: var(--text-dim); cursor: pointer; }
	:global(.handoff-preview-close:hover) { border-color: var(--border); background: var(--bg-inset); color: var(--text); }
	.handoff-preview-body { display: grid; align-content: start; gap: var(--space-4); min-height: 0; overflow: auto; padding: var(--space-4) var(--space-5); }
	.handoff-coverage { display: flex; align-items: center; flex-wrap: wrap; gap: var(--space-2); color: var(--text-dim); font-size: var(--fs-xs); }
	.handoff-records, .handoff-visualizations, .handoff-markdown { display: grid; gap: var(--space-2); min-width: 0; }
	.handoff-visualizations > header { display: grid; gap: 2px; }
	.handoff-visualizations h3, .handoff-records > summary { margin: 0; font-size: var(--fs-sm); font-weight: 650; }
	.handoff-records > summary { cursor: pointer; color: var(--text-dim); padding: 8px 0; }
	.handoff-visualizations > header p { margin: 0; color: var(--text-dim); font-size: var(--fs-xs); }
	.handoff-records > ul { display: grid; gap: var(--space-2); margin: 0; padding: 0; list-style: none; }
	.handoff-records > ul > li { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); padding: var(--space-2) var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg); }
	.handoff-records li > div:first-child { display: grid; min-width: 0; gap: 3px; }
	.handoff-records li strong { font-size: var(--fs-xs); font-weight: 600; overflow-wrap: anywhere; }
	.handoff-records li small, .handoff-record-empty { color: var(--text-faint); font-size: 11px; overflow-wrap: anywhere; }
	.handoff-freshness-warning { color: var(--warning, #8a5a00) !important; }
	.handoff-record-actions { display: flex; align-items: center; flex-wrap: wrap; justify-content: flex-end; gap: 2px; }
	.handoff-open-questions { border-top: 1px solid var(--border); padding-top: var(--space-2) !important; }
	.handoff-chart-card { min-width: 0; padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-lg); background: var(--bg); }
	.handoff-chart-card :global(.chart-card) { max-width: 680px; margin-inline: auto; }
	.handoff-chart-lineage { display: flex; align-items: center; flex-wrap: wrap; gap: var(--space-2); padding-top: var(--space-2); border-top: 1px solid var(--border); color: var(--text-dim); font-size: var(--fs-xs); }
	.handoff-markdown-content { min-width: 0; padding: var(--space-4); border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg); font-size: var(--fs-sm); line-height: 1.55; overflow-wrap: anywhere; }
	.handoff-markdown-content :global(h1) { margin: 0 0 var(--space-2); font-size: var(--fs-lg); }
	.handoff-markdown-content :global(h2) { margin: var(--space-4) 0 var(--space-2); padding-top: var(--space-2); border-top: 1px solid var(--border); font-size: var(--fs-md); }
	.handoff-markdown-content :global(h3) { margin: var(--space-3) 0 var(--space-1); font-size: var(--fs-sm); }
	.handoff-markdown-content :global(p), .handoff-markdown-content :global(ul) { margin: var(--space-1) 0; }
	.handoff-markdown-content :global(ul) { padding-left: 1.25rem; }
	.handoff-markdown-content :global(table) { display: block; max-width: 100%; overflow-x: auto; border-collapse: collapse; font-size: var(--fs-xs); font-variant-numeric: tabular-nums; }
	.handoff-markdown-content :global(th), .handoff-markdown-content :global(td) { padding: 5px 8px; border: 1px solid var(--border); text-align: left; }
	.handoff-markdown-content :global(th) { background: var(--bg-inset); }
	.handoff-markdown-content :global(code) { padding: 1px 4px; border-radius: var(--radius-xs); background: var(--bg-inset); font-size: .92em; }
	.handoff-preview-footer { display: grid; grid-template-columns: minmax(140px, 1fr) minmax(0, auto); border-top: 1px solid var(--border); }
	.handoff-preview-footer > span { min-width: 0; color: var(--text-dim); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.handoff-preview-footer > span[role='alert'] { color: var(--danger, #b42318); }
	.handoff-preview-footer > div { display: flex; flex: none; align-items: center; gap: var(--space-2); }
	.handoff-preview-footer .handoff-preview-actions { min-width: 0; }
	.handoff-preview-filename { min-width: 0; max-width: min(44ch, 35vw); margin-right: var(--space-2); color: var(--text-faint); font-size: var(--fs-xs); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.handoff-preview-actions :global(button) { flex: none; }
	@media (max-width: 720px) { .handoff-preview-footer { grid-template-columns: minmax(0, 1fr); } .handoff-preview-footer .handoff-preview-actions { justify-content: flex-end; } .handoff-preview-filename { flex: 1; max-width: none; } }
	.handoff-preview-filename code { color: var(--text-dim); }
	:global(.handoff-preview-secondary) { min-height: 36px; padding: 0 var(--space-3); border: 1px solid transparent; border-radius: var(--radius-sm); background: transparent; color: var(--text-dim); font: inherit; cursor: pointer; }
	:global(.handoff-preview-secondary:hover) { border-color: var(--border); background: var(--bg-inset); color: var(--text); }
	h1, h2, p { margin: 0; }
	h1 { display: -webkit-box; overflow: hidden; -webkit-box-orient: vertical; -webkit-line-clamp: 2; line-clamp: 2; font-size: 18px; line-height: 1.4; font-weight: 600; letter-spacing: -0.02em; }
	.page-head p { margin-top: var(--space-1); color: var(--text-dim); }
	.investigation-content { min-width: 0; }
	.brief-editor { min-width: 0; }
	.investigation-document-section { display: grid; min-width: 0; gap: var(--space-4); padding: var(--space-6) 0; }
	.investigation-document-heading { display: flex; min-width: 0; align-items: center; justify-content: space-between; gap: var(--space-4); }
	.investigation-document-heading > div { display: grid; min-width: 0; gap: 3px; }
	.investigation-document-heading h2, .investigation-document-heading h3 { margin: 0; font-size: var(--fs-md); font-weight: 650; }
	.investigation-document-heading p { color: var(--text-dim); font-size: var(--fs-xs); line-height: 1.45; }
	.investigation-data-section :global(.source-readiness) { padding:0; border-bottom: 0; }
	.investigation-analysis-section { scroll-margin-top: var(--space-5); }
	.investigation-results-section :global(.results-view) { padding: 0; }
	.investigation-share-section { margin-bottom: var(--space-6); }
	.investigation-share-section .report-export-status { max-width: none; }
	.brief-summary { display: grid; gap: 20px; padding: 12px 0 20px; margin-top: var(--space-2); border: 0; background: transparent; }
	.brief-summary-header { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--space-3); }
	.brief-objective { display: grid; min-width: 0; flex: 1; gap: 9px; }
	.brief-summary h3, .brief-scope h4 { margin:0; color:var(--text-faint); font-size:12px; font-weight:400; }
	.brief-scope { display: grid; gap: 5px; }
	.brief-scope dl { display: grid; margin: 0; }
	.brief-scope dl > div { display: grid; grid-template-columns: minmax(76px, .42fr) minmax(0, 1fr); gap: var(--space-3); padding: 10px 4px; border-bottom: 1px solid var(--border-subtle, var(--border)); }
	.brief-scope dl > div:last-child { border-bottom: 0; }
	.brief-scope dt { color: var(--text-faint); font-size: var(--fs-xs); }
	.brief-scope dd { min-width: 0; margin: 0; color: var(--text-dim); font-size: var(--fs-xs); line-height: 1.5; overflow-wrap: anywhere; }
	.brief-source-note { color: var(--text-faint); font-size: var(--fs-xs); }
	.brief-source-note summary { width: fit-content; cursor: pointer; }
	.brief-source-note p { margin-top: var(--space-2); line-height: 1.5; }
	.brief-edit-panel { display: grid; gap: var(--space-1); padding: var(--space-4); margin-top: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-lg); background: var(--bg-inset); }
	.brief-edit-actions { display: flex; align-items: center; justify-content: flex-end; gap: var(--space-2); padding-top: var(--space-3); margin-top: var(--space-2); border-top: 1px solid var(--border); }
	.brief-edit-actions > span { flex: 1; color: var(--text-faint); font-size: var(--fs-xs); }
	.brief-edit-actions > span[role='alert'] { color: var(--danger, #b42318); }
	.brief-proposal { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: center; gap: var(--space-3); padding: var(--space-3); margin-top: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg-inset); }
	.proposal-copy { grid-column: 1 / -1; }
	.proposal-copy h3 { margin: 0; font-size: var(--fs-sm); font-weight: 650; }
	.proposal-copy p { margin-top: 3px; color: var(--text-dim); font-size: var(--fs-xs); }
	.brief-proposal label { margin: 0; font-size: var(--fs-xs); }
	.brief-proposal :global(.fella-ui-select-trigger) { min-width: 190px; }
	.brief-proposal :global(.proposal-field) { grid-column: 1 / -1; min-height: 78px; }
	.proposal-actions { grid-column: 1 / -1; display: flex; justify-content: flex-end; gap: var(--space-2); }
	h2 { font-size: var(--fs-lg); font-weight: 650; }
	.brief-editor label { display: block; margin: var(--space-4) 0 var(--space-2); font-size: var(--fs-sm); font-weight: 600; }
	.brief-editor :global(.brief-field) { width: 100%; min-height: 74px; resize: vertical; }
	.brief-editor :global(.goal-field) { min-height: 112px; }
	.brief-editor :global(.constraints-field) { min-height: 96px; }
	.analyst-work-plan { display: grid; gap: var(--space-4); padding: 12px 0 20px; }
	.optional-context-heading { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); color: var(--text-faint); font-size: var(--fs-xs); }
	.optional-context-heading > p { font-weight: 600; }
	.definition-disclosure, .investigation-history { min-width: 0; border: 1px solid color-mix(in srgb, var(--border) 72%, transparent); border-radius: var(--radius-window); background: var(--bg-raised); }
	.definition-disclosure > summary, .investigation-history > summary { display: flex; min-height: 42px; align-items: center; justify-content: space-between; gap: var(--space-3); padding: 0 var(--space-3); color: var(--text); font-size: var(--fs-sm); font-weight: 600; cursor: pointer; list-style: none; }
	.definition-disclosure > summary::-webkit-details-marker, .investigation-history > summary::-webkit-details-marker { display: none; }
	.definition-disclosure > summary::before, .investigation-history > summary::before { content: '›'; flex: 0 0 auto; color: var(--text-faint); font-size: 1.2em; font-weight: 400; line-height: 1; transition: transform 120ms ease; }
	.definition-disclosure[open] > summary::before, .investigation-history[open] > summary::before { transform: rotate(90deg); }
	.definition-disclosure > summary > span, .investigation-history > summary > span { flex: 1; }
	.definition-disclosure > summary small, .investigation-history > summary small { color: var(--text-faint); font-size: var(--fs-xs); font-weight: 400; }
	.definition-editor { display: grid; gap: var(--space-3); padding: 0 var(--space-3) var(--space-3); }
	.definition-empty { margin-top: var(--space-1); color: var(--text-dim); font-size: var(--fs-xs); }
	.definition-list { display: grid; gap: var(--space-3); margin: 0; padding: 0; list-style: none; }
	.definition-item { display: grid; gap: var(--space-2); min-width: 0; padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); }
	.definition-item-heading, .definition-item-meta { display: flex; align-items: center; justify-content: space-between; gap: var(--space-2); }
	.definition-item-heading label { margin: 0; font-size: var(--fs-sm); font-weight: 600; }
	.definition-item-meta { color: var(--text-faint); font-size: var(--fs-xs); }
	.definition-item-meta :global(button) { min-height: 30px; }
	.definition-item :global(.fella-ui-select-trigger) { width: max-content; min-width: 140px; }
	.brief-editor :global(.definition-field) { width: 100%; min-height: 68px; resize: vertical; }
	.definition-empty { margin: 0; }
	.question-editor { display: grid; gap: 8px; padding: 0; margin-top: 0; }
	.question-heading, .question-save, .question-item-heading, .question-item-actions { display: flex; align-items: center; justify-content: space-between; gap: var(--space-2); }
	.question-heading { align-items: center; gap: var(--space-4); }
	.question-heading > div:first-child { min-width: 0; flex: 1; }
	.question-heading h3 { margin: 0; font-size: var(--fs-md); font-weight: 650; }
	.question-heading p { margin-top: var(--space-1); color: var(--text-dim); font-size: var(--fs-xs); line-height: 1.45; }
	.question-save { width: auto; min-width: 0; max-width: 50%; min-height: 20px; flex: 0 1 auto; justify-content: flex-end; }
	.question-save > span { display: block; min-width: 0; min-height: 18px; overflow: hidden; color: var(--text-faint); font-size: var(--fs-xs); line-height: 18px; text-overflow: ellipsis; white-space: nowrap; }
	.question-plan-actions { display: flex; align-items: center; justify-content: flex-start; flex-wrap: wrap; gap: var(--space-2); }
	.question-coverage { display: grid; gap: var(--space-3); padding: 0; margin-top: var(--space-1); border: 0; border-radius: 0; background: transparent; }
	.question-coverage > header { display: flex; align-items: center; justify-content: space-between; gap: var(--space-4); }
	.question-coverage h4 { margin: 0; font-size: var(--fs-sm); font-weight: 650; }
	.question-coverage-message { margin: 3px 0 0; color: var(--text-dim); font-size: var(--fs-xs); }
	.question-coverage-counts { display: grid; grid-template-columns: repeat(4, minmax(64px, auto)); gap: var(--space-2); margin: 0; }
	.question-coverage-counts > div { display: grid; gap: 2px; }
	.question-coverage-counts dt { color: var(--text-faint); font-size: 10px; }
	.question-coverage-counts dd { margin: 0; font-size: var(--fs-sm); font-weight: 650; font-variant-numeric: tabular-nums; }
	.question-coverage-message { margin: 0; }
	.question-coverage-review { min-width: 0; }
	.question-coverage-review > summary { width: fit-content; color: var(--text-dim); font-size: var(--fs-xs); cursor: pointer; }
	.question-coverage-review ul { display: grid; max-height: 240px; gap: var(--space-1); overflow: auto; overscroll-behavior: contain; margin: var(--space-2) 0 0; padding: 0; list-style: none; }
	.question-coverage-review button { display: grid; width: 100%; min-width: 0; justify-items: start; gap: 2px; padding: var(--space-2); border: 1px solid transparent; border-radius: var(--radius-sm); background: transparent; color: var(--text); text-align: left; cursor: pointer; }
	.question-coverage-review button:hover { border-color: var(--border); background: var(--bg); }
	.coverage-question-status { color: var(--text-dim); font-size: 10px; font-weight: 650; }
	.coverage-question-status[data-question-status='incomplete'], .coverage-question-status[data-question-status='unsupported'], .coverage-question-status[data-question-status='insufficient_data'], .coverage-question-status[data-question-status='outdated'] { color: var(--warn); }
	.coverage-question-status[data-question-status='blocked'] { color: var(--err); }
	.question-coverage-review strong { min-width: 0; font-size: var(--fs-xs); font-weight: 600; overflow-wrap: anywhere; }
	.question-coverage-review small { color: var(--text-faint); font-size: 11px; overflow-wrap: anywhere; }
	.subquestion-title { margin: 0; font-size: var(--fs-sm); font-weight: 650; }
	.question-list { display: grid; gap: 0; margin: 0; padding: 0; list-style: none; }
	.question-item { min-width: 0; padding: 12px 4px; border: 0; border-bottom: 1px solid var(--border-subtle, var(--border)); background: transparent; }
	.question-item:last-child { border-bottom: 0; }
	.question-item-heading { flex-wrap: wrap; justify-content: flex-start; margin-bottom: var(--space-2); }
	.question-item-heading label { min-width: 0; flex: 1 1 100px; margin: 0; font-size: var(--fs-xs); font-weight: 600; color: var(--text-faint); }
	.question-status-reason { margin: 0 0 var(--space-2); color: var(--text-dim); font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.question-status-reason[data-question-status='blocked'] { color: var(--err); }
	.question-stale-review { display: grid; gap: var(--space-2); margin: var(--space-2) 0; color: var(--text-dim); font-size: var(--fs-xs); line-height: 1.45; }
	.question-stale-review p { margin: 0; }
	.question-stale-review summary { width: fit-content; color: var(--text); cursor: pointer; }
	.question-stale-review ul { display: grid; gap: 2px; margin: var(--space-1) 0 0; padding-left: var(--space-5); }
	.question-item-actions { gap: 4px; margin-left: auto; }
	.question-item-actions :global(button) { min-width: 30px; min-height: 30px; padding: 4px 8px; }
	:global(.question-more) { display: inline-flex; width: 30px; min-width: 30px !important; align-items: center; justify-content: center; border: 0; border-radius: 999px; background: transparent; color: var(--text-faint); cursor: pointer; }
	:global(.question-more:hover) { background: var(--bg-inset); color: var(--text); }
	.brief-editor :global(.question-field) { width: 100%; min-height: 68px; resize: vertical; }
	.question-empty-state { display: flex; min-width: 0; align-items: center; justify-content: space-between; gap: var(--space-4); padding: var(--space-4); border: 1px dashed var(--border-strong, var(--border)); border-radius: var(--radius-md); background: var(--bg-inset); }
	.question-empty-state > div { min-width: 0; }
	.question-empty-state strong { font-size: var(--fs-sm); font-weight: 600; }
	.question-run-links { display: flex; align-items: center; gap: var(--space-1); margin: 0 0 var(--space-2); color: var(--text-faint); font-size: var(--fs-xs); }
	.question-run-links > span { margin-right: var(--space-1); }
	.question-run-links :global(button) { min-height: 28px; padding-inline: var(--space-2); }
	.findings { display: grid; gap: var(--space-4); padding: var(--space-5) 0; margin-top: var(--space-4); }
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
	.analysis-runs { display: grid; gap: var(--space-4); padding: var(--space-5) 0; margin-top: var(--space-4); }
	.analysis-runs-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--space-3); }
	.analysis-runs-heading h3, .analysis-run-detail h4 { margin: 0; font-size: var(--fs-md); font-weight: 650; }
	.analysis-runs-heading p { margin: var(--space-1) 0 0; color: var(--text-dim); font-size: var(--fs-xs); }
	.analysis-runs-heading > span { color: var(--text-faint); font-size: var(--fs-xs); white-space: nowrap; }
	.analysis-run-list { display: grid; gap: var(--space-1); margin: 0; padding: 0; list-style: none; }
	.analysis-run-link { display: flex; width: 100%; min-width: 0; align-items: flex-start; gap: var(--space-3); padding: var(--space-2) var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg); color: var(--text); text-align: left; cursor: pointer; }
	.analysis-run-link:hover, .analysis-run-link[aria-current='true'] { background: var(--bg-inset); }
	.analysis-run-number { flex: none; color: var(--text-faint); font-size: var(--fs-xs); }
	.analysis-run-copy { display: grid; min-width: 0; flex: 1; gap: 2px; }
	.analysis-run-copy strong { font-size: var(--fs-sm); font-weight: 600; overflow-wrap: anywhere; }
	.analysis-run-copy small { color: var(--text-faint); font-size: var(--fs-xs); }
	.analysis-run-link > :global(svg) { flex: none; margin-top: 3px; color: var(--text-faint); }
	.analysis-runs-empty { margin: 0; color: var(--text-faint); font-size: var(--fs-xs); }
	.analysis-run-detail { display: grid; gap: var(--space-3); min-width: 0; padding: var(--space-3); border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bg-inset); }
	.analysis-run-facts { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-2) var(--space-4); margin: 0; }
	.analysis-run-facts > div, .analysis-run-question-snapshot > div { min-width: 0; }
	.analysis-run-facts dt, .analysis-run-question-snapshot dt { color: var(--text-faint); font-size: var(--fs-xs); }
	.analysis-run-facts dd, .analysis-run-question-snapshot dd { margin: 2px 0 0; font-size: var(--fs-sm); overflow-wrap: anywhere; }
	.analysis-run-facts code { font-size: var(--fs-xs); }
	.analysis-run-facts :global(.analysis-run-parent-link) { min-height: 0; padding: 0; font-size: var(--fs-xs); }
	.analysis-run-question-snapshot { padding-top: var(--space-2); margin: 0; border-top: 1px solid var(--border); }
	.analysis-run-context { display: grid; gap: var(--space-2); padding: var(--space-2) 0 0; border: 0; border-radius: 0; background: transparent; }
	.analysis-run-context > header h5, .analysis-run-context-definitions h6, .analysis-run-evidence h5 { margin: 0; color: var(--text); font-size: var(--fs-sm); font-weight: 650; }
	.analysis-run-context > header p, .analysis-run-evidence header p { margin: 3px 0 0; color: var(--text-dim); font-size: var(--fs-xs); }
	.analysis-run-context-facts { display: grid; gap: var(--space-2); margin: 0; }
	.analysis-run-context-facts > div { display: grid; grid-template-columns: 150px minmax(0, 1fr); gap: var(--space-2); }
	.analysis-run-context-facts dt { color: var(--text-faint); font-size: var(--fs-xs); }
	.analysis-run-context-facts dd { min-width: 0; margin: 0; font-size: var(--fs-xs); overflow-wrap: anywhere; }
	.analysis-run-context-list, .analysis-run-context-definitions ul { display: grid; gap: var(--space-1); margin: 0; padding-left: var(--space-4); }
	.analysis-run-context-list li { display: grid; gap: 2px; }
	.analysis-run-context-list small { color: var(--text-faint); font-size: 11px; }
	.analysis-run-source-provenance { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--space-2); }
	.analysis-run-source-state { display: grid; align-content: start; gap: var(--space-2); min-width: 0; padding: var(--space-1) 0; border: 0; border-radius: 0; background: transparent; }
	.analysis-run-source-state h6 { margin: 0; color: var(--text); font-size: var(--fs-xs); font-weight: 650; }
	.analysis-run-source-state .analysis-run-context-facts > div { grid-template-columns: minmax(110px, 0.4fr) minmax(0, 1fr); }
	.analysis-run-context-definitions { display: grid; gap: var(--space-2); padding-top: var(--space-2); border-top: 1px solid var(--border); }
	.analysis-run-context-definitions li { display: grid; gap: 2px; }
	.analysis-run-context-definitions strong { font-size: var(--fs-xs); font-weight: 600; }
	.analysis-run-context-definitions li span, .analysis-run-context-definitions p { margin: 0; color: var(--text-dim); font-size: var(--fs-xs); white-space: pre-wrap; overflow-wrap: anywhere; }
	.analysis-run-evidence { display: grid; gap: var(--space-2); min-width: 0; }
	.analysis-run-evidence :global(.evidence-body) { margin-top: 0; }
	.analysis-run-assumptions { display: grid; gap: var(--space-2); padding: var(--space-2) 0 0; border: 0; border-radius: 0; background: transparent; }
	.analysis-run-assumptions h5 { margin: 0; color: var(--text); font-size: var(--fs-xs); font-weight: 650; }
	.analysis-run-assumptions p { margin: 3px 0 0; color: var(--text-dim); font-size: var(--fs-xs); }
	.analysis-run-assumptions ul { display: grid; gap: var(--space-2); margin: 0; padding: 0; list-style: none; }
	.analysis-run-assumptions li { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--space-3); }
	.analysis-run-assumptions li > span { min-width: 0; flex: 1; font-size: var(--fs-sm); overflow-wrap: anywhere; }
	.analysis-run-assumptions li :global(button) { flex: none; }
	.analysis-run-answer { display: grid; gap: var(--space-2); padding: var(--space-2) 0 0; border: 0; border-radius: 0; background: transparent; }
	.analysis-run-answer h5 { margin: 0; font-size: var(--fs-sm); font-weight: 650; }
	.analysis-run-answer { font-size: var(--fs-sm); }
	.analysis-run-answer small { color: var(--text-faint); font-size: var(--fs-xs); }
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
	.finding-capture :global(.fella-ui-select-trigger) { width: 100%; min-width: 0; font-size: var(--fs-sm); }
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
	.investigation-history { margin-bottom: 0; }
	.coverage-disclosure { margin: 8px 0 12px; color: var(--text-dim); font-size: var(--fs-xs); }
	.coverage-disclosure > summary { padding: 6px 0; cursor: pointer; }
	.investigation-completion-actions { display: flex; align-items: center; flex-wrap: wrap; gap: 12px; padding-top: 20px; }
	.investigation-completion-actions p { flex: 1; min-width: 180px; color: var(--text-dim); font-size: var(--fs-xs); }
	.question-text { display: block; width: 100%; padding: 4px 0; border: 0; background: transparent; color: var(--text); font: inherit; line-height: 1.55; text-align: left; white-space: pre-wrap; overflow-wrap: anywhere; cursor: text; }
	.question-text span { display: inline-block; margin-left: var(--space-2); color: var(--text-faint); font-size: var(--fs-xs); opacity: 0; }
	.question-text:hover span, .question-text:focus-visible span { opacity: 1; }
	.investigation-history-content { display: flex; flex-direction: column; gap: var(--space-4); padding: 8px 0; }
	.analysis-runs { order: -1; }
	.empty-state { max-width: 48ch; margin: 12vh auto 0; display: grid; justify-items: center; gap: var(--space-3); text-align: center; }
	.empty-state > :global(svg) { color: var(--text-faint); }
	.empty-state h2 { font-size: var(--fs-lg); }
	.empty-state p { color: var(--text-dim); }
	@media (max-width: 720px) {
		.brief-summary dl { grid-template-columns: 1fr; }
		.analysis-run-source-provenance { grid-template-columns: 1fr; }
		.analysis-run-source-state .analysis-run-context-facts > div { grid-template-columns: 1fr; gap: 2px; }
		.question-coverage > header { flex-direction: column; }
		.question-coverage-counts { width: 100%; grid-template-columns: repeat(4, minmax(0, 1fr)); }
	}
	@container investigation-pane (max-width: 900px) {
		.investigation-page { padding: 20px; }
		.page-head { align-items: flex-start; flex-direction: row; }
		.investigation-document-heading { align-items: flex-start; flex-direction: column; }
		.question-heading { align-items: center; flex-direction: row; gap: var(--space-2); }
		.question-coverage > header { flex-direction: column; }
		.question-coverage-counts { width: 100%; grid-template-columns: repeat(4, minmax(0, 1fr)); }
		.question-save { width: auto; min-width: 0; max-width: 50%; min-height: 20px; flex: 0 1 auto; justify-content: flex-end; }
		.question-plan-actions { justify-content: flex-start; }
		.question-empty-state { align-items: flex-start; flex-direction: column; }
	}
	/* Reference layout: fixed identity and section navigation, one inner scrollport. */
	.investigation-page { display:flex; flex-direction:column; flex:1; width:100%; max-width:none; height:100%; min-height:0; padding:0; margin:0; overflow:hidden; background:var(--bg); }
	.page-head { display:block; position:relative; flex:none; margin:0; padding:0 16px; background:var(--bg); }
	.notebook-panel-bar { display:flex; height:52px; padding-top:16px; align-items:center; justify-content:space-between; -webkit-app-region:drag; }
	.notebook-panel-bar :global(button) { -webkit-app-region:no-drag; }
	:global(.notebook-back) { display:inline-flex; min-height:32px; align-items:center; gap:4px; padding:6px 8px; border-radius:999px; color:var(--text-dim); font-size:12.5px; font-weight:400; }
	:global(.notebook-back) :global(svg) { transform:rotate(180deg); }
	:global(.notebook-back:hover), .notebook-panel-actions :global(button:hover) { background:var(--bg-raised-2); color:var(--text); }
	.notebook-panel-actions { display:flex; align-items:center; gap:8px; }
	:global(.notebook-share) { display:inline-flex; min-height:36px; align-items:center; gap:8px; padding:8px 16px; border-radius:999px; background:var(--bg-raised); color:var(--text); font-size:13px; font-weight:400; }
	:global(.notebook-close) { width:36px; height:36px; min-height:36px; border-radius:50%; background:var(--bg-raised); color:var(--text-dim); }
	.investigation-identity { display:flex; flex-direction:column; align-items:center; padding:20px 0 0; text-align:center; }
	.investigation-mark { display:grid; width:48px; height:48px; place-items:center; border-radius:50%; background:#fbbf24; color:#18181b; }
	.investigation-title-row { position:relative; display:flex; width:100%; min-height:23px; align-items:center; justify-content:center; margin-top:12px; }
	.investigation-title-row h1 { flex:0 1 auto; max-width:calc(100% - 64px); margin:0; color:var(--text); font-size:15px; font-weight:400; letter-spacing:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
	.investigation-title-row :global(button) { position:absolute; right:4px; width:25px; height:25px; min-height:25px; color:var(--text-faint); opacity:0; }
	.investigation-title-row:hover :global(button), .investigation-title-row :global(button:focus-visible) { opacity:1; }
	.investigation-identity > p { margin:4px 0 0; color:var(--text-faint); font-size:13px; }
	.investigation-content, .brief-editor { display:flex; flex:1; flex-direction:column; min-width:0; min-height:0; overflow:hidden; }
	.investigation-nav { position:relative; top:auto; z-index:5; display:grid; grid-template-columns:.7fr 1fr 1.2fr 1fr 1fr; flex:none; align-self:center; width:calc(100% - 24px); max-width:348px; justify-content:center; gap:0; margin:20px 12px 0; padding:0; overflow:hidden; scrollbar-width:none; background:var(--bg); }
	.investigation-nav::-webkit-scrollbar { display:none; }
	.investigation-nav > .fella-pill-nav-item { position:relative; flex:none; min-width:0; min-height:27px; padding:4px 10px; font-size:12.5px; font-weight:400; line-height:19px; }
	.investigation-nav > .fella-pill-nav-item[aria-selected='true'] { background:var(--bg-raised-2); color:var(--text); box-shadow:none; }
	.investigation-tab-indicator { position:absolute; top:2px; right:2px; width:6px; height:6px; border-radius:50%; }
	.question-indicator { background:var(--brand-icon); } .results-indicator { background:var(--warn); }
	.investigation-section-scroll { flex:1; min-height:0; min-width:0; overflow-y:auto; overflow-x:hidden; overscroll-behavior:contain; padding:20px 16px 16px; }
	.investigation-tab-panel > .investigation-document-section:first-child, .investigation-tab-panel > .analyst-work-plan:first-child { padding-top:0; }
	.brief-summary { gap:8px; margin:0; padding:0; }
	.brief-summary-header { align-items:center; padding:0 4px; }
	.brief-summary-header :global(button) { min-height:18px; height:18px; padding:0; border:0; background:transparent; color:var(--text-faint); font-size:12px; }
	.brief-objective { display:block; margin:0; padding:14px 16px; border-radius:16px; background:var(--bg-raised); font-size:14.5px; line-height:23px; color:var(--text); }
	.brief-scope { margin-top:16px; }
	.brief-scope dl > div { grid-template-columns:92px minmax(0,1fr); }
	.brief-source-note { margin-top:16px; }
	.question-add { display:flex; min-width:0; align-items:center; gap:8px; padding:6px 6px 6px 14px; margin-bottom:16px; border:1px solid var(--border); border-radius:999px; background:var(--bg-inset); }
	.question-add :global(input) { flex:1; min-height:28px; height:28px; padding:0; border:0; background:transparent; box-shadow:none; font-size:12.5px; }
	.question-add :global(button) { flex:none; width:28px; min-width:28px; height:28px; min-height:28px; padding:0; border-radius:50%; }
	.question-plan-heading h3 { font-size:12px; color:var(--text-dim); font-weight:400; }
	.question-item { padding:0; border:0; }
	.question-row, .root-question-item { border-bottom:1px solid var(--border); }
	.question-row:last-child { border-bottom:0; }
	.question-row-summary { display:grid; grid-template-columns:18px minmax(0,1fr) 12px; gap:8px; padding:12px 4px; list-style:none; cursor:pointer; }
	.question-row-summary::-webkit-details-marker { display:none; }
	.question-row-summary > :global(svg) { align-self:center; color:var(--text-faint); transition:transform .15s; }
	.question-item[open] > .question-row-summary > :global(svg) { transform:rotate(90deg); }
	.question-status-mark { width:8px; height:8px; margin-top:6px; border:1px solid var(--text-faint); border-radius:50%; }
	.question-status-mark[data-status='answered'] { border-color:var(--ok); background:var(--ok); }
	.question-status-mark[data-status='running'] { border-color:var(--brand-icon); background:var(--brand-icon); animation:pulse 1.3s infinite; }
	.question-status-mark[data-status='blocked'], .question-status-mark[data-status='incomplete'], .question-status-mark[data-status='needs_review'], .question-status-mark[data-status='outdated'] { border-color:var(--warn); background:var(--warn); }
	.question-summary-copy { display:grid; min-width:0; gap:4px; color:var(--text); font-size:13px; line-height:19px; overflow-wrap:anywhere; }
	.question-summary-copy > span > small { margin-right:6px; color:var(--text-faint); font:11px var(--mono); }
	.question-summary-status { color:var(--text-faint); font-size:11.5px; }
	.question-excerpt { display:-webkit-box; overflow:hidden; -webkit-line-clamp:2; line-clamp:2; -webkit-box-orient:vertical; color:var(--text-dim); font-size:12px; line-height:18px; }
	.question-expanded { padding:10px; margin:0 4px 12px; border-radius:12px; background:var(--bg-raised); }
	.question-plan-actions { margin-top:16px; }
	.question-plan-actions h4 { width:100%; margin:0; font-size:12px; color:var(--text-dim); font-weight:400; }
	.question-plan-actions :global(button) { min-height:30px; border:0; background:transparent; font-size:12.5px; color:var(--text-dim); }
	@keyframes pulse { 50% { opacity:.45; } }
	@container investigation-pane (max-width:360px) { .page-head { padding-inline:12px; } .investigation-section-scroll { padding-inline:12px; } .investigation-nav > .fella-pill-nav-item { padding-inline:2px; font-size:12px; } }
	.results-records { margin-top:20px; }
	.results-records > summary { width:fit-content; margin-bottom:12px; color:var(--text-dim); font-size:12px; cursor:pointer; }
	.investigation-history { border:0; border-radius:0; background:transparent; }
	.investigation-history > summary { min-height:30px; padding:0 4px; color:var(--text-dim); font-size:12px; font-weight:400; }
</style>
