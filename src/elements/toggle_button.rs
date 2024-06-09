use std::cell::{Ref, RefCell};
use std::rc::Rc;

use rootvg::math::Point;
use rootvg::text::glyphon::FontSystem;
use rootvg::text::TextProperties;
use rootvg::PrimitiveGroup;

use crate::event::{ElementEvent, EventCaptureStatus, PointerButton, PointerEvent};
use crate::layout::{Align, Align2, Padding};
use crate::math::{Rect, Size, ZIndex};
use crate::style::{
    Background, BorderStyle, QuadStyle, DEFAULT_ACCENT_COLOR, DEFAULT_TEXT_ATTRIBUTES,
};
use crate::vg::color::{self, RGBA8};
use crate::view::element::{
    Element, ElementBuilder, ElementContext, ElementFlags, ElementHandle, ElementTooltipInfo,
    RenderContext,
};
use crate::view::{ScissorRectID, MAIN_SCISSOR_RECT};
use crate::window::WindowContext;
use crate::CursorIcon;

use super::button::{ButtonState, ButtonStylePart};
use super::label::{LabelInner, LabelPrimitives, LabelStyle};

/// The style of a [`ToggleButton`] element
#[derive(Debug, Clone, PartialEq)]
pub struct ToggleButtonStyle {
    /// The text properties.
    pub properties: TextProperties,

    /// The vertical alignment of the text.
    ///
    /// By default this is set to `Align::Center`.
    pub vertical_align: Align,

    /// The minimum size of the clipped text area.
    ///
    /// By default this is set to `Size::new(5.0, 5.0)`.
    pub min_clipped_size: Size,

    /// The padding between the text and the bounding rectangle.
    ///
    /// By default this has all values set to `0.0`.
    pub padding: Padding,

    pub idle_on: ButtonStylePart,
    pub hovered_on: ButtonStylePart,
    //pub down_on: ButtonStylePart,
    pub disabled_on: ButtonStylePart,

    pub idle_off: ButtonStylePart,
    pub hovered_off: ButtonStylePart,
    //pub down_off: ButtonStylePart,
    pub disabled_off: ButtonStylePart,
}

impl ToggleButtonStyle {
    pub fn label_style(&self, state: ButtonState, toggled: bool) -> LabelStyle {
        let part = if toggled {
            match state {
                ButtonState::Idle => &self.idle_on,
                ButtonState::Hovered | ButtonState::Down => &self.hovered_on,
                ButtonState::Disabled => &self.disabled_on,
            }
        } else {
            match state {
                ButtonState::Idle => &self.idle_off,
                ButtonState::Hovered | ButtonState::Down => &self.hovered_off,
                ButtonState::Disabled => &self.disabled_off,
            }
        };

        LabelStyle {
            properties: self.properties,
            font_color: part.font_color,
            vertical_align: self.vertical_align,
            min_clipped_size: self.min_clipped_size,
            back_quad: part.back_quad.clone(),
            padding: self.padding,
        }
    }
}

