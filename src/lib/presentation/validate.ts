import { createParser, parseExpression, split, tokenize, toNumber, type ASTNode, type ElementNode, type Token } from '@openuidev/lang-core';
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

function skipDoubleQuotedString(source: string, start: number): number {
	if (source[start] !== '"') return start;
	let index = start + 1;
	while (index < source.length) {
		if (source[index] === '\\') index += 2;
		else if (source[index] === '"') return index + 1;
		else index++;
	}
	return index;
}

/** Match OpenUI Lang's own extraction of fenced model output. */
function stripOpenUiFences(input: string): string {
	const blocks: string[] = [];
	let index = 0;
	while (index < input.length) {
		let start = -1;
		while (index < input.length) {
			const afterString = skipDoubleQuotedString(input, index);
			if (afterString > index) { index = afterString; continue; }
			if (input.startsWith('```', index)) { start = index; break; }
			index++;
		}
		if (start === -1) break;
		let bodyStart = start + 3;
		while (bodyStart < input.length && input[bodyStart] !== '\n') bodyStart++;
		if (bodyStart >= input.length) {
			blocks.push(input.slice(start + 3).replace(/^[^\n]*\n?/, ''));
			break;
		}
		bodyStart++;
		let close = -1;
		let cursor = bodyStart;
		while (cursor < input.length) {
			const afterString = skipDoubleQuotedString(input, cursor);
			if (afterString > cursor) { cursor = afterString; continue; }
			if (input.startsWith('```', cursor)) { close = cursor; break; }
			cursor++;
		}
		blocks.push(input.slice(bodyStart, close === -1 ? input.length : close));
		if (close === -1) break;
		index = close + 3;
	}
	if (blocks.length) return blocks.join('\n');
	if (!input.startsWith('```')) return input;
	let firstLineEnd = input.indexOf('\n', 3);
	if (firstLineEnd === -1) firstLineEnd = input.length;
	const body = input.slice(firstLineEnd === input.length ? 3 : firstLineEnd + 1);
	const trailingFence = body.lastIndexOf('```');
	return trailingFence === -1 ? body : body.slice(0, trailingFence);
}

/** OpenUI strips // and # line comments outside both supported string forms. */
function stripOpenUiComments(input: string): string {
	let quote = '';
	return input.split('\n').map((line) => {
		for (let index = 0; index < line.length; index++) {
			const char = line[index];
			if (quote) {
				if (char === '\\' && index + 1 < line.length) index++;
				else if (char === quote) quote = '';
				continue;
			}
			if (char === '"' || char === "'") { quote = char; continue; }
			if (char === '#' || char === '/' && line[index + 1] === '/') return line.slice(0, index).trimEnd();
		}
		return line;
	}).join('\n');
}

function normalizeOpenUiSource(input: string): string {
	return stripOpenUiComments(stripOpenUiFences(input.trim())).trim();
}

/** The OpenUI lexer skips unknown characters; reject those before trusting its tokens. */
function validateOpenUiSourceCharacters(source: string): void {
	const brackets: string[] = [];
	let quote = '';
	let escaped = false;
	for (let index = 0; index < source.length; index++) {
		const char = source[index];
		if (quote) {
			if (escaped) escaped = false;
			else if (char === '\\') escaped = true;
			else if (char === quote) quote = '';
			continue;
		}
		if (char === '"' || char === "'") { quote = char; continue; }
		if (' \t\r\n'.includes(char)) continue;
		if (/[A-Za-z0-9_]/.test(char)) continue;
		if (char === '$' || char === '@') {
			if (!/[A-Za-z_]/.test(source[index + 1] ?? '')) throw new Error('The presentation contains an unsupported OpenUI token.');
			continue;
		}
		if (char === '&' || char === '|') {
			if (source[index + 1] !== char) throw new Error('OpenUI logical operators must use && or ||.');
			index++;
			continue;
		}
		if ('([{'.includes(char)) {
			brackets.push(char);
			if (brackets.length > limits.depth * 3) throw new Error('The presentation nesting is too deep.');
			continue;
		}
		if (')]}'.includes(char)) {
			const expected = char === ')' ? '(' : char === ']' ? '[' : '{';
			if (brackets.pop() !== expected) throw new Error('The presentation has mismatched delimiters.');
			continue;
		}
		if ('},:.?=<>!+-*/%'.includes(char)) continue;
		throw new Error('The presentation contains an unsupported character outside a string.');
	}
	if (quote || brackets.length) throw new Error('The presentation is incomplete.');
}

