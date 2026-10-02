//! The two chromes, and the one thing they have in common.
//!
//! A chrome is a list of outer workspace tabs with one selected. A one-item tab
//! is standalone; a multi-item pane workspace is one grouped entry. Sidebar
//! mode draws the list down the left and tabs mode draws it across the top.
//! Neither owns workspace state: both take [`Entry`] values and emit stable ids.

pub mod sidebar;
pub(crate) mod sidebar_pane;
#[cfg(test)]
mod sidebar_tests;
pub(crate) mod tab_sorter;
pub mod tabs;

use std::{cell::Cell, rc::Rc};

use crate::{
    fonts::UI_LABEL_DEFAULT,
    workspace::{ItemId, PaneId, WorkspaceTabId},
};
use chartr_herdr::control::SessionStatus;
use gpui::{ElementId, EntityId, Pixels, Role, SharedString, Stateful};
use ui::{ButtonLike, CommonAnimationExt, IconButton, Tooltip, prelude::*};

use crate::assets::PLUGIN_LAUNCHER_ICON_PATH;

const TAB_LABEL_MIN_WIDTH: f32 = 36.;
const STRIP_TAB_MAX_WIDTH: f32 = 200.;
/// Pane bars are one height whether or not they hold tabs.
pub(crate) const PANE_BAR_HEIGHT: f32 = 33.;
const PANE_TAB_PADDING: f32 = 11.;
const PANE_TAB_GAP: f32 = 8.;

pub(crate) fn new_item_button(id: impl Into<ElementId>) -> IconButton {
    IconButton::new(id, IconName::Plus).icon_size(IconSize::Small)
}

pub(crate) fn new_plugin_pane_button(id: impl Into<ElementId>, icon_size: IconSize) -> ButtonLike {
    new_plugin_pane_button_with_color(id, icon_size, Color::Default)
}

pub(crate) fn new_plugin_pane_button_with_color(
    id: impl Into<ElementId>,
    icon_size: IconSize,
    color: Color,
) -> ButtonLike {
    ButtonLike::new(id)
        .aria_label("New surface")
        .tooltip(Tooltip::text("New surface"))
        .child(Icon::from_path(PLUGIN_LAUNCHER_ICON_PATH).size(icon_size).color(color))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewItemKind {
    Terminal,
    Plugin,
}

/// A creation intent. Starting a drag never allocates a session or changes layout.
#[derive(Clone)]
pub struct DraggedNewItem {
    pub space: EntityId,
    pub kind: NewItemKind,
}

impl Render for DraggedNewItem {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(cx.theme().colors().elevated_surface_background)
            .border_1()
            .border_color(cx.theme().colors().border)
            .child(
                Label::new(match self.kind {
                    NewItemKind::Terminal => "New terminal",
                    NewItemKind::Plugin => "New surface",
                })
                .size(UI_LABEL_DEFAULT),
            )
    }
}

pub(crate) fn new_item_drag_handle(
    id: impl Into<ElementId>,
    space: Option<EntityId>,
    kind: NewItemKind,
    button: impl IntoElement,
) -> AnyElement {
    NewItemDragHandle { id: id.into(), space, kind, button: button.into_any_element() }
        .into_any_element()
}

#[derive(IntoElement)]
struct NewItemDragHandle {
    id: ElementId,
    space: Option<EntityId>,
    kind: NewItemKind,
    button: AnyElement,
}

