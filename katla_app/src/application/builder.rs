use std::{cell::RefCell, collections::HashMap, rc::Rc};

use katla_ecs::{System, SystemExecutionOrder, World};
use katla_ui::{FontId, ForkAwesome};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::ModifiersState;

use crate::{
    application::{Application, ApplicationInfo},
    entities::Camera,
    error::AppResult,
    input::InputMapper,
    preferences::Preferences,
    rendering::MaterialManager,
    resources::ResourceManager,
    ui::Theme,
    util::{FileCache, Timer},
};

#[derive(Default)]
pub struct ApplicationBuilder {
    app_name: String,
    validation_layer_enabled: bool,
    max_frames: Option<usize>,
    world: World,
}

impl ApplicationBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.app_name = name.into();
        self
    }

    pub fn validation_layer(mut self, on: bool) -> Self {
        self.validation_layer_enabled = on;
        self
    }

    pub fn single_frame(mut self, on: bool) -> Self {
        // When single_frame is enabled, render some frames for better validation
        self.max_frames = if on { Some(25) } else { None };
        self
    }

    pub fn max_frames(mut self, count: usize) -> Self {
        self.max_frames = Some(count);
        self
    }

    pub fn with_system(mut self, system: Box<dyn System>, order: SystemExecutionOrder) -> Self {
        self.world.register_system(system, order);
        self
    }

    pub fn with_systems(mut self, systems: Vec<Box<dyn System>>) -> Self {
        for system in systems {
            self.world
                .register_system(system, SystemExecutionOrder::default());
        }
        self
    }

    fn build_event_loop() -> EventLoop<()> {
        let event_loop = EventLoop::new().unwrap();
        event_loop.set_control_flow(ControlFlow::Poll);
        event_loop
    }

    pub fn build(self) -> AppResult<(Application, EventLoop<()>)> {
        let event_loop = Self::build_event_loop();

        let info = ApplicationInfo {
            name: self.app_name,
            validation_layer_enabled: self.validation_layer_enabled,
            max_frames: self.max_frames,
        };
        let mut world = self.world;
        let camera = Rc::new(RefCell::new(Camera::new(&mut world)));

        let resources = ResourceManager::discover()?;

        // Load user preferences
        let preferences = Preferences::load();
        let theme = Theme::by_name(&preferences.theme).unwrap_or_default();
        log::info!(
            "Loaded preferences: theme={}, show_grid={}, show_stats={}, font_scale={}",
            preferences.theme,
            preferences.show_grid,
            preferences.show_stats,
            preferences.font_scale
        );

        // Create UI context and load default font
        let mut ui_context = katla_ui::UiContext::new();

        // Load default font for text rendering
        let font_path = resources.font_path("roboto-regular.ttf");
        if font_path.exists() {
            match std::fs::read(&font_path) {
                Ok(font_bytes) => {
                    match ui_context.fonts.add_font(&font_bytes) {
                        Ok(font_id) => {
                            // Precache common ASCII characters at typical UI sizes
                            // Note: Using scale_factor 1.0 for initial cache; will re-rasterize at
                            // actual DPI scale on first use if different
                            ui_context.fonts.precache_ascii(font_id, 14.0, 1.0);
                            ui_context.fonts.precache_ascii(font_id, 16.0, 1.0);
                            ui_context.set_font(font_id);
                            log::info!("Loaded default font from {}", font_path.display());
                        }
                        Err(e) => {
                            log::warn!("Failed to parse font: {}", e);
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Failed to read font file {}: {}", font_path.display(), e);
                }
            }
        } else {
            log::warn!("Font file not found: {}", font_path.display());
        }

        // Load icon font (ForkAwesome)
        let icon_font_path = resources.font_path("forkawesome-webfont.ttf");
        if icon_font_path.exists() {
            match std::fs::read(&icon_font_path) {
                Ok(font_bytes) => {
                    match ui_context.fonts.add_font_with_id(&font_bytes, FontId::ICON) {
                        Ok(()) => {
                            // Precache common icons at typical UI sizes
                            // Note: Using scale_factor 1.0 for initial cache; will re-rasterize at
                            // actual DPI scale on first use if different
                            ui_context.fonts.precache_icons(
                                FontId::ICON,
                                14.0,
                                1.0,
                                ForkAwesome::common_icons(),
                            );
                            ui_context.fonts.precache_icons(
                                FontId::ICON,
                                16.0,
                                1.0,
                                ForkAwesome::common_icons(),
                            );
                            log::info!("Loaded icon font from {}", icon_font_path.display());
                        }
                        Err(e) => {
                            log::warn!("Failed to parse icon font: {}", e);
                        }
                    }
                }
                Err(e) => {
                    log::warn!(
                        "Failed to read icon font file {}: {}",
                        icon_font_path.display(),
                        e
                    );
                }
            }
        } else {
            log::warn!("Icon font file not found: {}", icon_font_path.display());
        }

        let app = Application {
            window: None,
            renderer: None,
            camera,
            gltf_cache: FileCache::new(),
            material_manager: MaterialManager::new(),
            stage_upload: false,
            timer: Timer::new(100),
            info,
            world,
            input_mapper: InputMapper::new(),
            current_modifiers: ModifiersState::empty(),
            frame_count: 0,
            resources,
            skeleton_buffers: HashMap::new(),
            ui_context,
            debug_overlay: crate::ui::DebugOverlay::new(),
            editor_ui: {
                let mut editor = crate::ui::EditorUI::with_theme(theme);
                editor.show_grid = preferences.show_grid;
                editor.show_stats = preferences.show_stats;
                editor.set_font_scale(preferences.font_scale);
                editor
            },
            use_editor_ui: true, // Default to editor UI mode
            preferences,
            scale_factor: 1.0, // Will be updated when window is created
        };

        Ok((app, event_loop))
    }
}
