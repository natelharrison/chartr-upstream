//! Sidebar mode: collapsible spaces with compact saved-layout rows.

use std::time::Duration;

use gpui::{
    Animation, AnimationExt as _, Context, EntityId, Focusable, FontWeight, Hsla, MouseButton,
    Rems, Render, Role, Transformation, deferred, percentage, px, svg,
};
use ui::{ButtonLike, TintColor, Tooltip, prelude::*};

use super::Emit;
use crate::components::popup_right_click_menu;

use super::{Action, Activity, DraggedSpace, LayoutEntry, SpaceEntries, item_indicator};
use crate::components::{ContextMenu, PopupMenu, selection_list};
use crate::fonts::UI_TEXT_DEFAULT;
use chartr_herdr::control::SessionStatus;
use chartr_plugin::ui::UI_TEXT_SMALL;

/// Keep the compact space gap shared by layout and drag-sort animation.
pub(crate) const CARD_GAP: Rems = Rems(1. / 14.);
pub type SpaceSorter = crate::components::ListSorter<EntityId>;

/// Sidebar tab drags stay inside their own space; pane and space drags use
/// different payloads so they cannot consume a sidebar reorder by mistake.
#[derive(Clone)]
struct DraggedLayout {
    space: EntityId,
    tab: crate::workspace::WorkspaceTabId,
    index: usize,
    title: String,
    /// The row's surface, so the ghost leads with the same icon.
    entry: Option<super::Entry>,
}

/// Width of the drag ghost: about one sidebar row.
const GHOST_WIDTH: f32 = 180.;

impl Render for DraggedLayout {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let shadow = if cx.theme().appearance().is_light() { 0.16 } else { 0.4 };
        div()
            .w(px(GHOST_WIDTH))
            .h(px(LAYOUT_ROW_HEIGHT))
            .rounded(px(ROW_RADIUS))
            .bg(colors.elevated_surface_background)
            .border_1()
            .border_color(colors.border_variant)
            .shadow(vec![gpui::BoxShadow {
                color: gpui::black().opacity(shadow),
                offset: gpui::point(px(0.), px(6.)),
                blur_radius: px(16.),
                spread_radius: px(-4.),
                inset: false,
            }])
            .text_size(UI_TEXT_SMALL)
            .text_color(colors.text)
            .child(
                h_flex()
                    .size_full()
                    .px(px(7.))
                    .gap(px(6.))
                    .child(div().flex_none().w(px(13.)).child(lead_icon(
                        self.entry.as_ref(),
                        colors.text_muted,
                        cx,
                    )))
                    .child(div().min_w_0().truncate().child(self.title.clone())),
            )
    }
}

/// The layout drag in flight, so rows can dim their source and pick an edge
/// for the drop line. The ghost is released when the drag ends.
struct ActiveLayoutDrag(gpui::WeakEntity<DraggedLayout>);

impl gpui::Global for ActiveLayoutDrag {}

fn active_layout_drag(cx: &App) -> Option<DraggedLayout> {
    if !cx.has_active_drag() {
        return None;
    }
    let dragged = cx.try_global::<ActiveLayoutDrag>()?.0.upgrade()?;
    Some(dragged.read(cx).clone())
}

/// Which edge of the hovered row shows the drop line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DropEdge {
    Top,
    Bottom,
}

/// A tab dragged down lands below the hovered row; dragged up, above it.
fn drop_edge(dragged_index: usize, target_index: usize) -> DropEdge {
    if dragged_index < target_index { DropEdge::Bottom } else { DropEdge::Top }
}

/// The tab's leading icon: the surface's own icon, or the split mark for groups.
fn lead_icon(entry: Option<&super::Entry>, muted: Hsla, cx: &App) -> AnyElement {
    match entry {
        Some(entry) => item_indicator(
            Activity::default(),
            entry.icon_path.clone().or_else(|| Some("icons/tool_terminal.svg".into())),
            entry.grouped,
            &entry.space_key,
            entry.key,
            cx,
        ),
        None => Icon::new(IconName::Split)
            .size(IconSize::XSmall)
            .color(Color::Custom(muted.opacity(0.75)))
            .into_any_element(),
    }
}

fn layout_drop_index(
    dragged: &DraggedLayout,
    space: EntityId,
    tab: crate::workspace::WorkspaceTabId,
    index: usize,
) -> Option<usize> {
    (dragged.space == space && dragged.tab != tab).then_some(index)
}

