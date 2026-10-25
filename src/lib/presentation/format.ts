export type PresentationFormatKind = 'number' | 'currency' | 'percent' | 'compact';

export interface PresentationFormatOptions {
	label?: string;
	format?: PresentationFormatKind;
	unit?: string;
	decimals?: number;
}

export interface FormattedPresentationValue {
	display: string;
	exact: string;
	formatted: boolean;
}

interface DecimalValue {
	negative: boolean;
	coefficient: bigint;
	scale: number;
}

const MAX_DECIMAL_INPUT_LENGTH = 1024;
const MAX_DECIMAL_SCALE = 512;

function exactText(value: unknown): string {
	if (value === null) return 'null';
	if (value === undefined) return 'undefined';
	if (typeof value === 'string') return value;
	if (typeof value === 'number') return String(value);
	if (typeof value === 'boolean' || typeof value === 'bigint') return String(value);
	try {
		return JSON.stringify(value) ?? String(value);
	} catch {
		return String(value);
	}
}

function parseDecimal(text: string): DecimalValue | undefined {
	if (text.length > MAX_DECIMAL_INPUT_LENGTH) return undefined;
	const match = /^([+-]?)(?:(\d+)(?:\.(\d*))?|\.(\d+))(?:[eE]([+-]?\d+))?$/.exec(text.trim());
	if (!match) return undefined;
	const integer = match[2] ?? '';
	const fraction = match[3] ?? match[4] ?? '';
	const exponent = Number(match[5] ?? '0');
	if (!Number.isSafeInteger(exponent) || Math.abs(exponent) > MAX_DECIMAL_SCALE) return undefined;
	const digits = `${integer}${fraction}`.replace(/^0+(?=\d)/, '') || '0';
	let coefficient: bigint;
	try {
		coefficient = BigInt(digits);
	} catch {
		return undefined;
	}
	let scale = fraction.length - exponent;
	if (scale < 0) {
		if (-scale > MAX_DECIMAL_SCALE) return undefined;
		coefficient *= 10n ** BigInt(-scale);
		scale = 0;
	}
	if (scale > MAX_DECIMAL_SCALE) return undefined;
	return { negative: match[1] === '-', coefficient, scale };
}

function roundedUnits(value: DecimalValue, decimals: number, multiplyByHundred = false): bigint {
	let coefficient = value.coefficient;
	let scale = value.scale;
	if (multiplyByHundred) scale -= 2;
	const shift = decimals - scale;
	if (shift >= 0) return coefficient * 10n ** BigInt(shift);
	const divisor = 10n ** BigInt(-shift);
	const quotient = coefficient / divisor;
	const remainder = coefficient % divisor;
	return quotient + (remainder * 2n >= divisor ? 1n : 0n);
}

function localizedDigits(digits: string, locale?: string): string {
	const formatter = new Intl.NumberFormat(locale, { useGrouping: false, maximumFractionDigits: 0 });
	const map = Array.from({ length: 10 }, (_, digit) => {
		const part = formatter.formatToParts(BigInt(digit)).find((item) => item.type === 'integer');
		return part?.value ?? String(digit);
	});
	return [...digits].map((digit) => map[Number(digit)]).join('');
}

function insertFraction(parts: Intl.NumberFormatPart[], fraction: string, locale?: string): Intl.NumberFormatPart[] {
	if (!fraction) return parts;
	let lastInteger = -1;
	for (let index = 0; index < parts.length; index += 1) {
		if (parts[index].type === 'integer') lastInteger = index;
	}
	if (lastInteger < 0) return parts;
	const decimal = new Intl.NumberFormat(locale, { useGrouping: false }).formatToParts(1.1).find((part) => part.type === 'decimal')?.value ?? '.';
	return [
		...parts.slice(0, lastInteger + 1),
		{ type: 'decimal', value: decimal },
		{ type: 'fraction', value: localizedDigits(fraction, locale) },
		...parts.slice(lastInteger + 1)
	];
}

function renderFixed(
	units: bigint,
	decimals: number,
	negative: boolean,
	locale: string | undefined,
	options: Intl.NumberFormatOptions,
	minimumFractionDigits: number
): string {
	const scale = 10n ** BigInt(decimals);
	const integer = units / scale;
	let fraction = decimals ? String(units % scale).padStart(decimals, '0') : '';
	while (fraction.length > minimumFractionDigits && fraction.endsWith('0')) fraction = fraction.slice(0, -1);
	const signedInteger = negative && integer !== 0n ? -integer : integer;
	const formatter = new Intl.NumberFormat(locale, {
		...options,
		minimumFractionDigits: 0,
		maximumFractionDigits: 0
	});
	let parts = formatter.formatToParts(signedInteger);
	if (negative) {
		const minus = new Intl.NumberFormat(locale, { signDisplay: 'always', useGrouping: false, maximumFractionDigits: 0 })
			.formatToParts(-0)
			.find((part) => part.type === 'minusSign');
		if (minus && !parts.some((part) => part.type === 'minusSign')) parts = [minus, ...parts];
	}
	parts = insertFraction(parts, fraction, locale);
	return parts.map((part) => part.value).join('');
}

