# Repository documentation

These pages are internal product, architecture, design, and contributor
references. The user-facing Mintlify site lives in [site/](site/).

## Product

- [Product](product/PRODUCT.md) — current capabilities and boundaries
- [Product model](product/PRODUCT-MODEL.md) — object ownership, data scope, and retention
- [Interaction model](product/INTERACTION-MODEL.md) — Chat, Notebook, sidebar, and Investigation navigation
- [Product vision](product/PRODUCT-VISION.md) — product intent and principles
- [Positioning](product/POSITIONING.md) — product narrative and claim boundaries
- [Roadmap](product/ROADMAP.md) — current open priorities

## Architecture and design

- [Architecture](architecture/ARCHITECTURE.md) — Electron, Svelte, Rust, analytical turns, and generated presentations
- [Analysis tool contracts](architecture/ANALYSIS-TOOL-CONTRACTS.md) — bounds, truncation, revision binding, and evidence
- [Interface design](design/DESIGN.md) — tokens, shared components, and layout rules

## Contribution and operations

- [Development](development/DEVELOPMENT.md) — setup and contributor conventions
- [Testing](testing/TESTING.md) — focused checks, end-to-end journeys, and model evaluation
- [Performance](testing/PERFORMANCE.md) — measurement methods and diagnostics
- [Release](operations/RELEASE.md) — packaging and release procedure
- [Docs deployment](operations/DOCS-DEPLOYMENT.md) — Mintlify hosting

## Audits and dated evidence

- [Desktop audit, 2026-10-14](audits/FELLA-DESKTOP-AUDIT-2026-10-14.md) — product and UI review with dated evidence
- [Versioned interaction and evaluation records](testing/) — preserve the acceptance criteria, failures, and limits for each recorded run
- [Benchmarks](../bench/) — canonical evaluation datasets and protocols

When a dated report conflicts with a current product or architecture page, use
the current page for present behavior and treat the dated report as historical
evidence.
