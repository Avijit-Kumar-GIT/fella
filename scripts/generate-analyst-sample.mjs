#!/usr/bin/env node

// Build a repeatable, entirely fictional workspace for exercising Fella's
// multi-source analysis, documentation search, QA, forecasting, and lineage.
// Usage: node scripts/generate-analyst-sample.mjs <new-output-directory>

import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

const destination = process.argv[2];
if (!destination) {
	console.error('Usage: node scripts/generate-analyst-sample.mjs <new-output-directory>');
	process.exit(2);
}

const root = path.resolve(destination);
let seed = 0x4f1a2026;
const random = () => {
	seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
	return seed / 0x1_0000_0000;
};
const pick = (values) => values[Math.floor(random() * values.length)];
const number = (min, max) => min + random() * (max - min);
const money = (value) => Math.round(value * 100) / 100;
const pad = (value, width = 2) => String(value).padStart(width, '0');
const csvCell = (value) => {
	if (value === null || value === undefined) return '';
	const text = String(value);
	return /[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
};
const csv = (columns, rows, delimiter = ',') => [
	columns.join(delimiter),
	...rows.map((row) => columns.map((column) => csvCell(row[column])).join(delimiter))
].join('\n') + '\n';
const monthDate = (year, month, day = 1) => `${year}-${pad(month)}-${pad(day)}`;
const months = Array.from({ length: 36 }, (_, index) => {
	const year = 2023 + Math.floor(index / 12);
	const month = index % 12 + 1;
	return { index, year, month, key: `${year}-${pad(month)}` };
});
const regions = ['North', 'South', 'East', 'West'];
const channels = ['Organic', 'Paid Search', 'Email', 'Referral', 'Affiliate'];
const segments = ['Consumer', 'Small Business', 'Enterprise'];
const files = new Map();
const add = (relativePath, content) => files.set(relativePath, content);

const productRows = Array.from({ length: 30 }, (_, index) => {
	const id = `P${pad(index + 1, 3)}`;
	const category = ['Packs', 'Footwear', 'Apparel', 'Climbing', 'Camp Kitchen'][index % 5];
	const price = money(24 + (index % 7) * 18 + number(0, 45));
	return {
		product_id: id,
		sku: `NS-${category.slice(0, 3).toUpperCase()}-${pad(index + 11, 3)}`,
		product_name: `${['Trail', 'Summit', 'Ridge', 'Alpine', 'Canyon'][index % 5]} ${category.slice(0, -1)} ${index + 1}`,
		category,
		unit_price_usd: price,
		unit_cost_usd: money(price * number(0.31, 0.59)),
		active: index % 13 === 0 ? 'false' : 'true',
		introduced_on: monthDate(2023 + Math.floor(index / 12), index % 12 + 1, 1)
	};
});
productRows.push({ ...productRows[0], product_name: 'Trail Pack — legacy catalog row', active: 'true' });
add('catalog/products.csv', csv(Object.keys(productRows[0]), productRows));

const customerRows = Array.from({ length: 360 }, (_, index) => ({
	customer_id: `C${pad(index + 1, 5)}`,
	created_on: monthDate(2020 + Math.floor(random() * 6), Math.floor(random() * 12) + 1, Math.floor(random() * 27) + 1),
	region: regions[index % regions.length],
	segment: segments[Math.floor(index / 2) % segments.length],
	industry: ['Consumer goods', 'Technology', 'Education', 'Hospitality', 'Other'][index % 5],
	loyalty_tier: ['Standard', 'Silver', 'Gold'][index % 3],
	marketing_opt_in: index % 7 === 0 ? 'unknown' : index % 2 === 0 ? 'true' : 'false'
}));
for (const duplicate of [17, 83, 211]) {
	customerRows.push({ ...customerRows[duplicate], region: pick(regions), segment: 'Legacy segment' });
}
add('customers/customer_master.csv', csv(Object.keys(customerRows[0]), customerRows));

const campaignRows = Array.from({ length: 25 }, (_, index) => ({
	campaign_id: `MKT-${pad(index + 1, 3)}`,
	campaign_name: `${['Spring Trail', 'Gear Refresh', 'Member Week', 'Summit Launch', 'Winter Ready'][index % 5]} ${Math.floor(index / 5) + 1}`,
	channel: channels[index % channels.length],
	owner: ['Growth', 'Lifecycle', 'Brand', 'Regional'][index % 4],
	started_on: monthDate(2023 + Math.floor(index / 10), index % 12 + 1, 1),
	objective: ['Acquisition', 'Repeat purchase', 'Awareness'][index % 3]
}));
add('marketing/campaign_registry.csv', csv(Object.keys(campaignRows[0]), campaignRows));

const orders = [];
const orderItems = [];
const monthTotals = new Map(months.map((month) => [month.key, {
	gross: 0, discounts: 0, refunds: 0, tax: 0, shipping: 0, orders: 0
}]));
let lineSequence = 0;
for (const month of months) {
	for (let orderNumber = 1; orderNumber <= 34; orderNumber += 1) {
		const customer = customerRows[Math.floor(random() * 360)];
		const id = `NS-${month.year}${pad(month.month)}-${pad(orderNumber, 4)}`;
		const orderDay = Math.floor(random() * 27) + 1;
		const status = orderNumber % 47 === 0 ? 'cancelled'
			: orderNumber % 31 === 0 ? 'pending'
			: orderNumber % 19 === 0 ? 'partially_refunded'
			: orderNumber % 11 === 0 ? 'shipped'
			: 'completed';
		const channel = channels[(orderNumber + month.index) % channels.length];
		const region = orderNumber % 103 === 0 ? pick(regions) : customer.region;
		const lineCount = 1 + Math.floor(random() * 3);
		let subtotal = 0;
		let discount = 0;
		for (let line = 0; line < lineCount; line += 1) {
			const product = productRows[Math.floor(random() * 30)];
			const quantity = orderNumber % 173 === 0 && line === 0 ? '' : 1 + Math.floor(random() * 4);
			const numericQuantity = quantity === '' ? 0 : Number(quantity);
			const unitPrice = money(product.unit_price_usd * number(0.9, 1.1));
			const lineGross = money(unitPrice * numericQuantity);
			const lineDiscount = money(lineGross * number(0, 0.17));
			subtotal += lineGross;
			discount += lineDiscount;
			lineSequence += 1;
			orderItems.push({
				line_id: `L${pad(lineSequence, 7)}`,
				order_id: id,
				product_id: product.product_id,
				quantity,
				unit_price_usd: unitPrice,
				line_discount_usd: lineDiscount,
				line_gross_usd: orderNumber % 199 === 0 && line === 0 ? 'not recorded' : lineGross,
				warehouse: ['SEA', 'DEN', 'ATL', 'BOS'][Math.floor(random() * 4)]
			});
		}
		subtotal = money(subtotal);
		discount = money(discount);
		const tax = money(subtotal * number(0.035, 0.095));
		const shipping = money(subtotal > 100 ? 0 : number(5, 18));
		const order = {
			order_id: id,
			order_date: orderNumber % 229 === 0 ? '' : monthDate(month.year, month.month, orderDay),
			customer_id: orderNumber % 89 === 0 ? '' : customer.customer_id,
			sales_channel: orderNumber % 71 === 0 ? channel.toLowerCase() : channel,
			region,
			status: orderNumber % 127 === 0 ? 'Complete' : status,
			currency: 'USD',
			subtotal_usd: subtotal,
			discount_usd: discount,
			tax_usd: tax,
			shipping_usd: shipping,
			order_total_usd: money(subtotal - discount + tax + shipping),
			order_source: ['web', 'mobile', 'marketplace'][orderNumber % 3],
			extract_batch: `annual-${month.year}-r${month.month > 8 ? 2 : 1}`
		};
		orders.push(order);
		const totals = monthTotals.get(month.key);
		totals.gross += subtotal;
		totals.discounts += discount;
		totals.tax += tax;
		totals.shipping += shipping;
		totals.orders += 1;
	}
}

// Annual extracts overlap at year end; duplicate IDs are deliberate.
for (const order of [orders[310], orders[715], orders[1090]]) {
	if (order) orders.push({ ...order, extract_batch: `${order.extract_batch}-overlap` });
}
const orderColumns = Object.keys(orders[0]);
for (const year of [2023, 2024, 2025]) {
	const rows = orders.filter((order) => order.order_id.startsWith(`NS-${year}`));
	add(`commerce/orders/orders_${year}.csv`, csv(orderColumns, rows));
}
add('commerce/order_lines.csv', csv(Object.keys(orderItems[0]), orderItems));

const refunds = [];
let refundSequence = 0;
for (const order of orders.slice(0, 34 * 36)) {
	if (order.status === 'cancelled' || order.order_id.endsWith('-0034') || random() > 0.115) continue;
	refundSequence += 1;
	const orderMonth = order.order_date?.slice(0, 7) || '2025-01';
	const month = months.find((item) => item.key === orderMonth) ?? months.at(-1);
	const amount = money(Number(order.subtotal_usd) * number(0.12, 0.88));
	const refund = {
		refund_id: `RF-${pad(refundSequence, 6)}`,
		order_id: order.order_id,
		refund_date: monthDate(month.year, month.month, Math.min(28, Number(order.order_date?.slice(-2) || 1) + Math.floor(random() * 14))),
		amount_usd: amount,
		reason: ['Damaged item', 'Wrong size', 'Late delivery', 'Changed mind', 'Price adjustment'][refundSequence % 5],
		status: refundSequence % 41 === 0 ? 'pending' : 'posted'
	};
	refunds.push(refund);
	monthTotals.get(month.key).refunds += amount;
}
if (refunds[9]) refunds.push({ ...refunds[9] });
refunds.push({ refund_id: 'RF-ORPHAN-01', order_id: 'NS-202599-9999', refund_date: '2025-12-19', amount_usd: 64.25, reason: 'Manual adjustment', status: 'posted' });
add('finance/refund_transactions.csv', csv(Object.keys(refunds[0]), refunds));

const payments = [];
for (let index = 0; index < orders.length - 3; index += 1) {
	const order = orders[index];
	payments.push({
		payment_id: `PAY-${pad(index + 1, 7)}`,
		order_id: order.order_id,
		paid_at: order.order_date,
		amount_usd: order.order_total_usd,
		payment_method: ['card', 'wallet', 'gift_card', 'buy_now_pay_later'][index % 4],
		payment_status: order.status === 'cancelled' ? 'voided' : index % 37 === 0 ? 'chargeback_review' : 'settled'
	});
	if (index % 53 === 0) payments.push({
		payment_id: `PAY-SPLIT-${pad(index + 1, 5)}`,
		order_id: order.order_id,
		paid_at: order.order_date,
		amount_usd: money(Number(order.order_total_usd) * 0.4),
		payment_method: 'buy_now_pay_later',
		payment_status: 'settled'
	});
}
payments.push({ payment_id: 'PAY-ORPHAN-01', order_id: 'NS-202399-8888', paid_at: '2023-12-30', amount_usd: 118, payment_method: 'card', payment_status: 'settled' });
add('finance/payment_transactions.csv', csv(Object.keys(payments[0]), payments));

const campaignPerformance = [];
for (const month of months) {
	for (let regionIndex = 0; regionIndex < regions.length; regionIndex += 1) {
		for (let channelIndex = 0; channelIndex < channels.length; channelIndex += 1) {
			const channel = channels[channelIndex];
			const base = 180 + month.index * 8 + channelIndex * 57 + regionIndex * 33;
			campaignPerformance.push({
				month: month.key,
				region: regions[regionIndex],
				channel: month.index === 19 && channel === 'Paid Search' ? 'paid_search' : channel,
				campaign_id: `MKT-${pad(1 + ((month.index + channelIndex * 3 + regionIndex) % campaignRows.length), 3)}`,
				sessions: Math.round(base * number(34, 72)),
				clicks: Math.round(base * number(8, 22)),
				spend_usd: money(base * number(3.8, 8.4)),
				attributed_orders: Math.max(0, Math.round(base * number(0.42, 1.12))),
				attributed_revenue_usd: money(base * number(42, 105)),
				attribution_window_days: channel === 'Email' ? 1 : 7
			});
		}
	}
}
campaignPerformance[101].spend_usd = '';
campaignPerformance[233].channel = 'Search';
campaignPerformance.push({ ...campaignPerformance[82] });
add('marketing/monthly_channel_performance.csv', csv(Object.keys(campaignPerformance[0]), campaignPerformance));

const monthlyTargets = [];
for (const month of months) {
	for (const region of regions) {
		for (const channel of channels) {
			const monthOrders = 34 * (1 + month.index * 0.009);
			monthlyTargets.push({
				month: month.key,
				region,
				channel,
				target_orders: Math.round(monthOrders * number(0.18, 0.37)),
				target_revenue_usd: money(monthOrders * number(580, 920)),
				target_version: month.index >= 30 && channel === 'Affiliate' ? 'reforecast' : 'original'
			});
		}
	}
}
monthlyTargets.splice(74, 0, { ...monthlyTargets[73], target_version: 'original-copy' });
add('planning/monthly_targets.csv', csv(Object.keys(monthlyTargets[0]), monthlyTargets));

const financeRows = months.map((month) => {
	const totals = monthTotals.get(month.key);
	const gross = money(totals.gross);
	const discounts = money(totals.discounts);
	const refundsAmount = money(totals.refunds);
	const tax = money(totals.tax);
	const shipping = money(totals.shipping);
	const net = money(gross - discounts - refundsAmount);
	return {
		month: month.key,
		close_status: month.index === 34 ? 'preliminary' : 'closed',
		gross_sales_usd: gross,
		discounts_usd: discounts,
		refunds_usd: refundsAmount,
		tax_collected_usd: tax,
		shipping_collected_usd: shipping,
		net_revenue_usd: month.index === 16 ? money(net + 875) : net,
		posted_at: monthDate(month.year, month.month, month.index === 34 ? 27 : 5),
		ledger_batch: `close-${month.key}-v${month.index === 34 ? 1 : 2}`
	};
});
financeRows.push({ ...financeRows[10], close_status: 'reopened', ledger_batch: 'close-2023-11-v1' });
add('finance/monthly_close_export.csv', csv(Object.keys(financeRows[0]), financeRows));

const stockRows = [];
for (const month of months) {
	for (let productIndex = 0; productIndex < 30; productIndex += 1) {
		if (month.index === 23 && productIndex % 6 === 0) continue;
		const onHand = Math.round(number(0, 180) - (productIndex % 4 === 0 ? 18 : 0));
		stockRows.push({
			month: month.key,
			product_id: `P${pad(productIndex + 1, 3)}`,
			warehouse: ['SEA', 'DEN', 'ATL', 'BOS'][productIndex % 4],
			units_on_hand: productIndex === 4 && month.index === 7 ? -3 : Math.max(0, onHand),
			stockout_days: Math.floor(random() * (productIndex % 5 === 0 ? 15 : 5)),
			counted_on: monthDate(month.year, month.month, 28)
		});
	}
}
add('inventory/monthly_stock_snapshot.csv', csv(Object.keys(stockRows[0]), stockRows));

const ticketTopics = ['Delivery delay', 'Return request', 'Product defect', 'Billing question', 'Size exchange', 'Warranty'];
const tickets = Array.from({ length: 340 }, (_, index) => {
	const order = orders[(index * 17) % orders.length];
	const day = (index % 27) + 1;
	return {
		ticket_id: `T-${pad(index + 1, 6)}`,
		created_at: monthDate(2023 + Math.floor((index % 360) / 120), Math.floor((index % 360) / 10) % 12 + 1, day),
		customer_id: index % 109 === 0 ? '' : `C${pad(((index * 13) % 360) + 1, 5)}`,
		order_id: index % 4 === 0 ? order.order_id : '',
		topic: ticketTopics[index % ticketTopics.length],
		priority: ['low', 'normal', 'high'][index % 3],
		first_response_minutes: index % 83 === 0 ? '' : Math.round(number(4, 960)),
		resolved: index % 17 === 0 ? 'no' : 'yes',
		satisfaction: index % 9 === 0 ? '' : 1 + (index * 7) % 5
	};
});
add('support/service_tickets.csv', csv(Object.keys(tickets[0]), tickets));

const experimentRows = Array.from({ length: 900 }, (_, index) => {
	const variant = index % 2 === 0 ? 'control' : 'new_checkout';
	const converted = (index * 29 + (variant === 'new_checkout' ? 11 : 0)) % (variant === 'new_checkout' ? 7 : 8) < 2;
	return {
		experiment_id: 'EXP-CHECKOUT-2025',
		visitor_id: `V${pad(index + 1, 6)}`,
		assigned_at: monthDate(2025, 1 + (index % 12), 1 + (index % 27)),
		variant,
		device: ['desktop', 'mobile', 'tablet'][index % 3],
		region: regions[index % regions.length],
		converted: converted ? '1' : '0',
		order_value_usd: converted ? money(number(35, 460)) : '',
		assignment_source: index % 59 === 0 ? 'unknown' : 'web'
	};
});
experimentRows.push({ ...experimentRows[44] });
experimentRows[251].converted = 'yes';
add('experiments/checkout_ab_test.csv', csv(Object.keys(experimentRows[0]), experimentRows));

const surveyComments = [
	'Great fit and comfortable on a long hike.',
	'Package arrived later than the delivery estimate.',
	'The zipper failed after the second trip.',
	'Sizing chart was helpful but the sleeves run short.',
	'Easy exchange; support answered quickly.',
	'Price was higher than I expected for this material.',
	'Checkout was confusing on my phone.',
	'Would buy again if the color selection improves.',
	'Gift card balance did not match the confirmation email.',
	'Good product, but the return label link expired.'
];
const surveys = Array.from({ length: 240 }, (_, index) => ({
	response_id: `S-${pad(index + 1, 6)}`,
	response_month: months[index % months.length].key,
	customer_id: index % 71 === 0 ? '' : `C${pad(((index * 19) % 360) + 1, 5)}`,
	region: regions[index % regions.length],
	rating: index % 53 === 0 ? 'five' : 1 + (index * 11) % 5,
	comment: surveyComments[index % surveyComments.length],
	product_category: ['Packs', 'Footwear', 'Apparel', 'Climbing', 'Camp Kitchen'][index % 5]
}));
add('feedback/customer_survey.tsv', csv(Object.keys(surveys[0]), surveys, '\t'));

const events = [];
for (const order of orders.slice(0, 900)) {
	const month = order.order_date?.slice(0, 7) || '2025-01';
	events.push({
		event_id: `EV-${events.length + 1}`,
		event_type: 'order_completed',
		event_date: order.order_date,
		order_id: order.order_id,
		customer_id: order.customer_id,
		channel: order.sales_channel,
		month
	});
	if (events.length % 4 === 0) events.push({
		event_id: `EV-${events.length + 1}`,
		event_type: 'return_started',
		event_date: monthDate(Number(month.slice(0, 4)), Number(month.slice(5, 7)), 20),
		order_id: order.order_id,
		customer_id: order.customer_id,
		channel: order.sales_channel,
		month
	});
}
add('events/order_activity.ndjson', events.map((event) => JSON.stringify(event)).join('\n') + '\n');

add('docs/README.md', `# Northstar Outfitters — analyst practice workspace

This is fictional retail data generated for testing Fella. It contains no real people, customers, or businesses.

## Start here

- Commerce: three annual order extracts, order lines, and customer records.
- Catalog: products and prices.
- Finance: refunds, payment transactions, and monthly close exports.
- Marketing and planning: campaign performance, campaign registry, and monthly targets.
- Inventory, support, experiments, feedback, and event-stream data provide additional angles.
- Docs contain metric notes and operating context. Archive contains older extracts that may not match current reporting.

The workspace intentionally combines different grains, repeated and missing keys, inconsistent labels, a few malformed values, overlapping extracts, and revised records. Treat joins, definitions, source scope, and totals as things to verify—not assumptions. Some files are unsupported legacy exports and should appear as skipped inventory items.

Suggested starting Investigation: “Why did net revenue change over the most recent quarter, and which channels, products, or customer groups explain the movement?”
`);

add('docs/data_dictionary.md', `# Data dictionary (current warehouse extracts)

| File | Grain | Important fields |
|---|---|---|
| commerce/orders/orders_YYYY.csv | One order per row | order_id, order_date, customer_id, sales_channel, status, subtotal, discount, tax, shipping, order total |
| commerce/order_lines.csv | One order line per row | line_id, order_id, product_id, quantity, unit price, line discount, line gross |
| customers/customer_master.csv | Intended one customer per row | customer_id, created date, region, segment, industry, loyalty tier |
| catalog/products.csv | Intended one product per row | product_id, SKU, category, unit price, unit cost, active |
| finance/refund_transactions.csv | One refund event per row | refund_id, order_id, date, amount, reason, status |
| finance/payment_transactions.csv | One payment or installment per row | payment_id, order_id, amount, method, status |
| marketing/monthly_channel_performance.csv | Month × region × channel × campaign | sessions, clicks, spend, attributed orders and revenue |
| planning/monthly_targets.csv | Month × region × channel | target orders, target revenue, target version |
| finance/monthly_close_export.csv | Intended one close per month | gross, discounts, refunds, tax, shipping, reported net, close status |
| inventory/monthly_stock_snapshot.csv | Month × product × warehouse | units on hand and stockout days |
| support/service_tickets.csv | One support ticket per row | customer/order references, topic, response time, resolution, satisfaction |
| experiments/checkout_ab_test.csv | Intended one visitor assignment per row | variant, device, region, conversion, order value |

The warehouse documentation describes IDs as stable keys and date fields as calendar dates. Exports are not guaranteed to follow those expectations. Amount fields ending in _usd are intended to be USD. Empty cells represent unknown or unrecorded values, not zero.
`);

add('docs/metric_policy_current.md', `# Current finance metric policy — approved 2025-02

For the current commerce report, **net merchandise revenue** is merchandise value after line discounts and posted refunds:

Formula: sum(quantity × unit_price_usd − line_discount_usd) − posted refund amount_usd

Exclude cancelled orders and pending refunds. Exclude tax and shipping from merchandise revenue. Use the refund transaction date for refund-period reporting; for order-cohort reporting, attach refunds back to the original order and state that choice. Report USD as stored; do not convert currency without an approved rate table. Keep gross sales, net merchandise revenue, and cash collected as distinct measures.

Use the current policy for current reporting, but surface any conflicting definition or source discrepancy rather than silently blending them.
`);

add('docs/archived_metric_policy.md', `# Archived dashboard note — superseded 2023-09

The old dashboard called **net revenue** the order subtotal after discounts **plus tax and shipping**, less refunds recorded in the finance close. The export did not consistently exclude cancelled orders. This definition was retired when the merchandise reporting policy was approved. Do not use it for a current report without explaining the difference.
`);

add('docs/marketing_attribution.md', `# Marketing attribution notes

The monthly channel performance export uses a seven-day click window for most channels and a one-day window for Email. Attributed revenue is a channel-reporting measure and is not additive to finance revenue. Channel labels come from several campaign systems; the campaign registry is a lookup, not proof that every row has a valid campaign assignment. Sessions are not unique people. Do not use attributed orders as the canonical paid-order count.
`);

add('docs/q4_review_notes.md', `# Q4 planning notes — draft

The growth team is concerned that net revenue softened late in the year while paid acquisition spend increased. They want a channel and product explanation, not just a top-line chart. Finance asked that refunds be separated from discounts. The draft slide uses “revenue” without naming a denominator; confirm the metric policy and time window before reusing it. A few regional targets were reforecast after the original plan was approved.
`);

add('archive/2023/orders.csv', csv(
	['id', 'date', 'customer', 'channel', 'state', 'subtotal', 'discount', 'tax', 'shipping', 'total'],
	orders.filter((order) => order.order_id.startsWith('NS-202301')).slice(0, 18).map((order) => ({
		id: order.order_id,
		date: order.order_date,
		customer: order.customer_id,
		channel: order.sales_channel,
		state: order.status,
		subtotal: order.subtotal_usd,
		discount: order.discount_usd,
		tax: order.tax_usd,
		shipping: order.shipping_usd,
		total: order.order_total_usd
	}))
));
add('archive/2024/campaign_spend.csv', csv(
	['period', 'market', 'source', 'spend', 'sales'],
	campaignPerformance.filter((row) => row.month.startsWith('2024-')).slice(0, 40).map((row) => ({
		period: row.month,
		market: row.region,
		source: row.channel,
		spend: row.spend_usd,
		sales: row.attributed_revenue_usd
	}))
));
add('archive/legacy_exports/README.md', `# Archived exports

These are retained to test source discovery and duplicate basenames. The 2023 order file uses an older schema. The 2024 campaign extract has fewer dimensions than the current monthly performance table. Do not union them without checking grain, field meaning, and overlapping periods.
`);
add('archive/legacy_exports/fiscal_totals.bak', 'Legacy binary-export placeholder. Unsupported by design; inventory should mark it as skipped.\n');
add('archive/legacy_exports/warehouse_snapshot.sql', 'SELECT * FROM legacy.monthly_sales; -- unsupported SQL export; retained as an inventory skip fixture\n');

const directory = await mkdir(root, { recursive: false }).then(() => root).catch((error) => {
	if (error.code === 'EEXIST') throw new Error(`Refusing to overwrite an existing directory: ${root}`);
	throw error;
});

for (const [relativePath, contents] of files) {
	const filePath = path.join(directory, relativePath);
	await mkdir(path.dirname(filePath), { recursive: true });
	await writeFile(filePath, contents, { flag: 'wx', encoding: 'utf8' });
}

console.log(`Created ${files.size} files in ${directory}`);
console.log(`Orders: ${orders.length} rows across three annual extracts; order lines: ${orderItems.length}`);
console.log(`Additional rows: ${customerRows.length} customers, ${refunds.length} refunds, ${payments.length} payments, ${campaignPerformance.length} campaign-performance rows, ${monthlyTargets.length} targets, ${stockRows.length} stock snapshots, ${tickets.length} tickets, ${experimentRows.length} experiment assignments, ${surveys.length} survey responses, ${events.length} events.`);
