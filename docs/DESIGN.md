# Interface design

Fella's interface should feel like a considered analytical instrument, not a
dashboard or an AI-themed template. The system lives in `src/app.css`, shared
components, and `src/lib/motion.ts`; prefer those shared primitives over local
one-off treatments.

## Visual principles

- **Calm before decorative.** Remove redundant labels and controls before
  adding more visual hierarchy. Keep evidence and technical detail available
  by progressive disclosure.
- **Use the brand with purpose.** Teal (`--brand: #008080`) marks Fella's
  primary action and presence, including the user's message surface. Do not
  spread it across unrelated icons, borders, and status states.
- **Build hierarchy with type and spacing.** A border or filled surface should
  clarify selection, grouping, or elevation—not turn each element into a card.
- **Make user and Fella turns distinct.** Use the restrained teal user surface
  and a quieter, text-led Fella response. Keep labels, metadata, and evidence
  subordinate to the content.
- **Reserve status colors for meaning.** `--ok`, `--warn`, and `--err` express
  status; charts use their own series palette. Color is not the only cue.
- **Keep interaction consistent.** Reuse button, list-row, focus, loading, and
  disclosure treatments across the app.

## Theme tokens

The canonical values are in `src/app.css`; update this summary when those
values change.

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `--bg` | `#fcfcfb` | `#0e0e10` | Window canvas |
| `--bg-raised` | `#ffffff` | `#17171a` | Conversation surface |
| `--bg-inset` | `#f2f2f0` | `#1e1e22` | Recessed controls and code |
| `--text` | `#1a1a17` | `#ededec` | Primary text |
| `--text-dim` | `#605e58` | `#8f8d88` | Secondary text |
| `--brand` | `#008080` | `#008080` | Primary action and user message surface |
| `--link` | `#3a5c8a` | `#8fb0dd` | Links and keyboard focus |
| `--ok` / `--warn` / `--err` | green / ochre / red | light green / gold / coral | Status only |

The interface uses Geist and Geist Mono, a compact 14px body scale, a 4px
spacing increment, restrained 8px control radius, and a more curved 18px
conversation surface. Chart series use dedicated palette tokens so category
colors are not confused with success or failure.

## Interaction and accessibility

- Keep a visible `:focus-visible` treatment and an accessible name on every
  interactive control.
- Preserve keyboard operation and respect `prefers-reduced-motion`.
- Keep one primary action per surface; quieter actions should not compete with
  it.
- Use a shared icon component and consistent dimensions/stroke treatment.
- Check light and dark modes for contrast, chart labels, selected states, and
  disclosure affordances.
- Keep empty states focused on the next useful action; do not put provider or
  implementation mechanics in user-facing copy.

## Where to change things

- Global colors, type, spacing, radii, focus, motion, and shared primitives:
  `src/app.css`.
- Icons: `src/lib/components/Icon.svelte`.
- Motion presets: `src/lib/motion.ts`.
- Component-specific layout: the owning Svelte component, only when the
  pattern is genuinely local.

For architecture or evidence-layout changes, update
[the architecture reference](ARCHITECTURE.md) as well. The product goal is in
[Product](PRODUCT.md).
