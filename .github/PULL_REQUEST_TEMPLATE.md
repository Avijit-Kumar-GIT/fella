<!--
Packs (themes, skills, MCP connectors) do not go here. They are submitted to
the fella-extensions repo. See CONTRIBUTING.md.
-->

## What changed

Brief description, and the reason for it.

## Lane

- [ ] App code (feature, fix, file format, tool, engine/UI)

## Checklist

- [ ] `cargo test` passes from `src-tauri/` (SQLite default features, not `--features duckdb`)
- [ ] `npm run check` is 0 errors / 0 warnings
- [ ] `npm run build` succeeds
- [ ] New behaviour has a test
- [ ] Commit messages follow Conventional Commits (`feat(...)`, `fix(...)`, `docs:`, ...)
- [ ] If this changes a design decision: added a dated entry to `docs/DECISIONS.md`
- [ ] If this changes something in `docs/ARCHITECTURE.md`: updated it in this PR
- [ ] Fits the non-negotiables in `CONTRIBUTING.md` (read-only, local-first, anti-bloat, not a task agent)

## How I verified it

The manual steps you ran, if any, beyond the automated checks.
