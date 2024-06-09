use std::cell::RefCell;
use std::rc::Rc;

use rootvg::math::Point;
use rootvg::quad::{SolidQuadBuilder, SolidQuadPrimitive};
use rootvg::text::{TextPrimitive, TextProperties};
use rootvg::PrimitiveGroup;

use crate::event::{ElementEvent, EventCaptureStatus, PointerButton, PointerEvent};
use crate::layout::Padding;
use crate::math::{Rect, Size, ZIndex};
use crate::style::{Background, BorderStyle, QuadStyle, DEFAULT_TEXT_ATTRIBUTES};
use crate::vg::color::{self, RGBA8};
use crate::view::element::{
    Element, ElementBuilder, ElementContext, ElementFlags, ElementHandle, RenderContext,
};
use crate::view::{ScissorRectID, MAIN_SCISSOR_RECT};
use crate::window::WindowContext;
use crate::CursorIcon;

use super::dual_label::{DualLabelInner, DualLabelLayout, DualLabelStyle};

// TODO: list of todos:
// * handle cases when the menu is too large to fit in the window, with
// two selectable behaviors:
//   * option A: use scroll wheel
//   * option B: stack horizontally
// * nested menus
// * keyboard navigation

#[derive(Debug, Clone, PartialEq)]
pub enum MenuEntry {
    Option {
        left_text: String,
        right_text: String,
        unique_id: usize,
    },
    Divider,
    // TODO: Nested menus
}

enum MenuEntryInner {
    Option {
        dual_label: DualLabelInner,
        start_y: f32,
        end_y: f32,
        unique_id: usize,
    },
    Divider {
        y: f32,
    },
    // TODO: Nested menus
}

/// The style of a [`DropDownMenu`] element
#[derive(Debug, Clone, PartialEq)]
pub struct DropDownMenuStyle {
    pub left_text_properties: TextProperties,
    pub right_text_properties: TextProperties,

    pub left_text_color_idle: RGBA8,
    pub right_text_color_idle: RGBA8,
    pub left_text_color_hover: RGBA8,
    pub right_text_color_hover: RGBA8,

    pub back_quad: QuadStyle,
    pub text_bg_quad_hover: QuadStyle,

    pub outer_padding: f32,
    pub left_text_padding: Padding,
    pub right_text_padding: Padding,

    pub divider_color: RGBA8,
    pub divider_width: f32,
    pub divider_padding: f32,
}

impl Default for DropDownMenuStyle {
    fn default() -> Self {
        Self {
            left_text_properties: TextProperties {
                attrs: DEFAULT_TEXT_ATTRIBUTES,
                ..Default::default()
            },
            right_text_properties: TextProperties {
                attrs: DEFAULT_TEXT_ATTRIBUTES,
                ..Default::default()
            },

            left_text_color_idle: color::WHITE,
            right_text_color_idle: color::WHITE,
            left_text_color_hover: color::WHITE,
            right_text_color_hover: color::WHITE,

            back_quad: QuadStyle {
                bg: Background::Solid(RGBA8::new(40, 40, 40, 255)),
                border: BorderStyle {
                    radius: 4.0.into(),
                    color: RGBA8::new(105, 105, 105, 255),
                    width: 1.0,
                    ..Default::default()
                },
            },
            text_bg_quad_hover: QuadStyle {
                bg: Background::Solid(RGBA8::new(65, 65, 65, 255)),
                border: BorderStyle {
                    radius: 4.0.into(),
                    color: RGBA8::new(105, 105, 105, 255),
                    width: 1.0,
                    ..Default::default()
                },
            },

            outer_padding: 4.0,
            left_text_padding: Padding::new(5.0, 10.0, 5.0, 10.0),
            right_text_padding: Padding::new(5.0, 10.0, 5.0, 30.0),

            divider_color: RGBA8::new(105, 105, 105, 150),
            divider_width: 1.0,
            divider_padding: 2.0,
        }
    }
}

impl DropDownMenuStyle {
    fn dual_label_style(&self, hovered: bool) -> DualLabelStyle {
        DualLabelStyle {
            left_properties: self.left_text_properties,
            right_properties: self.right_text_properties,
            left_font_color: if hovered {
                self.left_text_color_hover
            } else {
                self.left_text_color_idle
            },
            right_font_color: if hovered {
                self.right_text_color_hover
            } else {
                self.right_text_color_idle
            },
            vertical_align: crate::layout::Align::Center,
            left_padding: self.left_text_padding,
            right_padding: self.right_text_padding,
            layout: DualLabelLayout::LeftAndRightAlign,
            ..DualLabelStyle::default()
        }
    }

