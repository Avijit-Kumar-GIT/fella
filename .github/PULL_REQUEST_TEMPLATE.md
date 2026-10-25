<!--
Packs (themes, skills, MCP connectors) do not go here. They are submitted to
the fella-extensions repo. See CONTRIBUTING.md.
-->

## What changed

Brief description, and the reason for it.

## Lane

- [ ] App code (feature, fix, file format, tool, engine/UI)

## Checklist

- [ ] `cargo test --manifest-path backend/Cargo.toml --locked` passes (default SQLite features)
- [ ] `pnpm check` succeeds
- [ ] `pnpm build` succeeds
- [ ] New behavior has a focused test when it can be tested deterministically
- [ ] Commit messages follow Conventional Commits (`feat(...)`, `fix(...)`, `docs:`, ...)
- [ ] If this changes a design decision: added a dated entry to `docs/DECISIONS.md`
- [ ] If this changes something in `docs/ARCHITECTURE.md`: updated it in this PR
- [ ] Fits the non-negotiables in `CONTRIBUTING.md` (read-only, local-first, anti-bloat, not a task agent)

## How I verified it

The automated checks and manual steps you ran to verify the change.
