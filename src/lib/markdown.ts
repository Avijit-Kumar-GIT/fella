// Render the model's prose answer as markdown. The model runs locally (or on
// a provider the user chose) there's no remote content to defend against,
// but the model could still type literal HTML or a `javascript:` link, so:
//   - raw HTML is dropped (Fella only needs markdown constructs);
//   - link/image URLs are restricted to safe schemes, so a `[x](javascript:…)`
//     from prompt-injected file content can't run even if the page CSP allows
//     inline script.
import { marked, type Tokens } from 'marked';

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
		},
		table({ header, rows }) {
			const renderCell = (cell: Tokens.TableCell, headerCell: boolean): string => {
				const align = cell.align ? ` align="${cell.align}"` : '';
				const tag = headerCell ? 'th scope="col"' : 'td';
				return `<${tag}${align} data-slot="${headerCell ? 'table-head' : 'table-cell'}">${this.parser.parseInline(cell.tokens)}</${headerCell ? 'th' : 'td'}>`;
			};
			const head = header.map((cell) => renderCell(cell, true)).join('');
			const body = rows.map((row) => `<tr data-slot="table-row">${row.map((cell) => renderCell(cell, false)).join('')}</tr>`).join('');
			return `<div class="markdown-table-scroll" data-slot="table-container" role="region" aria-label="Answer table" tabindex="0"><table data-slot="table"><thead data-slot="table-header"><tr data-slot="table-row">${head}</tr></thead><tbody data-slot="table-body">${body}</tbody></table></div>`;
		}
	}
});
marked.setOptions({ gfm: true, breaks: true });

/** Parse `text` as markdown and return safe HTML for `{@html}`. */
export function renderMarkdown(text: string): string {
	return marked.parse(text, { async: false }) as string;
}