const SPACE_ROW_HEIGHT: f32 = 32.;
const LAYOUT_ROW_HEIGHT: f32 = 25.;
const ROW_RADIUS: f32 = 6.;
/// Horizontal padding inside space and tab rows.
const ROW_PADDING: f32 = 7.;
/// The tab list stops this far short of the space row's right edge.
const TREE_RIGHT_MARGIN: f32 = 3.;
const TREE_CONTENT_INSET: f32 = 8.;
const TREE_GUIDE_WIDTH: f32 = 1.;
const SELECTION_MARKER_WIDTH: f32 = 2.;
/// Tab-row right padding that centres the status slot on the space row's last
/// action button. The shapes differ (tile, bare glyph, 14px icon), so the
/// shared centre is what reads as intended; a near miss reads as a mistake.
const STATUS_COLUMN_PADDING: f32 = ROW_PADDING + 24. / 2. - TREE_RIGHT_MARGIN - 16. / 2.;
const CHEVRON_TURN: Duration = Duration::from_millis(120);
const HOVER_FADE: Duration = Duration::from_millis(120);

fn layout_name(index: usize, name: Option<&str>) -> String {
    name.map(str::to_owned).unwrap_or_else(|| {
        if index == 0 { "Main layout".into() } else { format!("Layout {}", index + 1) }
    })
}

fn layout_title(index: usize, name: Option<&str>, surface_title: Option<&str>) -> String {
    name.map(str::to_owned)
        .or_else(|| surface_title.map(str::to_owned))
        .unwrap_or_else(|| layout_name(index, None))
}

/// The one inline editor owned by the workspace window. Only Spaces navigation
/// uses this; other rename contexts retain their native dialog.
#[derive(Clone)]
pub struct RenameRows {
    pub space: Option<EntityId>,
    pub tab: Option<(EntityId, crate::workspace::WorkspaceTabId)>,
    pub input: gpui::Entity<crate::text_input::TextInput>,
}

fn inline_rename_field(
    id: impl Into<gpui::ElementId>,
    input: gpui::Entity<crate::text_input::TextInput>,
    help: &'static str,
    on: Emit,
    cx: &App,
) -> AnyElement {
    let focus = input.focus_handle(cx);
    let key_on = on.clone();
    h_flex()
        .id(id)
        .debug_selector(|| "INLINE_RENAME_FIELD".into())
        .flex_1()
        .min_w_0()
        .h(crate::design::INLINE_RENAME_HEIGHT)
        .px(px(4.))
        .rounded(crate::design::RADIUS_SMALL)
        .border_1()
        .border_color(cx.theme().colors().border_focused)
        .bg(cx.theme().colors().editor_background)
        .text_size(UI_TEXT_DEFAULT)
        .font_weight(FontWeight::NORMAL)
        .text_color(cx.theme().colors().text)
        .track_focus(&focus)
        .tooltip(Tooltip::text(help))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            window.focus(&focus, cx);
            cx.stop_propagation();
        })
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .on_click(|_, _, cx| cx.stop_propagation())
        .on_mouse_down_out(move |_, window, cx| on(Action::CancelRename, window, cx))
        .on_key_down(move |event, window, cx| {
            let action = match event.keystroke.key.as_str() {
                "enter" => Action::CommitRename,
                "escape" => Action::CancelRename,
                _ => return,
            };
            cx.stop_propagation();
            key_on(action, window, cx);
        })
        .child(input)
        .into_any_element()
}

