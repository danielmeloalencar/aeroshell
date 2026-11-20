//! Grabs interativos de move/resize disparados pelo frame (aero_frame_v1).
//! O frame envia `move`/`resize` com serial de clique válido; o compositor
//! inicia um grab de ponteiro que move/redimensiona a JANELA CLIENTE. O frame
//! segue o conteúdo via configure (ADR 0004). Teclado nunca vai para o frame.

use smithay::{
    desktop::Window,
    input::pointer::{
        AxisFrame, ButtonEvent, Focus, GestureHoldBeginEvent, GestureHoldEndEvent,
        GesturePinchBeginEvent, GesturePinchEndEvent, GesturePinchUpdateEvent,
        GestureSwipeBeginEvent, GestureSwipeEndEvent, GestureSwipeUpdateEvent, GrabStartData,
        MotionEvent, PointerGrab, PointerInnerHandle, RelativeMotionEvent,
    },
    utils::{Logical, Point, Size},
};

use crate::{aero_frame_protocol::aero_frame_v1::ResizeEdge, state::AeroState};

const MIN_W: i32 = 120;
const MIN_H: i32 = 80;

pub struct MoveGrab {
    window: Window,
    start_data: GrabStartData<AeroState>,
    initial_loc: Point<i32, Logical>,
}

impl MoveGrab {
    pub fn new(
        window: Window,
        start_data: GrabStartData<AeroState>,
        initial_loc: Point<i32, Logical>,
    ) -> Self {
        Self {
            window,
            start_data,
            initial_loc,
        }
    }
}

impl PointerGrab<AeroState> for MoveGrab {
    fn motion(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        _focus: Option<(
            <AeroState as smithay::input::SeatHandler>::PointerFocus,
            Point<f64, Logical>,
        )>,
        event: &MotionEvent,
    ) {
        let delta = event.location - self.start_data.location;
        let new_loc = self.initial_loc + delta.to_i32_round();
        data.space.map_element(self.window.clone(), new_loc, false);
        handle.motion(data, None, event);
    }

    fn relative_motion(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        focus: Option<(
            <AeroState as smithay::input::SeatHandler>::PointerFocus,
            Point<f64, Logical>,
        )>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);
        if event.state == smithay::backend::input::ButtonState::Released
            && event.button == self.start_data.button
        {
            handle.unset_grab(self, data, event.serial, event.time, true);
        }
    }

    fn axis(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        details: AxisFrame,
    ) {
        handle.axis(data, details);
    }

    fn frame(&mut self, data: &mut AeroState, handle: &mut PointerInnerHandle<'_, AeroState>) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(data, event);
    }

    fn gesture_swipe_update(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(data, event);
    }

    fn gesture_swipe_end(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(data, event);
    }

    fn gesture_pinch_begin(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(data, event);
    }

    fn gesture_pinch_update(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(data, event);
    }

    fn gesture_pinch_end(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(data, event);
    }

    fn gesture_hold_begin(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(data, event);
    }

    fn gesture_hold_end(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(data, event);
    }

    fn start_data(&self) -> &GrabStartData<AeroState> {
        &self.start_data
    }

    fn unset(&mut self, _data: &mut AeroState) {}
}

pub struct ResizeGrab {
    window: Window,
    start_data: GrabStartData<AeroState>,
    edges: ResizeEdge,
    initial_loc: Point<i32, Logical>,
    initial_size: Size<i32, Logical>,
}

impl ResizeGrab {
    pub fn new(
        window: Window,
        start_data: GrabStartData<AeroState>,
        edges: ResizeEdge,
        initial_loc: Point<i32, Logical>,
        initial_size: Size<i32, Logical>,
    ) -> Self {
        Self {
            window,
            start_data,
            edges,
            initial_loc,
            initial_size,
        }
    }
}

