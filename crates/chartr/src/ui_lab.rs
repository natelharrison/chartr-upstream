//! A fixture-only visual harness for inspecting the production sidebar chrome.
//!
//! The lab never opens persistence or application configuration. It renders the
//! real GPUI sidebar, icons, fonts, and theme with a placeholder workspace shell
//! directly to a Metal texture so screenshots do not require macOS Screen Recording access.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use anyhow::Context as _;
use chartr_herdr::control::SessionStatus;
use gpui::{
    AppContext as _, Context, Entity, EntityId, Focusable, Modifiers, Render, VisualTestAppContext,
    Window, img, point, px, size,
};
use ui::prelude::*;

use crate::{
    actions, assets,
    chrome::{self, SpaceEntries},
    components::ContextMenu,
    fonts::{self, Fonts, UI_TEXT_DEFAULT},
    settings,
    workspace::WorkspaceTabs,
};

const DEFAULT_OUTPUT: &str = "/private/tmp/chartr-ui-lab.png";

/// The real pane bar pieces with fixture tabs; the tools are inert here.
fn lab_pane_bar(cx: &App) -> impl IntoElement {
    let mut items = WorkspaceTabs::new();
    // A working and a waiting agent show their status glyphs.
    let tabs = [
        ("zsh", true, None, None),
        ("claude", false, Some(SessionStatus::Working), Some("icons/agent_claude.svg")),
        ("codex", false, Some(SessionStatus::Blocked), Some("icons/agent_chat_gpt.svg")),
    ]
    .map(|(title, selected, status, icon)| {
        let key = items.alloc_item();
        div().w(px(160.)).child(
            chrome::ItemTab::new(("lab-pane-tab", key.get()), title, selected, "lab", key)
                .activity(chrome::Activity { status, ..Default::default() })
                .icon_path(icon.map(Into::into))
                .build(cx),
        )
    });
    ui::TabBar::new("lab-pane-bar").children(tabs).child(chrome::new_item_cell(
        h_flex().gap(px(2.)).child(chrome::new_item_button("lab-new-item")).child(
            ui::ButtonLike::new("lab-create").width(px(24.)).height(px(24.).into()).child(
                Icon::from_path(assets::PLUGIN_LAUNCHER_ICON_PATH)
                    .size(IconSize::Small)
                    .color(Color::Muted),
            ),
        ),
        cx,
    ))
}

/// The strip's + and ▧, as drawn by the app's `new_item_button`.
fn lab_strip_create() -> AnyElement {
    let button = |id: &'static str, icon: Icon| {
        ui::ButtonLike::new(id)
            .width(px(22.))
            .height(px(22.).into())
            .child(icon.size(IconSize::Medium).color(Color::Muted))
    };
    h_flex()
        .gap(px(4.))
        .child(button("lab-strip-new", Icon::new(IconName::Plus)))
        .child(button("lab-strip-create", Icon::from_path(assets::PLUGIN_LAUNCHER_ICON_PATH)))
        .into_any_element()
}

pub fn capture_path(args: impl IntoIterator<Item = OsString>) -> Option<PathBuf> {
    let mut args = args.into_iter();
    args.next();
    while let Some(argument) = args.next() {
        if argument == "--ui-lab-capture" {
            return Some(args.next().map(PathBuf::from).unwrap_or_else(|| DEFAULT_OUTPUT.into()));
        }
    }
    None
}

pub fn capture(output: &Path) -> anyhow::Result<()> {
    let mut cx = VisualTestAppContext::with_asset_source(
        gpui_platform::current_platform(false),
        Arc::new(assets::Assets),
    );
    cx.update(|cx| {
        ::settings::init(cx);
        crate::text_input::init(cx);
        theme::init(theme::LoadThemes::All(Box::new(zed_assets::Assets)), cx);
        let settings = settings::SettingsStore::bare();
        settings::init_themes(settings.resolved(), cx);
        zed_assets::Assets.load_fonts(cx).expect("load Zed fonts for UI lab");
        fonts::load_bundled(cx).expect("load Chartr fonts for UI lab");
        fonts::install(settings.resolved(), cx);
        cx.set_global(settings);
    });

    let ids = cx.update(|cx| {
        [cx.new(|_| ()).entity_id(), cx.new(|_| ()).entity_id(), cx.new(|_| ()).entity_id()]
    });
    let window = cx.open_offscreen_window(size(px(1200.), px(780.)), move |_, cx| {
        cx.new(|_| UiLab::new(ids))
    })?;
    cx.run_until_parked();
    cx.update_window(window.into(), |_, window, _| window.refresh())?;
    cx.run_until_parked();
    cx.capture_screenshot(window.into())?
        .save(output)
        .with_context(|| format!("save {}", output.display()))?;

    capture_drag(&mut cx, window.into(), output)?;
    capture_inline_rename(&mut cx, ids, output)?;
    capture_menus(&mut cx, output)
}

