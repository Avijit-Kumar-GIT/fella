import type { AnalysisTurn } from './types';

export interface BoundResult {
	id: string;
	columns: string[];
	rows: unknown[][];
	exact_rows: string[][];
	row_count: number;
	complete: boolean;
}
/** Free-form, presentation-only design decision; not a closed chart enum. */
export interface PresentationDesignPlan {
	analytical_intent: string;
	visual_thesis: string;
	primary_form: string;
	supporting_elements: string[];
	interaction: string | null;
	why_this_fits: string;
	avoid: string[];
}
export interface ArtifactRevision {
	revision: number;
	parent_revision?: number | null;
	run_id: string;
	program: string;
	library_version: string;
	change_note: string;
	created_at_ms: number;
}
export interface ArtifactRecord {
	id: string;
	workspace: string;
	title: string;
	current_revision: number;
	saved: boolean;
	revisions: ArtifactRevision[];
	created_at_ms: number;
	updated_at_ms: number;
}
export interface ResolvedArtifact {
	artifact: ArtifactRecord;
	revision: ArtifactRevision;
	run: AnalysisTurn;
	results: BoundResult[];
}
export type NumericDelta =
	| { kind: 'exact_integer'; delta: string }
	| { kind: 'finite_float'; delta: number }
	| { kind: 'unavailable'; reason: string };
export interface ArtifactComparison {
	compatible: boolean;
	reasons: string[];
	warnings: string[];
	before_revision: number;
	after_revision: number;
	source_revision_changed: boolean;
	definitions_or_filters_changed: boolean;
	tables: Array<{
		before_result_id: string;
		after_result_id: string;
		key_fields: string[];
		values: Array<{ key: string[]; field: string; unit: string; before: string; after: string; delta: NumericDelta }>;
	}>;
}
export interface StudioPlacement {
	id: string;
	artifact_id: string;
	revision: number;
	view_state: Record<string, unknown>;
}
export interface StudioCheckpoint {
	id: string;
	title: string;
	body: string;
	placements: StudioPlacement[];
	created_at_ms: number;
}
export interface StudioDocument {
	id: string;
	workspace: string;
	title: string;
	version: number;
	body: string;
	placements: StudioPlacement[];
	checkpoints: StudioCheckpoint[];
	created_at_ms: number;
	updated_at_ms: number;
}

export interface ReportPackageSummary {
	id: string;
	title: string;
	artifact_count: number;
	run_count: number;
	modified_at_ms: number;
}
export interface ImportedReportPackage {
	format: 'fella-report';
	version: number;
	manifest: { generator: string; origin_workspace_fingerprint: string; snapshot_at_ms: number; artifact_count: number; run_count: number; content_sha256: string; exclusions: string[] };
	document: { origin_document_id: string; origin_version: number; title: string; body: string; placements: Array<{ id: string; artifact_id: string; view_state: Record<string, unknown> }> };
	artifacts: Array<{ id: string; origin_artifact_id: string; origin_revision: number; title: string; run_id: string; program: string; library_version: string; change_note: string }>;
	runs: Array<{ id: string; origin_run_id: string; original_question: string; answer: string; state: string; result_status: string; verification_status: string | null; checks: unknown[]; result_checks: unknown[]; source_revision: string | null; semantics: unknown; definitions: string[]; source_labels: string[]; methods: Array<{ evidence_id: string; tool: string; sql: string | null }>; results: BoundResult[] }>;
}
export interface ReportPackagePreview {
	read_only: boolean;
	programs_executable: boolean;
	source_authority: boolean;
	title: string;
	artifact_count: number;
	run_count: number;
	warnings: string[];
	package: ImportedReportPackage;
}
export interface ImportedReport {
	id: string;
	preview: ReportPackagePreview;
}