    fn text_row_height(&self) -> f32 {
        (self.left_text_properties.metrics.line_height
            + self.left_text_padding.top
            + self.left_text_padding.bottom)
            .max(
                self.right_text_properties.metrics.line_height
                    + self.right_text_padding.top
                    + self.right_text_padding.bottom,
            )
    }

    fn measure(&self, entries: &mut [MenuEntryInner]) -> Size {
        if entries.is_empty() {
            return Size::default();
        }

        let text_row_height = self.text_row_height();
        let dual_label_style = self.dual_label_style(false);

        let mut max_width: f32 = 0.0;
        let mut total_height: f32 = self.outer_padding;
        for entry in entries.iter_mut() {
            match entry {
                MenuEntryInner::Option {
                    dual_label,
                    start_y,
                    end_y,
                    ..
                } => {
                    let size = dual_label.desired_padded_size(&dual_label_style);

                    max_width = max_width.max(size.width);

                    *start_y = total_height;
                    total_height += text_row_height;
                    *end_y = total_height;
                }
                MenuEntryInner::Divider { y } => {
                    *y = total_height + self.divider_padding;

                    total_height +=
                        self.divider_width + self.divider_padding + self.divider_padding;
                }
            }
        }

        Size::new(
            max_width.ceil() + (self.outer_padding * 2.0),
            total_height + self.outer_padding,
        )
    }
}

pub struct DropDownMenuBuilder<A: Clone + 'static> {
    pub action: Option<Box<dyn FnMut(usize) -> A>>,
    pub entries: Vec<MenuEntry>,
    pub style: Rc<DropDownMenuStyle>,
    pub z_index: ZIndex,
    pub position: Point,
    pub scissor_rect_id: ScissorRectID,
}

impl<A: Clone + 'static> DropDownMenuBuilder<A> {
    pub fn new(style: &Rc<DropDownMenuStyle>) -> Self {
        Self {
            action: None,
            entries: Vec::new(),
            style: Rc::clone(style),
            z_index: 0,
            position: Point::default(),
            scissor_rect_id: MAIN_SCISSOR_RECT,
        }
    }

    pub fn build(self, cx: &mut WindowContext<'_, A>) -> DropDownMenu {
        DropDownMenuElement::create(self, cx)
    }

    pub fn on_entry_selected<F: FnMut(usize) -> A + 'static>(mut self, f: F) -> Self {
        self.action = Some(Box::new(f));
        self
    }

    pub fn entries(mut self, entries: Vec<MenuEntry>) -> Self {
        self.entries = entries;
        self
    }

    pub const fn z_index(mut self, z_index: ZIndex) -> Self {
        self.z_index = z_index;
        self
    }

    pub const fn position(mut self, position: Point) -> Self {
        self.position = position;
        self
    }

    pub const fn scissor_rect(mut self, scissor_rect_id: ScissorRectID) -> Self {
        self.scissor_rect_id = scissor_rect_id;
        self
    }
}

pub struct DropDownMenuElement<A: Clone + 'static> {
    shared_state: Rc<RefCell<SharedState>>,
    action: Option<Box<dyn FnMut(usize) -> A>>,
    entries: Vec<MenuEntryInner>,
    size: Size,
    active: bool,
    hovered_entry_index: Option<usize>,
}

