# Screenshot provenance

These images were generated from the fixture-only
[`ui-lab` capture source](../../../crates/chartr/src/ui_lab.rs), not from a live
workspace. The fixture uses generic project names, bare settings, and the bundled
`chartrx` theme. The capture flag returns before normal application startup, so
it does not load personal configuration, saved state, provider history, or live
terminal sessions.

The UI-lab source at capture time has SHA-256:

```text
31defb6ece0782149ccc05fb728df5571d7b6cddc6438db039e94e275fa23e07
```

| Published image | Capture output | Pixels |
| --- | --- | --- |
| [Spaces view](spaces.png) | `daily.png` | 2624 × 1784 |
| [Tabs view](tabs.png) | `daily-tabs.png` | 2624 × 1784 |
| [Inline layout rename](rename-layout.png) | `daily-rename-tab.png` | 2624 × 1784 |
| [Space context menu](space-menu.png) | `daily-space-menu.png` | 2624 × 1784 |

These are unedited PNG copies of the capture outputs. They were visually checked
for generic fixture content and checked for the absence of PNG text/EXIF metadata
chunks. Additional captures produced by the harness are not included here.

## The macOS window frame

The offscreen capture has no native window, so the harness draws the frame
itself:

- The traffic lights are stand-ins drawn at the native buttons' position.
- After capture, the harness gives the window macOS 26's rounded corners, dark
  outline, light inner edge, and drop shadow, on a transparent margin.

The frame values were measured from a native macOS window capture with its
shadow, and they match it to within a few alpha levels.

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

Each image shows one presentation as the app arranges it: Spaces shows the
sidebar without the tab strip, and Tabs shows the strip without the sidebar. The
terminal text is static fixture text, not a running shell. Menu actions and
session controls are inert in the fixture.

These images do not verify terminal input, drag/drop persistence, actual agent
launches, resume/reconnection, a computer restart, or complete **Tabs** or
**Chats** workflows. They are illustrations, not release acceptance evidence.
