import { createParser, parseExpression, split, tokenize, type ASTNode, type ElementNode } from '@openuidev/lang-core';
import { componentDefinitions, type ComponentName } from './schemas';
import { presentationSchema } from './manifest';
import { requireResult, validateResults } from './results';
import { validateChartSpec } from './chart';
import { PRESENTATION_LIMITS as limits, type BoundResult, type PresentationIssue } from './types';

export interface PresentationValidation {
	valid: boolean;
	root: ElementNode | null;
	issues: PresentationIssue[];
	resultIds: string[];
}

/** Resource preflight only; grammar and references are parsed by OpenUI itself. */
function preflight(program: string) {
	if (typeof program !== 'string' || !program.trim()) throw new Error('The presentation is empty.');
	if (new TextEncoder().encode(program).length > limits.programBytes) throw new Error('The presentation exceeds its size budget.');
	let quote = '';
	let escaped = false;
	let depth = 0;
	for (const char of program) {
		if (quote) {
			if (escaped) escaped = false;
			else if (char === '\\') escaped = true;
			else if (char === quote) quote = '';
		} else if (char === '"' || char === "'") quote = char;
		else if ('([{'.includes(char)) { if (++depth > limits.depth * 3) throw new Error('The presentation nesting is too deep.'); }
		else if (')]}'.includes(char)) depth--;
	}
	if (quote || depth !== 0) throw new Error('The presentation is incomplete.');
	// The upstream parser intentionally skips invalid lines/trailing tokens for streaming.
	// Validate its own token/AST output as a complete static program before materialization.
	const tokens = tokenize(program);
	const statements = split(tokens);
	const ids = new Set<string>();
	function staticTokenCount(node: ASTNode): number {
		switch (node.k) {
			case 'Str': case 'Num': case 'Bool': case 'Null': case 'Ref': return 1;
			case 'Comp':
				if (!Object.hasOwn(componentDefinitions, node.name)) throw new Error(`Unsupported component or builtin “${node.name}”.`);
				return 3 + Math.max(0, node.args.length - 1) + node.args.reduce((sum, arg) => sum + staticTokenCount(arg), 0);
			case 'Arr': return 2 + Math.max(0, node.els.length - 1) + node.els.reduce((sum, arg) => sum + staticTokenCount(arg), 0);
			case 'Obj': {
				const keys = node.entries.map(([key]) => key);
				if (new Set(keys).size !== keys.length || keys.some((key) => ['__proto__', 'constructor', 'prototype'].includes(key))) throw new Error('Duplicate or unsafe object keys are not allowed.');
				return 2 + Math.max(0, node.entries.length - 1) + node.entries.reduce((sum, [, arg]) => sum + 2 + staticTokenCount(arg), 0);
			}
			// Vega-Lite negative offsets are parsed as unary numeric literals by OpenUI.
			case 'UnaryOp': if (node.op === '-' && node.operand.k === 'Num') return 2; else break;
		}
		throw new Error('Presentation prop expressions, builtins and executable plans are not supported.');
	}
	let accounted = 0;
	for (const statement of statements) {
		if (ids.has(statement.id) || statement.id.startsWith('$')) throw new Error('Duplicate statements and reactive variables are not allowed.');
		ids.add(statement.id);
		// Token enum values are supplied by the pinned core API (not exported at runtime).
		// 7=comma, 2/4/6=closing delimiters. Ignore legal trailing commas in count checks.
		const expression = statement.tokens.filter((token, index, list) => !(token.t === 7 && [2, 4, 6].includes(list[index + 1]?.t)));
		if (staticTokenCount(parseExpression(expression)) !== expression.length) throw new Error('The presentation contains unparsed or malformed expression tokens.');
		accounted += statement.tokens.length + 2;
	}
	if (tokens.filter((token) => token.t !== 0 && token.t !== 13).length !== accounted) throw new Error('The presentation contains an invalid statement.');
}

