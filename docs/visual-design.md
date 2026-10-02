# Chartr visual design guide

This guide is the rulebook for Chartr's look. The numbers live in code, in
[`crates/chartr/src/design.rs`](../crates/chartr/src/design.rs). When a value
changes, update that file and this guide together.

The rendered version, with pictures of every rule, is
`.scratch-adopt/ui-refs/guide/design-guide.png` in the `chartr` repo.

## Principles

1. **Fewer values.** Pick from the tokens below. A new size, radius or color
   needs a reason and an update to this guide.
2. **One pattern per job.** Pane tabs and strip tabs use rounded fills.
   Content pickers use divided cells. Workspace modes use a capsule switch:
   navigation, not pane content.
   Status is a colored glyph; a tinted tile marks states you must act on.
3. **Color means something.** Chrome is neutral. Color appears only for state,
   hover, and brand glyphs.

## Color

Use theme tokens. Never write raw hex, except for the two brand glyph colors
in `agent_icons::icon_color`.

| Role | Token | Used for |
|---|---|---|
| Chrome | `colors.tab_inactive_background` | Tab strip, unselected cells |
| Background | `colors.background` | Window canvas, sidebar |
| Surface | `colors.tab_active_background` / `editor_background` | Panes, selected divided cells |
| Mode selection | `colors.element_selected` | Rounded selected workspace-mode choice |
| Mode group / hover | `colors.text.opacity(TINT_MODE_GROUP)` / `colors.text.opacity(TINT_MODE_HOVER)` | Quiet neutral mode control |
| Mode edge | `colors.text.opacity(TINT_MODE_EDGE)` | Hairline around the selected mode choice |
| Pane tab | `colors.text.opacity(TINT_TAB_SELECTED)` / `colors.text.opacity(0.05)` | Selected / hovered pane tab, over `editor_background` |
| Row fill | `colors.text.opacity(0.05)` | Hovered or menu-open unselected sidebar tab row |
| Border | `colors.border` | Panels, cells, controls |
| Border variant | `colors.border_variant` | Menus, quiet dividers |
| Text | `colors.text`, `text_muted`, `icon_muted` | Primary, secondary, faint |

| State | Token | Glyph |
|---|---|---|
| Working | `status().info` (blue) | Spinner (`LoadCircle`) |
| Needs you / blocked | `status().warning` (amber) | Bell (`BellRing`) |
| Done | `status().success` (green) | Check |
| Error / ended | `status().error` (red) | Close (×) |
| Idle Pi | `0x9d86ff` (brand) | Pi glyph |
| Idle Claude | `0xe39a72` (brand) | Claude glyph |

Tints are opacity over the state color:

```rust
pub(crate) const TINT_FILL: f32 = 0.16;   // status tile and chip fill
pub(crate) const TINT_BORDER: f32 = 0.35; // chip border
pub(crate) const TINT_HOVER: f32 = 0.13;  // pane/content hover fill (info blue)
pub(crate) const TINT_MODE_GROUP: f32 = 0.06; // neutral mode track
pub(crate) const TINT_MODE_HOVER: f32 = 0.05; // neutral mode-choice hover
pub(crate) const TINT_MODE_EDGE: f32 = 0.1;   // selected mode-choice hairline
pub(crate) const TINT_TAB_SELECTED: f32 = 0.08; // selected pane tab

pub(crate) fn hover_tint(cx: &App) -> Hsla {
    cx.theme().status().info.opacity(TINT_HOVER)
}
```

Don't:

- add decorative colors or extra accent markers. Selection is neutral except
  for the approved blue sidebar tab tint/guide bar;
- add a new status color. Five states map onto the four status tokens.

## Typography

IBM Plex Sans for UI, IBM Plex Mono for terminals and code. The rem base is
14px. Keep to these steps; hierarchy comes from weight and color first.