impl<A: Clone + 'static> DropDownMenuElement<A> {
    pub fn create(builder: DropDownMenuBuilder<A>, cx: &mut WindowContext<'_, A>) -> DropDownMenu {
        let DropDownMenuBuilder {
            action,
            entries,
            style,
            z_index,
            position,
            scissor_rect_id,
        } = builder;

        let shared_state = Rc::new(RefCell::new(SharedState {
            style: Rc::clone(&style),
            new_entries: None,
            open_requested: false,
            style_changed: false,
        }));

        let dual_label_style = style.dual_label_style(false);

        let mut entries: Vec<MenuEntryInner> = entries
            .into_iter()
            .map(|entry| match entry {
                MenuEntry::Option {
                    left_text,
                    right_text,
                    unique_id,
                } => MenuEntryInner::Option {
                    dual_label: DualLabelInner::new(
                        left_text,
                        right_text,
                        Point::default(),
                        Point::default(),
                        &dual_label_style,
                        cx.font_system,
                    ),
                    start_y: 0.0,
                    end_y: 0.0,
                    unique_id,
                },
                MenuEntry::Divider => MenuEntryInner::Divider { y: 0.0 },
            })
            .collect();

        let size = style.measure(&mut entries);

        let element_builder = ElementBuilder {
            element: Box::new(Self {
                shared_state: Rc::clone(&shared_state),
                action,
                entries,
                size,
                active: false,
                hovered_entry_index: None,
            }),
            z_index,
            bounding_rect: Rect::new(position, Size::zero()),
            manually_hidden: false,
            scissor_rect_id,
        };

        let el = cx
            .view
            .add_element(element_builder, cx.font_system, cx.clipboard);

        DropDownMenu { el, shared_state }
    }
}

