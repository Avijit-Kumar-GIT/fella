import assert from 'node:assert/strict';
import test from 'node:test';
import {
	formatPresentationValue,
	presentationColumnLabel
} from '../src/lib/presentation/format.ts';

// Contract criteria: unformatted values stay exact; explicit formatting uses
// decimal text, ratio-based percentages, exact BigInt grouping, and explicit
// labels only. Tests use a fixed locale to keep rendered strings deterministic.
const locale = 'en-US';

test('omitting formatting preserves exact source text, including null and large integers', () => {
	assert.deepEqual(formatPresentationValue('-3616.3799999999999'), {
		display: '-3616.3799999999999', exact: '-3616.3799999999999', formatted: false
	});
	assert.equal(formatPresentationValue('9007199254740993').display, '9007199254740993');
	assert.equal(formatPresentationValue(null).display, 'null');
});

test('currency rounds the decimal source text without exposing a binary float tail', () => {
	const value = formatPresentationValue('-3616.3799999999999', { format: 'currency', unit: 'USD' }, locale);
	assert.equal(value.display, '-$3,616.38');
	assert.equal(value.exact, '-3616.3799999999999');
	assert.equal(value.formatted, true);
});

test('number and currency rounding use decimal arithmetic', () => {
	assert.equal(formatPresentationValue('1.005', { format: 'number', decimals: 2 }, locale).display, '1.01');
	assert.equal(formatPresentationValue('-0.005', { format: 'number', decimals: 2 }, locale).display, '-0.01');
	assert.equal(formatPresentationValue('1.1234567890123', { format: 'number', decimals: 8 }, locale).display, '1.12345679');
	assert.equal(formatPresentationValue('12', { format: 'currency', unit: 'USD' }, locale).display, '$12.00');
	assert.equal(formatPresentationValue('-2.5', { format: 'currency', unit: '$' }, locale).display, '-$2.50');
	assert.equal(formatPresentationValue('12', { format: 'number' }, locale).display, '12');
});

test('percent formatting treats inputs as ratios and handles zero and null distinctly', () => {
	assert.equal(formatPresentationValue('0.043', { format: 'percent' }, locale).display, '4.3%');
	assert.equal(formatPresentationValue('0', { format: 'percent' }, locale).display, '0%');
	const empty = formatPresentationValue(null, { format: 'percent' }, locale);
	assert.equal(empty.display, '—');
	assert.equal(empty.exact, 'null');
});

test('integer strings beyond Number safe range are grouped without numeric coercion', () => {
	const exact = '900719925474099312345';
	const formatted = formatPresentationValue(exact, { format: 'number' }, locale);
	assert.equal(formatted.display, '900,719,925,474,099,312,345');
	assert.equal(formatted.exact, exact);
	assert.equal(formatPresentationValue(exact, { format: 'currency', unit: 'USD' }, locale).display, '$900,719,925,474,099,312,345.00');
});

test('custom units display without inferring semantics from field names', () => {
	assert.equal(formatPresentationValue('3.5', { format: 'number', unit: 'kg' }, locale).display, '3.5\u00a0kg');
	assert.equal(presentationColumnLabel('net_merchandise_revenue'), 'net_merchandise_revenue');
	assert.equal(presentationColumnLabel('net_merchandise_revenue', { label: 'Net merchandise revenue' }), 'Net merchandise revenue');
	assert.equal(formatPresentationValue('2025', { label: 'Year' }, locale).display, '2025');
});

test('non-numeric strings remain unchanged even when a numeric format was requested', () => {
	const value = formatPresentationValue('Northstar_2025', { format: 'currency', unit: 'USD' }, locale);
	assert.equal(value.display, 'Northstar_2025');
	assert.equal(value.exact, 'Northstar_2025');
	assert.equal(value.formatted, false);
});

test('compact notation accepts large integer values without converting through Number', () => {
	const exact = 900719925474099312345n;
	const value = formatPresentationValue(exact, { format: 'compact' }, locale);
	assert.equal(value.exact, exact.toString());
	assert.notEqual(value.display, exact.toString());
	assert.equal(value.formatted, true);
});
