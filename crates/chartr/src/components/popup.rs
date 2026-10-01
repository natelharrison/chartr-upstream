//! Context menus hosted in anchored native windows.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use gpui::{
    Action, Anchor, AnyElement, AnyView, AnyWindowHandle, App, AppContext as _, Bounds, Context,
    DismissEvent, ElementId, Entity, Focusable, IntoElement, MouseButton, ParentElement, Pixels,
    Render, RenderOnce, SharedString, Subscription, Window, WindowBackgroundAppearance,
    WindowBounds, WindowKind, WindowOptions, canvas, div, point, px, size,
};
use ui::{
    ContextMenu as UiContextMenu, ContextMenuEntry as UiContextMenuEntry, DynamicSpacing,
    IconPosition, MENU_ROW_HEIGHT, prelude::*,
};

#[cfg(test)]
use super::open_native_modal;
// Matches ui::ContextMenu's default minimum width.
const POPUP_CONTENT_WIDTH: Pixels = px(200.);
// Transparent room around the menu so its shadow is not clipped by the window.
const POPUP_OUTSET: Pixels = px(20.);
const POPUP_GAP: Pixels = px(4.);
// How far a right-click menu moves up and left so the pointer tip touches its
// rounded corner: 10px radius × (1 − 1/√2) ≈ 3px.
const POINTER_INSET: Pixels = px(3.);
const MENU_GAP_HEIGHT: Pixels = px(4.);
const MENU_SEPARATOR_HEIGHT: Pixels = px(7.);
const MENU_HEADER_HEIGHT: Pixels = px(20.);
const MENU_CONTEXT_LINE_HEIGHT: Pixels = px(26.);
// The panel's 1px hairline, top and bottom.
const MENU_BORDER_HEIGHT: Pixels = px(2.);

#[derive(Clone, Copy, Default)]
struct PopupMetrics {
    entries: usize,
    inter_item_gaps: usize,
    separators: usize,
    headers: usize,
    context_lines: usize,
}

impl PopupMetrics {
    fn height(self, cx: &App) -> Pixels {
        let list_padding = DynamicSpacing::Base04.px(cx) * 2.;
        let height = MENU_BORDER_HEIGHT
            + list_padding
            + MENU_ROW_HEIGHT * self.entries
            + MENU_GAP_HEIGHT * self.inter_item_gaps
            + MENU_SEPARATOR_HEIGHT * self.separators
            + MENU_HEADER_HEIGHT * self.headers
            + MENU_CONTEXT_LINE_HEIGHT * self.context_lines;
        px(height.as_f32().ceil().max(1.))
    }
}

/// A context menu prepared for rendering in an anchored child window.
pub struct AnchoredContextMenu {
    items: Vec<PopupItem>,
    target_window: AnyWindowHandle,
    height: Pixels,
    width: Pixels,
    has_custom_rows: bool,
}

type PopupHandler = Rc<dyn Fn(&mut Window, &mut App)>;
type PopupRowRenderer = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

struct PopupEntry {
    label: SharedString,
    toggle: Option<(IconPosition, bool)>,
    icon: Option<PopupIcon>,
    icon_color: Option<Color>,
    destructive: bool,
    meta: Option<SharedString>,
    action: Option<Box<dyn Action>>,
    handler: PopupHandler,
}

impl PopupEntry {
    fn new(label: SharedString, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        Self {
            label,
            toggle: None,
            icon: None,
            icon_color: None,
            destructive: false,
            meta: None,
            action: None,
            handler: Rc::new(handler),
        }
    }
}

enum PopupIcon {
    Asset(SharedString),
    External(SharedString),
}

enum PopupItem {
    Entry(PopupEntry),
    CustomRow(PopupRowRenderer),
    Gap,
    Separator,
    Header(SharedString, Option<SharedString>),
    ContextLine(Option<SharedString>, SharedString),
}

/// chartr's shared context-menu builder.
///
/// Zed's menu rows are flush by default. This wrapper inserts a small,
/// non-selectable gap between adjacent actions while leaving separators and
/// headers as distinct group boundaries.
pub struct ContextMenu {
    popup_items: Vec<PopupItem>,
    has_item_in_group: bool,
    metrics: PopupMetrics,
    popup_width: Pixels,
    has_custom_rows: bool,
}