/** Complete programs only. During streaming retain the previous valid frame/fallback. */
export function validatePresentation(program: string, results: BoundResult[]): PresentationValidation {
	const issues: PresentationIssue[] = [];
	const resultIds = new Set<string>();
	try {
		preflight(program);
		validateResults(results);
		const parsed = createParser(presentationSchema()).parse(program);
		if (parsed.meta.incomplete || parsed.meta.unresolved.length) throw new Error(`The presentation is incomplete or has unresolved references${parsed.meta.unresolved.length ? `: ${parsed.meta.unresolved.join(', ')}` : '.'}`);
		if (parsed.meta.statementCount > limits.components || parsed.meta.orphaned.length) throw new Error('The presentation contains unused statements or exceeds the statement budget.');
		if (parsed.meta.errors.length) {
			for (const error of parsed.meta.errors) issues.push({ path: `${error.component}${error.path}`, message: error.message });
			return { valid: false, root: null, issues, resultIds: [] };
		}
		if (parsed.queryStatements.length || parsed.mutationStatements.length || Object.keys(parsed.stateDeclarations).length) throw new Error('Queries, mutations and reactive state plans are not permitted in presentations.');
		if (!parsed.root || parsed.root.typeName !== 'Document' || parsed.root.statementId !== 'root') throw new Error('The presentation must have an explicit root = Document(...) statement.');
		let count = 0;
		const controls = new Set<string>();
		const ancestry = new Set<unknown>();
		function walk(value: unknown, path: string, depth: number): void {
			if (++count > limits.components || depth > limits.depth) throw new Error('The presentation exceeds its component budget.');
			if (!value || typeof value !== 'object' || ancestry.has(value)) throw new Error(`${path} must be a component, not a literal or cyclic reference.`);
			ancestry.add(value);
			const node = value as ElementNode;
			if (node.type !== 'element' || node.partial || !Object.hasOwn(componentDefinitions, node.typeName)) throw new Error(`${path} uses an unknown, partial or unsupported component.`);
			const definition = componentDefinitions[node.typeName as ComponentName];
			const check = definition.props.safeParse(node.props);
			if (!check.success) throw new Error(`${path}: ${check.error.issues.map((item) => item.message).join('; ')}`);
			const props = check.data as Record<string, unknown>;
			if (node.typeName === 'Document' && depth > 0) throw new Error('Document may only be the root.');
			if ('resultId' in props) {
				const result = requireResult(results, String(props.resultId));
				resultIds.add(result.id);
				if ('field' in props && !result.columns.includes(String(props.field))) throw new Error(`${path} references a missing saved field.`);
				if ('fields' in props && (props.fields as string[]).some((field) => !result.columns.includes(field))) throw new Error(`${path} references a missing table field.`);
				if (node.typeName === 'Metric' && Number(props.row) >= result.rows.length) throw new Error(`${path} references a missing saved row.`);
				if (node.typeName === 'Chart') validateChartSpec(props.spec, result);
				if (node.typeName === 'ResultFilter') {
					const index = result.columns.indexOf(String(props.field));
					if (new Set(result.rows.map((row) => row[index])).size > 200) throw new Error('A display filter has too many choices; use a more focused result.');
				}
			}
			if (node.typeName === 'Tabs' || node.typeName === 'ResultFilter') {
				if (controls.has(String(props.name))) throw new Error('Presentation control names must be unique.');
				controls.add(String(props.name));
			}
			if (node.typeName === 'Tabs' && (props.labels as unknown[]).length !== (props.children as unknown[]).length) throw new Error('Each tab needs one label and one child.');
			if ('children' in props) (props.children as unknown[]).forEach((child, index) => walk(child, `${path}.children[${index}]`, depth + 1));
			ancestry.delete(value);
		}
		walk(parsed.root, 'root', 0);
		return { valid: true, root: parsed.root, issues, resultIds: [...resultIds] };
	} catch (error) {
		issues.push({ path: 'root', message: error instanceof Error ? error.message : 'The presentation could not be validated.' });
		return { valid: false, root: null, issues, resultIds: [...resultIds] };
	}
}
