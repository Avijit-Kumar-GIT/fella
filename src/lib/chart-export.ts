export type ExportCell = string | number | boolean | null | undefined;
export type ExportDelimiter = ',' | '\t';

function escapeCell(value: ExportCell, delimiter: ExportDelimiter): string {
	if (value === null || value === undefined) return '';

	let text = String(value);
	// Spreadsheet applications may evaluate untrusted text as a formula.
	if (typeof value === 'string' && /^[\s]*[=+\-@]/.test(value)) text = `'${text}`;

	if (text.includes(delimiter) || /["\r\n]/.test(text)) {
		return `"${text.replaceAll('"', '""')}"`;
	}
	return text;
}

export function serializeDelimited(
	headings: ExportCell[],
	rows: ExportCell[][],
	delimiter: ExportDelimiter
): string {
	return [headings, ...rows]
		.map((row) => row.map((value) => escapeCell(value, delimiter)).join(delimiter))
		.join('\r\n');
}

export function chartExportFilename(title: string, extension: 'png' | 'csv'): string {
	const name = title
		.normalize('NFKD')
		.replace(/[\u0300-\u036f]/g, '')
		.replace(/[^a-zA-Z0-9 _-]/g, '')
		.trim()
		.replace(/[\s_-]+/g, '-')
		.replace(/^-+|-+$/g, '')
		.slice(0, 80);

	return `${name || 'chart'}.${extension}`;
}
