# Build and verify Chartr Daily on macOS

This guide describes the local **Chartr Daily** development bundle, not an
upstream release. This personal fork has only been tested on macOS; Tabs and
Chats have changed but still need proper end-to-end testing.

A successful Cargo build updates `target/debug/chartr`. It does **not** update an
installed `.app`. Staging, installation, relaunch, and visual verification are
separate actions.

## Source and prerequisites

1. Use a checkout of **this fork**, and confirm its branch, revision, and working
   tree. Follow the [source-build prerequisites](installation.md#build-from-source)
   for Rust, the macOS Command Line Tools, and the pinned Herdr sidecar. Upstream
   download packages do not contain this fork's changes.
2. From the repository root, run `sh scripts/build-daily-app.sh`. The script uses
   `cargo build --offline --locked -p chartr --bin chartr --features gpui_platform/runtime_shaders`.
   Dependencies and the pinned sidecar must already be available for the offline
   build. Runtime shader compilation avoids the build-time requirement for the
   full Xcode `metal` utility.
3. Build from committed, reviewed source when possible. Otherwise label the
   result as a dirty-tree build; a commit marker alone cannot identify uncommitted
   source.

## Stage an identifiable bundle

The script creates a fresh `target/daily-app.XXXXXX/Chartr Daily.app`, with bundle
ID `dev.chartr.daily-ui` and display name **Chartr Daily**. It checks the binary,
Herdr version, plist, signature, and recorded revision. It uses the configured
local code-signing identity when available and otherwise signs ad hoc. If
`iconutil` cannot compile the icon set, it uses the source PNG icon and reports
that fallback.

Read the script's `STAGED_APP`, `SOURCE_REVISION`, `SOURCE_TREE`, and SHA-256 output.
A failure stops before installation. The script does **not** copy the bundle to
Applications or any other installation directory, and it does not launch it.

## Install and relaunch

Installation is a separate, deliberate step:

1. Choose the exact installation path and preserve the previous app as a backup.
   Quit only the intended Daily app normally and confirm that its process exited.
   Do not terminate an unidentified Chartr process.
2. Install the verified staged bundle. Do not replace an upstream or other Chartr
   installation by accident.
3. Verify the installed plist's bundle ID, revision, and `chartrSourceTree`;
   verify its signature; and compare both executable hashes with the staged
   bundle. A mismatch means installation was not verified.
4. Launch that exact installed bundle after the old process exits. Confirm the
   running executable and inspect the UI. An `open` exit code or updated plist
   does not prove that the displayed process uses the new build.

**The Daily bundle does not have an isolated data profile.** It shares Chartr's
default settings, session history, and workspace-state locations. Use separate
`XDG_CONFIG_HOME`, `XDG_STATE_HOME`, and `XDG_DATA_HOME` directories for disposable
testing. CLI-provider data directories also need isolation: XDG overrides alone
do not relocate every provider's history. Do not delete, inspect, or overwrite
important live state to make a build or installation succeed. See [workspace data locations](workspace.md#your-data).

## Fixture-only screenshots

The `ui-lab` feature exits through a separate capture path before application
startup. It uses bare settings, generic fixture data, and offscreen GPUI windows;
it does not load the user's configuration, saved workspace, or agent histories.
See the [published screenshot provenance](assets/screenshots/README.md) for the
exact capture command and source hash.

These captures cover production chrome components around static terminal text,
in a macOS window frame drawn by the harness. They do not verify terminal input,
agent launches, recovery, or complete Tabs and Chats behavior.

## Completion record

Record **built**, **staged**, **installed**, **relaunched**, and **visually
verified** separately. Include the source revision and dirty state, build
command/result, staged bundle and executable hashes, installed identity/hash
comparison, process-exit and running-executable checks, and what was actually
observed. Do not describe a staged build as an installed or tested release.
