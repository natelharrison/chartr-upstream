# Screenshot provenance

These images were generated from the fixture-only
[`ui-lab` capture source](../../../crates/chartr/src/ui_lab.rs), not from a live
workspace. The fixture uses generic project names and bare settings. The capture
flag returns before normal application startup, so it does not load personal
configuration, saved state, provider history, or live terminal sessions.

The UI-lab source at capture time has SHA-256:

```text
53dd95908992ddd2422079ff5be4bf0bc7cbb9742d2b89a16a3dd13804792bd5
```

| Published image | Capture output | Pixels |
| --- | --- | --- |
| [Workspace chrome](workspace.png) | `daily.png` | 2400 × 1560 |
| [Inline layout rename](rename-layout.png) | `daily-rename-tab.png` | 2400 × 1560 |
| [Space context menu](space-menu.png) | `daily-menu.png` | 680 × 660 |

These are unedited PNG copies of the capture outputs. They were visually checked
for generic fixture content and checked for the absence of PNG text/EXIF metadata
chunks. Additional captures produced by the harness are not included here.

## Reproduction

From a source checkout with the pinned dependencies and Herdr build dependency
already available:

```sh
mkdir -p target/publication-screenshots
cargo run --offline --locked -p chartr --bin chartr \
  --features ui-lab,gpui_platform/runtime_shaders -- \
  --ui-lab-capture target/publication-screenshots/daily.png
```

The build requires the pinned sidecar, but the capture does not run it or start
agents. On macOS, the runtime-shader feature avoids the build-time `metal`
compiler requirement. Physical pixel dimensions can differ with platform scaling;
these captures were made on macOS at 2× scale.

## Limits

The images show production chrome components arranged in a synthetic scene.
Workspace content is deliberately a placeholder. The combined scene is not a
claim that every visible control is present together in a normal app view.
Menu actions and session controls are inert in the fixture.

These images do not verify terminal input, drag/drop persistence, actual agent
launches, resume/reconnection, a computer restart, or complete **Tabs** or
**Chats** workflows. They are illustrations, not release acceptance evidence.
