// Shared Svelte transition presets so durations/easings aren't re-tuned at
// every call site. Built on svelte/transition (in-framework, no dependency).
// The global `prefers-reduced-motion` rule in app.css neutralises whatever
// these produce, so components don't each need to guard.

import { fade, fly, scale } from 'svelte/transition';
import { cubicOut } from 'svelte/easing';

/** A message / menu row arriving: rises a few px while fading in. */
export function enterUp(node: Element) {
	return fly(node, { y: 6, duration: 170, easing: cubicOut });
}

/** A panel appearing (command palette): the Spotlight/Raycast scale-in. */
export function pop(node: Element) {
	return scale(node, { start: 0.96, opacity: 0, duration: 170, easing: cubicOut });
}

/** A quick crossfade backdrops, tab-switch transcript. */
export function fadeQuick(node: Element) {
	return fade(node, { duration: 120 });
}
