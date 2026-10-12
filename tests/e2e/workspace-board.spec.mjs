import { expect, test } from '@playwright/test';
import { readFile } from 'node:fs/promises';

const folders = [
	{
		path: 'C:\\FellaFixture\\northwind-sales',
		catalog: {
			workspace: 'C:\\FellaFixture\\northwind-sales',
			revision: 'sales-r1',
			sources: [
				{ name: 'sales.csv', path: 'C:\\FellaFixture\\northwind-sales\\sales.csv', kind: 'csv', view: 'sales', row_count: 124, size_bytes: 4096, mtime: 1 }
			]
		}
	},
	{
		path: 'C:\\FellaFixture\\health-journal',
		catalog: {
			workspace: 'C:\\FellaFixture\\health-journal',
			revision: 'health-r1',
			sources: [
				{ name: 'sleep.csv', path: 'C:\\FellaFixture\\health-journal\\sleep.csv', kind: 'csv', view: 'sleep', row_count: 38, size_bytes: 2048, mtime: 1 }
			]
		}
	},
	...['weekly-budget', 'travel-plans', 'reading-notes'].map((name, index) => ({
		path: `C:\\FellaFixture\\${name}`,
		catalog: {
			workspace: `C:\\FellaFixture\\${name}`,
			revision: `${name}-r1`,
			sources: [
				{ name: `${name}.csv`, path: `C:\\FellaFixture\\${name}\\${name}.csv`, kind: 'csv', view: name.replaceAll('-', '_'), row_count: (index + 1) * 12, size_bytes: 1024, mtime: 1 }
			]
		}
	}))
];

const workspacePresetButton = (page, name) =>
	page.locator('.workspace-preset-control').getByRole('button', { name: new RegExp(`^${name}:`) });

async function expandInvestigationHistory(page) {
	const history = page.locator('.investigation-history');
	if (!(await history.evaluate((element) => element.open))) {
		await history.locator(':scope > summary').click();
	}
}

async function expandInvestigationDefinitions(page) {
	const definitions = page.locator('.definition-disclosure');
	if (!(await definitions.evaluate((element) => element.open))) {
		await definitions.locator(':scope > summary').click();
	}
}

async function expectQuestionRunSettled(page) {
	const timeline = page.getByRole('region', { name: 'Question run' }).last();
	await expect(timeline).toBeVisible();
	await expect(timeline).toHaveAttribute(
		'data-run-state',
		/^(accepted|needs_review|unsupported|failed|cancelled|clarify)$/
	);
	return timeline;
}

async function installDesktopMock(
	page,
	{
		seedUnavailableConversation = false,
		seedDeletionConversations = false,
		fixtureFolders = folders,
		replayStatus = null,
		replayStatusError = false,
		reindexedCatalog = null,
		chartEvidence = null,
		detailEvidence = null,
		directAnswerText = null,
		savedAnalysisTurn = null,
		questionRunOutcomes = [],
		providerList = [],
		providerHealth = { reachable: true, rejected: false, models: ['gpt-4.1-mini'] },
		runLogEntries = []
	} = {}
) {
	await page.addInitScript(({ fixtureFolders, seedUnavailableConversation, seedDeletionConversations, replayStatus, replayStatusError, reindexedCatalog, chartEvidence, detailEvidence, directAnswerText, savedAnalysisTurn, questionRunOutcomes, providerList, providerHealth, runLogEntries }) => {
		const calls = [];
		const reviseFinding = (finding, update) => {
			const now = Math.max(Date.now(), (finding.updated_at_ms ?? 0) + 1);
			const revision = finding.revision ?? 1;
			const previous = {
				revision,
				question_id: finding.question_id,
				analysis_turn_id: finding.analysis_turn_id,
				evidence_ids: finding.evidence_ids,
				claim: finding.claim,
				caveats: finding.caveats ?? [],
				assumptions: finding.assumptions ?? [],
				revision_note: finding.revision_note ?? '',
				created_at_ms: finding.updated_at_ms,
				updated_at_ms: finding.updated_at_ms
			};
			return {
				...finding,
				...update,
				revision: revision + 1,
				revision_note: update.revision_note ?? finding.revision_note ?? '',
				revisions: [...(finding.revisions ?? []), previous],
				updated_at_ms: now
			};
		};
		const archivedConversations = new Map();
		const investigations = new Map();
		const analysisTurns = new Map();
		const questionRecords = new Map();
		const definitionRecords = new Map();
		const findingRecords = new Map();
		const investigationEnsureCalls = [];
		const questionProposalCalls = [];
		const deletedConversationIds = [];
		const deletedConversationWorkspaces = [];
		const unavailablePath = 'C:\\FellaFixture\\unavailable-archive';
		if (seedUnavailableConversation) {
			const id = 'archived-unavailable-workspace';
			const saved = {
				id,
				workspace: unavailablePath,
				title: null,
				messages: [
					{ id: 'archived-user', role: 'user', text: 'Summarize the unavailable archive', ts: 1 },
					{ id: 'archived-assistant', role: 'assistant', text: 'Saved result from this folder.', ts: 2 }
				]
			};
			archivedConversations.set(id, {
				saved,
				summary: {
					id,
					saved_at_ms: 2,
					workspace: unavailablePath,
					preview: 'Summarize the unavailable archive',
					message_count: 2,
					title: null
				}
			});
		}
		if (seedDeletionConversations) {
			const anotherId = 'archived-unavailable-workspace-2';
			const anotherSaved = {
				id: anotherId,
				workspace: unavailablePath,
				title: null,
				messages: [
					{ id: 'archived-user-2', role: 'user', text: 'Review another saved analysis', ts: 3 },
					{ id: 'archived-assistant-2', role: 'assistant', text: 'Another saved result.', ts: 4 }
				]
			};
			archivedConversations.set(anotherId, {
				saved: anotherSaved,
				summary: {
					id: anotherId,
					saved_at_ms: 4,
					workspace: unavailablePath,
					preview: 'Review another saved analysis',
					message_count: 2,
					title: null
				}
			});
			const unrelatedId = 'archived-northwind-sales';
			const unrelatedSaved = {
				id: unrelatedId,
				workspace: fixtureFolders[0].path,
				title: null,
				messages: [
					{ id: 'northwind-user', role: 'user', text: 'Summarize sales', ts: 5 },
					{ id: 'northwind-assistant', role: 'assistant', text: 'Sales summary.', ts: 6 }
				]
			};
			archivedConversations.set(unrelatedId, {
				saved: unrelatedSaved,
				summary: {
					id: unrelatedId,
					saved_at_ms: 6,
					workspace: fixtureFolders[0].path,
					preview: 'Summarize sales',
					message_count: 2,
					title: null
				}
			});
		}
		let pickerIndex = 0;
		const settings = {
			provider: 'openai',
			base_url: 'https://api.openai.com/v1',
			model: 'gpt-4.1-mini',
			embed_model: '',
			has_credential: true,
			capabilities: { table_analysis: true, document_analysis: true, python_analysis: true, visualizations: true }
		};
		window.__workspaceAskCalls = calls;
		window.__workspaceInvestigationEnsureCalls = investigationEnsureCalls;
		window.__workspaceQuestionProposalCalls = questionProposalCalls;
		window.__workspaceInvestigations = () => [...investigations.values()];
		window.__workspaceDefinitionRecords = () => [...definitionRecords.values()];
		window.__workspaceFindingRecords = () => [...findingRecords.values()];
		window.__workspaceSetRunLinks = (id, runIds) => {
			for (const [key, record] of investigations) {
				if (record.id === id) investigations.set(key, { ...record, analysis_turn_ids: [...runIds] });
			}
		};
		window.__workspaceOpenCalls = [];
		window.__deletedConversationIds = deletedConversationIds;
		window.__deletedConversationWorkspaces = deletedConversationWorkspaces;
		window.__remainingArchiveIds = () => [...archivedConversations.keys()];
		window.__replayStatusCalls = 0;
		window.__analysisTurnLoadCalls = [];
		window.fella = {
			invoke: async (command, args = {}) => {
				switch (command) {
					case 'app_ready': return 1;
					case 'investigations_list': return [...investigations.values()];
					case 'investigation_ensure': {
						investigationEnsureCalls.push(args);
						const key = JSON.stringify([args.workspaceId, args.conversationId]);
						if (!investigations.has(key)) {
							const now = Date.now();
							const id = `inv-${investigations.size + 1}`;
							const goal = args.initialRequest.trim();
							const rootQuestion = goal ? {
								id: `q-root-${id}`,
								investigation_id: id,
								parent_id: undefined,
								text: goal,
								position: 0,
								source_selection: null,
								status: 'active',
								answer_status: 'not_started',
								created_at_ms: now,
								updated_at_ms: now
							} : null;
							if (rootQuestion) questionRecords.set(rootQuestion.id, rootQuestion);
							investigations.set(key, {
								id,
								workspace_id: args.workspaceId,
								conversation_id: args.conversationId,
								title: goal.slice(0, 64),
								brief: { goal, audience: '', desired_output: '', constraints: [] },
								questions: rootQuestion ? [rootQuestion] : [],
								definitions: [],
								findings: [],
								source_selection: { mode: 'all_sources', included_paths: [] },
								brief_source_message_id: args.sourceMessageId,
								analysis_turn_ids: [],
								status: 'active',
								created_at_ms: now,
								updated_at_ms: now
							});
						}
						return investigations.get(key);
					}
					case 'investigation_update_brief': {
						for (const [key, record] of investigations) {
							if (record.id !== args.investigationId) continue;
							const updated = {
								...record,
								brief: args.brief,
								brief_reviewed_message_id: args.reviewedMessageId ?? record.brief_reviewed_message_id,
								updated_at_ms: Date.now()
							};
							investigations.set(key, updated);
							return updated;
						}
						throw new Error(`Unknown Investigation: ${args.investigationId}`);
					}
					case 'investigation_mark_complete': {
						for (const [key, record] of investigations) {
							if (record.id !== args.investigationId) continue;
						const questions = (record.questions ?? []).filter((question) => question.status === 'active');
						const hasRecordedGap = (question) =>
							['incomplete', 'unsupported'].includes(question.answer_status)
							&& Boolean(question.answer_status_reason?.trim());
						if (!questions.length || questions.some((question) => question.answer_status !== 'answered' && !hasRecordedGap(question))) {
							throw new Error('Every active Question needs an answer, a documented data gap, or review before this Investigation can be completed.');
						}
						const updated = { ...record, status: 'complete', updated_at_ms: Date.now() };
						investigations.set(key, updated);
						return updated;
						}
						throw new Error(`Unknown Investigation: ${args.investigationId}`);
					}
					case 'investigation_reopen': {
						for (const [key, record] of investigations) {
							if (record.id !== args.investigationId) continue;
						const updated = { ...record, status: 'active', updated_at_ms: Date.now() };
						investigations.set(key, updated);
						return updated;
						}
						throw new Error(`Unknown Investigation: ${args.investigationId}`);
					}
					case 'investigation_update_questions': {
						for (const [key, record] of investigations) {
							if (record.id === args.investigationId) {
								const now = Date.now();
								const nextQuestions = args.questions.map((item, position) => {
									const previous = questionRecords.get(item.id);
									const question = {
										id: item.id,
										investigation_id: record.id,
										parent_id: item.parent_id ?? undefined,
										text: item.text,
										position,
										source_selection: item.source_selection ?? null,
										status: 'active',
										answer_status: previous?.latest_analysis_turn_id && (
											previous.text !== item.text ||
											JSON.stringify(previous.source_selection ?? null) !== JSON.stringify(item.source_selection ?? null) ||
											previous.parent_id !== (item.parent_id ?? undefined)
										) ? 'outdated' : (previous?.answer_status ?? 'not_started'),
										answer_status_reason: previous?.latest_analysis_turn_id && (
											previous.text !== item.text ||
											JSON.stringify(previous.source_selection ?? null) !== JSON.stringify(item.source_selection ?? null) ||
											previous.parent_id !== (item.parent_id ?? undefined)
										) ? 'The Question or its source scope changed. Run it again to refresh the answer.' : previous?.answer_status_reason,
										latest_analysis_turn_id: previous?.latest_analysis_turn_id,
										analysis_turn_ids: previous?.analysis_turn_ids ?? [],
										created_at_ms: previous?.created_at_ms ?? now,
										updated_at_ms: now
									};
									questionRecords.set(question.id, question);
									return question;
								});
								for (const previous of record.questions ?? []) {
									if (!nextQuestions.some((item) => item.id === previous.id)) {
										questionRecords.set(previous.id, { ...previous, status: 'removed', updated_at_ms: now });
									}
								}
								const updated = { ...record, questions: nextQuestions, updated_at_ms: now };
								investigations.set(key, updated);
								return updated;
							}
						}
						throw new Error(`Unknown Investigation: ${args.investigationId}`);
					}
					case 'investigation_set_analysis_turn_questions': {
						const run = analysisTurns.get(args.analysisTurnId);
						if (!run) throw new Error(`Unknown analysis turn: ${args.analysisTurnId}`);
						for (const [key, record] of investigations) {
							if (record.id !== args.investigationId) continue;
							const desired = new Set(args.questionIds);
							const now = Date.now();
							const nextQuestions = (record.questions ?? []).map((question) => {
								const links = new Set(question.analysis_turn_ids ?? []);
								if (desired.has(question.id) || question.latest_analysis_turn_id === args.analysisTurnId) links.add(args.analysisTurnId);
								else links.delete(args.analysisTurnId);
								const updated = { ...question, analysis_turn_ids: [...links], updated_at_ms: now };
								questionRecords.set(updated.id, updated);
								return updated;
							});
							const runIds = new Set(record.analysis_turn_ids ?? []);
							runIds.add(args.analysisTurnId);
							const updated = { ...record, questions: nextQuestions, analysis_turn_ids: [...runIds], updated_at_ms: now };
							investigations.set(key, updated);
							return updated;
						}
						throw new Error(`Unknown Investigation: ${args.investigationId}`);
					}
					case 'investigation_link_analysis_turns': {
						for (const [key, record] of investigations) {
							if (record.id !== args.investigationId) continue;
							const updated = {
								...record,
								analysis_turn_ids: [...new Set([...(record.analysis_turn_ids ?? []), ...args.analysisTurnIds])],
								updated_at_ms: Date.now()
							};
							investigations.set(key, updated);
							return updated;
						}
						throw new Error(`Unknown Investigation: ${args.investigationId}`);
					}
					case 'investigation_question_load': {
						const question = questionRecords.get(args.questionId);
						if (!question) throw new Error(`Unknown Question: ${args.questionId}`);
						return question;
					}
					case 'investigation_update_definitions': {
						for (const [key, record] of investigations) {
							if (record.id !== args.investigationId) continue;
							const now = Date.now();
							const next = args.definitions.map((item) => {
								const previous = definitionRecords.get(item.id);
								const changed = previous && (previous.kind !== item.kind || previous.text !== item.text || previous.status !== 'active');
								const history = [...(previous?.history ?? [])];
								if (changed) history.push({ revision: previous.revision, kind: previous.kind, text: previous.text, recorded_at_ms: previous.updated_at_ms });
								const definition = {
									id: item.id,
									investigation_id: record.id,
									kind: item.kind,
									text: item.text,
									revision: previous ? previous.revision + (changed ? 1 : 0) : 1,
									history,
									status: 'active',
									created_at_ms: previous?.created_at_ms ?? now,
									updated_at_ms: changed ? now : (previous?.updated_at_ms ?? now)
								};
								definitionRecords.set(item.id, definition);
								return definition;
							});
							for (const previous of record.definitions ?? []) {
								if (next.some((item) => item.id === previous.id)) continue;
								const removed = {
									...previous,
									status: 'removed',
									revision: previous.revision + 1,
									history: [...(previous.history ?? []), { revision: previous.revision, kind: previous.kind, text: previous.text, recorded_at_ms: previous.updated_at_ms }],
									updated_at_ms: now
								};
								definitionRecords.set(previous.id, removed);
							}
							const updated = { ...record, definitions: next, updated_at_ms: now };
							investigations.set(key, updated);
							return updated;
						}
						throw new Error(`Unknown Investigation: ${args.investigationId}`);
					}
					case 'investigation_update_source_selection': {
						for (const [key, record] of investigations) {
							if (record.id !== args.investigationId) continue;
							const updated = {
								...record,
								source_selection: args.sourceSelection,
								updated_at_ms: Date.now()
							};
							investigations.set(key, updated);
							return updated;
						}
						throw new Error(`Unknown Investigation: ${args.investigationId}`);
					}
					case 'investigation_definition_load': {
						const definition = definitionRecords.get(args.definitionId);
						if (!definition) throw new Error(`Unknown definition: ${args.definitionId}`);
						return definition;
					}
					case 'investigation_create_finding': {
						const findingInput = args.finding;
						const run = analysisTurns.get(findingInput.analysis_turn_id);
						if (!run) throw new Error(`Unknown analysis turn: ${findingInput.analysis_turn_id}`);
						const available = new Set(run.result.evidence.map((item) => item?.id).filter(Boolean));
						if (findingInput.evidence_ids.some((id) => !available.has(id))) throw new Error('Finding evidence must belong to its linked run.');
						for (const [key, record] of investigations) {
							if (record.id !== args.investigationId) continue;
							if (!(record.analysis_turn_ids ?? []).includes(findingInput.analysis_turn_id)) throw new Error('Finding run is not linked to this Investigation.');
							if (!(record.questions ?? []).some((question) => question.id === findingInput.question_id)) throw new Error('Finding Question is not active in this Investigation.');
							const now = Date.now();
							const finding = { id: `finding-${findingRecords.size + 1}`, investigation_id: record.id, caveats: [], assumptions: [], ...findingInput, revision: 1, revision_note: '', revisions: [], created_at_ms: now, updated_at_ms: now };
							findingRecords.set(finding.id, finding);
							const updated = { ...record, findings: [...(record.findings ?? []), finding], updated_at_ms: now };
							investigations.set(key, updated);
							return updated;
						}
						throw new Error(`Unknown Investigation: ${args.investigationId}`);
					}
					case 'investigation_update_finding_claim': {
						for (const [key, record] of investigations) {
							if (record.id !== args.investigationId) continue;
							const finding = findingRecords.get(args.findingId);
							if (!finding || finding.investigation_id !== record.id) throw new Error('Finding does not belong to this Investigation.');
							const updatedFinding = reviseFinding(finding, { claim: args.claim });
							const now = updatedFinding.updated_at_ms;
							findingRecords.set(args.findingId, updatedFinding);
							const updated = { ...record, findings: (record.findings ?? []).map((item) => item.id === args.findingId ? updatedFinding : item), updated_at_ms: now };
							investigations.set(key, updated);
							return updated;
						}
						throw new Error(`Unknown Investigation: ${args.investigationId}`);
					}
					case 'investigation_update_finding': {
						for (const [key, record] of investigations) {
							if (record.id !== args.investigationId) continue;
							const finding = findingRecords.get(args.findingId);
							if (!finding || finding.investigation_id !== record.id) throw new Error('Finding does not belong to this Investigation.');
							const updatedFinding = reviseFinding(finding, args.update);
							const now = updatedFinding.updated_at_ms;
							findingRecords.set(args.findingId, updatedFinding);
							const updated = { ...record, findings: (record.findings ?? []).map((item) => item.id === args.findingId ? updatedFinding : item), updated_at_ms: now };
							investigations.set(key, updated);
							return updated;
						}
						throw new Error(`Unknown Investigation: ${args.investigationId}`);
					}
					case 'investigation_finding_load': {
						const finding = findingRecords.get(args.findingId);
						if (!finding) throw new Error(`Unknown Finding: ${args.findingId}`);
						return finding;
					}
					case 'investigation_propose_questions':
						questionProposalCalls.push(args);
						return { subquestions: ['Compare conversion by channel', 'Check the trend over time'] };
					case 'get_settings': return settings;
					case 'list_providers': return providerList;
					case 'provider_health': return providerHealth;
					case 'get_catalog': return { workspace: null, sources: [] };
					case 'sample_source': return {
						columns: ['order_id', 'region'],
						rows: [[101, 'North'], [102, 'South']],
						row_count: 2,
						ms: 1,
						truncated: false
					};
					case 'last_workspace_path': return null;
					case 'conversations_list':
						return [...archivedConversations.values()].map(({ summary }) => summary);
					case 'archive_conversation': {
						const saved = JSON.parse(args.body);
						const firstQuestion = saved.messages?.find((message) =>
							message.role === 'user' && message.text?.trim() && !message.text.trimStart().startsWith('/')
						);
						archivedConversations.set(args.id, {
							saved,
							summary: {
								id: args.id,
								saved_at_ms: saved.saved_at_ms,
								workspace: saved.workspace ?? null,
								first_question_message_id: firstQuestion?.id,
								preview: firstQuestion?.text ?? 'New conversation',
								message_count: saved.messages?.length ?? 0,
								title: saved.title ?? null
							}
						});
						return 'fixture-archive';
					}
					case 'conversation_load': {
						const saved = archivedConversations.get(args.id)?.saved;
						if (!saved) throw new Error(`Unknown archived conversation: ${args.id}`);
						return JSON.stringify(saved);
					}
					case 'delete_conversation': {
						const archived = archivedConversations.get(args.id);
						if (!archived) throw new Error(`Unknown archived conversation: ${args.id}`);
						deletedConversationWorkspaces.push({ id: args.id, workspace: archived.summary.workspace });
						archivedConversations.delete(args.id);
						deletedConversationIds.push(args.id);
						return null;
					}
					case 'close_workspace': return null;
					case 'analysis_turn_replay_status':
						window.__replayStatusCalls += 1;
						if (replayStatusError) throw new Error('Replay lookup unavailable.');
						return replayStatus;
					case 'analysis_turn_load':
						window.__analysisTurnLoadCalls.push(args.turnId);
						if (savedAnalysisTurn?.id === args.turnId) return savedAnalysisTurn;
						if (analysisTurns.has(args.turnId)) return analysisTurns.get(args.turnId);
						throw new Error(`Unknown analysis turn: ${args.turnId}`);
					case 'reindex': return reindexedCatalog;
					case 'run_log_recent': return runLogEntries;
					default: return null;
				}
			},
			openWorkspace: async (path) => {
				window.__workspaceOpenCalls.push(path);
				const folder = fixtureFolders.find((entry) => entry.path === path);
				if (!folder) throw new Error(`Unexpected fixture folder: ${path}`);
				return folder.catalog;
			},
			pickFolder: async () => fixtureFolders[pickerIndex++]?.path ?? null,
			ask: async (params, onEvent) => {
				calls.push(params);
				let questionTurnId;
				let questionParentTurnId;
				const questionTurnNumber = calls.filter((call) => call.questionId).length;
				const questionOutcome = questionRunOutcomes[questionTurnNumber - 1] ?? { status: 'answered' };
				const finalState = questionOutcome.status === 'answered'
					? 'accepted'
					: questionOutcome.status === 'blocked' ? 'failed' : 'needs_review';
				if (params.questionId && params.investigationId) {
					const question = questionRecords.get(params.questionId);
					questionParentTurnId = question?.latest_analysis_turn_id;
					questionTurnId = `fixture-question-turn-${questionTurnNumber}`;
					if (question) {
						const running = {
							...question,
							answer_status: 'running',
							answer_status_reason: undefined,
							latest_analysis_turn_id: questionTurnId
						};
						questionRecords.set(question.id, running);
						for (const [key, record] of investigations) {
							if (record.id === params.investigationId) {
								investigations.set(key, {
									...record,
									questions: (record.questions ?? []).map((item) => item.id === question.id ? running : item)
								});
							}
						}
					}
				}
				const answerVerificationStatus = questionTurnId
					? questionOutcome.verification_status ?? (questionOutcome.status === 'answered'
						? 'verified'
						: questionOutcome.status === 'blocked' ? 'failed' : 'insufficient_data')
					: undefined;
				const progressEvidence = questionTurnId
					? {
						id: `fixture-progress-${questionTurnNumber}`,
						tool: 'grep_files',
						args: {},
						result_summary: 'Found relevant passages in the available documents.',
						ms: 4,
						sources: []
					}
					: null;
				if (questionTurnId) {
					onEvent({ kind: 'assistant_delta', text: 'Private model scratch: test whether this text leaks into the run timeline.' });
					onEvent({ kind: 'turn_state', turn_id: questionTurnId, state: 'interpreting' });
					onEvent({ kind: 'turn_state', turn_id: questionTurnId, state: 'planning' });
					onEvent({
						kind: 'tool_start',
						tool: 'grep_files',
						args: { note: 'Search the workspace for implementation details' }
					});
					onEvent({
						kind: 'tool_end',
						item: progressEvidence
					});
					onEvent({ kind: 'turn_state', turn_id: questionTurnId, state: 'verifying' });
					if (questionOutcome.delay_ms) {
						await new Promise((resolve) => setTimeout(resolve, questionOutcome.delay_ms));
					}
					const question = questionRecords.get(params.questionId);
					if (question) {
						const finished = {
							...question,
							answer_status: questionOutcome.status ?? 'answered',
							answer_status_reason: questionOutcome.reason,
							latest_analysis_turn_id: questionTurnId,
							analysis_turn_ids: [...new Set([...(question.analysis_turn_ids ?? []), questionTurnId])]
						};
						questionRecords.set(question.id, finished);
						for (const [key, record] of investigations) {
							if (record.id === params.investigationId) {
								investigations.set(key, {
									...record,
									analysis_turn_ids: [...new Set([...(record.analysis_turn_ids ?? []), questionTurnId])],
									questions: (record.questions ?? []).map((item) => item.id === question.id ? finished : item)
								});
							}
						}
					}
					onEvent({ kind: 'turn_state', turn_id: questionTurnId, state: finalState });
				}
				const hasReplayEvidence = replayStatus !== null || replayStatusError;
				const evidence = [
					...(hasReplayEvidence
						? [{ id: 'fixture-replay-step', tool: 'run_sql', args: {}, result_summary: 'One row inspected.', ms: 12 }]
						: []),
					...(Array.isArray(questionOutcome.evidence) ? questionOutcome.evidence : []),
					...(detailEvidence ? [detailEvidence] : []),
					...(Array.isArray(chartEvidence) ? chartEvidence : chartEvidence ? [chartEvidence] : [])
				];
				const answerTrace = {
					id: `fixture-trace-${questionTurnNumber || calls.length}`,
					turn_id: questionTurnId ?? (hasReplayEvidence ? 'fixture-turn' : `fixture-ask-${calls.length}`),
					steps: evidence.filter((item) => item.id).map((item) => ({
						id: item.id,
						operation: item.tool,
						duration_ms: item.ms ?? 4,
						success: !item.error,
						summary: item.result_summary,
						sources: (item.sources ?? []).map((source) => source.table ? `${source.source} · ${source.table}` : source.source)
					}))
				};
				const answer = {
					text: questionTurnId && questionOutcome.answer_text
						? questionOutcome.answer_text
						: directAnswerText ?? `Scoped to ${params.workspaceId ?? 'general'}.`,
					status: answerVerificationStatus,
					turn_id: questionTurnId ?? (hasReplayEvidence ? 'fixture-turn' : undefined),
					trace: answerTrace,
					evidence,
					verification: questionOutcome.checks ?? [],
					workspace: params.workspaceId ? { path: params.workspaceId, revision: 'fixture-r1' } : null
				};
				if (questionTurnId) {
					const investigation = [...investigations.values()].find((item) => item.id === params.investigationId);
					const question = questionRecords.get(params.questionId);
					const questionWorkspaceRevision = questionOutcome.workspace_revision ??
						fixtureFolders.find((folder) => folder.path === params.workspaceId)?.catalog.revision;
					const verificationStatus = answerVerificationStatus ?? 'verified';
					const verificationChecks = questionOutcome.checks ?? [];
					analysisTurns.set(questionTurnId, {
						id: questionTurnId,
						conversation_id: params.conversationId ?? 'fixture-conversation',
						question: params.question,
						workspace: params.workspaceId,
						workspace_revision: questionWorkspaceRevision,
						contract: questionOutcome.contract,
						source_scope: questionOutcome.source_scope,
						investigation_context: {
							investigation_id: params.investigationId,
							question_id: params.questionId,
							question_text: question?.text ?? params.question,
							definitions: investigation?.definitions ?? [],
							source_selection: question?.source_selection ?? investigation?.source_selection ?? { mode: 'all_sources', included_paths: [] }
						},
						rerun_of: questionOutcome.rerun_of ?? questionParentTurnId,
						state: finalState,
						plan: questionOutcome.plan ?? { strategy: 'direct_tools', steps: ['run_sql'] },
						trace: { ...answerTrace, workspace_revision: questionWorkspaceRevision },
						verification: { status: verificationStatus, checks: verificationChecks },
						result: { text: answer.text, status: verificationStatus, verification: verificationChecks, evidence: answer.evidence }
					});
				}
				onEvent({ kind: 'answer_done', answer });
				return answer;
			},
			rerunAnalysisTurn: async () => { throw new Error('Rerun is not part of this journey'); },
			setWindowAppearance: async () => {},
			openExternal: async () => {},
			windowAction: async () => {},
			pathForFile: () => ''
		};
	}, { fixtureFolders, seedUnavailableConversation, seedDeletionConversations, replayStatus, replayStatusError, reindexedCatalog, chartEvidence, detailEvidence, directAnswerText, savedAnalysisTurn, questionRunOutcomes, providerList, providerHealth, runLogEntries });
}