pub fn render(
    spaces: &[SpaceEntries],
    agents: &super::AgentChoices,
    surfaces: &[super::SurfaceOption],
    on: Emit,
    sorter: &SpaceSorter,
    rename: Option<&RenameRows>,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    // Count real toggles so a chevron turns only when the user opens or
    // closes its space, not whenever the row is first drawn.
    let turns: Vec<usize> = spaces
        .iter()
        .map(|space| {
            let collapsed = space.collapsed;
            window
                .use_keyed_state(format!("space-chevron-{:?}", space.id), cx, move |_, _| {
                    (collapsed, 0usize)
                })
                .update(cx, |(was_collapsed, turn), _| {
                    if *was_collapsed != collapsed {
                        *was_collapsed = collapsed;
                        *turn += 1;
                    }
                    *turn
                })
        })
        .collect();
    let now = cx.background_executor().now();
    let reduce_motion = cx.reduce_motion();
    let hovers: Vec<_> = spaces
        .iter()
        .map(|space| {
            window.use_keyed_state(format!("space-hover-{:?}", space.id), cx, |_, _| {
                HoverFade::default()
            })
        })
        .collect();
    let hover_levels: Vec<f32> = hovers
        .iter()
        .map(|hover| {
            let (level, moving) = hover.read(cx).level(now, reduce_motion);
            if moving {
                window.request_animation_frame();
            }
            level
        })
        .collect();
    let colors = cx.theme().colors();
    // Quiet rows: no fills on spaces. Brightness and weight mark the active
    // space; a faint neutral fill marks the selected layout.
    let quiet = QuietColors {
        text: colors.text,
        muted: colors.text_muted,
        selected: colors.text.opacity(0.05),
        guide: colors.border_variant.opacity(0.75),
    };
    let mut cards = Vec::with_capacity(spaces.len());
    let surfaces = surfaces.to_vec();
    let agents = agents.clone();
    let movable_space_count = spaces.len();
    let add_space = on.clone();
    let spaces_header = h_flex()
        .id("spaces-header")
        .w_full()
        .h(px(30.))
        .mt(px(6.))
        .mb(px(2.))
        // Line the title up with the chevron glyphs: 8px list inset, 7px row
        // padding, and the glyph's own 3px inset in its 16px slot.
        .pl(px(18.))
        .pr(px(8.))
        .text_size(crate::design::UI_TEXT_TITLE)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(quiet.text)
        .child("Spaces");
    // Centre the guide under the chevron: 7px heading padding plus half its 16px slot.
    let layout_indent = 7. + 8. - 0.5;
    for (space_index, space) in spaces.iter().enumerate() {
        let mut contents = Vec::with_capacity(space.layouts.len() + 1);
        let toggle = on.clone();
        let add = on.clone();
        let new_terminal = on.clone();
        let surfaces = surfaces.clone();
        let agents = agents.clone();
        let actions = on.clone();
        let space_id = space.id;
        let removable = space.removable;
        let available = space.available;
        let tab_count = space.layouts.len();
        let space_drag = DraggedSpace(space.id);
        let begin_drag = on.clone();
        let dragging = cx.has_active_drag();
        let activate = on.clone();
        let show_layouts = !space.layouts.is_empty();
        let space_needs_you = space.layouts.iter().any(|layout| layout.needs_you)
            && (space.collapsed || !show_layouts);
        let turn = turns[space_index];
        let hover_level = hover_levels[space_index];
        let hover = hovers[space_index].clone();
        let title_bar = h_flex()
            .id(format!("space-heading-{space_id:?}"))
            .on_hover(move |hovered, window, cx| {
                let now = cx.background_executor().now();
                let reduce_motion = cx.reduce_motion();
                hover.update(cx, |hover, _| hover.set(*hovered, now, reduce_motion));
                window.refresh();
            })
            .group("space-heading")
            .w_full()
            .min_w_0()
            .h(px(SPACE_ROW_HEIGHT))
            .px(px(ROW_PADDING))
            .rounded(px(ROW_RADIUS))
            .role(Role::TreeItem)
            .aria_label(space.name.clone())
            .aria_expanded(!space.collapsed)
            .gap_1()
            .on_click(move |_, window, cx| {
                activate(Action::ActivateSpace { space: space_id }, window, cx);
            })
            .justify_between()
            .child(
                h_flex()
                    .min_w_0()
                    .flex_1()
                    .gap(px(5.))
                    .child(
                        div()
                            .id(format!("space-disclosure-{space_id:?}"))
                            .flex_none()
                            .size(px(16.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .opacity(0.45 + 0.55 * hover_level)
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(move |_, window, cx| {
                                cx.stop_propagation();
                                toggle(Action::ToggleSpaceCollapsed { space: space_id }, window, cx)
                            })
                            .child(chevron(
                                space_id,
                                turn,
                                space.collapsed,
                                colors.icon_muted,
                                reduce_motion,
                            )),
                    )
                    .child(
                        if let Some(rename) = rename.filter(|rename| rename.space == Some(space_id))
                        {
                            inline_rename_field(
                                format!("rename-space-{space_id:?}"),
                                rename.input.clone(),
                                "Enter to save · Escape to cancel",
                                on.clone(),
                                cx,
                            )
                        } else {
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(UI_TEXT_DEFAULT)
                                .font_weight(if space.active {
                                    FontWeight::SEMIBOLD
                                } else {
                                    FontWeight::MEDIUM
                                })
                                .text_color(if space.active { quiet.text } else { quiet.muted })
                                .group_hover("space-heading", |style| style.text_color(quiet.text))
                                .child(space.name.clone())
                                .into_any_element()
                        },
                    )
                    .when(space_needs_you, |row| row.child(needs_you_dot(cx))),
            )
            .child(
                div()
                    .id(format!("space-heading-actions-{space_id:?}"))
                    .flex()
                    .flex_none()
                    .gap(px(2.))
                    // Always visible, so the row's actions are easy to find.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .when(space_index == 0, |plus| {
                                plus.debug_selector(|| "SIDEBAR_NEW_TERMINAL".into())
                            })
                            .child(
                                ButtonLike::new(("new-terminal-in-space", space_index))
                                    .width(px(24.))
                                    .height(px(24.).into())
                                    .size(ButtonSize::None)
                                    .child(
                                        Icon::new(IconName::Plus)
                                            .size(IconSize::Small)
                                            .color(Color::Muted),
                                    )
                                    .aria_label("New terminal in space")
                                    .tooltip(Tooltip::text("New terminal"))
                                    .on_click(move |_, window, cx| {
                                        new_terminal(
                                            Action::NewInSpace { space: space_id },
                                            window,
                                            cx,
                                        )
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .when(space_index == 0, |trigger| {
                                trigger.debug_selector(|| "SIDEBAR_CREATE_TRIGGER".into())
                            })
                            .child(
                                PopupMenu::new(("new-in-space", space_index))
                                    .trigger_with_tooltip(
                                        ButtonLike::new(("new-in-space-trigger", space_index))
                                            .width(px(24.))
                                            .height(px(24.).into())
                                            .size(ButtonSize::None)
                                            .selected_style(ButtonStyle::Tinted(TintColor::Accent))
                                            .aria_label("Add to space")
                                            .child(
                                                Icon::from_path(
                                                    crate::assets::PLUGIN_LAUNCHER_ICON_PATH,
                                                )
                                                .size(IconSize::Small)
                                                .color(Color::Muted),
                                            ),
                                        Tooltip::text("Start agent or surface"),
                                    )
                                    .menu(move |window, cx| {
                                        let on = add.clone();
                                        let agents = agents.clone();
                                        let surfaces = surfaces.clone();
                                        Some(ContextMenu::build_popup(window, cx, move |menu| {
                                            super::create_menu(
                                                menu, space_id, &agents, &surfaces, on,
                                            )
                                        }))
                                    }),
                            ),
                    ),
            )
            .when(movable_space_count > 1, |handle| {
                handle
                    .when(!dragging, |handle| handle.cursor_grab())
                    .when(dragging, |handle| handle.cursor_grabbing())
                    .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                        begin_drag(Action::BeginSpaceDrag { at: event.position.y }, window, cx)
                    })
                    .on_drag(space_drag, |dragged, _, _, cx| {
                        let dragged = *dragged;
                        cx.new(move |_| dragged)
                    })
            });
        let title_bar = if removable || !available {
            let summary = SpaceMenu::of(space);
            popup_right_click_menu(format!("space-actions-{space_index}"))
                // Tint the heading while its menu is open, like a tab row but without the bar.
                .trigger(move |open, _, cx| {
                    let tint = cx.theme().status().info.opacity(0.1);
                    title_bar.when(open, |row| row.bg(tint))
                })
                .menu(move |window, cx| {
                    let summary = summary.clone();
                    let on = actions.clone();
                    ContextMenu::build_popup(window, cx, move |menu| space_menu(menu, &summary, on))
                })
                .into_any_element()
        } else {
            title_bar.into_any_element()
        };
        contents.push(title_bar);
        if !space.collapsed {
            let layouts: Vec<_> = space
                .layouts
                .iter()
                .enumerate()
                .map(|(layout_index, layout)| {
                    layout_row(
                        space_id,
                        layout_index,
                        tab_count,
                        layout,
                        space.active && layout.selected,
                        quiet,
                        on.clone(),
                        rename,
                        cx,
                    )
                })
                .collect();
            contents.push(
                v_flex()
                    .relative()
                    .when(show_layouts, |tree| {
                        tree.gap(px(2.))
                            .ml(px(layout_indent))
                            .mr(px(TREE_RIGHT_MARGIN))
                            .mb(px(5.))
                            .py(px(2.))
                            .pl(px(TREE_CONTENT_INSET))
                            .child(
                                div()
                                    .debug_selector(move || format!("SPACE_GUIDE_{space_id:?}"))
                                    .absolute()
                                    .left_0()
                                    .top_0()
                                    .bottom_0()
                                    .w(px(TREE_GUIDE_WIDTH))
                                    .bg(quiet.guide),
                            )
                            .children(layouts)
                    })
                    .into_any_element(),
            );
        }

        // Move the entire expanded or collapsed tree with its heading.
        let held = sorter.holds(space.id);
        let offset = sorter.offset_of(space.id, now, reduce_motion);
        let card = selection_list()
            .id(format!("space-card-{:?}", space.id))
            .group("space-card")
            .relative()
            .w_full()
            .flex_none()
            .rounded(px(ROW_RADIUS))
            .when(held, |tree| tree.bg(colors.panel_background).shadow_md())
            .when(offset != px(0.), |tree| tree.top(offset))
            .children(contents);
        cards.push(
            div()
                .id(format!("space-slot-{:?}", space.id))
                .relative()
                .w_full()
                .flex_none()
                .child(if held {
                    deferred(card).into_any_element()
                } else {
                    card.into_any_element()
                })
                .into_any_element(),
        );
    }

    v_flex()
        .id("spaces-sidebar")
        .relative()
        .w_full()
        .min_h_0()
        .h_full()
        // The space tree is part of the window canvas; the inset workspace is
        // the floating panel. Keeping this surface on the canvas avoids a
        // second, square-edged panel behind the rounded workspace.
        .bg(colors.background)
        .child(spaces_header)
        .child(crate::components::scrolling_list(
            "spaces-scrollbar",
            v_flex().id("sessions").pb(px(10.)).px(px(8.)).gap(CARD_GAP).children(cards).child(
                h_flex()
                    .id("new-space")
                    .debug_selector(|| "SIDEBAR_NEW_SPACE".into())
                    .w_full()
                    .h(px(SPACE_ROW_HEIGHT))
                    .px(px(7.))
                    .gap(px(5.))
                    .rounded(px(ROW_RADIUS))
                    .role(Role::Button)
                    .aria_label("New Space")
                    .text_size(UI_TEXT_DEFAULT)
                    .text_color(quiet.muted.opacity(0.85))
                    .hover(|style| style.text_color(quiet.text))
                    .on_click(move |_, window, cx| add_space(Action::NewSpace, window, cx))
                    .child(
                        div()
                            .flex_none()
                            .size(px(16.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                Icon::new(IconName::Plus).size(IconSize::Small).color(Color::Muted),
                            ),
                    )
                    .child("New space"),
            ),
            sorter.scroll_handle(),
            window,
            cx,
        ))
}

/// What the space name's right-click menu needs to know about its space.
#[derive(Clone)]
pub(crate) struct SpaceMenu {
    id: EntityId,
    name: String,
    tabs: usize,
    running: usize,
    removable: bool,
    available: bool,
}

impl SpaceMenu {
    pub(crate) fn of(space: &SpaceEntries) -> Self {
        Self {
            id: space.id,
            name: space.name.clone(),
            tabs: space.layouts.len(),
            running: space
                .layouts
                .iter()
                .filter(|layout| layout.entry.as_ref().is_some_and(|entry| entry.process_running))
                .count(),
            removable: space.removable,
            available: space.available,
        }
    }
}

fn tab_count_label(tabs: usize) -> String {
    if tabs == 1 { "1 tab".into() } else { format!("{tabs} tabs") }
}

/// The space name's right-click menu.
pub(crate) fn space_menu(menu: ContextMenu, space: &SpaceMenu, on: Emit) -> ContextMenu {
    let id = space.id;
    let tabs = tab_count_label(space.tabs);
    let mut summary = format!("· {tabs}");
    if space.running > 0 {
        summary.push_str(&format!(" · {} running", space.running));
    }
    let (new_terminal, open, locate, rename, close) =
        (on.clone(), on.clone(), on.clone(), on.clone(), on);
    menu.context_line(Some(space.name.clone()), summary)
        .when(space.available, |menu| {
            menu.entry("New Terminal", None, move |window, cx| {
                new_terminal(Action::NewInSpace { space: id }, window, cx)
            })
            .with_icon(IconName::Terminal.path())
            .with_shortcut(Box::new(crate::actions::workspace::NewTerminal))
            .entry("Open Folder", None, move |window, cx| {
                open(Action::OpenSpaceFolder { space: id }, window, cx)
            })
            .with_icon(IconName::Folder.path())
        })
        .when(!space.available, |menu| {
            menu.entry("Locate Space Folder", None, move |window, cx| {
                locate(Action::LocateSpace { space: id }, window, cx)
            })
            .with_icon(IconName::MagnifyingGlass.path())
        })
        .when(space.removable, |menu| {
            menu.entry("Rename Space…", None, move |window, cx| {
                rename(Action::RenameSpace { space: id }, window, cx)
            })
            .with_icon(IconName::Pencil.path())
            .separator()
            .danger_entry("Close Space", move |window, cx| {
                close(Action::CloseSpace { space: id }, window, cx)
            })
            .with_icon(IconName::Close.path())
            .with_meta(tabs)
        })
}

/// A tab row's right-click menu. `index` is the tab's place among `count` tabs.
pub(crate) fn tab_menu(
    menu: ContextMenu,
    space: EntityId,
    tab: crate::workspace::WorkspaceTabId,
    index: usize,
    count: usize,
    title: &str,
    running: bool,
    grouped: bool,
    on: Emit,
) -> ContextMenu {
    let (rename, up, down, close_others, close) =
        (on.clone(), on.clone(), on.clone(), on.clone(), on);
    menu.context_line(Some(title.to_owned()), if running { "· running" } else { "" })
        .entry("Rename Tab…", None, move |window, cx| {
            rename(Action::RenameGroup { space, tab }, window, cx)
        })
        .with_icon(IconName::Pencil.path())
        // `move_tab` removes the tab before inserting it, so both neighbours
        // are exactly one step away.
        .when(index > 0, |menu| {
            menu.entry("Move Up", None, move |window, cx| {
                up(Action::MoveWorkspaceTab { space, tab, target_index: index - 1 }, window, cx)
            })
            .with_icon(IconName::ArrowUp.path())
        })
        .when(index + 1 < count, |menu| {
            menu.entry("Move Down", None, move |window, cx| {
                down(Action::MoveWorkspaceTab { space, tab, target_index: index + 1 }, window, cx)
            })
            .with_icon(IconName::ArrowDown.path())
        })
        .separator()
        .when(count > 1, |menu| {
            menu.entry("Close Other Tabs", None, move |window, cx| {
                close_others(Action::CloseOtherTabs { space, tab }, window, cx)
            })
            .with_icon(IconName::Close.path())
            .with_meta((count - 1).to_string())
        })
        .danger_entry("Close Tab", move |window, cx| {
            close(Action::CloseGroup { space, tab }, window, cx)
        })
        .with_icon(IconName::Close.path())
        .when(!grouped, |menu| menu.with_shortcut(Box::new(crate::actions::pane::CloseActiveItem)))
}

fn layout_row(
    space: EntityId,
    index: usize,
    count: usize,
    layout: &LayoutEntry,
    selected: bool,
    quiet: QuietColors,
    on: Emit,
    rename: Option<&RenameRows>,
    cx: &App,
) -> AnyElement {
    let tab = layout.tab;
    let rename_input =
        rename.filter(|rename| rename.tab == Some((space, tab))).map(|rename| rename.input.clone());
    let editing = rename_input.is_some();
    let entry = layout.entry.as_ref();
    let title =
        layout_title(index, layout.name.as_deref(), entry.map(|entry| entry.title.as_str()));
    // Lead with the surface's own icon, as upstream does; groups keep the split mark.
    let icon = lead_icon(entry, quiet.muted, cx);
    // Then the live state after the name: paused, working, or done.
    let status = entry.filter(|entry| {
        entry.status.is_some() || entry.ended || entry.bell || entry.process_running
    });
    let status_shows_blocked =
        status.is_some_and(|entry| entry.status == Some(SessionStatus::Blocked));
    let menu_actions = on.clone();
    let menu_title = title.clone();
    let running = entry.is_some_and(|entry| entry.process_running);
    let grouped = entry.is_some_and(|entry| entry.grouped);
    let move_tab = on.clone();
    let dragged =
        DraggedLayout { space, tab, index, title: title.clone(), entry: layout.entry.clone() };
    let active_drag = active_layout_drag(cx);
    let dragging_self =
        active_drag.as_ref().is_some_and(|drag| drag.space == space && drag.tab == tab);
    // The drop line sits over the row edge so the list never shifts.
    let drop_line = active_drag
        .as_ref()
        .filter(|drag| layout_drop_index(drag, space, tab, index).is_some())
        .map(|drag| {
            let group = format!("layout-row-{space:?}-{}", tab.get());
            let line = div().absolute().left(px(4.)).right(px(4.)).h(px(2.)).rounded_full();
            match drop_edge(drag.index, index) {
                DropEdge::Top => line.top(px(-1.)),
                DropEdge::Bottom => line.bottom(px(-1.)),
            }
            .group_drag_over::<DraggedLayout>(group, |line| line.bg(cx.theme().status().info))
            // GPUI applies group drag styles only to elements with a hitbox;
            // this empty style gives the line one without catching events.
            .drag_over::<DraggedLayout>(|line, _, _, _| line)
        });
    let accent = cx.theme().status().info;
    let tint = accent.opacity(0.1);
    // Centre the 2px selected marker on the 1px tree guide, not on its left edge.
    let marker = move || {
        div()
            .debug_selector(move || format!("LAYOUT_SELECTION_{}", tab.get()))
            .absolute()
            .left(px(-TREE_CONTENT_INSET - (SELECTION_MARKER_WIDTH - TREE_GUIDE_WIDTH) / 2.))
            .top(px(5.))
            .bottom(px(5.))
            .w(px(SELECTION_MARKER_WIDTH))
            .rounded_full()
            .bg(accent)
    };
    let row = div()
        .id(format!("layout-row-{space:?}-{}", tab.get()))
        .debug_selector(move || format!("LAYOUT_ROW_{}", tab.get()))
        .group(format!("layout-row-{space:?}-{}", tab.get()))
        .relative()
        .h(px(LAYOUT_ROW_HEIGHT))
        .flex_none()
        .rounded(px(ROW_RADIUS))
        .when(dragging_self, |row| row.opacity(0.4))
        .when(selected, |row| row.bg(tint))
        .text_size(UI_TEXT_SMALL)
        .text_color(if selected { quiet.text } else { quiet.muted })
        .hover(move |style| {
            style.text_color(quiet.text).bg(if selected { tint } else { quiet.selected })
        })
        .role(Role::Tab)
        .aria_selected(selected)
        .aria_label(title.clone())
        .on_click({
            let activate = on.clone();
            move |_, window, cx| activate(Action::ActivateLayout { space, tab }, window, cx)
        })
        .on_drag(dragged, |dragged, _, _, cx| {
            let dragged = dragged.clone();
            let ghost = cx.new(move |_| dragged);
            cx.set_global(ActiveLayoutDrag(ghost.downgrade()));
            ghost
        })
        .can_drop(move |value, _, _| {
            value
                .downcast_ref::<DraggedLayout>()
                .is_some_and(|dragged| layout_drop_index(dragged, space, tab, index).is_some())
        })
        .on_drop(move |dragged: &DraggedLayout, window, cx| {
            if let Some(target_index) = layout_drop_index(dragged, space, tab, index) {
                move_tab(
                    Action::MoveWorkspaceTab { space, tab: dragged.tab, target_index },
                    window,
                    cx,
                );
            }
        })
        .child(
            h_flex()
                .size_full()
                .pl(px(ROW_PADDING))
                // Status sits in a fixed right-edge slot, so it never follows the title.
                .pr(px(STATUS_COLUMN_PADDING))
                .gap(px(6.))
                .child(div().flex_none().w(px(13.)).child(icon))
                .child(if let Some(input) = rename_input {
                    inline_rename_field(
                        format!("rename-tab-{space:?}-{}", tab.get()),
                        input,
                        "Enter to save · Escape to cancel · Leave blank for the default name",
                        on.clone(),
                        cx,
                    )
                } else {
                    div().flex_1().min_w_0().truncate().child(title).into_any_element()
                })
                .when(!editing, |row| {
                    row.child(
                        div()
                            .debug_selector(move || format!("LAYOUT_STATUS_{}", tab.get()))
                            .flex_none()
                            .size(crate::design::STATUS_TILE)
                            // Needs-you outranks this tab's own state when another pane waits.
                            .map(|slot| {
                                if layout.needs_you && !status_shows_blocked {
                                    slot.child(crate::design::needs_you_tile(cx))
                                } else {
                                    slot.children(status.map(|entry| {
                                        item_indicator(
                                            entry.activity(),
                                            None,
                                            false,
                                            &entry.space_key,
                                            entry.key,
                                            cx,
                                        )
                                    }))
                                }
                            }),
                    )
                }),
        )
        .when(selected, |row| row.child(marker()))
        .children(drop_line);
    popup_right_click_menu(format!("layout-row-menu-{space:?}-{}", tab.get()))
        // Hold the hover look while this row's menu is open.
        .trigger(move |open, _, _| {
            row.when(open, |row| {
                row.bg(if selected { tint } else { quiet.selected }).text_color(quiet.text)
            })
        })
        .menu(move |window, cx| {
            let on = menu_actions.clone();
            let title = menu_title.clone();
            ContextMenu::build_popup(window, cx, move |menu| {
                tab_menu(menu, space, tab, index, count, &title, running, grouped, on)
            })
        })
        .into_any_element()
}

#[derive(Clone, Copy)]
struct QuietColors {
    text: Hsla,
    muted: Hsla,
    selected: Hsla,
    guide: Hsla,
}

/// A row's hover amount, eased over a short fade the way the mockup's CSS
/// transitions do; GPUI's own hover styles switch instantly.
#[derive(Clone, Copy, Default)]
struct HoverFade {
    hovered: bool,
    from: f32,
    changed: Option<std::time::Instant>,
}

impl HoverFade {
    fn level(&self, now: std::time::Instant, reduce_motion: bool) -> (f32, bool) {
        let to = if self.hovered { 1. } else { 0. };
        let Some(changed) = self.changed.filter(|_| !reduce_motion) else {
            return (to, false);
        };
        let t = now.saturating_duration_since(changed).as_secs_f32() / HOVER_FADE.as_secs_f32();
        if t >= 1. {
            return (to, false);
        }
        let eased = 1. - (1. - t).powi(3);
        (self.from + (to - self.from) * eased, true)
    }

    fn set(&mut self, hovered: bool, now: std::time::Instant, reduce_motion: bool) {
        if hovered != self.hovered {
            self.from = self.level(now, reduce_motion).0;
            self.hovered = hovered;
            self.changed = Some(now);
        }
    }
}

/// One chevron that turns a quarter as the space opens or closes. Keying the
/// animation by toggle count replays it once per toggle and never on mount.
fn chevron(
    space: EntityId,
    turn: usize,
    collapsed: bool,
    color: Hsla,
    reduce_motion: bool,
) -> AnyElement {
    let (from, to) = if collapsed { (0.25, 0.) } else { (0., 0.25) };
    let icon = svg()
        .path(IconName::ChevronRight.path())
        .size(IconSize::Small.rems())
        .flex_none()
        .text_color(color);
    if reduce_motion || turn == 0 {
        return icon.with_transformation(Transformation::rotate(percentage(to))).into_any_element();
    }
    icon.with_animation(
        format!("space-chevron-turn-{space:?}-{turn}"),
        Animation::new(CHEVRON_TURN).with_easing(gpui::ease_out_quint()),
        move |icon, t| {
            icon.with_transformation(Transformation::rotate(percentage(from + (to - from) * t)))
        },
    )
    .into_any_element()
}

/// The only "needs you" cue in the space list: an amber bell tile after the name.
/// It follows the agent's state and clears only when the agent stops waiting.
fn needs_you_dot(cx: &App) -> impl IntoElement {
    div().flex_none().ml(px(6.)).child(crate::design::needs_you_tile(cx))
}

#[cfg(test)]
mod tests {
    use super::{
        DraggedLayout, DropEdge, HOVER_FADE, HoverFade, drop_edge, layout_drop_index, layout_name,
        layout_title,
    };
    use std::time::Instant;

    #[test]
    fn hover_fades_in_reverses_from_where_it_is_and_skips_reduced_motion() {
        let start = Instant::now();
        let mut hover = HoverFade::default();
        assert_eq!(hover.level(start, false), (0., false));
        hover.set(true, start, false);
        let (half, moving) = hover.level(start + HOVER_FADE / 2, false);
        assert!(moving && half > 0.5 && half < 1.);
        hover.set(false, start + HOVER_FADE / 2, false);
        let (back, _) = hover.level(start + HOVER_FADE / 2, false);
        assert!((back - half).abs() < 1e-6, "reversal starts from the current level");
        assert_eq!(hover.level(start + HOVER_FADE * 2, false), (0., false));
        hover.set(true, start, true);
        assert_eq!(hover.level(start, true), (1., false));
    }

    #[test]
    fn sidebar_drag_only_accepts_another_tab_in_the_same_space() {
        let space = 10_u64.into();
        let other_space = 11_u64.into();
        let mut tabs = crate::workspace::WorkspaceTabs::new();
        let first_item = tabs.alloc_item();
        let second_item = tabs.alloc_item();
        let first = tabs.push_standalone(first_item).unwrap();
        let second = tabs.push_standalone(second_item).unwrap();
        let dragged =
            DraggedLayout { space, tab: first, index: 0, title: "First".into(), entry: None };
        assert_eq!(layout_drop_index(&dragged, space, second, 1), Some(1));
        assert_eq!(layout_drop_index(&dragged, space, first, 0), None);
        assert_eq!(layout_drop_index(&dragged, other_space, second, 1), None);
    }

    #[test]
    fn drop_line_sits_below_when_dragging_down_and_above_when_dragging_up() {
        assert_eq!(drop_edge(0, 1), DropEdge::Bottom);
        assert_eq!(drop_edge(0, 4), DropEdge::Bottom);
        assert_eq!(drop_edge(3, 1), DropEdge::Top);
        assert_eq!(drop_edge(1, 0), DropEdge::Top);
    }

    #[test]
    fn layout_names_keep_the_first_layout_primary_and_the_rest_ordinal() {
        assert_eq!(layout_name(0, None), "Main layout");
        assert_eq!(layout_name(1, None), "Layout 2");
        assert_eq!(layout_name(4, None), "Layout 5");
        assert_eq!(layout_name(0, Some("Build")), "Build");
        assert_eq!(layout_title(0, Some("My Pi"), Some("Pi")), "My Pi");
        assert_eq!(layout_title(1, None, Some("Settings")), "Settings");
    }
}
