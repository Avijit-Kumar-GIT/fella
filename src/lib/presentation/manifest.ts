import { z } from 'zod/v4';
import { presentationCatalog } from './catalog';
import { componentDefinitions } from './schemas';
import { authoringSchema } from './authoring';
import { PRESENTATION_LIMITS, PRESENTATION_VERSION, type BoundResult } from './types';

export const PRESENTATION_DESIGN_GUIDANCE = `Choose an answer-specific visual presentation from the question, verified answer, and supplied result shape. The model chooses the composition: concise prose, exact metrics, tables, charts, comparisons, or a useful combination. Do not repeat one layout for every question. Do not use a table merely because analysis returned rows. Use a visualization when it makes a trend, ranking, distribution, relationship, or comparison easier to understand; a table is best for exact records, lookups, and detail. Keep the answer readable, responsive, and free of redundant decoration.

Choose chart forms by the analytical task, not by a default template: when the user asks for a trend or change across three or more ordered time periods and the result has a numeric measure, make a chart the primary view unless that would mislead; line charts make observed values easy to compare, while area charts emphasize continuous volume or accumulated magnitude. Bars compare categories or rank values (use horizontal bars for long labels); pie or donut charts show a small, meaningful part-to-whole breakdown, not time trends or long rankings; points show relationships or distributions. Group, stack, facet, or layer only when it clarifies a real comparison and the scales remain interpretable. Other safe Vega-Lite compositions are welcome when they better fit the result. Use a tooltip for useful exact values, a legend only when it helps decode multiple series, and axis titles or captions for units and scope. Keep labels readable and avoid unnecessary precision. Do not imitate unsupported chart forms with custom executable expressions.

Use Fella's registered components and design. Prefer a clear reading order, consistent spacing and type hierarchy, restrained surfaces, accessible labels, the Fella chart-color palette, and responsive layouts. Avoid cramped equal-weight cards, horizontal overflow, decorative controls, and repeated content. Keep caveats near affected values. Never imply completeness when a supplied result is incomplete.`;

export const PRESENTATION_RULES = [
 'The value of presentation_spec is a nested tree: {"version":"fella-ui-1","intent":"trend","capabilities":["display"],"state":{},"root":{"type":"Document","children":[{"type":"Title","props":{"content":"Answer"}}]}}. Keep the response envelope specified by the caller. The host generates element IDs, child references and empty leaf children. Use optional explicit id only to preserve identity across revisions.',
 'Intent describes the user goal (lookup, comparison, trend, distribution, relationship, exploration, explanation), not a layout template. Choose components that clarify this question. A plain canonical answer without a presentation is valid; never add decorative interactivity.',
 'Every analytical value binds to a supplied saved resultId and field. Metric and Progress also use a zero-based row. Chart encodings and Table fields reference only saved columns. No datasets, queries, code, network access, executable expressions or arbitrary chart transforms. Chart.spec must match its full typed schema, including supported layer, facet, repeat and concat forms.',
 'For local controls declare state: {"view":{"type":"enum","options":[{"value":"chart","label":"Chart"},{"value":"table","label":"Exact values"}],"default":"chart","persistence":"viewer"}}. Boolean state uses type:boolean, default:boolean, persistence:viewer. Bind Select, SegmentedControl or Checkbox using bind:{state:"view"}; the host derives stateKey, options and defaults.',
 'Every state must have a producer and a visible or filtering consumer. Conditional nodes use when:{state:"view",equals:"chart"}. Chart/Table filtering uses filters:[{state:"region",field:"region"}]. Filter enum values must match exact saved column values; the explicit $all choice means all saved rows. Filters are local saved-row display operations, not SQL or a model call. Defaults must be valid, modest, visible and reversible. Selections are viewer state; saved defaults and export snapshots retain explicit state.',
 'Honor requested exploration when saved fields support it: provide real controls with consumers, rather than describing possible filters in prose or listing every selection. A shared filter state can drive both a chart and an exact-values table, with a separate view state controlling disclosure. Preserve the user\'s analytical scope and previous selections when revising a saved view. If required dimensions or measures are missing, the analytical runtime must gather them first; a display filter cannot invent a missing breakdown.',
 'Put bind, when, filters and children on the authored node, outside props. For a field selector use Select with props:{name:"Group"}, bind:{state:"group"}, and an enum declaration containing actual saved field choices; consumers use filters:[{state:"group",field:"the_saved_field"}]. For chart/table switching, use a separate SegmentedControl state with when on the Chart and Table nodes; keep the field filter on both consumers. Do not invent another control-state mechanism. Table columnFormats values are objects, for example {"amount":{"format":"currency","unit":"USD","decimals":2}}, not format-name strings. Row.gap is a number; Spacer.size is sm, md or lg.',
 'Tabs need only TabPanel children with props.label and nested children; an explicit Tabs id preserves selection identity. Do not author labels arrays, panel names or indices; the host derives them. Keep tabs useful and accessible.',
 'The display capability permits presentation and local state only. Declare follow_up as well only when including FollowUp or Button follow_up. Those actions require an explicit user click and remain subject to host policy. Scenario, query, code and network capabilities are not implemented; do not request them or imitate hypothetical values as observed evidence.',
 'Use only registered components, props, bindings and typed chart settings. Host defaults provide Fella styling, spacing, sizing and routine formatting. Preserve coverage limitations and source caveats. Existing flat root/elements presentations are an internal compatibility format, not the preferred authoring language.',
 `Shared resource limits: ${JSON.stringify(PRESENTATION_LIMITS)}. Chart strings cannot contain URL/resource schemes, url(...), or script/svg/iframe markup. Numeric format strings allow at most four consecutive digits to bound label allocations. Facet/repeat panels, composed pixels and row/view work are cumulative conservative budgets. Linked chart fields must be encoded as raw unaggregated values. Saved-row options must exist in their bound result. These semantic constraints are checked before acceptance.`,
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
	const author = authoringSchema(componentSchemas, Object.fromEntries(Object.entries(componentDefinitions).map(([name, definition]) => [name, definition.slots]))) as any;
	const components = Object.entries(componentDefinitions).filter(([name]) => author.$defs[`props_${name}`]).map(([name, definition]) => {
		const props = structuredClone(author.$defs[`props_${name}`]);
		delete props.$schema; delete props.$defs;
		if (name === 'Chart') props.properties.spec = { description: 'The typed ChartProgram supplied in the response schema; bind only saved fields.' };
		return `${name}: ${definition.description}\nAuthor props: ${JSON.stringify(props)}. Full chart types are in the supplied response schema.`;
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
		authorSchema: authoringSchema(presentationComponentSchemas(), Object.fromEntries(Object.entries(componentDefinitions).map(([name,def])=>[name,def.slots]))),
		slots: Object.fromEntries(Object.entries(componentDefinitions).map(([name, definition]) => [name, definition.slots])),
		limits: PRESENTATION_LIMITS,
		capabilities: { display: true, follow_up: true, scenario: false, query: false, code: false, network: false },
		schema,
		componentSchemas: presentationComponentSchemas(),
		prompt: presentationPrompt(),
		designGuidance: PRESENTATION_DESIGN_GUIDANCE,
		rules: PRESENTATION_RULES
	};
}