/** Strictly consume the expression grammar that OpenUI's recovery parser accepts. */
function assertCompleteOpenUiExpression(tokens: Token[]): void {
	// Numeric token IDs are the public T enum from the pinned lang-core API.
	const precedence = new Map<number, number>([
		[34, 1], [32, 2], [31, 3], [25, 4], [26, 4], [27, 5], [28, 5], [29, 5], [30, 5],
		[20, 6], [21, 6], [22, 7], [23, 7], [24, 7], [19, 9], [3, 9]
	]);
	let position = 0;
	let nesting = 0;
	const current = () => tokens[position];
	const take = () => tokens[position++];
	const expect = (kind: number) => {
		if (current()?.t !== kind) throw new Error('The presentation contains malformed or unparsed OpenUI syntax.');
		position++;
	};
	const enter = () => {
		if (++nesting > limits.depth * 3) throw new Error('The presentation expression is nested too deeply.');
	};
	const leave = () => { nesting--; };
	function parseExpression(minPrecedence = 0): void {
		enter();
		parsePrefix();
		while (current() && (precedence.get(current().t) ?? 0) > minPrecedence) {
			const operator = take().t;
			if (operator === 19) {
				if (![14, 15, 16, 17, 18].includes(current()?.t ?? -1)) throw new Error('The presentation contains malformed member access.');
				take();
			} else if (operator === 3) {
				parseExpression(0);
				expect(4);
			} else if (operator === 34) {
				parseExpression(0);
				expect(8);
				parseExpression(0);
			} else parseExpression(precedence.get(operator)!);
		}
		leave();
	}
	function parseDelimitedList(close: number, parseItem: () => void): void {
		if (current()?.t === close) { take(); return; }
		while (true) {
			parseItem();
			if (current()?.t === close) { take(); return; }
			expect(7);
			if (current()?.t === close) { take(); return; }
		}
	}
	function parseCall(): void {
		expect(1);
		parseDelimitedList(2, () => parseExpression(0));
	}
	function parsePrefix(): void {
		const token = current();
		if (!token) throw new Error('The presentation contains an empty expression.');
		if ([10, 11, 12, 14, 15, 16].includes(token.t)) { take(); return; }
		if (token.t === 17 || token.t === 35) {
			take();
			if (current()?.t === 1) parseCall();
			return;
		}
		if (token.t === 18) {
			take();
			if (current()?.t === 9) { take(); parseExpression(0); }
			return;
		}
		if (token.t === 3) { take(); parseDelimitedList(4, () => parseExpression(0)); return; }
		if (token.t === 5) {
			take();
			parseDelimitedList(6, () => {
				if (![14, 15, 16, 17, 18].includes(current()?.t ?? -1)) throw new Error('The presentation contains an invalid object key.');
				take();
				expect(8);
				parseExpression(0);
			});
			return;
		}
		if (token.t === 33 || token.t === 21) { take(); parseExpression(8); return; }
		if (token.t === 1) { take(); parseExpression(0); expect(2); return; }
		throw new Error('The presentation contains malformed or unsupported OpenUI syntax.');
	}
	parseExpression(0);
	if (position !== tokens.length) throw new Error('The presentation contains trailing or unparsed OpenUI syntax.');
}

function validateStaticAst(node: ASTNode, depth = 0, state = { nodes: 0 }): void {
	if (++state.nodes > limits.programBytes || depth > limits.depth * 3) throw new Error('The presentation expression is too complex.');
	switch (node.k) {
		case 'Str': case 'Bool': case 'Null': case 'Ref': case 'Ph': case 'StateRef': case 'RuntimeRef': return;
		case 'Num':
			if (!Number.isFinite(node.v)) throw new Error('Presentation numbers must be finite.');
			return;
		case 'Comp':
			if (!Object.hasOwn(componentDefinitions, node.name)) throw new Error(`Unsupported component or builtin “${node.name}”.`);
			if (node.mappedProps) throw new Error('Named props are not supported in analytical presentations.');
			node.args.forEach((arg) => validateStaticAst(arg, depth + 1, state));
			return;
		case 'Arr': node.els.forEach((item) => validateStaticAst(item, depth + 1, state)); return;
		case 'Obj': {
			const keys = node.entries.map(([key]) => key);
			if (new Set(keys).size !== keys.length || keys.some((key) => ['__proto__', 'constructor', 'prototype'].includes(key))) throw new Error('Duplicate or unsafe object keys are not allowed.');
			node.entries.forEach(([, item]) => validateStaticAst(item, depth + 1, state));
			return;
		}
		case 'BinOp': validateStaticAst(node.left, depth + 1, state); validateStaticAst(node.right, depth + 1, state); return;
		case 'UnaryOp': validateStaticAst(node.operand, depth + 1, state); return;
		case 'Ternary': validateStaticAst(node.cond, depth + 1, state); validateStaticAst(node.then, depth + 1, state); validateStaticAst(node.else, depth + 1, state); return;
		case 'Member': validateStaticAst(node.obj, depth + 1, state); return;
		case 'Index': validateStaticAst(node.obj, depth + 1, state); validateStaticAst(node.index, depth + 1, state); return;
		case 'Assign': throw new Error('State assignments and executable OpenUI expressions are not allowed in presentations.');
		default: throw new Error(`Unsupported OpenUI expression “${(node as { k: string }).k}”.`);
	}
}

