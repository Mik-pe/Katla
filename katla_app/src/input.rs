use std::collections::HashMap;
use winit::{
    event::{self, ElementState, WindowEvent},
    keyboard::{KeyCode, PhysicalKey},
};
pub mod map;
pub use map::*;

struct Modifier {
    code: KeyCode,
    state: ElementState,
    value: f32,
}

#[derive(Default)]
struct AxisHandler {
    axis: u32,
    current_value: f32,
    modifiers: Vec<Modifier>,
    callbacks: Vec<Box<dyn FnMut(f32)>>,
}

impl AxisHandler {
    pub fn add_callback(&mut self, callback: Box<dyn FnMut(f32)>) {
        self.callbacks.push(callback);
    }

    pub fn modifier_changed(&mut self, code: KeyCode, state: ElementState) {
        let mut new_value = 0.0;
        for modifier in &mut self.modifiers {
            if modifier.code == code {
                modifier.state = state;
            }
            if modifier.state == ElementState::Pressed {
                new_value += modifier.value;
            }
        }
        self.current_value = new_value;
        for callback in &mut self.callbacks {
            callback(new_value);
        }
    }
}

#[derive(Default)]
pub struct InputController {
    inputmap: HashMap<KeyCode, (u32, f32)>,
    axis_key_map: HashMap<KeyCode, u32>,
    axis_handlers: Vec<AxisHandler>,
    action_callbacks: HashMap<u32, Vec<Box<dyn FnMut(f32)>>>,
    keypressmap_callback: HashMap<KeyCode, Vec<Box<dyn FnMut(KeyCode, event::ElementState)>>>,
}

impl<'a> InputController {
    pub fn assign_axis_input(&mut self, key_event: KeyCode, input: u32, value: f32) {
        let axis_handler: &mut AxisHandler = {
            let mut axis_handler = None;
            for handler in &mut self.axis_handlers {
                if handler.axis == input {
                    axis_handler = Some(handler);
                }
            }
            if axis_handler.is_none() {
                self.axis_handlers.push(AxisHandler {
                    axis: input,
                    current_value: 0.0,
                    modifiers: vec![],
                    callbacks: vec![],
                });
                axis_handler = self.axis_handlers.last_mut();
            }
            axis_handler.unwrap()
        };

        let modifier = Modifier {
            code: key_event,
            state: ElementState::Released,
            value,
        };
        axis_handler.modifiers.push(modifier);
        self.axis_key_map.insert(key_event, input);
    }

    pub fn assign_axis_callback<T>(&mut self, input: T, callback: Box<dyn FnMut(f32)>)
    where
        T: Into<u32>,
    {
        let mut axis_handler = None;
        let input = input.into();
        for handler in &mut self.axis_handlers {
            if handler.axis == input {
                axis_handler = Some(handler);
                break;
            }
        }
        if let Some(axis_handler) = axis_handler {
            axis_handler.callbacks.push(callback);
        } else {
            println!("Tried to setup axis callback for non-bound input!");
        }
    }

    pub fn assign_action_input(&mut self, key: KeyCode, input: u32, value: f32) {
        self.inputmap.insert(key, (input, value));
    }

    fn handle_input(&mut self, code: &KeyCode, state: ElementState) {
        if let Some((key, value)) = self.inputmap.get(code) {
            if state == ElementState::Pressed {
                if let Some(callbacks) = self.action_callbacks.get_mut(key) {
                    for callback in callbacks {
                        callback(*value)
                    }
                }
            } else if let Some(callbacks) = self.action_callbacks.get_mut(key) {
                for callback in callbacks {
                    callback(0.0)
                }
            }
        }
    }
    fn handle_axis(&mut self, code: &KeyCode, state: ElementState) {
        if let Some(key) = self.axis_key_map.get(code) {
            for axis_handler in &mut self.axis_handlers {
                if axis_handler.axis == *key {
                    axis_handler.modifier_changed(*code, state);
                }
            }
        }
    }

    pub fn handle_event(&mut self, event: &event::WindowEvent) {
        if let WindowEvent::KeyboardInput {
                device_id: _,
                event,
                is_synthetic: _,
            } = event {
            if let PhysicalKey::Code(code) = event.physical_key {
                self.handle_input(&code, event.state);
                self.handle_axis(&code, event.state);
                if let Some(callbacks) = self.keypressmap_callback.get_mut(&code) {
                    for callback in callbacks {
                        callback(code, event.state);
                    }
                }
            }
        }
    }

    pub fn bind_input_callback(&mut self, input_key: u32, callback: Box<dyn FnMut(f32)>) {
        self.action_callbacks
            .entry(input_key)
            .or_default()
            .push(callback);
    }

    pub fn bind_keycode_callback(
        &mut self,
        keycode: KeyCode,
        callback: Box<dyn FnMut(KeyCode, event::ElementState)>,
    ) {
        self.keypressmap_callback
            .entry(keycode)
            .or_default()
            .push(callback);
    }
}
