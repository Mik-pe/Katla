//! Application module - main application lifecycle and event handling.
//!
//! This module contains the main [`Application`] struct and its implementation
//! of [`ApplicationHandler`] for winit event handling. The heavy lifting is
//! delegated to submodules:
//!
//! - [`builder`] - Application builder pattern
//! - [`renderer`] - Render graph setup and frame rendering
//! - [`editor`] - UI rendering and entity management for the editor

pub mod builder;
pub mod editor;
mod renderer;

use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc, time::Instant};

use log::{debug, info, warn};
use winit::keyboard::ModifiersState;

pub use builder::*;
use katla_ecs::{input::Action, World};
use katla_gfx::renderer::VulkanRenderer;
use katla_math::Vec2;
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, ElementState, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use crate::{
    entities::Camera,
    gui_state::GuiState,
    input::{InputBinding, InputMapper, KeyCombo, MouseCombo},
    preferences::Preferences,
    resources::ResourceManager,
    util::{BackgroundLoader, FileCache, GLTFModel, Timer},
};

struct ApplicationInfo {
    name: String,
    validation_layer_enabled: bool,
    max_frames: Option<usize>, // Some(n) = exit after n frames, None = run indefinitely
}

/// Main application struct containing all engine state.
pub struct Application {
    pub(crate) window: Window,
    pub(crate) renderer: VulkanRenderer,
    pub(crate) camera: Rc<RefCell<Camera>>,
    pub(crate) gltf_cache: FileCache<GLTFModel, Box<dyn Fn(&PathBuf) -> GLTFModel>>,
    pub(crate) timer: Timer,
    pub(crate) info: ApplicationInfo,
    pub(crate) world: World,
    pub(crate) input_mapper: InputMapper,
    pub(crate) current_modifiers: ModifiersState,
    pub(crate) frame_count: usize,
    pub(crate) resources: ResourceManager,
    /// Immediate mode UI context
    pub(crate) ui_context: katla_ui::UiContext,
    /// Debug overlay UI (simplified stats)
    pub(crate) debug_overlay: crate::ui::DebugOverlay,
    /// Game engine editor UI
    pub(crate) editor_ui: crate::ui::EditorUI,
    /// Use editor UI mode (vs simple debug overlay)
    pub(crate) use_editor_ui: bool,
    /// User preferences (theme, settings)
    pub(crate) preferences: Preferences,
    /// GUI layout state (panel sizes, positions)
    pub(crate) gui_state: GuiState,
    /// DPI scale factor (physical pixels per logical pixel)
    pub(crate) scale_factor: f32,
    /// Background asset loader thread
    pub(crate) background_loader: BackgroundLoader,
    /// Mapping of thumbnail paths to their uploaded texture handles
    pub(crate) thumbnail_texture_handles: HashMap<PathBuf, katla_gfx::TextureHandle>,
    /// Application start time for double-click timestamp calculation
    pub(crate) start_time: Instant,
}

impl ApplicationHandler for Application {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        // Enable IME for text input (required for receiving text input events)
        self.window.set_ime_allowed(true);