/** Validate OpenUI's static expression subset without losing its grammar or recovery boundaries. */
function preflight(program: string): string {
	if (typeof program !== 'string' || !program.trim()) throw new Error('The presentation is empty.');
	if (new TextEncoder().encode(program).length > limits.programBytes) throw new Error('The presentation exceeds its size budget.');
	const source = normalizeOpenUiSource(program);
	if (!source) throw new Error('The presentation is empty.');
	validateOpenUiSourceCharacters(source);
	const tokens = tokenize(source);
	const statements = split(tokens);
	const ids = new Set<string>();
	const astById = new Map<string, ASTNode>();
	const statementTokens: { id: string; ast: ASTNode }[] = [];
	let accounted = 0;
	for (const statement of statements) {
		if (ids.has(statement.id) || statement.id.startsWith('$')) throw new Error('Duplicate statements and reactive variables are not allowed.');
		ids.add(statement.id);
		assertCompleteOpenUiExpression(statement.tokens);
		const ast = parseExpression(statement.tokens);
		validateStaticAst(ast);
		astById.set(statement.id, ast);
		statementTokens.push({ id: statement.id, ast });
		accounted += statement.tokens.length + 2;
	}
	if (!statements.length || tokens.filter((token) => token.t !== 0 && token.t !== 13).length !== accounted) throw new Error('The presentation contains an invalid or incomplete statement.');

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
	const missingReferences = [...sourceReachable].filter((id) => !ids.has(id));
	if (missingReferences.length) throw new Error(`The presentation contains unresolved references: ${missingReferences.join(', ')}.`);
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
			if (node.op === '-') return checkedArithmetic(-toNumber(operand));
			break;
		}
		case 'BinOp': {
			const left = evaluateStaticExpression(node.left, declarations, visiting, budget);
			if (node.op === '&&') return left ? evaluateStaticExpression(node.right, declarations, visiting, budget) : left;
			if (node.op === '||') return left ? left : evaluateStaticExpression(node.right, declarations, visiting, budget);
			const right = evaluateStaticExpression(node.right, declarations, visiting, budget);
			if (node.op === '+' && (typeof left === 'string' || typeof right === 'string')) {
				// Match OpenUI's String(value ?? '') coercion for every static literal.
				const leftText = String(left ?? '');
				const rightText = String(right ?? '');
				if (leftText.length + rightText.length > limits.programBytes) throw new Error('Presentation string expression exceeds its size budget.');
				return leftText + rightText;
			}
			if (node.op === '==' || node.op === '!=') {
				const equal = openUiStaticEquality(left, right);
				return node.op === '==' ? equal : !equal;
			}
			const leftNumber = toNumber(left);
			const rightNumber = toNumber(right);
			let result: number;
			switch (node.op) {
				case '+': result = leftNumber + rightNumber; break;
				case '-': result = leftNumber - rightNumber; break;
				case '*': result = leftNumber * rightNumber; break;
				case '/': result = rightNumber === 0 ? 0 : leftNumber / rightNumber; break;
				case '%': result = rightNumber === 0 ? 0 : leftNumber % rightNumber; break;
				case '>': return leftNumber > rightNumber;
				case '<': return leftNumber < rightNumber;
				case '>=': return leftNumber >= rightNumber;
				case '<=': return leftNumber <= rightNumber;
				default: throw new Error('Unsupported presentation arithmetic operator.');
			}
			return checkedArithmetic(result);
		}
		case 'Ternary': {
			const condition = evaluateStaticExpression(node.cond, declarations, visiting, budget);
			return condition
				? evaluateStaticExpression(node.then, declarations, visiting, budget)
				: evaluateStaticExpression(node.else, declarations, visiting, budget);
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
	// OpenUI uses JavaScript's abstract equality (`==`) for its `==` operator.
	// These values come only from the statically parsed literal subset above;
	// no model-authored functions or executable objects can reach this point.
	return left == right;
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
