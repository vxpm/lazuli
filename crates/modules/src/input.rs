use std::sync::{Arc, Mutex};

use gilrs::{Axis, Button, GamepadId, Gilrs};
use lazuli::modules::input::{ControllerState, InputModule};

struct GilrsInner {
    gilrs: Gilrs,
    active_gamepad: Option<GamepadId>,
    fallback_state: ControllerState,
}

impl Default for GilrsInner {
    fn default() -> Self {
        Self::new()
    }
}

impl GilrsInner {
    pub fn new() -> Self {
        let gilrs = Gilrs::new().unwrap();
        let active_gamepad = gilrs.gamepads().next().map(|g| g.0);

        Self {
            gilrs,
            active_gamepad,
            fallback_state: Default::default(),
        }
    }

    fn process_events(&mut self) {
        while let Some(event) = self.gilrs.next_event() {
            if self.active_gamepad.is_none() {
                self.active_gamepad = Some(event.id);
            }
        }
    }

    fn get_state(&mut self) -> ControllerState {
        let Some(gamepad_id) = self.active_gamepad else {
            return self.fallback_state;
        };

        let Some(gamepad) = self.gilrs.connected_gamepad(gamepad_id) else {
            self.active_gamepad = None;
            return self.fallback_state;
        };

        let axis = |axis| (255.0 * ((gamepad.value(axis) + 1.0) / 2.0)) as u8;
        let trigger =
            |button| (255.0 * gamepad.button_data(button).map_or(0.0, |v| v.value())) as u8;

        ControllerState {
            analog_x: axis(Axis::LeftStickX),
            analog_y: axis(Axis::LeftStickY),
            analog_sub_x: axis(Axis::RightStickX),
            analog_sub_y: axis(Axis::RightStickY),
            analog_trigger_left: trigger(Button::LeftTrigger2),
            analog_trigger_right: trigger(Button::RightTrigger2),
            trigger_z: gamepad.is_pressed(Button::LeftTrigger)
                || gamepad.is_pressed(Button::RightTrigger),
            trigger_right: gamepad.is_pressed(Button::RightTrigger2),
            trigger_left: gamepad.is_pressed(Button::LeftTrigger2),
            pad_left: gamepad.is_pressed(Button::DPadLeft),
            pad_right: gamepad.is_pressed(Button::DPadRight),
            pad_down: gamepad.is_pressed(Button::DPadDown),
            pad_up: gamepad.is_pressed(Button::DPadUp),
            button_a: gamepad.is_pressed(Button::South),
            button_b: gamepad.is_pressed(Button::East),
            button_x: gamepad.is_pressed(Button::West),
            button_y: gamepad.is_pressed(Button::North),
            button_start: gamepad.is_pressed(Button::Start),
        }
    }
}

/// This type is internally reference-counted.
#[derive(Clone)]
pub struct GilrsModule(Arc<Mutex<GilrsInner>>);

impl Default for GilrsModule {
    fn default() -> Self {
        Self::new()
    }
}

impl GilrsModule {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(GilrsInner::new())))
    }

    pub fn update_fallback(&mut self, f: impl FnOnce(&mut ControllerState)) {
        let mut inner = self.0.lock().unwrap();
        f(&mut inner.fallback_state);
    }
}

impl InputModule for GilrsModule {
    fn controller(&mut self, index: usize) -> Option<ControllerState> {
        let mut inner = self.0.lock().unwrap();
        inner.process_events();

        if index != 0 {
            return None;
        }

        Some(inner.get_state())
    }
}