impl Default for ToggleButtonStyle {
    fn default() -> Self {
        let idle_on = ButtonStylePart {
            font_color: color::WHITE,
            back_quad: QuadStyle {
                bg: Background::Solid(DEFAULT_ACCENT_COLOR),
                border: BorderStyle {
                    radius: 4.0.into(),
                    color: RGBA8::new(105, 105, 105, 255),
                    width: 1.0,
                    ..Default::default()
                },
            },
        };

        let idle_off = ButtonStylePart {
            back_quad: QuadStyle {
                bg: Background::Solid(RGBA8::new(40, 40, 40, 255)),
                ..idle_on.back_quad
            },
            ..idle_on
        };

        Self {
            properties: TextProperties {
                attrs: DEFAULT_TEXT_ATTRIBUTES,
                align: Some(rootvg::text::Align::Center),
                ..Default::default()
            },
            vertical_align: Align::Center,
            min_clipped_size: Size::new(5.0, 5.0),
            padding: Padding::new(6.0, 6.0, 6.0, 6.0),

            idle_on: idle_on.clone(),
            hovered_on: ButtonStylePart {
                back_quad: QuadStyle {
                    border: BorderStyle {
                        color: RGBA8::new(135, 135, 135, 255),
                        ..idle_on.back_quad.border
                    },
                    ..idle_on.back_quad
                },
                ..idle_on
            },
            /*
            down_on: ButtonStylePart {
                back_quad: QuadStyle {
                    bg: Background::Solid(RGBA8::new(40, 40, 40, 255)),
                    ..idle_on.back_quad
                },
                ..idle_on
            },
            */
            disabled_on: ButtonStylePart {
                font_color: RGBA8::new(150, 150, 150, 255),
                back_quad: QuadStyle {
                    bg: Background::Solid(RGBA8::new(76, 76, 76, 255)),
                    border: BorderStyle {
                        color: RGBA8::new(80, 80, 80, 255),
                        ..idle_on.back_quad.border
                    },
                },
                ..idle_on
            },

            idle_off: idle_off.clone(),
            hovered_off: ButtonStylePart {
                back_quad: QuadStyle {
                    border: BorderStyle {
                        color: RGBA8::new(135, 135, 135, 255),
                        ..idle_off.back_quad.border
                    },
                    ..idle_off.back_quad
                },
                ..idle_off
            },
            /*
            down_off: ButtonStylePart {
                back_quad: QuadStyle {
                    bg: Background::Solid(RGBA8::new(40, 40, 40, 255)),
                    ..idle_off.back_quad
                },
                ..idle_off
            },
            */
            disabled_off: ButtonStylePart {
                font_color: RGBA8::new(150, 150, 150, 255),
                back_quad: QuadStyle {
                    bg: Background::Solid(RGBA8::new(40, 40, 40, 255)),
                    border: BorderStyle {
                        color: RGBA8::new(80, 80, 80, 255),
                        ..idle_off.back_quad.border
                    },
                },
                ..idle_off
            },
        }
    }
}

/// A reusable button struct that can be used by other elements.
pub struct ToggleButtonInner {
    state: ButtonState,
    label_state: LabelInner,
    toggled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateChangeResult {
    pub state_changed: bool,
    pub needs_repaint: bool,
}

impl ToggleButtonInner {
    pub fn new(
        text: String,
        text_offset: Point,
        toggled: bool,
        style: &ToggleButtonStyle,
        font_system: &mut FontSystem,
    ) -> Self {
        let label_state = LabelInner::new(
            text,
            &style.label_style(ButtonState::Idle, toggled),
            font_system,
            text_offset,
        );

        Self {
            label_state,
            state: ButtonState::Idle,
            toggled,
        }
    }

    pub fn set_state(
        &mut self,
        state: ButtonState,
        style: &ToggleButtonStyle,
    ) -> StateChangeResult {
        if self.state != state {
            let old_part = if self.toggled {
                match self.state {
                    ButtonState::Idle => &style.idle_on,
                    ButtonState::Hovered | ButtonState::Down => &style.hovered_on,
                    ButtonState::Disabled => &style.disabled_on,
                }
            } else {
                match self.state {
                    ButtonState::Idle => &style.idle_off,
                    ButtonState::Hovered | ButtonState::Down => &style.hovered_off,
                    ButtonState::Disabled => &style.disabled_off,
                }
            };

            let new_part = if self.toggled {
                match state {
                    ButtonState::Idle => &style.idle_on,
                    ButtonState::Hovered | ButtonState::Down => &style.hovered_on,
                    ButtonState::Disabled => &style.disabled_on,
                }
            } else {
                match state {
                    ButtonState::Idle => &style.idle_off,
                    ButtonState::Hovered | ButtonState::Down => &style.hovered_off,
                    ButtonState::Disabled => &style.disabled_off,
                }
            };

            let needs_repaint = old_part != new_part;

            self.state = state;

            StateChangeResult {
                state_changed: true,
                needs_repaint,
            }
        } else {
            StateChangeResult {
                state_changed: false,
                needs_repaint: false,
            }
        }
    }

    pub fn state(&self) -> ButtonState {
        self.state
    }

    pub fn set_style(&mut self, style: &ToggleButtonStyle, font_system: &mut FontSystem) {
        self.label_state
            .set_style(&self.label_style(style), font_system);
    }

    pub fn set_toggled(&mut self, toggled: bool) -> StateChangeResult {
        if self.toggled != toggled {
            self.toggled = toggled;

            StateChangeResult {
                state_changed: true,
                needs_repaint: true,
            }
        } else {
            StateChangeResult {
                state_changed: false,
                needs_repaint: false,
            }
        }
    }

    pub fn toggled(&self) -> bool {
        self.toggled
    }

