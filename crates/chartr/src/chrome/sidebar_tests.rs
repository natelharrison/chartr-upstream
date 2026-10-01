use gpui::{
    Bounds, Context, Modifiers, MouseButton, Render, ScrollHandle, TestAppContext, point, size,
};
use ui::{ScrollAxes, Scrollbars, WithScrollbar, prelude::*};

struct ScrollbarHarness {
    handle: ScrollHandle,
}

impl Render for ScrollbarHarness {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .size(px(200.))
            .child(
                div()
                    .id("sessions")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.handle)
                    // A space heading must not steal the thumb's mouse-down event.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(div().h(px(1000.)).w_full()),
            )
            .custom_scrollbars(
                Scrollbars::on_hover(ScrollAxes::Vertical)
                    .id("spaces-scrollbar")
                    .tracked_scroll_handle(&self.handle)
                    .notify_content(),
                window,
                cx,
            )
    }
}

#[gpui::test]
fn sidebar_scrollbar_drags_past_the_container_and_releases(cx: &mut TestAppContext) {
    cx.update(|cx| {
        ::settings::init(cx);
        theme::init(theme::LoadThemes::JustBase, cx);
    });
    let handle = ScrollHandle::new();
    let (_, cx) = cx.add_window_view(|_, _| ScrollbarHarness { handle: handle.clone() });
    cx.simulate_mouse_move(point(px(250.), px(250.)), None, Modifiers::none());
    let hidden_quad_count = cx.update(|window, _| window.painted_quads().len());
    cx.simulate_mouse_move(point(px(50.), px(50.)), None, Modifiers::none());
    assert_eq!(
        cx.update(|window, _| window.painted_quads().len()),
        hidden_quad_count + 1,
        "entering the container must paint the thumb",
    );
    let thumb = point(px(193.), px(20.));
    cx.simulate_mouse_move(thumb, None, Modifiers::none());
    cx.simulate_mouse_down(thumb, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(point(px(193.), px(60.)), MouseButton::Left, Modifiers::none());
    let inside_offset = handle.offset().y;
    assert!(inside_offset < px(-100.), "thumb drag must scroll inside the container");

    let outside = point(px(250.), px(150.));
    cx.simulate_mouse_move(outside, MouseButton::Left, Modifiers::none());
    assert!(handle.offset().y < inside_offset, "drag must continue outside the container");
    assert_eq!(
        cx.update(|window, _| window.painted_quads().len()),
        hidden_quad_count + 1,
        "the thumb must remain visible throughout the drag",
    );
    cx.simulate_mouse_up(outside, MouseButton::Left, Modifiers::none());
    assert_eq!(
        cx.update(|window, _| window.painted_quads().len()),
        hidden_quad_count,
        "releasing outside must hide the thumb",
    );
    let released_offset = handle.offset();
    cx.simulate_mouse_move(point(px(250.), px(50.)), None, Modifiers::none());
    assert_eq!(handle.offset(), released_offset, "release must end scrolling");
}

#[gpui::test]
fn sidebar_scrollbar_has_contrast_in_every_theme(cx: &mut TestAppContext) {
    fn luminance(color: gpui::Hsla) -> f32 {
        let color = gpui::Rgba::from(color);
        let linear =
            |v: f32| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
        0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
    }
    cx.update(|cx| {
        theme::init(theme::LoadThemes::JustBase, cx);
        crate::settings::init_themes(&crate::settings::ResolvedSettings::default(), cx);
        let registry = theme::ThemeRegistry::global(cx);
        for name in registry.list_names() {
            let theme = registry.get(&name).unwrap();
            let colors = &theme.styles.colors;
            let sidebar = crate::settings::sidebar_theme_colors(&theme);
            for thumb in crate::components::scrollbar_thumb_colors(colors) {
                assert_eq!(thumb.a, 1.);
                for background in
                    [colors.panel_background, sidebar.card_active, sidebar.card_inactive]
                {
                    let a = luminance(thumb);
                    let b = luminance(background);
                    let contrast = (a.max(b) + 0.05) / (a.min(b) + 0.05);
                    assert!(contrast >= 3., "{name}: scrollbar contrast is only {contrast:.2}:1");
                }
            }
        }
    });
}

/// Exercise the real sidebar so bubbling clicks and collapsed layout are tested
/// together with the same scroll geometry used for space sorting.
struct TreeHarness {
    spaces: Vec<super::SpaceEntries>,
    sorter: super::sidebar::SpaceSorter,
    actions: Vec<super::Action>,
    width: Pixels,
    rename: Option<super::sidebar::RenameRows>,
    rename_input:
        Option<(gpui::Entity<crate::text_input::TextInput>, gpui::FocusHandle, gpui::FocusHandle)>,
}

impl Render for TreeHarness {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let on = cx.listener(|this, action: &super::Action, window, cx| {
            if let Some((input, _, _)) = this.rename_input.clone() {
                use gpui::Focusable;
                let target = match action {
                    super::Action::RenameGroup { space, tab } => {
                        Some((None, Some((*space, *tab)), "Initial tab"))
                    }
                    super::Action::RenameSpace { space } => Some((Some(*space), None, "Project")),
                    _ => None,
                };
                if let Some((space, tab, name)) = target {
                    input.update(cx, |input, cx| input.set_text(name, true, cx));
                    this.rename =
                        Some(super::sidebar::RenameRows { space, tab, input: input.clone() });
                    window.focus(&input.focus_handle(cx), cx);
                }
            }
            if let super::Action::BeginSpaceDrag { at } = action {
                this.sorter.press(*at);
            }
            if let super::Action::ToggleSpaceCollapsed { space } = action {
                let space = this.spaces.iter_mut().find(|entry| entry.id == *space).unwrap();
                space.collapsed = !space.collapsed;
            }
            if matches!(action, super::Action::CommitRename) {
                if let Some(rename) = this.rename.take() {
                    let name = rename.input.read(cx).text().trim().to_owned();
                    if let Some(id) = rename.space {
                        this.spaces.iter_mut().find(|space| space.id == id).unwrap().name = name;
                    } else if let Some((id, tab)) = rename.tab {
                        let space = this.spaces.iter_mut().find(|space| space.id == id).unwrap();
                        space.layouts.iter_mut().find(|layout| layout.tab == tab).unwrap().name =
                            (!name.is_empty()).then_some(name);
                    }
                }
            } else if matches!(action, super::Action::CancelRename) {
                this.rename = None;
            }
            this.actions.push(action.clone());
            cx.notify();
        });
        let mut spaces = self.spaces.clone();
        self.sorter.arrange(&mut spaces, |space| space.id);
        div()
            .when_some(self.rename_input.as_ref(), |tree, (_, root, terminal)| {
                tree.track_focus(root).child(div().track_focus(terminal).size(px(1.)))
            })
            .w(self.width)
            .h(px(400.))
            .on_drag_move::<super::DraggedSpace>(cx.listener(
                |this, event: &gpui::DragMoveEvent<super::DraggedSpace>, window, cx| {
                    let order = this.spaces.iter().map(|space| space.id).collect();
                    if this.sorter.drag_move(
                        event.drag(cx).0,
                        order,
                        event.event.position,
                        window.rem_size(),
                        cx.background_executor().now(),
                        true,
                    ) {
                        cx.notify();
                    }
                },
            ))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseUpEvent, window, cx| {
                    if let Some((space, target)) = this.sorter.drop_at(
                        event.position.y,
                        window.rem_size(),
                        cx.background_executor().now(),
                        cx.reduce_motion(),
                    ) {
                        let from = this.spaces.iter().position(|entry| entry.id == space).unwrap();
                        let moved = this.spaces.remove(from);
                        this.spaces.insert(target, moved);
                        this.sorter.accept_drop(cx.background_executor().now(), cx.reduce_motion());
                        cx.notify();
                    }
                }),
            )
            .child(super::sidebar::render(
                &spaces,
                &super::AgentChoices { names: vec!["Pi".into()], last_used: None },
                &[super::SurfaceOption {
                    title: "Wayfinder".into(),
                    key: chartr_plugin::PaneKey::new("com.chartr.wayfinder", "main"),
                    icon_path: crate::assets::PLUGIN_LAUNCHER_ICON_PATH.into(),
                    external_icon: false,
                }],
                std::rc::Rc::new(move |action, window, cx| on(&action, window, cx)),
                &self.sorter,
                self.rename.as_ref(),
                window,
                cx,
            ))
    }
}

