//! Window layout and semantic action dispatch.

use super::*;

impl Render for WorkspaceWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui_font = Fonts::setup_ui(window, cx);
        self.restore_plugins_once(window, cx);
        self.sync_conversation_spaces(cx);
        if self.mode == Mode::Inbox {
            self.sync_inbox_terminal(cx);
        }
        let entries = self.entries(cx);
        let now = cx.background_executor().now();
        if self.space_sorter.tick(now, window.rem_size(), cx.reduce_motion()) {
            window.request_animation_frame();
        }
        let (chrome_visibility, mode_animating) =
            self.mode_transition.advance(self.mode, now, cx.reduce_motion());
        let (sidebar_width, sidebar_animating) = self.sidebar.advance(now, cx.reduce_motion());
        let (sidebar_position, sidebar_switching) = self.mode_transition.sidebar_position(now);
        if mode_animating || sidebar_animating || sidebar_switching {
            window.request_animation_frame();
        }
        // The pane keeps sliding, but each terminal reflows only once, after
        // both the chrome transition and any Inbox width expansion settle.
        let terminal_views: Vec<_> = self
            .spaces
            .iter()
            .flat_map(|space| {
                let space = space.read(cx);
                space.all_item_ids().into_iter().filter_map(move |id| {
                    space.item(id).and_then(crate::item::Item::as_session)?.terminal_view()
                })
            })
            .collect();
        for view in terminal_views {
            view.update(cx, |view, cx| {
                view.set_resize_paused(mode_animating || sidebar_animating, cx);
            });
        }
        let error_notices = self.error_notices(cx);
        let mut sidebar_spaces = self.sidebar_spaces(cx);
        self.space_sorter.arrange(&mut sidebar_spaces, |space| space.id);
        let sidebar_agents = self.agent_choices(cx);
        let sidebar_surfaces = self.surface_options();
        let chrome_entries: &[Entry] = &entries;
        let new_item = self.new_item_button(cx);
        let (background, text, workspace_background) = {
            let colors = cx.theme().colors();
            (colors.background, colors.text, colors.editor_background)
        };

        let on_action =
            cx.listener(|this, action: &Action, window, cx| this.act(action.clone(), window, cx));
        let emit: chrome::Emit = Rc::new(move |action, window, cx| on_action(&action, window, cx));
        // Tabs lists every space in its strip, so the title bar keeps the brand
        // instead of a space switcher.
        let title_controls = cfg!(target_os = "macos").then(|| {
            let (end_controls, show_brand) = self.title_bar_end_controls(
                emit.clone(),
                error_notices.clone(),
                window.viewport_size().width
                    - px(window_chrome::TITLE_CONTROLS_LEFT + window_chrome::TITLE_CONTROLS_RIGHT),
                window,
                cx,
            );
            (gpui::Empty.into_any_element(), end_controls, show_brand)
        });
        let (title_bar, title_bar_foreground) =
            self.workspace_title_bar(title_controls, chrome_visibility, window, cx);
        let (app_bar, app_bar_foreground) = cfg!(target_os = "linux")
            .then(|| {
                self.linux_app_bar(
                    emit.clone(),
                    error_notices.clone(),
                    chrome_visibility,
                    window,
                    cx,
                )
            })
            .unzip();

        let workspace = v_flex()
            .id("mode-workspace")
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .bg(workspace_background)
            .child(if self.mode == Mode::Inbox {
                self.conversations.clone().into_any_element()
            } else {
                self.workspace_pane(window, cx)
            })
            .when(self.mode == Mode::Inbox, |workspace| {
                workspace.children(self.terminal_search_overlay(cx))
            });

        let full_surface =
            matches!(self.settings.resolved().work_surface, crate::settings::WorkSurface::Full);
        // The strip sits on the canvas above the surface and opens downward
        // as the sidebar closes. Inset leaves a gap below it; Full is flush.
        let (strip_height, strip_gap) =
            if full_surface { (chrome::tabs::HEIGHT + 4., 0.) } else { (chrome::tabs::HEIGHT, 6.) };
        let tab_slot = div()
            .id("mode-tab-slot")
            .relative()
            .w_full()
            .h(px((strip_height + strip_gap) * chrome_visibility.tabs))
            .flex_none()
            .overflow_hidden()
            .when(chrome_visibility.tabs > 0., |slot| {
                let end_control =
                    (!cfg!(any(target_os = "macos", target_os = "linux"))).then(|| {
                        self.chrome_end_controls(
                            emit.clone(),
                            error_notices.clone(),
                            window.viewport_size().width * 0.4,
                            window,
                            cx,
                        )
                    });
                slot.child(
                    h_flex()
                        .absolute()
                        .top_0()
                        .left_0()
                        .w_full()
                        .h(px(strip_height))
                        .when(full_surface, |strip| strip.px(px(8.)))
                        .opacity(chrome_visibility.tabs)
                        .child(chrome::tabs::render(
                            &sidebar_spaces,
                            chrome_entries,
                            end_control,
                            new_item,
                            chrome_visibility.tabs,
                            emit.clone(),
                            window,
                            cx,
                        ))
                        .when(self.mode != Mode::Tabs, |strip| {
                            // The outgoing strip is visual only. Block clicks,
                            // drags, hover and scroll immediately.
                            strip.child(div().absolute().inset_0().occlude())
                        }),
                )
            });
        let sidebar_slot = div()
            .id("mode-sidebar-slot")
            .relative()
            .w(px(sidebar_width * chrome_visibility.sidebar))
            .h_full()
            .flex_none()
            // The settled sidebar's resize handle extends into the workspace.
            .when(chrome_visibility.sidebar < 1., |slot| slot.overflow_hidden())
            .when(chrome_visibility.sidebar > 0., |slot| {
                let controls = (!cfg!(any(target_os = "macos", target_os = "linux"))).then(|| {
                    (
                        gpui::Empty.into_any_element(),
                        self.chrome_end_controls(
                            emit.clone(),
                            error_notices.clone(),
                            px(sidebar_width) - window.rem_size(),
                            window,
                            cx,
                        ),
                    )
                });
                // Both pages keep their full width while sliding through the
                // same clipped viewport. Spaces is left of Chats; controls and
                // the resize handle stay fixed outside this moving content.
                let sidebar_page_stride = sidebar_width + 32.;
                let contents = div()
                    .relative()
                    .size_full()
                    .overflow_hidden()
                    .when(sidebar_position < 1., |pages| {
                        pages.child(
                            div()
                                .id("sidebar-spaces-page")
                                .absolute()
                                .top_0()
                                .left(px(-sidebar_page_stride * sidebar_position))
                                .w(px(sidebar_width))
                                .h_full()
                                .child(chrome::sidebar::render(
                                    &sidebar_spaces,
                                    &sidebar_agents,
                                    &sidebar_surfaces,
                                    emit.clone(),
                                    &self.space_sorter,
                                    self.inline_rename_rows().as_ref(),
                                    window,
                                    cx,
                                ))
                                .when(self.mode != Mode::Sidebar, |page| {
                                    page.child(div().absolute().inset_0().occlude())
                                }),
                        )
                    })
                    .when(sidebar_position > 0., |pages| {
                        pages.child(
                            div()
                                .id("sidebar-chats-page")
                                .absolute()
                                .top_0()
                                .left(px(sidebar_page_stride * (1. - sidebar_position)))
                                .w(px(sidebar_width))
                                .h_full()
                                .child(
                                    self.conversations
                                        .update(cx, |inbox, cx| inbox.render_sidebar(window, cx)),
                                )
                                .when(self.mode != Mode::Inbox, |page| {
                                    page.child(div().absolute().inset_0().occlude())
                                }),
                        )
                    })
                    .into_any_element();
                slot.child(
                    div()
                        .absolute()
                        .left(px(sidebar_width * (chrome_visibility.sidebar - 1.)))
                        .top_0()
                        .w(px(sidebar_width))
                        .h_full()
                        .child(chrome::sidebar_pane::render(sidebar_width, controls, contents, cx)),
                )
            });
        let inset =
            matches!(self.settings.resolved().work_surface, crate::settings::WorkSurface::Inset);
        let surface = v_flex()
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .border_color(cx.theme().colors().border.opacity(0.72));
        let surface = match self.settings.resolved().work_surface {
            crate::settings::WorkSurface::Inset => surface
                .border_1()
                .rounded(crate::design::RADIUS_PANEL)
                .shadow(vec![gpui::BoxShadow {
                    color: gpui::black().opacity(0.12),
                    offset: gpui::point(px(0.), px(12.)),
                    blur_radius: px(26.),
                    spread_radius: px(0.),
                    inset: false,
                }]),
            // Full rounds only the corner that meets the sidebar, never a window
            // edge; the corner and the sidebar divider leave with the sidebar.
            crate::settings::WorkSurface::Full => surface
                .border_t_1()
                .when(chrome_visibility.sidebar > 0., |surface| surface.border_l_1())
                .rounded_tl(rems(0.5 * chrome_visibility.sidebar)),
        };
        let surface_frame = v_flex().flex_1().min_w_0().h_full();
        let surface_frame = match self.settings.resolved().work_surface {
            // The left inset opens with Tabs, as the sidebar closes.
            crate::settings::WorkSurface::Inset => {
                surface_frame.pl(px(6. * chrome_visibility.tabs)).pr(px(6.)).pb(px(6.))
            }
            crate::settings::WorkSurface::Full => surface_frame,
        };
        let body = h_flex().w_full().flex_1().min_h_0().child(sidebar_slot).child(
            surface_frame.child(tab_slot).child(
                div().relative().flex_1().min_h_0().w_full().child(surface.child(workspace)).when(
                    inset,
                    |surface| {
                        let colors = cx.theme().colors();
                        surface.children(rounded_corner_masks(
                            colors.background,
                            colors.border.opacity(0.72),
                        ))
                    },
                ),
            ),
        );

        if self.mode_focus_pending {
            self.mode_focus_pending = false;
            self.focus_active_terminal(window, cx);
        }

        // Native child webviews sit above their parent window's GPUI scene. Rename dialogs use a
        // window-sized native popup when possible; these in-window overlays are the platform
        // fallback only.
        let rename = self.rename_overlay(cx);

        div()
            .relative()
            .track_focus(&self.focus)
            .key_context(if self.rename_space.is_some() {
                "RenameSpace"
            } else if self.rename_group.is_some() {
                "RenameGroup"
            } else {
                "chartr"
            })
            .size_full()
            .flex()
            .flex_col()
            .font(ui_font)
            .text_size(UI_TEXT_DEFAULT)
            .bg(background)
            .text_color(text)
            .on_drag_move::<chrome::DraggedSidebar>(cx.listener(
                |this, event: &DragMoveEvent<chrome::DraggedSidebar>, _, cx| {
                    this.sidebar.resize(event.event.position.x / px(1.), this.mode);
                    cx.notify();
                },
            ))
            .on_drag_move::<chrome::DraggedSpace>(cx.listener(
                |this, event: &DragMoveEvent<chrome::DraggedSpace>, window, cx| {
                    let dragged = event.drag(cx).0;
                    let order = this.spaces.iter().map(|space| space.entity_id()).collect();
                    if this.space_sorter.drag_move(
                        dragged,
                        order,
                        event.event.position,
                        window.rem_size(),
                        cx.background_executor().now(),
                        cx.reduce_motion(),
                    ) {
                        cx.notify();
                    }
                },
            ))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseUpEvent, window, cx| {
                    this.finish_space_drag(event.position.y, window, cx)
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseUpEvent, window, cx| {
                    this.finish_space_drag(event.position.y, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &actions::pane::CloseActiveItem, _, cx| {
                this.close_active_item(cx)
            }))
            .on_action(cx.listener(|this, _: &actions::pane::CloseAllItems, window, cx| {
                this.request_close_active_pane(window, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::pane::MoveLeft, _, cx| {
                this.move_active_to_pane(SplitDirection::Left, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::pane::MoveRight, _, cx| {
                this.move_active_to_pane(SplitDirection::Right, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::pane::MoveUp, _, cx| {
                this.move_active_to_pane(SplitDirection::Up, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::pane::MoveDown, _, cx| {
                this.move_active_to_pane(SplitDirection::Down, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::pane::JoinIntoNext, _, cx| {
                this.join_active_into_next(cx)
            }))
            .on_action(cx.listener(|this, _: &actions::workspace::ActivatePaneLeft, window, cx| {
                this.activate_pane_in_direction(SplitDirection::Left, window, cx)
            }))
            .on_action(cx.listener(
                |this, _: &actions::workspace::ActivatePaneRight, window, cx| {
                    this.activate_pane_in_direction(SplitDirection::Right, window, cx)
                },
            ))
            .on_action(cx.listener(|this, _: &actions::workspace::ActivatePaneUp, window, cx| {
                this.activate_pane_in_direction(SplitDirection::Up, window, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::workspace::ActivatePaneDown, window, cx| {
                this.activate_pane_in_direction(SplitDirection::Down, window, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::workspace::NewTerminal, window, cx| {
                this.act(Action::New, window, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::workspace::NewTerminalPane, window, cx| {
                this.new_adaptive_pane(true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::workspace::NewSurface, window, cx| {
                this.act(Action::NewPluginPane, window, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::workspace::NewSurfacePane, window, cx| {
                this.new_adaptive_pane(false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::workspace::Ungroup, window, cx| {
                this.ungroup_current(window, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::workspace::ToggleStatusBar, _, cx| {
                this.toggle_status_bar(cx);
            }))
            .on_action(cx.listener(|this, _: &actions::workspace::SidebarMode, _, cx| {
                this.settings_set_mode(Mode::Sidebar, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::workspace::TabbedMode, _, cx| {
                this.settings_set_mode(Mode::Tabs, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::workspace::ConversationMode, _, cx| {
                this.settings_set_mode(Mode::Inbox, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::workspace::CycleViewMode, _, cx| {
                this.settings_set_mode(
                    match this.mode {
                        Mode::Tabs => Mode::Sidebar,
                        Mode::Sidebar => Mode::Inbox,
                        Mode::Inbox => Mode::Tabs,
                    },
                    cx,
                )
            }))
            .on_action(cx.listener(|this, _: &actions::workspace::NewSpace, window, cx| {
                this.act(Action::NewSpace, window, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::workspace::CloseSpace, window, cx| {
                if let Some(space) = this.active.as_ref() {
                    this.request_close_space(space.entity_id(), window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &actions::workspace::ZoomIn, _, cx| {
                this.adjust_zoom(false, 1., cx)
            }))
            .on_action(cx.listener(|this, _: &actions::workspace::ZoomOut, _, cx| {
                this.adjust_zoom(false, -1., cx)
            }))
            .on_action(cx.listener(|this, _: &actions::workspace::TerminalZoomIn, _, cx| {
                this.adjust_zoom(true, 1., cx)
            }))
            .on_action(cx.listener(|this, _: &actions::workspace::TerminalZoomOut, _, cx| {
                this.adjust_zoom(true, -1., cx)
            }))
            .on_action(cx.listener(|this, _: &actions::workspace::NewFreeTerminal, window, cx| {
                this.new_free_item(true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::workspace::NewFreeSurface, window, cx| {
                this.new_free_item(false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::settings::Open, window, cx| {
                this.open_settings(window, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::command_palette::Toggle, window, cx| {
                this.toggle_command_palette(window, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::terminal_search::Toggle, window, cx| {
                this.toggle_terminal_search(window, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::terminal_search::Next, _, cx| {
                this.navigate_terminal_search(true, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::terminal_search::Previous, _, cx| {
                this.navigate_terminal_search(false, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::terminal_search::Close, window, cx| {
                this.close_terminal_search(window, cx)
            }))
            .on_key_down(cx.listener(|this, event, window, cx| this.on_key(event, window, cx)))
            .child(title_bar)
            .children(app_bar)
            .when(self.state_restore_problem.is_some(), |view| {
                view.child(
                    div().w_full().flex_none().child(
                        Banner::new().severity(Severity::Warning).child(
                            Label::new(
                                "Workspace couldn’t be restored. Layout changes won’t be saved this time.",
                            )
                            .size(UI_LABEL_DEFAULT),
                        ),
                    ),
                )
            })
            .child(body)
            .when(self.settings.resolved().show_status_bar, |view| view.child(self.status_bar(cx)))
            .child(title_bar_foreground)
            .children(app_bar_foreground)
            .children(rename)
    }
}

impl WorkspaceWindow {
    /// Surfaces a create menu can open directly; session-bound plugins need
    /// a terminal first, so they stay behind "More surfaces…".
    pub(super) fn surface_options(&self) -> Vec<chrome::SurfaceOption> {
        self.catalog
            .panes()
            .into_iter()
            .filter_map(|pane| {
                let plugin = self.catalog.get(&pane.key.plugin)?;
                if plugin.capabilities().session_binding {
                    return None;
                }
                Some(chrome::SurfaceOption {
                    title: pane.title.clone(),
                    key: pane.key.clone(),
                    icon_path: plugin.icon_path().to_string_lossy().into_owned().into(),
                    external_icon: true,
                })
            })
            .collect()
    }
}

/// GPUI clips children to rectangles, so pane bars and terminals would poke
/// square corners past the inset surface's rounded edge. Paint the canvas in a
/// quarter ring over each corner, then redraw the border above the content.
pub(crate) fn rounded_corner_masks(canvas: gpui::Hsla, border: gpui::Hsla) -> Vec<AnyElement> {
    const RADIUS: f32 = 10.; // design::RADIUS_PANEL
    const RING: f32 = 6.;
    let corner = |top: bool, left: bool| {
        let ring = div()
            .absolute()
            .size(px((RADIUS + RING) * 2.))
            .rounded(px(RADIUS + RING))
            .border_6()
            .border_color(canvas);
        let ring = if top { ring.top_0() } else { ring.bottom_0() };
        let ring = if left { ring.left_0() } else { ring.right_0() };
        let clip = div().absolute().size(px(RADIUS + RING)).overflow_hidden();
        let clip = if top { clip.top(px(-RING)) } else { clip.bottom(px(-RING)) };
        let clip = if left { clip.left(px(-RING)) } else { clip.right(px(-RING)) };
        clip.child(ring).into_any_element()
    };
    vec![
        corner(true, true),
        corner(true, false),
        corner(false, true),
        corner(false, false),
        div()
            .absolute()
            .inset_0()
            .rounded(px(RADIUS))
            .border_1()
            .border_color(border)
            .into_any_element(),
    ]
}
