import { z } from 'zod/v4';
import { PRESENTATION_LIMITS as limits } from './types';

const label = z.string().min(1).max(240);
const resultId = z.string().min(1).max(240);
const interactionGroup = z.string().regex(/^[A-Za-z][A-Za-z0-9_.-]{0,79}$/);
const content = z.string().min(1).max(limits.text);
const selectOptions = z.array(z.object({ value: z.string().max(240), label: z.string().max(240) }).strict()).min(1).max(200);
const columnFormat = z.object({ label: label.optional(), format: z.enum(['number', 'currency', 'percent', 'compact']).optional(), unit: z.string().max(40).optional(), decimals: z.number().int().min(0).max(8).optional() }).strict();

export type CardTone = 'plain' | 'quiet' | 'accent';
export const CARD_TONE_ALIASES: Readonly<Record<string, CardTone>> = Object.freeze({
	plain: 'plain', default: 'plain', normal: 'plain', neutral: 'plain', outline: 'plain', outlined: 'plain', ghost: 'plain', transparent: 'plain',
	quiet: 'quiet', soft: 'quiet', subtle: 'quiet', muted: 'quiet', secondary: 'quiet', positive: 'quiet', negative: 'quiet', success: 'quiet', warning: 'quiet', caution: 'quiet', danger: 'quiet',
	accent: 'accent', primary: 'accent', highlight: 'accent', highlighted: 'accent', emphasis: 'accent', info: 'accent'
});

export function normalizeCardTone(value: unknown): CardTone {
	if (typeof value !== 'string') return 'plain';
	const key = value.trim().toLowerCase();
	return Object.hasOwn(CARD_TONE_ALIASES, key) ? CARD_TONE_ALIASES[key] : 'plain';
}