#[gpui::test]
fn sidebar_tree_collapses_adds_and_sorts_free_sessions(cx: &mut TestAppContext) {
    use super::{Action, LayoutEntry, SpaceEntries};
    use crate::workspace::WorkspaceTabs;
    cx.update(|cx| {
        ::settings::init(cx);
        theme::init(theme::LoadThemes::JustBase, cx);
        crate::fonts::install(&crate::settings::ResolvedSettings::default(), cx);
    });
    let free_id = 10_u64.into();
    let folder_id = 11_u64.into();
    let mut layouts = WorkspaceTabs::new();
    let first = layouts.alloc_item();
    let first_layout = layouts.push_standalone(first).unwrap();
    let second = layouts.alloc_item();
    let second_layout = layouts.push_standalone(second).unwrap();
    let (view, cx) = cx.add_window_view(|_, _| TreeHarness {
        spaces: vec![
            SpaceEntries {
                id: free_id,
                name: "Scratch".into(),
                collapsed: false,
                active: true,
                removable: false,
                available: true,
                layouts: vec![],
                activity: None,
            },
            SpaceEntries {
                id: folder_id,
                name: "Project".into(),
                collapsed: false,
                active: false,
                removable: true,
                available: true,
                layouts: vec![
                    LayoutEntry {
                        tab: first_layout,
                        name: None,
                        selected: false,
                        needs_you: false,
                        entry: None,
                    },
                    LayoutEntry {
                        tab: second_layout,
                        name: None,
                        selected: true,
                        needs_you: false,
                        entry: None,
                    },
                ],
                activity: None,
            },
        ],
        sorter: super::sidebar::SpaceSorter::new(super::sidebar::CARD_GAP).with_trailing(1),
        actions: vec![],
        width: px(280.),
        rename: None,
        rename_input: None,
    });
    cx.run_until_parked();
    let bounds = |cx: &mut gpui::VisualTestContext, index| {
        view.read_with(cx, |view, _| view.sorter.scroll_handle().bounds_for_item(index).unwrap())
    };
    let expanded = bounds(cx, 0);
    let project_before = bounds(cx, 1);
    // Scratch is a regular child in the shared sorter scroll list.
    assert!(project_before.top() > expanded.bottom());
    // The disclosure owns expansion; it is the only control in the row that
    // collapses the space.
    cx.simulate_click(expanded.origin + point(px(12.), px(12.)), Modifiers::none());
    cx.run_until_parked();
    let collapsed = bounds(cx, 0);
    assert!(collapsed.size.height < expanded.size.height);
    assert!(bounds(cx, 1).top() < project_before.top());
    assert!(view.read_with(cx, |view, _| view.spaces[0].collapsed));

    // Click the chevron to reopen the same row.
    cx.simulate_click(collapsed.origin + point(px(12.), px(12.)), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(bounds(cx, 0).size.height, expanded.size.height);
    view.update(cx, |view, _| view.actions.clear());

    // Clicking the space name activates that space, without toggling its
    // disclosure or selecting a representative session item.
    let expanded = bounds(cx, 0);
    cx.simulate_click(expanded.origin + point(px(64.), px(12.)), Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.actions.last(), Some(&Action::ActivateSpace { space: free_id }));
        assert!(!view.spaces[0].collapsed);
    });

    // Tab-row status shares a centre line with the space row's last action,
    // whatever the title length.
    let status = cx.debug_bounds("LAYOUT_STATUS_2").expect("status slot");
    let create_trigger = cx.debug_bounds("SIDEBAR_CREATE_TRIGGER").expect("create trigger");
    assert_eq!(status.center().x, create_trigger.center().x);

    // A layout row emits its stable layout identity rather than selecting the
    // item used to summarize its pane tree.
    view.update(cx, |view, _| view.actions.clear());
    let layout_row = cx.debug_bounds("LAYOUT_ROW_2").unwrap();
    cx.simulate_click(layout_row.center(), Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.actions,
            vec![Action::ActivateLayout { space: folder_id, tab: second_layout }]
        );
    });
    view.update(cx, |view, _| view.actions.clear());

    // Dragging one layout row onto another reorders tabs in that space.
    let first_row = cx.debug_bounds("LAYOUT_ROW_1").unwrap();
    let second_row = cx.debug_bounds("LAYOUT_ROW_2").unwrap();
    let start = first_row.center();
    let target = second_row.center();
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(start + point(px(0.), px(8.)), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(target, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(target, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.actions.contains(&Action::MoveWorkspaceTab {
            space: folder_id,
            tab: first_layout,
            target_index: 1,
        }));
    });
    view.update(cx, |view, _| view.actions.clear());

    // The row's + creates a terminal in one click, without opening a menu.
    let parent = cx.window_handle();
    let new_terminal = cx.debug_bounds("SIDEBAR_NEW_TERMINAL").expect("new terminal button");
    cx.simulate_click(new_terminal.center(), Modifiers::none());
    cx.run_until_parked();
    assert!(cx.windows().into_iter().all(|window| window == parent));
    view.read_with(cx, |view, _| {
        assert_eq!(view.actions, vec![Action::NewInSpace { space: free_id }]);
        assert!(!view.spaces[0].collapsed);
    });
    view.update(cx, |view, _| view.actions.clear());

    // The surface glyph opens the remaining creation choices. Opening it must
    // not begin a drag or collapse the space.
    let create_trigger = cx.debug_bounds("SIDEBAR_CREATE_TRIGGER").expect("create trigger");
    cx.simulate_click(create_trigger.center(), Modifiers::none());
    cx.run_until_parked();
    let popup = cx.windows().into_iter().find(|window| *window != parent).expect("create menu");
    {
        let mut popup = gpui::VisualTestContext::from_window(popup, cx);
        popup.run_until_parked();
        let entry = popup.debug_bounds("MENU_ITEM-Terminal").expect("terminal entry");
        popup.simulate_click(entry.center(), Modifiers::none());
    }
    view.read_with(cx, |view, _| {
        assert_eq!(view.actions, vec![Action::NewInSpace { space: free_id }]);
        assert!(!view.spaces[0].collapsed);
    });

    view.update(cx, |view, _| view.actions.clear());
    let create_trigger = cx.debug_bounds("SIDEBAR_CREATE_TRIGGER").expect("create trigger");
    cx.simulate_click(create_trigger.center(), Modifiers::none());
    cx.run_until_parked();
    let popup = cx.windows().into_iter().find(|window| *window != parent).expect("create menu");
    {
        let mut popup = gpui::VisualTestContext::from_window(popup, cx);
        popup.run_until_parked();
        let entry = popup.debug_bounds("MENU_ITEM-Pi").expect("agent entry");
        popup.simulate_click(entry.center(), Modifiers::none());
    }
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.actions,
            vec![Action::StartAgentInSpace { space: free_id, name: "Pi".into() }]
        );
    });

    view.update(cx, |view, _| view.actions.clear());
    let create_trigger = cx.debug_bounds("SIDEBAR_CREATE_TRIGGER").expect("create trigger");
    cx.simulate_click(create_trigger.center(), Modifiers::none());
    cx.run_until_parked();
    let popup = cx.windows().into_iter().find(|window| *window != parent).expect("create menu");
    {
        let mut popup = gpui::VisualTestContext::from_window(popup, cx);
        popup.run_until_parked();
        let entry = popup.debug_bounds("MENU_ITEM-More surfaces…").expect("surface browser entry");
        popup.simulate_click(entry.center(), Modifiers::none());
    }
    view.read_with(cx, |view, _| {
        assert_eq!(view.actions, vec![Action::NewPluginPaneInSpace { space: free_id }]);
    });

    view.update(cx, |view, _| view.actions.clear());
    let create_trigger = cx.debug_bounds("SIDEBAR_CREATE_TRIGGER").expect("create trigger");
    cx.simulate_click(create_trigger.center(), Modifiers::none());
    cx.run_until_parked();
    let popup = cx.windows().into_iter().find(|window| *window != parent).expect("create menu");
    {
        let mut popup = gpui::VisualTestContext::from_window(popup, cx);
        popup.run_until_parked();
        let entry = popup.debug_bounds("MENU_ITEM-Wayfinder").expect("surface entry");
        popup.simulate_click(entry.center(), Modifiers::none());
    }
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.actions,
            vec![Action::NewSurfaceInSpace {
                space: free_id,
                key: chartr_plugin::PaneKey::new("com.chartr.wayfinder", "main"),
            }]
        );
    });

    // New space is the list's last row, so every row + means "add to this space".
    view.update(cx, |view, _| view.actions.clear());
    let new_space = cx.debug_bounds("SIDEBAR_NEW_SPACE").expect("new space row");
    assert!(new_space.top() >= bounds(cx, 1).bottom());
    cx.simulate_click(new_space.center(), Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert_eq!(view.actions, vec![Action::NewSpace]));

    view.update(cx, |view, _| view.actions.clear());
    let start = expanded.origin + point(px(60.), px(12.));
    let end = point(start.x, project_before.bottom() - px(5.));
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(start + point(px(0.), px(8.)), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.spaces.iter().map(|space| space.id).collect::<Vec<_>>(),
            vec![folder_id, free_id]
        );
        assert!(
            view.actions
                .iter()
                .all(|action| !matches!(action, Action::ToggleSpaceCollapsed { .. }))
        );
        assert!(!view.spaces[1].collapsed, "dropping a title must not collapse it");
    });
}

