//! Input nativo via Wayland. Nenhum evento é encaminhado para TS.

use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
        KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
    },
    input::{
        keyboard::FilterResult,
        pointer::{AxisFrame, ButtonEvent, MotionEvent},
    },
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource},
    utils::SERIAL_COUNTER,
};

use crate::state::AeroState;

impl AeroState {
    /// Injeção de ponteiro usada pelo backend headless de teste (S3).
    pub fn inject_motion(&mut self, x: f64, y: f64) {
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        let under = self.surface_under((x, y).into());
        self.cursor_over_interactive = under
            .as_ref()
            .map(|(s, _)| self.is_interactive_layer_surface(s))
            .unwrap_or(false);
        tracing::info!(x, y, under = ?under.as_ref().map(|(s, _)| s.id()), "TESTE ponteiro motion");
        let pointer = self.seat.get_pointer().unwrap();
        pointer.motion(
            self,
            under,
            &MotionEvent {
                location: (x, y).into(),
                serial,
                time: 0,
            },
        );
        pointer.frame(self);
    }

    pub fn inject_button(&mut self, pressed: bool) {
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        let pointer = self.seat.get_pointer().unwrap();
        tracing::info!(pressed, "TESTE ponteiro button");
        pointer.button(
            self,
            &ButtonEvent {
                button: 0x110,
                state: if pressed {
                    smithay::backend::input::ButtonState::Pressed
                } else {
                    smithay::backend::input::ButtonState::Released
                },
                serial,
                time: 0,
            },
        );
        pointer.frame(self);
    }

    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time_msec(&event);
                self.seat.get_keyboard().unwrap().input::<(), _>(
                    self,
                    event.key_code(),
                    event.state(),
                    serial,
                    time,
                    |_, _, _| FilterResult::Forward,
                );
            }
            InputEvent::PointerMotion { .. } => {}
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let Some(output) = self.space.outputs().next() else {
                    return;
                };
                let Some(output_geo) = self.space.output_geometry(output) else {
                    return;
                };
                let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();
                let serial = SERIAL_COUNTER.next_serial();
                let pointer = self.seat.get_pointer().unwrap();
                let under = self.surface_under(pos);
                self.cursor_over_interactive = under
                    .as_ref()
                    .map(|(s, _)| self.is_interactive_layer_surface(s))
                    .unwrap_or(false);
                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: pos,
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerButton { event, .. } => {
                let pointer = self.seat.get_pointer().unwrap();
                let keyboard = self.seat.get_keyboard().unwrap();
                let serial = SERIAL_COUNTER.next_serial();
                let button_state = event.state();

                if ButtonState::Pressed == button_state && !pointer.is_grabbed() {
                    if let Some((window, _)) = self
                        .space
                        .element_under(pointer.current_location())
                        .map(|(w, l)| (w.clone(), l))
                    {
                        self.space.raise_element(&window, true);
                        if let Some(toplevel) = window.toplevel() {
                            keyboard.set_focus(self, Some(toplevel.wl_surface().clone()), serial);
                        }
                        self.space.elements().for_each(|w| {
                            if let Some(t) = w.toplevel() {
                                t.send_pending_configure();
                            }
                        });
                    } else {
                        keyboard.set_focus(self, Option::<WlSurface>::None, serial);
                    }
                }

                pointer.button(
                    self,
                    &ButtonEvent {
                        button: event.button_code(),
                        state: button_state,
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerAxis { event, .. } => {
                let source = event.source();
                let horizontal_amount = event.amount(Axis::Horizontal).unwrap_or_else(|| {
                    event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.0
                });
                let vertical_amount = event.amount(Axis::Vertical).unwrap_or_else(|| {
                    event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.0
                });
                let horizontal_amount_discrete = event.amount_v120(Axis::Horizontal);
                let vertical_amount_discrete = event.amount_v120(Axis::Vertical);

                let mut frame = AxisFrame::new(event.time_msec()).source(source);
                if horizontal_amount != 0.0 {
                    frame = frame.value(Axis::Horizontal, horizontal_amount);
                    if let Some(discrete) = horizontal_amount_discrete {
                        frame = frame.v120(Axis::Horizontal, discrete as i32);
                    }
                }
                if vertical_amount != 0.0 {
                    frame = frame.value(Axis::Vertical, vertical_amount);
                    if let Some(discrete) = vertical_amount_discrete {
                        frame = frame.v120(Axis::Vertical, discrete as i32);
                    }
                }
                if source == AxisSource::Finger {
                    if event.amount(Axis::Horizontal) == Some(0.0) {
                        frame = frame.stop(Axis::Horizontal);
                    }
                    if event.amount(Axis::Vertical) == Some(0.0) {
                        frame = frame.stop(Axis::Vertical);
                    }
                }
                let pointer = self.seat.get_pointer().unwrap();
                pointer.axis(self, frame);
                pointer.frame(self);
            }
            _ => {}
        }
    }
}
