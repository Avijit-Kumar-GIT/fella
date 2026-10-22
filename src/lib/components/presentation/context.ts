import { getContext, setContext } from 'svelte';
import type { BoundResult, PresentationAction, PresentationState } from '../../presentation/types';

interface PresentationContext {
	readonly results: BoundResult[];
	readonly state: PresentationState;
	readonly interactive: boolean;
	update(key: string, value: unknown): void;
	action(action: PresentationAction): void;
}
const key = Symbol('fella-analytical-presentation');
export function providePresentation(context: PresentationContext) { setContext(key, context); }
export function usePresentation(): PresentationContext { return getContext(key); }
