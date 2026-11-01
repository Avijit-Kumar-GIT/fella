import { mount, tick } from 'svelte';
import fixtures from './intelligent-ui-v1.json';
import type { PresentationAction, PresentationState } from '../../src/lib/presentation/types';

export interface HarnessObservation {
	readonly ready: boolean;
	readonly caseId: string;
	readonly storageKey: string;
	readonly counters: { readonly statechange: number; readonly action: number };
	readonly state: PresentationState;
	readonly savedViewerState: PresentationState;
	readonly actions: PresentationAction[];
	readonly compileerror: string | null;
	readonly queryCalls: number;
}

declare global {
	interface Window { readonly __contractHarness: HarnessObservation }
}

function freeze<T>(value: T): T {
	if (value && typeof value === 'object') {
		for (const child of Object.values(value)) freeze(child);
		Object.freeze(value);
	}
	return value;
}

const params = new URLSearchParams(location.search);
const caseId = params.get('case') ?? '';
const storageKey = `fella:ui-contract:${fixtures.fixture_version}:${caseId}`;
let observation: HarnessObservation = {
	ready: false, caseId, storageKey,
	counters: { statechange: 0, action: 0 }, state: {}, savedViewerState: {}, actions: [], compileerror: null, queryCalls: 0
};

// Only detached, deeply frozen observations are public. No mutation hook, IPC,
// model substitute or renderer substitute is installed. The query tripwire
// fails if a saved-row display interaction requests analytical replay.
Object.defineProperty(window, '__contractHarness', {
	configurable: false,
	get: () => freeze(structuredClone(observation))
});

try {
	// Dynamic imports leave compile/module failures observable to Playwright,
	// including a missing/invalid generated contract asset.
	const [{ default: Harness }, { validatePresentation }, { resolvePresentationState }] = await Promise.all([
		import('./IntelligentUIHarness.svelte'),
		import('../../src/lib/presentation/validate'),
		import('../../src/lib/presentation/results')
	]);
	const selected = fixtures.cases.find(item => item.id === caseId);
	if (!selected) throw new Error(`Unknown frozen fixture case: ${caseId}`);
	const compiled = validatePresentation(selected.input, fixtures.results);
	if (!compiled.valid || !compiled.spec) throw new Error(JSON.stringify(compiled.issues));
	const spec = compiled.spec;
	const companionId = params.get('companion');
	let companionSpec = null;
	if (companionId) {
		const companion = fixtures.cases.find(item => item.id === companionId);
		if (!companion) throw new Error(`Unknown frozen companion case: ${companionId}`);
		const accepted = validatePresentation(companion.input, fixtures.results);
		if (!accepted.valid || !accepted.spec) throw new Error(JSON.stringify(accepted.issues));
		companionSpec = accepted.spec;
	}
	const savedViewerState = JSON.parse(localStorage.getItem(storageKey) ?? '{}') as PresentationState;
	observation = {
		...observation, savedViewerState,
		state: resolvePresentationState(spec, savedViewerState, fixtures.results)
	};
	mount(Harness, {
		target: document.getElementById('contract-root')!,
		props: {
			spec, companionSpec, results: structuredClone(fixtures.results), savedViewerState,
			async onrequery() {
				observation = { ...observation, queryCalls: observation.queryCalls + 1 };
				throw new Error('Display capability cannot replay analytical queries');
			},
			onstatechange(state: PresentationState) {
				localStorage.setItem(storageKey, JSON.stringify(state));
				observation = {
					...observation, state, savedViewerState: state,
					counters: { ...observation.counters, statechange: observation.counters.statechange + 1 }
				};
			},
			onaction(action: PresentationAction) {
				observation = {
					...observation, actions: [...observation.actions, action],
					counters: { ...observation.counters, action: observation.counters.action + 1 }
				};
			}
		}
	});
	await tick();
	await new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
	observation = { ...observation, ready: true };
} catch (error) {
	observation = { ...observation, ready: true, compileerror: error instanceof Error ? error.message : String(error) };
	const failure = document.createElement('p');
	failure.setAttribute('role', 'alert');
	failure.textContent = observation.compileerror;
	document.getElementById('contract-root')?.append(failure);
}
