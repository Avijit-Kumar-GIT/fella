import { bindChartSpec } from './chart';
import type { BoundResult } from './types';

export interface ChartView {
	finalize(): unknown;
	width(value: number): ChartView;
	resize(): ChartView;
	runAsync(): Promise<unknown>;
	toSVG(): Promise<string>;
}

/** Fella-styled, text-only tooltip handler for Vega's declarative tooltip channel. */
export function createChartTooltipHandler(host: HTMLElement) {
	const tooltip = document.createElement('div');
	tooltip.className = 'fella-chart-tooltip';
	tooltip.setAttribute('role', 'tooltip');
	tooltip.hidden = true;
	host.classList.add('fella-chart-view');

	return (_handler: unknown, event: MouseEvent, _item: unknown, value: unknown): void => {
		if (value === null || value === undefined) {
			if (tooltip.parentElement === host) tooltip.hidden = true;
			return;
		}
		// Vega initializes its renderer by clearing the container, so append only
		// after the first real datum hover rather than before View construction.
		if (tooltip.parentElement !== host) host.append(tooltip);
		const entries = value && typeof value === 'object'
			? Object.entries(value as Record<string, unknown>).slice(0, 8)
			: [['', value] as [string, unknown]];
		tooltip.replaceChildren();
		for (const [label, raw] of entries) {
			const row = document.createElement('div');
			row.className = 'fella-chart-tooltip-row';
			if (label) {
				const name = document.createElement('span');
				name.className = 'fella-chart-tooltip-label';
				name.textContent = label;
				row.append(name);
			}
			const content = document.createElement('span');
			content.className = 'fella-chart-tooltip-value';
			content.textContent = typeof raw === 'number' && Number.isFinite(raw)
				? new Intl.NumberFormat(undefined, { maximumFractionDigits: 4 }).format(raw)
				: typeof raw === 'string' ? raw : String(raw ?? '—');
			row.append(content);
			tooltip.append(row);
		}
		tooltip.hidden = false;
		const bounds = host.getBoundingClientRect();
		const tip = tooltip.getBoundingClientRect();
		const x = Number.isFinite(event?.clientX) ? event.clientX - bounds.left : 8;
		const y = Number.isFinite(event?.clientY) ? event.clientY - bounds.top : 8;
		tooltip.style.left = `${Math.max(8, Math.min(x + 12, bounds.width - tip.width - 8))}px`;
		tooltip.style.top = `${Math.max(8, Math.min(y + 12, bounds.height - tip.height - 8))}px`;
	};
}

/** Lazy shared load. No Vega runtime enters the ordinary conversation path until a chart opens. */
const load = () => Promise.all([import('vega'), import('vega-lite'), import('vega-interpreter')]);

export async function renderBoundChart(container: HTMLElement | null, spec: Record<string, unknown>, result: BoundResult, rows: number[], colors: { text: string; grid: string; accent: string; series?: string[] }, width = 600): Promise<ChartView> {
	const [vega, lite, interpreter] = await load();
	const bound = bindChartSpec(spec, result, rows);
	const composed = ['facet', 'repeat', 'concat', 'hconcat', 'vconcat'].some((key) => key in spec);
	const plotWidth = Math.max(160, Math.min(width - 70, 1200));
	// Give Vega-Lite a concrete continuous width before compilation. Otherwise
	// discrete axes create a step-derived width signal that overwrites a later
	// view.width() call, leaving valid charts just a few dozen pixels wide.
	if (!composed && (bound.width === undefined || bound.width === 'container')) bound.width = plotWidth;
	const theme = {
		background: 'transparent',
		font: 'system-ui, -apple-system, BlinkMacSystemFont, sans-serif',
		range: { category: colors.series?.length ? colors.series : [colors.accent] },
		view: { stroke: null },
		axis: { labelColor: colors.text, titleColor: colors.text, gridColor: colors.grid, domainColor: colors.grid, tickColor: colors.grid, labelFontSize: 11, titleFontSize: 11, titleFontWeight: 500 },
		legend: { labelColor: colors.text, titleColor: colors.text, titleFontWeight: 500 },
		title: { color: colors.text, fontSize: 13, fontWeight: 500, anchor: 'start' },
		mark: { color: colors.series?.[0] || colors.accent }
	};
	const logger = { warn: () => {}, info: () => {}, debug: () => {}, error: (message: unknown) => { throw new Error(`Chart compilation failed: ${String(message)}`); }, level: () => 0 };
	const compiled = lite.compile(bound as never, { config: theme as never, logger: logger as never }).spec;
	const runtime = vega.parse(compiled, undefined, { ast: true });
	const deny = async () => { throw new Error('External chart resources are disabled.'); };
	const view = new vega.View(runtime, {
		renderer: container ? 'canvas' : 'none',
		container: container ?? undefined,
		expr: interpreter.expressionInterpreter,
		loader: { load: deny, sanitize: deny },
		tooltip: container ? createChartTooltipHandler(container) : undefined,
		logLevel: vega.Error,
		hover: true
	} as never);
	try {
		// Composed/faceted charts own their child dimensions; simple charts size to the pane.
		if (!composed && typeof spec.width !== 'number') view.width(plotWidth);
		await view.runAsync();
		return view as ChartView;
	} catch (error) { view.finalize(); throw error; }
}
