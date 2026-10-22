import { createParser, parseExpression, split, tokenize, type ASTNode, type ElementNode } from '@openuidev/lang-core';
import { componentDefinitions, type ComponentName } from './schemas';
import { presentationSchema } from './manifest';
import { requireResult, validateResults } from './results';
import { validateChartSpec } from './chart';
import { PRESENTATION_LIMITS as limits, type BoundResult, type PresentationIssue } from './types';

export interface PresentationValidation {
	valid: boolean;
	root: ElementNode | null;
	normalizedProgram: string;
	issues: PresentationIssue[];
	resultIds: string[];
}

/** Resource preflight only; grammar and references are parsed by OpenUI itself. */
function preflight(program: string): string {
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
	const astById = new Map<string, ASTNode>();
	const statementTokens: { id: string; tokens: typeof statements[number]['tokens']; ast: ASTNode }[] = [];
	function tokenCount(node: ASTNode): number {
		switch (node.k) {
			case 'Str': case 'Num': case 'Bool': case 'Null': case 'Ref': case 'Ph': case 'StateRef': case 'RuntimeRef': return 1;
			case 'Comp':
				if (!Object.hasOwn(componentDefinitions, node.name)) throw new Error(`Unsupported component or builtin “${node.name}”.`);
				return 3 + Math.max(0, node.args.length - 1) + node.args.reduce((sum, arg) => sum + tokenCount(arg), 0);
			case 'Arr': return 2 + Math.max(0, node.els.length - 1) + node.els.reduce((sum, arg) => sum + tokenCount(arg), 0);
			case 'Obj': {
				const keys = node.entries.map(([key]) => key);
				if (new Set(keys).size !== keys.length || keys.some((key) => ['__proto__', 'constructor', 'prototype'].includes(key))) throw new Error('Duplicate or unsafe object keys are not allowed.');
				return 2 + Math.max(0, node.entries.length - 1) + node.entries.reduce((sum, [, arg]) => sum + 2 + tokenCount(arg), 0);
			}
			case 'BinOp': return 1 + tokenCount(node.left) + tokenCount(node.right);
			case 'UnaryOp': return 1 + tokenCount(node.operand);
			case 'Ternary': return 2 + tokenCount(node.cond) + tokenCount(node.then) + tokenCount(node.else);
			case 'Member': return 2 + tokenCount(node.obj);
			case 'Index': return 2 + tokenCount(node.obj) + tokenCount(node.index);
		}
		throw new Error('State assignments and unsupported OpenUI expressions are not allowed in presentations.');
	}
	let accounted = 0;
	for (const statement of statements) {
		if (ids.has(statement.id) || statement.id.startsWith('$')) throw new Error('Duplicate statements and reactive variables are not allowed.');
		ids.add(statement.id);
		// Token enum values are supplied by the pinned core API (not exported at runtime).
		// 7=comma, 2/4/6=closing delimiters. Ignore legal trailing commas in count checks.
		const expression = statement.tokens.filter((token, index, list) => !(token.t === 7 && [2, 4, 6].includes(list[index + 1]?.t)));
		const ast = parseExpression(expression);
		if (tokenCount(ast) !== expression.length) throw new Error('The presentation contains unparsed or malformed expression tokens.');
		astById.set(statement.id, ast);
		statementTokens.push({ id: statement.id, tokens: statement.tokens, ast });
		accounted += statement.tokens.length + 2;
	}
	if (tokens.filter((token) => token.t !== 0 && token.t !== 13).length !== accounted) throw new Error('The presentation contains an invalid statement.');

	function collectReferences(node: ASTNode, into: Set<string>): void {
		switch (node.k) {
			case 'Ref': into.add(node.n); break;
			case 'Comp': node.args.forEach((arg) => collectReferences(arg, into)); break;
			case 'Arr': node.els.forEach((item) => collectReferences(item, into)); break;
			case 'Obj': node.entries.forEach(([, item]) => collectReferences(item, into)); break;
			case 'BinOp': collectReferences(node.left, into); collectReferences(node.right, into); break;
			case 'UnaryOp': collectReferences(node.operand, into); break;
			case 'Ternary': collectReferences(node.cond, into); collectReferences(node.then, into); collectReferences(node.else, into); break;
			case 'Member': collectReferences(node.obj, into); break;
			case 'Index': collectReferences(node.obj, into); collectReferences(node.index, into); break;
			case 'Assign': collectReferences(node.value, into); break;
		}
	}
	function reachableStatements(source: Map<string, ASTNode>): Set<string> {
		const reachable = new Set<string>();
		const pending = ['root'];
		while (pending.length) {
			const id = pending.pop()!;
			if (reachable.has(id)) continue;
			reachable.add(id);
			const ast = source.get(id);
			if (!ast) continue;
			const references = new Set<string>();
			collectReferences(ast, references);
			pending.push(...references);
		}
		return reachable;
	}

	const sourceReachable = reachableStatements(astById);
	if ([...ids].some((id) => !sourceReachable.has(id))) throw new Error('The presentation contains unused statements.');
	const evaluationBudget = { remaining: 20_000 };
	const normalizedStatements = statementTokens.map(({ id, ast }) => `${id} = ${serializeStatic(ast, astById, evaluationBudget)}`);
	// Folding a local constant into an expression can make its definition redundant.
	// Drop only those now-unreferenced definitions that were reachable in the source.
	const normalizedAstById = new Map<string, ASTNode>();
	for (const { id } of statementTokens) {
		const normalized = normalizedStatements.find((line) => line.startsWith(`${id} = `));
		if (!normalized) continue;
		const normalizedTokens = split(tokenize(normalized))[0]?.tokens;
		if (normalizedTokens) normalizedAstById.set(id, parseExpression(normalizedTokens));
	}
	const normalizedReachable = reachableStatements(normalizedAstById);
	const normalized = normalizedStatements.filter((line) => normalizedReachable.has(line.slice(0, line.indexOf(' = ')))).join('\n');
	if (new TextEncoder().encode(normalized).length > limits.programBytes) throw new Error('The normalized OpenUI presentation exceeds its size budget.');
	return normalized;
}