        // Get initial DPI scale factor
        self.scale_factor = self.window.scale_factor() as f32;
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event {
            if self.world.get_input().is_action_pressed(Action::LookEnable) {
                let current_delta = self.world.get_input().mouse_delta;
                self.world.get_input_mut().mouse_delta = Vec2::new(
                    current_delta.x() + delta.0 as f32,
                    current_delta.y() + delta.1 as f32,
                );
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if let WindowEvent::MouseInput { state, button, .. } = &event {
            let mouse_combo = MouseCombo::with_modifiers(*button, self.current_modifiers);
            let binding = InputBinding::Mouse(mouse_combo);

            if let Some(action) = self.input_mapper.get_action(&binding) {
                // Only send mouse input to game when viewport is focused
                if self.editor_ui.focused_panel == crate::ui::FocusedPanel::Viewport {
                    let pressed = matches!(state, ElementState::Pressed);
                    self.world.get_input_mut().set_action_state(action, pressed);
                }
            }

            let ui_button = match button {
                winit::event::MouseButton::Left => Some(katla_ui::input::mouse_button::LEFT),
                winit::event::MouseButton::Right => Some(katla_ui::input::mouse_button::RIGHT),
                winit::event::MouseButton::Middle => Some(katla_ui::input::mouse_button::MIDDLE),
                _ => None,
            };
            if let Some(btn) = ui_button {
                let pressed = matches!(state, ElementState::Pressed);
                let time = self.start_time.elapsed().as_secs_f64();
                self.ui_context
                    .input
                    .set_mouse_button_with_time(btn, pressed, time);
            }
        }

        match event {
            WindowEvent::Resized(logical_size) => {
                let new_width = logical_size.width;
                let new_height = logical_size.height as f32;

                if new_width > 0 && new_height > 0.0 {
                    // Wait for GPU to finish before destroying old resources
                    self.renderer.wait_for_device();
                    self.renderer.recreate_swapchain();

                    let extent = self.renderer.swapchain_extent();
                    let aspect = extent.width as f32 / extent.height as f32;
                    self.camera
                        .borrow_mut()
                        .aspect_ratio_changed(&mut self.world, aspect);
                    info!("=== Resize complete ===");
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Convert physical pixels to logical pixels for UI
                let logical_x = position.x as f32 / self.scale_factor;
                let logical_y = position.y as f32 / self.scale_factor;
                self.ui_context
                    .input
                    .set_mouse_pos(Vec2::new(logical_x, logical_y));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => {
                        Vec2::new(x * 20.0, y * 20.0)
                    }
                    winit::event::MouseScrollDelta::PixelDelta(pos) => {
                        // Convert physical to logical pixels
                        Vec2::new(
                            pos.x as f32 / self.scale_factor,
                            pos.y as f32 / self.scale_factor,
                        )
                    }
                };
                self.ui_context.input.scroll_delta = scroll;
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(keycode) = event.physical_key {
                    let key_combo = KeyCombo::with_modifiers(keycode, self.current_modifiers);
                    let binding = InputBinding::Keyboard(key_combo);

                    if let Some(action) = self.input_mapper.get_action(&binding) {
                        // Only send keyboard input to game when viewport is focused
                        if self.editor_ui.focused_panel == crate::ui::FocusedPanel::Viewport {
                            let pressed = matches!(event.state, ElementState::Pressed);
                            self.world.get_input_mut().set_action_state(action, pressed);
                        }
                    }

                    let ui_key = Self::winit_to_ui_key(keycode);
                    if let Some(key) = ui_key {
                        match event.state {
                            ElementState::Pressed => self.ui_context.input.add_key_press(key),
                            ElementState::Released => self.ui_context.input.add_key_release(key),
                        }
                    }

                    // Handle text input from key event (for UI text fields)
                    if event.state == ElementState::Pressed {
                        if let Some(text) = &event.text {
                            for c in text.chars() {
                                self.ui_context.input.add_char(c);
                            }
                        }
                    }

                    if event.state == ElementState::Pressed {
                        // Only process game-specific keys when viewport is focused
                        if self.editor_ui.focused_panel == crate::ui::FocusedPanel::Viewport {
                            if keycode == KeyCode::Escape {
                                event_loop.exit()
                            }
                        }
                    }
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.current_modifiers = modifiers.state();
            }
            WindowEvent::Ime(event) => {
                // Handle text input for UI widgets (text fields, search filters, etc.)
                if let winit::event::Ime::Preedit(_, _) | winit::event::Ime::Commit(_) = event {
                    // For Commit events, add each character to the UI input
                    if let winit::event::Ime::Commit(text) = event {
                        for c in text.chars() {
                            self.ui_context.input.add_char(c);
                        }
                    }
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_factor = scale_factor as f32;
                debug!("DPI scale factor changed to {}", self.scale_factor);
            }
            WindowEvent::RedrawRequested => {
                debug!("RedrawRequested received");
                self.timer.add_timestamp();
                let dt = self.timer.get_delta() as f32;

                // Update world (runs animation systems)
                debug!("Updating world...");
                self.world.update(dt);
                debug!("World updated");

                // Poll background loader for completed asset loads
                self.poll_background_loader();

                // Render debug UI overlay
                debug!("Rendering UI...");
                editor::render_debug_ui(self, dt);
                debug!("UI rendered");

                // Render frame to GPU
                debug!("Rendering frame...");
                self.render_frame();
                debug!("Frame rendered");

                // Handle max_frames limit
                if let Some(max) = self.info.max_frames {
                    self.frame_count += 1;
                    if self.frame_count >= max {
                        info!("Rendered {} frames, exiting", self.frame_count);
                        // Call cleanup directly since exiting() may not be triggered
                        self.cleanup_on_exit();
                        event_loop.exit();
                    }
                }

                self.window.request_redraw();
            }
            _ => {}
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        info!("Application exiting - cleaning up...");
        self.cleanup_on_exit();
    }
}

impl Application {
    /// Cleanup resources on exit.
    /// Called both from exiting() and directly before event_loop.exit() for max_frames mode.
    fn cleanup_on_exit(&mut self) {
        // Save preferences before exit
        if let Err(e) = self.preferences.save() {
            warn!("Failed to save preferences: {}", e);
        } else {
            info!("Saved preferences to disk");
        }

        // Save GUI state before exit
        self.gui_state.left_panel_width = self.editor_ui.left_panel_width;
        self.gui_state.right_panel_width = self.editor_ui.right_panel_width;
        self.gui_state.asset_browser_height = self.editor_ui.asset_browser.panel_height;

        if let Err(e) = self.gui_state.save() {
            warn!("Failed to save GUI state: {}", e);
        } else {
            info!("Saved GUI state to disk");
        }

        self.renderer.wait_for_device();

        self.renderer.destroy();
    }
}

impl Application {
    pub fn init(&mut self) {
        // Logger is now initialized in main() before building the application
    }

    /// Poll the background loader and process completed loads.
    fn poll_background_loader(&mut self) {
        use crate::ui::ThumbnailState;
        use crate::util::LoadResult;

        let results = self.background_loader.poll();

        for result in results {
            match result {
                LoadResult::ImageThumbnailLoaded {
                    path,
                    width,
                    height,
                    pixels: _,
                    ..
                } => {
                    debug!("Thumbnail loaded: {:?} ({}x{})", path, width, height);

                    // TODO: Upload texture to renderer and get TextureHandle
                    // For now, use NONE as placeholder until texture upload is implemented
                    let texture_handle = katla_gfx::TextureHandle::NONE;

                    // Update the thumbnail cache entry
                    if let Some(entry) = self.background_loader.get_thumbnail_mut(&path) {
                        entry.uploaded = true;
                    }

                    // Store texture handle for this path (persists across directory navigations)
                    self.thumbnail_texture_handles
                        .insert(path.clone(), texture_handle);

                    // Update asset browser entries with this thumbnail
                    for asset in self.editor_ui.asset_browser.assets.iter_mut() {
                        if asset.path == path {
                            asset.thumbnail_state = ThumbnailState::Loaded { texture_handle };
                            debug!("Updated thumbnail state for {:?}", path);
                            break;
                        }
                    }
                }
                LoadResult::ImageLoaded {
                    path,
                    width,
                    height,
                    pixels: _,
                    ..
                } => {
                    debug!("Full image loaded: {:?} ({}x{})", path, width, height);
                    // Future: Handle full image loads (e.g., for textures, skyboxes)
                }
                LoadResult::ModelLoaded {
                    path,
                    vertices: _,
                    indices: _,
                    ..
                } => {
                    debug!("Model loaded: {:?}", path);
                }
                LoadResult::ShaderSourceLoaded { path, source, .. } => {
                    debug!("Shader source loaded: {:?} ({} bytes)", path, source.len());
                    // Future: Handle shader loads (e.g., for hot reload)
                }
                LoadResult::Failed { path, error, .. } => {
                    warn!("Failed to load {:?}: {}", path, error);

                    // Update asset browser entry to show failed state
                    for asset in self.editor_ui.asset_browser.assets.iter_mut() {
                        if asset.path == path {
                            asset.thumbnail_state = ThumbnailState::Failed;
                            break;
                        }
                    }
                }
            }
        }
    }

    /// Convert winit KeyCode to UI KeyCode.
    fn winit_to_ui_key(keycode: KeyCode) -> Option<katla_ui::input::KeyCode> {
        use katla_ui::input::KeyCode as UiKey;
        Some(match keycode {
            KeyCode::ShiftLeft | KeyCode::ShiftRight => UiKey::Shift,
            KeyCode::ControlLeft | KeyCode::ControlRight => UiKey::Control,
            KeyCode::AltLeft | KeyCode::AltRight => UiKey::Alt,
            KeyCode::SuperLeft | KeyCode::SuperRight => UiKey::Super,
            KeyCode::Tab => UiKey::Tab,
            KeyCode::ArrowLeft => UiKey::ArrowLeft,
            KeyCode::ArrowRight => UiKey::ArrowRight,
            KeyCode::ArrowUp => UiKey::ArrowUp,
            KeyCode::ArrowDown => UiKey::ArrowDown,
            KeyCode::Home => UiKey::Home,
            KeyCode::End => UiKey::End,
            KeyCode::PageUp => UiKey::PageUp,
            KeyCode::PageDown => UiKey::PageDown,
            KeyCode::Enter | KeyCode::NumpadEnter => UiKey::Enter,
            KeyCode::Escape => UiKey::Escape,
            KeyCode::Backspace => UiKey::Backspace,
            KeyCode::Delete => UiKey::Delete,
            KeyCode::Insert => UiKey::Insert,
            KeyCode::Space => UiKey::Space,
            KeyCode::KeyA => UiKey::A,
            _ => return None,
        })
    }
}