#[gpui::test]
fn sidebar_menus_move_close_and_create(cx: &mut TestAppContext) {
    use super::{Action, LayoutEntry, SpaceEntries};
    use crate::workspace::WorkspaceTabs;
    cx.update(|cx| {
        ::settings::init(cx);
        theme::init(theme::LoadThemes::JustBase, cx);
        crate::fonts::install(&crate::settings::ResolvedSettings::default(), cx);
    });
    let space = 11_u64.into();
    let mut tabs = WorkspaceTabs::new();
    let ids: Vec<_> = (0..3)
        .map(|_| {
            let item = tabs.alloc_item();
            tabs.push_standalone(item).unwrap()
        })
        .collect();
    let layouts = ids
        .iter()
        .map(|&tab| LayoutEntry { tab, name: None, selected: false, needs_you: false, entry: None })
        .collect();
    let (view, cx) = cx.add_window_view(|_, _| TreeHarness {
        spaces: vec![SpaceEntries {
            id: space,
            name: "Project".into(),
            collapsed: false,
            active: true,
            removable: true,
            available: true,
            layouts,
            activity: None,
        }],
        sorter: super::sidebar::SpaceSorter::new(super::sidebar::CARD_GAP).with_trailing(1),
        actions: vec![],
        width: px(280.),
        rename: None,
        rename_input: None,
    });
    cx.run_until_parked();
    let parent = cx.window_handle();

    // Right-click `at`, then click `label` in the menu, or report it missing.
    let pick = |cx: &mut gpui::VisualTestContext, at: gpui::Point<Pixels>, label: &str| {
        cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::none());
        cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::none());
        cx.run_until_parked();
        let popup = cx.windows().into_iter().find(|window| *window != parent).expect("menu");
        let mut popup = gpui::VisualTestContext::from_window(popup, cx);
        popup.run_until_parked();
        let found = popup.debug_bounds(format!("MENU_ITEM-{label}").leak());
        match found {
            Some(entry) => popup.simulate_click(entry.center(), Modifiers::none()),
            None => {
                // Only tab menus are probed for missing rows; prove this one drew.
                assert!(popup.debug_bounds("MENU_ITEM-Close Tab").is_some());
                popup.update(|window, _| window.remove_window());
            }
        }
        cx.run_until_parked();
        found.is_some()
    };
    let row = |cx: &mut gpui::VisualTestContext, index: usize| {
        cx.debug_bounds(format!("LAYOUT_ROW_{}", ids[index].get()).leak()).unwrap().center()
    };
    let take = |cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, _| std::mem::take(&mut view.actions))
    };

    let middle = row(cx, 1);
    assert!(pick(cx, middle, "Move Up"));
    assert_eq!(take(cx), vec![Action::MoveWorkspaceTab { space, tab: ids[1], target_index: 0 }]);
    assert!(pick(cx, middle, "Move Down"));
    assert_eq!(take(cx), vec![Action::MoveWorkspaceTab { space, tab: ids[1], target_index: 2 }]);

    // The ends hide the move that would go nowhere.
    let first = row(cx, 0);
    assert!(!pick(cx, first, "Move Up"));
    let last = row(cx, 2);
    assert!(!pick(cx, last, "Move Down"));
    assert!(take(cx).is_empty());

    assert!(pick(cx, middle, "Close Other Tabs"));
    assert_eq!(take(cx), vec![Action::CloseOtherTabs { space, tab: ids[1] }]);

    assert!(pick(cx, last, "Close Tab"));
    assert_eq!(take(cx), vec![Action::CloseGroup { space, tab: ids[2] }]);

    // The menu must not offer "other tabs" when this is the only tab.
    view.update(cx, |view, cx| {
        view.spaces[0].layouts.truncate(1);
        cx.notify();
    });
    cx.run_until_parked();
    let only = row(cx, 0);
    assert!(!pick(cx, only, "Close Other Tabs"));
    assert!(take(cx).is_empty());

    let heading = view
        .read_with(cx, |view, _| view.sorter.scroll_handle().bounds_for_item(0).unwrap())
        .origin
        + point(px(64.), px(12.));
    assert!(pick(cx, heading, "New Terminal"));
    assert_eq!(take(cx), vec![Action::NewInSpace { space }]);
}