async function dispatchWorkspaceDrag(page, type, workspacePath, { x = 0.5, y = 0.5, existing = false } = {}) {
	await page.evaluate(({ eventType, path, targetX, targetY, alreadyOpen }) => {
		const board = document.querySelector('[aria-label="Workspace board"]');
		if (!board) throw new Error('The workspace board is not open');
		const rect = board.getBoundingClientRect();
		const dataTransfer = new DataTransfer();
		dataTransfer.setData('application/x-fella-workspace', path);
		if (alreadyOpen) dataTransfer.setData('application/x-fella-existing-workspace', path);
		board.dispatchEvent(new DragEvent(eventType, {
			bubbles: true,
			cancelable: true,
			clientX: rect.left + rect.width * targetX,
			clientY: rect.top + rect.height * targetY,
			dataTransfer
		}));
	}, { eventType: type, path: workspacePath, targetX: x, targetY: y, alreadyOpen: existing });
}

function expectSameRenderedBox(actual, expected) {
	expect(actual).not.toBeNull();
	expect(expected).not.toBeNull();
	for (const edge of ['x', 'y', 'width', 'height']) {
		expect(Math.abs(actual[edge] - expected[edge])).toBeLessThanOrEqual(1);
	}
}

test('environments compose General and repository panes while conversations retain their original scope', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const switcher = page.locator('.sidebar .environment-switcher-trigger');
	await expect(switcher).toBeVisible();
	await expect(switcher).toContainText('General');
	await expect(page.locator('.titlebar .environment-switcher-trigger')).toHaveCount(0);
	await switcher.click();
	await expect(page.getByRole('menuitemradio')).toHaveCount(1);
	await page.keyboard.press('Escape');
	await expect(page.getByRole('heading', { name: 'New conversation', level: 1 })).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(0);
	await expect(page.getByRole('button', { name: 'Mount a folder' })).toHaveCount(0);
	const surfaceColors = await page.evaluate(() => {
		const root = document.documentElement;
		const previous = root.getAttribute('data-color-mode');
		const measure = () => ({
			main: getComputedStyle(document.querySelector('main')).backgroundColor,
			dock: getComputedStyle(document.querySelector('.dock')).backgroundColor,
			composer: getComputedStyle(document.querySelector('.field')).backgroundColor
		});
		root.setAttribute('data-color-mode', 'light');
		const light = measure();
		root.setAttribute('data-color-mode', 'dark');
		const dark = measure();
		if (previous === null) root.removeAttribute('data-color-mode');
		else root.setAttribute('data-color-mode', previous);
		return { light, dark };
	});
	for (const surfaces of [surfaceColors.light, surfaceColors.dark]) {
		expect(surfaces.dock).toBe(surfaces.main);
		expect(surfaces.composer).toBe(surfaces.main);
	}

	await page.getByRole('combobox', { name: 'Ask a question' }).fill('What is a useful way to compare trends?');
	const initialSend = page.getByRole('button', { name: 'Send' });
	await expect(initialSend).toHaveAttribute('data-slot', 'button');
	await initialSend.click();
	await expect(page.getByText('Scoped to general.')).toBeVisible();
	const unboundCall = await page.evaluate(() => window.__workspaceAskCalls.at(-1));
	expect(unboundCall.workspaceId).toBeNull();

	await page.keyboard.press('Control+t');
	await expect(switcher).toContainText('General 2');
	await switcher.click();
	await expect(page.getByRole('menuitemradio')).toHaveCount(2);
	await expect(page.getByRole('menuitemradio').nth(1)).toHaveAttribute('aria-checked', 'true');
	await page.keyboard.press('Escape');
	await expect(page.getByRole('heading', { name: 'New conversation', level: 1 })).toBeVisible();
	await page.getByRole('combobox', { name: 'Ask a question' }).fill('What does a seasonal pattern look like?');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to general.')).toBeVisible();
	const conversationsAfterQuestion = await page.evaluate(() => window.__workspaceAskCalls.length);
	await page.keyboard.press('Control+[');
	await expect(switcher).toContainText('General');
	await expect(page.getByText('Scoped to general.')).toBeVisible();
	await page.keyboard.press('Control+]');
	await expect(switcher).toContainText('General 2');
	expect(await page.evaluate(() => window.__workspaceAskCalls.length)).toBe(conversationsAfterQuestion);
	await page.keyboard.press('Control+1');
	await expect(switcher).toContainText('General');

	const workspaceNavigation = page.getByRole('region', { name: 'Workspaces' });
	await workspaceNavigation.getByRole('button', { name: 'Add repository' }).click();
	await expect(switcher).toContainText('General +1');
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await expect(page.getByRole('article', { name: 'northwind-sales workspace' })).toBeVisible();
	await expect(page.getByRole('article', { name: 'General workspace' })).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(2);
	const salesTile = page.getByRole('article', { name: 'northwind-sales workspace' });
	const generalWorkspaceTile = page.getByRole('article', { name: 'General workspace' });
	const salesFocus = salesTile.locator('.tile-focus');
	const salesClose = salesTile.getByRole('button', { name: 'Close northwind-sales workspace' });
	const generalPreview = generalWorkspaceTile.locator('.tile-preview');
	await expect(salesTile).toHaveCSS('border-top-width', '2px');
	await expect(generalWorkspaceTile).toHaveCSS('border-top-width', '1px');
	await expect(generalPreview.locator('.preview-answer')).toContainText('Scoped to general.');
	await expect(salesFocus).toHaveAttribute('data-slot', 'button');
	await expect(salesClose).toHaveAttribute('data-slot', 'button');
	await expect(salesClose).toHaveCSS('width', '26px');
	await expect(salesClose).toHaveCSS('height', '26px');
	await expect(generalPreview).toHaveAttribute('data-slot', 'button');
	await generalPreview.click();
	await expect(generalWorkspaceTile.locator('.tile-focus')).toHaveAttribute('aria-pressed', 'true');
	await expect(generalWorkspaceTile).toHaveCSS('border-top-width', '2px');
	await expect(salesTile).toHaveCSS('border-top-width', '1px');
	await expect(salesTile.locator('.tile-preview')).toBeVisible();
	await expect(page.getByRole('combobox', { name: 'Ask a question' })).toBeVisible();
	await expect(page.locator('.workspace-context-bar')).toHaveCount(0);
	await salesFocus.click();
	await expect(salesFocus).toHaveAttribute('aria-pressed', 'true');
	await expect(page.getByRole('combobox', { name: 'Ask about northwind-sales' })).toBeVisible();
	await expect(page.locator('.workspace-context-bar')).toContainText('northwind-sales');
	const salesWorkspaceRow = page.getByRole('button', { name: 'Open workspace northwind-sales' });
	await expect(page.getByRole('button', { name: 'Open investigation brief in northwind-sales' })).toBeVisible();
	await expect(page.getByRole('button', { name: 'Create project for northwind-sales' })).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await expect(page.getByTestId('investigation-data-readiness').getByRole('button', { name: /sales\.csv/ })).toBeVisible();
	await salesWorkspaceRow.click();
	await salesWorkspaceRow.hover();
	await page.getByRole('button', { name: 'New conversation in northwind-sales' }).click();
	await expect(switcher).toContainText('General +1');
	const salesComposer = page.getByRole('combobox', { name: 'Ask about northwind-sales' });
	await salesComposer.fill('Summarize sales trends');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	const salesCall = await page.evaluate(() => window.__workspaceAskCalls.at(-1));
	expect(salesCall.workspaceId).toBe(folders[0].path);
	const automaticInvestigation = await page.evaluate(() => ({
		calls: window.__workspaceInvestigationEnsureCalls,
		records: window.__workspaceInvestigations()
	}));
	expect(automaticInvestigation.calls).toHaveLength(1);
	expect(automaticInvestigation.calls[0].workspaceId).toBe(folders[0].path);
	expect(automaticInvestigation.calls[0].conversationId).toBe(salesCall.conversationId);
	expect(automaticInvestigation.calls[0].initialRequest).toBe('Summarize sales trends');
	expect(automaticInvestigation.records).toHaveLength(1);
	expect(automaticInvestigation.records[0].conversation_id).toBe(salesCall.conversationId);
	expect(automaticInvestigation.records[0].brief.goal).toBe('Summarize sales trends');

	const generalTile = page.getByRole('article', { name: 'General workspace' });
	await generalTile.locator('.tile-focus').click();
	await page.getByRole('combobox', { name: 'Ask a question' }).fill('How should I compare two trend lines?');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to general.').last()).toBeVisible();
	expect((await page.evaluate(() => window.__workspaceAskCalls.at(-1))).workspaceId).toBeNull();
	expect(await page.evaluate(() => window.__workspaceInvestigationEnsureCalls.length)).toBe(1);
	await expect(switcher).toContainText('General +1');

	await salesWorkspaceRow.click();

	await page.getByRole('button', { name: 'Create project for northwind-sales' }).click();
	await expect(page.getByRole('heading', { name: 'Create a project' })).toBeVisible();
	await expect(page.getByLabel('Repository', { exact: true })).toHaveValue(folders[0].path);
	await page.getByRole('textbox', { name: 'Name' }).fill('Sales review');
	await page.getByRole('button', { name: 'Create project', exact: true }).click();
	await expect(page.getByRole('button', { name: 'Open project Sales review' })).toBeVisible();

	await salesWorkspaceRow.click();

	await dispatchWorkspaceDrag(page, 'dragover', folders[1].path, { x: 0.9, y: 0.5 });
	await expect(page.locator('.placement-preview .preview-slot')).toHaveCount(3);
	const healthTarget = page.locator('.placement-preview .preview-slot.target');
	await expect(healthTarget).toHaveText('Drop to place');
	const healthTargetBox = await healthTarget.boundingBox();
	await dispatchWorkspaceDrag(page, 'drop', folders[1].path, { x: 0.9, y: 0.5 });
	const sales = page.getByRole('article', { name: 'northwind-sales workspace' });
	const health = page.getByRole('article', { name: 'health-journal workspace' });
	await expect(sales).toBeVisible();
	await expect(health).toBeVisible();
	await expect(page.getByRole('button', { name: 'Open investigation brief in health-journal' })).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(3);
	await expect(page.getByRole('combobox', { name: 'Workspace arrangement' })).toHaveCount(0);
	const healthBox = await health.boundingBox();
	expectSameRenderedBox(healthBox, healthTargetBox);

	await dispatchWorkspaceDrag(page, 'dragover', folders[2].path, { x: 0.1, y: 0.5 });
	await expect(page.locator('.placement-preview .preview-slot')).toHaveCount(4);
	const budgetTargetBox = await page.locator('.placement-preview .preview-slot.target').boundingBox();
	await dispatchWorkspaceDrag(page, 'drop', folders[2].path, { x: 0.1, y: 0.5 });
	const budget = page.getByRole('article', { name: 'weekly-budget workspace' });
	await expect(budget).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(4);
	const budgetBox = await budget.boundingBox();
	expectSameRenderedBox(budgetBox, budgetTargetBox);
	const otherBoxes = await Promise.all([sales, health, page.getByRole('article', { name: 'General workspace' })].map((tile) => tile.boundingBox()));
	const allFourBoxes = [budgetBox, ...otherBoxes];
	expect(allFourBoxes.every(Boolean)).toBe(true);
	// Four panes should occupy each cell of a balanced 2×2, regardless of
	// which workspace identity occupies each cell after prior drag/reordering.
	expect(new Set(allFourBoxes.map((box) => Math.round(box.x))).size).toBe(2);
	expect(new Set(allFourBoxes.map((box) => Math.round(box.y))).size).toBe(2);
	expect(new Set(allFourBoxes.map((box) => `${Math.round(box.x)},${Math.round(box.y)}`)).size).toBe(4);

	await dispatchWorkspaceDrag(page, 'dragover', folders[0].path, { x: 0.1, y: 0.1, existing: true });
	await expect(page.locator('.placement-preview .preview-slot')).toHaveCount(4);
	const salesTargetBox = await page.locator('.placement-preview .preview-slot.target').boundingBox();
	await dispatchWorkspaceDrag(page, 'drop', folders[0].path, { x: 0.1, y: 0.1, existing: true });
	const movedSalesBox = await sales.boundingBox();
	expectSameRenderedBox(movedSalesBox, salesTargetBox);
});

test('an Investigation opens its canonical workspace conversation', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await expect(page.getByRole('button', { name: /Switch environment/ })).toBeVisible();
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	const composer = page.getByRole('combobox', { name: 'Ask about northwind-sales' });
	await composer.fill('Summarize cohort retention');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	const askCall = await page.evaluate(() => window.__workspaceAskCalls.at(-1));
	const workspaceContext = page.locator('.workspace-context-bar');
	await expect(workspaceContext).toContainText('Workspace');
	await expect(workspaceContext).toContainText('northwind-sales');
	await expect(page.getByRole('button', { name: 'Open investigation brief in northwind-sales' })).toBeVisible();
	await workspacePresetButton(page, 'Investigation').click();
	await expect(page.getByRole('heading', { name: 'Investigation', exact: true })).toBeVisible();
	await expect(workspaceContext).toContainText('northwind-sales');
	await expect(page.locator('.editor-heading h2')).toHaveText('Summarize cohort retention');
	const openConversation = page.getByRole('button', { name: 'Open conversation', exact: true });
	await expect(openConversation).toBeVisible();
	const linked = await page.evaluate(() => window.__workspaceInvestigations()[0]);
	expect(linked.conversation_id).toBe(askCall.conversationId);
	await openConversation.click();
	await expect(page.getByRole('combobox', { name: 'Ask about northwind-sales' })).toBeVisible();
	await expect(page.locator('.msg.user').filter({ hasText: 'Summarize cohort retention' })).toHaveCount(1);
	expect(await page.evaluate(() => window.__workspaceAskCalls.length)).toBe(1);
	await expect(workspacePresetButton(page, 'Investigation')).toBeVisible();
	await workspacePresetButton(page, 'Investigation').click();
	await expect(page.locator('.editor-heading h2')).toHaveText('Summarize cohort retention');
	expect(await page.evaluate(() => window.__workspaceAskCalls.length)).toBe(1);
});

test('Investigation work plan keeps the brief concise and opens editing and history on demand', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	const originalRequest = 'Explain weekly sales for the finance team';
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill(originalRequest);
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();

	const brief = page.getByRole('region', { name: 'Brief', exact: true });
	const definitions = page.locator('.definition-disclosure');
	const history = page.locator('.investigation-history');
	await expect(brief).toContainText(originalRequest);
	await expect(brief).toContainText('Audience');
	await expect(brief).toContainText('Deliverable');
	await expect(page.getByRole('button', { name: 'Edit brief' })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Questions', exact: true })).toBeVisible();
	await expect(page.getByRole('textbox', { name: 'Audience' })).toHaveCount(0);
	expect(await definitions.evaluate((element) => element.open)).toBe(false);
	expect(await history.evaluate((element) => element.open)).toBe(false);

	await page.getByRole('button', { name: 'Edit brief' }).click();
	await page.getByRole('textbox', { name: 'Audience' }).fill('Finance lead');
	await page.getByRole('button', { name: 'Discard changes' }).click();
	await expect(page.getByRole('textbox', { name: 'Audience' })).toHaveCount(0);
	await expect(brief).toContainText('Not specified');

	await page.getByRole('button', { name: 'Edit brief' }).click();
	await page.getByRole('textbox', { name: 'Audience' }).fill('Finance lead');
	await page.getByRole('textbox', { name: 'Desired output' }).fill('A concise weekly sales report');
	await page.getByRole('button', { name: 'Save brief' }).click();
	await expect(brief).toContainText('Finance lead');
	await expect(brief).toContainText('A concise weekly sales report');

	await page.getByRole('button', { name: 'Open conversation', exact: true }).click();
	await expect(page.locator('.msg.user').filter({ hasText: originalRequest })).toHaveCount(1);
	expect(await page.evaluate(() => window.__workspaceAskCalls.length)).toBe(1);
});

test('a saved answer opens its canonical run and returns to the same conversation result', async ({ page }) => {
	await installDesktopMock(page, {
		questionRunOutcomes: [{ answer_text: 'Weekly sales rose 12% in the saved analysis.' }]
	});
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill('Explain weekly sales');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await page.getByRole('button', { name: 'Analyze main Question' }).click();
	await page.getByRole('region', { name: 'Review this run' }).getByRole('button', { name: 'Run this Question' }).click();
	const answer = page.locator('.msg.assistant[data-analysis-turn-id="fixture-question-turn-1"]');
	await expect(answer).toContainText('Weekly sales rose 12% in the saved analysis.');
	await answer.getByRole('button', { name: 'Open in Investigation' }).click();
	await expect(page.getByRole('region', { name: 'Saved analysis run' })).toBeVisible();
	await expect(page.getByText('Question at run')).toBeVisible();
	await expect(page.getByRole('button', { name: 'Go to Question' })).toBeVisible();
	await page.getByRole('button', { name: 'Open conversation result' }).click();
	await expect(page.getByRole('combobox', { name: 'Ask about northwind-sales' })).toBeVisible();
	await expect(answer).toBeVisible();
	await expect(answer).toContainText('Weekly sales rose 12% in the saved analysis.');
	expect(await page.evaluate(() => window.__workspaceAskCalls.length)).toBe(2);
});

