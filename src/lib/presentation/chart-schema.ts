import { z } from 'zod/v4';
import { PRESENTATION_LIMITS as limits } from './types';

// Only JSON-Schema-representable constraints belong here: generation and acceptance
// must describe the same language. The host additionally checks saved fields,
// repeat references, depth, total nodes/views, facet cardinality and render cost.
const text = z.string().max(4000);
const field = z.string().min(1).max(240);
const number = z.number().min(-1e15).max(1e15);
const pixels = z.number().min(0).max(1600);
const offset = z.number().min(-1600).max(1600);
const fraction = z.number().min(0).max(1);
const count = z.number().int().min(1).max(256);
const scalar = z.union([text, number, z.boolean(), z.null()]).meta({ id: 'ChartScalar' });
const scalars = z.array(scalar).max(256);
const color = z.string().max(120).regex(/^(?:#[0-9a-fA-F]{3,8}|[A-Za-z]+|(?:rgb|hsl)a?\([0-9.,%+\- /]*\))$/);
const shape = z.string().max(240).regex(/^(?:circle|square|cross|diamond|triangle(?:-up|-down|-left|-right)?|arrow|wedge|stroke|[mM][0-9A-Za-z.,+\-\s]+)$/);
const fontWeight = z.union([z.enum(['normal', 'bold', 'lighter', 'bolder']), z.number().int().min(100).max(1000)]);
const align = z.enum(['left', 'center', 'right']);
const orient = z.enum(['top', 'bottom', 'left', 'right']);
const aggregate = z.enum(['count', 'sum', 'mean', 'average', 'median', 'min', 'max', 'distinct', 'valid', 'missing', 'q1', 'q3', 'stdev', 'stdevp', 'variance', 'variancep']);
const fieldType = z.enum(['quantitative', 'temporal', 'ordinal', 'nominal', 'geojson']);
const repeatRef = z.strictObject({ repeat: z.enum(['row', 'column', 'repeat', 'layer']) });
const fieldRef = z.union([field, repeatRef]);

const localTimeUnits = [
	'year', 'quarter', 'month', 'week', 'day', 'dayofyear', 'date', 'hours', 'minutes', 'seconds', 'milliseconds',
	'yearquarter', 'yearquartermonth', 'yearmonth', 'yearmonthdate', 'yearmonthdatehours', 'yearmonthdatehoursminutes', 'yearmonthdatehoursminutesseconds',
	'yearweek', 'yearweekday', 'yearweekdayhours', 'yearweekdayhoursminutes', 'yearweekdayhoursminutesseconds', 'yeardayofyear',
	'quartermonth', 'monthdate', 'monthdatehours', 'monthdatehoursminutes', 'monthdatehoursminutesseconds',
	'weekday', 'weekdayhours', 'weekdayhoursminutes', 'weekdayhoursminutesseconds', 'dayhours', 'dayhoursminutes', 'dayhoursminutesseconds',
	'hoursminutes', 'hoursminutesseconds', 'minutesseconds', 'secondsmilliseconds'
] as const;
const timeUnitName = z.enum([...localTimeUnits, ...localTimeUnits.map((unit) => `utc${unit}` as const)]);
const binnedTimeUnits = [
	'year', 'yearquarter', 'yearquartermonth', 'yearmonth', 'yearmonthdate', 'yearmonthdatehours', 'yearmonthdatehoursminutes',
	'yearmonthdatehoursminutesseconds', 'yearweek', 'yearweekday', 'yearweekdayhours', 'yearweekdayhoursminutes', 'yearweekdayhoursminutesseconds', 'yeardayofyear'
] as const;
const timeUnit = z.union([
	timeUnitName,
	z.enum([...binnedTimeUnits.map((unit) => `binned${unit}` as const), ...binnedTimeUnits.map((unit) => `binnedutc${unit}` as const)]),
	z.strictObject({ unit: timeUnitName.optional(), step: count.optional(), maxbins: count.optional(), utc: z.boolean().optional(), binned: z.boolean().optional() })
]).meta({ id: 'ChartTimeUnit' });
const pattern = z.string().max(256);
const formatting = {
	format: z.union([pattern, z.strictObject({
		year: pattern.optional(), quarter: pattern.optional(), month: pattern.optional(), date: pattern.optional(), week: pattern.optional(), day: pattern.optional(),
		hours: pattern.optional(), minutes: pattern.optional(), seconds: pattern.optional(), milliseconds: pattern.optional()
	})]).meta({ id: 'ChartFormat' }).optional(),
	formatType: z.enum(['number', 'time']).optional()
};
const bin = z.union([z.boolean(), z.literal('binned'), z.null(), z.strictObject({
	maxbins: count.optional(), step: number.positive().optional(), steps: z.array(number.positive()).min(1).max(256).optional(),
	base: number.gt(1).optional(), divide: z.array(number.positive()).length(2).optional(), extent: z.array(number).length(2).optional(),
	anchor: number.optional(), nice: z.boolean().optional(), binned: z.boolean().optional()
})]).meta({ id: 'ChartBin' });
const sortOrder = z.enum(['ascending', 'descending']);
const sort = z.union([
	sortOrder, z.null(), scalars,
	z.string().regex(/^-?(?:x|y|x2|y2|xOffset|yOffset|theta|radius|color|fill|stroke|opacity|fillOpacity|strokeOpacity|strokeWidth|size|shape|angle|text)$/),
	z.strictObject({ field, op: aggregate.optional(), order: sortOrder.optional() }),
	z.strictObject({ op: z.literal('count'), order: sortOrder.optional() })
]).meta({ id: 'ChartSort' });
const scale = z.strictObject({
	type: z.enum(['linear', 'log', 'pow', 'sqrt', 'symlog', 'identity', 'sequential', 'time', 'utc', 'quantile', 'quantize', 'threshold', 'bin-ordinal', 'ordinal', 'point', 'band']).optional(),
	domain: z.union([scalars, z.literal('unaggregated')]).optional(), domainMin: number.optional(), domainMax: number.optional(), domainMid: number.optional(),
	zero: z.boolean().optional(), nice: z.union([z.boolean(), count, z.enum(['millisecond', 'second', 'minute', 'hour', 'day', 'week', 'month', 'year'])]).optional(),
	reverse: z.boolean().optional(), clamp: z.boolean().optional(), round: z.boolean().optional(),
	padding: pixels.optional(), paddingInner: fraction.optional(), paddingOuter: fraction.optional(),
	range: z.union([z.array(z.union([number, color, shape])).min(1).max(256), z.enum(['width', 'height', 'symbol', 'category', 'ordinal', 'ramp', 'diverging', 'heatmap'])]).optional(),
	scheme: z.string().min(1).max(120).regex(/^[A-Za-z][A-Za-z0-9_-]*$/).optional(),
	base: number.gt(1).optional(), exponent: number.positive().optional(), constant: number.positive().optional()
}).meta({ id: 'ChartScale' });
const labelStyle = {
	labelColor: color.optional(), labelFontSize: pixels.optional(), labelFontWeight: fontWeight.optional(), labelLimit: pixels.optional(),
	labelPadding: pixels.optional(), labelAngle: offset.optional(), labelAlign: align.optional(),
	title: text.nullable().optional(), titleColor: color.optional(), titleFontSize: pixels.optional(), titleFontWeight: fontWeight.optional(),
	titleAnchor: z.enum(['start', 'middle', 'end']).nullable().optional(), titlePadding: pixels.optional()
};
const axis = z.strictObject({
	...formatting, ...labelStyle, orient: orient.optional(), values: scalars.optional(), tickCount: count.optional(),
	labelOverlap: z.union([z.boolean(), z.enum(['parity', 'greedy'])]).optional(), labels: z.boolean().optional(),
	grid: z.boolean().optional(), gridColor: color.optional(), domain: z.boolean().optional(), domainColor: color.optional(),
	tickColor: color.optional(), tickSize: pixels.optional(), offset: offset.optional()
}).meta({ id: 'ChartAxis' });
const legend = z.strictObject({
	...formatting, ...labelStyle,
	orient: z.enum(['left', 'right', 'top', 'bottom', 'top-left', 'top-right', 'bottom-left', 'bottom-right', 'none']).optional(),
	type: z.enum(['symbol', 'gradient']).optional(), values: scalars.optional(), columns: z.number().int().min(0).max(6).optional(),
	direction: z.enum(['horizontal', 'vertical']).optional(), gradientLength: pixels.optional(), symbolType: shape.optional(),
	symbolSize: pixels.optional(), symbolStrokeWidth: pixels.optional(), symbolOpacity: fraction.optional(), offset: offset.optional()
}).meta({ id: 'ChartLegend' });
const header = z.strictObject({
	...formatting, ...labelStyle, labels: z.boolean().optional(),
	labelOrient: orient.optional(), titleOrient: orient.optional()
}).meta({ id: 'ChartHeader' });
const fieldSettings = {
	type: fieldType.optional(), title: text.nullable().optional(), aggregate: aggregate.optional(), bin: bin.optional(), timeUnit: timeUnit.optional(),
	sort: sort.optional(), scale: scale.nullable().optional(), axis: axis.nullable().optional(), legend: legend.nullable().optional(),
	stack: z.union([z.enum(['zero', 'normalize', 'center']), z.boolean(), z.null()]).optional(),
	bandPosition: fraction.optional(), spacing: pixels.optional(), header: header.nullable().optional(), ...formatting
};
// Only count may omit a field. Analytical constants cannot enter position, text,
// tooltip, detail or order; static constants are restricted to style channels.
const fieldEncoding = z.union([
	z.strictObject({ ...fieldSettings, field: fieldRef }),
	z.strictObject({ ...fieldSettings, aggregate: z.literal('count') })
]).meta({ id: 'ChartFieldEncoding' });
const fieldChannel = fieldEncoding.nullable().optional();
const styleChannel = <T extends z.ZodType>(value: T) => z.union([fieldEncoding, z.strictObject({ value })]).nullable().optional();
const sharedEncoding = {
	x: fieldChannel, y: fieldChannel, x2: fieldChannel, y2: fieldChannel, xOffset: fieldChannel, yOffset: fieldChannel,
	theta: fieldChannel, theta2: fieldChannel, radius: fieldChannel, radius2: fieldChannel,
	longitude: fieldChannel, latitude: fieldChannel, longitude2: fieldChannel, latitude2: fieldChannel,
	color: styleChannel(color.nullable()), fill: styleChannel(color.nullable()), stroke: styleChannel(color.nullable()),
	opacity: styleChannel(fraction), fillOpacity: styleChannel(fraction), strokeOpacity: styleChannel(fraction),
	strokeWidth: styleChannel(pixels), size: styleChannel(pixels), shape: styleChannel(shape), angle: styleChannel(offset),
	text: fieldChannel,
	tooltip: z.union([fieldEncoding, z.array(fieldEncoding).min(1).max(100), z.null(), z.literal(false)]).optional(),
	detail: z.union([fieldEncoding, z.array(fieldEncoding).min(1).max(100)]).nullable().optional(),
	order: z.union([fieldEncoding, z.array(fieldEncoding).min(1).max(100)]).nullable().optional()
};
const facetField = z.strictObject({
	field, type: z.enum(['quantitative', 'temporal', 'ordinal', 'nominal']).optional(), title: text.nullable().optional(),
	bin: bin.optional(), timeUnit: timeUnit.optional(), sort: sort.optional(), header: header.nullable().optional(),
	spacing: pixels.optional(), ...formatting
}).meta({ id: 'ChartFacetField' });
const sharedEncodingSchema = z.strictObject(sharedEncoding).meta({ id: 'ChartSharedEncoding' });
const encoding = z.strictObject({ ...sharedEncoding, row: facetField.optional(), column: facetField.optional(), facet: facetField.optional() }).meta({ id: 'ChartEncoding' });
const markType = z.enum(['arc', 'area', 'bar', 'boxplot', 'circle', 'errorband', 'errorbar', 'line', 'point', 'rect', 'rule', 'square', 'text', 'tick', 'trail']);
const markStyle = {
	color: color.nullable().optional(), fill: color.nullable().optional(), stroke: color.nullable().optional(),
	opacity: fraction.optional(), fillOpacity: fraction.optional(), strokeOpacity: fraction.optional(), strokeWidth: pixels.optional(),
	strokeDash: z.array(pixels).max(256).optional(), size: pixels.optional(), shape: shape.optional(), angle: offset.optional(),
	orient: z.enum(['horizontal', 'vertical']).optional(),
	interpolate: z.enum(['linear', 'linear-closed', 'step', 'step-before', 'step-after', 'basis', 'basis-open', 'basis-closed', 'cardinal', 'cardinal-open', 'cardinal-closed', 'bundle', 'monotone']).optional(),
	tension: fraction.optional(), clip: z.boolean().optional(), filled: z.boolean().optional(),
	cornerRadius: pixels.optional(), cornerRadiusTopLeft: pixels.optional(), cornerRadiusTopRight: pixels.optional(),
	cornerRadiusBottomLeft: pixels.optional(), cornerRadiusBottomRight: pixels.optional(), thickness: pixels.optional(),
	binSpacing: pixels.optional(), continuousBandSize: pixels.optional(), discreteBandSize: pixels.optional(),
	strokeCap: z.enum(['butt', 'round', 'square']).optional(), strokeJoin: z.enum(['miter', 'round', 'bevel']).optional(),
	font: z.string().max(240).optional(), fontSize: pixels.optional(), fontStyle: z.enum(['normal', 'italic', 'oblique']).optional(), fontWeight: fontWeight.optional(),
	align: align.optional(), baseline: z.enum(['alphabetic', 'top', 'middle', 'bottom', 'line-top', 'line-bottom']).optional(),
	dx: offset.optional(), dy: offset.optional(), limit: pixels.optional(), ellipsis: z.string().max(40).optional()
};
const overlayMark = z.strictObject(markStyle).meta({ id: 'ChartOverlayMark' });
const mark = z.union([markType, z.strictObject({
	type: markType, ...markStyle,
	point: z.union([z.boolean(), z.literal('transparent'), overlayMark]).optional(), line: z.union([z.boolean(), overlayMark]).optional()
})]).meta({ id: 'ChartMark' });
const resolution = z.enum(['shared', 'independent']);
const resolve = z.strictObject({
	scale: z.strictObject({
		x: resolution.optional(), y: resolution.optional(), xOffset: resolution.optional(), yOffset: resolution.optional(),
		theta: resolution.optional(), radius: resolution.optional(), color: resolution.optional(), fill: resolution.optional(), stroke: resolution.optional(),
		opacity: resolution.optional(), fillOpacity: resolution.optional(), strokeOpacity: resolution.optional(), strokeWidth: resolution.optional(),
		size: resolution.optional(), shape: resolution.optional(), angle: resolution.optional()
	}).optional(),
	axis: z.strictObject({ x: resolution.optional(), y: resolution.optional() }).optional(),
	legend: z.strictObject({
		color: resolution.optional(), fill: resolution.optional(), stroke: resolution.optional(), opacity: resolution.optional(),
		fillOpacity: resolution.optional(), strokeOpacity: resolution.optional(), strokeWidth: resolution.optional(), size: resolution.optional(), shape: resolution.optional()
	}).optional()
}).meta({ id: 'ChartResolve' });
const viewSettings = {
	$schema: z.enum(['https://vega.github.io/schema/vega-lite/v5.json', 'https://vega.github.io/schema/vega-lite/v6.json']).optional(),
	title: text.optional(), description: text.optional(), width: z.union([pixels, z.literal('container')]).optional(), height: z.union([pixels, z.literal('container')]).optional(),
	background: color.nullable().optional(), padding: pixels.optional(), autosize: z.enum(['pad', 'fit', 'fit-x', 'fit-y', 'none']).optional()
};
const layoutAlign = z.enum(['all', 'each', 'none']);
const layoutSettings = {
	spacing: pixels.optional(), columns: z.number().int().min(1).max(6).optional(), bounds: z.enum(['full', 'flush']).optional(),
	center: z.union([z.boolean(), z.strictObject({ row: z.boolean().optional(), column: z.boolean().optional() })]).optional(),
	align: z.union([layoutAlign, z.strictObject({ row: layoutAlign.optional(), column: layoutAlign.optional() })]).optional(),
	resolve: resolve.optional()
};
const unit = z.strictObject({ ...viewSettings, mark, encoding: encoding.optional() }).meta({ id: 'ChartUnit' });
const layerUnit = unit.extend({ encoding: sharedEncodingSchema.optional() }).meta({ id: 'ChartLayerUnit' });
const layerBase = z.strictObject({ ...viewSettings, encoding: sharedEncodingSchema.optional(), resolve: resolve.optional() });
// Explicit types are needed only at recursive edges; all setting types are
// inferred from the same schemas used for parsing and JSON Schema generation.
type LayerSpec = z.infer<typeof layerBase> & { layer: (z.infer<typeof layerUnit> | LayerSpec)[] };
const layer: z.ZodType<LayerSpec> = z.lazy(() => layerBase.extend({
	layer: z.array(z.union([layerUnit, layer])).min(1).max(limits.chartLayers - 1)
})).meta({ id: 'ChartLayer' });
const unitOrLayer = z.union([unit, layer]);
const facetMapping = z.union([
	z.strictObject({ row: facetField, column: facetField.optional() }),
	z.strictObject({ column: facetField })
]);
const repeatFields = z.array(field).min(1).max(limits.facets);
const repeatMapping = z.union([
	z.strictObject({ row: repeatFields, column: repeatFields.optional() }),
	z.strictObject({ column: repeatFields })
]);
const facetView = z.strictObject({ ...viewSettings, ...layoutSettings, facet: z.union([facetField, facetMapping]), spec: unitOrLayer });
const layerRepeatView = z.strictObject({
	...viewSettings, ...layoutSettings,
	repeat: z.strictObject({ layer: repeatFields, row: repeatFields.optional(), column: repeatFields.optional() }),
	spec: z.union([layerUnit, layer])
});
const repeatBase = z.strictObject({ ...viewSettings, ...layoutSettings, repeat: z.union([repeatFields, repeatMapping]) });
const concatBase = z.strictObject({ ...viewSettings, ...layoutSettings });

export type ChartSpec = z.infer<typeof unitOrLayer> | z.infer<typeof facetView> | z.infer<typeof layerRepeatView>
	| (z.infer<typeof repeatBase> & { spec: ChartSpec })
	| (z.infer<typeof concatBase> & ({ concat: ChartSpec[] } | { hconcat: ChartSpec[] } | { vconcat: ChartSpec[] }));

/** Authoritative declarative chart grammar. No loaders, expressions, transforms,
 * parameters, conditions, arbitrary config or unknown properties at any level.
 * Layers may inherit the parent's encoding and override individual channels. */
export const chartSchema: z.ZodType<ChartSpec> = z.lazy(() => z.union([
	unitOrLayer,
	facetView,
	layerRepeatView,
	repeatBase.extend({ spec: chartSchema }),
	concatBase.extend({ concat: z.array(chartSchema).min(1).max(limits.chartLayers - 1) }),
	concatBase.extend({ hconcat: z.array(chartSchema).min(1).max(limits.chartLayers - 1) }),
	concatBase.extend({ vconcat: z.array(chartSchema).min(1).max(limits.chartLayers - 1) })
])).meta({ id: 'ChartSpec' });
