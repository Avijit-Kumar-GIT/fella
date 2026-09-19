# Provider brand assets

These files are bundled for the Ask composer’s model indicator. They are used
only to identify the provider currently selected in Fella; they are not Fella
branding and do not imply sponsorship or endorsement.

- `openai.svg` — Simple Icons asset served by Iconify:
  <https://api.iconify.design/simple-icons/openai.svg>. Simple Icons is
  CC0-1.0: <https://github.com/simple-icons/simple-icons>.
- `vercel-light.svg` and `vercel-dark.svg` — Vercel’s icon assets:
  <https://vercel.com/geist/brands>.
- `xai.ico` — the favicon served by xAI:
  <https://x.ai/favicon.ico>. Brand terms:
  <https://x.ai/legal/brand-guidelines>.
- `ollama.svg` — Ollama’s repository logo:
  <https://github.com/ollama/ollama/blob/main/docs/ollama-logo.svg>.
- `openrouter.svg` — OpenRouter’s official glyph:
  <https://openrouter.ai/brand>.

Provider trademarks remain the property of their respective owners. Keep these
marks paired with the accurate provider name and update them if the providers
change their public assets or usage terms.

`ProviderIcon.svelte` selects the appropriate bundled mark for Fella's
appearance setting. Where an official source is only available in one contrast
(currently the OpenAI, Ollama, and xAI marks), the UI applies a contrast-only
filter so the mark remains legible on both built-in surfaces; no remote asset is
requested at runtime.
