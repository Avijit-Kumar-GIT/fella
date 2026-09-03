// Render the model's prose answer as markdown. The model runs locally (or on
// a provider the user chose) there's no remote content to defend against,
// but the model could still type literal HTML or a `javascript:` link, so:
//   - raw HTML is dropped (Fella only needs markdown constructs);
//   - link/image URLs are restricted to safe schemes, so a `[x](javascript:…)`
//     from prompt-injected file content can't run even if the page CSP allows
//     inline script.
import { marked } from 'marked';

/** http(s), mailto, anchors, and relative paths. Anything else (javascript:,
 *  data:, vbscript:, file:, …) is dropped. */
const SAFE_HREF = /^(?:https?:\/\/|mailto:|#|\/(?!\/)|\.\.?\/)/i;
/** Images additionally allow inline `data:image/*` (cannot execute script in
 *  an <img>); remote <img> loads are still blocked by the page CSP. */
const SAFE_IMG_SRC = /^(?:https?:\/\/|\/(?!\/)|\.\.?\/|data:image\/[a-z0-9.+-]+;)/i;

const escapeAttr = (s: string): string =>
	s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);

marked.use({
	renderer: {
		html: () => '',
		link({ href, title, tokens }) {
			const text = this.parser.parseInline(tokens);
			if (!SAFE_HREF.test((href ?? '').trim())) return text;
			const t = title ? ` title="${escapeAttr(title)}"` : '';
			return `<a href="${escapeAttr(href)}"${t} rel="noopener noreferrer nofollow">${text}</a>`;
		},
		image({ href, title, text }) {
			if (!SAFE_IMG_SRC.test((href ?? '').trim())) return escapeAttr(text ?? '');
			const t = title ? ` title="${escapeAttr(title)}"` : '';
			return `<img src="${escapeAttr(href)}" alt="${escapeAttr(text ?? '')}"${t} />`;
		}
	}
});
marked.setOptions({ gfm: true, breaks: true });

/** Parse `text` as markdown and return safe HTML for `{@html}`. */
export function renderMarkdown(text: string): string {
	return marked.parse(text, { async: false }) as string;
}