impl ContextMenu {
    /// Entry handlers are routed back to `window`; the anchored popup is only
    /// a host for the stock UI context menu.
    pub fn build_popup(
        window: &mut Window,
        cx: &mut App,
        build: impl FnOnce(Self) -> Self,
    ) -> AnchoredContextMenu {
        let target_window = window.window_handle();
        let built = build(Self {
            popup_items: Vec::new(),
            has_item_in_group: false,
            metrics: PopupMetrics::default(),
            popup_width: POPUP_CONTENT_WIDTH,
            has_custom_rows: false,
        });
        AnchoredContextMenu {
            items: built.popup_items,
            target_window,
            height: built.metrics.height(cx),
            width: built.popup_width,
            has_custom_rows: built.has_custom_rows,
        }
    }

    fn before_item(mut self) -> Self {
        if self.has_item_in_group {
            self.popup_items.push(PopupItem::Gap);
            self.metrics.inter_item_gaps += 1;
        }
        self.has_item_in_group = true;
        self.metrics.entries += 1;
        self
    }

    pub fn entry(
        self,
        label: impl Into<SharedString>,
        action: Option<Box<dyn Action>>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        let mut this = self.before_item();
        this.popup_items.push(PopupItem::Entry(PopupEntry {
            action,
            ..PopupEntry::new(label.into(), handler)
        }));
        this
    }

    pub fn entry_with_icon_path(
        self,
        label: impl Into<SharedString>,
        icon_path: impl Into<SharedString>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.entry(label, None, handler).with_icon(icon_path)
    }

    pub fn entry_with_external_icon_path(
        self,
        label: impl Into<SharedString>,
        icon_path: impl Into<SharedString>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        let mut this = self.entry(label, None, handler);
        this.update_last_entry(|entry| entry.icon = Some(PopupIcon::External(icon_path.into())));
        this
    }

    /// Add a destructive action drawn in the theme's error colour.
    pub fn danger_entry(
        self,
        label: impl Into<SharedString>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        let mut this = self.entry(label, None, handler);
        this.update_last_entry(|entry| entry.destructive = true);
        this
    }

    fn update_last_entry(&mut self, update: impl FnOnce(&mut PopupEntry)) {
        if let Some(PopupItem::Entry(entry)) = self.popup_items.last_mut() {
            update(entry);
        }
    }

    /// Show an embedded icon at the start of the previous entry.
    pub fn with_icon(mut self, icon_path: impl Into<SharedString>) -> Self {
        self.update_last_entry(|entry| entry.icon = Some(PopupIcon::Asset(icon_path.into())));
        self
    }

    /// Draw the previous entry's icon in `color` instead of the muted default.
    pub fn with_icon_color(mut self, color: Color) -> Self {
        self.update_last_entry(|entry| entry.icon_color = Some(color));
        self
    }

    /// Show short muted text at the end of the previous entry.
    pub fn with_meta(mut self, meta: impl Into<SharedString>) -> Self {
        self.update_last_entry(|entry| entry.meta = Some(meta.into()));
        self
    }

    /// Add a non-selectable muted line naming what the menu acts on, with an
    /// optional emphasised lead.
    pub fn context_line(
        mut self,
        strong: Option<impl Into<SharedString>>,
        rest: impl Into<SharedString>,
    ) -> Self {
        self.popup_items.push(PopupItem::ContextLine(strong.map(Into::into), rest.into()));
        self.has_item_in_group = false;
        self.metrics.context_lines += 1;
        self
    }

    pub fn toggleable_entry(
        self,
        label: impl Into<SharedString>,
        toggled: bool,
        position: IconPosition,
        action: Option<Box<dyn Action>>,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        let mut this = self.entry(label, action, handler);
        this.update_last_entry(|entry| entry.toggle = Some((position, toggled)));
        this
    }

    /// Add a selectable row with an embedded icon at the start and its check at the end.
    pub fn toggleable_entry_with_icon_path(
        self,
        label: impl Into<SharedString>,
        icon_path: impl Into<SharedString>,
        toggled: bool,
        handler: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        let mut this = self.entry(label, None, handler).with_icon(icon_path);
        this.update_last_entry(|entry| entry.toggle = Some((IconPosition::End, toggled)));
        this
    }

