//! Chartr's visual design tokens and the small shared pieces built from them.
//!
//! `docs/visual-design.md` explains the rules; this module is the source of
//! truth for the numbers. Use these instead of new literals, and update both
//! files together when a value changes.

use gpui::{AnyElement, App, Hsla, Pixels, Rems, px};
use ui::{Icon, IconName, IconSize, prelude::*, rems_from_px};

// Corner radius. Inner radius = outer radius − gap, so nested curves run parallel.
/// Workspace panel and cards. The macOS window corner (~16) minus the 6px inset gap.
pub(crate) const RADIUS_PANEL: Pixels = px(10.);
/// Menus, popovers and the workspace strip group.
pub(crate) const RADIUS_MENU: Pixels = px(8.);
/// Rows, buttons, switches and inputs.
pub(crate) const RADIUS_CONTROL: Pixels = px(6.);
/// Status tiles, inline rename fields, key hints and menu item highlights.
pub(crate) const RADIUS_SMALL: Pixels = px(4.);

/// Every icon-only button: a 24px box around a 14px (`IconSize::Small`) icon.
pub(crate) const ICON_BUTTON: Pixels = px(24.);
/// Inset and gap of the workspace-mode switch; 8px outer / 6px inner corners.
pub(crate) const MODE_SWITCH_INSET: Pixels = px(2.);
/// Fits the single-line rename field inside a 25px sidebar tab row.
pub(crate) const INLINE_RENAME_HEIGHT: Pixels = px(23.);
/// A status tile is a 16px square with an 11px glyph.
pub(crate) const STATUS_TILE: Pixels = px(16.);
const STATUS_GLYPH: f32 = 11.;

// Text sizes beyond `fonts::UI_TEXT_*` (14 / 12 / 10). The rem base is 14px.
/// Sidebar section title ("Spaces"): 14px semibold, set apart by weight, not size.
pub(crate) const UI_TEXT_TITLE: Rems = Rems(14. / 14.);
/// Status chip text: 11px medium.
pub(crate) const UI_TEXT_CHIP: Rems = Rems(11. / 14.);

// Tint strengths, as opacity over the status or brand color.
/// Status tile and chip fill.
pub(crate) const TINT_FILL: f32 = 0.16;
/// Chip border.
pub(crate) const TINT_BORDER: f32 = 0.35;
/// Hover fill on tabs, switch cells and rows.
pub(crate) const TINT_HOVER: f32 = 0.13;
/// Quiet neutral track and hover fills for the workspace-mode switch.
pub(crate) const TINT_MODE_GROUP: f32 = 0.06;
pub(crate) const TINT_MODE_HOVER: f32 = 0.05;
/// Hairline around the mode switch's selected choice, over the text color.
pub(crate) const TINT_MODE_EDGE: f32 = 0.1;
/// Selected pane-tab fill, over the text color. Hover uses the 0.05 row fill.
pub(crate) const TINT_TAB_SELECTED: f32 = 0.08;

/// The hover fill for tabs, switch cells and rows: a faint wash of the info color.
pub(crate) fn hover_tint(cx: &App) -> Hsla {
    cx.theme().status().info.opacity(TINT_HOVER)
}

/// A status glyph on a soft tile of its own color, kept for states that need
/// the user (needs you, ended); see `chrome::item_indicator` for the mapping.
pub(crate) fn status_tile(icon: Icon, color: Hsla) -> Div {
    tile(status_glyph(icon, color), color)
}

/// A glyph sized and colored for a status tile or chip.
pub(crate) fn status_glyph(icon: Icon, color: Hsla) -> Icon {
    icon.size(IconSize::Custom(rems_from_px(STATUS_GLYPH))).color(Color::Custom(color))
}

/// The tile itself, for glyphs that are already wrapped.
pub(crate) fn tile(glyph: impl IntoElement, color: Hsla) -> Div {
    div()
        .flex_none()
        .size(STATUS_TILE)
        .flex()
        .items_center()
        .justify_center()
        .rounded(RADIUS_SMALL)
        .bg(color.opacity(TINT_FILL))
        .child(glyph)
}

/// The "needs you" tile, used wherever an agent or tab is waiting on the user.
pub(crate) fn needs_you_tile(cx: &App) -> AnyElement {
    status_tile(Icon::new(IconName::BellRing), cx.theme().status().warning).into_any_element()
}

/// A pill that names a state the user must act on: icon plus one or two words,
/// a tinted fill and a slightly stronger border in the same color.
pub(crate) fn status_chip(icon: IconName, label: impl Into<SharedString>, color: Hsla) -> Div {
    h_flex()
        .flex_none()
        .h(px(20.))
        .pl(px(6.))
        .pr(px(8.))
        .gap(px(4.))
        .rounded_full()
        .border_1()
        .border_color(color.opacity(TINT_BORDER))
        .bg(color.opacity(TINT_FILL))
        .child(status_glyph(Icon::new(icon), color))
        .child(
            Label::new(label.into())
                .size(LabelSize::Custom(UI_TEXT_CHIP))
                .weight(gpui::FontWeight::MEDIUM)
                .color(Color::Custom(color)),
        )
}