impl<A: Clone + 'static> Element<A> for DropDownMenuElement<A> {
    fn flags(&self) -> ElementFlags {
        ElementFlags::PAINTS
            | ElementFlags::LISTENS_TO_POINTER_INSIDE_BOUNDS
            | ElementFlags::LISTENS_TO_FOCUS_CHANGE
            | ElementFlags::LISTENS_TO_POINTER_OUTSIDE_BOUNDS_WHEN_FOCUSED
            | ElementFlags::LISTENS_TO_POSITION_CHANGE
    }

    fn on_event(
        &mut self,
        event: ElementEvent,
        cx: &mut ElementContext<'_, A>,
    ) -> EventCaptureStatus {
        match event {
            ElementEvent::CustomStateChanged => {
                let mut shared_state = RefCell::borrow_mut(&self.shared_state);

                let mut show = false;
                let mut request_focus = false;
                if shared_state.open_requested && !self.active {
                    self.active = true;
                    show = true;
                    request_focus = true;
                }
                shared_state.open_requested = false;

                let mut do_restyle = shared_state.style_changed;
                shared_state.style_changed = false;

                let mut measure = false;

                if let Some(new_entries) = shared_state.new_entries.take() {
                    let dual_label_style = shared_state.style.dual_label_style(false);

                    self.entries = new_entries
                        .into_iter()
                        .map(|entry| match entry {
                            MenuEntry::Option {
                                left_text,
                                right_text,
                                unique_id,
                            } => MenuEntryInner::Option {
                                dual_label: DualLabelInner::new(
                                    left_text,
                                    right_text,
                                    Point::default(),
                                    Point::default(),
                                    &dual_label_style,
                                    cx.font_system,
                                ),
                                start_y: 0.0,
                                end_y: 0.0,
                                unique_id,
                            },
                            MenuEntry::Divider => MenuEntryInner::Divider { y: 0.0 },
                        })
                        .collect();

                    measure = true;
                    do_restyle = false;
                }

                if do_restyle {
                    let dual_label_style = shared_state.style.dual_label_style(false);

                    for entry in self.entries.iter_mut() {
                        match entry {
                            MenuEntryInner::Option { dual_label, .. } => {
                                dual_label.set_style(&dual_label_style, cx.font_system);
                            }
                            _ => {}
                        }
                    }

                    measure = true;
                }

                if measure {
                    show = false;

                    self.size = shared_state.style.measure(&mut self.entries);

                    if self.active {
                        let rect = Rect::new(cx.rect().origin, self.size);
                        let layout_info = layout(rect, cx.window_size());
                        if let Some(new_bounds) = layout_info.new_bounds {
                            cx.set_bounding_rect(new_bounds);
                        } else {
                            cx.set_bounding_rect(rect)
                        }
                        cx.request_repaint();
                    } else {
                        cx.set_bounding_rect(Rect::new(cx.rect().origin, Size::zero()));
                    }
                }

                if show {
                    let rect = Rect::new(cx.rect().origin, self.size);
                    let layout_info = layout(rect, cx.window_size());
                    if let Some(new_bounds) = layout_info.new_bounds {
                        cx.set_bounding_rect(new_bounds);
                    } else {
                        cx.set_bounding_rect(rect)
                    }
                }

                if request_focus {
                    cx.steal_temporary_focus();
                    cx.listen_to_pointer_clicked_off();
                }
            }
            ElementEvent::ClickedOff => {
                cx.release_focus();
            }
            ElementEvent::ExclusiveFocus(false) => {
                self.active = false;
                self.hovered_entry_index = None;
                cx.release_focus();
                cx.set_bounding_rect(Rect::new(cx.rect().origin, Size::zero()));
            }
            ElementEvent::Pointer(PointerEvent::Moved { position, .. }) => {
                if !self.active {
                    return EventCaptureStatus::NotCaptured;
                }

                let mut new_hovered_entry_index = None;
                let pointer_y = position.y - cx.rect().min_y();
                if cx.rect().contains(position) {
                    for (i, entry) in self.entries.iter().enumerate() {
                        match entry {
                            MenuEntryInner::Option { start_y, end_y, .. } => {
                                if pointer_y >= *start_y && pointer_y < *end_y {
                                    new_hovered_entry_index = Some(i);
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                }

                if self.hovered_entry_index != new_hovered_entry_index {
                    self.hovered_entry_index = new_hovered_entry_index;
                    cx.request_repaint();
                }

                if self.hovered_entry_index.is_some() {
                    cx.cursor_icon = CursorIcon::Pointer;
                }

                return EventCaptureStatus::Captured;
            }
            ElementEvent::Pointer(PointerEvent::PointerLeft) => {
                if !self.active {
                    return EventCaptureStatus::NotCaptured;
                }

                return EventCaptureStatus::Captured;
            }
            ElementEvent::Pointer(PointerEvent::ButtonJustPressed {
                button, position, ..
            }) => {
                if !self.active {
                    return EventCaptureStatus::NotCaptured;
                }

                if button == PointerButton::Primary && cx.rect().contains(position) {
                    let mut selected_entry_id = None;
                    let pointer_y = position.y - cx.rect().min_y();
                    for entry in self.entries.iter() {
                        match entry {
                            MenuEntryInner::Option {
                                start_y,
                                end_y,
                                unique_id,
                                ..
                            } => {
                                if pointer_y >= *start_y && pointer_y < *end_y {
                                    selected_entry_id = Some(*unique_id);
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }

                    if let Some(id) = selected_entry_id {
                        if let Some(action) = &mut self.action {
                            cx.send_action((action)(id)).unwrap();
                        }

                        cx.release_focus();
                        cx.cursor_icon = CursorIcon::Default;
                    }
                }

                return EventCaptureStatus::Captured;
            }
            ElementEvent::Pointer(..) => {
                if !self.active {
                    return EventCaptureStatus::NotCaptured;
                }

                return EventCaptureStatus::Captured;
            }
            ElementEvent::PositionChanged => {
                if !self.active {
                    return EventCaptureStatus::NotCaptured;
                }

                let layout_info = layout(cx.rect(), cx.window_size());
                if let Some(new_bounds) = layout_info.new_bounds {
                    cx.set_bounding_rect(new_bounds);
                }
            }
            _ => {}
        }

        EventCaptureStatus::NotCaptured
    }

    fn render_primitives(&mut self, cx: RenderContext<'_>, primitives: &mut PrimitiveGroup) {
        let style = &RefCell::borrow(&self.shared_state).style;

        let dual_label_style_idle = style.dual_label_style(false);
        let dual_label_style_hover = style.dual_label_style(true);

        let label_size = Size::new(
            self.size.width - (style.outer_padding * 2.0),
            style.text_row_height(),
        );

        let mut text_primitives: Vec<TextPrimitive> = Vec::with_capacity(self.entries.len() * 2);
        let mut divider_primitives: Vec<SolidQuadPrimitive> =
            Vec::with_capacity(self.entries.len());

        primitives.add(
            style
                .back_quad
                .create_primitive(Rect::from_size(cx.bounds_size)),
        );

        for (i, entry) in self.entries.iter_mut().enumerate() {
            match entry {
                MenuEntryInner::Option {
                    dual_label,
                    start_y,
                    ..
                } => {
                    let hovered = if let Some(hovered_index) = self.hovered_entry_index {
                        i == hovered_index
                    } else {
                        false
                    };

                    if hovered {
                        primitives.set_z_index(1);
                        primitives.add(style.text_bg_quad_hover.create_primitive(Rect::new(
                            Point::new(style.outer_padding, *start_y),
                            label_size,
                        )));
                    }

                    let mut p = dual_label.render_primitives(
                        Rect::new(Point::new(style.outer_padding, *start_y), label_size),
                        if hovered {
                            &dual_label_style_hover
                        } else {
                            &dual_label_style_idle
                        },
                        cx.font_system,
                    );

                    if let Some(left_text) = p.left_text.take() {
                        text_primitives.push(left_text);
                    }
                    if let Some(right_text) = p.right_text.take() {
                        text_primitives.push(right_text);
                    }
                }
                MenuEntryInner::Divider { y } => divider_primitives.push(
                    SolidQuadBuilder::new(Size::new(label_size.width, style.divider_width))
                        .bg_color(style.divider_color)
                        .position(Point::new(style.outer_padding, *y))
                        .into(),
                ),
            }
        }

        primitives.set_z_index(2);

        // It is more efficient to batch primitives together.
        primitives.add_text_batch(text_primitives);
        primitives.add_solid_quad_batch(divider_primitives);
    }
}

struct SharedState {
    style: Rc<DropDownMenuStyle>,
    new_entries: Option<Vec<MenuEntry>>,
    open_requested: bool,
    style_changed: bool,
}

/// A handle to a [`DropDownMenuElement`].
pub struct DropDownMenu {
    pub el: ElementHandle,
    shared_state: Rc<RefCell<SharedState>>,
}

impl DropDownMenu {
    pub fn builder<A: Clone + 'static>(style: &Rc<DropDownMenuStyle>) -> DropDownMenuBuilder<A> {
        DropDownMenuBuilder::new(style)
    }

    pub fn set_style(&mut self, style: &Rc<DropDownMenuStyle>) {
        let mut shared_state = RefCell::borrow_mut(&self.shared_state);

        if !Rc::ptr_eq(&shared_state.style, style) {
            shared_state.style = Rc::clone(style);
            shared_state.style_changed = true;
            self.el.notify_custom_state_change();
        }
    }

    pub fn style(&self) -> Rc<DropDownMenuStyle> {
        Rc::clone(&RefCell::borrow(&self.shared_state).style)
    }

    pub fn set_position(&mut self, pos: Point) {
        self.el.set_pos(pos)
    }

    pub fn set_entries(&mut self, entries: Vec<MenuEntry>) {
        RefCell::borrow_mut(&self.shared_state).new_entries = Some(entries);
        self.el.notify_custom_state_change();
    }

    pub fn open(&mut self, position: Option<Point>) {
        if let Some(pos) = position {
            self.set_position(pos);
        }

        RefCell::borrow_mut(&self.shared_state).open_requested = true;
        self.el.notify_custom_state_change();
    }
}

fn layout(current_bounds: Rect, window_size: Size) -> LayoutInfo {
    let window_rect = Rect::from_size(window_size);

    let (width, width_clipped) = if current_bounds.width() > window_rect.width() {
        (window_rect.width(), true)
    } else {
        (current_bounds.width(), false)
    };
    let (height, height_clipped) = if current_bounds.height() > window_rect.height() {
        (window_rect.height(), true)
    } else {
        (current_bounds.height(), false)
    };

    let (x, x_repositioned) = if current_bounds.min_x() <= 0.0 {
        (0.0, true)
    } else if current_bounds.min_x() + width > window_size.width {
        (window_size.width - width, true)
    } else {
        (current_bounds.min_x(), false)
    };
    let (y, y_repositioned) = if current_bounds.min_y() <= 0.0 {
        (0.0, true)
    } else if current_bounds.min_y() + height > window_size.height {
        (window_size.height - height, true)
    } else {
        (current_bounds.min_y(), false)
    };

    let new_bounds = if width_clipped || height_clipped || x_repositioned || y_repositioned {
        Some(Rect::new(Point::new(x, y), Size::new(width, height)))
    } else {
        None
    };

    LayoutInfo {
        new_bounds,
        width_clipped,
        height_clipped,
    }
}

struct LayoutInfo {
    new_bounds: Option<Rect>,
    // TODO: use these
    #[allow(unused)]
    width_clipped: bool,
    #[allow(unused)]
    height_clipped: bool,
}
