import { BUILTIN_NAMES, createParser, parseExpression, split, tokenize, toNumber, type ASTNode, type ElementNode, type Token } from '@openuidev/lang-core';
import { componentDefinitions, normalizeCardTone, type ComponentName } from './schemas';
import { presentationSchema } from './manifest';
import { derivedRowsResult, requireResult, validateResults } from './results';
import { validateChartSpec } from './chart';
import { PRESENTATION_LIMITS as limits, type BoundResult, type PresentationIssue } from './types';
import { createPresentationStore, evaluatePresentationTree } from './runtime';

export interface PresentationValidation {
	valid: boolean;
	/** Default-state, evaluated tree retained for callers that inspect validated props. */
	root: ElementNode | null;
	/** Parsed dynamic OpenUI tree consumed by the reactive host renderer. */
	sourceRoot: ElementNode | null;
	normalizedProgram: string;
	issues: PresentationIssue[];
	resultIds: string[];
	stateDefaults: Record<string, unknown>;
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

function findMatchingDelimiter(source: string, start: number): number {
	const opening = source[start];
	const closers: Record<string, string> = { '(': ')', '[': ']', '{': '}' };
	if (!Object.hasOwn(closers, opening)) return -1;
	const stack = [closers[opening]];
	let quote = '';
	for (let index = start + 1; index < source.length; index++) {
		const char = source[index];
		if (quote) {
			if (char === '\\') index++;
			else if (char === quote) quote = '';
			continue;
		}
		if (char === '"' || char === "'") { quote = char; continue; }
		if (Object.hasOwn(closers, char)) stack.push(closers[char]);
		else if (')]}'.includes(char)) {
			if (stack.pop() !== char) return -1;
			if (!stack.length) return index;
		}
	}
	return -1;
}

function splitComponentArguments(source: string): string[] | null {
	if (!source.trim()) return [];
	const args: string[] = [];
	let start = 0;
	let quote = '';
	const stack: string[] = [];
	const closers: Record<string, string> = { '(': ')', '[': ']', '{': '}' };
	for (let index = 0; index < source.length; index++) {
		const char = source[index];
		if (quote) {
			if (char === '\\') index++;
			else if (char === quote) quote = '';
			continue;
		}
		if (char === '"' || char === "'") { quote = char; continue; }
		if (Object.hasOwn(closers, char)) stack.push(closers[char]);
		else if (')]}'.includes(char)) {
			if (stack.pop() !== char) return null;
		} else if (char === ',' && !stack.length) {
			const arg = source.slice(start, index).trim();
			if (arg) args.push(arg);
			start = index + 1;
		}
	}
	if (quote || stack.length) return null;
	const tail = source.slice(start).trim();
	if (tail) args.push(tail);
	return args;
}

function namedComponentArgument(argument: string): { name: string; value: string } | null {
	let quote = '';
	const stack: string[] = [];
	const closers: Record<string, string> = { '(': ')', '[': ']', '{': '}' };
	for (let index = 0; index < argument.length; index++) {
		const char = argument[index];
		if (quote) {
			if (char === '\\') index++;
			else if (char === quote) quote = '';
			continue;
		}
		if (char === '"' || char === "'") { quote = char; continue; }
		if (Object.hasOwn(closers, char)) stack.push(closers[char]);
		else if (')]}'.includes(char)) {
			if (stack.pop() !== char) return null;
		} else if ((char === ':' || char === '=' && argument[index + 1] !== '=' && !'=!<>'.includes(argument[index - 1] ?? ' ')) && !stack.length) {
			const name = argument.slice(0, index).trim();
			if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) continue;
			const value = argument.slice(index + 1).trim();
			if (!value) throw new Error(`Named component prop “${name}” needs a value.`);
			return { name, value };
		}
	}
	return null;
}

function requiredComponentProps(name: ComponentName): Set<string> {
	const cached = requiredPropsByComponent.get(name);
	if (cached) return cached;
	const definitions = presentationSchema().$defs as Record<string, { required?: string[] }>;
	const required = new Set(definitions[name]?.required ?? []);
	requiredPropsByComponent.set(name, required);
	return required;
}

const requiredPropsByComponent = new Map<ComponentName, Set<string>>();

/**
 * OpenUI Lang calls are positional. Model-facing JSON-schema signatures can
 * nevertheless lead models to emit `Card(..., title: "...")`. Accept that
 * ergonomic form only for registered Fella components, then normalize it to
 * native OpenUI positional calls before parsing or persistence.
 */
