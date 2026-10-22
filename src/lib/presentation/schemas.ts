import { z } from 'zod/v4';
import { PRESENTATION_LIMITS as limits } from './types';

const label = z.string().min(1).max(240);
const resultId = z.string().min(1).max(240);
// Element-node validation happens recursively after the real OpenUI parser resolves references.
const children = z.array(z.unknown()).min(1).max(limits.components);

/** One definition source drives the actual Svelte registry, prompt and strict validation. */
export const componentDefinitions = {
	Document: {
		description: 'The root analytical document. Children are registered component calls or references.',
		props: z.object({ children }).strict()
	},
	Section: {
		description: 'A titled, left-aligned section of analytical content.',
		props: z.object({ title: label, children }).strict()
	},
	Columns: {
		description: 'A responsive group, at most three columns. Collapses naturally on narrow surfaces.',
		props: z.object({ children, count: z.number().int().min(1).max(3).optional() }).strict()
	},
	Text: {
		description: 'Plain explanatory text, not HTML. Never use it to replace bound analytical values.',
		props: z.object({ content: z.string().min(1).max(limits.text) }).strict()
	},
	Metric: {
		description: 'An exact saved value. Use resultId, column field, zero-based row, title, optional unit.',
		props: z.object({ resultId, field: label, row: z.number().int().min(0), title: label, unit: z.string().max(40).optional() }).strict()
	},
	Table: {
		description: 'A saved result with exact values. Optional fields select columns, never replacement data.',
		props: z.object({ resultId, fields: z.array(label).min(1).max(100).optional() }).strict()
	},
	Chart: {
		description: 'A data-bound Vega-Lite JSON specification: layers, facets, concat, encodings, marks and style. No data, datasets, runtime expressions, parameters, transforms, URLs or executable code. Literal-only style arithmetic is allowed. Fields must exist in resultId. Values come only from that result.',
		props: z.object({ resultId, spec: z.record(z.string(), z.unknown()), title: label.optional() }).strict()
	},
	Tabs: {
		description: 'Local presentation tabs. Stable name, labels and equally sized children arrays. No queries are run.',
		props: z.object({ name: label, labels: z.array(label).min(1).max(12), children }).strict()
	},
	ResultFilter: {
		description: 'A local exact-match selector over a bound result. Filters only saved rows, never the source dataset; visible scope warning on incomplete results. Fields cannot change analytical definitions.',
		props: z.object({ resultId, field: label, name: label, title: label.optional() }).strict()
	},
	FollowUp: {
		description: 'Explicit user-pressed follow-up request. Emits a host callback only; no tool, code, URL or IPC execution.',
		props: z.object({ label, message: z.string().min(1).max(2000) }).strict()
	}
} as const;

export type ComponentName = keyof typeof componentDefinitions;