impl RenderOnce for NewItemDragHandle {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = (self.id, "drag");
        let dragged = window.use_keyed_state(id.clone(), cx, |_, _| false);
        let start = dragged.clone();
        let release = dragged.clone();
        div()
            .id(id)
            .capture_any_mouse_down(move |_, _, cx| {
                dragged.update(cx, |dragged, _| *dragged = false)
            })
            .capture_any_mouse_up(move |_, window, cx| {
                // A drag released back over its source, including after Escape,
                // must not fall through to the button's ordinary click handler.
                if *release.read(cx) {
                    release.update(cx, |dragged, _| *dragged = false);
                    cx.stop_active_drag(window);
                    cx.stop_propagation();
                }
            })
            .when_some(self.space, |handle, space| {
                handle.on_drag(
                    DraggedNewItem { space, kind: self.kind },
                    move |dragged, _, _, cx| {
                        start.update(cx, |started, _| *started = true);
                        cx.refresh_windows();
                        cx.new(|_| dragged.clone())
                    },
                )
            })
            .child(self.button)
    }
}

pub(crate) fn new_item_cell(button: impl IntoElement, cx: &App) -> AnyElement {
    h_flex()
        // Keep an empty bar at full height instead of collapsing to the button.
        .h(px(PANE_BAR_HEIGHT))
        .flex_none()
        .px(DynamicSpacing::Base04.rems(cx))
        .child(button)
        .into_any_element()
}

/// A quiet pane bar: it shares the pane's background, and only a soft line
/// separates it from the content.
pub(crate) fn pane_bar(id: impl Into<ElementId>, cx: &App) -> Stateful<Div> {
    let colors = cx.theme().colors();
    h_flex()
        .id(id)
        .w_full()
        .h(px(PANE_BAR_HEIGHT))
        .flex_none()
        .overflow_hidden()
        .bg(colors.editor_background)
        .border_b_1()
        .border_color(colors.border_variant)
}

/// A single selected pane tab for views that show one item, such as an open
/// chat: the same icon, spacing and accent underline as `ItemTab::build`.
pub(crate) fn single_pane_tab(
    id: impl Into<ElementId>,
    icon_path: SharedString,
    title: impl Into<SharedString>,
    cx: &App,
) -> impl IntoElement {
    h_flex()
        .id(id)
        .relative()
        .role(Role::Tab)
        .aria_selected(true)
        .flex_none()
        .max_w(px(320.))
        .h(px(PANE_BAR_HEIGHT))
        .px(px(PANE_TAB_PADDING))
        .gap(px(PANE_TAB_GAP))
        .child(
            Icon::from_path(icon_path.clone())
                .size(IconSize::XSmall)
                .color(crate::agent_icons::icon_color(&icon_path)),
        )
        .child(Label::new(title.into()).size(UI_LABEL_DEFAULT).single_line().truncate())
        .child(
            div()
                .absolute()
                .left(px(8.))
                .right(px(8.))
                .bottom_0()
                .h(px(2.))
                .rounded(px(2.))
                .bg(cx.theme().colors().border_focused),
        )
}

/// Clip the full title and paint a fade only when it reaches the trailing edge.
/// Close controls overlay the title, so hovering never changes text geometry.
fn tab_label(
    title: SharedString,
    fit: bool,
    color: Color,
    background: gpui::Hsla,
    hover_background: gpui::Hsla,
    close_slot: Option<AnyElement>,
    show_close: bool,
) -> impl IntoElement {
    let text_right = Rc::new(Cell::new(px(0.)));
    let measured_right = text_right.clone();
    let fade = move |hovered: bool| {
        let text_right = text_right.clone();
        let background = if hovered { hover_background } else { background };
        div()
            .absolute()
            .right_0()
            .top_0()
            .h_full()
            .w(px(20.))
            .map(|fade| {
                if hovered {
                    fade.invisible().group_hover("", |fade| fade.visible())
                } else {
                    fade.group_hover("", |fade| fade.invisible())
                }
            })
            .child(
                gpui::canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        if text_right.get() > bounds.right() {
                            window.paint_quad(gpui::fill(
                                bounds,
                                gpui::linear_gradient(
                                    90.,
                                    gpui::linear_color_stop(background, 1.),
                                    gpui::linear_color_stop(background.opacity(0.), 0.),
                                ),
                            ));
                        }
                    },
                )
                .size_full(),
            )
    };
    let label = h_flex()
        .relative()
        .map(|label| if fit { label.flex_auto() } else { label.flex_1() })
        .min_w_0()
        .min_h(px(14.))
        .overflow_hidden()
        .child(
            div()
                .flex_none()
                .on_children_prepainted(move |bounds, _, _| {
                    if let Some(label) = bounds.first() {
                        measured_right.set(label.right());
                    }
                })
                .child(Label::new(title).size(UI_LABEL_DEFAULT).color(color).single_line()),
        )
        .child(fade(false))
        .child(fade(true));
    // The close button has its own slot after the title so it never covers
    // the name; the slot is always reserved so hovering does not move text.
    h_flex()
        // Strip tabs size to their title; pane tabs share the bar's width.
        .map(|row| if fit { row.flex_auto() } else { row.flex_1() })
        .min_w_0()
        .gap(px(4.))
        .child(label)
        .when_some(close_slot, |row, close| {
            row.child(
                h_flex()
                    .flex_none()
                    .size(px(14.))
                    .justify_center()
                    .when(!show_close, |slot| {
                        slot.invisible().group_hover("", |slot| slot.visible())
                    })
                    .child(close),
            )
        })
}