    /// Add a non-selectable row whose rendered content determines its height.
    pub fn custom_row(
        mut self,
        render: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        let render: PopupRowRenderer = Rc::new(render);
        self.popup_items.push(PopupItem::CustomRow(render));
        self.has_custom_rows = true;
        self
    }

    /// Set the content width of an anchored native popup.
    pub fn popup_width(mut self, width: Pixels) -> Self {
        self.popup_width = width.max(px(1.));
        self
    }

    /// Show the keyboard shortcut bound to `action` on the previous entry.
    pub fn with_shortcut(mut self, action: Box<dyn Action>) -> Self {
        self.update_last_entry(|entry| entry.action = Some(action));
        self
    }

    pub fn separator(mut self) -> Self {
        self.popup_items.push(PopupItem::Separator);
        self.has_item_in_group = false;
        self.metrics.separators += 1;
        self
    }

    pub fn header(mut self, title: impl Into<SharedString>) -> Self {
        self.popup_items.push(PopupItem::Header(title.into(), None));
        self.has_item_in_group = false;
        self.metrics.headers += 1;
        self
    }

    /// A section header with muted, right-aligned text such as a count.
    pub fn header_with_meta(
        mut self,
        title: impl Into<SharedString>,
        meta: impl Into<SharedString>,
    ) -> Self {
        self.popup_items.push(PopupItem::Header(title.into(), Some(meta.into())));
        self.has_item_in_group = false;
        self.metrics.headers += 1;
        self
    }
}

impl FluentBuilder for ContextMenu {}

type PopupBuilder = Rc<dyn Fn(&mut Window, &mut App) -> Option<AnchoredContextMenu>>;

#[derive(Clone, Copy, Debug, PartialEq)]
enum PopupPlacement {
    Standard(Anchor),
    /// Top edge level with the pointer tip, opening to its right.
    Pointer,
}

/// A button-triggered menu rendered in a separate native popup window.
#[derive(IntoElement)]
pub struct PopupMenu {
    id: ElementId,
    trigger_builder: Option<Box<dyn FnOnce(bool, bool, &mut Window, &mut App) -> AnyElement>>,
    trigger_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    anchor: Rc<Cell<PopupPlacement>>,
    builder: Rc<RefCell<Option<PopupBuilder>>>,
}

impl PopupMenu {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            trigger_builder: None,
            trigger_bounds: Rc::default(),
            anchor: Rc::new(Cell::new(PopupPlacement::Standard(Anchor::TopLeft))),
            builder: Rc::default(),
        }
    }

    pub fn trigger<T: ui::PopoverTrigger>(mut self, trigger: T) -> Self {
        self.trigger_builder =
            Some(Box::new(move |_, open, _, _| trigger.toggle_state(open).into_any_element()));
        self
    }

    pub fn trigger_with_tooltip<T: ui::PopoverTrigger + ui::ButtonCommon>(
        mut self,
        trigger: T,
        tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static,
    ) -> Self {
        self.trigger_builder = Some(Box::new(move |window_active, open, _, _| {
            let trigger = trigger.toggle_state(open);
            if window_active {
                trigger.tooltip(tooltip).into_any_element()
            } else {
                trigger.into_any_element()
            }
        }));
        self
    }

    pub fn anchor(self, anchor: Anchor) -> Self {
        self.anchor.set(PopupPlacement::Standard(anchor));
        self
    }

    pub fn menu(
        self,
        builder: impl Fn(&mut Window, &mut App) -> Option<AnchoredContextMenu> + 'static,
    ) -> Self {
        *self.builder.borrow_mut() = Some(Rc::new(builder));
        self
    }
}

