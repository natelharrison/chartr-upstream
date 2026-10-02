//! A fixture-only visual harness for inspecting and publishing the production chrome.
//!
//! The lab never opens persistence or application configuration. It renders the
//! real GPUI sidebar, tab strip, icons, fonts, and `chartrx` theme around static
//! fixture content directly to a Metal texture, so screenshots do not require
//! macOS Screen Recording access. Each full-window capture then receives a macOS
//! window frame; see [`frame_window`].

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use anyhow::Context as _;
use chartr_herdr::control::SessionStatus;
use gpui::{
    AnyWindowHandle, AppContext as _, Context, Entity, EntityId, Focusable, FontWeight, Hsla,
    Modifiers, Pixels, Point, Render, VisualTestAppContext, Window, img, point, px, size,
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

/// The fixture window's logical size.
const LAB_WIDTH: f32 = 1200.;
const LAB_HEIGHT: f32 = 780.;
const SIDEBAR_WIDTH: f32 = 192.;
/// Where the space menu opens, as after a right-click on the "chartr" header.
const SPACE_MENU_ORIGIN: (f32, f32) = (72., 166.);

// The macOS 26 window frame, measured from a native 2× window capture with its
// shadow. Values are logical pixels. The margins are the transparent border a
// window capture leaves for the shadow; the side margin applies left and right.
const FRAME_SIDE: f32 = 56.;
const FRAME_TOP: f32 = 38.;
const FRAME_BOTTOM: f32 = 74.;
const WINDOW_RADIUS: f32 = 16.;
// The shadow is the window's rectangle under a Gaussian blur, moved down.
const SHADOW_ALPHA: f32 = 0.71;
const SHADOW_SIGMA: f32 = 20.;
const SHADOW_OFFSET_Y: f32 = 18.;
// A dark hairline outside the window and a light one inside it.
const OUTLINE_WIDTH: f32 = 0.5;
const OUTLINE_ALPHA: f32 = 0.9;
const EDGE_WIDTH: f32 = 1.;
const EDGE_ALPHA: f32 = 0.2;
const EDGE_TOP_ALPHA: f32 = 0.32;

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

/// Static text in the selected terminal's place; the lab runs no shell.
fn lab_terminal(cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    let settings = cx.global::<settings::SettingsStore>().resolved();
    let (text, status, path, quiet) = (
        colors.terminal_foreground,
        colors.terminal_ansi_green,
        colors.terminal_ansi_blue,
        colors.terminal_dim_foreground,
    );
    let line = |parts: &[(&'static str, Hsla)]| {
        h_flex().children(parts.iter().map(|&(part, color)| div().text_color(color).child(part)))
    };
    let prompt =
        |command: &'static str| line(&[("~/chartr", path), (" % ", quiet), (command, text)]);
    let cargo = |verb: &'static str, rest: &'static str| {
        h_flex()
            .child(div().font_weight(FontWeight::BOLD).text_color(status).child(verb))
            .child(div().text_color(text).child(rest))
    };
    v_flex()
        .flex_1()
        .min_h_0()
        .gap(px(2.))
        .p(px(12.))
        .bg(colors.terminal_background)
        .font_family(settings.terminal_font_family.clone())
        .text_size(px(settings.terminal_font_size))
        .child(prompt("cargo test -p chartr space"))
        .child(cargo(
            "    Finished",
            " `test` profile [unoptimized + debuginfo] target(s) in 0.42s",
        ))
        .child(cargo("     Running", " unittests src/main.rs"))
        .child(line(&[
            ("test result: ", text),
            ("ok", status),
            (". 63 passed; 0 failed; 0 ignored", text),
        ]))
        .child(prompt("").child(div().w(px(8.)).h(px(16.)).bg(colors.terminal_foreground)))
}

/// Stand-ins for the AppKit traffic lights, which an offscreen window does not
/// draw. macOS 26 draws 14 px circles on a 23 px pitch; these sit where
/// `title_bar::options` places the native buttons, centered one pixel above the
/// title bar's middle.
fn lab_traffic_lights() -> impl IntoElement {
    h_flex().absolute().left(px(8.75)).top(px(12.)).gap(px(9.)).children(
        [0xec6765, 0xf2ca44, 0x65c466].map(|color| {
            div().size(px(14.)).flex_none().rounded_full().bg(Hsla::from(gpui::rgb(color)))
        }),
    )
}

/// The title bar's settings button, as drawn by the app; inert here.
fn lab_settings_button() -> impl IntoElement {
    ui::ButtonLike::new("lab-settings")
        .width(crate::design::ICON_BUTTON)
        .height(crate::design::ICON_BUTTON.into())
        .size(ui::ButtonSize::None)
        .child(Icon::new(IconName::Settings).size(IconSize::Small).color(Color::Muted))
}

/// The title bar's amber count of agents waiting on the user; inert here.
fn lab_needs_you_chip(waiting: usize, cx: &App) -> impl IntoElement {
    let label = if waiting == 1 { "1 needs you".to_owned() } else { format!("{waiting} need you") };
    ui::ButtonLike::new("lab-needs-you")
        .height(crate::design::ICON_BUTTON.into())
        .size(ui::ButtonSize::None)
        .style(ui::ButtonStyle::Transparent)
        .child(crate::design::status_chip(IconName::BellRing, label, cx.theme().status().warning))
}

/// The strip's + and ▧, as drawn by the app's `new_item_button`.
fn lab_strip_create() -> AnyElement {
    let button = |id: &'static str, icon: Icon| {
        ui::ButtonLike::new(id)
            .width(crate::design::ICON_BUTTON)
            .height(crate::design::ICON_BUTTON.into())
            .size(ui::ButtonSize::None)
            .style(ui::ButtonStyle::Subtle)
            .child(icon.size(IconSize::Small).color(Color::Muted))
    };
    h_flex()
        .gap(px(2.))
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
    // Bare settings have no file, so this changes only the lab's theme.
    let mut settings = settings::SettingsStore::bare();
    settings.update(|content| {
        content.appearance.get_or_insert_with(Default::default).fixed_theme =
            Some(settings::CHARTRX_THEME.to_owned());
    })?;
    cx.update(move |cx| {
        ::settings::init(cx);
        crate::text_input::init(cx);
        theme::init(theme::LoadThemes::All(Box::new(zed_assets::Assets)), cx);
        settings::init_themes(settings.resolved(), cx);
        zed_assets::Assets.load_fonts(cx).expect("load Zed fonts for UI lab");
        fonts::load_bundled(cx).expect("load Chartr fonts for UI lab");
        fonts::install(settings.resolved(), cx);
        cx.set_global(settings);
        cx.bind_keys([
            gpui::KeyBinding::new("cmd-t", actions::workspace::NewTerminal, None),
            gpui::KeyBinding::new("cmd-w", actions::pane::CloseActiveItem, None),
        ]);
    });

    let ids = cx.update(|cx| {
        [cx.new(|_| ()).entity_id(), cx.new(|_| ()).entity_id(), cx.new(|_| ()).entity_id()]
    });
    let spaces = open_lab(&mut cx, move |_, _| UiLab::new(ids, LabMode::Spaces))?;
    save_framed(&mut cx, spaces, output)?;
    let tabs = open_lab(&mut cx, move |_, _| UiLab::new(ids, LabMode::Tabs))?;
    save_framed(&mut cx, tabs, &popup_output_path(output, "tabs"))?;

    capture_drag(&mut cx, spaces, output)?;
    capture_inline_rename(&mut cx, ids, output)?;
    capture_space_menu(&mut cx, ids, output)?;
    capture_menus(&mut cx, output)
}

/// Open the fixture in an offscreen window with room for the frame margin.
fn open_lab(
    cx: &mut VisualTestAppContext,
    build: impl FnOnce(&mut Window, &mut App) -> UiLab,
) -> anyhow::Result<AnyWindowHandle> {
    let framed = size(px(LAB_WIDTH + 2. * FRAME_SIDE), px(LAB_HEIGHT + FRAME_TOP + FRAME_BOTTOM));
    let window = cx.open_offscreen_window(framed, move |window, cx| {
        let lab = build(window, cx);
        cx.new(|_| lab)
    })?;
    Ok(window.into())
}

/// A fixture point in window coordinates, past the frame margin.
fn lab_point(x: f32, y: f32) -> Point<Pixels> {
    point(px(FRAME_SIDE + x), px(FRAME_TOP + y))
}

/// Settle a lab window, capture it, frame it as a macOS window, and save it.
fn save_framed(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    output: &Path,
) -> anyhow::Result<()> {
    cx.run_until_parked();
    cx.update_window(window, |_, window, _| window.refresh())?;
    cx.run_until_parked();
    let mut shot = cx.capture_screenshot(window)?;
    let (width, height) = shot.dimensions();
    frame_window(&mut shot, width, height);
    shot.save(output).with_context(|| format!("save {}", output.display()))
}

/// Give a full-window RGBA capture the macOS frame: clear the margin, round the
/// corners, add the outline and inner edge, and lay the drop shadow beneath.
fn frame_window(pixels: &mut [u8], width: u32, height: u32) {
    let scale = width as f32 / (LAB_WIDTH + 2. * FRAME_SIDE);
    let (left, top) = (FRAME_SIDE * scale, FRAME_TOP * scale);
    let (right, bottom) = (left + LAB_WIDTH * scale, top + LAB_HEIGHT * scale);
    let radius = WINDOW_RADIUS * scale;
    let (center_x, center_y) = ((left + right) / 2., (top + bottom) / 2.);
    let (inner_x, inner_y) = ((right - left) / 2. - radius, (bottom - top) / 2. - radius);
    // A blurred rectangle factors into one blurred span along each axis.
    let sigma = SHADOW_SIGMA * scale;
    let span = |position: u32, start: f32, end: f32| {
        let position = position as f32 + 0.5;
        gaussian_cdf((position - start) / sigma) - gaussian_cdf((position - end) / sigma)
    };
    let offset = SHADOW_OFFSET_Y * scale;
    let columns: Vec<f32> = (0..width).map(|x| span(x, left, right)).collect();
    let rows: Vec<f32> = (0..height).map(|y| span(y, top + offset, bottom + offset)).collect();

    for (index, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (column, row) = (index % width as usize, index / width as usize);
        let (x, y) = (column as f32 + 0.5, row as f32 + 0.5);
        // Signed distance to the rounded window edge; negative inside.
        let (qx, qy) = ((x - center_x).abs() - inner_x, (y - center_y).abs() - inner_y);
        let corner = qx.max(0.).hypot(qy.max(0.));
        let distance = corner + qx.max(qy).min(0.) - radius;
        let coverage = (0.5 - distance).clamp(0., 1.);
        let outline = (0.5 - distance + OUTLINE_WIDTH * scale).clamp(0., 1.) - coverage;
        let edge = coverage - (0.5 - distance - EDGE_WIDTH * scale).clamp(0., 1.);
        // How far the nearest edge faces up; the top edge catches more light.
        let up = if y >= center_y {
            0.
        } else if qx > 0. && qy > 0. {
            qy / corner
        } else if qy >= qx {
            1.
        } else {
            0.
        };
        let light = edge * (EDGE_ALPHA + (EDGE_TOP_ALPHA - EDGE_ALPHA) * up);
        let shadow = SHADOW_ALPHA * columns[column] * rows[row];
        composite(pixel, coverage, outline, light, shadow);
    }
}

/// Lay one RGBA window pixel over its frame. `coverage` and `outline` are the
/// shares of the pixel inside the window and inside the outline; `light` is the
/// lit share of the inner edge, already weighted by its strength. The shadow
/// shows through the rest, and the result keeps straight alpha.
fn composite(pixel: &mut [u8], coverage: f32, outline: f32, light: f32, shadow: f32) {
    let covered = coverage + OUTLINE_ALPHA * outline;
    let alpha = covered + shadow * (1. - covered);
    // The shadow and outline are black, so only the window adds color.
    for channel in &mut pixel[..3] {
        let color = f32::from(*channel);
        let premultiplied = color * coverage + (255. - color) * light;
        *channel = if alpha > 0. { (premultiplied / alpha).round() as u8 } else { 0 };
    }
    pixel[3] = (alpha * 255.).round() as u8;
}

/// The standard normal CDF, from the Abramowitz and Stegun 7.1.26 error
/// function (absolute error under 1.5e-7).
fn gaussian_cdf(z: f32) -> f32 {
    let x = f64::from(z.abs()) / std::f64::consts::SQRT_2;
    let t = 1. / (1. + 0.327_591_1 * x);
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736
                + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let erf = 1. - poly * (-x * x).exp();
    (0.5 * (1. + erf.copysign(f64::from(z)))) as f32
}

/// The real input rendered in each supported inline location; synthetic only.
fn capture_inline_rename(
    cx: &mut VisualTestAppContext,
    ids: [EntityId; 3],
    output: &Path,
) -> anyhow::Result<()> {
    for space_name in [false, true] {
        let window = open_lab(cx, move |window, cx| {
            let mut lab = UiLab::new(ids, LabMode::Spaces);
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
            lab
        })?;
        let target =
            popup_output_path(output, if space_name { "rename-space" } else { "rename-tab" });
        save_framed(cx, window, &target)?;
    }
    Ok(())
}

/// The chartr space's first tab dragged halfway onto "Review": the ghost row,
/// the dimmed source, and the drop line under the target.
fn capture_drag(
    cx: &mut VisualTestAppContext,
    window: AnyWindowHandle,
    output: &Path,
) -> anyhow::Result<()> {
    let (source, target) = (lab_point(120., 193.), lab_point(120., 213.));
    cx.simulate_mouse_move(window, source, None, Modifiers::none());
    cx.simulate_mouse_down(window, source, gpui::MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(
        window,
        source + point(px(0.), px(8.)),
        gpui::MouseButton::Left,
        Modifiers::none(),
    );
    cx.simulate_mouse_move(window, target, gpui::MouseButton::Left, Modifiers::none());
    save_framed(cx, window, &popup_output_path(output, "drag"))?;
    cx.update_window(window, |_, window, cx| cx.stop_active_drag(window))?;
    Ok(())
}

/// The space menu open over the sidebar, as after a right-click on the chartr
/// space, with its first action hovered.
fn capture_space_menu(
    cx: &mut VisualTestAppContext,
    ids: [EntityId; 3],
    output: &Path,
) -> anyhow::Result<()> {
    let window = open_lab(cx, move |window, cx| {
        let mut lab = UiLab::new(ids, LabMode::Spaces);
        let summary = chrome::sidebar::SpaceMenu::of(&lab.spaces[1]);
        lab.menu = Some(ContextMenu::build_lab_menu(window, cx, move |menu| {
            chrome::sidebar::space_menu(menu, &summary, Rc::new(|_, _, _| {}))
        }));
        lab
    })?;
    cx.run_until_parked();
    // The context line and list inset put the first action here.
    let (x, y) = SPACE_MENU_ORIGIN;
    cx.simulate_mouse_move(window, lab_point(x + 80., y + 45.), None, Modifiers::none());
    save_framed(cx, window, &popup_output_path(output, "space-menu"))
}

/// The real sidebar space and tab menus, built from the lab fixture, with
/// each first action hovered. The tab menu shows its running variant.
fn capture_menus(cx: &mut VisualTestAppContext, output: &Path) -> anyhow::Result<()> {
    let ids = cx.update(|cx| {
        [cx.new(|_| ()).entity_id(), cx.new(|_| ()).entity_id(), cx.new(|_| ()).entity_id()]
    });
    let space = UiLab::new(ids, LabMode::Spaces).spaces.swap_remove(1);
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

/// The presentation the fixture shows. Each matches the app: Spaces shows the
/// sidebar without the tab strip, and Tabs shows the strip without the sidebar.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LabMode {
    Spaces,
    Tabs,
}

struct UiLab {
    mode: LabMode,
    rename: Option<chrome::sidebar::RenameRows>,
    menu: Option<Entity<ui::ContextMenu>>,
    spaces: Vec<SpaceEntries>,
    sorter: chrome::sidebar::SpaceSorter,
}

impl UiLab {
    fn new(ids: [EntityId; 3], mode: LabMode) -> Self {
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
            mode,
            rename: None,
            menu: None,
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
                        // The pane bar shows this layout's blocked codex.
                        chrome::LayoutEntry {
                            tab: agent_tab,
                            name: None,
                            selected: true,
                            needs_you: true,
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
        let tabs = self.mode == LabMode::Tabs;
        // The app counts blocked agents; each waiting fixture layout has one.
        let waiting = self
            .spaces
            .iter()
            .flat_map(|space| &space.layouts)
            .filter(|layout| layout.needs_you)
            .count();

        let title_bar = div()
            .relative()
            .h(px(crate::title_bar::HEIGHT))
            .flex_none()
            .bg(colors.background)
            .child(lab_traffic_lights())
            .child(
                h_flex()
                    .absolute()
                    .left(px(crate::app::TITLE_CONTROLS_LEFT))
                    .top(px(crate::app::TITLE_CONTROLS_TOP))
                    .h(px(crate::title_bar::HEIGHT))
                    .child(
                        img(assets::TITLE_BRAND_PATH)
                            .w(px(crate::app::TITLE_BRAND_WIDTH))
                            .h(px(crate::app::TITLE_BRAND_HEIGHT))
                            .flex_none(),
                    ),
            )
            .child(
                h_flex()
                    .absolute()
                    .right(px(crate::app::TITLE_CONTROLS_RIGHT))
                    .top(px(crate::app::TITLE_CONTROLS_TOP))
                    .h(px(crate::title_bar::HEIGHT))
                    .gap_1()
                    .children((waiting > 0).then(|| lab_needs_you_chip(waiting, cx)))
                    .child(
                        crate::components::SegmentedControl::new(
                            "lab-presentation",
                            [("Tabs", tabs), ("Spaces", !tabs), ("Chats", false)].map(
                                |(label, selected)| {
                                    crate::components::SegmentedControlOption::new(
                                        label,
                                        label,
                                        selected,
                                        |_, _, _| {},
                                    )
                                },
                            ),
                        )
                        .soft_inset(),
                    )
                    .child(lab_settings_button()),
            );

        // Spaces shows the sidebar; Tabs shows the strip above the surface.
        let sidebar = if tabs {
            None
        } else {
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
            let sidebar = chrome::sidebar::render(
                &self.spaces,
                &agents,
                &surfaces,
                emit.clone(),
                &self.sorter,
                self.rename.as_ref(),
                window,
                cx,
            );
            Some(div().w(px(SIDEBAR_WIDTH)).h_full().flex_none().child(
                chrome::sidebar_pane::render(SIDEBAR_WIDTH, None, sidebar.into_any_element(), cx),
            ))
        };
        let strip = if tabs {
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
                emit,
                window,
                cx,
            );
            Some(
                div().relative().w_full().h(px(chrome::tabs::HEIGHT + 6.)).flex_none().child(
                    h_flex()
                        .absolute()
                        .top_0()
                        .left_0()
                        .w_full()
                        .h(px(chrome::tabs::HEIGHT))
                        .child(strip),
                ),
            )
        } else {
            None
        };

        // The inset work surface, styled as the app draws it.
        let surface = v_flex()
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .border_1()
            .border_color(colors.border.opacity(0.72))
            .rounded(crate::design::RADIUS_PANEL)
            .shadow(vec![gpui::BoxShadow {
                color: gpui::black().opacity(0.12),
                offset: point(px(0.), px(12.)),
                blur_radius: px(26.),
                spread_radius: px(0.),
                inset: false,
            }])
            .child(
                v_flex()
                    .size_full()
                    .bg(colors.editor_background)
                    .child(lab_pane_bar(cx))
                    .child(lab_terminal(cx)),
            );
        let surface_frame = v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .pl(px(if tabs { 6. } else { 0. }))
            .pr(px(6.))
            .pb(px(6.))
            .children(strip)
            .child(div().relative().flex_1().min_h_0().w_full().child(surface).children(
                crate::app::rounded_corner_masks(colors.background, colors.border.opacity(0.72)),
            ));

        let scene = div()
            .relative()
            .w(px(LAB_WIDTH))
            .h(px(LAB_HEIGHT))
            .flex()
            .flex_col()
            .key_context("chartr")
            .font(font)
            .text_size(UI_TEXT_DEFAULT)
            .bg(colors.background)
            .text_color(colors.text)
            .child(title_bar)
            .child(h_flex().flex_1().min_h_0().children(sidebar).child(surface_frame))
            .children(self.menu.clone().map(|menu| {
                let (x, y) = SPACE_MENU_ORIGIN;
                div().absolute().left(px(x)).top(px(y)).child(menu)
            }));
        // `frame_window` turns the margin into the transparent shadow area.
        div().size_full().pl(px(FRAME_SIDE)).pt(px(FRAME_TOP)).child(scene)
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

    #[test]
    fn gaussian_cdf_matches_known_values() {
        assert!((gaussian_cdf(0.) - 0.5).abs() < 1e-6);
        assert!((gaussian_cdf(1.959_964) - 0.975).abs() < 1e-5);
        assert!((gaussian_cdf(-1.) - 0.158_655).abs() < 1e-5);
    }

    #[test]
    fn window_frame_keeps_the_window_and_shades_the_margin() {
        // One device pixel per logical pixel.
        let width = (LAB_WIDTH + 2. * FRAME_SIDE) as u32;
        let height = (LAB_HEIGHT + FRAME_TOP + FRAME_BOTTOM) as u32;
        let mut pixels = [40, 44, 52, 255].repeat((width * height) as usize);
        frame_window(&mut pixels, width, height);
        let at = |x: f32, y: f32| {
            let index = (y as usize * width as usize + x as usize) * 4;
            <[u8; 4]>::try_from(&pixels[index..index + 4]).unwrap()
        };
        let (left, top) = (FRAME_SIDE, FRAME_TOP);
        let bottom = top + LAB_HEIGHT;

        // The window's interior is untouched.
        assert_eq!(at(left + 600., top + 400.), [40, 44, 52, 255]);
        // The image corner is clear, and the window's corner is cut round.
        assert_eq!(at(0., 0.)[3], 0);
        assert!(at(left + 1., top + 1.)[3] < 128);
        // The shadow falls mostly below the window.
        let (below, above) = (at(left + 600., bottom + 8.)[3], at(left + 600., top - 8.)[3]);
        assert!(above > 0 && below > 2 * above, "below {below}, above {above}");
        // The inner edge is lighter than the window, and lightest along the top.
        let (top_edge, side_edge) = (at(left + 600., top), at(left, top + 400.));
        assert!(top_edge[0] > side_edge[0] && side_edge[0] > 40, "{top_edge:?} {side_edge:?}");
    }

    #[test]
    fn composite_weights_each_band_by_its_share_of_the_pixel() {
        // Half window and half outline: the outline's half is not thinned by
        // the window's. Alpha 0.5 + 0.9 * 0.5 = 0.95; color 50 / 0.95.
        let mut pixel = [100, 100, 100, 255];
        composite(&mut pixel, 0.5, 0.5, 0., 0.);
        assert_eq!(pixel, [53, 53, 53, 242]);
        // A lit edge over the window's half adds only its own share:
        // (50 + 155 * 0.1) / 0.5.
        let mut pixel = [100, 100, 100, 255];
        composite(&mut pixel, 0.5, 0., 0.5 * EDGE_ALPHA, 0.);
        assert_eq!(pixel, [131, 131, 131, 128]);
        // Outside both, only the black shadow remains.
        let mut pixel = [100, 100, 100, 255];
        composite(&mut pixel, 0., 0., 0., 0.4);
        assert_eq!(pixel, [0, 0, 0, 102]);
    }
}
