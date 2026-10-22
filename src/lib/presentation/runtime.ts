import {
	BUILTINS,
	LAZY_BUILTINS,
	createStore,
	evaluate,
	evaluateElementProps,
	type ASTNode,
	type ElementNode,
	type EvalContext,
	type EvaluationContext,
	type Store
} from '@openuidev/lang-core';
import { analyticalLibrary } from './library';
import { PRESENTATION_FUNCTIONS } from './functions';
import { requireResult } from './results';
import type { BoundResult, PresentationState } from './types';
import { PRESENTATION_LIMITS } from './types';

const MAX_EACH_ITEMS = PRESENTATION_LIMITS.components;
const unsafeKeys = new Set(['__proto__', 'prototype', 'constructor']);
type PresentationEvalContext = EvaluationContext & { store: Store };

export function createPresentationStore(defaults: Record<string, unknown>, persisted: PresentationState = {}): Store {
	const store = createStore();
	const initial: Record<string, unknown> = Object.create(null);
	for (const [name, fallback] of Object.entries(defaults)) {
		const value = persisted[name];
		if (typeof value === typeof fallback || value === null && fallback === null) initial[name] = value;
	}
	store.initialize(defaults, initial);
	return store;
}

export interface EvaluatedPresentation {
	root: ElementNode | null;
	errors: string[];
}

function toAst(value: unknown, budget: { nodes: number }, depth = 0): ASTNode {
	if (++budget.nodes > PRESENTATION_LIMITS.cells + PRESENTATION_LIMITS.components || depth > PRESENTATION_LIMITS.depth * 3) throw new Error('A display expression exceeded its materialization budget.');
	if (value === null || value === undefined) return { k: 'Null' };
	if (typeof value === 'string') return { k: 'Str', v: value.slice(0, PRESENTATION_LIMITS.text) };
	if (typeof value === 'number') {
		if (!Number.isFinite(value) || Math.abs(value) > 1e15) throw new Error('Display calculations must produce finite, bounded numbers.');
		return { k: 'Num', v: value };
	}
	if (typeof value === 'boolean') return { k: 'Bool', v: value };
	if (Array.isArray(value)) {
		if (value.length > PRESENTATION_LIMITS.rowsPerResult) throw new Error('A display list exceeds the saved-result row limit.');
		return { k: 'Arr', els: value.map((item) => toAst(item, budget, depth + 1)) };
	}
	if (typeof value === 'object') {
		const entries = Object.entries(value).filter(([key]) => !unsafeKeys.has(key)).slice(0, 200);
		return { k: 'Obj', entries: entries.map(([key, item]) => [key, toAst(item, budget, depth + 1)]) };
	}
	throw new Error('Unsupported value in a display expression.');
}

function valueFromAst(node: ASTNode): unknown {
	switch (node.k) {
		case 'Str': return node.v;
		case 'Num': return node.v;
		case 'Bool': return node.v;
		case 'Null': return null;
		case 'Arr': return node.els.map(valueFromAst);
		case 'Obj': return Object.fromEntries(node.entries.filter(([key]) => !unsafeKeys.has(key)).map(([key, value]) => [key, valueFromAst(value)]));
		default: return undefined;
	}
}

function substituteReference(node: ASTNode, name: string, value: unknown, budget: { nodes: number }): ASTNode {
	if (++budget.nodes > PRESENTATION_LIMITS.components * 32) throw new Error('A repeated display expression is too complex.');
	if (node.k === 'Ref' && node.n === name) return toAst(value, budget);
	switch (node.k) {
		case 'Member': return { ...node, obj: substituteReference(node.obj, name, value, budget) };
		case 'Index': return { ...node, obj: substituteReference(node.obj, name, value, budget), index: substituteReference(node.index, name, value, budget) };
		case 'BinOp': return { ...node, left: substituteReference(node.left, name, value, budget), right: substituteReference(node.right, name, value, budget) };
		case 'UnaryOp': return { ...node, operand: substituteReference(node.operand, name, value, budget) };
		case 'Ternary': return { ...node, cond: substituteReference(node.cond, name, value, budget), then: substituteReference(node.then, name, value, budget), else: substituteReference(node.else, name, value, budget) };
		case 'Arr': return { ...node, els: node.els.map((item) => substituteReference(item, name, value, budget)) };
		case 'Obj': return { ...node, entries: node.entries.map(([key, item]) => [key, substituteReference(item, name, value, budget)]) };
		case 'Comp': return { ...node, args: node.args.map((item) => substituteReference(item, name, value, budget)), ...(node.mappedProps ? { mappedProps: Object.fromEntries(Object.entries(node.mappedProps).map(([key, item]) => [key, substituteReference(item, name, value, budget)])) } : {}) };
		case 'Assign': return { ...node, value: substituteReference(node.value, name, value, budget) };
		default: return node;
	}
}