function percentAffixes(locale?: string): { prefix: string; suffix: string } {
	const parts = new Intl.NumberFormat(locale, {
		style: 'percent',
		minimumFractionDigits: 0,
		maximumFractionDigits: 0
	}).formatToParts(1n);
	const numeric = parts.flatMap((part, index) => part.type === 'integer' || part.type === 'group' ? [index] : []);
	if (!numeric.length) return { prefix: '', suffix: '%' };
	return {
		prefix: parts.slice(0, numeric[0]).map((part) => part.value).join(''),
		suffix: parts.slice(numeric[numeric.length - 1] + 1).map((part) => part.value).join('')
	};
}

function compactInteger(value: DecimalValue, decimals: number, minimumFractionDigits: number, locale?: string): string {
	const integerUnits = roundedUnits(value, 0);
	const signed = value.negative && integerUnits !== 0n ? -integerUnits : integerUnits;
	const absDigits = String(integerUnits);
	if (absDigits.length < 4) {
		const units = roundedUnits(value, decimals);
		return renderFixed(units, decimals, value.negative, locale, {}, minimumFractionDigits);
	}
	// Intl accepts BigInt directly, retaining all integer digits without passing
	// through IEEE-754. Compact notation is a display approximation only.
	return new Intl.NumberFormat(locale, {
		notation: 'compact',
		maximumFractionDigits: decimals,
		minimumFractionDigits
	}).format(signed);
}

function validCurrency(unit: string, locale?: string): boolean {
	if (!/^[A-Za-z]{3}$/.test(unit)) return false;
	try {
		new Intl.NumberFormat(locale, { style: 'currency', currency: unit.toUpperCase() });
		return true;
	} catch {
		return false;
	}
}

function explicitFormat(config?: PresentationFormatOptions): boolean {
	return Boolean(config?.format || config?.unit || config?.decimals !== undefined);
}

function normalizedDecimals(config: PresentationFormatOptions, kind: PresentationFormatKind, locale?: string): { min: number; max: number } {
	if (Number.isInteger(config.decimals) && config.decimals! >= 0 && config.decimals! <= 8) {
		return { min: config.decimals!, max: config.decimals! };
	}
	if (kind === 'currency' && config.unit && validCurrency(config.unit, locale)) {
		const currency = new Intl.NumberFormat(locale, { style: 'currency', currency: config.unit.toUpperCase() }).resolvedOptions();
		return { min: currency.minimumFractionDigits ?? 2, max: currency.maximumFractionDigits ?? 2 };
	}
	if (kind === 'currency') return { min: 2, max: 2 };
	return kind === 'compact' ? { min: 0, max: 1 } : { min: 0, max: 2 };
}

function formatNumeric(value: DecimalValue, config: PresentationFormatOptions, locale?: string): string {
	const kind: PresentationFormatKind = config.format ?? 'number';
	const { min, max } = normalizedDecimals(config, kind, locale);
	if (kind === 'compact') return compactInteger(value, max, min, locale);

	const percent = kind === 'percent';
	const units = roundedUnits(value, max, percent);
	const currencyCode = kind === 'currency' && config.unit && validCurrency(config.unit, locale)
		? config.unit.toUpperCase()
		: undefined;
	const style: Intl.NumberFormatOptions = currencyCode
		? { style: 'currency', currency: currencyCode }
		: {};
	let rendered = renderFixed(units, max, value.negative, locale, style, min);
	if (percent) {
		// Use the locale's own percent placement/spacing around a number rendered
		// from exact decimal parts; do not let Intl multiply a second time.
		const affixes = percentAffixes(locale);
		rendered = `${affixes.prefix}${renderFixed(units, max, value.negative, locale, {}, min)}${affixes.suffix}`;
	}
	if (kind === 'currency' && config.unit && !currencyCode) {
		const symbol = config.unit;
		const prefix = /^[\p{Sc}]$/u.test(symbol) || /^[\p{Sc}]\s*$/u.test(symbol);
		if (prefix) {
			const minus = new Intl.NumberFormat(locale, { useGrouping: false }).formatToParts(-1n).find((part) => part.type === 'minusSign')?.value ?? '-';
			rendered = rendered.startsWith(minus)
				? `${minus}${symbol}${rendered.slice(minus.length)}`
				: `${symbol}${rendered}`;
		} else rendered = `${rendered}\u00a0${symbol}`;
	} else if (config.unit && !percent && !currencyCode) {
		rendered = `${rendered}\u00a0${config.unit}`;
	} else if (config.unit && percent && config.unit !== '%') {
		rendered = `${rendered}\u00a0${config.unit}`;
	}
	return rendered;
}

/** Format only when the model supplied an explicit format, unit, or precision. */
export function formatPresentationValue(
	value: unknown,
	config?: PresentationFormatOptions,
	locale?: string
): FormattedPresentationValue {
	const exact = exactText(value);
	if (!explicitFormat(config)) return { display: exact, exact, formatted: false };
	if (value === null || value === undefined) return { display: '—', exact, formatted: true };
	const decimal = parseDecimal(exact);
	if (!decimal || !config) return { display: exact, exact, formatted: false };
	try {
		return { display: formatNumeric(decimal, config, locale), exact, formatted: true };
	} catch {
		return { display: exact, exact, formatted: false };
	}
}

export function presentationColumnLabel(field: string, config?: PresentationFormatOptions): string {
	return config?.label?.trim() || field;
}