function serializeStatic(node: ASTNode, declarations: Map<string, ASTNode>, budget: { remaining: number }): string {
	switch (node.k) {
		case 'Str': return JSON.stringify(node.v);
		case 'Num': return JSON.stringify(node.v) ?? 'null';
		case 'Bool': return String(node.v);
		case 'Null': return 'null';
		case 'Ref': return node.n;
		case 'Arr': return `[${node.els.map((item) => serializeStatic(item, declarations, budget)).join(', ')}]`;
		case 'Obj': return `{${node.entries.map(([key, value]) => `${JSON.stringify(key)}: ${serializeStatic(value, declarations, budget)}`).join(', ')}}`;
		case 'Comp':
			if (!Object.hasOwn(componentDefinitions, node.name) || node.mappedProps) throw new Error(`Unsupported component, builtin or named prop in “${node.name}”.`);
			return `${node.name}(${node.args.map((arg) => serializeStatic(arg, declarations, budget)).join(', ')})`;
		case 'BinOp': case 'UnaryOp': case 'Ternary': case 'Member': case 'Index': {
			const value = evaluateStaticExpression(node, declarations, new Set(), budget);
			const serialized = JSON.stringify(value);
			if (serialized === undefined) throw new Error('The OpenUI expression did not resolve to a serializable static value.');
			if (new TextEncoder().encode(serialized).length > limits.programBytes) throw new Error('The normalized OpenUI expression exceeds its size budget.');
			return serialized;
		}
		default: throw new Error(`Unsupported OpenUI expression “${node.k}”; use only expressions that resolve from local static values. Reactive state, runtime results, and executable built-ins are not allowed.`);
	}
}

function isOpenUiAst(value: unknown): value is ASTNode {
	if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
	const kind = (value as { k?: unknown }).k;
	return ['Comp', 'Str', 'Num', 'Bool', 'Null', 'Arr', 'Obj', 'Ref', 'Ph', 'StateRef', 'RuntimeRef', 'BinOp', 'UnaryOp', 'Ternary', 'Member', 'Index', 'Assign'].includes(String(kind));
}