/// The common visual core for every workspace tab.
///
/// Zed's selected [`Tab`] replaces one horizontal pixel of padding with a
/// border. GPUI paints that border inside the box, so its intrinsic width is
/// one pixel smaller than the inactive state. Restore that pixel here to keep
/// selection from shifting the rest of either tab strip.
pub(crate) struct ItemTab<'a> {
    id: ElementId,
    title: SharedString,
    aria_label: SharedString,
    selected: bool,
    activity: Activity,
    icon_path: Option<SharedString>,
    grouped: bool,
    space: &'a str,
    key: ItemId,
    close_slot: Option<AnyElement>,
}

impl<'a> ItemTab<'a> {
    pub(crate) fn min_width(rounded: bool, cx: &App) -> Pixels {
        // Minimum 36px label, 16px status tile, 14px close slot, padding, and gaps.
        if rounded {
            px(TAB_LABEL_MIN_WIDTH + 16. + 14. + 2.)
                + DynamicSpacing::Base06.px(cx) * 2.
                + DynamicSpacing::Base04.px(cx) * 2.
        } else {
            px(TAB_LABEL_MIN_WIDTH + 16. + 14. + 2.)
                + DynamicSpacing::Base06.px(cx)
                + DynamicSpacing::Base04.px(cx) * 3.
        }
    }

    pub(crate) fn new(
        id: impl Into<ElementId>,
        title: impl Into<SharedString>,
        selected: bool,
        space: &'a str,
        key: ItemId,
    ) -> Self {
        let title = title.into();
        Self {
            id: id.into(),
            aria_label: title.clone(),
            title,
            selected,
            activity: Activity::default(),
            icon_path: None,
            grouped: false,
            space,
            key,
            close_slot: None,
        }
    }

    pub(crate) fn aria_label(mut self, label: impl Into<SharedString>) -> Self {
        self.aria_label = label.into();
        self
    }

    pub(crate) fn activity(mut self, activity: Activity) -> Self {
        self.activity = activity;
        self
    }

    pub(crate) fn icon_path(mut self, icon_path: Option<SharedString>) -> Self {
        self.icon_path = icon_path;
        self
    }

    pub(crate) fn grouped(mut self, grouped: bool) -> Self {
        self.grouped = grouped;
        self
    }

    pub(crate) fn close_slot(mut self, close_slot: Option<AnyElement>) -> Self {
        self.close_slot = close_slot;
        self
    }

