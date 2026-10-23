import type { PresentationState } from './types';

export type SafePresentationAction =
	| { type: 'follow_up'; message: string }
	| { type: 'set_state'; stateKey: string; value: string | number | boolean | null };

/** Only declarative local state changes and explicit user-requested follow-ups are supported. */
export function evaluatePresentationAction(action: unknown): SafePresentationAction | null {
	if (!action || typeof action !== 'object' || Array.isArray(action)) return null;
	const value = action as Record<string, unknown>;
	if (value.type === 'follow_up' && typeof value.message === 'string' && value.message.trim()) {
		return { type: 'follow_up', message: value.message.trim().slice(0, 2000) };
	}
	if (value.type === 'set_state' && typeof value.stateKey === 'string' && /^[A-Za-z][A-Za-z0-9_.-]{0,79}$/.test(value.stateKey)) {
		const next = value.value;
		if (next === null || typeof next === 'boolean' || typeof next === 'string' && next.length <= 256 || typeof next === 'number' && Number.isFinite(next) && Math.abs(next) <= 1e12) {
			return { type: 'set_state', stateKey: value.stateKey, value: next as string | number | boolean | null };
		}
	}
	return null;
}

export function setPresentationValue(state: PresentationState, key: string, value: unknown): PresentationState {
	if (!/^[A-Za-z][A-Za-z0-9_.-]{0,79}$/.test(key)) return state;
	if (!(value === null || typeof value === 'boolean' || typeof value === 'string' && value.length <= 256 || typeof value === 'number' && Number.isFinite(value) && Math.abs(value) <= 1e12)) return state;
	return { ...state, [`control:${key}`]: value };
}