function normalizeNamedComponentProps(source: string): string {
	let rewritten = '';
	let index = 0;
	while (index < source.length) {
		const char = source[index];
		if (char === '"' || char === "'") {
			const end = skipQuotedString(source, index);
			rewritten += source.slice(index, end);
			index = end;
			continue;
		}
		if (!/[A-Za-z_]/.test(char)) { rewritten += char; index++; continue; }
		let nameEnd = index + 1;
		while (nameEnd < source.length && /[A-Za-z0-9_]/.test(source[nameEnd])) nameEnd++;
		const name = source.slice(index, nameEnd);
		let open = nameEnd;
		while (open < source.length && /\s/.test(source[open])) open++;
		if (!Object.hasOwn(componentDefinitions, name) || source[open] !== '(') {
			rewritten += source.slice(index, nameEnd);
			index = nameEnd;
			continue;
		}
		const close = findMatchingDelimiter(source, open);
		if (close === -1) {
			rewritten += source.slice(index, nameEnd);
			index = nameEnd;
			continue;
		}
		const body = source.slice(open + 1, close);
		const args = splitComponentArguments(body);
		if (!args) {
			rewritten += source.slice(index, nameEnd);
			index = nameEnd;
			continue;
		}
		const namedArgs = args.map(namedComponentArgument);
		if (!namedArgs.some(Boolean)) {
			const normalizedBody = normalizeNamedComponentProps(body);
			rewritten += `${source.slice(index, open + 1)}${normalizedBody})`;
			index = close + 1;
			continue;
		}
		const nestedArgs = args.map(normalizeNamedComponentProps);

		const propNames = Object.keys(componentDefinitions[name as ComponentName].props.shape);
		const propIndexes = new Map(propNames.map((prop, propIndex) => [prop, propIndex]));
		const required = requiredComponentProps(name as ComponentName);
		const values: (string | undefined)[] = Array(propNames.length).fill(undefined);
		let positionalIndex = 0;
		let hasNamed = false;
		for (let argIndex = 0; argIndex < nestedArgs.length; argIndex++) {
			const named = namedArgs[argIndex];
			if (named) {
				hasNamed = true;
				const propIndex = propIndexes.get(named.name);
				if (propIndex === undefined) throw new Error(`Unknown named prop “${named.name}” on ${name}.`);
				if (values[propIndex] !== undefined) throw new Error(`The ${name} prop “${named.name}” was supplied more than once.`);
				values[propIndex] = normalizeNamedComponentProps(named.value);
			} else {
				if (hasNamed) throw new Error(`${name} positional arguments cannot follow named props.`);
				if (positionalIndex >= propNames.length) throw new Error(`${name} has too many positional arguments.`);
				values[positionalIndex++] = nestedArgs[argIndex];
			}
		}
		const lastValue = values.reduce((last, value, propIndex) => value === undefined ? last : propIndex, -1);
		for (let propIndex = 0; propIndex <= lastValue; propIndex++) {
			if (values[propIndex] !== undefined) continue;
			const prop = propNames[propIndex];
			if (required.has(prop)) throw new Error(`The required ${name} prop “${prop}” is missing.`);
			values[propIndex] = 'null';
		}
		rewritten += `${name}${source.slice(nameEnd, open + 1)}${values.slice(0, lastValue + 1).join(', ')})`;
		index = close + 1;
	}
	return rewritten;
}

function skipQuotedString(source: string, start: number): number {
	const quote = source[start];
	let index = start + 1;
	while (index < source.length) {
		if (source[index] === '\\') index += 2;
		else if (source[index] === quote) return index + 1;
		else index++;
	}
	return index;
}