impl RenderOnce for PopupMenu {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let open = is_popup_open(&self.id, window.window_handle(), cx);
        let trigger = self.trigger_builder.take().expect("popup menus require a trigger")(
            window.is_window_active(),
            open,
            window,
            cx,
        );
        let trigger_id = self.id.clone();
        let bounds = self.trigger_bounds;
        let measured_bounds = bounds.clone();
        let anchor = self.anchor;
        let builder = self.builder;
        div()
            .id(self.id)
            .relative()
            .child(trigger)
            .child(
                canvas(move |measured, _, _| measured_bounds.set(Some(measured)), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                if dismiss_popup_for_trigger(&trigger_id, window.window_handle(), cx) {
                    return;
                }
                let Some(bounds) = bounds.get() else {
                    return;
                };
                let Some(builder) = builder.borrow().clone() else {
                    return;
                };
                let Some(menu) = builder(window, cx) else {
                    return;
                };
                let owner = Some(trigger_id.clone());
                open_popup(menu, bounds, anchor.get(), owner.clone(), owner, window, cx);
            })
    }
}

/// A secondary-click menu rendered in a separate native popup window.
#[derive(IntoElement)]
pub struct PopupRightClickMenu {
    id: ElementId,
    child_builder: Option<Box<dyn FnOnce(bool, &mut Window, &mut App) -> AnyElement>>,
    menu_builder: Rc<RefCell<Option<PopupBuilder>>>,
}

pub fn popup_right_click_menu(id: impl Into<ElementId>) -> PopupRightClickMenu {
    PopupRightClickMenu { id: id.into(), child_builder: None, menu_builder: Rc::default() }
}

impl PopupRightClickMenu {
    pub fn trigger<F, E>(mut self, trigger: F) -> Self
    where
        F: FnOnce(bool, &mut Window, &mut App) -> E + 'static,
        E: IntoElement + 'static,
    {
        self.child_builder = Some(Box::new(move |active, window, cx| {
            trigger(active, window, cx).into_any_element()
        }));
        self
    }

    pub fn menu(
        self,
        builder: impl Fn(&mut Window, &mut App) -> AnchoredContextMenu + 'static,
    ) -> Self {
        *self.menu_builder.borrow_mut() =
            Some(Rc::new(move |window, cx| Some(builder(window, cx))));
        self
    }
}

impl RenderOnce for PopupRightClickMenu {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let open = is_popup_open(&self.id, window.window_handle(), cx);
        let child = self.child_builder.take().expect("right-click menus require a trigger")(
            open, window, cx,
        );
        let builder = self.menu_builder;
        let owner = self.id.clone();
        div().id(self.id).child(child).on_mouse_down(
            MouseButton::Right,
            move |event, window, cx| {
                cx.stop_propagation();
                window.prevent_default();
                let Some(builder) = builder.borrow().clone() else {
                    return;
                };
                let Some(menu) = builder(window, cx) else {
                    return;
                };
                open_popup(
                    menu,
                    Bounds::new(event.position, size(px(1.), px(1.))),
                    PopupPlacement::Pointer,
                    None,
                    Some(owner.clone()),
                    window,
                    cx,
                );
            },
        )
    }
}

struct AnchoredMenuWindow {
    menu: Entity<UiContextMenu>,
    target_window: AnyWindowHandle,
    trigger_id: Option<ElementId>,
    /// The button or row that opened this menu, shown as active while it is open.
    owner: Option<ElementId>,
    popup_width: Pixels,
    maximum_height: Pixels,
    fitted_height: Rc<Cell<Option<Pixels>>>,
    _refresh_owner_on_close: Subscription,
}

impl AnchoredMenuWindow {
    fn new(
        menu: AnchoredContextMenu,
        trigger_id: Option<ElementId>,
        owner: Option<ElementId>,
        maximum_height: Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let target_window = menu.target_window;
        let menu_width = menu.width;
        let context_menu = Self::build_menu(menu, window, cx);

        window
            .subscribe(&context_menu, cx, move |_, _: &DismissEvent, window, _| {
                window.remove_window();
            })
            .detach();

        let focus = context_menu.focus_handle(cx);
        window.on_next_frame(move |window, _| {
            window.on_next_frame(move |window, cx| window.focus(&focus, cx));
        });
        Self {
            menu: context_menu,
            target_window,
            trigger_id,
            owner,
            popup_width: menu_width + POPUP_OUTSET * 2.,
            maximum_height,
            fitted_height: Rc::default(),
            // Let the owner drop its open styling once the menu is gone.
            _refresh_owner_on_close: cx.on_release(move |_, cx| {
                cx.defer(move |cx| {
                    let _ = target_window.update(cx, |_, window, _| window.refresh());
                })
            }),
        }
    }

