import { componentDefinitions, type ComponentName } from './schemas';
import { presentationCatalog } from './library';
import { requireResult, validateResults } from './results';
import { validateChartSpec } from './chart';
import { PRESENTATION_LIMITS as limits, type BoundResult, type PresentationIssue, type PresentationSpec } from './types';

export interface PresentationValidation {
	valid: boolean;
	spec: PresentationSpec | null;
	issues: PresentationIssue[];
	resultIds: string[];
}

type ElementRecord = { type: string; props: Record<string, unknown>; children: string[]; visible?: unknown; repeat?: unknown };
type SpecRecord = { root: string; elements: Record<string, ElementRecord> };
const unsafeKeys = new Set(['__proto__', 'prototype', 'constructor']);

function inspectJson(value: unknown, path: string, depth = 0, count = { value: 0 }): void {
	if (++count.value > limits.chartNodes || depth > limits.depth * 2) throw new Error(`${path} exceeds the JSON complexity budget.`);
	if (typeof value === 'string' && value.length > limits.text) throw new Error(`${path} contains an overlong string.`);
	if (typeof value === 'number' && !Number.isFinite(value)) throw new Error(`${path} contains a non-finite number.`);
	if (Array.isArray(value)) {
		if (value.length > limits.components * 8) throw new Error(`${path} exceeds the array budget.`);
		value.forEach((item, index) => inspectJson(item, `${path}[${index}]`, depth + 1, count));
	} else if (value && typeof value === 'object') {
		for (const [key, item] of Object.entries(value)) {
			if (unsafeKeys.has(key)) throw new Error(`${path} contains an unsafe property.`);
			inspectJson(item, `${path}.${key}`, depth + 1, count);
		}
	} else if (value !== null && !['string', 'number', 'boolean', 'undefined'].includes(typeof value)) {
		throw new Error(`${path} must contain JSON values only.`);
	}
}

function validateBindings(spec: SpecRecord, results: BoundResult[]): string[] {
	const resultIds = new Set<string>();
	const controlKeys = new Set<string>();
	for (const [key, element] of Object.entries(spec.elements)) {
		const path = `elements.${key}`;
		const props = element.props;
		if (element.type === 'Document' && key !== spec.root) throw new Error(`${path}: Document may only be the root element.`);
		if (element.type === 'TabPanel') {
			const parent = Object.values(spec.elements).find((candidate) => candidate.children.includes(key));
			if (parent?.type !== 'Tabs') throw new Error(`${path}: TabPanel must belong to Tabs.`);
		}
		if (['Select', 'SegmentedControl', 'Checkbox', 'Tabs', 'ResultFilter'].includes(element.type)) {
			const name = String(props.name ?? props.stateKey ?? props.label ?? '');
			if (!name || controlKeys.has(name)) throw new Error(`${path}: presentation control names must be unique.`);
			controlKeys.add(name);
		}
		if (element.type === 'Metric' || element.type === 'Progress') {
			const result = requireResult(results, String(props.resultId));
			resultIds.add(result.id);
			const field = String(props.field);
			const row = Number(props.row);
			if (!result.columns.includes(field) || !Number.isInteger(row) || row < 0 || row >= result.rows.length) throw new Error(`${path}: exact result field or row is unavailable.`);
			if (element.type === 'Progress') {
				const column = result.columns.indexOf(field);
				if (typeof result.rows[row][column] !== 'number') throw new Error(`${path}: Progress must bind to a numeric saved value.`);
			}
		}
		if (element.type === 'Table' || element.type === 'Chart' || element.type === 'ResultFilter') {
			const result = requireResult(results, String(props.resultId));
			resultIds.add(result.id);
			if (element.type === 'Table' && Array.isArray(props.fields) && props.fields.some((field) => typeof field !== 'string' || !result.columns.includes(field))) throw new Error(`${path}: a table field is not present in the saved result.`);
			if ((element.type === 'Table' || element.type === 'Chart') && Array.isArray(props.linkBy) && props.linkBy.some((field) => typeof field !== 'string' || !result.columns.includes(field))) throw new Error(`${path}: a linked filter field is not present in the saved result.`);
			if (element.type === 'Table' && Array.isArray(props.linkBy)) {
				const visibleFields = Array.isArray(props.fields) ? props.fields : result.columns;
				if (props.linkBy.some((field) => !visibleFields.includes(field))) throw new Error(`${path}: linked table fields must be visible in the table.`);
			}
			if (element.type === 'Table' && props.columnFormats && Object.keys(props.columnFormats as object).some((field) => !result.columns.includes(field))) throw new Error(`${path}: a formatted field is not present in the saved result.`);
			if (element.type === 'ResultFilter') {
				if (!result.columns.includes(String(props.field))) throw new Error(`${path}: filter field is not present in the saved result.`);
				const index = result.columns.indexOf(String(props.field));
				if (new Set(result.rows.map((row) => row[index])).size > 200) throw new Error(`${path}: filter has too many choices; use a more focused result.`);
			}
			if (element.type === 'Chart') {
				validateChartSpec(props.spec, result);
				if (Array.isArray(props.linkBy)) {
					const encoded = new Map<string, boolean>();
					function collect(value: unknown): void {
						if (Array.isArray(value)) { value.forEach(collect); return; }
						if (!value || typeof value !== 'object') return;
						const record = value as Record<string, unknown>;
						if (typeof record.field === 'string') encoded.set(record.field, !('aggregate' in record) && !('bin' in record) && !('timeUnit' in record));
						for (const child of Object.values(record)) collect(child);
					}
					collect(props.spec);
					if (props.linkBy.some((field) => !encoded.get(String(field)))) throw new Error(`${path}: linked chart fields must be encoded as raw, unaggregated saved values.`);
				}
			}
		}
		if (element.type === 'Tabs') {
			const labels = props.labels as string[];
			if (labels.length !== element.children.length) throw new Error(`${path}: each tab label needs exactly one panel.`);
			for (const [index, childKey] of element.children.entries()) {
				const child = spec.elements[childKey];
				if (child?.type !== 'TabPanel' || child.props.index !== index) throw new Error(`${path}: tabs need matching TabPanel children in label order.`);
			}
		}
		if (element.type === 'Tabs' || element.type === 'TabPanel') continue;
		if (element.type === 'Chart') {
			const result = requireResult(results, String(props.resultId));
			const chartFields = new Set(result.columns);
			const chartSpec = props.spec as Record<string, unknown>;
			// The chart validator checks every encoding field; this check also prevents
			// arbitrary user-authored values from entering serialized chart metadata.
			inspectJson(chartSpec, `${path}.props.spec`);
			for (const field of result.columns) chartFields.add(field);
		}
	}
	return [...resultIds];
}