/// Selection owns the blue tint and one persistent bar centred on the tree
/// guide; hover and open menus on other rows stay neutral with no bar.
#[gpui::test]
fn sidebar_selection_bar_sits_on_the_guide_and_hover_stays_neutral(cx: &mut TestAppContext) {
    use super::{Action, LayoutEntry, SpaceEntries};
    use crate::workspace::WorkspaceTabs;
    cx.update(|cx| {
        ::settings::init(cx);
        theme::init(theme::LoadThemes::JustBase, cx);
        crate::fonts::install(&crate::settings::ResolvedSettings::default(), cx);
    });
    for (rem, width) in [(px(16.), px(280.)), (px(20.), px(360.))] {
        let space = 11_u64.into();
        let mut tabs = WorkspaceTabs::new();
        let ids: Vec<_> = (0..2)
            .map(|_| {
                let item = tabs.alloc_item();
                tabs.push_standalone(item).unwrap()
            })
            .collect();
        let layouts = ids
            .iter()
            .enumerate()
            .map(|(index, &tab)| LayoutEntry {
                tab,
                name: None,
                selected: index == 1,
                needs_you: false,
                entry: None,
            })
            .collect();
        let (view, cx) = cx.add_window_view(|_, _| TreeHarness {
            spaces: vec![SpaceEntries {
                id: space,
                name: "Project".into(),
                collapsed: false,
                active: true,
                removable: true,
                available: true,
                layouts,
                activity: None,
            }],
            sorter: super::sidebar::SpaceSorter::new(super::sidebar::CARD_GAP).with_trailing(1),
            actions: vec![],
            width,
            rename: None,
            rename_input: None,
        });
        cx.update(|window, _| {
            window.set_rem_size(rem);
            window.refresh();
        });
        cx.run_until_parked();
        assert_eq!(cx.update(|window, _| window.rem_size()), rem);
        let parent = cx.window_handle();
        let (info, tint, neutral, guide) = cx.update(|_, cx| {
            let colors = cx.theme().colors();
            let info = cx.theme().status().info;
            (
                info,
                info.opacity(0.1),
                colors.text.opacity(0.05),
                colors.border_variant.opacity(0.75),
            )
        });
        let unselected = format!("LAYOUT_ROW_{}", ids[0].get()).leak();
        let selected = format!("LAYOUT_ROW_{}", ids[1].get()).leak();
        let unselected_marker = format!("LAYOUT_SELECTION_{}", ids[0].get()).leak();
        let selected_marker = format!("LAYOUT_SELECTION_{}", ids[1].get()).leak();
        let guide_selector = format!("SPACE_GUIDE_{space:?}").leak();
        // Painted quads with `color`, converted from scaled to logical pixels.
        let painted = |cx: &mut gpui::VisualTestContext, color: gpui::Hsla| {
            cx.update(|window, _| {
                let scale = window.scale_factor();
                window
                    .painted_quads()
                    .into_iter()
                    .filter(|quad| quad.background == color.into())
                    .map(|quad| {
                        let b = quad.bounds;
                        Bounds::new(
                            point(px(b.origin.x.as_f32() / scale), px(b.origin.y.as_f32() / scale)),
                            size(
                                px(b.size.width.as_f32() / scale),
                                px(b.size.height.as_f32() / scale),
                            ),
                        )
                    })
                    .collect::<Vec<_>>()
            })
        };
        // Exactly one bar, on the selected row, centred on the guide in both
        // layout and paint; the unselected row carries no bar.
        let check_selection = |cx: &mut gpui::VisualTestContext, state: &str| {
            let row = cx.debug_bounds(selected).expect("selected row");
            let tree = cx.debug_bounds(guide_selector).expect("tree guide");
            assert!(cx.debug_bounds(unselected_marker).is_none(), "{state}: unselected bar");
            let marker = cx.debug_bounds(selected_marker).expect("selected bar");
            assert_eq!(marker.center().x, tree.center().x, "{state}: bar centre on guide");
            assert_eq!(marker.size.width, px(2.));
            assert_eq!(marker.top(), row.top() + px(5.));
            assert_eq!(marker.bottom(), row.bottom() - px(5.));
            let tints = painted(cx, tint);
            assert_eq!(tints, vec![row], "{state}: one selected tint, on the selected row");
            let bars: Vec<_> = painted(cx, info)
                .into_iter()
                .filter(|bar| bar.top() >= tree.top() && bar.bottom() <= tree.bottom())
                .collect();
            assert_eq!(bars.len(), 1, "{state}: one painted bar in the tree");
            let guides: Vec<_> =
                painted(cx, guide).into_iter().filter(|line| line.size.width == px(1.)).collect();
            assert_eq!(guides.len(), 1, "{state}: one painted guide");
            // Quads are exact before rasterization; allow float rounding only.
            let offset = (bars[0].center().x - guides[0].center().x).abs();
            assert!(offset < px(0.01), "{state}: painted bar is {offset:?} off the guide");
            assert!(!painted(cx, neutral).contains(&row), "{state}: neutral fill on selection");
        };
        let away = point(width - px(10.), px(395.));

        cx.simulate_mouse_move(away, None, Modifiers::none());
        cx.run_until_parked();
        check_selection(cx, "resting");
        let other = cx.debug_bounds(unselected).expect("unselected row");
        assert!(!painted(cx, neutral).contains(&other), "resting row has no fill");

        cx.simulate_mouse_move(other.center(), None, Modifiers::none());
        cx.run_until_parked();
        check_selection(cx, "unselected hover");
        assert!(painted(cx, neutral).contains(&other), "hover is the neutral fill");

        let row = cx.debug_bounds(selected).expect("selected row");
        cx.simulate_mouse_move(row.center(), None, Modifiers::none());
        cx.run_until_parked();
        check_selection(cx, "selected hover");
        assert!(!painted(cx, neutral).contains(&other));

        // An open menu holds the neutral look on its row after the pointer
        // leaves, adds no bar, and does not activate the tab.
        view.update(cx, |view, _| view.actions.clear());
        cx.simulate_mouse_down(other.center(), MouseButton::Right, Modifiers::none());
        cx.simulate_mouse_up(other.center(), MouseButton::Right, Modifiers::none());
        cx.run_until_parked();
        let popup = cx.windows().into_iter().find(|window| *window != parent).expect("menu");
        let popup_handle = popup;
        cx.simulate_mouse_move(away, None, Modifiers::none());
        cx.run_until_parked();
        check_selection(cx, "unselected menu");
        assert!(painted(cx, neutral).contains(&other), "open menu holds the neutral fill");
        view.read_with(cx, |view, _| {
            assert!(
                !view.actions.iter().any(|action| matches!(action, Action::ActivateLayout { .. })),
                "opening a menu must not activate the tab"
            );
        });
        let mut popup = gpui::VisualTestContext::from_window(popup, cx);
        popup.update(|window, _| window.remove_window());
        cx.run_until_parked();
        assert!(!cx.windows().contains(&popup_handle), "menu closed");
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        check_selection(cx, "menu closed");
        assert!(!painted(cx, neutral).contains(&other));
        cx.update(|window, _| window.remove_window());
        cx.run_until_parked();
    }
}