function stripOmittedOptionalProps(value: unknown): void {
	if (Array.isArray(value)) { value.forEach(stripOmittedOptionalProps); return; }
	if (!value || typeof value !== 'object' || (value as ElementNode).type !== 'element') return;
	const node = value as ElementNode;
	const required = requiredComponentProps(node.typeName as ComponentName);
	for (const [name, prop] of Object.entries(node.props)) {
		if (prop === null && !required.has(name)) delete node.props[name];
		else if (name === 'children') stripOmittedOptionalProps(prop);
	}
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
				if (current()?.t === 1) throw new Error('OpenUI does not support JavaScript method calls such as .slice(...). Use @Top(@Rows("result-id"), "field", count, "desc") to select saved rows, or a supported OpenUI function.');
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

const unsafeObjectKeys = new Set(['__proto__', 'constructor', 'prototype']);

function validateSafeAst(node: ASTNode, declaredState: Set<string>, depth = 0, state = { nodes: 0 }, scoped = new Set<string>()): void {
	if (++state.nodes > limits.programBytes || depth > limits.depth * 3) throw new Error('The presentation expression is too complex.');
	switch (node.k) {
		case 'Str': case 'Bool': case 'Null': return;
		case 'Num':
			if (!Number.isFinite(node.v) || Math.abs(node.v) > 1e15) throw new Error('Presentation numbers must be finite and bounded.');
			return;
		case 'Ref':
			if (!scoped.has(node.n) && node.n.startsWith('$')) throw new Error('Reactive variables must use OpenUI state references.');
			return;
		case 'StateRef':
			if (!declaredState.has(node.n)) throw new Error(`Undeclared local presentation state “${node.n}”.`);
			return;
		case 'Ph': throw new Error(`Unresolved OpenUI reference “${node.n}”.`);
		case 'RuntimeRef': throw new Error('Runtime, query and mutation references are not permitted in presentations.');
		case 'Comp': {
			if (node.name === 'Run' || node.name === 'OpenUrl') throw new Error(`${node.name} actions are not permitted in analytical presentations.`);
			const isComponent = Object.hasOwn(componentDefinitions, node.name);
			const isFunction = BUILTIN_NAMES.has(node.name);
			if (!isComponent && !isFunction) throw new Error(`Unsupported component or OpenUI function “${node.name}”.`);
			if (node.name === 'Set' && (node.args.length !== 2 || node.args[0]?.k !== 'StateRef' || !declaredState.has(node.args[0].n))) throw new Error('Set() may update only a declared local state variable.');
			if (node.name === 'Reset' && (!node.args.length || node.args.some((arg) => arg.k !== 'StateRef' || !declaredState.has(arg.n)))) throw new Error('Reset() may reset only declared local state variables.');
			if (node.name === 'ToAssistant' && (!node.args.length || node.args.length > 2)) throw new Error('ToAssistant() needs a follow-up message.');
			if (node.name === 'Rows' && (node.args.length !== 1 || node.args[0]?.k !== 'Str')) throw new Error('Rows() must name one literal supplied-result ID.');
			if (node.name === 'Each') {
				if (node.args.length !== 3) throw new Error('Each() needs a list, a row variable, and a component template.');
				const binder = node.args[1].k === 'Ref' ? node.args[1].n : node.args[1].k === 'Str' ? node.args[1].v : '';
				if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(binder)) throw new Error('Each() needs a simple local row variable.');
				validateSafeAst(node.args[0], declaredState, depth + 1, state, scoped);
				const nested = new Set(scoped); nested.add(binder);
				validateSafeAst(node.args[2], declaredState, depth + 1, state, nested);
				return;
			}
			if (node.mappedProps) {
				if (!isComponent) throw new Error('Named props are not permitted for OpenUI functions.');
				for (const value of Object.values(node.mappedProps)) validateSafeAst(value, declaredState, depth + 1, state, scoped);
			} else node.args.forEach((arg) => validateSafeAst(arg, declaredState, depth + 1, state, scoped));
			return;
		}
		case 'Arr': node.els.forEach((item) => validateSafeAst(item, declaredState, depth + 1, state, scoped)); return;
		case 'Obj': {
			const keys = node.entries.map(([key]) => key);
			if (new Set(keys).size !== keys.length || keys.some((key) => unsafeObjectKeys.has(key))) throw new Error('Duplicate or unsafe object keys are not allowed.');
			node.entries.forEach(([, item]) => validateSafeAst(item, declaredState, depth + 1, state, scoped));
			return;
		}
		case 'BinOp': validateSafeAst(node.left, declaredState, depth + 1, state, scoped); validateSafeAst(node.right, declaredState, depth + 1, state, scoped); return;
		case 'UnaryOp': validateSafeAst(node.operand, declaredState, depth + 1, state, scoped); return;
		case 'Ternary': validateSafeAst(node.cond, declaredState, depth + 1, state, scoped); validateSafeAst(node.then, declaredState, depth + 1, state, scoped); validateSafeAst(node.else, declaredState, depth + 1, state, scoped); return;
		case 'Member': validateSafeAst(node.obj, declaredState, depth + 1, state, scoped); return;
		case 'Index': validateSafeAst(node.obj, declaredState, depth + 1, state, scoped); validateSafeAst(node.index, declaredState, depth + 1, state, scoped); return;
		case 'Assign': throw new Error('Unscoped state assignments are not permitted. Use a user-triggered Button action.');
		default: throw new Error(`Unsupported OpenUI expression “${(node as { k: string }).k}”.`);
	}
}

interface PreflightResult { program: string; stateDefaults: Record<string, unknown>; componentValues: Set<string>; statementCount: number }

