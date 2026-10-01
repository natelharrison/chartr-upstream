//! Tabs mode: the sidebar's space tree turned sideways.
//!
//! Every space is a quiet tab. The active space is outlined and holds its
//! layouts, as its open row does in the sidebar. The items arrive from the
//! left, where the sidebar rows just left.

use gpui::{Entity, FontWeight, Hsla};
use ui::{ButtonSize, IconButtonShape, Tooltip, prelude::*};

use super::Emit;

use super::tab_sorter::{SortableTab, SortableTabList};
use super::{Action, DraggedItem, Entry, ItemTab, SpaceEntries};
use crate::components::SortAxis;
use crate::components::{ContextMenu, popup_right_click_menu};
use crate::fonts::UI_TEXT_DEFAULT;
use crate::settings::SettingsStore;
use crate::workspace::WorkspaceTabId;

/// Strip height; the Inset surface adds a gap below it.
pub(crate) const HEIGHT: f32 = 30.;
/// How far each item travels in from the left while the strip opens.
const ARRIVAL_OFFSET: f32 = 28.;
/// Stagger between neighbouring items, capped after the sixth.
const ARRIVAL_STAGGER: f32 = 0.03;

type HoveredTab = Option<(gpui::EntityId, WorkspaceTabId)>;

/// Recover linear progress from the eased mode reveal, then delay each item
/// a little more than the last so they arrive one after another.
fn arrival(reveal: f32, index: usize) -> f32 {
    if reveal >= 1. {
        return 1.;
    }
    let scale = 1. - 2_f32.powi(-10);
    let linear = (-(1. - reveal.clamp(0., 1.) * scale).log2() / 10.).clamp(0., 1.);
    let delay = ARRIVAL_STAGGER * index.min(5) as f32;
    let local = ((linear - delay) / (1. - delay)).clamp(0., 1.);
    (1. - 2_f32.powf(-10. * local)) / scale
}

fn arriving(item: impl IntoElement, reveal: f32, index: usize) -> AnyElement {
    let t = arrival(reveal, index);
    div()
        .relative()
        .flex_none()
        .left(px(-ARRIVAL_OFFSET * (1. - t)))
        .opacity(t)
        .child(item)
        .into_any_element()
}

fn needs_you_dot(_color: Hsla, cx: &App) -> impl IntoElement {
    crate::design::needs_you_tile(cx)
}

