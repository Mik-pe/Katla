//! Non-editor stub implementations for Application methods.
//!
//! All methods in this file are gated behind `#[cfg(not(feature = "editor"))]`.
//! Editor implementations live in `editor_methods.rs`.

use super::Application;
use katla_math::Vec2;

impl Application {
    pub(crate) fn on_viewport_texture_recreated(&mut self, _slot: u32) {}

    pub(crate) fn filter_scroll_for_editor(&self, wheel_y: f32) -> f32 {
        wheel_y
    }

    pub(crate) fn should_track_mouse_motion(&self) -> bool {
        true
    }

    pub(crate) fn should_send_game_input(&self) -> bool {
        true
    }

    pub(crate) fn on_cursor_moved(&mut self, _mouse_pos: Vec2) {}

    pub(crate) fn on_mouse_input(
        &mut self,
        _state: &winit::event::ElementState,
        _button: &winit::event::MouseButton,
    ) {
    }

    pub(crate) fn on_keyboard_input(
        &mut self,
        _event: &winit::event::KeyEvent,
        _keycode: winit::keyboard::KeyCode,
        _event_loop: &winit::event_loop::ActiveEventLoop,
    ) {
    }

    pub(crate) fn attach_billboard_icon(
        &mut self,
        _entity_id: katla_ecs::EntityId,
        _icon: crate::components::billboard::BillboardIcon,
    ) {
    }

    pub(crate) fn save_editor_state(&mut self) {}

    pub(crate) fn poll_background_loader(&mut self) {}

    pub(crate) fn render_editor_frame(&mut self, dt: f32) {
        log::debug!("Rendering frame...");
        self.render_frame(None, dt, self.frame_count);
        log::debug!("Frame rendered");
    }
}
