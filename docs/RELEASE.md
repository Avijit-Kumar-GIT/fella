# Release runbook

This document describes the maintained Electron release. The app version and
Rust sidecar version must agree with the tag; the GitHub workflow validates
them before packaging.

## v0.3.0 candidate status

The local release-gate run recorded on 2026-10-07 passed the default Rust
suite, Clippy, frontend checks/build, Electron bridge/update/chart/artifact
checks, eight Playwright release journeys, and packaged Windows/Linux startup
smokes. This records local validation only: no tag or GitHub draft was created
by that run, installers are unsigned, and no macOS GUI test was run. The latest
real-model chart-family repeat completed Luna 6/6 and Gemma 5/6 workflow turns;
Gemma's forecast chart failed after two invalid chart-tool attempts. That
journey did not grade analytical correctness. See
[`TESTING.md`](TESTING.md) for the exact scope and caveats.

Do not treat a green packaging or interaction gate as proof that the model is
correct on arbitrary analytics. FQA-Bench v0.1 remains a draft; release claims
must reflect that limit.

## Tag and draft

The Electron shell is a one-time migration from the Tauri `v0.2.0` app. The
`v0.3.0` app retains the `dev.fella.app` data location, but existing users must
install the Electron package manually; the old Tauri updater cannot install an
Electron build. Ask users to confirm settings, credentials, and history are
present before removing the old app. Subsequent Electron releases use the new
updater.

After the release commit is on `main`, check versions and push the tag:

```sh
node scripts/check-release-version.mjs v0.3.0
git tag v0.3.0
git push origin v0.3.0
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
[`DEVELOPMENT.md`](DEVELOPMENT.md). For acceptance criteria and test evidence,
see [`TESTING.md`](TESTING.md).