| Role | Constant | Size · weight | Used for |
|---|---|---|---|
| Title | `design::UI_TEXT_TITLE` | 14 · semibold | Sidebar "Spaces" title, one per sidebar |
| Large | `fonts::UI_TEXT_LARGE` / `UI_LABEL_LARGE` | 14 | Strip labels |
| Default | `fonts::UI_TEXT_DEFAULT` / `UI_LABEL_DEFAULT` | 12 | Space names, tab rows, pane tabs, menus, body |
| Chip | `design::UI_TEXT_CHIP` | 11 · medium | Status chips |
| Small | `fonts::UI_TEXT_SMALL` / `UI_LABEL_SMALL` | 10 | Counts and captions |

```rust
// The sidebar title: 14 semibold, left edge on the chevron glyphs.
h_flex()
    .h(px(30.))
    .pl(px(18.)) // 8 list inset + 7 row padding + 3 glyph inset
    .text_size(crate::design::UI_TEXT_TITLE)
    .font_weight(FontWeight::SEMIBOLD)
    .text_color(colors.text)
    .child("Spaces")
```

Rules:

- Use the `UI_LABEL_*` helpers. Don't mix in `LabelSize::Small` or `XSmall`.
- Semibold marks section titles and active space names. Pane tabs and
  workspace-mode choices stay regular; don't add bold body-text emphasis.

## Corner radius

Four tokens, chosen by role. Inner radius = outer radius − gap, so nested
curves run parallel. The workspace panel sits 6px inside a ~16px macOS window
corner, so it gets 10.

```rust
pub(crate) const RADIUS_PANEL: Pixels = px(10.);  // workspace panel, cards
pub(crate) const RADIUS_MENU: Pixels = px(8.);    // menus, popovers, strip group
pub(crate) const RADIUS_CONTROL: Pixels = px(6.); // rows, buttons, switches, inputs
pub(crate) const RADIUS_SMALL: Pixels = px(4.);   // status tiles, inline rename, key hints, menu highlights
```

- `rounded_full()` is only for status chips, drag pills and the workspace-mode
  capsule.
- Pane tabs round their fill with `RADIUS_CONTROL`, like the strip's tabs.
- Divided content cells are square inside; only their outer group is rounded.
- Workspace modes: a 28px capsule (radius 14) − inset 2 = a 24px capsule choice
  (radius 12). No dividers.
- The vendored context menu mirrors `RADIUS_MENU` in its own `MENU_RADIUS`.
- Don't use 2, 12 or `rounded_lg`; map them to the nearest token.

## Sizes

| Element | Height | Notes |
|---|---|---|
| Title bar | 40 | `title_bar::HEIGHT` |
| Pane tab bar | 33 | `chrome::PANE_BAR_HEIGHT`, 24px rounded tabs on the pane's surface |
| Space row (sidebar) | 32 | `SPACE_ROW_HEIGHT` |
| Tab row (sidebar) | 25 | `LAYOUT_ROW_HEIGHT` |
| Menu item | 26 | Header 20, separator 7 |
| Icon button / mode choice | 24 | `design::ICON_BUTTON`, 14px icon or 12px mode label |
| Workspace mode group | 28 | Choice 24 + `design::MODE_SWITCH_INSET` (2) on each side |
| Inline rename | 23 | `design::INLINE_RENAME_HEIGHT`, fits a 25px tab row |
| Status chip | 20 | Pill |
| Status tile | 16 | `design::STATUS_TILE`, 11px glyph |

Spacing uses the steps 2, 4, 6, 8, 12 and 16. The window-to-panel gap is 6.
Menus are 164–250px wide.

## Icon buttons

One button everywhere: a 24px box, a 14px (`IconSize::Small`) icon, the theme's
control radius. Buttons that open a menu show the blue tint while it's open.

```rust
ButtonLike::new("new-surface-menu-trigger")
    .width(crate::design::ICON_BUTTON)
    .height(crate::design::ICON_BUTTON.into())
    .size(ButtonSize::None)
    .style(ButtonStyle::Subtle)
    .selected_style(ButtonStyle::Tinted(TintColor::Accent)) // menu open
    .aria_label("Start agent or surface")
    .child(Icon::new(IconName::Plus).size(IconSize::Small).color(Color::Muted))
    .tooltip(Tooltip::text("Start agent or surface"))
```