function rowSource(result: BoundResult, store: Store): unknown[] {
	const filters = Object.entries(store.getSnapshot()).flatMap(([key, value]) => {
		if (!key.startsWith('filter:') || !value || typeof value !== 'object') return [];
		const filter = value as Record<string, unknown>;
		return filter.resultId === result.id && typeof filter.field === 'string' && typeof filter.value === 'string' ? [filter] : [];
	});
	return result.rows.flatMap((row, rowIndex) => {
		const matches = filters.every((filter) => {
			const column = result.columns.indexOf(String(filter.field));
			return column >= 0 && result.exact_rows[rowIndex]?.[column] === filter.value;
		});
		return matches ? [Object.fromEntries(result.columns.flatMap((column, index) => unsafeKeys.has(column) ? [] : [[column, row[index]]]))] : [];
	});
}

function transformAst(node: ASTNode, results: BoundResult[], context: PresentationEvalContext, budget: { nodes: number }): ASTNode {
	if (++budget.nodes > PRESENTATION_LIMITS.chartNodes + PRESENTATION_LIMITS.components * 32) throw new Error('The presentation expression exceeds its evaluation budget.');
	if (node.k === 'Comp') {
		if (node.mappedProps) {
			const mappedProps = Object.fromEntries(Object.entries(node.mappedProps).map(([key, value]) => [key, transformAst(value, results, context, budget)]));
			return { ...node, mappedProps };
		}
		if (node.name === 'Rows') {
			const id = valueFromAst(node.args[0]);
			if (typeof id !== 'string') throw new Error('Rows() needs a literal saved result ID.');
			const bound = requireResult(results, id);
			return toAst(rowSource(bound, context.store), budget);
		}
		if (node.name === 'Each') {
			if (node.args.length !== 3) throw new Error('Each() needs a list, a row name, and a component template.');
			const source = transformAst(node.args[0], results, context, budget);
			const rows = evaluate(source, context);
			if (!Array.isArray(rows)) return { k: 'Arr', els: [] };
			const variable = node.args[1].k === 'Ref' ? node.args[1].n : node.args[1].k === 'Str' ? node.args[1].v : '';
			if (!variable) throw new Error('Each() needs a valid row variable name.');
			if (rows.length > MAX_EACH_ITEMS) throw new Error(`Each() can render up to ${MAX_EACH_ITEMS} items. Use Top() or Filter() to narrow the saved rows.`);
			const template = node.args[2];
			return { k: 'Arr', els: rows.map((row) => transformAst(substituteReference(template, variable, row, budget), results, context, budget)) };
		}
		const args = node.args.map((arg) => transformAst(arg, results, context, budget));
		const fn = PRESENTATION_FUNCTIONS[node.name];
		if (fn) {
			const values = args.map((arg) => evaluate(arg, context));
			const result = fn(...values);
			return toAst(result, budget);
		}
		return { ...node, args };
	}
	if (node.k === 'Arr') return { ...node, els: node.els.map((item) => transformAst(item, results, context, budget)) };
	if (node.k === 'Obj') return { ...node, entries: node.entries.map(([key, item]) => [key, transformAst(item, results, context, budget)]) };
	if (node.k === 'BinOp') return { ...node, left: transformAst(node.left, results, context, budget), right: transformAst(node.right, results, context, budget) };
	if (node.k === 'UnaryOp') return { ...node, operand: transformAst(node.operand, results, context, budget) };
	if (node.k === 'Ternary') return { ...node, cond: transformAst(node.cond, results, context, budget), then: transformAst(node.then, results, context, budget), else: transformAst(node.else, results, context, budget) };
	if (node.k === 'Member') return { ...node, obj: transformAst(node.obj, results, context, budget) };
	if (node.k === 'Index') return { ...node, obj: transformAst(node.obj, results, context, budget), index: transformAst(node.index, results, context, budget) };
	return node;
}