#[allow(clippy::too_many_arguments)]
pub fn render(
    spaces: &[SpaceEntries],
    entries: &[Entry],
    end_control: Option<AnyElement>,
    create: AnyElement,
    reveal: f32,
    on: Emit,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    // Use stable workspace identities so hover cannot move to an unrelated
    // tab when entries are reordered, closed, or the current space changes.
    let hovered_tab = window.use_keyed_state("workspace-tab-hover", cx, |_, _| HoveredTab::None);
    let colors = cx.theme().colors();
    let text = colors.text;
    let muted = colors.text_muted;
    let outline = colors.border_variant;
    let needs_you = cx.theme().status().warning;
    let move_tab = on.clone();
    let space = entries.first().map(|entry| entry.space);
    let space_key = entries.first().map(|entry| entry.space_key.as_str()).unwrap_or_default();
    let active = spaces.iter().find(|space| space.active);
    let tabs = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let dragged = DraggedItem {
                space: entry.space_key.clone(),
                tab: entry.tab,
                pane: entry.pane,
                index,
                item: entry.key,
                top_level: true,
                grouped: entry.grouped,
            };
            let layout_needs_you = active.is_some_and(|space| {
                space.layouts.iter().any(|layout| layout.tab == entry.tab && layout.needs_you)
            });
            let entry = entry.clone();
            let on = on.clone();
            let hovered_tab = hovered_tab.clone();
            SortableTab::new(dragged, move |placement, cx| {
                h_flex()
                    .gap(px(5.))
                    .child(tab(placement.index, &entry, &hovered_tab, on, cx))
                    .when(layout_needs_you, |tab| tab.child(needs_you_dot(needs_you, cx)))
                    .into_any_element()
            })
        })
        .collect();
    let list = SortableTabList::new(
        format!("workspace-tab-sorter-{space_key}"),
        h_flex().id("workspace-tab-list").flex_none().h(px(24.)),
        SortAxis::Horizontal,
        gpui::rems(2. / 14.),
        tabs,
        move |dragged, target_index, window, cx| {
            if let Some(space) = space {
                move_tab(
                    Action::MoveWorkspaceTab { space, tab: dragged.tab, target_index },
                    window,
                    cx,
                );
            }
        },
    );
    let mut list = Some(list);
    // The strip's + and ▧ ride at the end of the current space's group, so
    // they read as "add to this space" and stay apart from the pane bar's.
    let mut create = Some(create);

    let mut items = Vec::with_capacity(spaces.len() + 1);
    for (index, space) in spaces.iter().enumerate() {
        let id = space.id;
        let activate = on.clone();
        let label = h_flex()
            .id(format!("strip-space-{id:?}"))
            .flex_none()
            .h(px(if space.active { 24. } else { 26. }))
            .pl(px(10.))
            .pr(px(7.))
            .gap(px(8.))
            .rounded(crate::design::RADIUS_CONTROL)
            .role(gpui::Role::Tab)
            .aria_label(space.name.clone())
            .aria_selected(space.active)
            .text_size(UI_TEXT_DEFAULT)
            .font_weight(if space.active { FontWeight::SEMIBOLD } else { FontWeight::MEDIUM })
            .text_color(if space.active { text } else { muted })
            .hover(move |style| style.text_color(text))
            .debug_selector(move || format!("STRIP_SPACE_{index}"))
            .on_click(move |_, window, cx| {
                activate(Action::ActivateSpace { space: id }, window, cx)
            })
            .child(space.name.clone())
            .when(!space.active && space.layouts.iter().any(|layout| layout.needs_you), |tab| {
                tab.child(needs_you_dot(needs_you, cx))
            });
        let item = if space.active {
            h_flex()
                .flex_none()
                .mx(px(4.))
                .pl(px(1.))
                .pr(px(2.))
                .py(px(1.))
                .gap(px(2.))
                .border_1()
                .border_color(outline)
                .rounded(crate::design::RADIUS_MENU)
                .child(label)
                .children(list.take())
                .children(create.take().map(|create| div().ml(px(2.)).child(create)))
                .into_any_element()
        } else {
            label.into_any_element()
        };
        items.push(arriving(item, reveal, index));
    }
    let add_space = on.clone();
    items.push(arriving(
        h_flex()
            .id("strip-new-space")
            .debug_selector(|| "STRIP_NEW_SPACE".into())
            .flex_none()
            .h(px(26.))
            .px(px(8.))
            .gap(px(6.))
            .rounded(crate::design::RADIUS_CONTROL)
            .role(gpui::Role::Button)
            .aria_label("New Space")
            .text_size(UI_TEXT_DEFAULT)
            .text_color(muted)
            .hover(move |style| style.text_color(text))
            .tooltip(Tooltip::text("New space"))
            .on_click(move |_, window, cx| add_space(Action::NewSpace, window, cx))
            .child(Icon::new(IconName::Plus).size(IconSize::Small).color(Color::Muted)),
        reveal,
        spaces.len(),
    ));

    h_flex()
        .id("workspace-tabs")
        .group("tab_bar")
        .w_full()
        .h(px(HEIGHT))
        .flex_none()
        .gap(px(4.))
        .child(
            h_flex()
                .id("workspace-strip-tree")
                .flex_1()
                .min_w_0()
                .h_full()
                .gap(px(2.))
                .overflow_x_scroll()
                .children(items),
        )
        .child(h_flex().flex_none().pr(px(2.)).gap(px(4.)).children(create).children(end_control))
}