- Every icon-only button has a tooltip and an `aria_label`.
- Row buttons are always visible; don't hide them until hover.
- Don't make 22px, 28px or 16px variants.

## Tabs and switches

Inbox / Archive and settings pickers remain **divided cells**. Pane tabs use
**rounded fills**, in the same family as the Tabs strip. The title-bar Tabs /
Spaces / Chats switch is a **capsule**, through the opt-in
`SegmentedControl::soft_inset()` method. Its location and mode behavior stay
unchanged; this is not a global segmented-control restyle.

Mode group: 28px high, neutral `text` tint at 0.06, a capsule, no border.
Each choice: 24px high, 10px horizontal padding, 12px regular text, a capsule.
Inset and gap are both 2px. The selected fill uses `element_selected` with a
1px `text` hairline at 0.1, so it reads as raised; hover on another choice uses
neutral `text` at 0.05. The selected capsule keeps its shape while the existing
measured fill slides; reduced motion snaps.

```rust
// Only WorkspaceWindow::presentation_toggle and its UI-lab fixture opt in.
SegmentedControl::new("Session list presentation", options).soft_inset()
// Geometry comes from design::MODE_SWITCH_INSET and ICON_BUTTON; both shapes are
// capsules. Fills: TINT_MODE_GROUP / TINT_MODE_HOVER; edge: TINT_MODE_EDGE.
```

Pane tabs sit on a quiet bar (`chrome::pane_bar`) that shares the pane's
surface, 2px apart, in equal slots up to 200px wide:

| State | Look |
|---|---|
| Current | `TINT_TAB_SELECTED` fill, 6px corners. Primary text. |
| Hover | Neutral `text` at 0.05. Closable tabs show their close button. |
| Other | No fill, muted text, no divider. |

```rust
// Pane tabs (chrome::ItemTab::build): a rounded fill, no outline.
h_flex()
    .w_full()
    .h(crate::design::ICON_BUTTON)
    .rounded(crate::design::RADIUS_CONTROL)
    .when_else(selected, |tab| tab.bg(background), |tab| tab.hover(move |style| style.bg(hover)))

// Default switches (components::SegmentedControl): fill slides between divided cells.
h_flex()
    .rounded(crate::design::RADIUS_CONTROL)
    .border_1()
    .border_color(colors.border.opacity(0.8))
    .bg(colors.tab_inactive_background)
    .children(options.enumerate().map(|(index, option)| {
        cell.when(index > 0, |cell| cell.border_l_1().border_color(border))
            .when(hovered && !selected, |cell| cell.bg(hover_tint))
    }))
// …with the selected fill painted in colors.tab_active_background.
```

Don't use accent lines, underlines or bold to mark the current tab.

## Status

Every agent, tab and space shows state as a colored glyph in a 16px slot.
Only states you must act on (needs you, ended) get a **tinted tile**, so the
tile works as an alarm. A **chip** adds words in the title bar.
`chrome::item_indicator` maps states to marks; use it rather than drawing
them by hand.

```rust
/// A status glyph on a soft tile of its own color.
pub(crate) fn status_tile(icon: Icon, color: Hsla) -> Div {
    tile(status_glyph(icon, color), color)
}

pub(crate) fn tile(glyph: impl IntoElement, color: Hsla) -> Div {
    div()
        .flex_none()
        .size(STATUS_TILE)                 // 16px
        .flex().items_center().justify_center()
        .rounded(RADIUS_SMALL)             // 4px
        .bg(color.opacity(TINT_FILL))      // 16%
        .child(glyph)                      // 11px glyph in the full color
}

/// A pill naming a state the user must act on.
pub(crate) fn status_chip(icon: IconName, label: impl Into<SharedString>, color: Hsla) -> Div {
    h_flex()
        .h(px(20.)).pl(px(6.)).pr(px(8.)).gap(px(4.))
        .rounded_full()
        .border_1().border_color(color.opacity(TINT_BORDER))
        .bg(color.opacity(TINT_FILL))
        .child(status_glyph(Icon::new(icon), color))
        .child(Label::new(label.into()).size(LabelSize::Custom(UI_TEXT_CHIP)).color(Color::Custom(color)))
}
```