/// The real input rendered in each supported inline location; synthetic only.
fn capture_inline_rename(
    cx: &mut VisualTestAppContext,
    ids: [EntityId; 3],
    output: &Path,
) -> anyhow::Result<()> {
    for space_name in [false, true] {
        let window = cx.open_offscreen_window(size(px(1200.), px(780.)), move |window, cx| {
            let mut lab = UiLab::new(ids);
            let space = lab.spaces[1].id;
            let tab = lab.spaces[1].layouts[0].tab;
            let input = cx.new(|cx| crate::text_input::TextInput::new("Type a name…", cx));
            input.update(cx, |input, cx| {
                input.set_text(if space_name { "chartr" } else { "Main layout" }, true, cx)
            });
            window.focus(&input.focus_handle(cx), cx);
            lab.rename = Some(chrome::sidebar::RenameRows {
                space: space_name.then_some(space),
                tab: (!space_name).then_some((space, tab)),
                input,
            });
            cx.new(|_| lab)
        })?;
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, _| window.refresh())?;
        cx.run_until_parked();
        let target =
            popup_output_path(output, if space_name { "rename-space" } else { "rename-tab" });
        cx.capture_screenshot(window.into())?.save(&target)?;
    }
    Ok(())
}

/// The chartr space's first tab dragged halfway onto "Review": the ghost row,
/// the dimmed source, and the drop line under the target.
fn capture_drag(
    cx: &mut VisualTestAppContext,
    window: gpui::AnyWindowHandle,
    output: &Path,
) -> anyhow::Result<()> {
    let (source, target) = (point(px(120.), px(193.)), point(px(120.), px(213.)));
    cx.simulate_mouse_move(window, source, None, Modifiers::none());
    cx.simulate_mouse_down(window, source, gpui::MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        window,
        source + point(px(0.), px(8.)),
        gpui::MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(window, target, gpui::MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.update_window(window, |_, window, _| window.refresh())?;
    cx.run_until_parked();
    let drag_output = popup_output_path(output, "drag");
    cx.capture_screenshot(window)?
        .save(&drag_output)
        .with_context(|| format!("save {}", drag_output.display()))?;
    cx.update_window(window, |_, window, cx| cx.stop_active_drag(window))?;
    Ok(())
}

/// The real sidebar space and tab menus, built from the lab fixture, with
/// each first action hovered. The tab menu shows its running variant.
fn capture_menus(cx: &mut VisualTestAppContext, output: &Path) -> anyhow::Result<()> {
    cx.update(|cx| {
        cx.bind_keys([
            gpui::KeyBinding::new("cmd-t", actions::workspace::NewTerminal, None),
            gpui::KeyBinding::new("cmd-w", actions::pane::CloseActiveItem, None),
        ]);
    });
    let ids = cx.update(|cx| {
        [cx.new(|_| ()).entity_id(), cx.new(|_| ()).entity_id(), cx.new(|_| ()).entity_id()]
    });
    let space = UiLab::new(ids).spaces.swap_remove(1);
    let summary = chrome::sidebar::SpaceMenu::of(&space);
    capture_menu(cx, &popup_output_path(output, "menu"), move |menu| {
        chrome::sidebar::space_menu(menu, &summary, Rc::new(|_, _, _| {}))
    })?;
    let space_id = space.id;
    capture_menu(cx, &popup_output_path(output, "create-menu"), move |menu| {
        let agents = chrome::AgentChoices {
            names: vec!["Pi".to_owned(), "Claude".to_owned()],
            last_used: Some("Pi".to_owned()),
        };
        chrome::create_menu(menu, space_id, &agents, &[], Rc::new(|_, _, _| {}))
    })?;
    let (count, tab) = (space.layouts.len(), space.layouts[1].tab);
    capture_menu(cx, &popup_output_path(output, "tab-menu"), move |menu| {
        chrome::sidebar::tab_menu(
            menu,
            space.id,
            tab,
            1,
            count,
            "Review",
            true,
            false,
            Rc::new(|_, _, _| {}),
        )
    })
}

fn capture_menu(
    cx: &mut VisualTestAppContext,
    output: &Path,
    build: impl FnOnce(ContextMenu) -> ContextMenu + 'static,
) -> anyhow::Result<()> {
    let window = cx.open_offscreen_window(size(px(340.), px(330.)), move |window, cx| {
        let menu =
            ContextMenu::build_lab_menu(window, cx, |menu| build(menu.popup_width(px(260.))));
        cx.new(|_| MenuPreview { menu })
    })?;
    cx.run_until_parked();
    // Padding, hairline, list inset and context line put the first action here.
    let first_row = point(px(MENU_PREVIEW_PADDING + 80.), px(MENU_PREVIEW_PADDING + 45.));
    cx.simulate_mouse_move(window.into(), first_row, None, Modifiers::none());
    cx.run_until_parked();
    cx.update_window(window.into(), |_, window, _| window.refresh())?;
    cx.run_until_parked();
    cx.capture_screenshot(window.into())?
        .save(output)
        .with_context(|| format!("save {}", output.display()))?;
    Ok(())
}

const MENU_PREVIEW_PADDING: f32 = 32.;

struct MenuPreview {
    menu: Entity<ui::ContextMenu>,
}

impl Render for MenuPreview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let font = Fonts::setup_ui(window, cx);
        div()
            .size_full()
            .key_context("chartr")
            .font(font)
            .text_size(UI_TEXT_DEFAULT)
            .bg(cx.theme().colors().background)
            .p(px(MENU_PREVIEW_PADDING))
            .child(self.menu.clone())
    }
}