test('Investigation index summarizes work and resumes the same completed record', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	const composer = page.getByRole('combobox', { name: 'Ask about northwind-sales' });
	await composer.fill('First return-to-work question');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	const firstId = await page.evaluate(() => window.__workspaceInvestigations()[0].id);

	const workspaceRow = page.getByRole('button', { name: 'Open workspace northwind-sales' });
	await workspaceRow.hover();
	await page.getByRole('button', { name: 'New conversation in northwind-sales' }).click();
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill('Second return-to-work question');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	await workspacePresetButton(page, 'Investigation').click();
	const choices = page.locator('.investigation-choice');
	await expect(choices).toHaveCount(2);
	const firstChoice = choices.filter({ hasText: 'First return-to-work question' });
	await expect(firstChoice).toContainText('Active');
	await expect(firstChoice).toContainText('Updated');
	await expect(firstChoice).toContainText('0/1 answered · 1 needs attention');
	await firstChoice.click();
	await expect(page.locator('.editor-heading h2')).toHaveText('First return-to-work question');
	await expect(page.getByRole('textbox', { name: 'Main question' })).toHaveValue('First return-to-work question');
	const openConversation = page.getByRole('button', { name: 'Open conversation', exact: true });
	await openConversation.click();
	await expect(page.locator('.msg.user').filter({ hasText: 'First return-to-work question' })).toHaveCount(1);
	await workspacePresetButton(page, 'Investigation').click();
	await page.getByRole('button', { name: 'Analyze main Question' }).click();
	await page.getByRole('region', { name: 'Review this run' }).getByRole('button', { name: 'Run this Question' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.').last()).toBeVisible();
	await workspacePresetButton(page, 'Investigation').click();
	await page.getByRole('button', { name: 'Mark complete' }).click();
	await expect(choices.filter({ hasText: 'First return-to-work question' })).toContainText('Complete');
	const completedId = await page.evaluate(() => window.__workspaceInvestigations().find((item) => item.title === 'First return-to-work question').id);
	expect(completedId).toBe(firstId);
	await page.getByRole('button', { name: 'Reopen Investigation' }).click();
	await expect(choices.filter({ hasText: 'First return-to-work question' })).toContainText('Active');
	const resumed = await page.evaluate((id) => window.__workspaceInvestigations().find((item) => item.id === id), firstId);
	expect(resumed.questions[0].text).toBe('First return-to-work question');
	expect(resumed.questions[0].answer_status).toBe('answered');
});

test('workspace layout presets preserve the same Investigation, transcript, and unsaved plan', async ({ page }) => {
	await installDesktopMock(page, {
		questionRunOutcomes: [{ status: 'answered', answer_text: 'Weekly sales rose 12%.' }]
	});
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	const originalRequest = 'Explain weekly sales in northwind-sales';
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill(originalRequest);
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	const presets = page.locator('.workspace-preset-control');
	const preset = (name) => presets.getByRole('button', { name: new RegExp(`^${name}:`) });
	await expect(preset('Focus')).toHaveAttribute('aria-pressed', 'false');
	await preset('Focus').click();
	await expect(preset('Focus')).toHaveAttribute('aria-pressed', 'true');
	await expect(page.getByTestId('workspace-investigation-summary')).toContainText('0/1 answered');
	await expect(page.getByTestId('workspace-investigation-summary')).toContainText('need attention');

	const initial = await page.evaluate(() => {
		const investigation = window.__workspaceInvestigations()[0];
		return { id: investigation.id, questionId: investigation.questions[0].id };
	});
	await preset('Investigation').click();
	await expect(page.getByRole('heading', { name: 'Investigation', exact: true })).toBeVisible();
	const mainQuestion = page.getByRole('textbox', { name: 'Main question' });
	await mainQuestion.fill('Measure weekly net sales');

	await preset('Focus').click();
	await expect(page.getByRole('combobox', { name: 'Ask about northwind-sales' })).toBeVisible();
	await expect(page.locator('.msg.user').filter({ hasText: originalRequest })).toHaveCount(1);
	await preset('Workbench').click();
	await expect(page.locator('.workspace-preset-layout')).toHaveAttribute('data-workspace-preset', 'workbench');
	await expect(preset('Workbench')).toHaveAttribute('aria-pressed', 'true');
	await expect(preset('Focus')).toHaveAttribute('aria-pressed', 'false');
	const workbenchColumns = await page.locator('.workspace-preset-layout').evaluate((element) =>
		getComputedStyle(element).gridTemplateColumns.trim().split(/\s+/).length
	);
	expect(workbenchColumns).toBe(1);
	await expect(page.getByRole('combobox', { name: 'Ask about northwind-sales' })).toBeVisible();
	await expect(page.getByRole('textbox', { name: 'Main question' })).toHaveValue('Measure weekly net sales');
	await expect(page.getByRole('button', { name: 'Ask', exact: true })).toBeVisible();
	await preset('Investigation').click();
	await expect(page.getByRole('textbox', { name: 'Main question' })).toHaveValue('Measure weekly net sales');
	const afterLayoutChanges = await page.evaluate(() => ({
		investigations: window.__workspaceInvestigations(),
		ensureCalls: window.__workspaceInvestigationEnsureCalls.length
	}));
	expect(afterLayoutChanges.investigations).toHaveLength(1);
	expect(afterLayoutChanges.investigations[0].id).toBe(initial.id);
	expect(afterLayoutChanges.ensureCalls).toBe(1);
	await page.getByRole('button', { name: 'Save plan' }).click();
	await expect(page.getByRole('region', { name: 'Questions' }).locator('.question-save')).toContainText('Plan saved');

	await page.getByRole('button', { name: 'Analyze main Question' }).click();
	await page.getByRole('region', { name: 'Review this run' }).getByRole('button', { name: 'Run this Question' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.').last()).toBeVisible();
	await expect(page.getByTestId('workspace-investigation-summary')).toContainText('1/1 answered');

	const finalState = await page.evaluate(() => ({
		investigations: window.__workspaceInvestigations(),
		askCalls: window.__workspaceAskCalls.length
	}));
	expect(finalState.investigations).toHaveLength(1);
	expect(finalState.investigations[0].id).toBe(initial.id);
	expect(finalState.investigations[0].questions[0].id).toBe(initial.questionId);
	expect(finalState.investigations[0].questions[0].text).toBe('Measure weekly net sales');
	expect(finalState.investigations[0].questions[0].answer_status).toBe('answered');
	expect(finalState.investigations[0].analysis_turn_ids).toContain('fixture-question-turn-1');
	expect(finalState.askCalls).toBe(2);
});

test('Workbench composes curated panels while keeping Investigation scope and coverage visible', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	const request = 'Review weekly sales performance';
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill(request);
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	const investigationTitle = await page.evaluate(() => window.__workspaceInvestigations()[0].title);
	await workspacePresetButton(page, 'Workbench').click();

	const panels = page.locator('.workspace-preset-layout');
	const conversation = panels.locator('[data-panel-id="conversation"]');
	const investigation = panels.locator('[data-panel-id="investigation"]');
	const companion = panels.locator('[data-panel-id="companion"]');
	await expect(conversation).toBeVisible();
	await expect(investigation).toBeVisible();
	await expect(companion).toBeHidden();
	const coverage = page.getByTestId('workspace-investigation-summary');
	await expect(coverage).toContainText(investigationTitle);
	await expect(coverage).toContainText('0/1 answered');
	await expect(coverage).toHaveAttribute('aria-label', new RegExp(investigationTitle));

	await page.locator('.workbench-panel-picker > summary').click();
	const companionToggle = page.getByRole('checkbox', { name: 'Show Chart or source preview panel' });
	await expect(companionToggle).not.toBeChecked();
	await companionToggle.check();
	await expect(companion).toBeVisible();
	await expect(companion).toContainText('Choose a chart or source to open it beside the conversation.');

	const companionGroup = page.getByLabel('Group Chart or source preview');
	await companionGroup.selectOption('conversation');
	await expect(companion).toHaveAttribute('data-panel-group', 'conversation');
	await page.getByRole('button', { name: 'Move Chart or source preview up' }).click();
	const storedAfterReorder = await page.evaluate(() => JSON.parse(localStorage.getItem('fella:workbench-layout:v1')));
	const orderedConversationPanels = storedAfterReorder.panels
		.filter(({ group, visible }) => group === 'conversation' && visible !== false)
		.sort((a, b) => a.order - b.order)
		.map(({ id }) => id);
	expect(orderedConversationPanels).toEqual(['companion', 'conversation']);

	const conversationToggle = page.getByRole('checkbox', { name: 'Show Conversation panel' });
	await conversationToggle.uncheck();
	await expect(conversation).toBeHidden();
	await expect(investigation).toBeVisible();
	await expect(page.getByRole('checkbox', { name: 'Show Investigation panel' })).toBeDisabled();
	await expect(coverage).toContainText(investigationTitle);
	await expect(page.getByRole('button', { name: 'Ask', exact: true })).toBeVisible();

	const settings = page.locator('.panel-picker-popover');
	await settings.getByRole('button', { name: 'Reset' }).click();
	await expect(conversation).toBeVisible();
	await expect(investigation).toBeVisible();
	await expect(companion).toBeHidden();
	const resetLayout = await page.evaluate(() => JSON.parse(localStorage.getItem('fella:workbench-layout:v1')));
	expect(resetLayout.panels.find(({ id }) => id === 'companion')).toMatchObject({ visible: null, group: 'analysis', order: 1 });
});

test('Workbench panes resize accessibly, respect minimum widths, persist, and reset independently of analysis', async ({ page }) => {
	await page.setViewportSize({ width: 1720, height: 960 });
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill('Review weekly sales performance');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	const investigationId = await page.evaluate(() => window.__workspaceInvestigations()[0].id);
	await workspacePresetButton(page, 'Workbench').click();

	const splitter = page.getByRole('separator', { name: 'Resize Conversation and Analysis panes' });
	await expect(splitter).toBeVisible();
	await expect(splitter).toHaveAttribute('aria-valuemin', '32');
	await expect(splitter).toHaveAttribute('aria-valuemax', '60');
	const conversation = page.locator('[data-panel-id="conversation"]');
	const analysis = page.locator('[data-panel-id="investigation"]');
	const initialWidth = (await conversation.boundingBox()).width;
	const initialShare = Number(await splitter.getAttribute('aria-valuenow'));

	await splitter.focus();
	await splitter.press('ArrowRight');
	await expect(splitter).toHaveAttribute('aria-valuenow', String(initialShare + 2));
	await expect.poll(async () => (await conversation.boundingBox()).width).toBeGreaterThan(initialWidth);
	await expect.poll(async () => {
		const saved = await page.evaluate(() => JSON.parse(localStorage.getItem('fella:workbench-layout:v1')));
		return saved.conversationShare;
	}).toBe(initialShare + 2);

	const handle = await splitter.boundingBox();
	await page.mouse.move(handle.x + handle.width / 2, handle.y + handle.height / 2);
	await page.mouse.down();
	await page.mouse.move(handle.x + handle.width / 2 + 72, handle.y + handle.height / 2, { steps: 4 });
	await page.mouse.up();
	await expect.poll(async () => Number(await splitter.getAttribute('aria-valuenow'))).toBeGreaterThan(initialShare + 2);
	await splitter.press('End');
	await expect(splitter).toHaveAttribute('aria-valuenow', '60');
	const conversationWidth = (await conversation.boundingBox()).width;
	const analysisWidth = (await analysis.boundingBox()).width;
	expect(conversationWidth).toBeGreaterThanOrEqual(320);
	expect(analysisWidth).toBeGreaterThanOrEqual(420);
	const afterResize = await page.evaluate(() => window.__workspaceInvestigations()[0].id);
	expect(afterResize).toBe(investigationId);

	await page.reload();
	const restored = await page.evaluate(() => JSON.parse(localStorage.getItem('fella:workbench-layout:v1')));
	expect(restored.conversationShare).toBe(60);
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	await workspacePresetButton(page, 'Workbench').click();
	await expect(page.getByRole('separator', { name: 'Resize Conversation and Analysis panes' })).toHaveAttribute('aria-valuenow', '60');
	await page.locator('.workbench-panel-picker > summary').click();
	await page.locator('.panel-picker-popover').getByRole('button', { name: 'Reset' }).click();
	const reset = await page.evaluate(() => JSON.parse(localStorage.getItem('fella:workbench-layout:v1')));
	expect(reset.conversationShare).toBe(46);
	await expect(page.getByRole('separator', { name: 'Resize Conversation and Analysis panes' })).toHaveAttribute('aria-valuenow', '46');
});

test('an analyst can edit the main Question and linked subquestions without rewriting the request', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	const composer = page.getByRole('combobox', { name: 'Ask about northwind-sales' });
	await composer.fill('Understand conversion by channel');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	const investigationId = await page.evaluate(() => window.__workspaceInvestigations()[0].id);
	await page.evaluate((id) => window.__workspaceSetRunLinks(id, ['prior-analysis-turn']), investigationId);
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await expect(page.getByRole('heading', { name: 'Questions', exact: true })).toBeVisible();

	await page.getByRole('textbox', { name: 'Main question' }).fill('Measure conversion by channel');
	await page.getByRole('button', { name: 'Add subquestion' }).click();
	await page.getByRole('textbox', { name: 'Subquestion 1' }).fill('Compare conversion over time');
	await page.getByRole('button', { name: 'Add subquestion' }).click();
	await page.getByRole('textbox', { name: 'Subquestion 2' }).fill('Compare conversion by device');
	await page.getByRole('button', { name: 'Add subquestion' }).click();
	await page.getByRole('textbox', { name: 'Subquestion 3' }).fill('Compare conversion by region');
	await page.getByRole('button', { name: 'Move Subquestion 3 up' }).click();
	await page.getByRole('button', { name: 'Remove Subquestion 1' }).click();
	await page.getByRole('button', { name: 'Save plan' }).click();
	await expect(page.locator('.question-save')).toContainText('Plan saved');

	const saved = await page.evaluate(() => window.__workspaceInvestigations()[0]);
	expect(saved.questions.map((item) => item.text)).toEqual([
		'Measure conversion by channel',
		'Compare conversion by region',
		'Compare conversion by device'
	]);
	expect(saved.questions[0].parent_id).toBeFalsy();
	expect(saved.questions.slice(1).every((item) => item.parent_id === saved.questions[0].id)).toBe(true);
	expect(saved.analysis_turn_ids).toEqual(['prior-analysis-turn']);
	const independentlyLoaded = await page.evaluate((questionId) =>
		window.fella.invoke('investigation_question_load', { questionId }), saved.questions[1].id);
	expect(independentlyLoaded).toMatchObject({
		id: saved.questions[1].id,
		investigation_id: saved.id,
		parent_id: saved.questions[0].id,
		text: 'Compare conversion by region',
		status: 'active'
	});

	await page.getByRole('button', { name: 'Open conversation', exact: true }).click();
	await expect(composer).toBeVisible();
	const originalTranscriptQuestion = page.locator('.msg.user').filter({ hasText: 'Understand conversion by channel' });
	await expect(originalTranscriptQuestion).toHaveCount(1);
	await expect(originalTranscriptQuestion).not.toContainText('Measure conversion by channel');
	await composer.fill('Which device has the strongest change?');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.').last()).toBeVisible();
	const nextAsk = await page.evaluate(() => window.__workspaceAskCalls.at(-1));
	expect(nextAsk.question).toContain('Compare conversion by device');
	expect(nextAsk.question).not.toContain('Compare conversion over time');
	expect(nextAsk.question.indexOf('Compare conversion by region')).toBeLessThan(nextAsk.question.indexOf('Compare conversion by device'));
	expect(nextAsk.question).toContain('Main Question: Measure conversion by channel');
	expect(nextAsk.question).toContain('Which device has the strongest change?');
	// The message renderer adds a presentation-only "You asked:" prefix. The
	// actual contract is that the original request remains visible and the
	// private Investigation-plan context stays out of the transcript.
	const transcriptQuestion = page.locator('.msg.user').last();
	await expect(transcriptQuestion).toContainText('Which device has the strongest change?');
	await expect(transcriptQuestion).not.toContainText('Measure conversion by channel');
	await expect(transcriptQuestion).not.toContainText('Compare conversion by region');
	await expect(transcriptQuestion).not.toContainText('Compare conversion over time');
	await expect(transcriptQuestion).not.toContainText('Compare conversion by device');
});

test('a Question can narrow its source set and launches a run linked to that Question', async ({ page }) => {
	const fixtureFolders = folders.map((folder) => folder.path === folders[0].path
		? {
			...folder,
			catalog: {
				...folder.catalog,
				sources: [
					...folder.catalog.sources,
					{ name: 'refunds.csv', path: `${folder.path}\\refunds.csv`, kind: 'csv', view: 'refunds', row_count: 18, size_bytes: 1024, mtime: 1 }
				]
			}
		}
		: folder);
	await installDesktopMock(page, { fixtureFolders });
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	const composer = page.getByRole('combobox', { name: 'Ask about northwind-sales' });
	await composer.fill('Explain changes in sales and refunds');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await expect(page.getByRole('heading', { name: 'Questions', exact: true })).toBeVisible();

	await page.getByRole('button', { name: 'Add subquestion' }).click();
	await page.getByRole('textbox', { name: 'Subquestion 1' }).fill('Compare refund rates with sales');
	const questionCard = page.locator('.question-item').filter({ has: page.getByRole('textbox', { name: 'Subquestion 1' }) });
	await questionCard.getByText(/Sources/).click();
	await questionCard.getByLabel('Use a specific source set').check();
	await questionCard.getByLabel('refunds.csv').uncheck();
	const questionScope = questionCard.locator('.question-source-scope > summary');
	await expect(questionScope).toContainText('1 selected · 1 effective');

	await page.getByLabel('Choose files').check();
	await page.getByRole('checkbox', { name: 'Include sales.csv in this Investigation' }).uncheck();
	await expect.poll(() => page.evaluate(() => window.__workspaceInvestigations()[0].source_selection)).toEqual({
		mode: 'selected_sources',
		included_paths: ['refunds.csv']
	});
	await expect(questionScope).toContainText('1 selected · 0 effective');
	await expect(questionCard).toContainText('sales.csv');
	await expect(questionCard).toContainText('outside this Investigation’s scope');
	await page.getByLabel('All sources').check();
	await expect(questionScope).toContainText('1 selected · 1 effective');

	await page.getByRole('button', { name: 'Add definition or assumption' }).click();
	await page.getByRole('textbox', { name: 'Context note 1' }).fill('Count a refund only when it is linked to a completed sale.');

	await page.getByRole('button', { name: 'Analyze Subquestion 1' }).click();
	const setupPreview = page.getByRole('region', { name: 'Review this run' });
	await expect(setupPreview).toContainText('Compare refund rates with sales');
	await expect(setupPreview).toContainText('Question-specific scope · 1 effective of 1 selected');
	await expect(setupPreview).toContainText('Your unsaved Question and context edits will be saved before this run starts.');
	await setupPreview.locator('details.run-preview-files > summary').click();
	await expect(setupPreview).toContainText('sales.csv');
	await expect(setupPreview).toContainText('Count a refund only when it is linked to a completed sale.');
	expect(await page.evaluate(() => window.__workspaceAskCalls.length)).toBe(1);
	const beforeConfirmation = await page.evaluate(() => window.__workspaceInvestigations()[0]);
	expect(beforeConfirmation.questions.map((item) => item.text)).toEqual(['Explain changes in sales and refunds']);
	expect(beforeConfirmation.definitions).toEqual([]);
	await setupPreview.getByRole('button', { name: 'Save changes and run' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.').last()).toBeVisible();
	const saved = await page.evaluate(() => window.__workspaceInvestigations()[0]);
	const question = saved.questions[1];
	expect(question).toMatchObject({
		text: 'Compare refund rates with sales',
		source_selection: { mode: 'selected_sources', included_paths: ['sales.csv'] }
	});
	expect(saved.definitions).toMatchObject([
		{ kind: 'definition', text: 'Count a refund only when it is linked to a completed sale.' }
	]);
	const run = await page.evaluate(() => window.__workspaceAskCalls.at(-1));
	expect(run).toMatchObject({ investigationId: saved.id, questionId: question.id });
	expect(run.question).toContain('Compare refund rates with sales');
	await expect(page.locator('.msg.user').last()).toContainText('Compare refund rates with sales');
});

test('Questions show persisted coverage states and explain incomplete, blocked, and outdated answers', async ({ page }) => {
	await installDesktopMock(page, {
		questionRunOutcomes: [
			{
				status: 'incomplete',
				reason: 'The selected files do not cover the full requested period.',
				delay_ms: 800,
				evidence: [{
					id: 'fixture-completion-output-1',
					tool: 'run_sql',
					note: 'Compare weekly sales',
					result_summary: 'Weekly sales were calculated from the available period.',
					ms: 8,
					sources: [{ source: 'sales.csv', table: 'sales' }]
				}],
				checks: [{ label: 'The requested period is covered', ok: false, outcome: 'flagged' }]
			},
			{ status: 'blocked', reason: 'The required date field is unavailable.' },
			{ status: 'answered' }
		]
	});
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill('Explain the change in weekly sales');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	const rootQuestion = page.locator('.root-question-item');
	await expect(rootQuestion.locator('[data-slot="badge"][data-question-status]')).toHaveText('Not started');

	let questionRunCount = 0;
	const runMainQuestion = async () => {
		questionRunCount += 1;
		await page.getByRole('button', { name: 'Analyze main Question' }).click();
		await page.getByRole('region', { name: 'Review this run' }).getByRole('button', { name: 'Run this Question' }).click();
		if (questionRunCount === 1) {
			const timeline = page.getByRole('region', { name: 'Question run' }).last();
			await expect(timeline).toBeVisible();
			await expect(timeline).toHaveAttribute('data-question-id', /^q-root-/);
			await expect(timeline.locator('.run-question')).toContainText('Explain the change in weekly sales');
			await expect(timeline).toContainText('Checking the result');
			await expect(timeline).toContainText('Finding relevant passages');
			await expect(timeline).not.toContainText('grep_files');
			await expect(timeline).not.toContainText('Search the workspace for implementation details');
			await expect(timeline).not.toContainText('Raw file search details');
		}
		await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.').last()).toBeVisible();
		const timeline = await expectQuestionRunSettled(page);
		if (questionRunCount === 1) {
			await expect(page.locator('.msg.assistant').last()).not.toContainText('Private model scratch');
			await expect(timeline).toContainText('Found relevant passages in the available documents.');
			await expect(timeline).toContainText('Quality checks');
			await expect(timeline).toContainText('1 flagged');
			await expect(timeline).not.toContainText('grep_files');
			await timeline.getByRole('button', { name: 'Inspect output' }).click();
			const output = page.locator('.msg.assistant').last().locator('[data-evidence-id="fixture-completion-output-1"]');
			await expect(output).toBeVisible();
			await expect(output).toBeFocused();
		}
		await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	};

	await runMainQuestion();
	await expect(page.locator('.root-question-item [data-slot="badge"][data-question-status]')).toHaveText('Partial answer');
	await expect(page.locator('.root-question-item .question-status-reason')).toContainText('The selected files do not cover the full requested period.');

	await runMainQuestion();
	await expect(page.locator('.root-question-item [data-slot="badge"][data-question-status]')).toHaveText('Blocked');
	await expect(page.locator('.root-question-item .question-status-reason')).toContainText('The required date field is unavailable.');

	await runMainQuestion();
	await expect(page.locator('.root-question-item [data-slot="badge"][data-question-status]')).toHaveText('Complete answer');
	await expect(page.locator('.root-question-item .question-status-reason')).toHaveCount(0);

	await page.getByRole('textbox', { name: 'Main question' }).fill('Explain the change in weekly net sales');
	await page.getByRole('button', { name: 'Save plan' }).click();
	await expect(page.locator('.root-question-item [data-slot="badge"][data-question-status]')).toHaveText('Outdated');
	await expect(page.locator('.root-question-item .question-status-reason')).toContainText('The Question or its source scope changed. Run it again to refresh the answer.');
});

test('completion follows Question coverage and keeps documented gaps visible', async ({ page }) => {
	await installDesktopMock(page, {
		questionRunOutcomes: [
			{ status: 'incomplete', reason: 'Only 10 of the requested 12 weeks are present.' },
			{ status: 'unsupported', reason: 'The available files do not include cohort dates.' },
			{ status: 'answered' }
		]
	});
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill('Explain weekly sales and refunds');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();

	await page.getByRole('button', { name: 'Add subquestion' }).click();
	await page.getByRole('textbox', { name: 'Subquestion 1' }).fill('Compare refund rates with sales');
	await page.getByRole('button', { name: 'Add subquestion' }).click();
	await page.getByRole('textbox', { name: 'Subquestion 2' }).fill('Check the seasonal pattern');
	await page.getByRole('button', { name: 'Add subquestion' }).click();
	await page.getByRole('textbox', { name: 'Subquestion 3' }).fill('Inspect returns by region');
	await page.getByRole('button', { name: 'Save plan' }).click();

	const runQuestion = async (buttonName) => {
		await page.getByRole('button', { name: buttonName }).click();
		await page.getByRole('region', { name: 'Review this run' }).getByRole('button', { name: 'Run this Question' }).click();
		await expectQuestionRunSettled(page);
		await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	};
	await runQuestion('Analyze main Question');
	await runQuestion('Analyze Subquestion 1');
	await runQuestion('Analyze Subquestion 2');

	const coverage = page.getByRole('region', { name: 'Question coverage' });
	await expect(coverage.locator('[data-coverage="answered"] dd')).toHaveText('1');
	await expect(coverage.locator('[data-coverage="partial"] dd')).toHaveText('1');
	await expect(coverage.locator('[data-coverage="unanswered"] dd')).toHaveText('1');
	await expect(coverage.locator('[data-coverage="unsupported"] dd')).toHaveText('1');
	await expect(coverage).toContainText('Only 10 of the requested 12 weeks are present.');
	await expect(coverage).toContainText('The available files do not include cohort dates.');
	const blockedCompletion = page.getByRole('button', { name: 'Mark complete' });
	await expect(blockedCompletion).toBeDisabled();

	await coverage.getByRole('button', { name: 'Review Unsupported Question: Compare refund rates with sales' }).click();
	await expect(page.getByRole('textbox', { name: 'Subquestion 1' })).toBeFocused();

	await runQuestion('Analyze Subquestion 3');
	await expect(coverage.locator('[data-coverage="unanswered"] dd')).toHaveText('0');
	const closeWithGaps = page.getByRole('button', { name: 'Mark complete' });
	await expect(closeWithGaps).toBeEnabled();
	await closeWithGaps.click();
	await expect(coverage).toHaveAttribute('data-investigation-status', 'complete');
	await expect(coverage).toContainText('Investigation closed with documented gaps. It is not fully answered');
	await expect(coverage.locator('.question-coverage-review')).toContainText('Only 10 of the requested 12 weeks are present.');
	await expect(coverage.locator('.question-coverage-review')).toContainText('The available files do not include cohort dates.');
	await page.getByRole('button', { name: 'Reopen Investigation' }).click();

	await runQuestion('Analyze main Question');
	await runQuestion('Analyze Subquestion 1');
	await expect(coverage.locator('[data-coverage="answered"] dd')).toHaveText('4');
	await expect(coverage.locator('[data-coverage="partial"] dd')).toHaveText('0');
	await expect(coverage.locator('[data-coverage="unanswered"] dd')).toHaveText('0');
	await expect(coverage.locator('[data-coverage="unsupported"] dd')).toHaveText('0');
	const markComplete = page.getByRole('button', { name: 'Mark complete' });
	await expect(markComplete).toBeEnabled();
	await markComplete.click();
	await expect(coverage).toHaveAttribute('data-investigation-status', 'complete');
	await expect(coverage).toContainText('Investigation fully answered.');
	await page.getByRole('button', { name: 'Reopen Investigation' }).click();
	await expect(coverage).toHaveAttribute('data-investigation-status', 'active');
});

test('verification outcomes stay beside their outputs and explain check limits', async ({ page }) => {
	const queryEvidence = {
		id: 'evidence-weekly-sales',
		tool: 'run_sql',
		args: {},
		note: 'Calculate weekly sales',
		sql: 'select sum(net_sales) as weekly_sales from sales',
		result_summary: 'One weekly sales result was recorded.',
		columns: ['weekly_sales'],
		rows: [[40]],
		row_count: 1,
		ms: 8,
		sources: [{ source: 'sales.csv', table: 'sales' }]
	};
	const checks = [
		{
			label: 'the answer mentions 50 not found in any result',
			ok: false,
			outcome: 'flagged',
			detail: 'No recorded result contains 50.',
			finding: {
				code: 'unsupported_claim',
				effect: 'repair',
				target: 'claim',
				target_id: '50',
				evidence_ids: ['evidence-weekly-sales'],
				guidance: 'recompute or retract this figure'
			}
		},
		{
			label: 'replayed the recorded query; same result',
			ok: true,
			outcome: 'passed',
			finding: {
				code: 'replay_match',
				effect: 'informational',
				target: 'answer',
				evidence_ids: []
			}
		},
		{
			label: 'an expensive query was not re-run',
			ok: true,
			outcome: 'not_run',
			detail: 'The saved result is retained without a replay comparison.'
		},
		{
			label: 'legacy check found an issue without scope metadata',
			ok: false
		}
	];
	await installDesktopMock(page, {
		questionRunOutcomes: [{
			status: 'incomplete',
			reason: 'The available result does not support every claim; review the flagged $50 figure.',
			verification_status: 'needs_review',
			answer_text: 'Weekly sales were 40.',
			checks,
			evidence: [queryEvidence]
		}]
	});
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill('Explain weekly sales');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await page.getByRole('button', { name: 'Analyze main Question' }).click();
	await page.getByRole('region', { name: 'Review this run' }).getByRole('button', { name: 'Run this Question' }).click();
	await expectQuestionRunSettled(page);
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.').last()).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	// The check leaves this Question partial. Its documented gap may be closed,
	// but neither the Question nor Investigation may be described as fully answered.
	const affectedQuestion = page.locator('.root-question-item');
	await expect(affectedQuestion.locator('[data-slot="badge"][data-question-status]')).toHaveText('Partial answer');
	await expect(affectedQuestion.locator('.question-status-reason')).toContainText(
		'The available result does not support every claim; review the flagged $50 figure.'
	);
	const coverage = page.getByRole('region', { name: 'Question coverage' });
	await expect(coverage).toContainText(
		'Ready to close with 1 documented gap; this does not mean every Question was fully answered.'
	);
	await expect(page.getByRole('button', { name: 'Mark complete' })).toBeEnabled();
	await page.getByRole('button', { name: 'Mark complete' }).click();
	await expect(coverage).toContainText('Investigation closed with documented gaps. It is not fully answered');
	await expect(coverage).not.toContainText('Investigation fully answered.');
	await expandInvestigationHistory(page);
	await page.getByRole('button', { name: 'Open saved analysis run 1: Explain weekly sales' }).click();

	const runDetail = page.getByRole('region', { name: 'Saved analysis run' });
	const outputEvidence = runDetail.getByRole('region', { name: 'Query and output evidence' });
	await expect(outputEvidence.getByRole('region', { name: 'Verification outcome' })).toHaveAttribute('data-verification-status', 'needs_review');
	const queryStep = outputEvidence.locator('.step').filter({ hasText: 'Calculate weekly sales' });
	const claimCheck = queryStep.locator('[data-verification-check][data-verification-code="unsupported_claim"]');
	await expect(claimCheck).toHaveAttribute('data-check-outcome', 'flagged');
	await expect(claimCheck.locator('.check-outcome')).toHaveText('Flagged');
	await expect(claimCheck.locator('.check-outcome')).not.toHaveText('Passed');
	await expect(claimCheck).toContainText('No recorded result contains 50.');
	await claimCheck.getByText('What this check tests—and does not').click();
	await expect(claimCheck).toContainText('The named claim was not supported by the recorded evidence');
	await expect(claimCheck).toContainText('Lack of support does not prove the claim false');
	await expect(claimCheck).toContainText('recompute or retract this figure');

	const runWide = outputEvidence.getByRole('heading', { name: 'Run-wide checks' }).locator('..');
	await expect(runWide.locator('[data-check-outcome="passed"]')).toContainText('replayed the recorded query');
	await expect(runWide.locator('[data-check-outcome="not_run"]')).toContainText('Not run');
	const legacyCheck = runWide.locator('[data-verification-check]').filter({ hasText: 'legacy check found an issue' });
	await expect(legacyCheck).toHaveAttribute('data-check-outcome', 'flagged');
	await legacyCheck.getByText('What this check tests—and does not').click();
	await expect(legacyCheck).toContainText('no structured scope note was saved');
});

test('each Question opens its saved answer with explicit partial or complete coverage and run provenance', async ({ page }) => {
	await installDesktopMock(page, {
		questionRunOutcomes: [
			{
				status: 'incomplete',
				reason: 'Only 10 of the requested 12 weeks are present.',
				answer_text: 'Partial result: signups rose 8% across the 10 available weeks.'
			},
			{
				status: 'answered',
				answer_text: 'Refunds were highest in the online channel.'
			}
		]
	});
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill('Explain weekly signup and refund patterns');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await page.getByRole('button', { name: 'Analyze main Question' }).click();
	await page.getByRole('region', { name: 'Review this run' }).getByRole('button', { name: 'Run this Question' }).click();
	await expectQuestionRunSettled(page);
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();

	const mainQuestion = page.locator('.root-question-item');
	await expect(mainQuestion.locator('[data-slot="badge"][data-question-status]')).toHaveText('Partial answer');
	await mainQuestion.getByRole('button', { name: 'View partial answer for Main Question' }).click();
	const partialPreview = mainQuestion.getByRole('region', { name: 'Latest answer for Main Question' });
	await expect(partialPreview).toContainText('Partial result: signups rose 8% across the 10 available weeks.');
	await expect(partialPreview).toContainText('fixture-question-turn-1');
	await partialPreview.getByRole('button', { name: 'Open run details' }).click();
	const runDetail = page.getByRole('region', { name: 'Saved analysis run' });
	await expect(runDetail.locator('.analysis-run-result')).toContainText('Partial result: signups rose 8% across the 10 available weeks.');
	await expect(runDetail.locator('.analysis-run-question-snapshot')).toContainText('Explain weekly signup and refund patterns');
	await runDetail.getByRole('button', { name: 'Close saved analysis run' }).click();

	await page.getByRole('button', { name: 'Add subquestion' }).click();
	await page.getByRole('textbox', { name: 'Subquestion 1' }).fill('Compare refund patterns by channel');
	await page.getByRole('button', { name: 'Save plan' }).click();
	await page.getByRole('button', { name: 'Analyze Subquestion 1' }).click();
	await page.getByRole('region', { name: 'Review this run' }).getByRole('button', { name: 'Run this Question' }).click();
	await expectQuestionRunSettled(page);
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();

	const subquestion = page.locator('.question-list .question-item').first();
	await expect(subquestion.locator('[data-slot="badge"][data-question-status]')).toHaveText('Complete answer');
	await subquestion.getByRole('button', { name: 'View complete answer for Subquestion 1' }).click();
	const completePreview = subquestion.getByRole('region', { name: 'Latest answer for Subquestion 1' });
	await expect(completePreview).toContainText('Refunds were highest in the online channel.');
	await expect(completePreview).toContainText('fixture-question-turn-2');
});

test('a multi-question Investigation carries coverage, QA, and run links into its report', async ({ page }) => {
	const rootEvidence = {
		id: 'evidence-q219-root-sales',
		tool: 'run_sql',
		args: { sql: 'SELECT SUM(net_sales) FROM sales' },
		sql: 'SELECT SUM(net_sales) FROM sales',
		result_summary: 'Observed regional sales for ten available weeks.',
		columns: ['net_sales'],
		rows: [[1400]],
		row_count: 1,
		ms: 12,
		sources: [{ source: 'sales.csv', table: 'sales' }]
	};
	const regionEvidence = {
		id: 'evidence-q219-region-sales',
		tool: 'run_sql',
		args: { sql: 'SELECT region, SUM(net_sales) FROM sales GROUP BY region' },
		sql: 'SELECT region, SUM(net_sales) FROM sales GROUP BY region',
		result_summary: 'West had the largest observed sales increase.',
		columns: ['region', 'net_sales'],
		rows: [['West', 21]],
		row_count: 1,
		ms: 9,
		sources: [{ source: 'sales.csv', table: 'sales' }]
	};
	const refundEvidence = {
		id: 'evidence-q219-refunds',
		tool: 'run_sql',
		args: { sql: 'SELECT region, SUM(refunds) FROM refunds GROUP BY region' },
		sql: 'SELECT region, SUM(refunds) FROM refunds GROUP BY region',
		result_summary: 'Refunds offset part of the West increase.',
		columns: ['region', 'refunds'],
		rows: [['West', 8]],
		row_count: 1,
		ms: 7,
		sources: [{ source: 'refunds.csv', table: 'refunds' }]
	};
	const rootQuestion = 'Assess second-quarter sales and returns across regions.';
	const regionalQuestion = 'Compare net sales growth across regions.';
	const refundQuestion = 'Check whether refunds offset the strongest regional increase.';
	const rootAnswer = 'Regional sales increased 14% over the 10 available weeks.';
	const regionalAnswer = 'The West led net sales growth at 21%.';
	const refundAnswer = 'Refunds offset 38% of the West increase.';
	await installDesktopMock(page, {
		questionRunOutcomes: [
			{
				status: 'incomplete',
				verification_status: 'needs_review',
				reason: 'Only 10 of the requested 12 weeks are present.',
				answer_text: rootAnswer,
				checks: [{
					label: 'The full twelve-week window is present',
					ok: false,
					outcome: 'flagged',
					detail: 'Only ten weeks were present.'
				}],
				evidence: [rootEvidence],
				source_scope: {
					used_sources: [{ source: 'sales.csv', table: 'sales', evidence_ids: [rootEvidence.id] }],
					skipped_sources: ['legacy-sales.csv']
				}
			},
			{
				status: 'answered',
				verification_status: 'verified',
				answer_text: regionalAnswer,
				evidence: [regionEvidence],
				source_scope: {
					used_sources: [{ source: 'sales.csv', table: 'sales', evidence_ids: [regionEvidence.id] }],
					skipped_sources: []
				}
			},
			{
				status: 'answered',
				verification_status: 'verified',
				answer_text: refundAnswer,
				evidence: [refundEvidence],
				source_scope: {
					used_sources: [{ source: 'refunds.csv', table: 'refunds', evidence_ids: [refundEvidence.id] }],
					skipped_sources: []
				}
			}
		]
	});

	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill(rootQuestion);
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await page.getByRole('button', { name: 'Edit brief' }).click();
	await page.getByRole('textbox', { name: 'Audience' }).fill('Regional manager');
	await page.getByRole('textbox', { name: 'Desired output' }).fill('A decision-ready regional summary');
	await page.getByRole('button', { name: 'Save brief' }).click();
	await page.getByRole('button', { name: 'Add subquestion' }).click();
	await page.getByRole('textbox', { name: 'Subquestion 1' }).fill(regionalQuestion);
	await page.getByRole('button', { name: 'Add subquestion' }).click();
	await page.getByRole('textbox', { name: 'Subquestion 2' }).fill(refundQuestion);
	await page.getByRole('button', { name: 'Save plan' }).click();

	const runQuestion = async (label) => {
		await page.getByRole('button', { name: `Analyze ${label}` }).click();
		await page.getByRole('region', { name: 'Review this run' }).getByRole('button', { name: 'Run this Question' }).click();
		await expectQuestionRunSettled(page);
		await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	};
	await runQuestion('main Question');

	const mainQuestion = page.locator('.root-question-item');
	await expect(mainQuestion.locator('[data-slot="badge"][data-question-status]')).toHaveText('Partial answer');
	await mainQuestion.getByRole('button', { name: 'View partial answer for Main Question' }).click();
	const mainPreview = mainQuestion.getByRole('region', { name: 'Latest answer for Main Question' });
	await expect(mainPreview).toContainText(rootAnswer);
	await expect(mainPreview).toContainText('fixture-question-turn-1');
	await mainPreview.getByRole('button', { name: 'Open run details' }).click();
	const runDetail = page.getByRole('region', { name: 'Saved analysis run' });
	await expect(runDetail.locator('.analysis-run-question-snapshot')).toContainText(rootQuestion);
	const rootRunContext = runDetail.getByRole('region', { name: 'Definitions and source scope used' });
	await expect(rootRunContext).toContainText(rootEvidence.id);
	const rootOutput = runDetail.getByRole('region', { name: 'Query and output evidence' });
	await expect(rootOutput.getByRole('region', { name: 'Verification outcome' })).toHaveAttribute('data-verification-status', 'needs_review');
	await expect(rootOutput.locator('[data-check-outcome="flagged"]')).toContainText('Only ten weeks were present.');
	await runDetail.getByRole('button', { name: 'Close saved analysis run' }).click();

	await runQuestion('Subquestion 1');
	const regionalItem = page.locator('.question-list .question-item').nth(0);
	await expect(regionalItem.getByRole('textbox', { name: 'Subquestion 1' })).toHaveValue(regionalQuestion);
	await expect(regionalItem.locator('[data-slot="badge"][data-question-status]')).toHaveText('Complete answer');
	await regionalItem.getByRole('button', { name: 'View complete answer for Subquestion 1' }).click();
	const regionalPreview = regionalItem.getByRole('region', { name: 'Latest answer for Subquestion 1' });
	await expect(regionalPreview).toContainText(regionalAnswer);
	await expect(regionalPreview).toContainText('fixture-question-turn-2');
	await regionalPreview.getByRole('button', { name: 'Open run details' }).click();
	await expect(runDetail.locator('.analysis-run-question-snapshot')).toContainText(regionalQuestion);
	await expect(runDetail.getByRole('region', { name: 'Definitions and source scope used' })).toContainText(regionEvidence.id);
	await runDetail.getByRole('button', { name: 'Close saved analysis run' }).click();

	await runQuestion('Subquestion 2');
	const refundItem = page.locator('.question-list .question-item').nth(1);
	await expect(refundItem.getByRole('textbox', { name: 'Subquestion 2' })).toHaveValue(refundQuestion);
	await expect(refundItem.locator('[data-slot="badge"][data-question-status]')).toHaveText('Complete answer');
	await refundItem.getByRole('button', { name: 'View complete answer for Subquestion 2' }).click();
	const refundPreview = refundItem.getByRole('region', { name: 'Latest answer for Subquestion 2' });
	await expect(refundPreview).toContainText(refundAnswer);
	await expect(refundPreview).toContainText('fixture-question-turn-3');
	await refundPreview.getByRole('button', { name: 'Open run details' }).click();
	await expect(runDetail.locator('.analysis-run-question-snapshot')).toContainText(refundQuestion);
	await expect(runDetail.getByRole('region', { name: 'Definitions and source scope used' })).toContainText(refundEvidence.id);
	await runDetail.getByRole('button', { name: 'Close saved analysis run' }).click();

	const downloadPromise = page.waitForEvent('download');
	await page.getByRole('button', { name: 'Export report' }).click();
	const download = await downloadPromise;
	expect(download.suggestedFilename()).toBe('assess-second-quarter-sales-and-returns-across-regions-report.md');
	const report = await readFile(await download.path(), 'utf8');
	for (const expected of [
		'Assess second-quarter sales and returns across regions.',
		'Regional manager',
		'A decision-ready regional summary',
		'Partial answer',
		'Only 10 of the requested 12 weeks are present.',
		rootAnswer,
		regionalQuestion,
		regionalAnswer,
		refundQuestion,
		refundAnswer,
		'needs review',
		'flagged: The full twelve-week window is present',
		'sales.csv (sales)',
		'refunds.csv (refunds)',
		'legacy-sales.csv',
		rootEvidence.id,
		regionEvidence.id,
		refundEvidence.id,
		'conversation transcript is excluded'
	]) {
		expect(report.toLowerCase()).toContain(expected.toLowerCase());
	}
	expect(report.indexOf(`Subquestion 1: ${regionalQuestion}`)).toBeLessThan(report.indexOf(`Subquestion 2: ${refundQuestion}`));
	expect(report).not.toContain('Scoped to C:\\FellaFixture\\northwind-sales.');
});

test('saved analysis runs open from an Investigation and link to every Question they address', async ({ page }) => {
	await installDesktopMock(page, { questionRunOutcomes: [{ status: 'answered', delay_ms: 250 }] });
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill('Explain how refund rates changed');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await page.getByRole('button', { name: 'Analyze main Question' }).click();
	await page.getByRole('region', { name: 'Review this run' }).getByRole('button', { name: 'Run this Question' }).click();
	const questionRun = page.getByRole('region', { name: 'Question run' }).last();
	await expect(questionRun).toBeVisible();
	await expect(questionRun).toHaveAttribute('data-question-id', /^q-root-/);
	await expectQuestionRunSettled(page);
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.').last()).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();

	await expandInvestigationHistory(page);
	const savedRunLink = page.getByRole('button', { name: 'Open saved analysis run 1: Explain how refund rates changed' });
	await expect(savedRunLink).toBeVisible();
	await savedRunLink.click();
	const runDetail = page.getByRole('region', { name: 'Saved analysis run' });
	await expect(runDetail).toBeVisible();
	await expect(runDetail.locator('.analysis-run-question-snapshot')).toContainText('Explain how refund rates changed');
	await runDetail.getByRole('button', { name: 'Close saved analysis run' }).click();
	const mainQuestion = page.locator('.root-question-item');
	await mainQuestion.getByRole('button', { name: 'View complete answer for Main Question' }).click();
	await mainQuestion.getByRole('button', { name: 'Open run details' }).click();
	await expect(runDetail).toBeVisible();
	await runDetail.getByRole('button', { name: 'Close saved analysis run' }).click();

	await page.getByRole('button', { name: 'Add subquestion' }).click();
	await page.getByRole('textbox', { name: 'Subquestion 1' }).fill('Compare refunds by sales channel');
	await page.getByRole('button', { name: 'Save plan' }).click();
	await savedRunLink.click();
	await runDetail.locator('.analysis-run-question-links input[type="checkbox"]').nth(1).check();
	await runDetail.getByRole('button', { name: 'Save Question links' }).click();

	const subquestionRunLink = page.getByRole('button', { name: 'Open linked analysis run 1 for Subquestion 1' });
	await expect(subquestionRunLink).toBeVisible();
	await subquestionRunLink.click();
	await expect(runDetail.locator('.analysis-run-question-snapshot')).toContainText('Explain how refund rates changed');
	await expect(runDetail.locator('.analysis-run-question-links input[type="checkbox"]').nth(1)).toBeChecked();
});

test('an analyst can review a run assumption, correct it in Investigation context, and keep the old run intact', async ({ page }) => {
	const recordedAssumption = 'Treat accounts with no cancellation date as active.';
	const correctedAssumption = 'Treat accounts with no cancellation date as active only when their last activity is within 90 days.';
	await installDesktopMock(page, {
		questionRunOutcomes: [
			{ status: 'answered', delay_ms: 250, contract: { assumptions: [recordedAssumption] } }
		]
	});
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill('Explain weekly account retention');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await page.getByRole('button', { name: 'Analyze main Question' }).click();
	await page.getByRole('region', { name: 'Review this run' }).getByRole('button', { name: 'Run this Question' }).click();
	await expectQuestionRunSettled(page);
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.').last()).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();

	await expandInvestigationHistory(page);
	const runLink = page.getByRole('button', { name: 'Open saved analysis run 1: Explain weekly account retention' });
	await runLink.click();
	const runDetail = page.getByRole('region', { name: 'Saved analysis run' });
	const runAssumptions = runDetail.locator('.analysis-run-assumptions');
	await expect(runAssumptions).toContainText(recordedAssumption);
	await expect(runAssumptions).toContainText('not observed facts');
	await runAssumptions.getByRole('button', { name: `Add assumption to editable context: ${recordedAssumption}` }).click();

	const contextNote = page.getByRole('textbox', { name: 'Context note 1' });
	await expect(contextNote).toHaveValue(recordedAssumption);
	await contextNote.fill(correctedAssumption);
	await page.getByRole('button', { name: 'Save context' }).click();
	await expect(page.locator('.definition-editor')).toContainText('Context saved');
	expect((await page.evaluate(() => window.__workspaceInvestigations()[0])).definitions).toMatchObject([
		{ kind: 'assumption', text: correctedAssumption }
	]);

	// Correcting future context never rewrites the recorded interpretation of
	// the historical run currently open beside it.
	await expect(runAssumptions).toContainText(recordedAssumption);
	await expect(runAssumptions).not.toContainText(correctedAssumption);
	await page.getByRole('button', { name: 'Analyze main Question' }).click();
	const nextRun = page.getByRole('region', { name: 'Review this run' });
	await expect(nextRun).toContainText(correctedAssumption);
	await expect(nextRun).not.toContainText(recordedAssumption);
	await nextRun.getByRole('button', { name: 'Cancel' }).click();
});

test('saved Investigation definitions are revisioned, attached to later asks, and removable', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	const composer = page.getByRole('combobox', { name: 'Ask about northwind-sales' });
	await composer.fill('Explain the change in weekly sales');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	expect(await page.locator('.definition-disclosure').evaluate((element) => element.open)).toBe(false);
	await expandInvestigationDefinitions(page);
	await expect(page.locator('.definition-disclosure > summary')).toContainText('Definitions & assumptions');

	await page.getByRole('button', { name: 'Add definition or assumption' }).click();
	await page.getByRole('combobox', { name: 'Context type 1' }).selectOption('assumption');
	const note = page.getByRole('textbox', { name: 'Context note 1' });
	await note.fill('A missing cancellation date means the account is still active.');
	await page.getByRole('button', { name: 'Save context' }).click();
	await expect(page.locator('.definition-editor')).toContainText('Context saved');

	let saved = await page.evaluate(() => window.__workspaceInvestigations()[0]);
	expect(saved.definitions).toHaveLength(1);
	expect(saved.definitions[0]).toMatchObject({
		kind: 'assumption',
		text: 'A missing cancellation date means the account is still active.',
		revision: 1,
		status: 'active'
	});
	const definitionId = saved.definitions[0].id;
	const firstRecord = await page.evaluate((id) => window.fella.invoke('investigation_definition_load', { definitionId: id }), definitionId);
	expect(firstRecord).toMatchObject({ id: definitionId, investigation_id: saved.id, revision: 1, history: [] });

	await note.fill('A missing cancellation date is not sufficient evidence of an active account.');
	await page.getByRole('button', { name: 'Save context' }).click();
	await expect(page.locator('.definition-item')).toContainText('Saved revision 2');
	saved = await page.evaluate(() => window.__workspaceInvestigations()[0]);
	const revised = await page.evaluate((id) => window.fella.invoke('investigation_definition_load', { definitionId: id }), definitionId);
	expect(revised).toMatchObject({ revision: 2, text: 'A missing cancellation date is not sufficient evidence of an active account.' });
	expect(revised.history).toHaveLength(1);
	expect(revised.history[0]).toMatchObject({ revision: 1, text: 'A missing cancellation date means the account is still active.' });

	await page.getByRole('button', { name: 'Open conversation', exact: true }).click();
	await expect(composer).toBeVisible();
	await composer.fill('Check whether this assumption changes the result.');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.').last()).toBeVisible();
	const nextAsk = await page.evaluate(() => window.__workspaceAskCalls.at(-1));
	expect(nextAsk.investigationId).toBe(saved.id);
	expect(nextAsk.question).toContain('Check whether this assumption changes the result.');
	const transcript = page.locator('.msg.user').last();
	await expect(transcript).toContainText('Check whether this assumption changes the result.');
	await expect(transcript).not.toContainText('missing cancellation date is not sufficient evidence');

	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await expandInvestigationDefinitions(page);
	await expect(page.locator('.definition-disclosure > summary')).toContainText('Definitions & assumptions');
	await page.getByRole('button', { name: 'Remove context note 1' }).click();
	await page.getByRole('button', { name: 'Save context' }).click();
	await expect(page.locator('.definition-empty')).toContainText('No saved definitions or assumptions');
	saved = await page.evaluate(() => window.__workspaceInvestigations()[0]);
	expect(saved.definitions).toEqual([]);
	const removed = await page.evaluate((id) => window.fella.invoke('investigation_definition_load', { definitionId: id }), definitionId);
	expect(removed).toMatchObject({ status: 'removed', revision: 3 });
	expect(removed.history).toHaveLength(2);
});

test('saved run review exposes its method, query output, definitions, and actual source scope in the Investigation', async ({ page }) => {
	const query = 'SELECT cohort_month, retained / cohort_size AS retention FROM cohort_retention ORDER BY cohort_month';
	const evidence = {
		id: 'review-cohort-query',
		tool: 'run_sql',
		note: 'Calculate retention by cohort',
		sql: query,
		result_summary: 'Three monthly cohorts were returned; retention ranged from 42% to 51%.',
		columns: ['cohort_month', 'retention'],
		rows: [['2026-01', 0.42], ['2026-02', 0.47], ['2026-03', 0.51]],
		row_count: 3,
		sources: [{ source: 'sales.csv', table: 'cohort_retention' }]
	};
	await installDesktopMock(page, {
		detailEvidence: evidence,
		questionRunOutcomes: [{
			status: 'answered',
			answer_text: 'Retention increased across the three monthly cohorts.',
			source_scope: {
				used_sources: [{ source: 'sales.csv', table: 'cohort_retention', evidence_ids: [evidence.id] }],
				skipped_sources: ['legacy-retention.xlsx: encrypted workbook']
			}
		}]
	});
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill('Compare monthly retention by signup cohort');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	const readiness = page.getByTestId('investigation-data-readiness');
	await expect(readiness.getByTestId('current-catalog-revision')).toContainText('sales-r1');
	await expect(readiness.getByRole('heading', { name: 'Cataloged sources' })).toBeVisible();
	await expect(readiness.locator('.scope-controls legend')).toContainText('future runs');
	await page.getByRole('button', { name: 'Add definition or assumption' }).click();
	await page.getByRole('textbox', { name: 'Context note 1' }).fill('One row represents one signup-month cohort; retention is retained accounts divided by cohort accounts.');
	await page.getByRole('button', { name: 'Save context' }).click();
	await expect(page.locator('.definition-editor')).toContainText('Context saved');
	await page.getByRole('button', { name: 'Analyze main Question' }).click();
	await page.getByRole('region', { name: 'Review this run' }).getByRole('button', { name: 'Run this Question' }).click();
	await expectQuestionRunSettled(page);
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();

	await expandInvestigationHistory(page);
	const runLink = page.getByRole('button', { name: 'Open saved analysis run 1: Compare monthly retention by signup cohort' });
	await runLink.click();
	const runDetail = page.getByRole('region', { name: 'Saved analysis run' });
	await expect(runDetail.locator('.analysis-run-question-snapshot')).toContainText('Compare monthly retention by signup cohort');
	const runContext = runDetail.getByRole('region', { name: 'Definitions and source scope used' });
	await expect(runContext).toContainText('All sources in the Question’s Investigation scope');
	await expect(runContext).toContainText('sales.csv · cohort_retention');
	await expect(runContext).toContainText(evidence.id);
	await expect(runContext).toContainText('legacy-retention.xlsx: encrypted workbook');
	await expect(runContext).toContainText('Definition · revision 1');
	await expect(runContext).toContainText('One row represents one signup-month cohort; retention is retained accounts divided by cohort accounts.');
	await expect(runDetail.locator('.analysis-run-facts')).toContainText('Source catalog revision');
	await expect(runDetail.locator('.analysis-run-facts')).toContainText('sales-r1');
	await expect(runContext.locator('.analysis-run-source-provenance')).toContainText('Requested for this run');
	await expect(runContext.locator('.analysis-run-source-provenance')).toContainText('Observed in recorded evidence');

	// Changing current Investigation scope does not rewrite the saved run's snapshot.
	await page.getByLabel('Choose files').check();
	await page.getByRole('checkbox', { name: 'Include sales.csv in this Investigation' }).uncheck();
	await expect.poll(() => page.evaluate(() => window.__workspaceInvestigations()[0].source_selection)).toEqual({
		mode: 'selected_sources',
		included_paths: []
	});
	await expect(runContext).toContainText('All sources in the Question’s Investigation scope');
	await expect(runContext).toContainText('sales.csv · cohort_retention');

	const runTimeline = runDetail.getByRole('region', { name: 'Question run' });
	await expect(runTimeline).toBeVisible();
	await expect(runTimeline).toContainText('Calculating from the data');
	await expect(runTimeline).toContainText('Three monthly cohorts were returned; retention ranged from 42% to 51%.');
	const queryTimelineStep = runTimeline.locator('.run-step').filter({
		hasText: 'Three monthly cohorts were returned; retention ranged from 42% to 51%.'
	});
	await queryTimelineStep.getByRole('button', { name: 'Inspect output' }).click();
	const evidenceReview = runDetail.getByRole('region', { name: 'Query and output evidence' });
	await expect(evidenceReview).toContainText('Calculate retention by cohort');
	await evidenceReview.getByRole('button', { name: 'show the query' }).click();
	await expect(evidenceReview).toContainText(query);
	await expect(evidenceReview).toContainText('Three monthly cohorts were returned; retention ranged from 42% to 51%.');
	await expect(evidenceReview).toContainText('2026-03');
});

test('repeated Question runs stay chronological and preserve each attempt context', async ({ page }) => {
	const firstEvidence = {
		id: 'attempt-one-evidence',
		tool: 'run_sql',
		note: 'First attempt query',
		sql: 'SELECT SUM(net_sales) FROM sales',
		result_summary: 'First attempt returned 120.',
		columns: ['net_sales'],
		rows: [[120]],
		row_count: 1,
		sources: [{ source: 'sales.csv', table: 'sales' }]
	};
	const secondEvidence = {
		id: 'attempt-two-evidence',
		tool: 'run_sql',
		note: 'Second attempt query',
		sql: 'SELECT SUM(refunds) FROM returns',
		result_summary: 'Second attempt returned 18.',
		columns: ['refunds'],
		rows: [[18]],
		row_count: 1,
		sources: [{ source: 'returns.csv', table: 'refunds' }]
	};
	await installDesktopMock(page, {
		questionRunOutcomes: [
			{
				status: 'incomplete',
				answer_text: 'Attempt one: net sales were 120.',
				plan: { strategy: 'compiled_sql', steps: ['inspect_sales', 'run_sql'] },
				evidence: [firstEvidence],
				source_scope: {
					used_sources: [{ source: 'sales.csv', table: 'sales', evidence_ids: [firstEvidence.id] }],
					skipped_sources: []
				}
			},
			{
				status: 'answered',
				answer_text: 'Attempt two: refunds were 18.',
				plan: { strategy: 'python_fallback', steps: ['inspect_returns', 'run_python'] },
				evidence: [secondEvidence],
				source_scope: {
					used_sources: [{ source: 'returns.csv', table: 'refunds', evidence_ids: [secondEvidence.id] }],
					skipped_sources: []
				}
			}
		]
	});
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill('Explain weekly sales');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await page.getByRole('button', { name: 'Add definition or assumption' }).click();
	const savedDefinition = page.getByRole('textbox', { name: 'Context note 1' });
	await savedDefinition.fill('Net sales exclude refunds.');
	await page.getByRole('button', { name: 'Save context' }).click();
	await expect(page.locator('.definition-editor')).toContainText('Context saved');

	const runQuestion = async () => {
		await page.getByRole('button', { name: 'Analyze main Question' }).click();
		await page.getByRole('region', { name: 'Review this run' }).getByRole('button', { name: 'Run this Question' }).click();
		await expectQuestionRunSettled(page);
		await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	};
	await runQuestion();
	await expandInvestigationDefinitions(page);
	await savedDefinition.fill('Net sales include refunds.');
	await page.getByRole('button', { name: 'Save context' }).click();
	await expect(page.locator('.definition-item')).toContainText('Saved revision 2');
	await runQuestion();

	const history = page.locator('.analysis-runs');
	await expandInvestigationHistory(page);
	const attempts = history.locator('.analysis-run-link');
	await expect(attempts).toHaveCount(2);
	await expect(attempts.nth(0)).toHaveAttribute('aria-label', 'Open saved analysis run 1: Explain weekly sales');
	await expect(attempts.nth(1)).toHaveAttribute('aria-label', 'Open saved analysis run 2: Explain weekly sales');

	await attempts.nth(0).click();
	const runDetail = page.getByRole('region', { name: 'Saved analysis run' });
	await expect(runDetail.locator('.analysis-run-facts')).toContainText('compiled sql');
	await expect(runDetail.locator('.analysis-run-facts')).toContainText('Original saved attempt');
	await expect(runDetail.locator('.analysis-run-result')).toContainText('Attempt one: net sales were 120.');
	await expect(runDetail.getByRole('region', { name: 'Definitions and source scope used' })).toContainText('Definition · revision 1');
	await expect(runDetail.getByRole('region', { name: 'Definitions and source scope used' })).toContainText('Net sales exclude refunds.');
	await expect(runDetail.getByRole('region', { name: 'Definitions and source scope used' })).toContainText('sales.csv · sales');
	await runDetail.getByRole('button', { name: 'Close saved analysis run' }).click();

	await attempts.nth(1).click();
	await expect(attempts.nth(1)).toContainText('rerun of Run 1');
	await expect(runDetail.locator('.analysis-run-facts')).toContainText('python fallback');
	await expect(runDetail.getByRole('button', { name: 'Open parent run 1' })).toBeVisible();
	await expect(runDetail.locator('.analysis-run-result')).toContainText('Attempt two: refunds were 18.');
	await expect(runDetail.getByRole('region', { name: 'Definitions and source scope used' })).toContainText('Definition · revision 2');
	await expect(runDetail.getByRole('region', { name: 'Definitions and source scope used' })).toContainText('Net sales include refunds.');
	await expect(runDetail.getByRole('region', { name: 'Definitions and source scope used' })).toContainText('returns.csv · refunds');
	await runDetail.getByRole('button', { name: 'Open parent run 1' }).click();
	await expect(runDetail.locator('.analysis-run-result')).toContainText('Attempt one: net sales were 120.');
});

test('Finding caveats and assumptions are visible, editable, and preserve provenance', async ({ page }) => {
	const supportingEvidence = {
		id: 'finding-evidence-retention',
		tool: 'run_sql',
		result_summary: 'Observed retention by signup cohort across the selected period.',
		sql: 'SELECT cohort, retention_rate FROM cohort_retention',
		sources: [{ source: 'sales.csv', table: 'cohort_retention' }]
	};
	await installDesktopMock(page, {
		detailEvidence: supportingEvidence,
		questionRunOutcomes: [{
			status: 'answered',
			answer_text: 'Retention rose by 14% across the observed cohorts.'
		}]
	});
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill('Explain retention by signup cohort.');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await page.getByRole('button', { name: 'Analyze main Question' }).click();
	await page.getByRole('region', { name: 'Review this run' }).getByRole('button', { name: 'Run this Question' }).click();
	await expect(page.getByText('Retention rose by 14% across the observed cohorts.')).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await expandInvestigationHistory(page);
	await page.getByRole('button', { name: /Open saved analysis run 1:/ }).click();

	const capture = page.getByRole('region', { name: 'Saved analysis run' }).locator('.finding-capture');
	await expect(capture.getByRole('textbox', { name: 'Finding claim' })).toHaveValue('Retention rose by 14% across the observed cohorts.');
	await capture.getByRole('textbox', { name: 'Finding caveats' }).fill('The latest cohort is incomplete.');
	await capture.getByRole('textbox', { name: 'Finding assumptions' }).fill('Each account is counted once per cohort.');
	await expect(capture).toContainText('Observed retention by signup cohort across the selected period.');
	await capture.getByRole('button', { name: 'Save Finding' }).click();

	const findingCard = page.locator('.finding-card');
	await expect(findingCard).toContainText('Retention rose by 14% across the observed cohorts.');
	await expect(findingCard).toContainText('Explain retention by signup cohort.');
	await expect(findingCard).toContainText('finding-evidence-retention');
	const originalFinding = await page.evaluate(() => window.__workspaceFindingRecords()[0]);
	expect(originalFinding).toMatchObject({
		claim: 'Retention rose by 14% across the observed cohorts.',
		caveats: ['The latest cohort is incomplete.'],
		assumptions: ['Each account is counted once per cohort.'],
		question_id: expect.any(String),
		analysis_turn_id: 'fixture-question-turn-1',
		evidence_ids: ['finding-evidence-retention']
	});
	await expect(findingCard.getByRole('region', { name: 'Interpretation caveats' })).toContainText('The latest cohort is incomplete.');
	await expect(findingCard.getByRole('region', { name: 'Assumptions' })).toContainText('Each account is counted once per cohort.');
	await findingCard.getByRole('button', { name: 'Edit finding' }).click();
	await page.getByRole('textbox', { name: 'Edit Finding claim' }).fill('Retention increased in every observed cohort.');
	await page.getByRole('textbox', { name: 'Edit Finding caveats' }).fill('The latest cohort is incomplete.\nRefunds are not included in this export.');
	await page.getByRole('textbox', { name: 'Edit Finding assumptions' }).fill('Each account is counted once per cohort.');
	await findingCard.getByRole('button', { name: 'Save finding' }).click();
	await expect(findingCard).toContainText('Retention increased in every observed cohort.');
	await expect(findingCard.getByRole('region', { name: 'Interpretation caveats' })).toContainText('Refunds are not included in this export.');
	await expect(findingCard.getByRole('region', { name: 'Assumptions' })).toContainText('Each account is counted once per cohort.');
	const editedFinding = await page.evaluate(() => window.__workspaceFindingRecords()[0]);
	expect(editedFinding).toMatchObject({
		id: originalFinding.id,
		claim: 'Retention increased in every observed cohort.',
		caveats: ['The latest cohort is incomplete.', 'Refunds are not included in this export.'],
		assumptions: ['Each account is counted once per cohort.'],
		question_id: originalFinding.question_id,
		analysis_turn_id: originalFinding.analysis_turn_id,
		evidence_ids: originalFinding.evidence_ids
	});
	await page.getByRole('button', { name: 'Open conversation', exact: true }).click();
	await expect(page.getByRole('combobox', { name: 'Ask about northwind-sales' })).toBeVisible();
	// Running the saved Question adds a fresh turn; the original request remains first.
	const originalRequest = page.locator('.msg.user').first().locator('.you-copy');
	await expect(originalRequest).toHaveText('Explain retention by signup cohort.');
});

test('Investigation results summarize saved work, render linked charts, and keep open Questions visible', async ({ page }) => {
	await page.context().grantPermissions(['clipboard-read', 'clipboard-write'], { origin: 'http://127.0.0.1:1421' });
	const chartSource = {
		id: 'results-source-evidence',
		tool: 'run_sql',
		args: { sql: 'SELECT month, SUM(net_sales) AS net_sales FROM sales GROUP BY month ORDER BY month' },
		sql: 'SELECT month, SUM(net_sales) AS net_sales FROM sales GROUP BY month ORDER BY month',
		result_summary: 'Three monthly net sales totals from sales.csv.',
		sources: [{ source: 'sales.csv', table: 'sales' }],
		columns: ['month', 'net_sales'],
		rows: [['Jan', 120], ['Feb', 146], ['Mar', 139]],
		row_count: 3,
		ms: 6
	};
	const chart = {
		id: 'results-chart-evidence',
		tool: 'make_chart',
		args: {},
		result_summary: 'Monthly net sales chart, three months.',
		ms: 8,
		chart: {
			kind: 'line',
			title: 'Monthly net sales',
			labels: ['Jan', 'Feb', 'Mar'],
			series: [{ name: 'Net sales', values: [120, 146, 139] }],
			unit: '$',
			x_label: 'Month',
			y_label: 'Net sales',
			metadata: {
				source_label: 'sales.csv',
				source_evidence_id: 'results-source-evidence',
				source_run_id: 'fixture-question-turn-1',
				finding_snapshot: 'Net sales rose from January to February.',
				fields: ['month', 'net_sales'],
				aggregation: 'sum',
				filters: [],
				part_to_whole: false
			}
		}
	};
	await installDesktopMock(page, {
		chartEvidence: [chartSource, chart],
		questionRunOutcomes: [{
			status: 'answered',
			answer_text: 'Net sales rose 22% from January to February, then eased slightly in March.',
			source_scope: {
				used_sources: [{ source: 'sales.csv', table: 'sales', evidence_ids: [chartSource.id] }],
				skipped_sources: ['legacy_sales.csv']
			}
		}]
	});
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill('Explain the monthly net sales trend.');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await page.getByRole('button', { name: 'Edit brief' }).click();
	await page.getByRole('textbox', { name: 'Audience' }).fill('Finance lead');
	await page.getByRole('textbox', { name: 'Desired output' }).fill('A concise monthly trend summary');
	await page.getByRole('button', { name: 'Save brief' }).click();
	await page.getByRole('button', { name: 'Add subquestion' }).click();
	await page.getByRole('textbox', { name: 'Subquestion 1' }).fill('Check whether discounts explain the March dip.');
	await page.getByRole('button', { name: 'Save plan' }).click();
	await page.getByRole('button', { name: 'Analyze main Question' }).click();
	await page.getByRole('region', { name: 'Review this run' }).getByRole('button', { name: 'Run this Question' }).click();
	await expect(page.getByText('Net sales rose 22% from January to February, then eased slightly in March.')).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await expandInvestigationHistory(page);
	await page.getByRole('button', { name: /Open saved analysis run 1:/ }).click();
	const capture = page.getByRole('region', { name: 'Saved analysis run' }).locator('.finding-capture');
	await capture.getByRole('textbox', { name: 'Finding caveats' }).fill('March results include only the first three weeks.');
	await capture.getByRole('button', { name: 'Save Finding' }).click();
	await expect(page.locator('.finding-card')).toContainText('Net sales rose 22% from January to February');

	await page.getByRole('button', { name: 'Results' }).click();
	const results = page.getByRole('region', { name: 'Current Investigation results' });
	await expect(page.getByRole('heading', { name: 'Investigation results' })).toBeVisible();
	await expect(results.locator('#results-goal-title')).toHaveText('Explain the monthly net sales trend.');
	await expect(results).toContainText('Finance lead');
	await expect(results).toContainText('A concise monthly trend summary');
	await expect(results).toContainText('1/2 Questions answered');
	await expect(results.locator('.results-finding')).toContainText('Net sales rose 22% from January to February');
	await expect(results.locator('.results-finding')).toContainText('Answers: Explain the monthly net sales trend.');
	const findingDetails = results.locator('.results-finding details');
	const findingArtifact = results.getByRole('article', { name: 'Finding 1' });
	await expect(findingArtifact).toContainText('Analyst-reviewed Finding · Revision 1');
	await findingArtifact.getByRole('button', { name: 'Copy Finding claim' }).click();
	await expect(findingArtifact.getByRole('status')).toHaveText('Finding claim copied.');
	expect(await page.evaluate(() => navigator.clipboard.readText())).toBe('Net sales rose 22% from January to February, then eased slightly in March.');
	await findingDetails.locator('summary').click();
	await expect(findingDetails).toContainText('March results include only the first three weeks.');
	await expect(findingDetails).toContainText('results-chart-evidence');
	await expect(findingDetails).toContainText('fixture-question-turn-1');
	const resultChart = results.locator('.chart-card');
	await expect(resultChart).toContainText('Monthly net sales');
	await expect(resultChart).toContainText('Model-produced visualization');
	await expect(resultChart.locator('[data-interpretation-source="model"]')).toContainText('Model interpretation');
	await expect(resultChart.locator('[data-interpretation-source="model"]')).toContainText('Net sales rose from January to February.');
	await expect(resultChart).toContainText('sales.csv');
	const artifact = results.getByRole('article', { name: 'Visualization artifact from Explain the monthly net sales trend.' });
	const lineage = artifact.locator('.results-artifact-lineage');
	await lineage.locator('summary').click();
	await expect(lineage).toContainText('sales.csv · sales');
	await expect(lineage).toContainText('legacy_sales.csv');
	await expect(lineage).toContainText('results-chart-evidence');
	await expect(lineage).toContainText('results-source-evidence');
	await expect(lineage).toContainText('fixture-question-turn-1');
	await expect(lineage.getByRole('link', { name: /Revision 1 · Net sales rose 22%/ })).toHaveAttribute('href', '#finding-finding-1');
	const openQuestion = results.locator('.results-question').filter({ hasText: 'Check whether discounts explain the March dip.' });
	await expect(openQuestion).toContainText('Not answered');
	await expect(openQuestion).toContainText('No analysis run is linked to this Question yet.');
	const answers = results.locator('.results-answer-index');
	await answers.locator(':scope > details > summary').click();
	const savedAnswer = answers.getByText('Net sales rose 22% from January to February, then eased slightly in March.');
	await expect(savedAnswer).toHaveCount(1);
	await expect(savedAnswer).toBeHidden();
	await answers.locator('details details > summary').click();
	await expect(savedAnswer).toBeVisible();
	const chartExport = resultChart.getByRole('button', { name: 'Export chart and data' });
	await chartExport.click();
	await page.getByRole('menuitem', { name: 'Copy data' }).click();
	await expect(resultChart.getByRole('status')).toHaveText('Chart data copied to clipboard.');
	expect(await page.evaluate(() => navigator.clipboard.readText())).toContain('Net sales');
	await results.locator('.results-chart').getByRole('button', { name: 'Open analysis record' }).click();
	await expect(page.getByRole('button', { name: 'Work plan', exact: true })).toHaveAttribute('aria-pressed', 'true');
	await expect(page.getByRole('region', { name: 'Saved analysis run' })).toBeVisible();
});

test('Investigation Results keep transcript-only historical work outside current Question coverage', async ({ page }) => {
	await installDesktopMock(page, {
		directAnswerText: 'The initial scan found four relevant files and no single source of truth.',
		questionRunOutcomes: [{ status: 'answered', answer_text: 'The weekly total increased by 12%.' }],
		replayStatus: {
			turn_id: 'fixture-turn',
			same_workspace: true,
			revision_changed: false,
			snapshot_available: true,
			can_rerun: false,
			source_changes: []
		}
	});
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	const composer = page.getByRole('combobox', { name: 'Ask about northwind-sales' });
	await composer.fill('What do the weekly sales files show?');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.locator('.msg.assistant').first()).toContainText('The initial scan found four relevant files');
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await page.getByRole('button', { name: 'Analyze main Question' }).click();
	await page.getByRole('region', { name: 'Review this run' }).getByRole('button', { name: 'Run this Question' }).click();
	await expect(page.getByText('The weekly total increased by 12%.')).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await page.getByRole('button', { name: 'Results' }).click();

	const results = page.getByRole('region', { name: 'Current Investigation results' });
	const historical = results.getByRole('region', { name: 'Historical and unassigned analysis' });
	await expect(results).toContainText('1/1 Questions answered');
	await expect(results.locator('.results-finding')).toHaveCount(0);
	await expect(historical.locator('.results-historical-run')).toHaveCount(1);
	await expect(historical).toContainText('Transcript fallback');
	await expect(historical).toContainText('What do the weekly sales files show?');
	await expect(historical).toContainText('The initial scan found four relevant files and no single source of truth.');
	await expect(historical).toContainText('Not linked to a current Question');
	await expect(historical).toContainText('not counted toward coverage or Findings');
	await expect(historical).not.toContainText('Answers: What do the weekly sales files show?');

	await historical.getByRole('button', { name: 'Open transcript answer' }).click();
	await expect(page.getByRole('combobox', { name: 'Ask about northwind-sales' })).toBeVisible();
	await expect(page.locator('.msg.user').first().locator('.you-copy')).toHaveText('What do the weekly sales files show?');
	await expect(page.locator('.msg.assistant[data-analysis-turn-id="fixture-turn"]')).toContainText('The initial scan found four relevant files and no single source of truth.');
});

test('Finding Results connect partial coverage, scoped checks, provenance, and exact evidence', async ({ page }) => {
	const evidence = {
		id: 'finding-weekly-sales-evidence',
		tool: 'run_sql',
		args: { sql: 'SELECT week, SUM(net_sales) FROM sales GROUP BY week' },
		sql: 'SELECT week, SUM(net_sales) FROM sales GROUP BY week',
		result_summary: 'Ten weekly totals are present in sales.csv.',
		sources: [{ source: 'sales.csv', table: 'weekly_sales' }],
		columns: ['week', 'net_sales'],
		rows: [['W38', 40]],
		row_count: 10,
		ms: 8
	};
	await installDesktopMock(page, {
		questionRunOutcomes: [{
			status: 'incomplete',
			verification_status: 'insufficient_data',
			reason: 'Only 10 of the requested 12 weeks are present.',
			answer_text: 'The ten available weeks total $40.',
			plan: { strategy: 'compiled_sql', steps: ['inspect_schema', 'run_sql'] },
			checks: [
				{ label: 'The complete twelve-week window is present', ok: false, outcome: 'flagged', detail: 'Only ten weeks were present.' },
				{ label: 'The returned total matches the selected rows', ok: true, outcome: 'passed', detail: 'Ten returned rows sum to $40.' }
			],
			evidence: [evidence],
			source_scope: {
				used_sources: [{ source: 'sales.csv', table: 'weekly_sales', evidence_ids: [evidence.id] }],
				skipped_sources: []
			}
		}]
	});
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill('Explain weekly sales for finance.');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await page.getByRole('button', { name: 'Analyze main Question' }).click();
	await page.getByRole('region', { name: 'Review this run' }).getByRole('button', { name: 'Run this Question' }).click();
	const timeline = await expectQuestionRunSettled(page);
	await expect(timeline).toContainText('Insufficient data');
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await expandInvestigationHistory(page);
	await page.getByRole('button', { name: /Open saved analysis run 1: Explain weekly sales for finance\./ }).click();
	const capture = page.getByRole('region', { name: 'Saved analysis run' }).locator('.finding-capture');
	await capture.getByRole('textbox', { name: 'Finding claim' }).fill('The ten available weeks total $40.');
	await capture.getByRole('textbox', { name: 'Finding caveats' }).fill('Only 10 of 12 weeks are available.');
	await capture.getByRole('textbox', { name: 'Finding assumptions' }).fill('The available weeks use the same Monday–Sunday definition.');
	await capture.getByRole('button', { name: 'Save Finding' }).click();
	await page.getByRole('button', { name: 'Results' }).click();

	const results = page.getByRole('region', { name: 'Current Investigation results' });
	await expect(results).toContainText('0/1 Questions answered');
	await expect(results.locator('.results-question')).toContainText('Partial answer');
	await expect(results.locator('.results-question')).toContainText('Only 10 of the requested 12 weeks are present.');
	const finding = results.getByRole('article', { name: 'Finding 1' });
	await expect(finding).toContainText('Answers: Explain weekly sales for finance.');
	await finding.locator('.results-supporting > summary').click();
	await expect(finding).toContainText('Only 10 of 12 weeks are available.');
	await expect(finding).toContainText('The available weeks use the same Monday–Sunday definition.');
	await expect(finding.locator('[aria-label="Finding coverage and run status"]')).toContainText('Partial answer');
	await expect(finding.locator('[aria-label="Finding coverage and run status"]')).toContainText('Only 10 of the requested 12 weeks are present.');
	await expect(finding.locator('[aria-label="Finding coverage and run status"]')).toContainText('Insufficient data');
	const checks = finding.locator('[aria-label="Verification checks"]');
	await expect(checks).toContainText('Flagged · The complete twelve-week window is present');
	await expect(checks).toContainText('Passed · The returned total matches the selected rows');
	await expect(checks).toContainText('passing checks do not establish overall business correctness');
	const inspectEvidence = finding.getByRole('button', { name: 'Inspect evidence: Ten weekly totals are present in sales.csv.' });
	await expect(inspectEvidence).toBeVisible();
	await expect(finding).toContainText('finding-weekly-sales-evidence');
	await expect(finding).toContainText('sales.csv · weekly_sales');
	await inspectEvidence.click();
	await expect(page.getByRole('region', { name: 'Saved analysis run' })).toBeVisible();
	const evidenceTarget = page.getByRole('region', { name: 'Query and output evidence' }).locator('[data-evidence-id="finding-weekly-sales-evidence"]');
	await expect(evidenceTarget).toBeFocused();
});

test('Investigation report exports saved answers, Findings, QA, and source provenance without chat transcript', async ({ page }) => {
	const evidence = {
		id: 'evidence-weekly-sales-export',
		tool: 'run_sql',
		args: {},
		result_summary: 'Ten weekly totals from sales.csv.',
		sql: 'SELECT week, SUM(net_sales) FROM sales GROUP BY week',
		sources: [{ source: 'sales.csv', table: 'sales' }]
	};
	const chartEvidence = {
		id: 'evidence-weekly-sales-chart',
		tool: 'make_chart',
		args: {},
		result_summary: 'Weekly net sales visualization.',
		ms: 8,
		chart: {
			kind: 'line',
			title: 'Weekly net sales',
			labels: ['W1', 'W2'],
			series: [{ name: 'Net sales', values: [31, 40] }],
			unit: '$',
			x_label: 'Week',
			y_label: 'Net sales',
			metadata: {
				source_label: 'sales.csv',
				source_evidence_id: evidence.id,
				source_run_id: 'fixture-question-turn-1',
				finding_snapshot: 'Weekly sales rose to $40.',
				fields: ['week', 'net_sales'],
				aggregation: 'sum',
				filters: [],
				part_to_whole: false
			}
		}
	};
	await installDesktopMock(page, {
		questionRunOutcomes: [{
			status: 'incomplete',
			verification_status: 'needs_review',
			reason: 'Only 10 of the requested 12 weeks are present.',
			answer_text: 'Weekly sales increased 12% to $40.',
			plan: { strategy: 'compiled_sql', steps: ['inspect_schema', 'run_sql'] },
			checks: [{
				label: 'The full twelve-week window is present',
				ok: false,
				outcome: 'flagged',
				detail: 'Only ten weeks were present.'
			}],
			evidence: [evidence, chartEvidence],
			source_scope: {
				used_sources: [{ source: 'sales.csv', table: 'sales', evidence_ids: [evidence.id] }],
				skipped_sources: ['legacy.xls']
			}
		}]
	});
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill('Explain weekly sales for finance');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await page.getByRole('button', { name: 'Edit brief' }).click();
	await page.getByRole('textbox', { name: 'Audience' }).fill('Finance lead');
	await page.getByRole('textbox', { name: 'Desired output' }).fill('A concise weekly report');
	await page.getByRole('button', { name: 'Save brief' }).click();
	await expect(page.locator('.brief-summary')).toContainText('Finance lead');
	await expect(page.locator('.brief-summary')).toContainText('A concise weekly report');
	await page.getByRole('button', { name: 'Add definition or assumption' }).click();
	await page.getByRole('textbox', { name: 'Context note 1' }).fill('Net sales exclude refunds.');
	await page.getByRole('button', { name: 'Save context' }).click();
	await page.getByRole('button', { name: 'Add subquestion' }).click();
	await page.getByRole('textbox', { name: 'Subquestion 1' }).fill('Compare sales by region.');
	await page.getByRole('button', { name: 'Save plan' }).click();
	await page.getByRole('button', { name: 'Analyze main Question' }).click();
	await page.getByRole('region', { name: 'Review this run' }).getByRole('button', { name: 'Run this Question' }).click();
	await expectQuestionRunSettled(page);
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.').last()).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await expandInvestigationHistory(page);
	await page.getByRole('button', { name: 'Open saved analysis run 1: Explain weekly sales for finance' }).click();
	const findingCapture = page.getByRole('region', { name: 'Saved analysis run' }).locator('.finding-capture');
	await findingCapture.getByRole('textbox', { name: 'Finding claim' }).fill('Weekly sales rose 12% to $40.');
	await findingCapture.getByRole('textbox', { name: 'Finding caveats' }).fill('Only 10 of 12 weeks are available.');
	await findingCapture.getByRole('textbox', { name: 'Finding assumptions' }).fill('Test orders are excluded.');
	await findingCapture.getByRole('button', { name: 'Save Finding' }).click();
	await expect(page.locator('.finding-card')).toContainText('Weekly sales rose 12% to $40.');

	let downloadObservedBeforeReview = false;
	page.on('download', () => { downloadObservedBeforeReview = true; });
	await page.getByRole('button', { name: 'Review handoff' }).click();
	const handoffDialog = page.getByRole('dialog', { name: 'Review before sharing' });
	await expect(handoffDialog).toBeVisible();
	await expect(handoffDialog).toContainText('2 unresolved Questions');
	await expect(handoffDialog).not.toContainText('Fully answered');
	await expect(handoffDialog.getByRole('region', { name: 'Markdown handoff preview' })).toContainText('Weekly net sales');
	await expect(handoffDialog.getByRole('region', { name: 'Markdown handoff preview' })).toContainText('No reason was recorded for this unresolved status.');
	await expect(handoffDialog.getByRole('region', { name: 'Markdown handoff preview' })).toContainText('Weekly sales rose to $40.');
	await expect(handoffDialog.getByRole('region', { name: 'Visualization previews included in the handoff' })).toContainText('Model-produced visualization');
	await expect(handoffDialog.getByRole('region', { name: 'Visualization previews included in the handoff' })).toContainText('Weekly net sales');
	await expect(handoffDialog.getByRole('button', { name: 'Open evidence · evidence-weekly-sales-export' })).toBeVisible();
	await expect(handoffDialog.getByRole('button', { name: 'Open chart evidence' })).toBeVisible();
	expect(downloadObservedBeforeReview).toBe(false);
	await handoffDialog.getByRole('button', { name: 'Open evidence · evidence-weekly-sales-export' }).click();
	await expect(page.getByRole('region', { name: 'Saved analysis run' })).toBeVisible();
	const exactEvidenceTarget = page.getByRole('region', { name: 'Query and output evidence' }).locator('[data-evidence-id="evidence-weekly-sales-export"]');
	await expect(exactEvidenceTarget).toBeFocused();
	await page.getByRole('button', { name: 'Review handoff' }).click();
	const reviewedDialog = page.getByRole('dialog', { name: 'Review before sharing' });
	await expect(reviewedDialog).toBeVisible();
	const downloadPromise = page.waitForEvent('download');
	await reviewedDialog.getByRole('button', { name: 'Download Markdown' }).click();
	const download = await downloadPromise;
	expect(download.suggestedFilename()).toBe('explain-weekly-sales-for-finance-report.md');
	const report = await readFile(await download.path(), 'utf8');
	for (const expected of [
		'# Explain weekly sales for finance',
		'Finance lead',
		'A concise weekly report',
		'Weekly sales increased 12% to $40.',
		'Compare sales by region.',
		'Not started',
		'Weekly sales rose 12% to $40.',
		'Only 10 of 12 weeks are available.',
		'compiled sql',
		'needs review',
		'Only ten weeks were present.',
		'sales.csv (sales)',
		'legacy.xls',
		'Net sales exclude refunds.',
		'Weekly net sales',
		'Weekly sales rose to $40.',
		'evidence-weekly-sales-chart',
		'| W2 | 40 |',
		'evidence-weekly-sales-export',
		'conversation transcript is excluded'
	]) {
		expect(report.toLowerCase()).toContain(expected.toLowerCase());
	}
	expect(report).not.toContain('Scoped to C:\\FellaFixture\\northwind-sales.');
	await expect(reviewedDialog.getByRole('status').filter({ hasText: 'Downloaded explain-weekly-sales-for-finance-report.md.' })).toBeVisible();
});

test('Finding revisions preserve prior versions and compare the supporting run context', async ({ page }) => {
	const firstEvidence = {
		id: 'finding-net-sales-before-refunds',
		tool: 'run_sql',
		result_summary: 'The sales export reports $120 before refunds.',
		sql: 'SELECT SUM(net_sales) FROM sales',
		sources: [{ source: 'sales.csv', table: 'sales' }]
	};
	const secondEvidence = {
		id: 'finding-net-sales-after-refunds',
		tool: 'run_python',
		result_summary: 'The refreshed export reports $102 after refunds.',
		sources: [{ source: 'returns.csv', table: 'refunds' }]
	};
	await installDesktopMock(page, {
		questionRunOutcomes: [
			{
				status: 'answered',
				answer_text: 'Net sales were $120 before refunds.',
				plan: { strategy: 'compiled_sql', steps: ['inspect_sales', 'run_sql'] },
				evidence: [firstEvidence],
				source_scope: {
					used_sources: [{ source: 'sales.csv', table: 'sales', evidence_ids: [firstEvidence.id] }],
					skipped_sources: []
				}
			},
			{
				status: 'answered',
				answer_text: 'Net sales were $102 after refunds.',
				plan: { strategy: 'python_fallback', steps: ['inspect_returns', 'run_python'] },
				evidence: [secondEvidence],
				source_scope: {
					used_sources: [{ source: 'returns.csv', table: 'refunds', evidence_ids: [secondEvidence.id] }],
					skipped_sources: []
				}
			}
		]
	});
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill('Explain weekly net sales.');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await page.getByRole('button', { name: 'Add definition or assumption' }).click();
	const contextNote = page.getByRole('textbox', { name: 'Context note 1' });
	await contextNote.fill('Net sales exclude refunds.');
	await page.getByRole('button', { name: 'Save context' }).click();
	await expect(page.locator('.definition-editor')).toContainText('Context saved');

	const runQuestion = async () => {
		await page.getByRole('button', { name: 'Analyze main Question' }).click();
		await page.getByRole('region', { name: 'Review this run' }).getByRole('button', { name: 'Run this Question' }).click();
		await expectQuestionRunSettled(page);
		await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	};
	await runQuestion();
	await expandInvestigationHistory(page);
	await page.locator('.analysis-runs .analysis-run-link').nth(0).click();
	const firstCapture = page.getByRole('region', { name: 'Saved analysis run' }).locator('.finding-capture');
	await firstCapture.getByRole('textbox', { name: 'Finding caveats' }).fill('The export is before refund adjustments.');
	await firstCapture.getByRole('textbox', { name: 'Finding assumptions' }).fill('Each order appears once.');
	await firstCapture.getByRole('button', { name: 'Save Finding' }).click();
	const findingId = await page.evaluate(() => window.__workspaceFindingRecords()[0].id);
	await page.getByRole('button', { name: 'Close saved analysis run' }).click();

	await expandInvestigationDefinitions(page);
	await contextNote.fill('Net sales include refund adjustments.');
	await page.getByRole('button', { name: 'Save context' }).click();
	await expect(page.locator('.definition-item')).toContainText('Saved revision 2');
	await runQuestion();
	await expandInvestigationHistory(page);
	const attempts = page.locator('.analysis-runs .analysis-run-link');
	await expect(attempts).toHaveCount(2);
	await attempts.nth(1).click();
	const secondCapture = page.getByRole('region', { name: 'Saved analysis run' }).locator('.finding-capture');
	await secondCapture.getByRole('combobox', { name: 'Revise existing Finding' }).selectOption(findingId);
	await secondCapture.getByRole('textbox', { name: 'Finding claim' }).fill('Net sales were $102 after refund adjustments.');
	await secondCapture.getByRole('textbox', { name: 'Finding caveats' }).fill('The refreshed export includes refund adjustments.');
	await secondCapture.getByRole('textbox', { name: 'Finding assumptions' }).fill('Order IDs remain stable across exports.');
	await secondCapture.getByRole('textbox', { name: 'Finding revision note' }).fill('Revised after reconciling the returns export.');
	const refreshedEvidence = secondCapture.locator('.finding-evidence-select label')
		.filter({ hasText: 'run_python · The refreshed export reports $102 after refunds.' })
		.locator('input');
	await refreshedEvidence.check();
	await secondCapture.getByRole('button', { name: 'Save Finding revision' }).click();
	const savedFinding = await page.evaluate(() => window.__workspaceFindingRecords()[0]);
	expect(savedFinding).toMatchObject({
		id: findingId,
		revision: 2,
		claim: 'Net sales were $102 after refund adjustments.',
		analysis_turn_id: 'fixture-question-turn-2',
		evidence_ids: [secondEvidence.id],
		revision_note: 'Revised after reconciling the returns export.',
		revisions: [{
			revision: 1,
			claim: 'Net sales were $120 before refunds.',
			analysis_turn_id: 'fixture-question-turn-1',
			evidence_ids: [firstEvidence.id],
			caveats: ['The export is before refund adjustments.'],
			assumptions: ['Each order appears once.']
		}]
	});
	await page.getByRole('button', { name: 'Close saved analysis run' }).click();
	const findingCard = page.locator('.finding-card');
	await expect(findingCard).toContainText('Revision 2');
	await findingCard.getByRole('button', { name: 'Compare revision 1 with current' }).click();
	const comparison = page.getByRole('region', { name: 'Finding revision comparison' });
	await expect(comparison).toContainText('Claim changed.');
	await expect(comparison).toContainText('Supporting evidence changed.');
	await expect(comparison).toContainText('Analysis method changed.');
	await expect(comparison).toContainText('Source scope changed.');
	await expect(comparison).toContainText('Definitions or assumptions changed.');
	await expect(comparison).toContainText('Revised after reconciling the returns export.');
	const priorVersion = comparison.getByRole('article', { name: 'Previous Finding version' });
	await expect(priorVersion).toContainText('Net sales were $120 before refunds.');
	await expect(priorVersion).toContainText('fixture-question-turn-1');
	await expect(priorVersion).toContainText('compiled sql');
	await expect(priorVersion).toContainText('sales.csv');
	await expect(priorVersion).toContainText('Net sales exclude refunds.');
	await expect(priorVersion).toContainText('The export is before refund adjustments.');
	const currentVersion = comparison.getByRole('article', { name: 'Current Finding version' });
	await expect(currentVersion).toContainText('Net sales were $102 after refund adjustments.');
	await expect(currentVersion).toContainText('fixture-question-turn-2');
	await expect(currentVersion).toContainText('python fallback');
	await expect(currentVersion).toContainText('returns.csv');
	await expect(currentVersion).toContainText('Net sales include refund adjustments.');
	await expect(currentVersion).toContainText('The refreshed export includes refund adjustments.');

	await page.getByRole('button', { name: 'Results' }).click();
	const resultsFinding = page.getByRole('region', { name: 'Current Investigation results' }).getByRole('article', { name: 'Finding 1' });
	await resultsFinding.locator('.results-supporting > summary').click();
	const history = resultsFinding.locator('.results-finding-history');
	await history.locator(':scope > summary').click();
	await expect(history).toContainText('Net sales were $120 before refunds.');
	await expect(history).toContainText('The export is before refund adjustments.');
	await expect(history).toContainText('Revised after reconciling the returns export.');
	const inspectPriorEvidence = history.getByRole('button', { name: 'Inspect prior evidence finding-net-sales-before-refunds' });
	await inspectPriorEvidence.click();
	await expect(page.getByRole('region', { name: 'Saved analysis run' })).toBeVisible();
	await expect(page.getByRole('region', { name: 'Query and output evidence' }).locator('[data-evidence-id="finding-net-sales-before-refunds"]')).toBeFocused();
});

test('suggested Question breakdown stays a draft until saved as durable child records', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	const composer = page.getByRole('combobox', { name: 'Ask about northwind-sales' });
	await composer.fill('Understand conversion by channel');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await expect(page.getByRole('heading', { name: 'Questions', exact: true })).toBeVisible();

	const askCountBeforeSuggestion = await page.evaluate(() => window.__workspaceAskCalls.length);
	const originalRequest = await page.evaluate(() => window.__workspaceAskCalls[0].question);
	await page.getByRole('button', { name: 'Suggest breakdown' }).click();
	await expect(page.getByRole('textbox', { name: 'Subquestion 1' })).toHaveValue('Compare conversion by channel');
	await expect(page.getByRole('textbox', { name: 'Subquestion 2' })).toHaveValue('Check the trend over time');
	await expect(page.locator('.question-save')).toContainText('Unsaved plan');
	const afterSuggestion = await page.evaluate(() => ({
		askCount: window.__workspaceAskCalls.length,
		proposalCalls: window.__workspaceQuestionProposalCalls,
		questions: window.__workspaceInvestigations()[0].questions
	}));
	expect(afterSuggestion.askCount).toBe(askCountBeforeSuggestion);
	expect(originalRequest).toContain('Understand conversion by channel');
	expect(afterSuggestion.proposalCalls).toHaveLength(1);
	expect(afterSuggestion.questions).toHaveLength(1);
	await expect(page.getByRole('button', { name: 'Open conversation', exact: true })).toBeVisible();
	await page.getByRole('button', { name: 'Discard plan' }).click();
	await expect(page.locator('.question-save')).toContainText('Plan saved');
	expect((await page.evaluate(() => window.__workspaceInvestigations()[0].questions)).map((item) => item.text)).toEqual([
		'Understand conversion by channel'
	]);
	await page.getByRole('button', { name: 'Suggest breakdown' }).click();
	await expect(page.getByRole('textbox', { name: 'Subquestion 1' })).toHaveValue('Compare conversion by channel');

	await page.getByRole('button', { name: 'Save plan' }).click();
	await expect(page.locator('.question-save')).toContainText('Plan saved');
	const saved = await page.evaluate(() => window.__workspaceInvestigations()[0]);
	expect(saved.questions.map((item) => item.text)).toEqual([
		'Understand conversion by channel',
		'Compare conversion by channel',
		'Check the trend over time'
	]);
	expect(saved.questions[0].parent_id).toBeFalsy();
	expect(saved.questions.slice(1).every((item) => item.parent_id === saved.questions[0].id)).toBe(true);
	const loadedChild = await page.evaluate((questionId) =>
		window.fella.invoke('investigation_question_load', { questionId }), saved.questions[2].id);
	expect(loadedChild).toMatchObject({
		id: saved.questions[2].id,
		investigation_id: saved.id,
		parent_id: saved.questions[0].id,
		text: 'Check the trend over time',
		status: 'active'
	});
	await expect(page.getByRole('heading', { name: 'Investigation', exact: true })).toBeVisible();
});