/// Select Rename through the shipping popup and workspace focus listener.
/// Only OS window activation is supplied by the fixture; element-focus events
/// come from GPUI's real dispatch tree and frame processing.
#[gpui::test]
fn sidebar_menu_rename_keeps_editor_focus_after_dismissal(cx: &mut TestAppContext) {
    use super::{LayoutEntry, SpaceEntries};
    use crate::workspace::WorkspaceTabs;
    use gpui::Focusable;
    cx.update(|cx| {
        ::settings::init(cx);
        theme::init(theme::LoadThemes::JustBase, cx);
        crate::fonts::install(&crate::settings::ResolvedSettings::default(), cx);
        crate::text_input::init(cx);
    });
    for space_name in [false, true] {
        let space = 17_u64.into();
        let mut tabs = WorkspaceTabs::new();
        let item = tabs.alloc_item();
        let tab = tabs.push_standalone(item).unwrap();
        let input = cx.new(|cx| crate::text_input::TextInput::new("Type a name…", cx));
        let (view, cx) = cx.add_window_view(|window, cx| {
            let root = cx.focus_handle();
            let terminal = cx.focus_handle();
            crate::app::on_workspace_focus_in(
                &root,
                window,
                cx,
                |this: &mut TreeHarness, window, cx| {
                    let (_, _, terminal) = this.rename_input.as_ref().unwrap();
                    window.focus(terminal, cx);
                },
            );
            cx.on_focus_out(&input.focus_handle(cx), window, |this: &mut TreeHarness, _, _, cx| {
                if this.rename.is_some() {
                    this.rename = None;
                    cx.notify();
                }
            })
            .detach();
            TreeHarness {
                spaces: vec![SpaceEntries {
                    id: space,
                    name: "Project".into(),
                    collapsed: false,
                    active: true,
                    removable: true,
                    available: true,
                    activity: None,
                    layouts: vec![LayoutEntry {
                        tab,
                        name: Some("Initial tab".into()),
                        selected: true,
                        needs_you: false,
                        entry: None,
                    }],
                }],
                sorter: super::sidebar::SpaceSorter::new(super::sidebar::CARD_GAP),
                actions: vec![],
                width: px(280.),
                rename: None,
                rename_input: Some((input.clone(), root, terminal)),
            }
        });
        cx.run_until_parked();
        let parent = cx.window_handle();
        cx.update(|window, _| window.activate_window());
        cx.update(|window, cx| {
            let root = view.read(cx).rename_input.as_ref().unwrap().1.clone();
            root.focus(window, cx);
        });
        cx.run_until_parked();
        assert!(
            cx.update(|window, cx| {
                view.read(cx).rename_input.as_ref().unwrap().2.is_focused(window)
            }),
            "direct workspace focus still forwards to active content"
        );
        let row = cx.debug_bounds(format!("LAYOUT_ROW_{}", tab.get()).leak()).unwrap();
        let at = if space_name { point(row.center().x, row.top() - px(15.)) } else { row.center() };
        cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::none());
        cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::none());
        cx.run_until_parked();
        let popup = cx.windows().into_iter().find(|window| *window != parent).expect("native menu");
        let mut popup = gpui::VisualTestContext::from_window(popup, cx);
        // TestPlatform does not honor WindowOptions::focus or reactivate a
        // popup's parent on close. Supply only those native activation edges;
        // GPUI computes and dispatches all element-focus events itself.
        popup.update(|window, _| window.activate_window());
        popup.run_until_parked();
        let label =
            if space_name { "MENU_ITEM-Rename Space…" } else { "MENU_ITEM-Rename Tab…" };
        let entry = popup.debug_bounds(label).expect("rename menu entry");
        popup.simulate_click(entry.center(), Modifiers::none());
        cx.run_until_parked();
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        assert!(!cx.windows().contains(&popup.window_handle()), "menu dismissed");
        assert!(
            view.read_with(cx, |view, _| view.rename.is_some()),
            "menu handoff must not cancel rename"
        );
        assert!(cx.debug_bounds("INLINE_RENAME_FIELD").is_some(), "editor remains rendered");
        assert!(
            cx.update(|window, cx| input.focus_handle(cx).is_focused(window)),
            "input retains focus"
        );
        cx.simulate_input("Replacement");
        assert_eq!(
            input.read_with(cx, |input, _| input.text().to_owned()),
            "Replacement",
            "initial label is selected"
        );
        cx.simulate_keystrokes(if space_name { "escape" } else { "enter" });
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(view.rename.is_none());
            if space_name {
                assert_eq!(view.spaces[0].name, "Project", "Escape leaves the label alone");
            } else {
                assert_eq!(
                    view.spaces[0].layouts[0].name.as_deref(),
                    Some("Replacement"),
                    "Enter saves"
                );
            }
            assert!(
                !view.actions.iter().any(|action| matches!(action, super::Action::Select { .. })),
                "rename never activates the row"
            );
        });
        cx.update(|window, _| window.remove_window());
        cx.run_until_parked();
    }
}