    /// Returns the size of the padded background rectangle if it were to
    /// cover the entire size of the unclipped text.
    pub fn desired_padded_size(&mut self, style: &ToggleButtonStyle) -> Size {
        self.label_state
            .desired_padded_size(&self.label_style(style))
    }

    /// Returns the size of the unclipped text.
    ///
    /// This can be useful to lay out elements that depend on text size.
    pub fn unclipped_text_size(&mut self) -> Size {
        self.label_state.unclipped_text_size()
    }

    /// Returns `true` if the text has changed.
    pub fn set_text(&mut self, text: &str, font_system: &mut FontSystem) -> bool {
        self.label_state.set_text(text, font_system)
    }

    pub fn text(&self) -> &str {
        self.label_state.text()
    }

    pub fn label_style(&self, style: &ToggleButtonStyle) -> LabelStyle {
        style.label_style(self.state, self.toggled)
    }

    pub fn render_primitives(
        &mut self,
        bounds: Rect,
        style: &ToggleButtonStyle,
        font_system: &mut FontSystem,
    ) -> LabelPrimitives {
        let p = self
            .label_state
            .render_primitives(bounds, &self.label_style(style), font_system);
        LabelPrimitives {
            text: p.text,
            bg_quad: p.bg_quad,
        }
    }

    /// An offset that can be used mainly to correct the position of icon glyphs.
    /// This does not effect the position of the background quad.
    ///
    /// Returns `true` if the text offset has changed.
    pub fn set_text_offset(&mut self, offset: Point) -> bool {
        self.label_state.set_text_offset(offset)
    }
}

pub struct ToggleButtonBuilder<A: Clone + 'static> {
    pub action: Option<Box<dyn FnMut(bool) -> A>>,
    pub tooltip_message: Option<String>,
    pub tooltip_align: Align2,
    pub toggled: bool,
    pub text: String,
    pub text_offset: Point,
    pub style: Rc<ToggleButtonStyle>,
    pub z_index: ZIndex,
    pub bounding_rect: Rect,
    pub manually_hidden: bool,
    pub scissor_rect_id: ScissorRectID,
}

impl<A: Clone + 'static> ToggleButtonBuilder<A> {
    pub fn new(style: &Rc<ToggleButtonStyle>) -> Self {
        Self {
            action: None,
            tooltip_message: None,
            tooltip_align: Align2::TOP_CENTER,
            toggled: false,
            text: String::new(),
            text_offset: Point::default(),
            style: Rc::clone(style),
            z_index: 0,
            bounding_rect: Rect::default(),
            manually_hidden: false,
            scissor_rect_id: MAIN_SCISSOR_RECT,
        }
    }

    pub fn build(self, cx: &mut WindowContext<'_, A>) -> ToggleButton {
        ToggleButtonElement::create(self, cx)
    }

    pub fn on_toggled<F: FnMut(bool) -> A + 'static>(mut self, f: F) -> Self {
        self.action = Some(Box::new(f));
        self
    }

    pub fn tooltip_message(mut self, message: impl Into<String>, align: Align2) -> Self {
        self.tooltip_message = Some(message.into());
        self.tooltip_align = align;
        self
    }

    pub const fn toggled(mut self, toggled: bool) -> Self {
        self.toggled = toggled;
        self
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }

    // An offset that can be used mainly to correct the position of icon glyphs.
    /// This does not effect the position of the background quad.
    pub const fn text_offset(mut self, offset: Point) -> Self {
        self.text_offset = offset;
        self
    }

    pub const fn z_index(mut self, z_index: ZIndex) -> Self {
        self.z_index = z_index;
        self
    }

    pub const fn bounding_rect(mut self, rect: Rect) -> Self {
        self.bounding_rect = rect;
        self
    }

    pub const fn hidden(mut self, hidden: bool) -> Self {
        self.manually_hidden = hidden;
        self
    }

    pub const fn scissor_rect(mut self, scissor_rect_id: ScissorRectID) -> Self {
        self.scissor_rect_id = scissor_rect_id;
        self
    }
}

/// A button element with a label.
pub struct ToggleButtonElement<A: Clone + 'static> {
    shared_state: Rc<RefCell<SharedState>>,
    action: Option<Box<dyn FnMut(bool) -> A>>,
    tooltip_message: Option<String>,
    tooltip_align: Align2,
}

