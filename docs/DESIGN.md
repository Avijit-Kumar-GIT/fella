# Fella design

One screen of rules so the UI stays consistent as it grows. The whole look lives
in `src/app.css` (tokens + shared primitives) and small per-component `<style>`
blocks. There is no component library and no CSS framework; keep it that way.

## Pillars

- **Calm and uncluttered, with room to breathe.** A lightweight macOS register
  layered warm neutrals, soft shadows, generous rounding not a dense terminal.
  When a screen feels busy, remove or combine content before restyling it.
- **One visual grammar.** The same button, the same row, the same code block
  everywhere. If you're about to write a rule that already exists in another
  component, promote it to a primitive in `app.css` instead.
- **Hierarchy from type and space, then surfaces.** Reach for weight, size, and
  spacing first. Earn a border or a filled surface only for selection, a real
  grouping, or a raised layer (the composer, menus, the palette, cards).
- **Monochrome first.** Color is `--ok` / `--warn` / `--err` / `--link` and
  nothing else. It only ever marks a state, a result, or a link, and it is
  always paired with a word or a shape never color alone.
- **One primary action per screen** (`.pill.primary`, filled). Everything else
  is a quieter `.pill` or a plain button.
- **Every interactive thing shows keyboard focus.** One ring: a soft `--link`
  glow (`:focus-visible` in `app.css`, `--focus-ring`). Inputs that hide the
  default outline get the glow, they don't just drop it.
- **Motion is gentle, short, and optional.** `--dur*` / `--ease` tokens; a
  global `prefers-reduced-motion` rule in `app.css` neutralises all of it, so
  components don't each guard.
- **Plain language.** User-facing copy names things the way the person using
  Fella would (`docs/*`, the `discoverability` rule in `CLAUDE.md`). No
  `auth.json`, `base_url`, "mcp pack" in anything on screen.

## Tokens (`src/app.css` `:root`)

**Pack-overridable** a theme pack may replace these; keep the set in sync with
`THEME_TOKEN_KEYS` in `src/lib/prefs.svelte.ts` **and**
`src-tauri/src/engine/extensions.rs`:

| | light | dark |
|---|---|---|
| `--bg` (window tint) | `#fcfcfb` | `#0e0e10` |
| `--bg-raised` (writing surface) | `#ffffff` | `#17171a` |
| `--bg-inset` (recessed fill) | `#f2f2f0` | `#1e1e22` |
| `--border` / `--border-strong` | `#e9e8e4` / `#d7d5d0` | `#26262a` / `#37373d` |
| `--text` / `--text-dim` / `--text-faint` | `#1a1a17` / `#605e58` / `#6c6a63` | `#ededec` / `#8f8d88` / `#838079` |
| `--accent` | `#1a1a17` | `#ededec` |
| `--link` | `#3a5c8a` | `#8fb0dd` |
| `--ok` / `--warn` / `--err` | `#3f7d3f` / `#855800` / `#b23b3b` | `#7cc07c` / `#d8a640` / `#e06c6c` |
| `--radius` | `8px` | |
| `--pad` (outer gutter) | `28px` | |

`--bg` and `--bg-raised` differ now: the chrome (tab strip, header, status bar)
sits on `--bg`, the transcript on `--bg-raised`. `--text-faint` still carries
real content, so it must hold WCAG AA (≥ 4.5:1) on both grounds in both themes.

**Fixed** not overridable:

- `--sans` = **Geist**, `--mono` = **Geist Mono** both bundled as variable
  woff2 under `static/fonts/` (SIL OFL; `local()` first). System stacks follow.
- Type scale: `--fs` `14` (body), `--fs-sm` `12.5` (chrome), `--fs-xs` `11.5`
  (dense data: code, tables), `--fs-lg` `15.5`, `--fs-xl` `22` (headings /
  empty state). `--lh` `1.55`. `body` carries `letter-spacing: -0.006em`.
- Spacing scale: `--space-1..-6` = 4 / 8 / 12 / 16 / 24 / 32. Anything vertical
  or inset should land on one of these.
- Radii: `--radius` `8`, `--radius-sm` `6` (buttons), `--radius-chip` `4`
  (inline code, list rows).
- Shadows: `--shadow-sm` (composer, cards, active tab), `--shadow-pop` (menus,
  palette) both soft and layered, with dark overrides. Raised surfaces also
  take a `1px --border` hairline.
- Motion: `--dur-fast` `110ms`, `--dur` `170ms`, `--ease`
  `cubic-bezier(.32,.72,0,1)`. `--focus-ring` = the `:focus-visible` glow.

## Primitives (`src/app.css`)

- `.pill` bordered low-emphasis action button. `.pill.ghost` (quieter),
  `.pill.primary` (the one filled primary per screen).
- `.rowbtn` a full-width row in a drop-up or the command palette. Selection is
  `.sel` or `aria-selected="true"`.
- `.thinking` the shared 3-dot "working" indicator; inherits `color`.
- `.rich` put on any container of rendered markdown or evidence output; it
  styles descendant `code` / `pre` / `table` / `th` / `td` once.
- `.sr-only` visually hidden, screen-reader only.
- `Icon.svelte` (`src/lib/components/`) line icons, `currentColor`,
  `aria-hidden`; add a `d` string to `ICONS` when a new one is needed.

## Layout

`src/routes/+page.svelte` is a flex column: `TabBar?` / `Header?` / `main`
(the scrolling `Transcript` on `--bg-raised`) / `Composer` / `StatusBar`. The
chrome rows are `flex: none` on `--bg`; the transcript is the only scroller. The
top band is a `data-tauri-drag-region` (moves the window). Message text is
capped to a readable column width.

## When you add UI

1. Does a token or primitive already cover it? Use it.
2. New shared look? Add it to `app.css`, not a third component copy.
3. New color? It must mean something and pair with a non-color cue.
4. New interactive element? It gets `:focus-visible`, an accessible name, and
   keyboard operation.
5. Motion goes through `--dur*` / `--ease` (or `src/lib/motion.ts` presets).
6. `npm run check` stays at 0/0; check both themes in `pnpm tauri dev`.