    /// Builds the stock UI menu that renders chartr's popup items.
    fn build_menu(
        menu: AnchoredContextMenu,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<UiContextMenu> {
        let target_window = menu.target_window;
        let menu_height = menu.height;
        let menu_width = menu.width;
        UiContextMenu::build(window, cx, move |mut context_menu, _, _| {
            context_menu =
                context_menu.max_height(menu_height.into()).fixed_width(menu_width.into());
            for item in menu.items {
                context_menu = match item {
                    PopupItem::Entry(entry) => {
                        let target = target_window;
                        let handler = entry.handler;
                        let mut menu_entry = UiContextMenuEntry::new(entry.label)
                            .destructive(entry.destructive)
                            .handler(move |_, cx| {
                                let _ = target.update(cx, |_, window, cx| handler(window, cx));
                            });
                        if let Some(icon) = entry.icon {
                            menu_entry = match icon {
                                PopupIcon::Asset(path) => menu_entry.custom_icon_path(path),
                                PopupIcon::External(path) => menu_entry.custom_icon_svg(path),
                            }
                            .icon_position(IconPosition::Start)
                            .icon_size(IconSize::Small);
                        }
                        if let Some(color) = entry.icon_color {
                            menu_entry = menu_entry.icon_color(color);
                        }
                        if let Some((position, toggled)) = entry.toggle {
                            menu_entry = menu_entry.toggle(position, toggled);
                        }
                        if let Some(action) = entry.action {
                            menu_entry = menu_entry.action(action);
                        }
                        if let Some(meta) = entry.meta {
                            menu_entry = menu_entry.meta(meta);
                        }
                        context_menu.item(menu_entry)
                    }
                    PopupItem::CustomRow(render) => {
                        context_menu.custom_row(move |window, cx| render(window, cx))
                    }
                    PopupItem::Gap => {
                        context_menu.custom_row(|_, _| div().h(MENU_GAP_HEIGHT).into_any_element())
                    }
                    // Inset, quieter dividers and headings than the stock menu,
                    // matching the mockup. Fixed heights keep PopupMetrics exact.
                    PopupItem::Separator => context_menu.custom_row(|_, cx| {
                        div()
                            .h(MENU_SEPARATOR_HEIGHT)
                            .w_full()
                            .flex()
                            .items_center()
                            .child(div().h_px().w_full().bg(cx.theme().colors().border_variant))
                            .into_any_element()
                    }),
                    PopupItem::Header(label, meta) => context_menu.custom_row(move |_, _| {
                        div()
                            .h(MENU_HEADER_HEIGHT)
                            .w_full()
                            .flex()
                            .items_end()
                            .justify_between()
                            .pb(px(1.))
                            .child(
                                Label::new(label.clone())
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                            )
                            .children(meta.clone().map(|meta| {
                                Label::new(meta).size(LabelSize::XSmall).color(Color::Muted)
                            }))
                            .into_any_element()
                    }),
                    PopupItem::ContextLine(strong, rest) => context_menu.custom_row(move |_, _| {
                        h_flex()
                            .h(MENU_CONTEXT_LINE_HEIGHT)
                            .min_w_0()
                            .gap_1()
                            .when_some(strong.clone(), |this, strong| {
                                this.child(
                                    Label::new(strong)
                                        .size(LabelSize::Small)
                                        .color(Color::Default)
                                        .weight(gpui::FontWeight::MEDIUM),
                                )
                            })
                            .child(
                                Label::new(rest.clone())
                                    .size(LabelSize::Small)
                                    .color(Color::Muted)
                                    .truncate(),
                            )
                            .into_any_element()
                    }),
                };
            }
            context_menu
        })
    }
}

#[cfg(feature = "ui-lab")]
impl ContextMenu {
    /// Builds a menu in `window` without opening a popup, for UI-lab captures.
    pub(crate) fn build_lab_menu(
        window: &mut Window,
        cx: &mut App,
        build: impl FnOnce(Self) -> Self,
    ) -> Entity<UiContextMenu> {
        let menu = Self::build_popup(window, cx, build);
        AnchoredMenuWindow::build_menu(menu, window, cx)
    }
}

impl Render for AnchoredMenuWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let fitted_height = self.fitted_height.clone();
        let popup_width = self.popup_width;
        let maximum_height = self.maximum_height;
        // Share the app's key context so entries can show their chartr shortcuts.
        div().size_full().key_context("chartr").p(POPUP_OUTSET).child(
            div().id("anchored-menu-content").relative().w_full().child(self.menu.clone()).child(
                canvas(
                    move |bounds, window, _| {
                        if bounds.size.height <= px(1.) {
                            return;
                        }
                        let height = clamp_pixels(
                            bounds.size.height + POPUP_OUTSET * 2.,
                            px(1.),
                            maximum_height,
                        );
                        if fitted_height.replace(Some(height)) != Some(height) {
                            window.resize(size(popup_width, height));
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            ),
        )
    }
}

/// Whether a menu opened by `owner` in `parent` is still showing.
fn is_popup_open(owner: &ElementId, parent: AnyWindowHandle, cx: &App) -> bool {
    cx.windows().into_iter().filter_map(|window| window.downcast::<AnchoredMenuWindow>()).any(
        |popup| {
            popup.read(cx).is_ok_and(|menu| {
                menu.target_window == parent && menu.owner.as_ref() == Some(owner)
            })
        },
    )
}

/// Close any button-triggered menu owned by `parent`. Returning `true` for the same trigger
/// gives native popups toggle behavior and prevents their dismissing click from opening a second
/// popup before the first child window has finished closing.
fn dismiss_popup_for_trigger(
    trigger_id: &ElementId,
    parent: AnyWindowHandle,
    cx: &mut App,
) -> bool {
    let mut matched_trigger = false;
    let open_menus = cx
        .windows()
        .into_iter()
        .filter_map(|window| window.downcast::<AnchoredMenuWindow>())
        .collect::<Vec<_>>();

    for popup in open_menus {
        let (owned_by_parent, matches_trigger) = popup
            .read(cx)
            .map(|menu| {
                (
                    menu.target_window == parent && menu.trigger_id.is_some(),
                    menu.target_window == parent && menu.trigger_id.as_ref() == Some(trigger_id),
                )
            })
            .unwrap_or_default();
        if owned_by_parent {
            matched_trigger |= matches_trigger;
            let _ = popup.update(cx, |_, window, _| window.remove_window());
        }
    }

    matched_trigger
}

fn open_popup(
    mut menu: AnchoredContextMenu,
    trigger_bounds: Bounds<Pixels>,
    anchor: PopupPlacement,
    trigger_id: Option<ElementId>,
    owner: Option<ElementId>,
    parent_window: &mut Window,
    cx: &mut App,
) {
    let display = parent_window.display(cx);
    let display_id = display.as_ref().map(|display| display.id());
    let display_height = display
        .as_ref()
        .map(|display| display.visible_bounds().size.height)
        .unwrap_or_else(|| parent_window.bounds().size.height);
    let maximum_height =
        (parent_window.viewport_size().height - POPUP_GAP).max(px(1.)).min(display_height);
    let maximum_width = display
        .as_ref()
        .map(|display| display.visible_bounds().size.width)
        .unwrap_or_else(|| parent_window.bounds().size.width);
    let popup_height = if menu.has_custom_rows {
        maximum_height
    } else {
        clamp_pixels(menu.height + POPUP_OUTSET * 2., px(1.), maximum_height)
    };
    let popup_width = clamp_pixels(menu.width + POPUP_OUTSET * 2., px(1.), maximum_width);
    menu.height = (maximum_height - POPUP_OUTSET * 2.).max(px(1.));
    menu.width = (popup_width - POPUP_OUTSET * 2.).max(px(1.));
    let popup_size = size(popup_width, popup_height);
    let anchor =
        keep_inside_window(anchor, trigger_bounds, menu.width, parent_window.viewport_size().width);
    let kind = anchored_popup_window_kind(parent_window, trigger_bounds, anchor);
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                Default::default(),
                popup_size,
            ))),
            titlebar: None,
            focus: true,
            show: true,
            kind,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            display_id,
            window_background: WindowBackgroundAppearance::Transparent,
            window_min_size: Some(size(popup_width, px(1.))),
            ..Default::default()
        },
        move |window, cx| {
            cx.new(|cx| {
                AnchoredMenuWindow::new(menu, trigger_id, owner, maximum_height, window, cx)
            })
        },
    );

    match opened {
        Ok(_) => parent_window.refresh(),
        Err(error) => eprintln!("chartr could not open a menu: {error}"),
    }
}

