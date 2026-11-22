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
    utils::{Logical, Rectangle, SERIAL_COUNTER},
};

use crate::state::AeroState;

/// Espaço de coordenadas do ponteiro absoluto do winit.
///
/// `smithay-0.7.0/src/backend/winit/mod.rs:482-490` normaliza a posição pela
/// janela host inteira (N tiles), mas `input.rs` transformava usando apenas
/// o primeiro output: com host 1280×480 e output 640×480 o ponteiro andava
/// pela metade e o output da direita ficava inalcançável.
fn absolute_pointer_space(
    geometries: impl IntoIterator<Item = Rectangle<i32, Logical>>,
) -> Option<Rectangle<i32, Logical>> {
    let mut iter = geometries.into_iter();
    let first = iter.next()?;
    Some(iter.fold(first, |acc, geo| {
        let x = acc.loc.x.min(geo.loc.x);
        let y = acc.loc.y.min(geo.loc.y);
        let right = (acc.loc.x + acc.size.w).max(geo.loc.x + geo.size.w);
        let bottom = (acc.loc.y + acc.size.h).max(geo.loc.y + geo.size.h);
        Rectangle::new((x, y).into(), (right - x, bottom - y).into())
    }))
}

impl AeroState {
    /// Injeção de ponteiro usada pelo backend headless de teste (S3).
    pub fn inject_motion(&mut self, x: f64, y: f64) {
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        let under = self.surface_under((x, y).into());
        self.cursor_over_interactive = under
            .as_ref()
            .map(|(s, _)| self.is_interactive_layer_surface(s))
            .unwrap_or(false);
        #[cfg(feature = "bench")]
        {
            let t_nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            eprintln!(
                "POINTER_MOTION serial={} t={t_nanos} x={x} y={y}",
                u32::from(serial)
            );
        }
        tracing::info!(x, y, under = ?under.as_ref().map(|(s, p)| (s.id(), p)), "TESTE ponteiro motion");
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
        if pressed {
            self.last_button_serial = Some(u32::from(serial));
            self.focus_on_press(serial);
        }
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

    /// Foco de teclado no clique: vai para a janela do space sob o ponteiro, ou
    /// None. O frame NUNCA recebe foco — não é janela do space e sua layer surface
    /// tem keyboard_interactivity None. Compartilhado pelo caminho real e sintético.
    fn focus_on_press(&mut self, serial: smithay::utils::Serial) {
        let pointer = self.seat.get_pointer().unwrap();
        let keyboard = self.seat.get_keyboard().unwrap();
        if pointer.is_grabbed() {
            return;
        }
        // A superfície sob o ponteiro é a fonte da verdade: `surface_under`
        // respeita layers/decoração e z-order. NUNCA usar `element_under` aqui:
        // ele ignora a titlebar (layer surface), então um clique na barra da
        // janela da frente sobre o conteúdo da de trás elevaria a de trás.
        let under = self.surface_under(pointer.current_location());
        let window = under.as_ref().and_then(|(surface, _)| {
            // Frame -> janela dona pelo window_id associado.
            if let Some((_, _, window_id, _)) = self
                .associated_frames
                .iter()
                .find(|(frame_surface, _, _, _)| frame_surface == surface)
            {
                return self.window_for_id(*window_id).cloned();
            }
            // Superfície de cliente -> Window correspondente no Space.
            self.space
                .elements()
                .find(|w| w.toplevel().map(|t| t.wl_surface()) == Some(surface))
                .cloned()
        });
        if let Some(window) = window {
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
                // O winit normaliza pelo tamanho da janela host; em N>1 isso
                // inclui todos os outputs, não apenas o primeiro.
                let geometries = self
                    .space
                    .outputs()
                    .filter_map(|output| self.space.output_geometry(output));
                let Some(space_geo) = absolute_pointer_space(geometries) else {
                    return;
                };
                let pos = event.position_transformed(space_geo.size) + space_geo.loc.to_f64();
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
                let serial = SERIAL_COUNTER.next_serial();
                let button_state = event.state();

                if ButtonState::Pressed == button_state {
                    self.last_button_serial = Some(u32::from(serial));
                    self.focus_on_press(serial);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ponteiro_absoluto_cobre_todos_os_outputs() {
        let geos = [
            Rectangle::<i32, Logical>::new((0, 0).into(), (640, 480).into()),
            Rectangle::<i32, Logical>::new((640, 0).into(), (640, 480).into()),
        ];
        let Some(space) = absolute_pointer_space(geos) else {
            panic!("dois outputs devem produzir geometria");
        };
        assert_eq!(
            (space.loc.x, space.loc.y, space.size.w, space.size.h),
            (0, 0, 1280, 480)
        );
    }
}