    /// Pane tab: fills its sorter slot, with a rounded fill for the selected
    /// or hovered tab and no outline, in the same family as the strip's tabs.
    pub(crate) fn build(self, cx: &App) -> Stateful<Div> {
        let colors = cx.theme().colors();
        let surface = colors.editor_background;
        let background = surface.blend(colors.text.opacity(crate::design::TINT_TAB_SELECTED));
        let hover_background = surface.blend(colors.text.opacity(0.05));
        let selected = self.selected;
        h_flex()
            .id(self.id)
            .group("")
            .role(Role::Tab)
            .aria_label(self.aria_label)
            .aria_selected(selected)
            .w_full()
            .h(crate::design::ICON_BUTTON)
            .pl(DynamicSpacing::Base06.px(cx))
            .pr(DynamicSpacing::Base04.px(cx))
            .gap(DynamicSpacing::Base04.rems(cx))
            .rounded(crate::design::RADIUS_CONTROL)
            .when_else(
                selected,
                |tab| tab.bg(background),
                |tab| tab.hover(move |style| style.bg(hover_background)),
            )
            .cursor_pointer()
            .child(h_flex().flex_none().size(crate::design::STATUS_TILE).justify_center().child(
                item_indicator(
                    self.activity,
                    self.icon_path,
                    self.grouped,
                    self.space,
                    self.key,
                    cx,
                ),
            ))
            .child(tab_label(
                self.title,
                false,
                if selected { Color::Default } else { Color::Muted },
                if selected { background } else { surface },
                if selected { background } else { hover_background },
                self.close_slot,
                selected,
            ))
    }

    /// Compact layout tab for the Tabs strip: sized to its title, with a faint
    /// fill for the selected or hovered tab and no outline.
    pub(crate) fn build_rounded(self, hovered: bool, cx: &App) -> Stateful<Div> {
        let colors = cx.theme().colors();
        let fill = colors.text.opacity(0.05);
        let filled = colors.background.blend(fill);
        let lit = self.selected || hovered;
        h_flex()
            .id(self.id)
            .group("")
            .role(Role::Tab)
            .aria_label(self.aria_label)
            .aria_selected(self.selected)
            .flex_none()
            .max_w(px(STRIP_TAB_MAX_WIDTH))
            .h(px(22.))
            .px(px(9.))
            .gap(px(7.))
            .rounded(px(8.))
            .when(lit, |tab| tab.bg(fill))
            .cursor_pointer()
            .child(h_flex().flex_none().size(crate::design::STATUS_TILE).justify_center().child(
                item_indicator(
                    self.activity,
                    self.icon_path,
                    self.grouped,
                    self.space,
                    self.key,
                    cx,
                ),
            ))
            .child(tab_label(
                self.title,
                true,
                if lit { Color::Default } else { Color::Muted },
                if self.selected { filled } else { colors.background },
                filled,
                self.close_slot,
                self.selected,
            ))
    }
}

/// One row in the sidebar, or one tab in the strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub space: EntityId,
    pub space_key: String,
    pub key: ItemId,
    pub tab: WorkspaceTabId,
    pub pane: PaneId,
    pub index: usize,
    pub title: String,
    /// A provider glyph for recognized sessions, a plugin's package-owned SVG,
    /// or the embedded plugin-launcher icon. Groups have no provider icon.
    pub icon_path: Option<SharedString>,
    /// Herdr's agent state. Plugins and grouped outer tabs have no aggregate
    /// session state of their own.
    pub status: Option<SessionStatus>,
    /// A non-agent process currently owns the foreground process group.
    pub process_running: bool,
    /// A session whose reader has stopped is still listed — closing it is the
    /// user's decision, not something that happens to them.
    pub ended: bool,
    /// Zed's terminal emulator received BEL since the terminal last handled input.
    pub bell: bool,
    pub selected: bool,
    pub closable: bool,
    pub grouped: bool,
}