/** Validate complete OpenUI syntax and only the safe, local presentation runtime subset. */
function preflight(program: string): PreflightResult {
	if (typeof program !== 'string' || !program.trim()) throw new Error('The presentation is empty.');
	if (new TextEncoder().encode(program).length > limits.programBytes) throw new Error('The presentation exceeds its size budget.');
	const source = normalizeNamedComponentProps(normalizeOpenUiSource(program));
	if (!source) throw new Error('The presentation is empty.');
	validateOpenUiSourceCharacters(source);
	const tokens = tokenize(source);
	const statements = split(tokens);
	const ids = new Set<string>();
	const astById = new Map<string, ASTNode>();
	const stateDefaults: Record<string, unknown> = Object.create(null);
	const declarations: { id: string; ast: ASTNode; state: boolean }[] = [];
	let accounted = 0;
	for (const statement of statements) {
		const isState = statement.id.startsWith('$');
		if (ids.has(statement.id)) throw new Error('Duplicate OpenUI statements are not allowed.');
		if (isState && !/^\$[A-Za-z_][A-Za-z0-9_]*$/.test(statement.id)) throw new Error('A local state name is invalid.');
		ids.add(statement.id);
		assertCompleteOpenUiExpression(statement.tokens);
		const ast = parseExpression(statement.tokens);
		declarations.push({ id: statement.id, ast, state: isState });
		if (isState) {
			if (!['Str', 'Num', 'Bool', 'Null'].includes(ast.k)) throw new Error('Presentation state defaults must be simple text, number, boolean, or null values.');
			const value = ast.k === 'Str' ? ast.v : ast.k === 'Num' ? ast.v : ast.k === 'Bool' ? ast.v : null;
			if (typeof value === 'string' && value.length > 256 || typeof value === 'number' && (!Number.isFinite(value) || Math.abs(value) > 1e12)) throw new Error('A local state default exceeds its value limit.');
			stateDefaults[statement.id] = value;
		} else astById.set(statement.id, ast);
		accounted += statement.tokens.length + 2;
	}
	if (!statements.length || statements.length > limits.components + 32 || tokens.filter((token) => token.t !== 0 && token.t !== 13).length !== accounted) throw new Error('The presentation contains an invalid or incomplete statement.');
	const declaredState = new Set(Object.keys(stateDefaults));
	for (const declaration of declarations) if (!declaration.state) validateSafeAst(declaration.ast, declaredState);
	const componentValues = new Set<string>();
	function referencesComponent(node: ASTNode, visiting = new Set<string>()): boolean {
		if (node.k === 'Comp') {
			if (Object.hasOwn(componentDefinitions, node.name)) return true;
			return node.args.some((arg) => referencesComponent(arg, visiting))
				|| Object.values(node.mappedProps ?? {}).some((arg) => referencesComponent(arg, visiting));
		}
		if (node.k === 'Ref') {
			if (visiting.has(node.n)) return false;
			const declaration = astById.get(node.n);
			if (!declaration) return false;
			const next = new Set(visiting); next.add(node.n);
			return referencesComponent(declaration, next);
		}
		if (node.k === 'Arr') return node.els.some((item) => referencesComponent(item, visiting));
		if (node.k === 'Obj') return node.entries.some(([, item]) => referencesComponent(item, visiting));
		if (node.k === 'BinOp') return referencesComponent(node.left, visiting) || referencesComponent(node.right, visiting);
		if (node.k === 'UnaryOp') return referencesComponent(node.operand, visiting);
		if (node.k === 'Ternary') return referencesComponent(node.cond, visiting) || referencesComponent(node.then, visiting) || referencesComponent(node.else, visiting);
		if (node.k === 'Member') return referencesComponent(node.obj, visiting);
		if (node.k === 'Index') return referencesComponent(node.obj, visiting) || referencesComponent(node.index, visiting);
		if (node.k === 'Assign') return referencesComponent(node.value, visiting);
		return false;
	}
	for (const [id, ast] of astById) if (referencesComponent(ast)) componentValues.add(id);

	function collectReferences(node: ASTNode, into: Set<string>, scoped = new Set<string>()): void {
		switch (node.k) {
			case 'Ref': if (!scoped.has(node.n)) into.add(node.n); break;
			case 'Comp':
				if (node.name === 'Each' && node.args.length === 3) {
					collectReferences(node.args[0], into, scoped);
					const binder = node.args[1].k === 'Ref' ? node.args[1].n : node.args[1].k === 'Str' ? node.args[1].v : '';
					const nested = new Set(scoped); if (binder) nested.add(binder);
					collectReferences(node.args[2], into, nested);
				} else node.args.forEach((arg) => collectReferences(arg, into, scoped));
				if (node.mappedProps) Object.values(node.mappedProps).forEach((arg) => collectReferences(arg, into, scoped));
				break;
			case 'Arr': node.els.forEach((item) => collectReferences(item, into, scoped)); break;
			case 'Obj': node.entries.forEach(([, item]) => collectReferences(item, into, scoped)); break;
			case 'BinOp': collectReferences(node.left, into, scoped); collectReferences(node.right, into, scoped); break;
			case 'UnaryOp': collectReferences(node.operand, into, scoped); break;
			case 'Ternary': collectReferences(node.cond, into, scoped); collectReferences(node.then, into, scoped); collectReferences(node.else, into, scoped); break;
			case 'Member': collectReferences(node.obj, into, scoped); break;
			case 'Index': collectReferences(node.obj, into, scoped); collectReferences(node.index, into, scoped); break;
			case 'Assign': collectReferences(node.value, into, scoped); break;
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
	if ([...astById.keys()].some((id) => !sourceReachable.has(id))) throw new Error('The presentation contains unused statements.');
	if (new TextEncoder().encode(source).length > limits.programBytes) throw new Error('The normalized OpenUI presentation exceeds its size budget.');
	return { program: source, stateDefaults, componentValues, statementCount: declarations.length };
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

function isAstNode(value: unknown): value is ASTNode {
	return !!value && typeof value === 'object' && typeof (value as { k?: unknown }).k === 'string';
}

function containsRows(value: unknown): boolean {
	if (isAstNode(value)) {
		if (value.k === 'Comp' && value.name === 'Rows') return true;
		if (value.k === 'Comp') return value.args.some(containsRows) || Object.values(value.mappedProps ?? {}).some(containsRows);
		if (value.k === 'Arr') return value.els.some(containsRows);
		if (value.k === 'Obj') return value.entries.some(([, item]) => containsRows(item));
		if (value.k === 'BinOp') return containsRows(value.left) || containsRows(value.right);
		if (value.k === 'UnaryOp') return containsRows(value.operand);
		if (value.k === 'Ternary') return containsRows(value.cond) || containsRows(value.then) || containsRows(value.else);
		if (value.k === 'Member') return containsRows(value.obj);
		if (value.k === 'Index') return containsRows(value.obj) || containsRows(value.index);
		if (value.k === 'Assign') return containsRows(value.value);
		return false;
	}
	if (Array.isArray(value)) return value.some(containsRows);
	if (value && typeof value === 'object') return Object.values(value).some(containsRows);
	return false;
}

function onlyRowsDerived(value: unknown): boolean {
	if (!isAstNode(value)) return false;
	if (value.k === 'Comp') {
		if (value.name === 'Rows') return value.args.length === 1 && value.args[0]?.k === 'Str';
		if (value.name === 'Each') return onlyRowsDerived(value.args[0]);
		const listOperators = new Set(['Filter', 'Sort', 'GroupBy', 'Pivot', 'Top', 'Between', 'Change', 'Cumulative', 'MovingAvg', 'Histogram']);
		return listOperators.has(value.name) && onlyRowsDerived(value.args[0]);
	}
	if (value.k === 'Ternary') return onlyRowsDerived(value.then) && onlyRowsDerived(value.else);
	if (value.k === 'Ref') return true;
	return false;
}

function rowIdsFrom(value: unknown, into: Set<string>): void {
	if (isAstNode(value)) {
		if (value.k === 'Comp' && value.name === 'Rows') {
			const id = value.args[0]?.k === 'Str' ? value.args[0].v : '';
			if (!id) throw new Error('Rows() needs a literal saved result ID.');
			into.add(id);
		}
		if (value.k === 'Comp') {
			value.args.forEach((arg) => rowIdsFrom(arg, into));
			Object.values(value.mappedProps ?? {}).forEach((item) => rowIdsFrom(item, into));
		} else if (value.k === 'Arr') value.els.forEach((item) => rowIdsFrom(item, into));
		else if (value.k === 'Obj') value.entries.forEach(([, item]) => rowIdsFrom(item, into));
		else if (value.k === 'BinOp') { rowIdsFrom(value.left, into); rowIdsFrom(value.right, into); }
		else if (value.k === 'UnaryOp') rowIdsFrom(value.operand, into);
		else if (value.k === 'Ternary') { rowIdsFrom(value.cond, into); rowIdsFrom(value.then, into); rowIdsFrom(value.else, into); }
		else if (value.k === 'Member') rowIdsFrom(value.obj, into);
		else if (value.k === 'Index') { rowIdsFrom(value.obj, into); rowIdsFrom(value.index, into); }
		else if (value.k === 'Assign') rowIdsFrom(value.value, into);
		return;
	}
	if (Array.isArray(value)) value.forEach((item) => rowIdsFrom(item, into));
	else if (value && typeof value === 'object') Object.values(value).forEach((item) => rowIdsFrom(item, into));
}

function containsComponentValue(value: unknown, componentValues: ReadonlySet<string>, scoped = new Set<string>(), seen = new Set<string>()): boolean {
	if (isAstNode(value)) {
		if (value.k === 'Comp') {
			if (Object.hasOwn(componentDefinitions, value.name)) return true;
			if (value.name === 'Each' && value.args.length === 3) {
				const binder = value.args[1].k === 'Ref' ? value.args[1].n : value.args[1].k === 'Str' ? value.args[1].v : '';
				const nested = new Set(scoped); if (binder) nested.add(binder);
				return containsComponentValue(value.args[0], componentValues, scoped, seen)
					|| containsComponentValue(value.args[2], componentValues, nested, seen);
			}
			return value.args.some((item) => containsComponentValue(item, componentValues, scoped, seen))
				|| Object.values(value.mappedProps ?? {}).some((item) => containsComponentValue(item, componentValues, scoped, seen));
		}
		if (value.k === 'Ref') {
			if (scoped.has(value.n)) return false;
			if (componentValues.has(value.n)) return true;
			if (seen.has(value.n)) return false;
			return false;
		}
		if (value.k === 'Arr') return value.els.some((item) => containsComponentValue(item, componentValues, scoped, seen));
		if (value.k === 'Obj') return value.entries.some(([, item]) => containsComponentValue(item, componentValues, scoped, seen));
		if (value.k === 'BinOp') return containsComponentValue(value.left, componentValues, scoped, seen) || containsComponentValue(value.right, componentValues, scoped, seen);
		if (value.k === 'UnaryOp') return containsComponentValue(value.operand, componentValues, scoped, seen);
		if (value.k === 'Ternary') return containsComponentValue(value.cond, componentValues, scoped, seen) || containsComponentValue(value.then, componentValues, scoped, seen) || containsComponentValue(value.else, componentValues, scoped, seen);
		if (value.k === 'Member') return containsComponentValue(value.obj, componentValues, scoped, seen);
		if (value.k === 'Index') return containsComponentValue(value.obj, componentValues, scoped, seen) || containsComponentValue(value.index, componentValues, scoped, seen);
		if (value.k === 'Assign') return containsComponentValue(value.value, componentValues, scoped, seen);
		return false;
	}
	if (Array.isArray(value)) return value.some((item) => containsComponentValue(item, componentValues, scoped, seen));
	if (value && typeof value === 'object') return Object.values(value).some((item) => containsComponentValue(item, componentValues, scoped, seen));
	return false;
}

function inspectSourceTree(value: unknown, results: BoundResult[], resultIds: Set<string>, componentValues: ReadonlySet<string>, depth = 0): void {
	if (depth > limits.depth * 3) throw new Error('Presentation source nesting is too deep.');
	if (Array.isArray(value)) { value.forEach((item) => inspectSourceTree(item, results, resultIds, componentValues, depth + 1)); return; }
	if (!value || typeof value !== 'object') return;
	if ((value as ElementNode).type === 'element') {
		const node = value as ElementNode;
		const props = node.props;
		for (const [name, prop] of Object.entries(props)) {
			if (name !== 'children' && containsComponentValue(prop, componentValues)) throw new Error('Component references may only be used as presentation children, not as computed prop values.');
		}
		// Each/component templates retain literal props as official AST Str nodes until
		// evaluation. Recognize those exact IDs; never invent or infer a missing binding.
		const resultId = typeof props.resultId === 'string' ? props.resultId : isAstNode(props.resultId) && props.resultId.k === 'Str' ? props.resultId.v : null;
		if (['Metric', 'Table', 'Chart', 'ResultFilter'].includes(node.typeName) && !resultId && !(node.typeName === 'Metric' && containsRows(props.value))) {
			throw new Error(node.typeName === 'Metric'
				? 'Metric must be bound to a supplied saved result. Use resultId, field and a zero-based row for an exact value, or derive value from @Rows("result-id"). Do not copy analytical numbers into literal value props.'
				: `${node.typeName} must be bound to a supplied saved result.`);
		}
		if (resultId) { requireResult(results, resultId); resultIds.add(resultId); }
		if ((node.typeName === 'Table' || node.typeName === 'Chart') && props.rows !== undefined) {
			if (!onlyRowsDerived(props.rows)) throw new Error(`${node.typeName} rows must derive from @Rows("result-id").`);
			const sources = new Set<string>(); rowIdsFrom(props.rows, sources);
			if (sources.size !== 1 || resultId && !sources.has(resultId)) throw new Error(`${node.typeName} display rows must come from its declared resultId.`);
		}
		if (node.typeName === 'Chart') staticChartSpec(props.spec);
		if (node.typeName === 'Metric' && props.value !== undefined && !containsRows(props.value)) throw new Error('A computed Metric must derive from supplied saved result rows.');
		if (node.typeName === 'Progress' && !containsRows(props.value)) throw new Error('Progress values must derive from supplied saved result rows.');
		for (const item of Object.values(props)) inspectSourceTree(item, results, resultIds, componentValues, depth + 1);
		return;
	}
	if (isAstNode(value)) {
		rowIdsFrom(value, resultIds);
		if (value.k === 'Comp' && Object.hasOwn(componentDefinitions, value.name)) {
			const keys = Object.keys(componentDefinitions[value.name as ComponentName].props.shape);
			const props = value.mappedProps ?? Object.fromEntries(value.args.flatMap((arg, index) => keys[index] ? [[keys[index], arg]] : []));
			inspectSourceTree({ type: 'element', typeName: value.name, props, partial: false, hasDynamicProps: true }, results, resultIds, componentValues, depth + 1);
			return;
		}
		if (value.k === 'Comp') { value.args.forEach((arg) => inspectSourceTree(arg, results, resultIds, componentValues, depth + 1)); Object.values(value.mappedProps ?? {}).forEach((item) => inspectSourceTree(item, results, resultIds, componentValues, depth + 1)); }
		else if (value.k === 'Arr') value.els.forEach((item) => inspectSourceTree(item, results, resultIds, componentValues, depth + 1));
		else if (value.k === 'Obj') value.entries.forEach(([, item]) => inspectSourceTree(item, results, resultIds, componentValues, depth + 1));
		else if (value.k === 'BinOp') { inspectSourceTree(value.left, results, resultIds, componentValues, depth + 1); inspectSourceTree(value.right, results, resultIds, componentValues, depth + 1); }
		else if (value.k === 'UnaryOp') inspectSourceTree(value.operand, results, resultIds, componentValues, depth + 1);
		else if (value.k === 'Ternary') { inspectSourceTree(value.cond, results, resultIds, componentValues, depth + 1); inspectSourceTree(value.then, results, resultIds, componentValues, depth + 1); inspectSourceTree(value.else, results, resultIds, componentValues, depth + 1); }
		else if (value.k === 'Member') inspectSourceTree(value.obj, results, resultIds, componentValues, depth + 1);
		else if (value.k === 'Index') { inspectSourceTree(value.obj, results, resultIds, componentValues, depth + 1); inspectSourceTree(value.index, results, resultIds, componentValues, depth + 1); }
		return;
	}
	for (const item of Object.values(value)) inspectSourceTree(item, results, resultIds, componentValues, depth + 1);
}

function staticChartSpec(value: unknown): unknown {
	if (isAstNode(value)) {
		switch (value.k) {
			case 'Str': return value.v;
			case 'Num': return value.v;
			case 'Bool': return value.v;
			case 'Null': return null;
			case 'Arr': return value.els.map(staticChartSpec);
			case 'Obj': return Object.fromEntries(value.entries.map(([key, item]) => [key, staticChartSpec(item)]));
			case 'BinOp': case 'UnaryOp': case 'Ternary': case 'Member': case 'Index': return evaluateStaticExpression(value, new Map());
			default: throw new Error('Chart specifications must be static and cannot be driven by executable or local-state expressions.');
		}
	}
	if (Array.isArray(value)) return value.map(staticChartSpec);
	if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, staticChartSpec(item)]));
	return value;
}