test('Investigation shows source readiness and opens source details in place', async ({ page }) => {
	const workspacePath = folders[0].path;
	const fixtureFolders = [{
		...folders[0],
		catalog: {
			...folders[0].catalog,
			sources: [
				{
						name: 'sales.csv',
						path: `${workspacePath}\\sales.csv`,
						kind: 'csv',
						view: 'sales',
					row_count: 124, columns: [
							{
								name: 'order_id', type: 'BIGINT', distinct: 124, null_fraction: 0,
								min: '1', max: '124'
							},
							{
								name: 'ordered_at', type: 'TEXT', distinct: 124, null_fraction: 0.25,
								min: '2025-01-03', max: '2025-12-29'
							},
							{
								name: 'amount', type: 'REAL', distinct: 83, null_fraction: 0,
								min: '0', max: '95000'
							}
					],
					size_bytes: 4096, mtime: 1
				},
				{
					name: 'quarterly-notes.txt',
					path: `${workspacePath}\\quarterly-notes.txt`,
					kind: 'text',
					synopsis: 'Regional launches moved two weeks later.', size_bytes: 2048, mtime: 1
				},
				{
					name: 'quarterly.xlsx · East',
					path: `${workspacePath}\\quarterly.xlsx`,
					kind: 'xlsx', view: 'quarterly_east', row_count: 12, columns: [],
					size_bytes: 4096, mtime: 1
				},
				{
					name: 'quarterly.xlsx · West',
					path: `${workspacePath}\\quarterly.xlsx`,
					kind: 'xlsx', view: 'quarterly_west', row_count: 10, columns: [],
					size_bytes: 4096, mtime: 1
				}
			],
			skipped: [
				{ name: 'old-report.docx', reason: "Fella can't read this file type yet" },
				{ name: 'locked.csv', reason: "couldn't be read (permission, or open in another app)" }
			]
		}
	}];
	await installDesktopMock(page, { fixtureFolders });
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	const composer = page.getByRole('combobox', { name: 'Ask about northwind-sales' });
	await composer.fill('Compare sales and regional notes');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText(`Scoped to ${workspacePath}.`)).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await expect(page.getByRole('heading', { name: 'Data available' })).toBeVisible();
	await page.getByLabel('Choose files').check();
	await page.getByRole('checkbox', { name: 'Include quarterly-notes.txt in this Investigation' }).uncheck();
	await expect.poll(() => page.evaluate(() => window.__workspaceInvestigations()[0].source_selection)).toEqual({
		mode: 'selected_sources',
		included_paths: ['sales.csv', 'quarterly.xlsx']
	});
	await expect(page.getByRole('checkbox', { name: 'Include quarterly.xlsx in this Investigation' })).toHaveCount(1);
	await page.getByRole('checkbox', { name: 'Include quarterly.xlsx in this Investigation' }).uncheck();
	await expect.poll(() => page.evaluate(() => window.__workspaceInvestigations()[0].source_selection)).toEqual({
		mode: 'selected_sources',
		included_paths: ['sales.csv']
	});
	await page.getByLabel('All sources').check();
	await expect.poll(() => page.evaluate(() => window.__workspaceInvestigations()[0].source_selection)).toEqual({
		mode: 'all_sources',
		included_paths: []
	});
	await expect(page.getByRole('button', { name: /sales\.csv.*Queryable/ })).toBeVisible();
	const documentSource = page.getByRole('button', { name: /quarterly-notes\.txt.*Readable/ });
	await expect(documentSource).toBeVisible();
	await documentSource.click();
	await expect(page.getByRole('region', { name: 'Details for quarterly-notes.txt' })).toContainText('Regional launches moved two weeks later.');

	const skipped = page.locator('.skipped-sources');
	await skipped.locator('summary').click();
	await expect(skipped).toContainText('Unsupported');
	await expect(skipped).toContainText('Unavailable');

	const sourceFieldSearch = page.getByRole('textbox', { name: 'Find a source or field' });
	await sourceFieldSearch.fill('ordered_at');
	await expect(page.getByRole('button', { name: /sales\.csv.*Queryable/ })).toBeVisible();
	await expect(documentSource).not.toBeVisible();
	await expect.poll(() => page.evaluate(() => window.__workspaceInvestigations()[0].source_selection)).toEqual({
		mode: 'all_sources',
		included_paths: []
	});
	await page.getByRole('button', { name: /sales\.csv.*Queryable/ }).click();
	const details = page.getByRole('region', { name: 'Details for sales.csv' });
	await expect(details).toBeVisible();
	await expect(details).toContainText('124');
	await expect(details.getByRole('table')).toContainText('North');
	const fieldProfile = details.locator('.field-profile');
	await fieldProfile.locator('summary').click();
	await expect(fieldProfile.locator('tbody tr')).toHaveCount(1);
	await expect(fieldProfile.getByRole('table')).toContainText('ordered_at');
	await expect(fieldProfile).toContainText('31 missing · 25%');
	await expect(fieldProfile).toContainText('2025-01-03 – 2025-12-29');
	await expect(fieldProfile).toContainText('measured from the loaded rows only');
	await sourceFieldSearch.fill('');
	await expect(fieldProfile.locator('tbody tr')).toHaveCount(3);
	await expect(page.getByRole('heading', { name: 'Investigation', exact: true })).toBeVisible();
});

