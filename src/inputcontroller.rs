use std::rc::Rc;
use std::{cell::RefCell, collections::HashMap};
use winit::event::{self, ElementState, VirtualKeyCode};

struct Modifier {
    code: VirtualKeyCode,
    state: ElementState,
    value: f32,
}

#[derive(Default)]
struct AxisHandler {
    axis: String,
    current_value: f32,
    modifiers: Vec<Modifier>,
    callbacks: Vec<Box<dyn FnMut(f32)>>,
}

impl AxisHandler {
    pub fn add_callback(&mut self, callback: Box<dyn FnMut(f32)>) {
        self.callbacks.push(callback);
    }

    pub fn modifier_changed(&mut self, code: VirtualKeyCode, state: ElementState) {
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
pub struct InputController {
    inputmap: HashMap<event::VirtualKeyCode, (String, f32)>,
    axis_key_map: HashMap<event::VirtualKeyCode, String>,
    axis_handlers: Vec<AxisHandler>,
    action_callbacks: HashMap<String, Vec<Box<dyn FnMut(f32)>>>,
    keypressmap_callback: HashMap<
        event::VirtualKeyCode,
        Vec<Box<dyn FnMut(event::VirtualKeyCode, event::ElementState)>>,
    >,
}

impl<'a> InputController {
    pub fn new() -> Self {
        Self {
            inputmap: HashMap::new(),
            axis_key_map: HashMap::new(),
            axis_handlers: vec![],
            action_callbacks: HashMap::new(),
            keypressmap_callback: HashMap::new(),
        }
    }

    pub fn assign_axis_input(
        &mut self,
        key_event: event::VirtualKeyCode,
        input: String,
        value: f32,
    ) {
        let axis_handler: &mut AxisHandler = {
            let mut axis_handler = None;
            for handler in &mut self.axis_handlers {
                if handler.axis == input {
                    axis_handler = Some(handler);
                }
            }
            if axis_handler.is_none() {
                self.axis_handlers.push(AxisHandler {
                    axis: input.clone(),
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
            value: value,
        };
        axis_handler.modifiers.push(modifier);
        self.axis_key_map.insert(key_event, input);
    }

    pub fn assign_axis_callback(&mut self, input: String, callback: Box<dyn FnMut(f32)>) {
        let mut axis_handler = None;
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

    pub fn assign_action_input(
        &mut self,
        key_event: event::VirtualKeyCode,
        input: String,
        value: f32,
    ) {
        self.inputmap.insert(key_event, (input, value));
    }

    fn handle_input(&mut self, code: &VirtualKeyCode, state: ElementState) {
        if let Some((key, value)) = self.inputmap.get(&code) {
            if state == ElementState::Pressed {
                if let Some(callbacks) = self.action_callbacks.get_mut(key) {
                    for callback in callbacks {
                        callback(*value)
                    }
                }
            } else {
                if let Some(callbacks) = self.action_callbacks.get_mut(key) {
                    for callback in callbacks {
                        callback(0.0)
                    }
                }
            }
        }
    }
    fn handle_axis(&mut self, code: &VirtualKeyCode, state: ElementState) {
        if let Some(key) = self.axis_key_map.get(&code) {
            for axis_handler in &mut self.axis_handlers {
                if axis_handler.axis == *key {
                    axis_handler.modifier_changed(*code, state);
                }
            }
        }
    }

    pub fn handle_event(&mut self, event: &event::WindowEvent) {
        match event {
            event::WindowEvent::KeyboardInput {
                device_id: _,
                input,
                is_synthetic: _,
            } => match input.virtual_keycode {
                Some(code) => {
                    self.handle_input(&code, input.state);
                    self.handle_axis(&code, input.state);
                    if let Some(callbacks) = self.keypressmap_callback.get_mut(&code) {
                        for callback in callbacks {
                            callback(code, input.state);
                        }
                    }
                }
                None => {}
            },
            _ => {}
        }
    }

    pub fn bind_input_callback(&mut self, input_key: String, callback: Box<dyn FnMut(f32)>) {
        self.action_callbacks
            .entry(input_key)
            .or_default()
            .push(callback);
    }

    pub fn bind_keycode_callback(
        &mut self,
        keycode: event::VirtualKeyCode,
        callback: Box<dyn FnMut(event::VirtualKeyCode, event::ElementState)>,
    ) {
        self.keypressmap_callback
            .entry(keycode)
            .or_default()
            .push(callback);
    }
}
