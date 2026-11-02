import { z } from 'zod/v4';

const key = z.string().regex(/^[A-Za-z][A-Za-z0-9_.-]{0,79}$/);
const option = z.strictObject({ value: z.string().max(240), label: z.string().min(1).max(240) });
export const stateDeclarationSchema = z.discriminatedUnion('type', [
	z.strictObject({ type: z.literal('enum'), options: z.array(option).min(1).max(200), default: z.string().max(240), persistence: z.literal('viewer') }),
	z.strictObject({ type: z.literal('boolean'), default: z.boolean(), persistence: z.literal('viewer') })
]);
export const contractMetadataSchema = z.strictObject({
	version: z.literal('fella-ui-1'),
	capabilities: z.array(z.enum(['display', 'follow_up'])).min(1).max(2),
	state: z.record(key, stateDeclarationSchema),
	intent: z.enum(['lookup', 'comparison', 'trend', 'distribution', 'relationship', 'exploration', 'explanation']).optional()
});

type Schema = Record<string, any>;
const stateReference = { type: 'string', pattern: '^[A-Za-z][A-Za-z0-9_.-]{0,79}$' };
const derivedProps: Record<string, string[]> = {
	Document: ['contract'], Select: ['stateKey', 'options', 'defaultValue'],
	SegmentedControl: ['stateKey', 'options', 'defaultValue'], Checkbox: ['stateKey', 'defaultChecked'],
	Tabs: ['labels', 'name'], TabPanel: ['name', 'index'], Chart: ['filters'], Table: ['filters']
};

/** Derive author props from the renderer catalog; omit mechanical bookkeeping. */
export function authoringSchema(components: Record<string, object>, slots: Record<string, readonly string[]>): object {
	const defs: Schema = {};
	// New authoring has one state/binding system. Keep ResultFilter in the
	// canonical registry for saved layouts, rather than advertise its separate
	// legacy filter state alongside declared controls.
	const variants = Object.entries(components).filter(([name]) => name !== 'ResultFilter').map(([name, original]) => {
		const props = structuredClone(original) as Schema;
		const derived = derivedProps[name] ?? [];
		for (const field of derived) delete props.properties?.[field];
		if (props.required) props.required = props.required.filter((field: string) => !derived.includes(field));
		if (name === 'TabPanel') {
			props.properties.label = { type: 'string', minLength: 1, maxLength: 240 };
			props.required = [...(props.required ?? []), 'label'];
		}
		// Each component's generated definitions become local to its props branch.
		function scope(value: any): any {
			if (Array.isArray(value)) return value.map(scope);
			if (!value || typeof value !== 'object') return value;
			return Object.fromEntries(Object.entries(value).map(([key, child]) => [key,
				key === '$ref' && typeof child === 'string' && child.startsWith('#/')
					? `#/$defs/props_${name}/${child.slice(2)}` : scope(child)
			]));
		}
		defs[`props_${name}`] = scope(props);
		const properties: Schema = {
			type: { const: name },
			id: { type: 'string', pattern: '^[A-Za-z][A-Za-z0-9_-]{0,79}$' },
			props: { $ref: `#/$defs/props_${name}` },
			children: slots[name]?.length
				? { type: 'array', maxItems: 240, items: { $ref: '#/$defs/node' } }
				: { type: 'array', maxItems: 0 },
			when: { type: 'object', properties: { state: stateReference, equals: { anyOf: [{ type: 'string' }, { type: 'boolean' }] } }, required: ['state', 'equals'], additionalProperties: false }
		};
		const required = ['type'];
		if (['Select', 'SegmentedControl', 'Checkbox'].includes(name)) {
			properties.bind = { type: 'object', properties: { state: stateReference }, required: ['state'], additionalProperties: false };
			required.push('bind');
		}
		if (['Chart', 'Table'].includes(name)) properties.filters = {
			type: 'array', minItems: 1, maxItems: 8,
			items: { type: 'object', properties: { state: stateReference, field: { type: 'string', minLength: 1, maxLength: 240 } }, required: ['state', 'field'], additionalProperties: false }
		};
		if (props.required?.length) required.push('props');
		return { type: 'object', properties, required, additionalProperties: false };
	});
	defs.node = { anyOf: variants };
	defs.document = variants.find(variant => variant.properties.type.const === 'Document');
	const metadata = z.toJSONSchema(contractMetadataSchema) as Schema;
	return {
		type: 'object', properties: {
			version: { const: 'fella-ui-1' }, intent: metadata.properties.intent,
			capabilities: { ...metadata.properties.capabilities, uniqueItems: true },
			state: metadata.properties.state, root: { $ref: '#/$defs/document' }
		}, required: ['version', 'root'], additionalProperties: false, $defs: defs
	};
}