test('the four-pane limit is scoped to one environment and closing a pane frees its slot', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const workspaceNavigation = page.getByRole('region', { name: 'Workspaces' });
	await workspaceNavigation.getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('article')).toHaveCount(2); // General and northwind-sales
	for (let index = 1; index < 3; index += 1) {
		await dispatchWorkspaceDrag(page, 'drop', folders[index].path);
		await expect(page.getByRole('article')).toHaveCount(index + 2);
	}
	await expect(page.getByRole('article')).toHaveCount(4); // General plus three repositories
	const fourth = folders[3];
	const fifth = folders[4];

	await dispatchWorkspaceDrag(page, 'dragover', fourth.path);
	await expect(page.locator('.placement-preview .full-notice')).toContainText('Close a workspace');
	const beforeDrop = await page.getByRole('article').count();
	await dispatchWorkspaceDrag(page, 'drop', fourth.path);
	await expect(page.getByText(/environment already has four workspaces/i)).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(beforeDrop);

	const closeGeneral = page.getByRole('button', { name: 'Close General workspace' });
	await expect(closeGeneral).toHaveAttribute('data-slot', 'button');
	await closeGeneral.click();
	await expect(page.getByRole('article')).toHaveCount(3);
	await dispatchWorkspaceDrag(page, 'drop', fourth.path);
	await expect(page.getByRole('article', { name: 'travel-plans workspace' })).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(4);

	await dispatchWorkspaceDrag(page, 'drop', fifth.path);
	await expect(page.getByText(/environment already has four workspaces/i)).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(4);

	const closeSales = page.getByRole('button', { name: 'Close northwind-sales workspace' });
	await expect(closeSales).toHaveAttribute('data-slot', 'button');
	await closeSales.click();
	await expect(page.getByRole('article')).toHaveCount(3);
	await dispatchWorkspaceDrag(page, 'drop', fifth.path);
	await expect(page.getByRole('article', { name: 'reading-notes workspace' })).toBeVisible();
	await expect(page.getByRole('article')).toHaveCount(4);

	await page.reload();
	await expect(page.getByRole('article')).toHaveCount(4);
	await expect(page.getByRole('article', { name: 'health-journal workspace' })).toBeVisible();
	await expect(page.getByRole('article', { name: 'reading-notes workspace' })).toBeVisible();
});

