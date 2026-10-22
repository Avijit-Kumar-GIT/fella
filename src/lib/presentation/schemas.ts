import { z } from 'zod/v4';
import { markReactive } from '@openuidev/lang-core';
import { PRESENTATION_LIMITS as limits } from './types';

const label = z.string().min(1).max(240);
const resultId = z.string().min(1).max(240);
// Element-node validation happens recursively after the real OpenUI parser resolves references.
const children = z.array(z.unknown()).min(1).max(limits.components);
const selectOptions = z.array(z.object({ value: z.string().max(240), label: z.string().max(240) }).strict()).min(1).max(200);

/** One definition source drives the actual Svelte registry, prompt and strict validation. */
export const componentDefinitions = {
	Document: {
		description: 'The root analytical document. Children are registered component calls or references.',
		props: z.object({ children }).strict()
	},
	Card: {
		description: 'A restrained surface that groups related content. Use sparingly; do not wrap every component in a card.',
		props: z.object({ children, title: label.nullable().optional(), detail: z.string().max(1000).nullable().optional(), tone: z.enum(['plain', 'quiet', 'accent']).nullable().optional() }).strict()
	},
	Section: {
		description: 'A titled, left-aligned section of analytical content.',
		props: z.object({ title: label, children }).strict()
	},
	Columns: {
		description: 'A responsive group, at most three columns. Collapses naturally on narrow surfaces.',
		props: z.object({ children, count: z.number().int().min(1).max(3).nullable().optional() }).strict()
	},
	Row: {
		description: 'A flexible horizontal group. Wraps on narrow surfaces.',
		props: z.object({ children, gap: z.number().int().min(0).max(8).nullable().optional(), align: z.enum(['start', 'center', 'end', 'stretch']).nullable().optional() }).strict()
	},
	Grid: {
		description: 'A responsive grid for a small related set of items. It collapses to one column on narrow surfaces.',
		props: z.object({ children, columns: z.number().int().min(1).max(4).nullable().optional() }).strict()
	},
	Divider: {
		description: 'A quiet separator between two larger groups, not between every component.',
		props: z.object({ label: z.string().max(240).nullable().optional() }).strict()
	},
	Spacer: {
		description: 'A small amount of intentional breathing room.',
		props: z.object({ size: z.enum(['sm', 'md', 'lg']).nullable().optional() }).strict()
	},
	Title: {
		description: 'A clear heading for the answer or one of its major sections.',
		props: z.object({ content: label, level: z.number().int().min(1).max(4).nullable().optional() }).strict()
	},
	Text: {
		description: 'Plain explanatory text, not HTML. Use expressions to bind analytical values from saved results.',
		props: z.object({ content: z.string().min(1).max(limits.text) }).strict()
	},
	Caption: {
		description: 'Quiet supporting context such as a unit, date range, method note, or source caveat.',
		props: z.object({ content: z.string().min(1).max(2000) }).strict()
	},
	Badge: {
		description: 'A compact label for a category or status; use tone sparingly.',
		props: z.object({ content: label, tone: z.enum(['neutral', 'positive', 'caution', 'negative', 'accent']).nullable().optional() }).strict()
	},
	Callout: {
		description: 'An emphasized interpretation or caveat. Use for meaningful context, not routine decoration.',
		props: z.object({ title: label.nullable().optional(), content: z.string().min(1).max(4000), tone: z.enum(['neutral', 'info', 'positive', 'caution']).nullable().optional() }).strict()
	},
	Metric: {
		description: 'A prominent exact value. Never copy a data value into this component as a literal. Bind an exact value to a saved result row (resultId, field, row); use value only for an expression derived from @Rows("result-id"). Literal values are rejected so the displayed number always retains evidence provenance.',
		props: z.object({ resultId: resultId.nullable().optional(), field: label.nullable().optional(), row: z.number().int().min(0).nullable().optional(), title: label, unit: z.string().max(40).nullable().optional(), value: z.unknown().optional(), format: z.enum(['number', 'currency', 'percent', 'compact']).nullable().optional(), detail: z.string().max(1000).nullable().optional() }).strict()
	},
	Select: {
		description: 'A local selector. Bind value to a declared $variable; changes update every expression that reads it without a model or analytics call.',
		props: z.object({ name: label, options: selectOptions, value: z.string().nullable() }).strict()
	},
	SegmentedControl: {
		description: 'A compact single-choice control for a few mutually exclusive display options. Bind value to a declared $variable.',
		props: z.object({ name: label, options: selectOptions.min(2).max(6), value: z.string().nullable() }).strict()
	},
	Checkbox: {
		description: 'A local display toggle. Bind checked to a declared $variable; it does not change analytical definitions.',
		props: z.object({ label, checked: z.boolean() }).strict()
	},
	Button: {
		description: 'A deliberate user action such as setting local presentation state or asking Fella a follow-up. No automatic action runs.',
		props: z.object({ label, action: z.unknown().optional() }).strict()
	},
	Table: {
		description: 'A table bound to saved result rows. An optional rows expression may filter, sort, or summarize those rows locally; exact source values remain accessible.',
		props: z.object({ resultId, fields: z.array(label).min(1).max(100).nullable().optional(), rows: z.unknown().optional(), title: label.nullable().optional() }).strict()
	},
	Chart: {
		description: 'A flexible Vega-Lite visualization: layers, facets, concatenation, encodings, marks and style. Optional rows are a local view derived only from the bound saved result. No authored data, transforms, URL loaders or executable expressions.',
		props: z.object({ resultId, spec: z.record(z.string(), z.unknown()), title: label.nullable().optional(), rows: z.unknown().optional() }).strict()
	},
	Progress: {
		description: 'A compact progress or share indicator with an optional numeric display value.',
		props: z.object({ title: label, value: z.unknown(), detail: z.string().max(1000).nullable().optional() }).strict()
	},
	Tabs: {
		description: 'Local presentation tabs. Stable name, labels and equally sized children arrays. No queries are run.',
		props: z.object({ name: label, labels: z.array(label).min(1).max(12), children }).strict()
	},
	ResultFilter: {
		description: 'A local exact-match selector over a bound result. Filters only saved rows, never the source dataset; visible scope warning on incomplete results. Fields cannot change analytical definitions.',
		props: z.object({ resultId, field: label, name: label, title: label.nullable().optional() }).strict()
	},
	FollowUp: {
		description: 'Explicit user-pressed follow-up request. Emits a host callback only; no tool, code, URL or IPC execution.',
		props: z.object({ label, message: z.string().min(1).max(2000) }).strict()
	}
} as const;

// OpenUI's schema-aware evaluator preserves these state bindings as ReactiveAssign
// values. Native Fella controls write the resulting value back to the local store.
markReactive(componentDefinitions.Select.props.shape.value);
markReactive(componentDefinitions.SegmentedControl.props.shape.value);
markReactive(componentDefinitions.Checkbox.props.shape.checked);

export type ComponentName = keyof typeof componentDefinitions;