function transformTree(value: unknown, results: BoundResult[], context: PresentationEvalContext, budget: { nodes: number }): unknown {
	if (Array.isArray(value)) return value.map((item) => transformTree(item, results, context, budget));
	if (!value || typeof value !== 'object') return value;
	if ((value as ElementNode).type === 'element') {
		const node = value as ElementNode;
		return { ...node, props: Object.fromEntries(Object.entries(node.props).map(([key, item]) => [key, transformTree(item, results, context, budget)])) };
	}
	if (typeof (value as ASTNode).k === 'string') return transformAst(value as ASTNode, results, context, budget);
	return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, transformTree(item, results, context, budget)]));
}

function safeContext(store: Store): PresentationEvalContext {
	return {
		store,
		getState: (name) => store.get(name),
		resolveRef: () => undefined
	};
}

export function evaluatePresentationTree(root: ElementNode, results: BoundResult[], store: Store): EvaluatedPresentation {
	try {
		const context = safeContext(store);
		const transformed = transformTree(root, results, context, { nodes: 0 }) as ElementNode;
		const runtimeErrors: string[] = [];
		const evaluationContext: EvalContext = { ctx: context, library: analyticalLibrary, store, errors: [] };
	const flattenComponentChildren = (element: ElementNode): ElementNode => {
			const props = { ...element.props };
			if (Array.isArray(props.children)) {
				const flatten = (items: unknown[]): unknown[] => items.flatMap((item) => {
					if (Array.isArray(item)) return flatten(item);
					if (item && typeof item === 'object' && (item as ElementNode).type === 'element') return [flattenComponentChildren(item as ElementNode)];
					return [item];
				});
				props.children = flatten(props.children);
			}
			return { ...element, props };
		};
		const rendered = flattenComponentChildren(evaluateElementProps(transformed, evaluationContext));
		for (const error of evaluationContext.errors ?? []) runtimeErrors.push(error.message);
		return { root: rendered, errors: runtimeErrors };
	} catch (cause) {
		return { root: null, errors: [cause instanceof Error ? cause.message : 'The display expression could not be evaluated.'] };
	}
}

export function evaluatePresentationAction(action: unknown, defaults: Record<string, unknown>, store: Store): { followUp?: string; error?: string } {
	if (!action || typeof action !== 'object') return { error: 'This action is not available.' };
	const record = action as Record<string, unknown>;
	const context = safeContext(store);
	const applyStep = (step: unknown): { followUp?: string; error?: string } => {
		if (!step || typeof step !== 'object') return {};
		const item = step as Record<string, unknown>;
		if (item.type === 'set' && typeof item.target === 'string' && item.target.startsWith('$')) {
			if (!Object.hasOwn(defaults, item.target)) return { error: 'The button cannot update undeclared presentation state.' };
			try {
				const value = evaluate(item.valueAST as ASTNode, context);
				const fallback = defaults[item.target];
				if (!(typeof value === typeof fallback || value === null && fallback === null) || typeof value === 'string' && value.length > 256 || typeof value === 'number' && (!Number.isFinite(value) || Math.abs(value) > 1e12) || value !== null && !['string', 'number', 'boolean'].includes(typeof value)) return { error: 'The button value does not match the declared local state type.' };
				store.set(item.target, value); return {};
			}
			catch { return { error: 'The local selection could not be updated.' }; }
		}
		if (item.type === 'reset' && Array.isArray(item.targets)) {
			for (const target of item.targets) if (typeof target === 'string' && target.startsWith('$') && Object.hasOwn(defaults, target)) store.set(target, defaults[target]);
			return {};
		}
		if (item.type === 'continue_conversation' && typeof item.message === 'string' && item.message.trim()) return { followUp: item.message.slice(0, 2000) };
		return { error: 'This action is not permitted in an analytical presentation.' };
	};
	if (Array.isArray(record.steps)) {
		for (const step of record.steps) {
			const result = applyStep(step);
			if (result.error || result.followUp) return result;
		}
		return {};
	}
	return applyStep(record);
}

export function hasAllowedDisplayFunction(name: string): boolean {
	return name === 'Rows' || Object.hasOwn(PRESENTATION_FUNCTIONS, name) || Object.hasOwn(BUILTINS, name) || LAZY_BUILTINS.has(name);
}