impl PointerGrab<AeroState> for ResizeGrab {
    fn motion(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        _focus: Option<(
            <AeroState as smithay::input::SeatHandler>::PointerFocus,
            Point<f64, Logical>,
        )>,
        event: &MotionEvent,
    ) {
        let delta = event.location - self.start_data.location;
        let dx = delta.x as i32;
        let dy = delta.y as i32;
        let (top, bottom, left, right) = match self.edges {
            ResizeEdge::Top => (true, false, false, false),
            ResizeEdge::Bottom => (false, true, false, false),
            ResizeEdge::Left => (false, false, true, false),
            ResizeEdge::Right => (false, false, false, true),
            ResizeEdge::TopLeft => (true, false, true, false),
            ResizeEdge::BottomLeft => (false, true, true, false),
            ResizeEdge::TopRight => (true, false, false, true),
            ResizeEdge::BottomRight => (false, true, false, true),
            _ => (false, false, false, false),
        };
        let mut size = self.initial_size;
        let mut loc = self.initial_loc;
        if right {
            size.w = (self.initial_size.w + dx).max(MIN_W);
        }
        if left {
            size.w = (self.initial_size.w - dx).max(MIN_W);
            loc.x = self.initial_loc.x + (self.initial_size.w - size.w);
        }
        if bottom {
            size.h = (self.initial_size.h + dy).max(MIN_H);
        }
        if top {
            size.h = (self.initial_size.h - dy).max(MIN_H);
            loc.y = self.initial_loc.y + (self.initial_size.h - size.h);
        }
        if let Some(toplevel) = self.window.toplevel() {
            toplevel.with_pending_state(|state| state.size = Some(size));
            toplevel.send_configure();
        }
        data.space.map_element(self.window.clone(), loc, false);
        handle.motion(data, None, event);
    }

    fn relative_motion(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        focus: Option<(
            <AeroState as smithay::input::SeatHandler>::PointerFocus,
            Point<f64, Logical>,
        )>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);
        if event.state == smithay::backend::input::ButtonState::Released
            && event.button == self.start_data.button
        {
            handle.unset_grab(self, data, event.serial, event.time, true);
        }
    }

    fn axis(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        details: AxisFrame,
    ) {
        handle.axis(data, details);
    }

    fn frame(&mut self, data: &mut AeroState, handle: &mut PointerInnerHandle<'_, AeroState>) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(data, event);
    }

    fn gesture_swipe_update(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(data, event);
    }

    fn gesture_swipe_end(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(data, event);
    }

    fn gesture_pinch_begin(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(data, event);
    }

    fn gesture_pinch_update(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(data, event);
    }

    fn gesture_pinch_end(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(data, event);
    }

    fn gesture_hold_begin(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(data, event);
    }

    fn gesture_hold_end(
        &mut self,
        data: &mut AeroState,
        handle: &mut PointerInnerHandle<'_, AeroState>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(data, event);
    }

    fn start_data(&self) -> &GrabStartData<AeroState> {
        &self.start_data
    }

    fn unset(&mut self, _data: &mut AeroState) {}
}

/// Inicia o grab de move/resize para a janela cliente associada ao window_id.
/// Retorna false se a janela não existe ou o serial não casa com um clique.
pub fn start_geometry_grab(
    state: &mut AeroState,
    window_id: u32,
    serial: u32,
    resize: Option<ResizeEdge>,
) -> bool {
    if !crate::protocols::aero_frame::serial_valid(state.last_button_serial, serial) {
        return false;
    }
    // window_id é lógico e estável; `elements().nth()` é ordem z e foi reordenado
    // pelo raise_element do clique que iniciou este grab.
    let Some(window) = state.window_for_id(window_id).cloned() else {
        return false;
    };
    let Some(loc) = state.space.element_location(&window) else {
        return false;
    };
    let pointer = state.seat.get_pointer().unwrap();
    let start_data = GrabStartData {
        focus: None,
        button: 0x110,
        location: pointer.current_location(),
    };
    let serial = smithay::utils::SERIAL_COUNTER.next_serial();
    match resize {
        None => {
            let grab = MoveGrab::new(window, start_data, loc);
            pointer.set_grab(state, grab, serial, Focus::Clear);
        }
        Some(edges) => {
            let size = state
                .space
                .element_geometry(&window)
                .map(|g| g.size)
                .unwrap_or((320, 240).into());
            let grab = ResizeGrab::new(window, start_data, edges, loc, size);
            pointer.set_grab(state, grab, serial, Focus::Clear);
        }
    }
    true
}
