import type { Message } from '../types';
import type { BoundResult } from './types';
import { validatePresentation } from './validate';

/** A bounded display cache; canonical analytical records remain in Rust. */
export interface PresentationSnapshot {
	workspace: string;
	turn_id: string;
	program: string;
	results: BoundResult[];
}
const CACHE_CHARACTERS = 256_000;

export function checkedSnapshot(snapshot: PresentationSnapshot | undefined, message: Message): PresentationSnapshot | null {
	if (!snapshot || snapshot.workspace !== message.answer?.workspace?.path || snapshot.turn_id !== message.answer?.turn_id) return null;
	try { return validatePresentation(snapshot.program, snapshot.results).valid ? snapshot : null; }
	catch { return null; }
}

export function cachePresentation(message: Message, program: string, results: BoundResult[]): void {
	const workspace = message.answer?.workspace?.path, turn_id = message.answer?.turn_id;
	if (!workspace || !turn_id) return;
	const snapshot = { workspace, turn_id, program, results };
	// Large result tables stay in the canonical store, rather than filling the
	// transcript/localStorage with another large copy of the same data.
	if (JSON.stringify(snapshot).length <= CACHE_CHARACTERS) message.presentation_snapshot = snapshot;
}

/** Legacy transcripts can display accepted, precision-safe evidence immediately
 * while their canonical artifact is loaded. Unsafe integer cells wait for Rust's
 * exact_rows, so the preview cannot round an identifier or analytical value. */
export function transcriptPresentation(message: Message): PresentationSnapshot | null {
	const cached = checkedSnapshot(message.presentation_snapshot, message);
	if (cached) return cached;
	const answer = message.answer;
	if (!answer?.workspace || !answer.turn_id || !answer.presentation_program || answer.clarification) return null;
	const results: BoundResult[] = [];
	for (const item of answer.evidence) {
		if (!['run_sql', 'run_python', 'forecast_analysis'].includes(item.tool) || item.error || !item.id) continue;
		const disposition = item.verifier_disposition;
		if (disposition && (disposition.state !== 'artifact_withheld' || ['table', 'result_table', 'openui', 'presentation'].includes(disposition.artifact))) continue;
		const table = item.result_table ?? item;
		if (!table.columns || !table.rows) continue;
		if (table.rows.some(row => row.some(value => typeof value === 'number' && Number.isInteger(value) && !Number.isSafeInteger(value)))) return null;
		const count = item.result_table ? table.rows.length : item.row_count;
		const truncated = /capped|truncated/i.test(item.result_summary) || (item as unknown as { truncated?: boolean }).truncated;
		results.push({ id: item.id, columns: table.columns, rows: table.rows, exact_rows: table.rows.map(row => row.map(value => value === null ? 'null' : String(value))), row_count: Math.max(count ?? table.rows.length, table.rows.length), complete: count === table.rows.length && !truncated });
	}
	const snapshot = { workspace: answer.workspace.path, turn_id: answer.turn_id, program: answer.presentation_program, results };
	return checkedSnapshot(snapshot, message);
}