fn popup_output_path(output: &Path, suffix: &str) -> PathBuf {
    let stem = output.file_stem().and_then(|stem| stem.to_str()).unwrap_or("chartr-ui-lab");
    let extension = output.extension().and_then(|extension| extension.to_str()).unwrap_or("png");
    output.with_file_name(format!("{stem}-{suffix}.{extension}"))
}

struct UiLab {
    rename: Option<chrome::sidebar::RenameRows>,
    spaces: Vec<SpaceEntries>,
    sorter: chrome::sidebar::SpaceSorter,
}

impl UiLab {
    fn new(ids: [EntityId; 3]) -> Self {
        let mut tabs = WorkspaceTabs::new();
        let terminal = tabs.alloc_item();
        let terminal_tab = tabs.push_standalone(terminal).expect("fixture terminal tab");
        let agent = tabs.alloc_item();
        let agent_tab = tabs.push_standalone(agent).expect("fixture agent tab");
        let group = tabs.alloc_item();
        let group_tab = tabs.push_standalone(group).expect("fixture group tab");
        let waiting = tabs.alloc_item();
        let waiting_tab = tabs.push_standalone(waiting).expect("fixture waiting tab");

        Self {
            rename: None,
            spaces: vec![
                SpaceEntries {
                    id: ids[0],
                    name: "Scratch".into(),
                    collapsed: false,
                    active: false,
                    removable: false,
                    available: true,
                    layouts: vec![chrome::LayoutEntry {
                        tab: terminal_tab,
                        name: None,
                        selected: true,
                        needs_you: false,
                        entry: None,
                    }],
                    activity: None,
                },
                SpaceEntries {
                    id: ids[1],
                    name: "chartr".into(),
                    collapsed: false,
                    active: true,
                    removable: true,
                    available: true,
                    layouts: vec![
                        chrome::LayoutEntry {
                            tab: agent_tab,
                            name: None,
                            selected: true,
                            needs_you: false,
                            entry: None,
                        },
                        chrome::LayoutEntry {
                            tab: group_tab,
                            name: Some("Review".into()),
                            selected: false,
                            needs_you: true,
                            entry: None,
                        },
                    ],
                    activity: Some(chrome::SpaceActivity::Inactive),
                },
                SpaceEntries {
                    id: ids[2],
                    name: "example-project".into(),
                    collapsed: true,
                    active: false,
                    removable: true,
                    available: true,
                    layouts: vec![chrome::LayoutEntry {
                        tab: waiting_tab,
                        name: None,
                        selected: true,
                        needs_you: true,
                        entry: None,
                    }],
                    activity: None,
                },
            ],
            sorter: chrome::sidebar::SpaceSorter::new(chrome::sidebar::CARD_GAP).with_trailing(1),
        }
    }
}

