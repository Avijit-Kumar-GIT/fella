# Fella release runbook

This is the active release procedure. Historical release details live in
[`CHANGELOG.md`](../CHANGELOG.md) and [`DECISIONS.md`](DECISIONS.md); this file
describes the current Electron product only.

## Current candidate: 0.3.0

The previously published `v0.2.0` tag is immutable and must not be moved or
reused. The Electron shell transition is version `0.3.0`; `package.json` and
`src-tauri/Cargo.toml` carry the app and sidecar versions. The release workflow
checks both against the pushed tag before building.

### Release gate

Do not tag a production release until each item below is resolved or explicitly
accepted by the maintainer:

- [ ] `pnpm check`, `pnpm build`, bridge/update/chart tests, Rust formatting,
  Clippy, and the full default-feature Rust test suite pass.
- [ ] The G5 clarification-continuation failure in
  [`ELECTRON-VALIDATION.md`](ELECTRON-VALIDATION.md) is fixed and rerun with the
  same expected behavior. Do not weaken the case to match the current output.
- [ ] Finish the partial G3/G7 checks, including retained evidence after Stop,
  a fully framed chart in both themes, and forecast details.
- [ ] Build and launch the packaged app on Windows, macOS, and Linux, or record
  an explicit platform limitation before publication. CI compilation alone is
  not a native GUI smoke test.
- [ ] Review the live-model, performance, security, and installer-size notes
  in the validation record. Keep grader concerns and incomplete coverage
  visible in the release notes.

The web-research threat suite is not a gate for this version because web
research is not shipped. It becomes a required gate before backlog #2 enables
web access.

## Build and draft

This is a one-time shell migration from the published Tauri `v0.2.0` app.
Existing users must install the Electron `v0.3.0` package from the release
page; the old Tauri `/update` path does not upgrade across shell formats. The
Electron build retains the `dev.fella.app` data location. Ask users to confirm
their settings, credentials, and history are present before uninstalling the
old app. Subsequent Electron releases use the new `/update` path.

Releases are created by pushing a version tag after the candidate commit is on
`main`:

```bash
node scripts/check-release-version.mjs v0.3.0
git tag v0.3.0
git push origin v0.3.0
```

The workflow reruns the quality gate before creating a GitHub draft. It builds:

- macOS universal DMG and ZIP (Intel + Apple silicon)
- Windows x64 NSIS EXE and MSI
- Linux x64 AppImage and DEB

Each app package includes the matching Rust engine sidecar. The workflow then
attaches `SHA256SUMS`. Review every installer, checksum, and release note in the
draft; publish it manually only after the gate is satisfied. The updater reads
the latest published release, not a draft.

## Signing and user warnings

This release follows the prior unsigned distribution path because signing
credentials are not configured in CI. SHA-256 checksums detect a mismatched or
corrupted download but do not prove publisher identity. macOS may block an
unnotarized app until the user explicitly opens it; Windows may show a
SmartScreen warning. If Developer ID notarization or Windows signing becomes
available, add the credentials as protected CI secrets and verify signing
before changing these claims.

## Installer and updater contract

Artifact names are generated from the Electron app version and architecture in
`package.json`. `electron/update.mjs` selects the matching platform installer,
checks its entry in `SHA256SUMS`, then applies it only after the user invokes
`/update`. Linux DEB updates remain manual because replacing a package requires
package-manager privileges.

To build the current platform locally, use `pnpm electron:package`. For the
Windows development workflow and sidecar details, see
[`ELECTRON.md`](ELECTRON.md). For the test evidence and gaps, see
[`ELECTRON-VALIDATION.md`](ELECTRON-VALIDATION.md).