/** Resolve OpenUI expression syntax only when every input is a local static value. */
function evaluateStaticExpression(node: ASTNode, declarations: Map<string, ASTNode>, visiting = new Set<string>(), budget = { remaining: 20_000 }): unknown {
	if (--budget.remaining < 0) throw new Error('The OpenUI static expression exceeds its evaluation budget.');
	switch (node.k) {
		case 'Str': return node.v;
		case 'Num': return node.v;
		case 'Bool': return node.v;
		case 'Null': return null;
		case 'Ref': {
			const target = declarations.get(node.n);
			if (!target) throw new Error(`OpenUI expression refers to unknown static value “${node.n}”.`);
			if (visiting.has(node.n)) throw new Error(`OpenUI expression contains a cyclic static reference at “${node.n}”.`);
			visiting.add(node.n);
			try { return evaluateStaticExpression(target, declarations, visiting, budget); }
			finally { visiting.delete(node.n); }
		}
		case 'Arr': return node.els.map((item) => evaluateStaticExpression(item, declarations, visiting, budget));
		case 'Obj': {
			const entries = node.entries.map(([key, item]) => [key, evaluateStaticExpression(item, declarations, visiting, budget)] as const);
			return Object.fromEntries(entries);
		}
		case 'UnaryOp': {
			const operand = evaluateStaticExpression(node.operand, declarations, visiting, budget);
			if (node.op === '!') return !operand;
			if (node.op === '-' && typeof operand === 'number') return checkedArithmetic(-operand);
			break;
		}
		case 'BinOp': {
			const left = evaluateStaticExpression(node.left, declarations, visiting, budget);
			const right = evaluateStaticExpression(node.right, declarations, visiting, budget);
			if (node.op === '&&') return left ? right : left;
			if (node.op === '||') return left ? left : right;
			if (node.op === '+' && (typeof left === 'string' || typeof right === 'string') && ['string', 'number', 'boolean'].includes(typeof left) && ['string', 'number', 'boolean'].includes(typeof right)) {
				const leftText = String(left ?? '');
				const rightText = String(right ?? '');
				if (leftText.length + rightText.length > limits.programBytes) throw new Error('Presentation string expression exceeds its size budget.');
				return leftText + rightText;
			}
			if (node.op === '==' || node.op === '!=') {
				const equal = openUiStaticEquality(left, right);
				return node.op === '==' ? equal : !equal;
			}
			if (typeof left === 'number' && typeof right === 'number') {
				switch (node.op) {
					case '>': return left > right;
					case '<': return left < right;
					case '>=': return left >= right;
					case '<=': return left <= right;
				}
			}
			if (typeof left === 'string' && typeof right === 'string') {
				switch (node.op) {
					case '>': return left > right;
					case '<': return left < right;
					case '>=': return left >= right;
					case '<=': return left <= right;
				}
			}
			if (typeof left !== 'number' || typeof right !== 'number') break;
			if ((node.op === '/' || node.op === '%') && right === 0) throw new Error('Presentation arithmetic cannot divide by zero.');
			let result: number;
			switch (node.op) {
				case '+': result = left + right; break;
				case '-': result = left - right; break;
				case '*': result = left * right; break;
				case '/': result = left / right; break;
				case '%': result = left % right; break;
				default: throw new Error('Unsupported presentation arithmetic operator.');
			}
			return checkedArithmetic(result);
		}
		case 'Ternary': {
			const condition = evaluateStaticExpression(node.cond, declarations, visiting, budget);
			const thenValue = evaluateStaticExpression(node.then, declarations, visiting, budget);
			const elseValue = evaluateStaticExpression(node.else, declarations, visiting, budget);
			return condition ? thenValue : elseValue;
		}
		case 'Member': {
			const object = evaluateStaticExpression(node.obj, declarations, visiting, budget);
			if (typeof object === 'string' && node.field === 'length') return object.length;
			if (Array.isArray(object)) {
				if (node.field === 'length') return object.length;
				if (['__proto__', 'constructor', 'prototype'].includes(node.field)) break;
				return object.map((item) => item && typeof item === 'object' && Object.hasOwn(item, node.field) ? (item as Record<string, unknown>)[node.field] ?? null : null);
			}
			if (object && typeof object === 'object' && !['__proto__', 'constructor', 'prototype'].includes(node.field) && Object.hasOwn(object, node.field)) return (object as Record<string, unknown>)[node.field];
			break;
		}
		case 'Index': {
			const object = evaluateStaticExpression(node.obj, declarations, visiting, budget);
			const index = evaluateStaticExpression(node.index, declarations, visiting, budget);
			const key = String(index);
			if (['__proto__', 'constructor', 'prototype'].includes(key)) break;
			if (Array.isArray(object) && typeof index === 'number' && Number.isInteger(index) && Object.hasOwn(object, index)) return object[index];
			if (typeof object === 'string' && typeof index === 'number' && Number.isInteger(index) && index >= 0 && index < object.length) return object[index];
			if (object && typeof object === 'object' && Object.hasOwn(object, key)) return (object as Record<string, unknown>)[key];
			break;
		}
	}
	throw new Error(`OpenUI expression “${node.k}” must resolve from declared static values; runtime references, built-ins, and executable plans are not allowed.`);
}

