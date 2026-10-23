import { z } from 'zod/v4';
import { presentationCatalog } from './library';
import { componentDefinitions } from './schemas';
import { PRESENTATION_VERSION, type BoundResult } from './types';

export const PRESENTATION_DESIGN_GUIDANCE = `Choose an answer-specific visual presentation from the question, verified answer, and supplied result shape. The model chooses the composition: concise prose, exact metrics, tables, charts, comparisons, or a useful combination. Do not repeat one layout for every question. Use a visualization when it makes a trend, ranking, distribution, relationship, or comparison easier to understand; a table is best for exact records, lookups, and detail. Keep the answer readable, responsive, and free of redundant decoration.

Choose chart forms by the analytical task, not by a default template: when the user asks for a trend or change across three or more ordered time periods and the result has a numeric measure, make a chart the primary view unless that would mislead; line charts make observed values easy to compare, while area charts emphasize continuous volume or accumulated magnitude. Bars compare categories or rank values (use horizontal bars for long labels); pie or donut charts show a small, meaningful part-to-whole breakdown, not time trends or long rankings; points show relationships or distributions. Group, stack, facet, or layer only when it clarifies a real comparison and the scales remain interpretable. Other safe Vega-Lite compositions are welcome when they better fit the result. Use a tooltip for useful exact values, a legend only when it helps decode multiple series, and axis titles or captions for units and scope. Keep labels readable and avoid unnecessary precision. Do not imitate unsupported chart forms with custom executable expressions.

Use Fella's registered components and design. Prefer a clear reading order, consistent spacing and type hierarchy, restrained surfaces, accessible labels, the Fella chart-color palette, and responsive layouts. Avoid cramped equal-weight cards, horizontal overflow, decorative controls, and repeated content. Keep caveats near affected values. Never imply completeness when a supplied result is incomplete.`;

export const PRESENTATION_RULES = [
	'Output one JSON object matching the supplied schema: {"root":"element-key","elements":{"element-key":{"type":"Component","props":{},"children":[]}}}. The root element must be Document. Children are references to other element keys, never nested objects. Every referenced key must exist, every key must be reachable from root, and keys must be unique.',
	'Compose an answer for this question; the component catalog is a vocabulary, not a template. Select a presentation based on the actual question and results. Do not use a table merely because analysis returned rows. Use charts when they reveal a meaningful pattern, change, ranking, distribution, or relationship.',
	'Analytical numbers must remain bound to the supplied saved result. Metric and Progress require exact resultId, field, and zero-based row references. Table and Chart require a supplied resultId. Never invent result IDs or fields, author replacement datasets, or use copied numeric values as analytical facts in Metric, Progress, chart data, or labels.',
	'Chart.spec is a flexible Vega-Lite presentation of the bound result. Bind every encoding—including tooltip fields—to columns in that result. Choose an appropriate chart form and composition for the question; do not default to a table or repeat one chart form. Do not include data, datasets, URL loaders, JavaScript, signal expressions, or executable transforms.',
	'Table fields must exist in the bound result. Numeric formats change display only; exact saved values and source evidence remain authoritative. For incomplete results, keep the limitation visible and never imply full-dataset coverage.',
	'Controls are presentation-only. Give each control a stable stateKey; changing it may only alter the visible presentation. ResultFilter only filters loaded rows from its declared result. FollowUp and Button follow_up actions are explicit user-clicked requests; they cannot run tools, queries, code, IPC, or external navigation.',
	'Use only registered component names and documented props. Do not add arbitrary styles, HTML, scripts, URLs, unknown fields, or hidden behavior. Each element must include type, props, and children; leaves use children: [].',
	'Use Tabs with one TabPanel child per label, in the same order, with matching zero-based indices. Keep labels concise. Avoid unnecessary tabs, dividers, spacers, cards, and duplicate representations.',
	PRESENTATION_DESIGN_GUIDANCE
];

export function presentationSchema(): object {
	return presentationCatalog.jsonSchema();
}

export function presentationComponentSchemas(): Record<string, object> {
	return Object.fromEntries(Object.entries(componentDefinitions).map(([name, definition]) => [name, z.toJSONSchema(definition.props)]));
}

export function presentationPrompt(results: Pick<BoundResult, 'id' | 'columns' | 'row_count' | 'complete'>[] = []): string {
	const componentSchemas = presentationComponentSchemas();
	const components = Object.entries(componentDefinitions).map(([name, definition]) => {
		return `${name}: ${definition.description}\nProps schema: ${JSON.stringify(componentSchemas[name])}`;
	});
	return [
		`Fella analytical answer output format: json-render spec ${PRESENTATION_VERSION}.`,
		...PRESENTATION_RULES,
		'', 'Registered components:', ...components,
		'', 'Available result bindings (metadata only; the result rows themselves are provided separately as evidence):',
		JSON.stringify(results)
	].join('\n');
}

export function presentationManifest() {
	const schema = presentationSchema();
	return {
		version: PRESENTATION_VERSION,
		componentNames: Object.keys(componentDefinitions),
		schema,
		componentSchemas: presentationComponentSchemas(),
		prompt: presentationPrompt(),
		designGuidance: PRESENTATION_DESIGN_GUIDANCE,
		rules: PRESENTATION_RULES
	};
}