test('Composer source search filters and attaches a workspace source', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const workspaceNavigation = page.getByRole('region', { name: 'Workspaces' });
	await workspaceNavigation.getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await expect(page.getByTestId('investigation-data-readiness').getByRole('button', { name: /sales\.csv/ })).toBeVisible();
	const salesWorkspaceRow = page.getByRole('button', { name: 'Open workspace northwind-sales' });
	await salesWorkspaceRow.click();
	await salesWorkspaceRow.hover();
	await page.getByRole('button', { name: 'New conversation in northwind-sales' }).click();
	await expect(page.getByRole('combobox', { name: 'Ask about northwind-sales' })).toBeVisible();

	const addSource = page.getByRole('button', { name: 'Add a source or field to this question' });
	await expect(addSource).toHaveAttribute('data-slot', 'button');
	await addSource.click();
	const closePicker = page.getByRole('button', { name: 'Close context picker' });
	await expect(closePicker).toHaveAttribute('data-slot', 'button');
	await closePicker.click();
	await expect(page.getByRole('textbox', { name: 'Find a source or field' })).toHaveCount(0);
	await addSource.click();
	const search = page.getByRole('textbox', { name: 'Find a source or field' });
	await expect(search).toBeFocused();
	await expect(search).toHaveClass(/fella-ui-input/);
	await search.fill('sales.csv');
	const source = page.locator('.context-main').filter({ hasText: 'sales.csv' });
	await expect(source).toBeVisible();
	await expect(source).toHaveAttribute('data-slot', 'button');
	await source.click();
	const reference = page.locator('.ref-pill').filter({ hasText: 'sales.csv' });
	await expect(reference).toBeVisible();
	const removeReference = reference.getByRole('button', { name: 'Remove sales.csv from this question' });
	await expect(removeReference).toHaveAttribute('data-slot', 'button');
	await removeReference.click();
	await expect(reference).toHaveCount(0);
});

test('Composer model search filters the available models', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const modelTrigger = page.locator('.model-trigger');
	await expect(modelTrigger).toBeVisible();
	await expect(modelTrigger).toHaveAttribute('data-slot', 'button');
	await modelTrigger.click();
	const search = page.getByRole('textbox', { name: 'Find a model' });
	await expect(search).toBeFocused();
	await expect(search).toHaveClass(/fella-ui-input/);
	await search.fill('no-such-model');
	await expect(page.getByText('No models available from openai.')).toBeVisible();
	await search.fill('gpt-4.1-mini');
	const modelOption = page.getByRole('option', { name: 'gpt-4.1-mini' });
	await expect(modelOption).toBeVisible();
	await expect(modelOption).toHaveAttribute('data-slot', 'button');
	await modelOption.click();
	await expect(page.getByRole('dialog', { name: 'Choose a model' })).toHaveCount(0);
	await modelTrigger.click();
	const providerSettings = page.getByRole('button', { name: 'Provider settings' });
	await expect(providerSettings).toHaveAttribute('data-slot', 'button');
	await providerSettings.click();
	await expect(page.getByRole('heading', { name: 'Settings' })).toBeVisible();
});

test('Composer command completion uses shared buttons and keeps keyboard selection', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	const workspace = page.getByRole('button', { name: 'Open workspace northwind-sales' });
	await workspace.click();
	await workspace.hover();
	await page.getByRole('button', { name: 'New conversation in northwind-sales' }).click();

	const composer = page.getByRole('combobox', { name: 'Ask about northwind-sales' });
	await composer.fill('/fi');
	const completion = page.getByRole('option', { name: /\/files/ });
	await expect(completion).toHaveAttribute('data-slot', 'button');
	await expect(completion).toHaveAttribute('aria-selected', 'false');
	await page.keyboard.press('ArrowDown');
	await expect(completion).toHaveAttribute('aria-selected', 'true');
	await page.keyboard.press('Enter');
	await expect(composer).toHaveValue('/files ');
	await expect(page.getByRole('listbox', { name: 'completions' })).toHaveCount(0);
});

test('environment dropdown saves different workspace arrangements without leaking panes between them', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const navigation = page.getByRole('region', { name: 'Workspaces' });
	await navigation.getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('article', { name: 'northwind-sales workspace' })).toBeVisible();

	const switcher = page.locator('.sidebar .environment-switcher-trigger');
	await switcher.click();
	await page.getByRole('menuitem', { name: /New environment/ }).click();
	await expect(switcher).toContainText('General');
	await navigation.getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('article', { name: 'health-journal workspace' })).toBeVisible();

	await switcher.click();
	await page.getByRole('menuitemradio').filter({ hasText: 'northwind-sales' }).click();
	await expect(page.getByRole('article', { name: 'northwind-sales workspace' })).toBeVisible();
	await expect(page.getByRole('article', { name: 'health-journal workspace' })).toHaveCount(0);
	await expect(page.getByRole('article', { name: 'General workspace' })).toBeVisible();

	await switcher.click();
	await page.getByRole('menuitemradio').filter({ hasText: 'health-journal' }).click();
	await expect(page.getByRole('article', { name: 'health-journal workspace' })).toBeVisible();
	await expect(page.getByRole('article', { name: 'northwind-sales workspace' })).toHaveCount(0);
	await expect(page.getByRole('article', { name: 'General workspace' })).toBeVisible();

	await page.reload();
	const restoredSwitcher = page.locator('.sidebar .environment-switcher-trigger');
	await restoredSwitcher.click();
	await expect(page.getByRole('menuitemradio').filter({ hasText: 'health-journal' })).toHaveAttribute('aria-checked', 'true');
	await page.keyboard.press('Escape');
	await expect(page.getByRole('article', { name: 'health-journal workspace' })).toBeVisible();
	await expect(page.getByRole('article', { name: 'northwind-sales workspace' })).toHaveCount(0);
});

test('environment dropdown creates and closes the current environment', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const switcher = page.locator('.sidebar .environment-switcher-trigger');
	await expect(switcher).toHaveAttribute('aria-haspopup', 'menu');
	await switcher.click();
	await page.getByRole('menuitem', { name: /New environment/ }).click();
	await expect(switcher).toContainText('General 2');

	await switcher.click();
	await expect(page.getByRole('menuitemradio')).toHaveCount(2);
	await page.getByRole('menuitem', { name: /Close current environment/ }).click();
	await expect(switcher).toContainText('General');
	await switcher.click();
	await expect(page.getByRole('menuitemradio')).toHaveCount(1);
	await expect(page.getByRole('menuitemradio').first()).toHaveAttribute('aria-checked', 'true');
});

test('environment dropdown supports keyboard selection and focus restoration', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const switcher = page.locator('.sidebar .environment-switcher-trigger');
	await switcher.click();
	await page.getByRole('menuitem', { name: /New environment/ }).click();

	await switcher.focus();
	await page.keyboard.press('Enter');
	const menu = page.getByRole('menu');
	await expect(menu).toBeVisible();
	await page.keyboard.press('Escape');
	await expect(menu).toHaveCount(0);
	await expect(switcher).toBeFocused();

	await page.keyboard.press('Enter');
	await page.keyboard.press('Home');
	const firstEnvironment = page.getByRole('menuitemradio').first();
	await expect(firstEnvironment).toBeFocused();
	await page.keyboard.press('Enter');
	await expect(switcher).toContainText('General');
	await expect(switcher).toBeFocused();
});

test('environment dropdown remains available when the sidebar is collapsed', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const sidebarSwitcher = page.locator('.sidebar .environment-switcher-trigger');
	await sidebarSwitcher.click();
	await page.getByRole('menuitem', { name: /New environment/ }).click();
	await page.getByRole('button', { name: 'Collapse sidebar' }).click();

	const titlebarSwitcher = page.locator('.titlebar .environment-switcher-trigger');
	await expect(titlebarSwitcher).toBeVisible();
	await expect(titlebarSwitcher).toContainText('General 2');
	await titlebarSwitcher.click();
	await page.getByRole('menuitemradio').first().click();
	await expect(titlebarSwitcher.locator('.environment-switcher-label')).toHaveText('General');
	await expect(page.locator('.sidebar')).toHaveCount(0);
});

test('Composer mode selection uses the shared radio menu interaction', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const modeTrigger = page.getByRole('button', { name: 'Ask', exact: true });
	await modeTrigger.click();
	const menu = page.getByRole('menu');
	const askOption = page.getByRole('menuitemradio', { name: /^Ask/ });
	const inspectOption = page.getByRole('menuitemradio', { name: /^Check data/ });
	await expect(menu).toBeVisible();
	await expect(askOption).toHaveAttribute('aria-checked', 'true');
	await page.keyboard.press('Escape');
	await expect(menu).toHaveCount(0);
	await expect(modeTrigger).toBeFocused();

	await page.keyboard.press('Enter');
	await expect(askOption).toBeFocused();
	await page.keyboard.press('ArrowDown');
	await expect(inspectOption).toBeFocused();
	await page.keyboard.press('Enter');
	const selectedModeTrigger = page.getByRole('button', { name: 'Check data', exact: true });
	await expect(selectedModeTrigger).toContainText('Check data');
	await expect(selectedModeTrigger).toHaveAttribute('aria-expanded', 'false');
});

test('many environments remain in a bounded, scrollable dropdown', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const switcher = page.locator('.sidebar .environment-switcher-trigger');
	const create = page.getByRole('menuitem', { name: /New environment/ });
	for (let index = 0; index < 11; index += 1) {
		await switcher.click();
		await create.click();
	}
	await switcher.click();
	const environments = page.getByRole('menuitemradio');
	await expect(environments).toHaveCount(12);
	await expect(environments.last()).toHaveAttribute('aria-checked', 'true');

	const metrics = await page.evaluate(() => {
		const menu = document.querySelector('.environment-menu-content');
		const selected = menu?.querySelector('[aria-checked="true"]');
		if (!(menu instanceof HTMLElement) || !(selected instanceof HTMLElement)) return null;
		const menuRect = menu.getBoundingClientRect();
		const selectedRect = selected.getBoundingClientRect();
		return {
			menuHeight: menu.clientHeight,
			menuScrollHeight: menu.scrollHeight,
			selectedVisible: selectedRect.top >= menuRect.top - 1 && selectedRect.bottom <= menuRect.bottom + 1,
			pageWidth: document.documentElement.scrollWidth,
			viewportWidth: window.innerWidth
		};
	});
	expect(metrics).not.toBeNull();
	expect(metrics.menuScrollHeight).toBeGreaterThan(metrics.menuHeight);
	expect(metrics.selectedVisible).toBe(true);
	expect(metrics.pageWidth).toBeLessThanOrEqual(metrics.viewportWidth);
});