/** Validate json-render structure and Fella's evidence boundary before render or persistence. */
export function validatePresentation(input: unknown, results: BoundResult[]): PresentationValidation {
	const issues: PresentationIssue[] = [];
	try {
		validateResults(results);
		inspectJson(input, 'spec');
		if (new TextEncoder().encode(JSON.stringify(input)).length > limits.specBytes) throw new Error('The presentation spec exceeds its size budget.');
		const checked = presentationCatalog.validate(input);
		if (!checked.success || !checked.data) {
			const details = checked.error?.issues.map((issue) => `${issue.path.join('.')}: ${issue.message}`).slice(0, 12).join('; ');
			throw new Error(details || 'The JSON presentation does not match Fella’s component catalog.');
		}
		const spec = checked.data as unknown as SpecRecord;
		if (!spec.root || !spec.elements[spec.root]) throw new Error('The root element is missing.');
		if (spec.elements[spec.root].type !== 'Document') throw new Error('The root element must be Document.');
		const keys = Object.keys(spec.elements);
		if (!keys.length || keys.length > limits.components) throw new Error('The presentation exceeds the component budget.');
		const parents = new Map<string, string>();
		for (const [key, element] of Object.entries(spec.elements)) {
			if (!/^[A-Za-z][A-Za-z0-9_-]{0,79}$/.test(key)) throw new Error(`Element key “${key}” is invalid.`);
			if (!Object.hasOwn(componentDefinitions, element.type)) throw new Error(`elements.${key} uses an unsupported component.`);
			if (!Array.isArray(element.children)) throw new Error(`elements.${key}.children must be an array.`);
			for (const child of element.children) {
				if (typeof child !== 'string' || !spec.elements[child]) throw new Error(`elements.${key} references a missing child.`);
				if (parents.has(child)) throw new Error(`Element “${child}” is attached more than once.`);
				parents.set(child, key);
			}
			if (element.repeat !== undefined) throw new Error(`elements.${key}: data repetition is not enabled for analytical answers.`);
		}
		if (parents.has(spec.root)) throw new Error('The root element cannot have a parent.');
		const visited = new Set<string>();
		function walk(key: string, depth: number): void {
			if (depth > limits.depth) throw new Error('The presentation nesting exceeds its depth budget.');
			if (visited.has(key)) throw new Error('The presentation contains a cyclic child reference.');
			visited.add(key);
			for (const child of spec.elements[key].children) walk(child, depth + 1);
		}
		walk(spec.root, 0);
		if (visited.size !== keys.length) throw new Error('Every presentation element must be reachable from the root.');
		const resultIds = validateBindings(spec, results);
		return { valid: true, spec: checked.data as unknown as PresentationSpec, issues, resultIds };
	} catch (error) {
		issues.push({ path: 'root', message: error instanceof Error ? error.message : 'The presentation could not be validated.' });
		return { valid: false, spec: null, issues, resultIds: [] };
	}
}
