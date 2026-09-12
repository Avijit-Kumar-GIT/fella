// Sanitizes chart SVG before it's injected into the DOM via `{@html}`.
// The SVG itself is Rust-generated (src-tauri/src/engine/chart.rs), not
// LLM-authored, so the real job here is defense-in-depth against a bug in
// that generator, not defense against hostile SVG per se: a model-supplied
// label could echo file content verbatim, and every interpolated string
// there is already XML-escaped, but this allow-list walk strips a
// <script>/on* attribute even if that had a bug. Deliberately its own path,
// not an extension of markdown.ts's `html: () => ''` rule, which exists
// specifically to keep model prose untrusted -- carving an <svg>-shaped
// hole into that would reopen it for any raw markup, not just chart output.

const ALLOWED_TAGS = new Set([
	'svg',
	'g',
	'rect',
	'line',
	'path',
	'polyline',
	'polygon',
	'circle',
	'text',
	'title'
]);

const ALLOWED_ATTRS = new Set([
	'viewbox',
	'width',
	'height',
	'xmlns',
	'x',
	'y',
	'x1',
	'y1',
	'x2',
	'y2',
	'rx',
	'ry',
	'r',
	'cx',
	'cy',
	'd',
	'points',
	'fill',
	'stroke',
	'stroke-width',
	'stroke-dasharray',
	'text-anchor',
	'font-family',
	'font-size',
	'font-weight',
	'class'
]);

/** Whether `tag` (already lowercased) may appear in a sanitized chart. */
export function isAllowedTag(tag: string): boolean {
	return ALLOWED_TAGS.has(tag);
}

/** Whether `name` (already lowercased, as `Attr.name` gives it -- a
 *  namespaced attribute like `xlink:href` comes through as the literal
 *  string `xlink:href`) may appear on a sanitized chart element. `style`
 *  and any `on*` handler are excluded categorically: the generator never
 *  emits either, so there's nothing legitimate to allow through. */
export function isAllowedAttr(name: string): boolean {
	return !name.startsWith('on') && ALLOWED_ATTRS.has(name);
}

function clean(el: Element): void {
	if (!isAllowedTag(el.tagName.toLowerCase())) {
		el.remove();
		return;
	}
	for (const attr of [...el.attributes]) {
		if (!isAllowedAttr(attr.name.toLowerCase())) {
			el.removeAttribute(attr.name);
		}
	}
	for (const child of [...el.children]) {
		clean(child);
	}
}

/** Parse `raw` as SVG and strip anything outside the allow-list, returning
 *  the sanitized markup (empty string if it doesn't parse or isn't an SVG
 *  at all). Safe to inject with `{@html}` afterward. */
export function sanitizeSvg(raw: string): string {
	if (!raw.trim()) return '';
	let doc: Document;
	try {
		doc = new DOMParser().parseFromString(raw, 'image/svg+xml');
	} catch {
		return '';
	}
	if (doc.querySelector('parsererror')) return '';
	const root = doc.documentElement;
	if (root.tagName.toLowerCase() !== 'svg') return '';
	clean(root);
	return root.outerHTML;
}