impl Entry {
    pub(crate) fn activity(&self) -> Activity {
        Activity {
            status: self.status,
            process_running: self.process_running,
            ended: self.ended,
            bell: self.bell,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Activity {
    pub status: Option<SessionStatus>,
    pub process_running: bool,
    pub ended: bool,
    pub bell: bool,
}

/// One complete workspace arrangement surfaced as a saved layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutEntry {
    pub tab: WorkspaceTabId,
    pub name: Option<String>,
    pub selected: bool,
    /// An agent in this layout is blocked on the user.
    pub needs_you: bool,
    /// The tab as the strip shows it: its surface's title, icon, and status.
    pub entry: Option<Entry>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpaceActivity {
    Live,
    Waiting,
    Inactive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaceEntries {
    pub id: EntityId,
    pub name: String,
    pub collapsed: bool,
    pub active: bool,
    pub removable: bool,
    pub available: bool,
    pub layouts: Vec<LayoutEntry>,
    pub activity: Option<SpaceActivity>,
}

/// An agent the title-bar notice lists: waiting on the user, or working.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentNotice {
    pub space: EntityId,
    pub item: ItemId,
    /// The agent and its space, as in "codex · chartr".
    pub title: String,
    /// The conversation title, when the agent reported one.
    pub detail: Option<String>,
    pub icon_path: Option<SharedString>,
    pub waiting: bool,
}

/// The agents the create menu offers, and the one started most recently in
/// this app session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentChoices {
    pub names: Vec<String>,
    pub last_used: Option<String>,
}

/// A directly launchable, non-session-bound surface shown in creation menus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceOption {
    pub title: String,
    pub key: chartr_plugin::PaneKey,
    pub icon_path: SharedString,
    pub external_icon: bool,
}

/// The create menu shared by space rows and pane bars: a terminal, then agent
/// chats, then surfaces. Every choice starts in `space`.
pub fn create_menu(
    mut menu: crate::components::ContextMenu,
    space: EntityId,
    agents: &AgentChoices,
    surfaces: &[SurfaceOption],
    on: Emit,
) -> crate::components::ContextMenu {
    let terminal = on.clone();
    menu = menu
        .entry_with_icon_path(
            "Terminal",
            crate::assets::CREATE_TERMINAL_ICON_PATH,
            move |window, cx| terminal(Action::NewInSpace { space }, window, cx),
        )
        .with_shortcut(Box::new(crate::actions::workspace::NewTerminal));
    if !agents.names.is_empty() {
        menu = menu.separator().header_with_meta("Agents", agents.names.len().to_string());
        for name in &agents.names {
            let start = on.clone();
            let agent = name.clone();
            let icon = crate::agent_icons::known_agent_icon(name)
                .unwrap_or(crate::agent_icons::GENERIC_AGENT_ICON);
            menu = menu
                .entry_with_icon_path(name.clone(), icon, move |window, cx| {
                    start(Action::StartAgentInSpace { space, name: agent.clone() }, window, cx)
                })
                .with_icon_color(crate::agent_icons::icon_color(icon))
                .when(agents.last_used.as_ref() == Some(name), |menu| menu.with_meta("last used"));
        }
    }
    menu = menu.separator().header("Surfaces");
    for surface in surfaces {
        let open = on.clone();
        let key = surface.key.clone();
        let action = move |window: &mut Window, cx: &mut App| {
            open(Action::NewSurfaceInSpace { space, key: key.clone() }, window, cx)
        };
        menu = if surface.external_icon {
            menu.entry_with_external_icon_path(
                surface.title.clone(),
                surface.icon_path.clone(),
                action,
            )
        } else {
            menu.entry_with_icon_path(surface.title.clone(), surface.icon_path.clone(), action)
        };
    }
    menu.entry_with_icon_path(
        "More surfaces…",
        crate::assets::BROWSE_SURFACES_ICON_PATH,
        move |window, cx| on(Action::NewPluginPaneInSpace { space }, window, cx),
    )
    .with_shortcut(Box::new(crate::actions::workspace::NewSurface))
    .popup_width(px(236.))
}

/// What the user did to the chrome.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    ToggleSpaceCollapsed { space: EntityId },
    ActivateSpace { space: EntityId },
    ActivateLayout { space: EntityId, tab: WorkspaceTabId },
    Select { space: Option<EntityId>, item: ItemId },
    Close { space: Option<EntityId>, item: ItemId },
    CloseGroup { space: EntityId, tab: WorkspaceTabId },
    CloseOtherTabs { space: EntityId, tab: WorkspaceTabId },
    UngroupPane { space: EntityId, tab: WorkspaceTabId },
    RenameGroup { space: EntityId, tab: WorkspaceTabId },
    RenameItem { space: Option<EntityId>, item: ItemId },
    MoveWorkspaceTab { space: EntityId, tab: WorkspaceTabId, target_index: usize },
    BeginSpaceDrag { at: Pixels },
    CloseSpace { space: EntityId },
    RenameSpace { space: EntityId },
    CommitRename,
    CancelRename,
    OpenSpaceFolder { space: EntityId },
    LocateSpace { space: EntityId },
    SwitchToTabs,
    SwitchToSidebar,
    SwitchToConversations,
    NewSpace,
    NewInSpace { space: EntityId },
    NewPluginPaneInSpace { space: EntityId },
    NewSurfaceInSpace { space: EntityId, key: chartr_plugin::PaneKey },
    StartAgentInSpace { space: EntityId, name: String },
    New,
    NewPluginPane,
    OpenSettings,
}