impl Render for UiLab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let font = Fonts::setup_ui(window, cx);
        let colors = cx.theme().colors().clone();
        let emit: chrome::Emit = Rc::new(|_, _, _| {});
        let agents = chrome::AgentChoices {
            names: vec!["Pi".to_owned(), "Codex".to_owned()],
            last_used: Some("Pi".to_owned()),
        };
        let surfaces = vec![
            chrome::SurfaceOption {
                title: "Wayfinder".into(),
                key: chartr_plugin::PaneKey::new("com.chartr.wayfinder", "main"),
                icon_path: assets::PLUGIN_LAUNCHER_ICON_PATH.into(),
                external_icon: false,
            },
            chrome::SurfaceOption {
                title: "Notes".into(),
                key: chartr_plugin::PaneKey::new("com.chartr.notes", "main"),
                icon_path: assets::PLUGIN_LAUNCHER_ICON_PATH.into(),
                external_icon: false,
            },
        ];
        // Tabs-mode strip for the active fixture space, drawn above the surface.
        let entries: Vec<chrome::Entry> = self.spaces[1]
            .layouts
            .iter()
            .enumerate()
            .map(|(index, layout)| chrome::Entry {
                space: self.spaces[1].id,
                space_key: "lab".into(),
                key: serde_json::from_value(serde_json::json!(layout.tab.get())).unwrap(),
                tab: layout.tab,
                pane: serde_json::from_value(serde_json::json!(1)).unwrap(),
                index,
                title: if index == 0 { "Pi".into() } else { "2 tabs".into() },
                icon_path: None,
                status: None,
                process_running: false,
                ended: false,
                bell: false,
                selected: layout.selected,
                closable: true,
                grouped: index == 1,
            })
            .collect();
        let strip = chrome::tabs::render(
            &self.spaces,
            &entries,
            None,
            lab_strip_create(),
            1.,
            emit.clone(),
            window,
            cx,
        )
        .into_any_element();
        let sidebar = chrome::sidebar::render(
            &self.spaces,
            &agents,
            &surfaces,
            emit,
            &self.sorter,
            self.rename.as_ref(),
            window,
            cx,
        );

        div()
            .size_full()
            .flex()
            .flex_col()
            .font(font)
            .text_size(UI_TEXT_DEFAULT)
            .bg(colors.background)
            .text_color(colors.text)
            .child(
                h_flex()
                    .relative()
                    .h(px(crate::title_bar::HEIGHT))
                    .flex_none()
                    .bg(colors.background)
                    .justify_between()
                    .child(
                        h_flex()
                            .absolute()
                            .left(px(16.))
                            .top(px(crate::app::TITLE_CONTROLS_TOP))
                            .h(px(crate::title_bar::HEIGHT))
                            .child(
                                img(assets::TITLE_BRAND_PATH)
                                    .w(px(crate::app::TITLE_BRAND_WIDTH))
                                    .h(px(crate::app::TITLE_BRAND_HEIGHT)),
                            ),
                    )
                    .child(
                        h_flex()
                            .absolute()
                            .right(px(12.))
                            .top(px(crate::app::TITLE_CONTROLS_TOP))
                            .h(px(crate::title_bar::HEIGHT))
                            .child(
                                crate::components::SegmentedControl::new(
                                    "lab-presentation",
                                    ["Tabs", "Spaces", "Chats"].map(|label| {
                                        crate::components::SegmentedControlOption::new(
                                            label,
                                            label,
                                            label == "Spaces",
                                            |_, _, _| {},
                                        )
                                    }),
                                )
                                .soft_inset(),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(div().w(px(192.)).h_full().flex_none().child(
                        chrome::sidebar_pane::render(192., None, sidebar.into_any_element(), cx),
                    ))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .pl(px(6.))
                            .pr(px(8.))
                            .pb(px(8.))
                            .child(div().h(px(36.)).flex_none().child(strip))
                            .child(
                                div()
                                    .relative()
                                    .flex_1()
                                    .min_h_0()
                                    .w_full()
                                    .child(
                                        v_flex()
                                            .size_full()
                                            .rounded(px(12.))
                                            .border_1()
                                            .border_color(colors.border.opacity(0.72))
                                            .overflow_hidden()
                                            .child(lab_pane_bar(cx))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .p_4()
                                                    .bg(colors.editor_background)
                                                    .child(
                                                        Label::new(
                                                            "Sidebar fixture — workspace content is a placeholder",
                                                        )
                                                        .color(Color::Muted),
                                                    ),
                                            ),
                                    )
                                    .children(crate::app::rounded_corner_masks(
                                        colors.background,
                                        colors.border.opacity(0.72),
                                    )),
                            ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_flag_defaults_and_accepts_an_output_path() {
        assert_eq!(
            capture_path([OsString::from("chartr"), OsString::from("--ui-lab-capture")]),
            Some(DEFAULT_OUTPUT.into())
        );
        assert_eq!(
            capture_path([
                OsString::from("chartr"),
                OsString::from("--ui-lab-capture"),
                OsString::from("/tmp/custom.png"),
            ]),
            Some("/tmp/custom.png".into())
        );
        assert_eq!(
            popup_output_path(Path::new("/tmp/chartr.png"), "menu"),
            PathBuf::from("/tmp/chartr-menu.png")
        );
    }
}
