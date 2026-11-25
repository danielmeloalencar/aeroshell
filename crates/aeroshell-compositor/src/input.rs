//! Input nativo via Wayland. Nenhum evento é encaminhado para TS.

use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
        KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
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
        self.dirty = true;
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
        self.dirty = true;
        if !pressed {
            self.redraw_until =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(500));
        }
        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        if pressed {
            self.last_button_serial = Some(u32::from(serial));
            self.focus_on_press(serial);
        }
        let pointer = self.seat.get_pointer().unwrap();
        #[cfg(feature = "bench")]
        eprintln!(
            "BUTTON_FOCUS t={} serial={} button=0x110 pressed={} focus={:?} grabbed={}",
            crate::frame_diagnostics::now_ns(),
            u32::from(serial),
            pressed,
            pointer.current_focus().map(|s| s.id()),
            pointer.is_grabbed()
        );
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

    /// Texto sintético para o harness do launcher. Usa o mesmo KeyboardHandle
    /// do input físico; keycodes XKB = evdev + 8 (convenção já usada no udev).
    pub fn inject_text(&mut self, text: &str) {
        for ch in text.chars() {
            let evdev = match ch {
                'a' => 30,
                'b' => 48,
                'c' => 46,
                'd' => 32,
                'e' => 18,
                'f' => 33,
                'g' => 34,
                'h' => 35,
                'i' => 23,
                'j' => 36,
                'k' => 37,
                'l' => 38,
                'm' => 50,
                'n' => 49,
                'o' => 24,
                'p' => 25,
                'q' => 16,
                'r' => 19,
                's' => 31,
                't' => 20,
                'u' => 22,
                'v' => 47,
                'w' => 17,
                'x' => 45,
                'y' => 21,
                'z' => 44,
                ' ' => 57,
                '\u{8}' => 14, // BackSpace: usado para limpar a query no harness.
                _ => continue,
            };
            let keycode = smithay::input::keyboard::Keycode::new(evdev + 8);
            for state in [
                smithay::backend::input::KeyState::Pressed,
                smithay::backend::input::KeyState::Released,
            ] {
                self.seat.get_keyboard().unwrap().input::<(), _>(
                    self,
                    keycode,
                    state,
                    SERIAL_COUNTER.next_serial(),
                    0,
                    |_, _, _| FilterResult::Forward,
                );
            }
        }
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
        // Painéis que declararam keyboard_interactivity OnDemand/Exclusive são
        // clientes de texto reais (launcher, renomear workspace). Frames não
        // entram em layer_surfaces e continuam sem receber foco.
        let layer_focus = under.as_ref().and_then(|(surface, _)| {
            self.layer_surfaces
                .iter()
                .any(|layer| layer.wl_surface() == surface && layer.can_receive_keyboard_focus())
                .then(|| surface.clone())
        });
        if let Some(surface) = layer_focus {
            keyboard.set_focus(self, Some(surface), serial);
            return;
        }
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
            InputEvent::PointerMotion { event, .. } => {
                // Libinput (backend udev) emite movimento RELATIVO; os demais
                // backends usam absoluto/injeção sintética e não caem aqui.
                // F1d3: sem este braço o ponteiro não anda em sessão real.
                let serial = SERIAL_COUNTER.next_serial();
                let pointer = self.seat.get_pointer().unwrap();
                let current = pointer.current_location();
                let delta = event.delta();
                let mut pos = smithay::utils::Point::<f64, Logical>::new(
                    current.x + delta.x,
                    current.y + delta.y,
                );
                if let Some(space) = absolute_pointer_space(
                    self.space
                        .outputs()
                        .filter_map(|output| self.space.output_geometry(output)),
                ) {
                    pos.x = pos
                        .x
                        .clamp(space.loc.x as f64, (space.loc.x + space.size.w) as f64);
                    pos.y = pos
                        .y
                        .clamp(space.loc.y as f64, (space.loc.y + space.size.h) as f64);
                }
                let under = self.surface_under(pos);
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
                #[cfg(feature = "bench")]
                eprintln!(
                    "BUTTON_FOCUS t={} serial={} button={:#x} pressed={} focus={:?} grabbed={}",
                    crate::frame_diagnostics::now_ns(),
                    u32::from(serial),
                    event.button_code(),
                    ButtonState::Pressed == button_state,
                    pointer.current_focus().map(|s| s.id()),
                    pointer.is_grabbed()
                );

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