/** Fella's native json-render catalog. Components receive structure as child keys, not props. */
export const componentDefinitions = {
	Document: {
		description: 'The root analytical answer. Organize it into a useful reading order.',
		props: z.object({}).strict(), slots: ['default'] as const
	},
	Card: {
		description: 'A restrained surface grouping related information. Use sparingly, not around every element.',
		props: z.object({ title: label.optional(), detail: z.string().max(1000).optional(), tone: z.enum(['plain', 'quiet', 'accent']).optional() }).strict(), slots: ['default'] as const
	},
	Section: {
		description: 'A titled, left-aligned group of analytical content.',
		props: z.object({ title: label }).strict(), slots: ['default'] as const
	},
	Columns: {
		description: 'A responsive group of at most three content columns; collapses at narrow widths.',
		props: z.object({ count: z.number().int().min(1).max(3).optional() }).strict(), slots: ['default'] as const
	},
	Row: {
		description: 'A flexible horizontal group that wraps at narrow widths.',
		props: z.object({ gap: z.number().min(0).max(8).optional(), align: z.enum(['start', 'center', 'end', 'stretch']).optional() }).strict(), slots: ['default'] as const
	},
	Grid: {
		description: 'A responsive grid for a small set of related components.',
		props: z.object({ columns: z.number().int().min(1).max(4).optional() }).strict(), slots: ['default'] as const
	},
	Divider: {
		description: 'A quiet separator between major content groups.',
		props: z.object({ label: z.string().max(240).optional() }).strict(), slots: [] as const
	},
	Spacer: {
		description: 'A small amount of intentional breathing room.',
		props: z.object({ size: z.enum(['sm', 'md', 'lg']).optional() }).strict(), slots: [] as const
	},
	Title: {
		description: 'A clear heading for the answer or one of its major sections.',
		props: z.object({ content: label, level: z.number().int().min(1).max(4).optional() }).strict(), slots: [] as const
	},
	Text: {
		description: 'Explanatory prose with Markdown emphasis, lists, and links. Use structured Table and Chart components for analytical results.',
		props: z.object({ content }).strict(), slots: [] as const
	},
	Caption: {
		description: 'Quiet supporting context such as units, date range, source, or caveat.',
		props: z.object({ content: z.string().min(1).max(2000) }).strict(), slots: [] as const
	},
	Badge: {
		description: 'A compact label for a category or meaningful status.',
		props: z.object({ content: label, tone: z.enum(['neutral', 'positive', 'caution', 'negative', 'accent']).optional() }).strict(), slots: [] as const
	},
	Callout: {
		description: 'An emphasized interpretation or caveat; not routine decoration.',
		props: z.object({ title: label.optional(), content: z.string().min(1).max(4000), tone: z.enum(['neutral', 'info', 'positive', 'caution']).optional() }).strict(), slots: [] as const
	},
	Metric: {
		description: 'A prominent exact value read from a supplied saved result. Always bind resultId, field and zero-based row; literal metric values are not accepted.',
		props: z.object({ resultId, field: label, row: z.number().int().min(0), title: label, unit: z.string().max(40).optional(), format: z.enum(['number', 'currency', 'percent', 'compact']).optional(), detail: z.string().max(1000).optional() }).strict(), slots: [] as const
	},
	Select: {
		description: 'A local selector that changes only the presentation state, not the question or analysis.',
		props: z.object({ name: label, options: selectOptions, stateKey: label, defaultValue: z.string().max(240).optional() }).strict(), slots: [] as const
	},
	SegmentedControl: {
		description: 'A compact single-choice local presentation control for a few mutually exclusive options.',
		props: z.object({ name: label, options: selectOptions.min(2).max(6), stateKey: label, defaultValue: z.string().max(240).optional() }).strict(), slots: [] as const
	},
	Checkbox: {
		description: 'A local display toggle; it does not change analytical definitions.',
		props: z.object({ label, stateKey: label, defaultChecked: z.boolean().optional() }).strict(), slots: [] as const
	},
	Button: {
		description: 'A deliberate user action. Its only allowed effect is to set local display state or ask Fella a follow-up.',
		props: z.object({ label, action: z.discriminatedUnion('type', [
			z.object({ type: z.literal('follow_up'), message: z.string().min(1).max(2000) }).strict(),
			z.object({ type: z.literal('set_state'), stateKey: label, value: z.union([z.string().max(256), z.number().finite(), z.boolean(), z.null()]) }).strict()
		]).optional() }).strict(), slots: [] as const
	},
	Table: {
		description: 'A result-bound table for exact records, lookups, and detailed comparisons. For linked filtering, use the same interactionGroup on compatible components and include only visible raw fields in linkBy. Do not choose a table by default when a chart better explains a pattern.',
		props: z.object({ resultId, fields: z.array(label).min(1).max(100).optional(), title: label.optional(), columnFormats: z.record(label, columnFormat).optional(), interactionGroup: interactionGroup.optional(), linkBy: z.array(label).min(1).max(8).optional() }).strict().superRefine((props, context) => {
			if (props.linkBy && !props.interactionGroup) context.addIssue({ code: 'custom', message: 'Table linkBy requires an interactionGroup.' });
		}), slots: [] as const
	},
	Chart: {
		description: 'A flexible, responsive result-bound Vega-Lite visualization. Select a useful form for the task (for example, line or area for a time trend, bar for category comparisons, arc for a small part-to-whole view, or points for relationships), and compose, facet, layer, or add a legend and result-bound tooltip when they clarify the answer. Keep axes, labels, units, and number of series readable. For chart-click filtering, list the displayed categorical fields in linkBy; use the same interactionGroup on semantically compatible charts, tables, and filters. Fella may replay an eligible saved SQL query for exact selected values inside Rust; the presentation cannot query or supply data, change source scope, or define executable transforms.',
		props: z.object({ resultId, spec: z.record(z.string(), z.unknown()), title: label.optional(), interactionGroup: interactionGroup.optional(), linkBy: z.array(label).min(1).max(8).optional() }).strict().superRefine((props, context) => {
			if (props.linkBy && !props.interactionGroup) context.addIssue({ code: 'custom', message: 'Chart linkBy requires an interactionGroup.' });
		}), slots: [] as const
	},
	Progress: {
		description: 'A compact indicator for an exact saved numeric value bound to a result row.',
		props: z.object({ resultId, field: label, row: z.number().int().min(0), title: label, maximum: z.number().positive().optional(), detail: z.string().max(1000).optional() }).strict(), slots: [] as const
	},
	Tabs: {
		description: 'A local presentation tab set. Supply one TabPanel child per label in the same order.',
		props: z.object({ name: label, labels: z.array(label).min(1).max(12) }).strict(), slots: ['default'] as const
	},
	TabPanel: {
		description: 'One child panel of a Tabs component. index must match its zero-based position.',
		props: z.object({ name: label, index: z.number().int().min(0).max(11) }).strict(), slots: ['default'] as const
	},
	ResultFilter: {
		description: 'An exact-value selector sourced from rows in a saved result. Give it an interactionGroup to link it to compatible components that expose the same field. Fella may replay an eligible read-only SQL result in Rust against the same source revision; unsupported results remain limited to their saved rows. This control never asks the model to query or changes source scope.',
		props: z.object({ resultId, field: label, name: label, title: label.optional(), interactionGroup: interactionGroup.optional() }).strict(), slots: [] as const
	},
	FollowUp: {
		description: 'An explicit user-pressed follow-up request to Fella.',
		props: z.object({ label, message: z.string().min(1).max(2000) }).strict(), slots: [] as const
	}
} as const;

export type ComponentName = keyof typeof componentDefinitions;
