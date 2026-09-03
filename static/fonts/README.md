# Bundled fonts

`Geist-Variable.woff2` and `GeistMono-Variable.woff2` are **Geist** and
**Geist Mono** by Vercel, distributed under the **SIL Open Font License 1.1**
(<https://github.com/vercel/geist-font>, `LICENSE.txt` in that repo).

They are vendored (not an npm dependency) so the app has no network font fetch
local-first. Loaded from `src/app.css` via `@font-face` with `local()` first, so
a system-installed Geist is used when present.
