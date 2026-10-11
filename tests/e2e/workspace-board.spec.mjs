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
		savedAnalysisTurn = null,
		questionRunOutcomes = [],
		providerList = [],
		providerHealth = { reachable: true, rejected: false, models: ['gpt-4.1-mini'] },
		runLogEntries = []
	} = {}
) {
	await page.addInitScript(({ fixtureFolders, seedUnavailableConversation, seedDeletionConversations, replayStatus, replayStatusError, reindexedCatalog, chartEvidence, detailEvidence, savedAnalysisTurn, questionRunOutcomes, providerList, providerHealth, runLogEntries }) => {
		const calls = [];
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
					case 'investigation_mark_complete': {
						for (const [key, record] of investigations) {
							if (record.id !== args.investigationId) continue;
						const questions = (record.questions ?? []).filter((question) => question.status === 'active');
						if (!questions.length || questions.some((question) => question.answer_status !== 'answered')) {
							throw new Error('Every active Question needs a complete answer before this Investigation can be completed.');
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
							const finding = { id: `finding-${findingRecords.size + 1}`, investigation_id: record.id, caveats: [], assumptions: [], ...findingInput, created_at_ms: now, updated_at_ms: now };
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
							const now = Date.now();
							const updatedFinding = { ...finding, claim: args.claim, updated_at_ms: now };
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
							const now = Date.now();
							const updatedFinding = { ...finding, ...args.update, updated_at_ms: now };
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
				if (questionTurnId) {
					onEvent({ kind: 'turn_state', turn_id: questionTurnId, state: 'interpreting' });
					onEvent({ kind: 'turn_state', turn_id: questionTurnId, state: 'planning' });
					onEvent({
						kind: 'tool_start',
						tool: 'grep_files',
						args: { note: 'Search the workspace for implementation details' }
					});
					onEvent({
						kind: 'tool_end',
						item: {
							id: `fixture-progress-${questionTurnNumber}`,
							tool: 'grep_files',
							args: {},
							result_summary: 'Raw file search details must remain out of the primary timeline.',
							ms: 4,
							sources: []
						}
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
				const answer = {
					text: questionTurnId && questionOutcome.answer_text
						? questionOutcome.answer_text
						: `Scoped to ${params.workspaceId ?? 'general'}.`,
					status: 'complete',
					turn_id: questionTurnId ?? (hasReplayEvidence ? 'fixture-turn' : undefined),
					evidence: [
						...(hasReplayEvidence
							? [{ id: 'fixture-replay-step', tool: 'run_sql', args: {}, result_summary: 'One row inspected.', ms: 12 }]
							: []),
						...(Array.isArray(questionOutcome.evidence) ? questionOutcome.evidence : []),
						...(detailEvidence ? [detailEvidence] : []),
						...(Array.isArray(chartEvidence) ? chartEvidence : chartEvidence ? [chartEvidence] : [])
					],
					verification: questionOutcome.checks ?? [],
					workspace: params.workspaceId ? { path: params.workspaceId, revision: 'fixture-r1' } : null
				};
				if (questionTurnId) {
					const investigation = [...investigations.values()].find((item) => item.id === params.investigationId);
					const question = questionRecords.get(params.questionId);
					const verificationStatus = questionOutcome.verification_status ?? (questionOutcome.status === 'answered'
						? 'verified'
						: questionOutcome.status === 'blocked' ? 'failed' : 'insufficient_data');
					const verificationChecks = questionOutcome.checks ?? [];
					analysisTurns.set(questionTurnId, {
						id: questionTurnId,
						conversation_id: params.conversationId ?? 'fixture-conversation',
						question: params.question,
						workspace: params.workspaceId,
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
						trace: {
							id: `fixture-trace-${questionTurnNumber}`,
							turn_id: questionTurnId,
							workspace_revision: 'fixture-r1',
							steps: [{ id: `fixture-step-${questionTurnNumber}`, operation: 'run_sql', duration_ms: 4, success: true, sources: ['sales.csv'] }]
						},
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
	}, { fixtureFolders, seedUnavailableConversation, seedDeletionConversations, replayStatus, replayStatusError, reindexedCatalog, chartEvidence, detailEvidence, savedAnalysisTurn, questionRunOutcomes, providerList, providerHealth, runLogEntries });
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
	await salesFocus.click();
	await expect(salesFocus).toHaveAttribute('aria-pressed', 'true');
	await expect(page.getByRole('combobox', { name: 'Ask about northwind-sales' })).toBeVisible();
	const salesWorkspaceRow = page.getByRole('button', { name: 'Open workspace northwind-sales' });
	await expect(page.getByRole('button', { name: 'Open sources in northwind-sales' })).toBeVisible();
	await expect(page.getByRole('button', { name: 'Create project for northwind-sales' })).toBeVisible();
	await page.getByRole('button', { name: 'Open sources in northwind-sales' }).click();
	await expect(page.getByRole('heading', { name: 'sales.csv' })).toBeVisible();
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
	await expect(page.getByRole('button', { name: 'Open sources in health-journal' })).toBeVisible();
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
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	await expect(page.getByRole('heading', { name: 'Investigation brief' })).toBeVisible();
	const openConversation = page.getByRole('button', { name: 'Open conversation', exact: true });
	await expect(openConversation).toBeVisible();
	const linked = await page.evaluate(() => window.__workspaceInvestigations()[0]);
	expect(linked.conversation_id).toBe(askCall.conversationId);
	await openConversation.click();
	await expect(page.getByRole('combobox', { name: 'Ask about northwind-sales' })).toBeVisible();
	await expect(page.locator('.msg.user').filter({ hasText: 'Summarize cohort retention' })).toHaveCount(1);
	expect(await page.evaluate(() => window.__workspaceAskCalls.length)).toBe(1);
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
	await questionCard.getByText(/Source set/).click();
	await questionCard.getByLabel('Use a specific source set').check();
	await questionCard.getByLabel('refunds.csv').uncheck();
	await page.getByRole('button', { name: 'Add definition or assumption' }).click();
	await page.getByRole('textbox', { name: 'Context note 1' }).fill('Count a refund only when it is linked to a completed sale.');

	await page.getByRole('button', { name: 'Analyze Subquestion 1' }).click();
	const setupPreview = page.getByRole('region', { name: 'Review this run' });
	await expect(setupPreview).toContainText('Compare refund rates with sales');
	await expect(setupPreview).toContainText('Question-specific scope · 1 file');
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
			{ status: 'incomplete', reason: 'The selected files do not cover the full requested period.', delay_ms: 800 },
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
			const timeline = page.getByRole('region', { name: 'Question run' });
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
		if (questionRunCount === 1) {
			await expect(page.getByRole('region', { name: 'Question run' })).toBeHidden();
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

test('coverage links expose every open Question and completion waits for verified answers', async ({ page }) => {
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
		await expect(page.getByRole('region', { name: 'Question run' })).toBeHidden();
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

	await runQuestion('Analyze main Question');
	await runQuestion('Analyze Subquestion 1');
	await runQuestion('Analyze Subquestion 3');
	await expect(coverage.locator('[data-coverage="answered"] dd')).toHaveText('4');
	await expect(coverage.locator('[data-coverage="partial"] dd')).toHaveText('0');
	await expect(coverage.locator('[data-coverage="unanswered"] dd')).toHaveText('0');
	await expect(coverage.locator('[data-coverage="unsupported"] dd')).toHaveText('0');
	const markComplete = page.getByRole('button', { name: 'Mark complete' });
	await expect(markComplete).toBeEnabled();
	await markComplete.click();
	await expect(coverage).toHaveAttribute('data-investigation-status', 'complete');
	await expect(coverage).toContainText('Investigation complete.');
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
	await expect(page.getByRole('region', { name: 'Question run' })).toBeHidden();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.').last()).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
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
	await expect(page.getByRole('region', { name: 'Question run' })).toBeHidden();
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
	await expect(page.getByRole('region', { name: 'Question run' })).toBeHidden();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();

	const subquestion = page.locator('.question-list .question-item').first();
	await expect(subquestion.locator('[data-slot="badge"][data-question-status]')).toHaveText('Complete answer');
	await subquestion.getByRole('button', { name: 'View complete answer for Subquestion 1' }).click();
	const completePreview = subquestion.getByRole('region', { name: 'Latest answer for Subquestion 1' });
	await expect(completePreview).toContainText('Refunds were highest in the online channel.');
	await expect(completePreview).toContainText('fixture-question-turn-2');
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
	const questionRun = page.getByRole('region', { name: 'Question run' });
	await expect(questionRun).toBeVisible();
	await expect(questionRun).toHaveAttribute('data-question-id', /^q-root-/);
	await expect(questionRun).toBeHidden();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.').last()).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();

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
	await expect(page.getByRole('region', { name: 'Question run' })).toBeHidden();
	await expect(page.getByText('Scoped to C:\\FellaFixture\\northwind-sales.').last()).toBeVisible();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();

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
	await expect(page.getByRole('heading', { name: 'Definitions & assumptions' })).toBeVisible();

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
	await expect(page.getByRole('heading', { name: 'Definitions & assumptions' })).toBeVisible();
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
	await page.getByRole('button', { name: 'Add definition or assumption' }).click();
	await page.getByRole('textbox', { name: 'Context note 1' }).fill('One row represents one signup-month cohort; retention is retained accounts divided by cohort accounts.');
	await page.getByRole('button', { name: 'Save context' }).click();
	await expect(page.locator('.definition-editor')).toContainText('Context saved');
	await page.getByRole('button', { name: 'Analyze main Question' }).click();
	await page.getByRole('region', { name: 'Review this run' }).getByRole('button', { name: 'Run this Question' }).click();
	await expect(page.getByRole('region', { name: 'Question run' })).toBeHidden();
	await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();

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

	await runDetail.locator('.analysis-run-trace > summary').click();
	await expect(runDetail.locator('.analysis-run-trace')).toContainText('run sql');
	await expect(runDetail.locator('.analysis-run-trace')).toContainText('Succeeded');
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
		await expect(page.getByRole('region', { name: 'Question run' })).toBeHidden();
		await page.getByRole('button', { name: 'Open investigation brief in northwind-sales' }).click();
	};
	await runQuestion();
	await savedDefinition.fill('Net sales include refunds.');
	await page.getByRole('button', { name: 'Save context' }).click();
	await expect(page.locator('.definition-item')).toContainText('Saved revision 2');
	await runQuestion();

	const history = page.locator('.analysis-runs');
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
	await expect(page.getByRole('heading', { name: 'Investigation brief' })).toBeVisible();
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
	await expect(page.getByRole('heading', { name: 'Investigation brief' })).toBeVisible();
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
	await page.getByRole('button', { name: 'Open sources in northwind-sales' }).click();
	await expect(page.getByRole('heading', { name: 'sales.csv' })).toBeVisible();
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
	const initialOpenCalls = await page.evaluate(() => window.__workspaceOpenCalls.length);
	expect(initialOpenCalls).toBe(1);

	await page.getByRole('button', { name: 'Reconnect unavailable-archive' }).click();
	await expect.poll(() => page.evaluate(() => window.__workspaceOpenCalls.length)).toBe(2);
	await expect(page.getByText(/Its conversations remain in history only/)).toBeVisible();
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
	await page.getByRole('button', { name: 'Open sources in northwind-sales' }).click();

	const workspaceTabs = page.getByRole('tablist', { name: 'Workspace sections' });
	const sourcesTab = workspaceTabs.getByRole('tab', { name: 'Sources' });
	const guideTab = workspaceTabs.getByRole('tab', { name: 'Guide' });
	await expect(sourcesTab).toHaveAttribute('aria-selected', 'true');
	await sourcesTab.focus();
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

test('Sidebar workspace tools use shared buttons and preserve source and project actions', async ({ page }) => {
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
	await expect(page.getByRole('button', { name: 'Open sources in northwind-sales' })).toHaveCount(0);
	await expandWorkspace.click();
	await expect(page.getByRole('button', { name: 'Open sources in northwind-sales' })).toBeVisible();

	const sources = page.getByRole('button', { name: 'Open sources in northwind-sales' });
	const createProject = page.getByRole('button', { name: 'Create project for northwind-sales' });
	for (const action of [sources, createProject]) {
		await expect(action).toHaveAttribute('data-slot', 'button');
		const bounds = await action.boundingBox();
		expect(bounds).not.toBeNull();
		expect(bounds.height).toBeLessThanOrEqual(32);
	}

	await sources.click();
	await expect(page.getByRole('heading', { name: 'sales.csv' })).toBeVisible();
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

test('Sources catalog filter uses the shared input and preserves source selection', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Open sources in northwind-sales' }).click();
	await expect(page.getByRole('heading', { name: 'sales.csv' })).toBeVisible();

	const filter = page.getByRole('textbox', { name: 'Filter sources' });
	await expect(filter).toHaveClass(/fella-ui-input/);
	await filter.fill('no-such-source');
	await expect(page.getByText('No sources match “no-such-source”.')).toBeVisible();
	await filter.fill('sales.csv');
	const source = page.getByRole('option', { name: /sales\.csv/ });
	await expect(source).toBeVisible();
	await expect(source).toHaveAttribute('data-slot', 'button');
	await expect(source).toHaveAttribute('aria-selected', 'true');
});

test('empty Sources action uses a shared button and opens the selected folder', async ({ page }) => {
	const emptyFolder = {
		...folders[0],
		catalog: { ...folders[0].catalog, revision: 'empty-r1', sources: [] }
	};
	await installDesktopMock(page, { fixtureFolders: [emptyFolder, folders[1]] });
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Open sources in northwind-sales' }).click();

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

test('Sources pagination uses shared buttons and preserves catalog and skipped-file boundaries', async ({ page }) => {
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
	await page.getByRole('button', { name: 'Open sources in northwind-sales' }).click();

	const sourcePages = page.getByRole('navigation', { name: 'Source pages' });
	const sourcePrevious = sourcePages.getByRole('button', { name: 'Previous' });
	const sourceNext = sourcePages.getByRole('button', { name: 'Next' });
	for (const action of [sourcePrevious, sourceNext]) {
		await expect(action).toHaveAttribute('data-slot', 'button');
	}
	await expect(sourcePages).toContainText('1–100 of 205');
	await expect(sourcePrevious).toBeDisabled();
	await sourceNext.click();
	await expect(sourcePages).toContainText('101–200 of 205');
	await expect(page.getByRole('option').first()).toContainText('sales-101.csv');
	await sourceNext.click();
	await expect(sourcePages).toContainText('201–205 of 205');
	await expect(sourceNext).toBeDisabled();
	await expect(page.getByRole('option')).toHaveCount(5);

	const skippedFiles = page.locator('.skipped');
	const skippedToggle = skippedFiles.getByRole('button', { name: '105 skipped files Why?' });
	await skippedToggle.click();
	await expect(skippedToggle).toHaveAttribute('aria-expanded', 'true');
	const skippedFilter = page.getByRole('textbox', { name: 'Filter skipped files' });
	await expect(skippedFilter).toHaveAttribute('data-slot', 'input');
	await skippedFilter.fill('unsupported-001');
	await expect(skippedFiles.locator('.skipped-count')).toHaveText('1–1 of 1 files');
	await expect(skippedFiles.getByText('unsupported-001.bin')).toBeVisible();
	await skippedFilter.fill('');
	await expect(skippedFiles.locator('.skipped-count')).toHaveText('1–100 of 105 files');

	const skippedPages = page.getByRole('navigation', { name: 'Skipped file pages' });
	const skippedPrevious = skippedPages.getByRole('button', { name: 'Previous' });
	const skippedNext = skippedPages.getByRole('button', { name: 'Next' });
	for (const action of [skippedPrevious, skippedNext]) {
		await expect(action).toHaveAttribute('data-slot', 'button');
	}
	await expect(skippedPages).toContainText('1–100 of 105');
	await expect(skippedPrevious).toBeDisabled();
	await skippedNext.click();
	await expect(skippedPages).toContainText('101–105 of 105');
	await expect(skippedNext).toBeDisabled();
	await skippedToggle.click();
	await expect(skippedToggle).toHaveAttribute('aria-expanded', 'false');
	await expect(skippedFilter).not.toBeVisible();
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
	await page.getByRole('button', { name: 'Open sources in northwind-sales' }).click();
	await page.getByRole('button', { name: 'Open project Sales field guide' }).click();
	await expect(page.getByRole('textbox', { name: 'Project wiki' })).toHaveValue(
		'Net revenue excludes refunds and chargebacks.'
	);
});

test('Project actions use shared buttons and retain their navigation and delete behavior', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Create project for northwind-sales' }).click();
	await page.getByRole('textbox', { name: 'Name' }).fill('Sales actions');
	await page.getByRole('button', { name: 'Create project', exact: true }).click();
	const openProject = page.getByRole('button', { name: 'Open project Sales actions' });
	await openProject.click();

	const askRepository = page.getByRole('button', { name: 'Ask repository' });
	const sources = page.getByRole('button', { name: 'Sources', exact: true });
	const deleteProject = page.getByRole('button', { name: 'Delete project' });
	for (const action of [askRepository, sources, deleteProject]) {
		await expect(action).toHaveAttribute('data-slot', 'button');
	}

	await sources.click();
	await expect(page.getByRole('heading', { name: 'sales.csv' })).toBeVisible();
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

test('Sources open-beside action uses the shared button and preserves conversation scope', async ({ page }) => {
	await installDesktopMock(page);
	await page.goto('/');
	await page.getByRole('button', { name: 'Add repository' }).click();
	await page.getByRole('button', { name: 'Open sources in northwind-sales' }).click();

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
	await page.getByRole('button', { name: 'Open sources in northwind-sales' }).click();
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
	await page.getByRole('button', { name: 'Open sources in northwind-sales' }).click();
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