impl<A: Clone + 'static> ToggleButtonElement<A> {
    pub fn create(builder: ToggleButtonBuilder<A>, cx: &mut WindowContext<'_, A>) -> ToggleButton {
        let ToggleButtonBuilder {
            action,
            tooltip_message,
            tooltip_align,
            toggled,
            text,
            text_offset,
            style,
            z_index,
            bounding_rect,
            manually_hidden,
            scissor_rect_id,
        } = builder;

        let shared_state = Rc::new(RefCell::new(SharedState {
            inner: ToggleButtonInner::new(text, text_offset, toggled, &style, cx.font_system),
            style,
        }));

        let element_builder = ElementBuilder {
            element: Box::new(Self {
                shared_state: Rc::clone(&shared_state),
                action,
                tooltip_message,
                tooltip_align,
            }),
            z_index,
            bounding_rect,
            manually_hidden,
            scissor_rect_id,
        };

        let el = cx
            .view
            .add_element(element_builder, cx.font_system, cx.clipboard);

        ToggleButton { el, shared_state }
    }
}

impl<A: Clone + 'static> Element<A> for ToggleButtonElement<A> {
    fn flags(&self) -> ElementFlags {
        ElementFlags::PAINTS | ElementFlags::LISTENS_TO_POINTER_INSIDE_BOUNDS
    }

    fn on_event(
        &mut self,
        event: ElementEvent,
        cx: &mut ElementContext<'_, A>,
    ) -> EventCaptureStatus {
        match event {
            ElementEvent::CustomStateChanged => {
                cx.request_repaint();
            }
            ElementEvent::Pointer(PointerEvent::Moved { just_entered, .. }) => {
                let mut shared_state = RefCell::borrow_mut(&self.shared_state);
                let SharedState { inner, style } = &mut *shared_state;

                if inner.state == ButtonState::Disabled {
                    return EventCaptureStatus::NotCaptured;
                }

                cx.cursor_icon = CursorIcon::Pointer;

                if just_entered && self.tooltip_message.is_some() {
                    cx.start_hover_timeout();
                }

                if inner.state == ButtonState::Idle {
                    let res = inner.set_state(ButtonState::Hovered, style);

                    if res.needs_repaint {
                        cx.request_repaint();
                    }
                }

                return EventCaptureStatus::Captured;
            }
            ElementEvent::Pointer(PointerEvent::PointerLeft) => {
                let mut shared_state = RefCell::borrow_mut(&self.shared_state);
                let SharedState { inner, style, .. } = &mut *shared_state;

                if inner.state == ButtonState::Hovered || inner.state == ButtonState::Down {
                    let res = inner.set_state(ButtonState::Idle, style);

                    if res.needs_repaint {
                        cx.request_repaint();
                    }

                    return EventCaptureStatus::Captured;
                }
            }
            ElementEvent::Pointer(PointerEvent::ButtonJustPressed { button, .. }) => {
                let mut shared_state = RefCell::borrow_mut(&self.shared_state);
                let SharedState { inner, style, .. } = &mut *shared_state;

                if button == PointerButton::Primary
                    && (inner.state == ButtonState::Idle || inner.state == ButtonState::Hovered)
                {
                    let res1 = inner.set_state(ButtonState::Down, style);
                    let res2 = inner.set_toggled(!inner.toggled());

                    if res1.needs_repaint || res2.needs_repaint {
                        cx.request_repaint();
                    }

                    if let Some(action) = &mut self.action {
                        cx.send_action((action)(inner.toggled())).unwrap();
                    }

                    return EventCaptureStatus::Captured;
                }
            }
            ElementEvent::Pointer(PointerEvent::ButtonJustReleased {
                position, button, ..
            }) => {
                let mut shared_state = RefCell::borrow_mut(&self.shared_state);
                let SharedState { inner, style, .. } = &mut *shared_state;

                if button == PointerButton::Primary
                    && (inner.state == ButtonState::Down || inner.state == ButtonState::Hovered)
                {
                    let new_state = if cx.is_point_within_visible_bounds(position) {
                        ButtonState::Hovered
                    } else {
                        ButtonState::Idle
                    };

                    let res = inner.set_state(new_state, style);

                    if res.needs_repaint {
                        cx.request_repaint();
                    }

                    return EventCaptureStatus::Captured;
                }
            }
            ElementEvent::Pointer(PointerEvent::HoverTimeout { .. }) => {
                if let Some(message) = &self.tooltip_message {
                    cx.show_tooltip(ElementTooltipInfo {
                        message: message.clone(),
                        element_bounds: cx.rect(),
                        align: self.tooltip_align,
                    });
                }
            }
            _ => {}
        }

        EventCaptureStatus::NotCaptured
    }

    fn render_primitives(&mut self, cx: RenderContext<'_>, primitives: &mut PrimitiveGroup) {
        let mut shared_state = RefCell::borrow_mut(&self.shared_state);
        let SharedState { inner, style, .. } = &mut *shared_state;

        let label_primitives =
            inner.render_primitives(Rect::from_size(cx.bounds_size), style, cx.font_system);

        if let Some(quad_primitive) = label_primitives.bg_quad {
            primitives.add(quad_primitive);
        }

        if let Some(text_primitive) = label_primitives.text {
            primitives.set_z_index(1);
            primitives.add_text(text_primitive);
        }
    }
}