fn tab(
    index: usize,
    entry: &Entry,
    hovered_tab: &Entity<HoveredTab>,
    on: Emit,
    cx: &App,
) -> AnyElement {
    let close = on.clone();
    let middle_close = on.clone();
    let middle_click_closes_tab = cx.global::<SettingsStore>().resolved().middle_click_closes_tab;
    let select = entry.key;
    let select_item = on.clone();
    let ungroup = on.clone();
    let rename = on.clone();
    let move_tab = on;
    let close_key = entry.key;
    let close_tab = entry.tab;
    let grouped = entry.grouped;
    let space = entry.space;
    let close_space = entry.space;
    let target_index = index;
    let target_space_key = entry.space_key.clone();
    let hover_key = (entry.space, entry.tab);
    let hovered = *hovered_tab.read(cx) == Some(hover_key);
    let hovered_tab = hovered_tab.clone();
    let close_slot: Option<AnyElement> = entry.closable.then(|| {
        IconButton::new(("close", index), IconName::Close)
            .shape(IconButtonShape::Square)
            .size(ButtonSize::None)
            .icon_size(IconSize::XSmall)
            .icon_color(if entry.selected || hovered { Color::Default } else { Color::Muted })
            .tooltip(Tooltip::text("Close"))
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                close(
                    if grouped {
                        Action::CloseGroup { space: close_space, tab: close_tab }
                    } else {
                        Action::Close { space: Some(close_space), item: close_key }
                    },
                    window,
                    cx,
                )
            })
            .into_any_element()
    });
    let aria_label =
        if entry.grouped { format!("Pane group: {}", entry.title) } else { entry.title.clone() };
    let tab = ItemTab::new(
        format!("workspace-tab-{space:?}-{}", entry.tab.get()),
        entry.title.clone(),
        entry.selected,
        &entry.space_key,
        entry.key,
    )
    .aria_label(aria_label)
    .activity(entry.activity())
    .icon_path(entry.icon_path.clone())
    .grouped(entry.grouped)
    .close_slot(close_slot)
    .build_rounded(hovered, cx)
    .debug_selector(move || format!("WORKSPACE_TAB_{}", close_tab.get()))
    .on_hover(move |is_hovered, window, cx| {
        hovered_tab.update(cx, |hovered, _| {
            let next = if *is_hovered {
                Some(hover_key)
            } else if *hovered == Some(hover_key) {
                None
            } else {
                *hovered
            };
            if *hovered != next {
                *hovered = next;
                window.refresh();
            }
        });
    })
    .on_click(move |_, window, cx| {
        select_item(Action::Select { space: Some(space), item: select }, window, cx)
    })
    .when(entry.closable && middle_click_closes_tab, |tab| {
        tab.on_aux_click(move |event, window, cx| {
            if event.is_middle_click() {
                cx.stop_propagation();
                middle_close(
                    if grouped {
                        Action::CloseGroup { space: close_space, tab: close_tab }
                    } else {
                        Action::Close { space: Some(close_space), item: close_key }
                    },
                    window,
                    cx,
                );
            }
        })
    })
    .can_drop(move |value, _, _| {
        value
            .downcast_ref::<DraggedItem>()
            .is_some_and(|dragged| dragged.space == target_space_key && dragged.top_level)
    })
    .on_drop(move |dragged: &DraggedItem, window, cx| {
        move_tab(Action::MoveWorkspaceTab { space, tab: dragged.tab, target_index }, window, cx);
    });

    if grouped {
        // Keep the menu wrapper as wide as the tab itself.
        div()
            .flex_none()
            .child(
                popup_right_click_menu(format!("group-tab-menu-{space:?}-{}", close_tab.get()))
                    .trigger(move |_, _, _| tab)
                    .menu(move |window, cx| {
                        let ungroup = ungroup.clone();
                        let rename = rename.clone();
                        ContextMenu::build_popup(window, cx, move |menu| {
                            menu.entry("Rename Tab", None, move |window, cx| {
                                rename(Action::RenameGroup { space, tab: close_tab }, window, cx)
                            })
                            .entry(
                                "Ungroup",
                                None,
                                move |window, cx| {
                                    ungroup(
                                        Action::UngroupPane { space, tab: close_tab },
                                        window,
                                        cx,
                                    )
                                },
                            )
                        })
                    }),
            )
            .into_any_element()
    } else {
        tab.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Context, Modifiers, Render, TestAppContext, point};
    use std::rc::Rc;

    struct Harness {
        grouped: bool,
        width: f32,
    }

    impl Render for Harness {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let hovered = window.use_keyed_state("test-hover", cx, |_, _| HoveredTab::None);
            let entry = Entry {
                space: cx.entity_id(),
                space_key: "test".into(),
                key: serde_json::from_value(serde_json::json!(1)).unwrap(),
                tab: serde_json::from_value(serde_json::json!(1)).unwrap(),
                pane: serde_json::from_value(serde_json::json!(1)).unwrap(),
                index: 0,
                title: if self.grouped { "2 tabs" } else { "codex" }.into(),
                icon_path: None,
                status: None,
                process_running: false,
                ended: false,
                bell: false,
                selected: false,
                closable: true,
                grouped: self.grouped,
            };
            div().size_full().child(h_flex().w(px(self.width)).child(tab(
                0,
                &entry,
                &hovered,
                Rc::new(|_, _, _| {}),
                cx,
            )))
        }
    }

    #[test]
    fn strip_items_arrive_in_order_and_settle_together() {
        for index in 0..8 {
            assert_eq!(arrival(0., index), 0.);
            assert_eq!(arrival(1., index), 1.);
        }
        for reveal in [0.2, 0.5, 0.9] {
            let first = arrival(reveal, 0);
            assert!((first - reveal).abs() < 1e-4, "the first item follows the strip");
            for index in 1..8 {
                assert!(arrival(reveal, index) <= arrival(reveal, index - 1));
            }
            assert_eq!(arrival(reveal, 5), arrival(reveal, 7), "the stagger is capped");
        }
    }

    #[gpui::test]
    fn strip_tabs_fit_their_title_and_hold_still_on_hover(cx: &mut TestAppContext) {
        cx.update(|cx| {
            ::settings::init(cx);
            theme::init(theme::LoadThemes::JustBase, cx);
            crate::fonts::install(&crate::settings::ResolvedSettings::default(), cx);
            cx.set_global(SettingsStore::bare());
        });
        for grouped in [false, true] {
            let (view, cx) = cx.add_window_view(|_, _| Harness { grouped, width: 200. });
            for width in [200., 110., 200.] {
                view.update(cx, |view, cx| {
                    view.width = width;
                    cx.notify();
                });
                cx.simulate_mouse_move(point(px(400.), px(100.)), None, Modifiers::none());
                cx.run_until_parked();
                let before = cx.debug_bounds("WORKSPACE_TAB_1").unwrap();
                assert!(before.size.width < px(110.), "grouped={grouped}");
                cx.simulate_mouse_move(
                    point(before.right() - px(5.), before.center().y),
                    None,
                    Modifiers::none(),
                );
                cx.run_until_parked();
                assert_eq!(cx.debug_bounds("WORKSPACE_TAB_1").unwrap(), before);
            }
        }
    }
}