| State | Tile | Chip |
|---|---|---|
| Idle agent | Bare brand glyph, no tile. Tiles are for states only | — |
| Plain process running | Muted spinner, no tile | — |
| Working | Bare blue spinner, no tile | — |
| Needs you / blocked | Amber bell (`design::needs_you_tile`) | "needs you" in the title bar |
| Done | Bare green check, no tile | — |
| Ended / error | Red × | "error" where shown |

- Needs-you rolls up: tab row → collapsed space → title-bar chip with a count.
- Don't use pause bars or bare dots for "needs you".

## Sidebar rows and menus

- **Selected tab row:** blue tint (`info` at 10%) plus a permanent short 2px
  blue bar, centred on the 1px tree guide (`TREE_CONTENT_INSET`,
  `TREE_GUIDE_WIDTH` and `SELECTION_MARKER_WIDTH` in `sidebar.rs`). Only
  selection has the bar. Hovering the selected row keeps the same look.
- **Unselected tab row:** hover is the neutral row fill with no bar; checked,
  with the bar's centre on the guide, by
  `sidebar_selection_bar_sits_on_the_guide_and_hover_stays_neutral`.
- **Tab row status:** a fixed 16px slot at the right edge, reserved except
  while inline rename gives that space to the input. Titles normally truncate
  at one point. Its centre sits exactly on the centre of the
  space row's last action button (`STATUS_COLUMN_PADDING` in `sidebar.rs`,
  checked by `sidebar_tree_collapses_adds_and_sorts_free_sessions`). Align
  centres, not edges: the shapes differ, and a near miss reads as a mistake.
- **Row with its menu open:** a space heading shows the blue tint (`info` at
  10%). A tab row keeps its own look: the selected row keeps its tint and bar;
  an unselected row takes the neutral hover fill with no bar. Opening a menu
  does not select the tab.
- **Right-click menus** open at the pointer. **Button menus** drop down
  left-aligned and flip to the button's right edge near the window edge.
- **Menus:** `border_variant` border, tight 2–6px shadow, `RADIUS_MENU`
  corners, 4px item highlight.
- Destructive items go last, after a separator, and ask to confirm.
- Menu headers may carry a right-aligned count (`ContextMenu::header_with_meta`).
- Don't add keyboard shortcuts to menus without a separate decision.


## Inline rename (Spaces navigation)

Replace the name in its own row, not with an overlay. Space headings keep their
chevron/actions; tab rows keep the leading glyph and selected guide marker.
A tab's status slot hides during editing and returns on exit. Row sizes do not
change. The shared field is 23px high, padding 4, radius 4, one focused-border
pixel, `editor_background`, 12px regular text, and primary text color.

`chrome::sidebar::RenameRows` carries the existing workspace-owned TextInput.
Enter routes `CommitRename` to the existing owner commit methods, reading the
input directly. Escape, outside clicks and blur cancel without saving. Mode
changes cancel before the row disappears. Native plugin pointer-down uses the
existing `chartr.focus` bridge to cancel without stealing focus back. An inline
field cannot appear while a native rename dialog is open. Native text editing,
IME and undo remain TextInput's responsibility; the field stops row activation
and dragging.

Space/tab names in Spaces mode use this placement. Other modes and pane-item
rename keep the existing native dialog; no mode is switched just to rename.
The same owner validation/error/persistence behavior remains in place.

Tests: `sidebar_inline_rename_edits_in_place_and_routes_save_cancel_without_activation`
and `soft_mode_picker_is_a_capsule_without_changing_divided_controls`.
UI-lab captures include `-rename-tab.png` and `-rename-space.png`.