test('a history-only workspace retries its folder mount and keeps history available when it is still missing', async ({ page }) => {
	await installDesktopMock(page, { seedUnavailableConversation: true });
	await page.goto('/');
	await page.getByRole('button', { name: 'Expand unavailable-archive' }).click();
	await page.getByRole('button', { name: 'Open conversation: Summarize the unavailable archive' }).click();
	const workspace = page.getByRole('article', { name: 'unavailable-archive workspace' });
	await expect(workspace).toBeVisible();
	await expect(page.getByRole('status', {
		name: 'History only. The folder could not be opened; saved conversations remain available.'
	})).toBeVisible();
	const workspaceContext = page.locator('.workspace-context-bar');
	await expect(workspaceContext).toContainText('History only');
	const contextReconnect = workspaceContext.getByRole('button', { name: 'Reconnect folder' });
	await expect(contextReconnect).toBeVisible();
	const initialOpenCalls = await page.evaluate(() => window.__workspaceOpenCalls.length);
	expect(initialOpenCalls).toBe(1);

	await contextReconnect.click();
	await expect.poll(() => page.evaluate(() => window.__workspaceOpenCalls.length)).toBe(2);
	await expect(page.getByText(/Its conversations remain in history only/).last()).toBeVisible();

	await page.getByRole('button', { name: 'Open workspace unavailable-archive' }).hover();
	await page.getByRole('button', { name: 'Reconnect unavailable-archive' }).click();
	await expect.poll(() => page.evaluate(() => window.__workspaceOpenCalls.length)).toBe(3);
	await expect(page.getByText(/Its conversations remain in history only/).last()).toBeVisible();
	await expect(workspace).toContainText('Saved result from this folder.');
	const reconnect = page.getByRole('button', { name: 'Reconnect unavailable-archive' });
	await expect(reconnect).toHaveAttribute('data-slot', 'button');
	await expect(reconnect).toHaveCSS('width', '22px');
	await expect(reconnect).toHaveCSS('height', '22px');
	const unavailableComposer = page.getByRole('combobox', { name: 'Workspace unavailable: unavailable-archive' });
	await expect(unavailableComposer).toBeDisabled();
	await expect(page.getByRole('button', { name: 'Open workspace', exact: true })).toHaveAttribute('data-slot', 'button');
});

test('deleting a history-only workspace confirms and removes all of its saved conversations and environment panes', async ({ page }) => {
	await installDesktopMock(page, { seedUnavailableConversation: true, seedDeletionConversations: true });
	await page.goto('/');

	await page.getByRole('button', { name: 'Expand unavailable-archive' }).click();
	await page.getByRole('button', { name: 'Open conversation: Summarize the unavailable archive' }).click();
	await expect(page.getByRole('article', { name: 'unavailable-archive workspace' })).toBeVisible();

	await page.locator('.sidebar .environment-switcher-trigger').click();
	await page.getByRole('menuitem', { name: 'New environment' }).click();
	const unavailableDisclosure = page.getByRole('button', { name: /^(Expand|Collapse) unavailable-archive$/ });
	if (await unavailableDisclosure.getAttribute('aria-expanded') === 'false') await unavailableDisclosure.click();
	await page.getByRole('button', { name: 'Open conversation: Summarize the unavailable archive' }).click();
	await expect(page.getByRole('article', { name: 'unavailable-archive workspace' })).toBeVisible();

	const repository = page.locator('.repository').filter({ hasText: 'unavailable-archive' });
	await repository.hover();
	await repository.getByRole('button', { name: 'Repository actions' }).click();
	await page.getByRole('menuitem', { name: 'Delete workspace and conversations' }).click();
	const dialog = page.getByRole('dialog', { name: 'Delete unavailable-archive?' });
	await expect(dialog).toBeVisible();
	await expect(dialog).toContainText('permanently removes');
		await expect(dialog).toContainText('folder, files, and any project notes are kept');
	await dialog.getByRole('button', { name: 'Cancel' }).click();
	await expect(dialog).toHaveCount(0);
	await expect(page.getByRole('article', { name: 'unavailable-archive workspace' })).toBeVisible();
	expect(await page.evaluate(() => window.__deletedConversationIds)).toEqual([]);

	await repository.hover();
	await repository.getByRole('button', { name: 'Repository actions' }).click();
	await page.getByRole('menuitem', { name: 'Delete workspace and conversations' }).click();
	await dialog.getByRole('button', { name: 'Delete workspace and conversations' }).click();
	await expect(dialog).toHaveCount(0);
	await expect(page.getByRole('article', { name: 'unavailable-archive workspace' })).toHaveCount(0);
	await expect(page.getByRole('button', { name: 'Expand unavailable-archive' })).toHaveCount(0);
	const deletedHistory = await page.evaluate(() => window.__deletedConversationWorkspaces);
	expect(deletedHistory.map(({ id }) => id)).toEqual(expect.arrayContaining([
		'archived-unavailable-workspace',
		'archived-unavailable-workspace-2'
	]));
	expect(deletedHistory.every(({ workspace }) => workspace === 'C:\\FellaFixture\\unavailable-archive')).toBe(true);
	expect(await page.evaluate(() => [...window.__remainingArchiveIds()].sort())).toEqual(['archived-northwind-sales']);
	const persistedEnvironments = await page.evaluate(() => JSON.parse(localStorage.getItem('fella:environments')));
	expect(persistedEnvironments.environments).toHaveLength(2);
	expect(persistedEnvironments.environments.flatMap((environment) => environment.panes).some((pane) => pane.workspaceId === 'C:\\FellaFixture\\unavailable-archive')).toBe(false);
});

test('a mounted workspace can be deleted with its saved conversations while its files are kept', async ({ page }) => {
	await installDesktopMock(page, { seedUnavailableConversation: true, seedDeletionConversations: true });
	await page.goto('/');
	await page.getByRole('button', { name: 'Open workspace northwind-sales' }).click();
	await expect(page.getByRole('article', { name: 'northwind-sales workspace' })).toBeVisible();

	const repository = page.locator('.repository').filter({ hasText: 'northwind-sales' });
	await repository.hover();
	await repository.getByRole('button', { name: 'Repository actions' }).click();
	await page.getByRole('menuitem', { name: 'Delete workspace and conversations' }).click();
	const dialog = page.getByRole('dialog', { name: 'Delete northwind-sales?' });
	await expect(dialog).toContainText('folder, files, and any project notes are kept');
	await dialog.getByRole('button', { name: 'Delete workspace and conversations' }).click();

	await expect(dialog).toHaveCount(0);
	await expect(page.getByRole('article', { name: 'northwind-sales workspace' })).toHaveCount(0);
	await expect(page.getByRole('button', { name: 'Open workspace northwind-sales' })).toHaveCount(0);
	const deletedHistory = await page.evaluate(() => window.__deletedConversationWorkspaces);
	expect(deletedHistory.map(({ id }) => id)).toContain('archived-northwind-sales');
	expect(deletedHistory.every(({ workspace }) => workspace === 'C:\\FellaFixture\\northwind-sales')).toBe(true);
	expect(await page.evaluate(() => [...window.__remainingArchiveIds()].sort())).toEqual([
		'archived-unavailable-workspace',
		'archived-unavailable-workspace-2'
	].sort());
});

test('shared workspace tabs and modal/menu primitives preserve keyboard interaction', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');

	const search = page.getByRole('button', { name: 'Search' });
	await search.focus();
	await page.keyboard.press('Control+k');
	const palette = page.getByRole('dialog', { name: 'Search Fella' });
	const searchInput = page.getByRole('textbox', { name: 'Search Fella' });
	await expect(palette).toBeVisible();
	await expect(searchInput).toBeFocused();
	await page.keyboard.press('Escape');
	await expect(palette).toHaveCount(0);
	await expect(search).toBeFocused();

	const navigation = page.getByRole('region', { name: 'Workspaces' });
	await navigation.getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('article', { name: 'northwind-sales workspace' })).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();

	const workspaceTabs = page.getByRole('tablist', { name: 'Workspace sections' });
	const investigationTab = workspaceTabs.getByRole('tab', { name: 'Investigation' });
	const guideTab = workspaceTabs.getByRole('tab', { name: 'Guide' });
	await expect(investigationTab).toHaveAttribute('aria-selected', 'true');
	await expect(workspaceTabs.getByRole('tab', { name: 'Sources' })).toHaveCount(0);
	await investigationTab.focus();
	await page.keyboard.press('ArrowRight');
	await expect(guideTab).toHaveAttribute('aria-selected', 'true');
	await expect(page.getByRole('heading', { name: 'Guide', level: 1 })).toBeVisible();
	await expect(page.getByRole('textbox', { name: 'Workspace guide' })).toBeVisible();

	const createProject = page.getByRole('button', { name: 'Create project for northwind-sales' });
	await createProject.click();
	const projectDialog = page.getByRole('dialog', { name: 'Create a project' });
	const projectName = page.locator('#project-name');
	await expect(projectDialog).toBeVisible();
	await expect(projectName).toBeFocused();
	await page.keyboard.press('Escape');
	await expect(projectDialog).toHaveCount(0);
	await expect(createProject).toBeFocused();

	await navigation.getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('article', { name: 'health-journal workspace' })).toBeVisible();
	const priorRepository = page.locator('.repository').filter({ hasText: 'northwind-sales' });
	await priorRepository.hover();
	const repositoryActions = priorRepository.getByRole('button', { name: 'Repository actions' });
	await repositoryActions.click();
	await expect(repositoryActions).toHaveAttribute('aria-expanded', 'true');
	const menuItem = page.getByRole('menuitem', { name: 'Hide repository' });
	await expect(menuItem).toBeVisible();
	await page.keyboard.press('Escape');
	await expect(menuItem).toHaveCount(0);
	await expect(priorRepository.getByRole('button', { name: 'Repository actions' })).toBeFocused();
});

test('Sidebar navigation actions use shared buttons and retain compact keyboard access', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const newConversation = page.getByRole('button', { name: 'New conversation' });
	const search = page.getByRole('button', { name: 'Search' });
	const settings = page.getByRole('button', { name: 'Settings' });

	for (const action of [newConversation, search, settings]) {
		await expect(action).toHaveAttribute('data-slot', 'button');
		expect(await action.evaluate((element) => getComputedStyle(element).justifyContent)).toBe('flex-start');
		const bounds = await action.boundingBox();
		expect(bounds).not.toBeNull();
		expect(bounds.height).toBeLessThanOrEqual(32);
	}

	await search.focus();
	await page.keyboard.press('Control+k');
	const palette = page.getByRole('dialog', { name: 'Search Fella' });
	await expect(palette).toBeVisible();
	await page.keyboard.press('Escape');
	await expect(palette).toHaveCount(0);
	await expect(search).toBeFocused();

	await settings.click();
	await expect(settings).toHaveAttribute('aria-current', 'page');
});

test('Sidebar icon actions use shared compact buttons and Add repository still opens the board', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const collapse = page.getByRole('button', { name: 'Collapse sidebar' });
	const addRepository = page.getByRole('button', { name: 'Add repository' });

	for (const action of [collapse, addRepository]) {
		await expect(action).toHaveAttribute('data-slot', 'button');
		const bounds = await action.boundingBox();
		expect(bounds).not.toBeNull();
		expect(bounds.width).toBeLessThanOrEqual(32);
		expect(bounds.height).toBeLessThanOrEqual(32);
	}

	await addRepository.click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await expect(page.getByRole('article', { name: 'northwind-sales workspace' })).toBeVisible();
});

test('Titlebar Fella actions use shared buttons and remain clickable', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const paletteTrigger = page.locator('.titlebar .hint');
	await expect(paletteTrigger).toHaveAttribute('data-slot', 'button');
	await paletteTrigger.click();
	const palette = page.getByRole('dialog', { name: 'Search Fella' });
	await expect(palette).toBeVisible();
	await page.keyboard.press('Escape');
	await expect(palette).toHaveCount(0);
	await expect(paletteTrigger).toBeFocused();

	await page.getByRole('button', { name: 'Collapse sidebar' }).click();
	const expandSidebar = page.locator('.titlebar .navbtn');
	await expect(expandSidebar).toHaveAttribute('data-slot', 'button');
	await expandSidebar.click();
	await expect(page.getByRole('button', { name: 'Collapse sidebar' })).toBeVisible();
});

test('Sidebar workspace tools use shared buttons and preserve Investigation and project actions', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('article', { name: 'northwind-sales workspace' })).toBeVisible();
	const workspaceRow = page.getByRole('button', { name: 'Open workspace northwind-sales' });
	await expect(workspaceRow).toHaveAttribute('data-slot', 'button');
	const collapseWorkspace = page.getByRole('button', { name: 'Collapse northwind-sales' });
	await expect(collapseWorkspace).toHaveAttribute('data-slot', 'button');
	await collapseWorkspace.click();
	const expandWorkspace = page.getByRole('button', { name: 'Expand northwind-sales' });
	await expect(expandWorkspace).toHaveAttribute('data-slot', 'button');
	await expect(page.getByRole('button', { name: 'Open investigation brief in northwind-sales' })).toHaveCount(0);
	await expandWorkspace.click();
	await expect(page.getByRole('button', { name: 'Open investigation brief in northwind-sales' })).toBeVisible();

	const investigation = page.getByRole('button', { name: 'Open investigation brief in northwind-sales' });
	const createProject = page.getByRole('button', { name: 'Create project for northwind-sales' });
	for (const action of [investigation, createProject]) {
		await expect(action).toHaveAttribute('data-slot', 'button');
		const bounds = await action.boundingBox();
		expect(bounds).not.toBeNull();
		expect(bounds.height).toBeLessThanOrEqual(32);
	}

	await investigation.click();
	await expect(page.getByRole('heading', { name: 'Data available' })).toBeVisible();
	await expect(page.getByRole('heading', { name: 'Investigation', exact: true })).toBeVisible();
	await createProject.click();
	await expect(page.getByRole('heading', { name: 'Create a project' })).toBeVisible();
	await page.getByRole('textbox', { name: 'Name' }).fill('Sales review');
	await page.getByRole('button', { name: 'Create project', exact: true }).click();
	await expect(page.getByRole('button', { name: 'Open project Sales review' })).toBeVisible();
});

test('Sidebar workspace title stays clear of the hover new-conversation action', async ({ page }) => {
	const workspaceName = 'quarterly-revenue-analysis-workspace-with-a-long-name';
	const longFolder = {
		...folders[0],
		path: `C:\\FellaFixture\\${workspaceName}`,
		catalog: {
			...folders[0].catalog,
			workspace: `C:\\FellaFixture\\${workspaceName}`
		}
	};
	await installDesktopMock(page, { fixtureFolders: [longFolder, folders[1]] });
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Add repository' }).click();

	const workspace = page.getByRole('button', { name: `Open workspace ${workspaceName}` });
	const repository = page.locator('.repository').filter({ has: workspace });
	const title = repository.locator('.repository-name');
	await workspace.hover();

	const newConversation = repository.getByRole('button', { name: `New conversation in ${workspaceName}` });
	await expect(newConversation).toBeVisible();
	const [titleBounds, actionBounds] = await Promise.all([title.boundingBox(), newConversation.boundingBox()]);
	expect(titleBounds).not.toBeNull();
	expect(actionBounds).not.toBeNull();
	expect(titleBounds.x + titleBounds.width + 4).toBeLessThanOrEqual(actionBounds.x);
});

test('Sidebar conversation actions use shared buttons and Input, cancel rename, and delete history', async ({ page }) => {
	await installDesktopMock(page, { seedUnavailableConversation: true });
	await page.goto('/');
	await page.getByRole('button', { name: 'Expand unavailable-archive' }).click();
	const conversation = page.getByRole('button', {
		name: 'Open conversation: Summarize the unavailable archive'
	});
	await expect(conversation).toBeVisible();
	await expect(conversation).toHaveAttribute('data-slot', 'button');
	const conversationBounds = await conversation.boundingBox();
	expect(conversationBounds).not.toBeNull();
	expect(conversationBounds.height).toBeLessThanOrEqual(32);
	await conversation.hover();

	const rename = page.getByRole('button', { name: 'Rename conversation' });
	const remove = page.getByRole('button', { name: 'Delete conversation' });
	for (const action of [rename, remove]) {
		await expect(action).toHaveAttribute('data-slot', 'button');
		const bounds = await action.boundingBox();
		expect(bounds).not.toBeNull();
		expect(bounds.width).toBe(24);
		expect(bounds.height).toBe(24);
	}

	await rename.click();
	const renameInput = page.getByRole('textbox', { name: 'Rename conversation' });
	await expect(renameInput).toHaveAttribute('data-slot', 'input');
	await expect(renameInput).toBeFocused();
	await renameInput.fill('Unsaved title');
	await page.keyboard.press('Escape');
	await expect(renameInput).toHaveCount(0);
	await expect(conversation).toBeVisible();

	await conversation.hover();
	await page.getByRole('button', { name: 'Delete conversation' }).click();
	await expect(conversation).toHaveCount(0);
});

test('Investigation data catalog filters sources and opens contextual profiles', async ({ page }) => {
	const profiledFolder = {
		...folders[0],
		catalog: {
			...folders[0].catalog,
			sources: folders[0].catalog.sources.map((source) => ({
				...source,
				columns: [
					{ name: 'order_id', type: 'BIGINT', distinct: 124, null_fraction: 0, min: '1', max: '124' },
					{ name: 'amount', type: 'REAL', distinct: 83, null_fraction: 0, min: '0', max: '95000' }
				]
			}))
		}
	};
	await installDesktopMock(page, { fixtureFolders: [profiledFolder, ...folders.slice(1)] });
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	const readiness = page.getByTestId('investigation-data-readiness');
	await expect(readiness.getByRole('heading', { name: 'Data available' })).toBeVisible();

	const filter = page.getByRole('textbox', { name: 'Find a source or field' });
	await expect(filter).toHaveClass(/fella-ui-input/);
	await filter.fill('no-such-source');
	await expect(readiness.getByText('No sources or fields match “no-such-source”.')).toBeVisible();
	await filter.fill('sales.csv');
	const source = readiness.getByRole('button', { name: /sales\.csv/ });
	await expect(source).toBeVisible();
	await expect(source).toHaveAttribute('aria-expanded', 'false');
	await source.click();
	await expect(source).toHaveAttribute('aria-expanded', 'true');
	await expect(readiness.locator('#investigation-source-detail')).toBeVisible();
	const fieldProfile = readiness.locator('.field-profile');
	await fieldProfile.locator('summary').click();
	await expect(readiness.getByRole('table', { name: 'Observed field profile for sales.csv' })).toBeVisible();
});

test('global file search opens the matching source inside Investigation data', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await page.getByRole('button', { name: 'Search' }).click();

	const palette = page.getByRole('dialog', { name: 'Search Fella' });
	await palette.getByRole('textbox', { name: 'Search Fella' }).fill('sales.csv');
	const sourceResult = palette.locator('.search-result').filter({ hasText: 'sales.csv' });
	await expect(sourceResult).toHaveCount(1);
	await sourceResult.click();

	const readiness = page.getByTestId('investigation-data-readiness');
	await expect(palette).toHaveCount(0);
	await expect(readiness.locator('#investigation-source-detail')).toContainText('sales.csv');
	await expect(readiness.getByRole('button', { name: /sales\.csv/ })).toHaveAttribute('aria-expanded', 'true');
});

test('empty Investigation catalog offers a contextual folder replacement action', async ({ page }) => {
	const emptyFolder = {
		...folders[0],
		catalog: { ...folders[0].catalog, revision: 'empty-r1', sources: [] }
	};
	await installDesktopMock(page, { fixtureFolders: [emptyFolder, folders[1]] });
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await expect(page.getByTestId('investigation-data-readiness')).toContainText('Fella could not load any usable sources');

	const chooseAnother = page.getByRole('button', { name: 'Choose another folder' });
	await expect(chooseAnother).toHaveAttribute('data-slot', 'button');
	await chooseAnother.click();
	await expect.poll(() => page.evaluate(() => window.__workspaceOpenCalls)).toContain(folders[1].path);
	await expect(page.getByRole('article', { name: 'health-journal workspace' })).toBeVisible();
	await expect(page.getByRole('combobox', { name: 'Ask about health-journal' })).toBeVisible();
});

test('Settings actions use shared buttons and folder selection still works', async ({ page }) => {
	await installDesktopMock(page, {
		runLogEntries: [{
			id: 'run-42',
			at_ms: 1_700_000_000_000,
			kind: 'turn',
			mode: 'workspace_ask',
			model: 'gpt-4.1-mini',
			elapsed_ms: 1200,
			model_calls: [{ model: 'gpt-4.1-mini', duration_ms: 520, success: true }],
			operations: [{ operation: 'search_files', duration_ms: 38, success: true }],
			prior_analysis_count: 0,
			context_reference_count: 0,
			clarification_continuation: false,
			rerun: false,
			outcome: 'complete'
		}],
		providerList: [
			{ id: 'openai', display: 'OpenAI', auth: 'key', base_url: 'https://api.openai.com/v1', get_key_url: '', embeddings: true, authed: true, current: true },
			{ id: 'anthropic', display: 'Anthropic', auth: 'key', base_url: 'https://api.anthropic.com', get_key_url: '', embeddings: false, authed: false, current: false }
		]
	});
	await page.goto('/');
	await page.getByRole('button', { name: 'Settings' }).click();
	const runEntry = page.locator('.run-log-entry');
	await expect(runEntry).toHaveCount(1);
	const runEntryToggle = runEntry.getByRole('button');
	await expect(runEntryToggle).toHaveAttribute('aria-expanded', 'false');
	await runEntryToggle.focus();
	await page.keyboard.press('Enter');
	await expect(runEntryToggle).toHaveAttribute('aria-expanded', 'true');
	await expect(runEntry).toContainText('Tool steps');
	await expect(runEntry).toContainText('search_files');
	await page.keyboard.press('Space');
	await expect(runEntryToggle).toHaveAttribute('aria-expanded', 'false');
	await expect(runEntry.locator('.run-log-detail')).not.toBeVisible();

	for (const name of ['Change', 'Choose', 'Privacy and security', 'Refresh connection status', 'Choose a folder', 'Refresh']) {
		await expect(page.getByRole('button', { name, exact: true })).toHaveAttribute('data-slot', 'button');
	}
	const providers = page.locator('.provider-row');
	await expect(providers).toHaveCount(2);
	for (const provider of await providers.all()) await expect(provider).toHaveAttribute('data-slot', 'button');
	await expect(page.getByRole('button', { name: 'Anthropic Connect' })).toHaveAttribute('data-slot', 'button');
	const appearance = page.locator('.appearance-row');
	await expect(appearance).toHaveCount(3);
	for (const option of await appearance.all()) await expect(option).toHaveAttribute('data-slot', 'button');
	const darkAppearance = appearance.filter({ hasText: 'Dark' });
	await expect(darkAppearance).toHaveAttribute('aria-pressed', 'false');
	await darkAppearance.click();
	await expect(darkAppearance).toHaveAttribute('aria-pressed', 'true');
	await expect(page.locator('html')).toHaveAttribute('data-appearance', 'dark');

	await page.getByRole('button', { name: 'Choose a folder' }).click();
	await expect.poll(() => page.evaluate(() => window.__workspaceOpenCalls)).toContain(folders[0].path);
});

test('CommandPalette filters and results use shared buttons without losing search behavior', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Search' }).click();
	const palette = page.getByRole('dialog', { name: 'Search Fella' });
	const searchInput = page.getByRole('textbox', { name: 'Search Fella' });
	await searchInput.fill('northwind-sales');

	const repositoryFilter = palette.getByRole('button', { name: 'Repositories', exact: true });
	await expect(repositoryFilter).toHaveAttribute('data-slot', 'button');
	await expect(repositoryFilter).toHaveAttribute('aria-pressed', 'false');
	await repositoryFilter.click();
	await expect(repositoryFilter).toHaveAttribute('aria-pressed', 'true');
	const result = palette.locator('.search-result');
	await expect(result).toHaveCount(1);
	await expect(result).toHaveAttribute('data-slot', 'button');
	await expect(result).toContainText('northwind-sales');
	await result.click();
	await expect(palette).toHaveCount(0);
	await expect(page.getByRole('article', { name: 'northwind-sales workspace' })).toBeVisible();
});

test('Investigation data catalog searches and expands large source and unavailable-file lists', async ({ page }) => {
	const paginatedFolders = [...folders];
	const salesFolder = folders[0];
	const paginatedCatalog = {
		...salesFolder.catalog,
		sources: Array.from({ length: 205 }, (_, index) => {
			const number = String(index + 1).padStart(3, '0');
			return {
				name: `sales-${number}.csv`,
				path: `${salesFolder.path}\\sales-${number}.csv`,
				kind: 'csv',
				view: `sales_${number}`,
				row_count: 10,
				size_bytes: 1024,
				mtime: 1
			};
		}),
		skipped: Array.from({ length: 105 }, (_, index) => ({
			name: `unsupported-${String(index + 1).padStart(3, '0')}.bin`,
			reason: 'Unsupported file type'
		}))
	};
	paginatedFolders[0] = { ...salesFolder, catalog: paginatedCatalog };

	await installDesktopMock(page, { fixtureFolders: paginatedFolders });
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();

	const readiness = page.getByTestId('investigation-data-readiness');
	const sourceList = readiness.getByRole('list', { name: 'Cataloged data sources' });
	await expect(sourceList.locator('li')).toHaveCount(5);
	await readiness.getByRole('button', { name: 'Show all 205 matching sources' }).click();
	await expect(sourceList.locator('li')).toHaveCount(205);
	const sourceFilter = readiness.getByRole('textbox', { name: 'Find a source or field' });
	await sourceFilter.fill('sales-101.csv');
	await expect(sourceList.locator('li')).toHaveCount(1);
	await expect(sourceList).toContainText('sales-101.csv');

	const skippedFiles = readiness.locator('.skipped-sources');
	const skippedSummary = skippedFiles.locator('summary');
	await skippedSummary.click();
	await expect(skippedSummary).toContainText('105');
	const skippedFilter = skippedFiles.getByRole('textbox', { name: 'Filter unavailable files' });
	await expect(skippedFilter).toHaveAttribute('data-slot', 'input');
	await skippedFilter.fill('unsupported-001');
	await expect(skippedFiles).toContainText('1 matching file');
	await expect(skippedFiles.getByText('unsupported-001.bin')).toBeVisible();
	await skippedFilter.fill('');
	await skippedFiles.getByRole('button', { name: 'Show all 105 files' }).click();
	await expect(skippedFiles.locator('li')).toHaveCount(105);
});

test('Project title and wiki use shared editing controls and retain saved content', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Create project for northwind-sales' }).click();
	await page.getByRole('textbox', { name: 'Name' }).fill('Sales notes');
	await page.getByRole('button', { name: 'Create project', exact: true }).click();
	await page.getByRole('button', { name: 'Open project Sales notes' }).click();

	const title = page.getByRole('textbox', { name: 'Project name' });
	const wiki = page.getByRole('textbox', { name: 'Project wiki' });
	await expect(title).toHaveClass(/fella-ui-input/);
	await expect(wiki).toHaveClass(/fella-ui-textarea/);
	await title.fill('Sales field guide');
	await title.press('Tab');
	await expect(page.getByRole('button', { name: 'Open project Sales field guide' })).toBeVisible();

	await wiki.fill('Net revenue excludes refunds and chargebacks.');
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await page.getByRole('button', { name: 'Open project Sales field guide' }).click();
	await expect(page.getByRole('textbox', { name: 'Project wiki' })).toHaveValue(
		'Net revenue excludes refunds and chargebacks.'
	);
});

