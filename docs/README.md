# Documentation

These guides describe the source in this **heavily customized personal fork** of
Chartr. It has only been tested on **macOS**. Changes to **Tabs** and **Chats**
remain largely untested end to end; Linux packaging and release procedures are
inherited material, not support claims for this fork.

Start with the [fork overview](../README.md) and
[Daily macOS build guide](daily-app-build-and-verify.md). Upstream downloads do not
include these customizations.

## Current guides

| Topic | Guide |
| --- | --- |
| Local Daily bundle, installation checks, and screenshot capture | [Daily build](daily-app-build-and-verify.md) |
| Upstream packages and general source-build prerequisites | [Installation](installation.md) |
| First space, agent setup, and the Wayfinder workflow | [Getting started](getting-started.md) |
| Spaces, panes, terminals, settings, persistence, and data locations | [Workspace](workspace.md) |
| Chats view, Inbox/Archive, agent discovery, and session logs | [Inbox](conversations.md) |
| Package installation, prerequisites, permissions, and SDK contracts | [Plugins](plugins.md) |
| Persistent plugin activity | [Status bar](status-bar.md) |
| Ordered local and Git skill sources | [Skill sources](../plugins/skills/README.md) |
| Shared prompt library and editor modals | [Saved Prompts](../plugins/prompts/README.md) |
| Template composition and explicit project-file saves | [Markdown Prompt](../plugins/markdown-prompt/README.md) |
| Maps, ticket launch, and claim recovery | [Wayfinder](../plugins/wayfinder/README.md) |
| Markdown map/ticket format | [Tracker convention](../plugins/wayfinder/TRACKER-CONVENTION.md) |

## Development and design

- [Code map](code-map.md): implementation owners and entry points.
- [Visual design](visual-design.md): native chrome conventions and shared controls.
- [Screenshots](assets/screenshots/README.md): generic fixture captures, source
  provenance, and their limitations.
- [Release builds](releasing.md): inherited packaging procedures, not verified
  releases of this fork.
- [Acceptance checklist](acceptance.md): checks still needed before broader
  support or release claims. A checklist is not a record of passing results.
- [Architecture decisions](adr/README.md): rationale and changes to boundaries.
- [Plugin examples](../examples/plugins/README.md),
  [theme playground](../misc/theme-playground/README.md),
  [font assets](../crates/chartr/assets/fonts/README.md), and
  [icon provenance](../crates/chartr/assets/icons/README.md).
- [Platform patches](../vendor/zed-platform/README.md) and
  [terminal-view patch](../vendor/zed-terminal-view/chartr-PATCH.md): maintained
  changes to the pinned Zed source.

## Historical and inactive material

Upstream's [workspace specification](../.plan/maps/chartr-workspace/spec.md),
[research notes](research/README.md), and
[17 September documentation audit](research/2026-09-17-documentation-audit.md)
are kept unchanged from upstream. They record upstream's design history, not the
behavior of this fork. My personal planning notes and local handoffs are not
included.

[Mobile Companion](../plugins/companion/README.md) and its
[development protocol](companion-protocol.md) are retained for future work;
Companion is excluded from the current desktop build.