/// A whole sidebar space card in flight.
///
/// Space sorting deliberately has its own payload type. Session rows nested in
/// the card continue to carry [`DraggedItem`], so GPUI dispatches the two drag
/// gestures to different listeners without either surface inspecting or
/// rejecting the other's values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DraggedSpace(pub EntityId);

impl Render for DraggedSpace {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

/// How a chrome reports what the user did.
///
/// `Rc` because both chromes hand the same callback to every row they draw,
/// and a `cx.listener` closure is not `Clone`.
pub type Emit = Rc<dyn Fn(Action, &mut Window, &mut App)>;

#[derive(Clone)]
pub struct DraggedSidebar;

impl Render for DraggedSidebar {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

#[derive(Clone)]
pub struct DraggedItem {
    pub space: String,
    pub tab: WorkspaceTabId,
    pub pane: PaneId,
    pub index: usize,
    pub item: ItemId,
    pub top_level: bool,
    /// The drag represents the whole outer workspace tab, not its
    /// representative item. Grouped tabs may be sorted by outer chrome, but
    /// cannot be dropped into an individual pane as though they were one item.
    pub grouped: bool,
}

impl Render for DraggedItem {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        dragged_item_pill(self.grouped, cx)
    }
}

const DRAGGED_ITEM_PILL_WIDTH: f32 = 44.;
const DRAGGED_GROUP_PILL_WIDTH: f32 = 60.;
const DRAGGED_ITEM_PILL_HEIGHT: f32 = 22.;

fn dragged_item_pill_width(grouped: bool) -> f32 {
    if grouped { DRAGGED_GROUP_PILL_WIDTH } else { DRAGGED_ITEM_PILL_WIDTH }
}

fn dragged_item_pill(grouped: bool, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    div()
        .flex()
        .items_center()
        .justify_center()
        .w(px(dragged_item_pill_width(grouped)))
        .h(px(DRAGGED_ITEM_PILL_HEIGHT))
        .rounded_full()
        .border_1()
        .border_color(colors.border)
        .bg(colors.elevated_surface_background)
        .shadow_md()
        .child(Label::new(if grouped { "group" } else { "tab" }).size(UI_LABEL_DEFAULT))
}

/// Builds the one drag preview used by every chartr tab surface.
///
/// GPUI positions a drag view at `pointer - offset_within_source`, which is
/// perfect when the preview has the source element's dimensions. chartr's
/// sidebar rows and outer tabs are often much wider than the compact preview,
/// though, so using the source offset makes the visible ghost trail behind the
/// pointer. Translating the compact preview by that same offset and half of
/// its own size locks its center to GPUI's current-frame pointer position.
pub(crate) fn dragged_item_preview(
    dragged: &DraggedItem,
    source_offset: gpui::Point<gpui::Pixels>,
    sorter: Option<gpui::WeakEntity<crate::components::ListSorter<u64>>>,
    cx: &mut App,
) -> gpui::Entity<DraggedItemPreview> {
    let dragged = dragged.clone();
    cx.new(|_| DraggedItemPreview { dragged, source_offset, sorter })
}

pub(crate) struct DraggedItemPreview {
    dragged: DraggedItem,
    source_offset: gpui::Point<gpui::Pixels>,
    sorter: Option<gpui::WeakEntity<crate::components::ListSorter<u64>>>,
}

impl Render for DraggedItemPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self
            .sorter
            .as_ref()
            .and_then(|sorter| sorter.upgrade())
            .is_some_and(|sorter| sorter.read(cx).is_dragging())
        {
            return div().into_any_element();
        }
        let width = dragged_item_pill_width(self.dragged.grouped);
        div()
            .pl(self.source_offset.x - px(width / 2.))
            .pt(self.source_offset.y - px(DRAGGED_ITEM_PILL_HEIGHT / 2.))
            .child(dragged_item_pill(self.dragged.grouped, cx))
            .into_any_element()
    }
}