test('Projects retain stable Finding and analysis-artifact references without copying their content', async ({ page }) => {
	const references = [
		{ kind: 'finding', investigation_id: 'investigation-q221', finding_id: 'finding-q221', finding_revision: 2 },
		{ kind: 'analysis_artifact', analysis_turn_id: 'analysis-turn-q221', evidence_id: 'chart-evidence-q221' }
	];
	const project = {
		id: 'project-sales-evidence',
		name: 'Sales evidence',
		workspace: folders[0].path,
		body: 'Review selected, evidence-backed material here.',
		references,
		created_at_ms: 100,
		updated_at_ms: 100
	};
	await installDesktopMock(page);
	await page.addInitScript((record) => {
		localStorage.setItem('fella:projects', JSON.stringify([record]));
	}, project);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	const projectLink = page.getByRole('button', { name: 'Open project Sales evidence' });
	await projectLink.click();
	await page.getByRole('textbox', { name: 'Project wiki' }).fill('Curate by stable Finding and run/evidence IDs.');

	const storedProjects = await page.evaluate(() => JSON.parse(localStorage.getItem('fella:projects') ?? '[]'));
	expect(storedProjects).toHaveLength(1);
	expect(storedProjects[0]).toMatchObject({
		id: project.id,
		body: 'Curate by stable Finding and run/evidence IDs.',
		references
	});
	expect(storedProjects[0]).not.toHaveProperty('findings');
	expect(storedProjects[0]).not.toHaveProperty('artifacts');
});

test('Project actions open Investigation data and retain navigation and delete behavior', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Create project for northwind-sales' }).click();
	await page.getByRole('textbox', { name: 'Name' }).fill('Sales actions');
	await page.getByRole('button', { name: 'Create project', exact: true }).click();
	const openProject = page.getByRole('button', { name: 'Open project Sales actions' });
	await openProject.click();

	const askRepository = page.getByRole('button', { name: 'Ask repository' });
	const investigations = page.getByRole('button', { name: 'Investigations', exact: true });
	const deleteProject = page.getByRole('button', { name: 'Delete project' });
	for (const action of [askRepository, investigations, deleteProject]) {
		await expect(action).toHaveAttribute('data-slot', 'button');
	}

	await investigations.click();
	await expect(page.getByRole('heading', { name: 'Data available' })).toBeVisible();
	await openProject.click();
	await askRepository.click();
	await expect(page.getByRole('combobox', { name: 'Ask about northwind-sales' })).toBeVisible();

	await openProject.click();
	const dialogPromise = page.waitForEvent('dialog');
	const confirmationCheck = dialogPromise.then(async (confirmation) => {
		expect(confirmation.message()).toContain('Delete the local project');
		await confirmation.accept();
	});
	await Promise.all([deleteProject.click(), confirmationCheck]);
	await expect(openProject).toHaveCount(0);
});

test('Analysis details uses the shared button and preserves disclosure state', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('combobox', { name: 'Ask a question' }).fill('Summarize the workspace.');
	await page.getByRole('button', { name: 'Send' }).click();

	const answer = page.locator('.msg.assistant').last();
	await expect(answer).toContainText('Scoped to general.');
	const details = answer.getByRole('button', { name: 'Analysis details' });
	await expect(details).toHaveAttribute('data-slot', 'button');
	await expect(details).toHaveAttribute('aria-expanded', 'false');

	await details.click();
	await expect(details).toHaveAttribute('aria-expanded', 'true');
	await expect(answer).toContainText('Source details were not recorded for this archived answer.');

	await details.click();
	await expect(details).toHaveAttribute('aria-expanded', 'false');
	await expect(page.getByText('Source details were not recorded for this archived answer.')).toHaveCount(0);
});

test('General chat stays usable without creating or inheriting a workspace Investigation', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	const request = 'What is a useful way to compare two trend lines?';
	await page.getByRole('combobox', { name: 'Ask a question' }).fill(request);
	await page.getByRole('button', { name: 'Send' }).click();

	const answer = page.locator('.msg.assistant').last();
	await expect(answer).toContainText('Scoped to general.');
	const generalAsk = await page.evaluate(() => window.__workspaceAskCalls.at(-1));
	expect(generalAsk.workspaceId).toBeNull();
	expect(generalAsk.investigationId).toBeNull();
	expect(generalAsk.questionId).toBeNull();
	expect(generalAsk.question).toBe(request);
	expect(await page.evaluate(() => window.__workspaceInvestigationEnsureCalls)).toEqual([]);
	expect(await page.evaluate(() => window.__workspaceInvestigations())).toEqual([]);
});

test('a quick workspace question stays low-ceremony while keeping its answer evidence inspectable', async ({ page }) => {
	const request = 'What were total net sales?';
	const answerText = 'Net sales totaled $1,240 across 124 rows.';
	const query = 'SELECT SUM(net_sales) AS net_sales FROM sales';
	await installDesktopMock(page, {
		directAnswerText: answerText,
		detailEvidence: {
			id: 'quick-sales-evidence',
			tool: 'run_sql',
			note: 'Sum net sales',
			args: { query },
			sql: query,
			result_summary: 'Net sales totaled 1240 across 124 rows.',
			columns: ['net_sales'],
			rows: [[1240]],
			row_count: 1,
			ms: 8,
			sources: [{ source: 'sales.csv', table: 'sales' }]
		}
	});
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await expect(page.getByRole('region', { name: 'Workspace board' })).toBeVisible();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	const composer = page.getByRole('combobox', { name: 'Ask about northwind-sales' });
	await composer.fill(request);
	await page.getByRole('button', { name: 'Send' }).click();

	const answer = page.locator('.msg.assistant').last();
	await expect(answer).toContainText(answerText);
	await expect(composer).toBeVisible();
	await expect(page.getByRole('region', { name: 'Investigation', exact: true })).toHaveCount(0);
	await expect(page.getByRole('region', { name: 'Review this run' })).toHaveCount(0);
	await expect(page.getByRole('region', { name: 'Question run' }).last()).toBeVisible();

	const details = answer.getByRole('button', { name: 'Analysis details' });
	await details.click();
	await expect(answer).toContainText('Workspace tool results are linked to the steps below.');
	await expect(answer).toContainText('from sales.csv (sales)');
	const queryDisclosure = answer.getByRole('button', { name: 'show the query' });
	await queryDisclosure.click();
	await expect(answer.locator('pre.sql')).toContainText(query);
	await expect(answer.locator('.result')).toContainText('Net sales totaled 1240 across 124 rows.');
	await expect(answer.locator('.tablewrap')).toContainText('1240');
	await answer.getByRole('button', { name: 'hide', exact: true }).click();
	await details.click();
	await expect(details).toHaveAttribute('aria-expanded', 'false');
	await expect(answer).toContainText(answerText);
	await expect(composer).toBeVisible();
});

test('workspace-scoped analysis still auto-links its Investigation to the conversation', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('region', { name: 'Workspaces' }).getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill('Summarize sales trends');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.')).toBeVisible();

	const result = await page.evaluate(() => ({
		ask: window.__workspaceAskCalls.at(-1),
		ensureCalls: window.__workspaceInvestigationEnsureCalls,
		investigations: window.__workspaceInvestigations()
	}));
	expect(result.ask.workspaceId).toBe(folders[0].path);
	expect(result.ensureCalls).toHaveLength(1);
	expect(result.ensureCalls[0].workspaceId).toBe(folders[0].path);
	expect(result.ensureCalls[0].conversationId).toBe(result.ask.conversationId);
	expect(result.investigations).toHaveLength(1);
	expect(result.investigations[0].conversation_id).toBe(result.ask.conversationId);
});

test('Evidence query disclosure uses a shared button and preserves expansion behavior', async ({ page }) => {
	await installDesktopMock(page, {
		detailEvidence: {
			id: 'fixture-query',
			tool: 'run_sql',
			args: { query: 'SELECT total FROM sales' },
			sql: 'SELECT total FROM sales',
			result_summary: 'One row returned.',
			ms: 8
		}
	});
	await page.goto('/');
	await page.getByRole('combobox', { name: 'Ask a question' }).fill('Show the evidence.');
	await page.getByRole('button', { name: 'Send' }).click();

	const answer = page.locator('.msg.assistant').last();
	await answer.getByRole('button', { name: 'Analysis details' }).click();
	const reveal = answer.locator('.detailtoggle');
	await expect(reveal).toHaveAttribute('data-slot', 'button');
	await expect(reveal).toHaveText('show the query');
	await expect(reveal).toHaveAttribute('aria-expanded', 'false');
	await reveal.click();
	await expect(reveal).toHaveAttribute('aria-expanded', 'true');
	await expect(reveal).toHaveText('hide');
	await expect(answer.locator('pre.sql')).toContainText('SELECT total FROM sales');
	await reveal.click();
	await expect(reveal).toHaveAttribute('aria-expanded', 'false');
	await expect(answer.locator('pre.sql')).toHaveCount(0);
});

test('a saved run keeps its own method and evidence inspectable after a later answer', async ({ page }) => {
	const transcriptEvidence = {
		id: 'latest-transcript-evidence',
		tool: 'run_sql',
		args: { query: 'SELECT current_total FROM sales' },
		sql: 'SELECT current_total FROM sales',
		result_summary: 'Latest transcript-only evidence.',
		ms: 6
	};
	const persistedEvidence = {
		id: 'original-run-evidence',
		tool: 'run_sql',
		note: 'Original run: compare revenue by region.',
		args: { query: 'SELECT region, SUM(revenue) FROM sales GROUP BY region' },
		sources: [{ source: 'sales.csv', table: 'sales' }],
		sql: 'SELECT region, SUM(revenue) FROM sales GROUP BY region',
		result_summary: 'Original run returned two regions.',
		columns: ['region', 'revenue'],
		rows: [['West', 1200], ['East', 950]],
		row_count: 2,
		ms: 12
	};
	const savedAnalysisTurn = {
		id: 'fixture-turn',
		conversation_id: 'fixture-conversation',
		question: 'Compare revenue by region.',
		workspace: folders[0].path,
		workspace_revision: 'fixture-r1',
		state: 'accepted',
		contract: { assumptions: ['Treat a missing refund date as unknown, not as zero.'] },
		plan: { strategy: 'compiled_sql', steps: ['run_sql'] },
		trace: {
			id: 'fixture-trace',
			turn_id: 'fixture-turn',
			steps: [{ id: 'trace-sql', operation: 'run_sql', duration_ms: 12, success: true, summary: 'Original query completed.', sources: ['sales.csv'] }]
		},
		source_scope: { used_sources: [{ source: 'sales.csv', table: 'sales', evidence_ids: ['original-run-evidence'] }], skipped_sources: [] },
		investigation_context: {
			investigation_id: 'fixture-investigation',
			question_id: 'fixture-question',
			source_selection: { mode: 'selected_sources', included_paths: ['sales.csv'] },
			definitions: [{ id: 'definition-1', revision: 2, kind: 'assumption', text: 'A refund counts only when tied to a completed sale.' }]
		},
		result: {
			text: 'Original saved answer.',
			status: 'verified',
			verification: [],
			evidence: [
				persistedEvidence,
				{ tool: 'run_sql', sources: 'not-an-array', result_summary: 'Malformed source metadata.' },
				null
			]
		}
	};
	await installDesktopMock(page, {
		detailEvidence: transcriptEvidence,
		savedAnalysisTurn,
		replayStatus: {
			turn_id: 'fixture-turn',
			same_workspace: true,
			revision_changed: false,
			snapshot_available: true,
			can_rerun: false,
			source_changes: []
		}
	});
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('article', { name: 'northwind-sales workspace' }).locator('.tile-focus').click();
	const composer = page.getByRole('combobox', { name: 'Ask about northwind-sales' });
	await composer.fill('Compare revenue by region.');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(page.locator('.msg.assistant').first()).toContainText('Scoped to C:\\FellaFixture\\northwind-sales.');
	await composer.fill('Now check the current total.');
	await page.getByRole('button', { name: 'Send' }).click();
	const laterAnswer = page.locator('.msg.assistant').nth(1);
	await expect(laterAnswer).toContainText('Scoped to C:\\FellaFixture\\northwind-sales.');
	await laterAnswer.getByRole('button', { name: 'Analysis details' }).click();
	await laterAnswer.getByRole('button', { name: 'show the query' }).click();
	await expect(laterAnswer).toContainText('Latest transcript-only evidence.');

	const originalAnswer = page.locator('.msg.assistant').first();
	await originalAnswer.getByRole('button', { name: 'Analysis details' }).click();
	await originalAnswer.getByRole('button', { name: 'Open saved run record' }).click();
	const savedRunPanel = originalAnswer.locator('.saved-run-panel');
	await expect(savedRunPanel).toContainText('compiled sql');
	await expect(savedRunPanel).toContainText('Requested source scope');
	await expect(savedRunPanel).toContainText('1 selected source');
	await expect(savedRunPanel).toContainText('Assumption · revision 2');
	await expect(savedRunPanel).toContainText('A refund counts only when tied to a completed sale.');
	await expect(savedRunPanel.locator('.saved-run-assumptions')).toContainText('Treat a missing refund date as unknown, not as zero.');
	await expect(savedRunPanel.locator('.saved-run-assumptions')).toContainText('state the replacement in a follow-up');
	await expect(savedRunPanel.getByRole('heading', { name: 'Recorded evidence' })).toBeVisible();
	const recordedEvidence = savedRunPanel.locator('.saved-run-evidence');
	await expect(recordedEvidence.locator('li')).toHaveCount(1);
	await expect(recordedEvidence).toContainText('Original run: compare revenue by region.');
	await expect(recordedEvidence).toContainText('sales.csv (sales)');
	await expect(recordedEvidence).not.toContainText('Latest transcript-only evidence.');
	await recordedEvidence.locator('summary').click();
	await expect(recordedEvidence.locator('pre.sql')).toContainText('SELECT region, SUM(revenue) FROM sales GROUP BY region');
	await expect(recordedEvidence).toContainText('Original run returned two regions.');
	await expect(recordedEvidence).toContainText('West');
	await expect.poll(() => page.evaluate(() => window.__analysisTurnLoadCalls)).toEqual(['fixture-turn']);
});

test('Transcript setup links use shared buttons and retain the sign-in action', async ({ page }) => {
	await installDesktopMock(page, {
		providerHealth: { reachable: false, rejected: true, models: [] }
	});
	await page.goto('/');
	await page.getByRole('combobox', { name: 'Ask a question' }).fill('Check my data.');
	await page.getByRole('button', { name: 'Send' }).click();

	const setup = page.locator('.setup.compact');
	await expect(setup).toContainText('key was refused');
	const enterKey = setup.getByRole('button', { name: 'Enter a new key' });
	await expect(enterKey).toHaveAttribute('data-slot', 'button');
	await enterKey.click();
	await expect(page.getByText('unknown provider: openai')).toBeVisible();
});

test('Replay freshness uses shared status badges and rerun buttons', async ({ page }) => {
	await installDesktopMock(page, {
		replayStatus: {
			turn_id: 'fixture-turn',
			same_workspace: true,
			revision_changed: true,
			snapshot_available: true,
			can_rerun: true,
			source_changes: [{ name: 'sales.csv', kind: 'changed', details: ['Rows changed'] }]
		}
	});
	await page.goto('/');
	await page.getByRole('combobox', { name: 'Ask a question' }).fill('Check freshness.');
	await page.getByRole('button', { name: 'Send' }).click();

	const answer = page.locator('.msg.assistant').last();
	await expect(answer).toContainText('Scoped to general.');
	await answer.getByRole('button', { name: 'Analysis details' }).click();

	const freshness = answer.locator('[data-slot="badge"]');
	await expect(freshness).toHaveAttribute('class', /fella-ui-badge-warning/);
	await expect(freshness).toContainText('1 source changed since this answer');
	const rerun = answer.getByRole('button', { name: 'Rerun' });
	await expect(rerun).toHaveAttribute('data-slot', 'button');
	await expect(rerun).toBeEnabled();
	await expect(answer).toContainText('Rerun repeats this saved request and its recorded context.');
	await rerun.click();
	const failedRerun = page.getByRole('region', { name: 'Question run' }).last();
	await expect(failedRerun).toHaveAttribute('data-run-state', 'failed');
	await expect(failedRerun).toContainText('Rerun');
	await expect(failedRerun).toContainText('Analysis did not finish');
	await expect(failedRerun.locator('[data-step-state="error"]')).toHaveCount(1);
	await expect(page.locator('.msg.assistant').last()).toContainText('Rerun is not part of this journey');
});

test('Replay freshness lookup errors use an alert and a working retry action', async ({ page }) => {
	await installDesktopMock(page, { replayStatusError: true });
	await page.goto('/');
	await page.getByRole('combobox', { name: 'Ask a question' }).fill('Check freshness.');
	await page.getByRole('button', { name: 'Send' }).click();

	const answer = page.locator('.msg.assistant').last();
	await expect(answer).toContainText('Scoped to general.');
	await answer.getByRole('button', { name: 'Analysis details' }).click();

	const alert = answer.getByRole('alert');
	await expect(alert).toContainText("Couldn't check freshness.");
	await expect(alert).toContainText('Replay lookup unavailable.');
	const retry = alert.getByRole('button', { name: 'Try again' });
	await expect(retry).toHaveAttribute('data-slot', 'button');
	await retry.click();
	await expect.poll(() => page.evaluate(() => window.__replayStatusCalls)).toBe(2);
});

test('Investigation source details open beside without changing conversation scope', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	const readiness = page.getByTestId('investigation-data-readiness');
	await readiness.getByRole('button', { name: /sales\.csv/ }).click();

	const openBeside = page.getByRole('button', { name: 'Open source preview beside the active conversation' });
	await expect(openBeside).toHaveAttribute('data-slot', 'button');
	await openBeside.click();

	const companion = page.getByRole('complementary', { name: 'Companion pane' });
	await expect(companion).toBeVisible();
	await expect(companion.getByRole('heading', { name: 'sales.csv' })).toBeVisible();
	await expect(companion).toContainText('Preview only · this file is not added to the conversation context.');
	await expect(page.getByRole('combobox', { name: 'Ask about northwind-sales' })).toBeVisible();
});

test('Companion snapshot recovery uses a shared button and reopens the current source version', async ({ page }) => {
	await installDesktopMock(page, {
		reindexedCatalog: { ...folders[0].catalog, revision: 'sales-r2' }
	});
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await page.getByTestId('investigation-data-readiness').getByRole('button', { name: /sales\.csv/ }).click();
	await page.getByRole('button', { name: 'Open source preview beside the active conversation' }).click();

	const companion = page.getByRole('complementary', { name: 'Companion pane' });
	await page.getByRole('combobox', { name: 'Ask about northwind-sales' }).fill('/reindex');
	await page.getByRole('button', { name: 'Send' }).click();
	await expect(companion).toContainText('The workspace changed after this preview was opened.');

	const reopen = companion.getByRole('button', { name: 'Open current version' });
	await expect(reopen).toHaveAttribute('data-slot', 'button');
	await reopen.click();
	await expect(companion).not.toContainText('The workspace changed after this preview was opened.');
	await expect(companion).toContainText('Preview only · this file is not added to the conversation context.');
});

test('Chart disclosures use Collapsible and Open beside preserves the chart in the companion pane', async ({ page }) => {
	await page.context().grantPermissions(['clipboard-read', 'clipboard-write'], { origin: 'http://127.0.0.1:1421' });
	await installDesktopMock(page, {
		chartEvidence: [
			// The source result and chart are separate evidence objects, linked by
			// the stable evidence ID the engine stores in chart metadata.
			{
				id: 'fixture-source-result',
				tool: 'run_sql',
				args: { sql: 'SELECT month, SUM(revenue) AS revenue FROM sales GROUP BY month ORDER BY month' },
				sql: 'SELECT month, SUM(revenue) AS revenue FROM sales GROUP BY month ORDER BY month',
				result_summary: 'Three monthly revenue rows from sales.csv.',
				sources: [{ table: 'sales', source: 'sales.csv' }],
				columns: ['month', 'revenue'],
				rows: [['Jan', 12], ['Feb', 18], ['Mar', 15]],
				row_count: 3,
				ms: 6
			},
			{
				id: 'fixture-chart',
				tool: 'make_chart',
				args: {},
				result_summary: 'Monthly revenue chart, 3 plotted items.',
				ms: 12,
				chart: {
					kind: 'line',
					title: 'Monthly revenue',
					labels: ['Jan', 'Feb', 'Mar'],
					series: [{ name: 'Revenue', values: [12, 18, 15] }],
					unit: '$',
					x_label: 'Month',
					y_label: 'Revenue',
					metadata: {
						source_label: 'sales.csv',
						source_evidence_id: 'fixture-source-result',
						source_run_id: 'fixture-chart-run',
						finding_snapshot: 'Monthly revenue rose from January to February.',
						fields: ['month', 'revenue'],
						aggregation: 'sum',
						filters: [],
						part_to_whole: false
					}
				}
			}
		],
	});
	await page.goto('/');
	const composer = page.getByRole('combobox', { name: 'Ask a question' });
	await composer.fill('Show monthly revenue.');
	await composer.press('Enter');

	const chart = page.locator('.chart-card');
	await expect(chart).toContainText('Monthly revenue');
	const chartDetails = chart.getByRole('button', { name: 'Chart details' });
	await expect(chartDetails).toHaveAttribute('aria-expanded', 'false');
	await chartDetails.click();
	await expect(chartDetails).toHaveAttribute('aria-expanded', 'true');
	await expect(chart).toContainText('sales.csv');
	await expect(chart).toContainText('fixture-chart-run');
	await expect(chart).toContainText('fixture-source-result');
	await expect(chart).toContainText('Monthly revenue rose from January to February.');
	await expect(chart).toContainText('Three monthly revenue rows from sales.csv.');
	await expect(chart).toContainText('SELECT month, SUM(revenue) AS revenue FROM sales');
	const exactValues = chart.getByRole('button', { name: 'Show exact values' });
	await exactValues.click();
	await expect(exactValues).toHaveAttribute('aria-expanded', 'true');
	const exactTable = chart.getByRole('table', { name: 'Monthly revenue values' });
	await expect(exactTable).toContainText('$18');

	const exportTrigger = chart.getByRole('button', { name: 'Export chart and data' });
	await exportTrigger.click();
	await page.getByRole('menuitem', { name: 'Copy chart image' }).click();
	await expect(chart.getByRole('status')).toHaveText('Chart image copied to clipboard.');
	const copiedImageTypes = await page.evaluate(async () =>
		(await navigator.clipboard.read()).flatMap((item) => item.types)
	);
	expect(copiedImageTypes).toContain('image/png');

	await exportTrigger.click();
	const pngDownloadPromise = page.waitForEvent('download');
	await page.getByRole('menuitem', { name: 'Download chart image (PNG)' }).click();
	const pngDownload = await pngDownloadPromise;
	expect(pngDownload.suggestedFilename()).toBe('Monthly-revenue.png');
	const pngPath = await pngDownload.path();
	const pngBytes = await readFile(pngPath);
	expect([...pngBytes.subarray(0, 8)]).toEqual([137, 80, 78, 71, 13, 10, 26, 10]);

	await exportTrigger.click();
	await page.getByRole('menuitem', { name: 'Copy data' }).click();
	await expect(chart.getByRole('status')).toHaveText('Chart data copied to clipboard.');
	expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(
		'Month\tRevenue ($)\r\nJan\t12\r\nFeb\t18\r\nMar\t15'
	);

	await exportTrigger.click();
	const csvDownloadPromise = page.waitForEvent('download');
	await page.getByRole('menuitem', { name: 'Download data (CSV)' }).click();
	const csvDownload = await csvDownloadPromise;
	expect(csvDownload.suggestedFilename()).toBe('Monthly-revenue.csv');
	const csvPath = await csvDownload.path();
	expect(await readFile(csvPath, 'utf8')).toBe('\uFEFFMonth,Revenue ($)\r\nJan,12\r\nFeb,18\r\nMar,15');

	const openBeside = page.getByRole('button', { name: 'Open beside' });
	await expect(openBeside).toHaveAttribute('data-slot', 'button');
	await openBeside.click();

	const companion = page.getByRole('complementary', { name: 'Companion pane' });
	await expect(companion.getByRole('heading', { name: 'Monthly revenue' })).toBeVisible();
	const companionDetails = companion.getByRole('button', { name: 'Chart details' });
	await companionDetails.click();
	await expect(companion).toContainText('fixture-chart-run');
	await expect(companion).toContainText('Monthly revenue rose from January to February.');
	await expect(companion).toContainText('SELECT month, SUM(revenue) AS revenue FROM sales');
	await expect(page.getByLabel('Chart open beside conversation')).toContainText('Monthly revenue is open beside the conversation.');
});

test('Suggested follow-ups use shared link buttons and submit the selected question', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('combobox', { name: 'Ask a question' }).fill('Summarize the workspace.');
	await page.getByRole('button', { name: 'Send' }).click();

	const firstAnswer = page.locator('.msg.assistant').last();
	await expect(firstAnswer).toContainText('Scoped to general.');
	const followup = firstAnswer.getByRole('button', { name: 'What else stands out?' });
	await expect(followup).toHaveAttribute('data-slot', 'button');
	await followup.click();

	await expect(page.locator('.msg.user').last()).toContainText('What else stands out?');
	await expect(page.locator('.msg.assistant').last()).toContainText('Scoped to general.');
});

test('Workspace starter prompts use shared outline buttons and submit the selected prompt', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	const workspace = page.getByRole('button', { name: 'Open workspace northwind-sales' });
	await workspace.click();
	await workspace.hover();
	await page.getByRole('button', { name: 'New conversation in northwind-sales' }).click();

	const prompts = page.locator('.examples');
	await expect(prompts).toBeVisible();
	const prompt = prompts.getByRole('button', { name: 'How did my spending change this year?' });
	await expect(prompt).toHaveAttribute('data-slot', 'button');
	await prompt.click();

	await expect(page.locator('.msg.user').last()).toContainText('How did my spending change this year?');
	await expect(page.locator('.msg.assistant').last()).toContainText('Scoped to C:\\FellaFixture\\northwind-sales.');
});

test('Companion pane close uses a shared compact button and returns to the workspace conversation', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await page.getByTestId('investigation-data-readiness').getByRole('button', { name: /sales\.csv/ }).click();
	await page.getByRole('button', { name: 'Open source preview beside the active conversation' }).click();

	const companion = page.getByRole('complementary', { name: 'Companion pane' });
	await expect(companion).toBeVisible();
	const close = companion.getByRole('button', { name: 'Close companion pane and return to conversation' });
	await expect(close).toHaveAttribute('data-slot', 'button');
	const bounds = await close.boundingBox();
	expect(bounds).not.toBeNull();
	expect(bounds.width).toBe(30);
	expect(bounds.height).toBe(30);

	await close.click();
	await expect(companion).toHaveCount(0);
	await expect(page.getByRole('combobox', { name: 'Ask about northwind-sales' })).toBeVisible();
});
