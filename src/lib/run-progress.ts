import type { RuntimeTurnState } from './types';

/** Translate runtime lifecycle into short, analyst-facing progress language. */
export function runStateLabel(state: RuntimeTurnState | null | undefined): string | null {
	if (!state) return null;
	const labels: Record<RuntimeTurnState, string> = {
		received: 'Preparing the analysis',
		interpreting: 'Understanding the question',
		grounding: 'Checking data definitions',
		planning: 'Planning the analysis',
		executing: 'Working with the data',
		verifying: 'Checking the result',
		accepted: 'Answer checked',
		clarify: 'Waiting for your input',
		retry: 'Refining the analysis',
		needs_review: 'Result needs review',
		unsupported: 'This question is not supported by the available data',
		failed: "The analysis couldn't finish",
		cancelled: 'Analysis stopped'
	};
	return labels[state];
}

/** Lifecycle states that represent a distinct analyst-visible milestone. */
export function progressStepForState(state: RuntimeTurnState): string | null {
	switch (state) {
		case 'interpreting': return 'Understanding the question';
		case 'grounding': return 'Checking data definitions';
		case 'planning': return 'Planning the analysis';
		case 'verifying': return 'Checking the result';
		case 'retry': return 'Refining the analysis';
		default: return null;
	}
}

/** Hide implementation tool names and model-authored notes from the timeline. */
export function analystStepForTool(tool: string): string {
	const labels: Record<string, string> = {
		list_files: 'Finding relevant files',
		inspect_table: 'Checking the available fields',
		grep_files: 'Finding relevant passages',
		read_file: 'Reviewing source material',
		read_prior_analysis: 'Reviewing earlier results',
		run_sql: 'Calculating from the data',
		run_python: 'Analyzing the data',
		forecast_analysis: 'Testing a forecast',
		make_chart: 'Preparing a visualization'
	};
	return labels[tool] ?? 'Working with the data';
}