/// Exercise the production field and its event routing without constructing a
/// WorkspaceWindow (which would restore persisted history/configuration).
#[gpui::test]
fn sidebar_inline_rename_edits_in_place_and_routes_save_cancel_without_activation(
    cx: &mut TestAppContext,
) {
    use super::{Action, LayoutEntry, SpaceEntries};
    use crate::workspace::WorkspaceTabs;
    use gpui::Focusable;
    cx.update(|cx| {
        ::settings::init(cx);
        theme::init(theme::LoadThemes::JustBase, cx);
        crate::fonts::install(&crate::settings::ResolvedSettings::default(), cx);
        crate::text_input::init(cx);
    });
    for space_name in [false, true] {
        let space = 17_u64.into();
        let mut tabs = WorkspaceTabs::new();
        let item = tabs.alloc_item();
        let tab = tabs.push_standalone(item).unwrap();
        let input = cx.new(|cx| crate::text_input::TextInput::new("Type a name…", cx));
        input.update(cx, |input, cx| {
            input.set_text(if space_name { "Project" } else { "Initial tab" }, true, cx)
        });
        let editor = super::sidebar::RenameRows {
            space: space_name.then_some(space),
            tab: (!space_name).then_some((space, tab)),
            input: input.clone(),
        };
        let (view, cx) = cx.add_window_view(|_, _| TreeHarness {
            spaces: vec![
                SpaceEntries {
                    id: space,
                    name: "Project".into(),
                    collapsed: false,
                    active: true,
                    removable: true,
                    available: true,
                    activity: None,
                    layouts: vec![LayoutEntry {
                        tab,
                        name: Some("Initial tab".into()),
                        selected: true,
                        needs_you: true,
                        entry: None,
                    }],
                },
                SpaceEntries {
                    id: 18_u64.into(),
                    name: "Other project".into(),
                    collapsed: false,
                    active: false,
                    removable: true,
                    available: true,
                    activity: None,
                    layouts: vec![],
                },
            ],
            // Two movable headings make the drag-isolation check meaningful.
            sorter: super::sidebar::SpaceSorter::new(super::sidebar::CARD_GAP),
            actions: vec![],
            width: px(280.),
            rename: Some(editor.clone()),
            rename_input: None,
        });
        cx.update(|window, cx| window.focus(&input.focus_handle(cx), cx));
        cx.run_until_parked();
        let row_id = format!("LAYOUT_ROW_{}", tab.get()).leak();
        let status_id = format!("LAYOUT_STATUS_{}", tab.get()).leak();
        let row = cx.debug_bounds(row_id).unwrap();
        let field = cx.debug_bounds("INLINE_RENAME_FIELD").expect("inline input");
        assert_eq!(field.size.height, crate::design::INLINE_RENAME_HEIGHT);
        if space_name {
            assert!(field.bottom() < row.top(), "space input is above its tab rows");
            assert!(cx.debug_bounds(status_id).is_some());
        } else {
            assert_eq!(field.center().y, row.center().y);
            assert_eq!(field.top(), row.top() + px(1.));
            assert!(field.left() >= row.left() && field.right() <= row.right());
            assert!(
                cx.debug_bounds(status_id).is_none(),
                "editing gives the status space to the name"
            );
        }
        cx.simulate_click(field.center(), Modifiers::none());
        cx.run_until_parked();
        assert!(
            view.read_with(cx, |view, _| view.actions.is_empty()),
            "input click neither activates nor collapses"
        );
        let windows = cx.windows().len();
        cx.simulate_mouse_down(field.center(), MouseButton::Right, Modifiers::none());
        cx.simulate_mouse_up(field.center(), MouseButton::Right, Modifiers::none());
        cx.run_until_parked();
        assert_eq!(cx.windows().len(), windows, "editing does not open the row menu");
        // Text selection must not start a layout/space drag.
        let start = point(field.left() + px(5.), field.center().y);
        let end = point(field.right() - px(5.), field.center().y);
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
        assert!(
            !cx.update(|window, cx| cx.stop_active_drag(window)),
            "text selection cannot drag its row"
        );
        cx.simulate_keystrokes(if cfg!(target_os = "macos") { "cmd-a" } else { "ctrl-a" });
        cx.simulate_input(" Renamed label ");
        assert_eq!(input.read_with(cx, |input, _| input.text().to_owned()), " Renamed label ");
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert!(cx.debug_bounds("INLINE_RENAME_FIELD").is_none());
        assert_eq!(cx.debug_bounds(row_id).unwrap(), row, "editing does not move the row");
        assert!(cx.debug_bounds(status_id).is_some(), "status returns");
        view.read_with(cx, |view, _| {
            assert!(matches!(view.actions.last(), Some(Action::CommitRename)));
            if space_name {
                assert_eq!(view.spaces[0].name, "Renamed label");
            } else {
                assert_eq!(view.spaces[0].layouts[0].name.as_deref(), Some("Renamed label"));
            }
        });
        // Escape cancels a later edit without overwriting the saved label.
        input.update(cx, |input, cx| input.set_text("Do not save", true, cx));
        view.update(cx, |view, cx| {
            view.rename = Some(editor.clone());
            view.actions.clear();
            cx.notify();
        });
        cx.update(|window, cx| window.focus(&input.focus_handle(cx), cx));
        cx.run_until_parked();
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert!(cx.debug_bounds("INLINE_RENAME_FIELD").is_none());
        assert!(
            view.read_with(cx, |view, _| matches!(view.actions.last(), Some(Action::CancelRename)))
        );
        // Outside mouse-down also cancels; it cannot silently save the draft.
        view.update(cx, |view, cx| {
            view.rename = Some(editor);
            view.actions.clear();
            cx.notify();
        });
        cx.update(|window, cx| window.focus(&input.focus_handle(cx), cx));
        cx.run_until_parked();
        cx.simulate_click(point(px(260.), px(390.)), Modifiers::none());
        cx.run_until_parked();
        assert!(cx.debug_bounds("INLINE_RENAME_FIELD").is_none());
        view.read_with(cx, |view, _| {
            assert!(matches!(view.actions.last(), Some(Action::CancelRename)));
            if space_name {
                assert_eq!(view.spaces[0].name, "Renamed label");
            } else {
                assert_eq!(view.spaces[0].layouts[0].name.as_deref(), Some("Renamed label"));
            }
        });
        cx.update(|window, _| window.remove_window());
        cx.run_until_parked();
    }
}