function validateAction(value: unknown, defaults: Record<string, unknown>): void {
	if (!value || typeof value !== 'object') return;
	const action = value as Record<string, unknown>;
	if (Array.isArray(action.steps)) { action.steps.forEach((step) => validateAction(step, defaults)); return; }
	if (action.type === 'set') {
		if (typeof action.target !== 'string' || !Object.hasOwn(defaults, action.target)) throw new Error('Button actions may set only a declared local state variable.');
		const ast = action.valueAST as ASTNode | undefined;
		if (!ast || !['Str', 'Num', 'Bool', 'Null', 'StateRef', 'BinOp', 'UnaryOp', 'Ternary', 'Member', 'Index'].includes(ast.k) || containsRows(ast)) throw new Error('Button state actions may assign only a simple local value.');
	} else if (action.type === 'reset') {
		if (!Array.isArray(action.targets) || action.targets.some((target) => typeof target !== 'string' || !Object.hasOwn(defaults, target))) throw new Error('Button actions may reset only declared local state variables.');
	} else if (action.type === 'continue_conversation') {
		if (typeof action.message !== 'string' || !action.message.trim() || action.message.length > 2000) throw new Error('The follow-up message is invalid or too long.');
	} else throw new Error('This OpenUI action is not permitted in an analytical presentation.');
}