/// A pull-down that would run past the window's right edge lines up with its
/// button's right edge instead, as macOS does near the edge of the screen.
fn keep_inside_window(
    anchor: PopupPlacement,
    trigger: Bounds<Pixels>,
    menu_width: Pixels,
    window_width: Pixels,
) -> PopupPlacement {
    match anchor {
        PopupPlacement::Standard(Anchor::TopLeft) if trigger.left() + menu_width > window_width => {
            PopupPlacement::Standard(Anchor::TopRight)
        }
        anchor => anchor,
    }
}

fn anchored_popup_window_kind(
    parent: &Window,
    trigger: Bounds<Pixels>,
    anchor: PopupPlacement,
) -> WindowKind {
    use gpui::popup::{PopupAnchor, PopupConstraintAdjustment, PopupGravity, PopupOptions};

    let (anchor, gravity, offset) = match anchor {
        PopupPlacement::Pointer => (
            PopupAnchor::TopLeft,
            PopupGravity::BottomRight,
            point(-POINTER_INSET - POPUP_OUTSET, -POINTER_INSET - POPUP_OUTSET),
        ),
        PopupPlacement::Standard(anchor) => match anchor {
            Anchor::TopLeft => (
                PopupAnchor::BottomLeft,
                PopupGravity::BottomRight,
                point(-POPUP_OUTSET, POPUP_GAP - POPUP_OUTSET),
            ),
            Anchor::TopCenter => {
                (PopupAnchor::Bottom, PopupGravity::Bottom, point(px(0.), POPUP_GAP - POPUP_OUTSET))
            }
            Anchor::TopRight => (
                PopupAnchor::BottomRight,
                PopupGravity::BottomLeft,
                point(POPUP_OUTSET, POPUP_GAP - POPUP_OUTSET),
            ),
            Anchor::BottomLeft => (
                PopupAnchor::TopLeft,
                PopupGravity::TopRight,
                point(-POPUP_OUTSET, POPUP_OUTSET - POPUP_GAP),
            ),
            Anchor::BottomCenter => {
                (PopupAnchor::Top, PopupGravity::Top, point(px(0.), POPUP_OUTSET - POPUP_GAP))
            }
            Anchor::BottomRight => (
                PopupAnchor::TopRight,
                PopupGravity::TopLeft,
                point(POPUP_OUTSET, POPUP_OUTSET - POPUP_GAP),
            ),
            Anchor::LeftCenter => {
                (PopupAnchor::Left, PopupGravity::Left, point(POPUP_OUTSET - POPUP_GAP, px(0.)))
            }
            Anchor::RightCenter => {
                (PopupAnchor::Right, PopupGravity::Right, point(POPUP_GAP - POPUP_OUTSET, px(0.)))
            }
        },
    };
    WindowKind::AnchoredPopup(PopupOptions {
        parent: parent.window_handle(),
        anchor_rect: trigger,
        anchor,
        gravity,
        constraint_adjustment: PopupConstraintAdjustment::SLIDE_X
            | PopupConstraintAdjustment::SLIDE_Y
            | PopupConstraintAdjustment::FLIP_X
            | PopupConstraintAdjustment::FLIP_Y,
        offset,
        grab: true,
    })
}

fn clamp_pixels(value: Pixels, minimum: Pixels, maximum: Pixels) -> Pixels {
    px(value.as_f32().clamp(minimum.as_f32(), maximum.max(minimum).as_f32()))
}

#[cfg(test)]
#[path = "popup_tests.rs"]
mod tests;
