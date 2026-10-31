import { getContext, setContext } from 'svelte';
import type { BoundResult, PresentationAction, PresentationState } from '../../presentation/types';
import type { FilterSelection, RowFilterValue } from '../../presentation/results';

interface PresentationContext {
	readonly results: BoundResult[];
	readonly sourceResults: BoundResult[];
	readonly replayedResultIds: Set<string>;
	readonly state: PresentationState;
	readonly interactive: boolean;
	readonly interactionScope?: string;
	readonly sourceArtifactId?: string;
	readonly sourceRevision?: number;
	update(key: string, value: unknown): void;
	setFilter(key: string, filter: FilterSelection): void;
	selectRows(resultId: string, interactionGroup: string | undefined, values: RowFilterValue[]): void;
	runAction(action: unknown): void;
	action(action: PresentationAction): void;
}
const key = Symbol('fella-analytical-presentation');
export function providePresentation(context: PresentationContext) { setContext(key, context); }
export function usePresentation(): PresentationContext { return getContext(key); }