/** Complete programs only. Invalid generated layouts fall back to the verified answer. */
export function validatePresentation(program: string, results: BoundResult[], persistedState: Record<string, unknown> = {}): PresentationValidation {
	const issues: PresentationIssue[] = [];
	const resultIds = new Set<string>();
	let normalizedProgram = '';
	let stateDefaults: Record<string, unknown> = {};
	let sourceRoot: ElementNode | null = null;
	try {
		const checked = preflight(program);
		normalizedProgram = checked.program;
		stateDefaults = checked.stateDefaults;
		validateResults(results);
		const parsed = createParser(presentationSchema()).parse(normalizedProgram);
		if (parsed.meta.incomplete || parsed.meta.unresolved.length) throw new Error(`The presentation is incomplete or has unresolved references${parsed.meta.unresolved.length ? `: ${parsed.meta.unresolved.join(', ')}` : '.'}`);
		if (parsed.meta.statementCount > limits.components + Object.keys(stateDefaults).length || parsed.meta.orphaned.length) throw new Error('The presentation contains unused statements or exceeds the statement budget.');
		if (parsed.meta.errors.length) {
			for (const error of parsed.meta.errors) issues.push({ path: `${error.component}${error.path}`, message: error.message });
			return { valid: false, root: null, sourceRoot: null, normalizedProgram, issues, resultIds: [], stateDefaults };
		}
		if (parsed.queryStatements.length || parsed.mutationStatements.length) throw new Error('Queries and mutations are not permitted in presentations.');
		if (!parsed.root || parsed.root.typeName !== 'Document' || parsed.root.statementId !== 'root') throw new Error('The presentation must have an explicit root = Document([...]) statement.');
		sourceRoot = parsed.root;
		stripOmittedOptionalProps(sourceRoot);
		inspectSourceTree(sourceRoot, results, resultIds, checked.componentValues);
		const selectedState: Record<string, unknown> = Object.create(null);
		for (const [name, defaultValue] of Object.entries(stateDefaults)) {
			const candidate = persistedState[name];
			selectedState[name] = typeof candidate === typeof defaultValue || candidate === null && defaultValue === null ? candidate : defaultValue;
		}
		const store = createPresentationStore(stateDefaults, selectedState);
		let evaluated: ElementNode | null = null;
		try {
			const result = evaluatePresentationTree(sourceRoot, results, store);
			if (result.errors.length) throw new Error(result.errors.join('; '));
			if (!result.root) throw new Error('The OpenUI presentation could not be evaluated.');
			evaluated = result.root;
		} finally { store.dispose(); }
		let count = 0;
		const controls = new Set<string>();
		const ancestry = new Set<unknown>();
		function walk(value: unknown, path: string, depth: number): void {
			if (++count > limits.components || depth > limits.depth) throw new Error('The presentation exceeds its component budget.');
			if (!value || typeof value !== 'object' || ancestry.has(value)) throw new Error(`${path} must be a component, not a literal or cyclic reference.`);
			ancestry.add(value);
			const node = value as ElementNode;
			if (node.type !== 'element' || node.partial || !Object.hasOwn(componentDefinitions, node.typeName)) throw new Error(`${path} uses an unknown, partial or unsupported component.`);
			const props = { ...node.props } as Record<string, unknown>;
			for (const [key, prop] of Object.entries(props)) if (prop && typeof prop === 'object' && (prop as Record<string, unknown>).__reactive === 'assign') props[key] = persistedState[(prop as Record<string, unknown>).target as string] ?? stateDefaults[(prop as Record<string, unknown>).target as string] ?? null;
			const definition = componentDefinitions[node.typeName as ComponentName];
			const check = definition.props.safeParse(props);
			if (!check.success) throw new Error(`${path}: ${check.error.issues.map((item) => item.message).join('; ')}`);
			const safeProps = check.data as Record<string, unknown>;
			if (node.typeName === 'Card') {
				safeProps.tone = normalizeCardTone(safeProps.tone);
				node.props.tone = safeProps.tone;
			}
			if (node.typeName === 'Document' && depth > 0) throw new Error('Document may only be the root.');
			if (node.typeName === 'Metric') {
				if (safeProps.value === undefined) {
					if (typeof safeProps.resultId !== 'string' || typeof safeProps.field !== 'string' || typeof safeProps.row !== 'number') throw new Error(`${path} must use an exact result row or an explicit Rows-derived value.`);
					const result = requireResult(results, safeProps.resultId); resultIds.add(result.id);
					if (!result.columns.includes(safeProps.field) || safeProps.row < 0 || safeProps.row >= result.rows.length) throw new Error(`${path} references a missing saved field or row.`);
				} else if (typeof safeProps.value !== 'string' && (typeof safeProps.value !== 'number' || !Number.isFinite(safeProps.value)) && safeProps.value !== null) throw new Error(`${path} value must be a scalar display result.`);
			}
			if (node.typeName === 'Progress' && (typeof safeProps.value !== 'number' || !Number.isFinite(safeProps.value))) throw new Error(`${path} needs a finite numeric value.`);
			if (['Table', 'Chart', 'ResultFilter'].includes(node.typeName)) {
				const result = requireResult(results, String(safeProps.resultId)); resultIds.add(result.id);
				if (node.typeName === 'Table') {
					const tableData = safeProps.rows === undefined ? result : derivedRowsResult(result, safeProps.rows as unknown[]);
					if (Array.isArray(safeProps.fields) && safeProps.fields.some((field) => !tableData.columns.includes(String(field)))) throw new Error(`${path} references a missing saved table field.`);
					if (safeProps.columnFormats && Object.keys(safeProps.columnFormats).some((field) => !tableData.columns.includes(field))) throw new Error(`${path} formats a missing table field.`);
				}
				if (node.typeName === 'ResultFilter') {
					if (!result.columns.includes(String(safeProps.field))) throw new Error(`${path} references a missing saved filter field.`);
					const index = result.columns.indexOf(String(safeProps.field));
					if (new Set(result.rows.map((row) => row[index])).size > 200) throw new Error('A display filter has too many choices; use a more focused result.');
				}
				if (node.typeName === 'Chart') {
					const chartData = safeProps.rows === undefined ? result : derivedRowsResult(result, safeProps.rows as unknown[]);
					validateChartSpec(safeProps.spec, chartData);
				}
			}
			if (['Tabs', 'ResultFilter', 'Select', 'SegmentedControl', 'Checkbox'].includes(node.typeName)) {
				const controlName = String(safeProps.name ?? safeProps.label);
				if (controls.has(controlName)) throw new Error('Presentation control names must be unique.');
				controls.add(controlName);
			}
			if (node.typeName === 'Tabs' && (safeProps.labels as unknown[]).length !== (safeProps.children as unknown[]).length) throw new Error('Each tab needs one label and one child.');
			if (node.typeName === 'Button') validateAction(safeProps.action, stateDefaults);
			if ('children' in safeProps) (safeProps.children as unknown[]).forEach((child, index) => walk(child, `${path}.children[${index}]`, depth + 1));
			ancestry.delete(value);
		}
		walk(evaluated, 'root', 0);
		for (const id of resultIds) requireResult(results, id);
		return { valid: true, root: evaluated, sourceRoot, normalizedProgram, issues, resultIds: [...resultIds], stateDefaults };
	} catch (error) {
		issues.push({ path: 'root', message: error instanceof Error ? error.message : 'The presentation could not be validated.' });
		return { valid: false, root: null, sourceRoot, normalizedProgram, issues, resultIds: [...resultIds], stateDefaults };
	}
}