function checkedArithmetic(value: number): number {
	if (Number.isFinite(value) && Math.abs(value) <= 1e15) return value;
	throw new Error('Presentation arithmetic must produce a finite, bounded number.');
}

function openUiStaticEquality(left: unknown, right: unknown): boolean {
	if (left === null || right === null) return left === right;
	if (typeof left === typeof right) return left === right;
	if (typeof left === 'boolean') return (left ? 1 : 0) === right;
	if (typeof right === 'boolean') return left === (right ? 1 : 0);
	if (typeof left === 'number' && typeof right === 'string') return Number.isFinite(Number(right)) && left === Number(right);
	if (typeof right === 'number' && typeof left === 'string') return Number.isFinite(Number(left)) && Number(left) === right;
	return false;
}

function materializeStaticValues(value: unknown): unknown {
	if (isOpenUiAst(value)) return evaluateStaticExpression(value, new Map());
	if (Array.isArray(value)) return value.map(materializeStaticValues);
	if (value && typeof value === 'object') {
		if ((value as { type?: unknown }).type === 'element') return value;
		return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, materializeStaticValues(item)]));
	}
	return value;
}

/** Complete programs only. During streaming retain the previous valid frame/fallback. */
export function validatePresentation(program: string, results: BoundResult[]): PresentationValidation {
	const issues: PresentationIssue[] = [];
	const resultIds = new Set<string>();
	let normalizedProgram = '';
	try {
		normalizedProgram = preflight(program);
		validateResults(results);
		const parsed = createParser(presentationSchema()).parse(normalizedProgram);
		if (parsed.meta.incomplete || parsed.meta.unresolved.length) throw new Error(`The presentation is incomplete or has unresolved references${parsed.meta.unresolved.length ? `: ${parsed.meta.unresolved.join(', ')}` : '.'}`);
		if (parsed.meta.statementCount > limits.components || parsed.meta.orphaned.length) throw new Error('The presentation contains unused statements or exceeds the statement budget.');
		if (parsed.meta.errors.length) {
			for (const error of parsed.meta.errors) issues.push({ path: `${error.component}${error.path}`, message: error.message });
			return { valid: false, root: null, normalizedProgram, issues, resultIds: [] };
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
			node.props = materializeStaticValues(node.props) as Record<string, unknown>;
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
		return { valid: true, root: parsed.root, normalizedProgram, issues, resultIds: [...resultIds] };
	} catch (error) {
		issues.push({ path: 'root', message: error instanceof Error ? error.message : 'The presentation could not be validated.' });
		return { valid: false, root: null, normalizedProgram, issues, resultIds: [...resultIds] };
	}
}