/// The fixed leading mark used by sidebar rows, outer tabs, and pane-local tabs.
///
/// States that need the user sit on a tinted tile (`design::status_tile`):
/// amber needs you, red ended. Working (blue spinner) and done (green check)
/// are bare glyphs, so the tile works as an alarm. An idle agent shows its
/// bare brand glyph and plugins their plain Hugeicon. A plain
/// foreground process gets a slower neutral spinner with no tile so it cannot
/// be mistaken for an agent actively working.
pub fn item_indicator(
    activity: Activity,
    icon_path: Option<SharedString>,
    grouped: bool,
    space: &str,
    key: ItemId,
    cx: &App,
) -> AnyElement {
    let status = cx.theme().status();
    let slot = || {
        div().flex_none().size(crate::design::STATUS_TILE).flex().items_center().justify_center()
    };
    let icon = |name, color| Icon::new(name).size(IconSize::XSmall).color(color);
    let tile = |name, color| crate::design::status_tile(Icon::new(name), color);

    if activity.ended {
        return tile(IconName::Close, status.error).into_any_element();
    }
    if grouped {
        return slot().child(icon(IconName::Split, Color::Muted)).into_any_element();
    }
    if activity.bell {
        return tile(IconName::BellRing, status.warning).into_any_element();
    }

    match activity.status {
        Some(SessionStatus::Working) => slot()
            .child(
                icon(IconName::LoadCircle, Color::Custom(status.info)).with_keyed_rotate_animation(
                    format!("working-status-{space}-{}", key.get()),
                    2,
                ),
            )
            .into_any_element(),
        Some(SessionStatus::Blocked) => tile(IconName::BellRing, status.warning).into_any_element(),
        Some(SessionStatus::Done) => {
            slot().child(icon(IconName::Check, Color::Custom(status.success))).into_any_element()
        }
        Some(SessionStatus::Idle | SessionStatus::Unknown) if activity.process_running => {
            slot()
                .child(icon(IconName::LoadCircle, Color::Muted).with_keyed_rotate_animation(
                    format!("process-status-{space}-{}", key.get()),
                    5,
                ))
                .into_any_element()
        }
        Some(SessionStatus::Unknown) => gpui::Empty.into_any_element(),
        Some(SessionStatus::Idle) | None => match icon_path {
            Some(path) => {
                let color = crate::agent_icons::icon_color(&path);
                let glyph = if path.starts_with("icons/") {
                    Icon::from_path(path)
                } else {
                    Icon::from_external_svg(path)
                };
                slot().child(glyph.size(IconSize::XSmall).color(color)).into_any_element()
            }
            None => slot().into_any_element(),
        },
    }
}
