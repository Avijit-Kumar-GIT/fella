# Release runbook

This document describes the maintained Electron release. The app version and
Rust sidecar version must agree with the tag; the GitHub workflow validates
them before packaging.

## Release quality scope

The tagged release workflow runs the default Rust suite, Clippy, frontend and
Electron checks, packaged-app smoke checks, FQA fixture validation, and
cross-platform packaging. Passing those gates does not establish analytical
correctness for arbitrary user data. FQA-Bench v0.1 remains a draft; release
claims must reflect that limit. See [`TESTING.md`](../testing/TESTING.md) for
the scope and caveats.

## Tag and draft

The `v0.3.0` release migrated the app from Tauri to Electron while retaining
the `dev.fella.app` data location. Users upgrading from Tauri had to install
that Electron package manually; the Tauri updater could not install it. Later
Electron releases use the user-triggered updater.

After the v0.4.0 version and changelog commit is on `main`, check versions and
push the tag:

```sh
node scripts/check-release-version.mjs v0.4.0
git tag v0.4.0
git push origin v0.4.0
```

The tag workflow reruns quality gates, builds the platform packages, verifies
the expected six installers, computes `SHA256SUMS`, and creates a GitHub draft.
It builds macOS universal DMG/ZIP, Windows x64 NSIS/MSI, and Linux x64
AppImage/DEB packages. Review the draft artifacts, checksums, and notes before
publishing; the updater reads the latest published release, not a draft.

## Signing and updates

Published installers follow the existing unsigned distribution path. Checksums
detect a mismatched or corrupted download but do not prove publisher identity.
macOS may block an unnotarized app and Windows may show a SmartScreen warning.
Do not claim signed distribution until CI signing/notarization is configured
and verified.

In a packaged build, `/update` is user-triggered, selects the platform
artifact, verifies it against `SHA256SUMS`, and only then applies it. Linux
DEB updates require manual reinstall because replacing the package requires
package-manager privileges.

For development and package commands, see
[`DEVELOPMENT.md`](../development/DEVELOPMENT.md). For acceptance criteria and test evidence,
see [`TESTING.md`](../testing/TESTING.md).