/// A handle to a [`ToggleButtonElement`].
pub struct ToggleButton {
    pub el: ElementHandle,
    shared_state: Rc<RefCell<SharedState>>,
}

struct SharedState {
    inner: ToggleButtonInner,
    style: Rc<ToggleButtonStyle>,
}

impl ToggleButton {
    pub fn builder<A: Clone + 'static>(style: &Rc<ToggleButtonStyle>) -> ToggleButtonBuilder<A> {
        ToggleButtonBuilder::new(style)
    }

    /// Returns the size of the padded background rectangle if it were to
    /// cover the entire size of the unclipped text.
    ///
    /// This can be useful to lay out elements that depend on text size.
    pub fn desired_padded_size(&self) -> Size {
        let mut shared_state = RefCell::borrow_mut(&self.shared_state);
        let SharedState { inner, style, .. } = &mut *shared_state;

        inner.desired_padded_size(style)
    }

    /// Returns the size of the unclipped text.
    ///
    /// This can be useful to lay out elements that depend on text size.
    pub fn unclipped_text_size(&self) -> Size {
        RefCell::borrow_mut(&self.shared_state)
            .inner
            .unclipped_text_size()
    }

    pub fn set_text(&mut self, text: &str, font_system: &mut FontSystem) {
        if RefCell::borrow_mut(&self.shared_state)
            .inner
            .set_text(text, font_system)
        {
            self.el.notify_custom_state_change();
        }
    }

    pub fn text<'a>(&'a self) -> Ref<'a, str> {
        Ref::map(RefCell::borrow(&self.shared_state), |s| s.inner.text())
    }

    pub fn set_style(&mut self, style: &Rc<ToggleButtonStyle>, font_system: &mut FontSystem) {
        let mut shared_state = RefCell::borrow_mut(&self.shared_state);

        if !Rc::ptr_eq(&shared_state.style, style) {
            shared_state.style = Rc::clone(style);
            shared_state.inner.set_style(style, font_system);
            self.el.notify_custom_state_change();
        }
    }

    pub fn style(&self) -> Rc<ToggleButtonStyle> {
        Rc::clone(&RefCell::borrow(&self.shared_state).style)
    }

    pub fn set_toggled(&mut self, toggled: bool) {
        let mut shared_state = RefCell::borrow_mut(&self.shared_state);

        if shared_state.inner.toggled() != toggled {
            shared_state.inner.set_toggled(toggled);
            self.el.notify_custom_state_change();
        }
    }

    pub fn toggled(&self) -> bool {
        RefCell::borrow(&self.shared_state).inner.toggled()
    }

    pub fn set_disabled(&mut self, disabled: bool) {
        let mut shared_state = RefCell::borrow_mut(&self.shared_state);
        let SharedState { inner, style, .. } = &mut *shared_state;

        if disabled && inner.state != ButtonState::Disabled {
            inner.set_state(ButtonState::Disabled, style);
            self.el.notify_custom_state_change();
        } else if !disabled && inner.state == ButtonState::Disabled {
            inner.set_state(ButtonState::Idle, style);
            self.el.notify_custom_state_change();
        }
    }

    /// An offset that can be used mainly to correct the position of icon glyphs.
    /// This does not effect the position of the background quad.
    pub fn set_text_offset(&mut self, offset: Point) {
        let changed = RefCell::borrow_mut(&self.shared_state)
            .inner
            .set_text_offset(offset);

        if changed {
            self.el.notify_custom_state_change();
        }
    }
}
