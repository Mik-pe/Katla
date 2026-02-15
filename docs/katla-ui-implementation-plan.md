# katla_ui - Immediate Mode UI Crate Implementation Plan

**Status**: Planning
**Created**: 2026-02-15
**Updated**: 2026-02-15

---

## Quick Summary

A new `katla_ui` crate providing immediate mode UI capabilities for the Katla engine. This will enable debug overlays, in-game HUDs, editor tools, and runtime UI panels.

---

## Overview

### Why a Separate Crate?

1. **Separation of Concerns** - UI is a distinct subsystem from rendering/ ECS
2. **Optional Dependency** - Games that don't need UI can omit it
3. **Independent Testing** - UI can be tested in isolation
4. **Clean Architecture** - Follows Katla's modular crate pattern

### Why Immediate Mode?

1. **Simple API** - No complex state management, just function calls
2. **Debug-Friendly** - Easy to add temporary debug overlays
3. **ECS-Friendly** - Natural fit with per-frame updates
4. **Performance** - Modern immediate mode UIs are GPU-accelerated

---

## Crate Structure

```
katla_ui/
├── Cargo.toml
├── src/
│   ├── lib.rs                    # Crate entry point
│   ├── context.rs                # UiContext - main API
│   ├── input.rs                  # UI input handling
│   ├── layout/
│   │   ├── mod.rs
│   │   ├── flex.rs               # Flexbox-like layout
│   │   └── text.rs               # Text layout
│   ├── primitives/
│   │   ├── mod.rs
│   │   ├── rect.rs               # Colored rectangles
│   │   ├── text.rs               # Text rendering
│   │   ├── image.rs              # Image/texture rendering
│   │   └── clip.rs               # Clipping regions
│   ├── widgets/
│   │   ├── mod.rs
│   │   ├── button.rs             # Button widget
│   │   ├── slider.rs             # Slider widget
│   │   ├── checkbox.rs           # Checkbox widget
│   │   ├── text_input.rs         # Text input field
│   │   ├── window.rs             # Draggable window
│   │   ├── panel.rs              # Panel/container
│   │   └── label.rs              # Text label
│   ├── renderer/
│   │   ├── mod.rs
│   │   ├── pipeline.rs           # Vulkan pipeline for UI
│   │   ├── vertex.rs             # UI vertex format
│   │   ├── texture_atlas.rs      # Glyph/icon atlas
│   │   └── draw_list.rs          # Batched draw commands
│   ├── text/
│   │   ├── mod.rs
│   │   ├── font.rs               # Font loading
│   │   ├── glyph_cache.rs        # Rendered glyph cache
│   │   └── shaper.rs             # Text shaping
│   └── style/
│       ├── mod.rs
│       ├── theme.rs              # Color themes
│       └── style.rs              # Widget styling
└── tests/
    └── ...
```

---

## Dependencies

### External Crates

```toml
[dependencies]
katla_math = { path = "../katla_math" }
katla_vulkan = { path = "../katla_vulkan" }

# Font handling
fontdue = "0.9"              # Fast font rasterization

# Optional: For complex text shaping
# rustybuzz = "0.18"         # HarfBuzz bindings for text shaping
```

### Dependency Restrictions

Following Katla's architecture rules:
- **katla_ui** CAN depend on: `katla_math`, `katla_vulkan`
- **katla_ui** MUST NOT depend on: `katla_ecs`, `katla_app`

This ensures the UI crate is reusable in different contexts (editor, runtime, etc.)

---

## Core Components

### 1. UiContext (context.rs)

The main entry point for UI rendering:

```rust
pub struct UiContext {
    draw_list: DrawList,
    input: UiInputState,
    style: UiStyle,
    font_cache: FontCache,
    clip_stack: Vec<Rect2D>,
    id_stack: Vec<u64>,
}

impl UiContext {
    /// Begin a new frame
    pub fn begin(&mut self, screen_size: Vec2);

    /// End frame and get draw data
    pub fn end(&mut self) -> &DrawList;

    /// Window container
    pub fn window(&mut self, id: &str, bounds: Rect2D) -> WindowBuilder<'_>;

    /// Basic widgets
    pub fn button(&mut self, id: &str, text: &str, bounds: Rect2D) -> bool;
    pub fn checkbox(&mut self, id: &str, label: &str, checked: &mut bool, bounds: Rect2D);
    pub fn slider(&mut self, id: &str, value: &mut f32, min: f32, max: f32, bounds: Rect2D);
    pub fn label(&mut self, text: &str, bounds: Rect2D);
    pub fn text_input(&mut self, id: &str, text: &mut String, bounds: Rect2D) -> bool;

    /// Low-level primitives
    pub fn rect(&mut self, bounds: Rect2D, color: Color);
    pub fn text(&mut self, text: &str, position: Vec2, color: Color, size: f32);
    pub fn image(&mut self, texture: TextureId, bounds: Rect2D, uv: Rect2D);

    /// Layout helpers
    pub fn layout_row(&mut self, height: f32, widths: &[f32]);
    pub fn next_item_bounds(&self) -> Rect2D;
}
```

### 2. UiInputState (input.rs)

Input handling for UI interactions:

```rust
pub struct UiInputState {
    pub mouse_pos: Vec2,
    pub mouse_delta: Vec2,
    pub mouse_down: [bool; 5],      // Left, Right, Middle, Forward, Back
    pub mouse_clicked: [bool; 5],
    pub mouse_released: [bool; 5],
    pub scroll_delta: Vec2,
    pub key_chars: Vec<char>,        // Text input
    pub keys_pressed: Vec<VirtualKeyCode>,
    pub focused_widget: Option<u64>,
}
```

### 3. DrawList (renderer/draw_list.rs)

Batched rendering data:

```rust
pub struct DrawList {
    pub commands: Vec<DrawCommand>,
    pub vertices: Vec<UiVertex>,
    pub indices: Vec<u32>,
}

pub struct DrawCommand {
    pub texture: Option<TextureId>,
    pub clip_rect: Rect2D,
    pub index_count: u32,
}

#[repr(C)]
pub struct UiVertex {
    pub position: Vec2,      // Screen space
    pub uv: Vec2,            // Texture coordinates
    pub color: Color,        // Vertex color
}
```

### 4. Font System (text/)

Font loading and glyph caching:

```rust
pub struct FontCache {
    fonts: HashMap<FontId, Font>,
    glyph_atlas: TextureAtlas,
    glyph_cache: HashMap<(FontId, char, f32), CachedGlyph>,
}

pub struct CachedGlyph {
    pub uv_rect: Rect2D,     // Location in atlas
    pub size: Vec2,          // Glyph size
    pub offset: Vec2,        // Offset from baseline
    pub advance: f32,        // Advance width
}
```

### 5. Vulkan Renderer (renderer/)

UI-specific Vulkan pipeline:

```rust
pub struct UiRenderer {
    pipeline: Pipeline,
    vertex_buffer: Buffer,
    index_buffer: Buffer,
    uniform_buffer: Buffer,
    texture_atlas: Texture,
    sampler: Sampler,
    descriptor_set: DescriptorSet,
}

impl UiRenderer {
    pub fn new(context: &Rc<VulkanContext>) -> Result<Self>;

    /// Render draw list to command buffer
    pub fn render(
        &mut self,
        cmd: vk::CommandBuffer,
        draw_list: &DrawList,
        screen_size: Vec2,
    );
}
```

---

## Shader

```wgsl
// ui.wgsl

struct UiVertex {
    @location(0) position: vec2f,
    @location(1) uv: vec2f,
    @location(2) color: vec4f,
}

struct Uniforms {
    screen_size: vec2f,
    _padding: vec2f,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(0) @binding(1)
var texture_atlas: texture_2d<f32>;

@group(0) @binding(2)
var texture_sampler: sampler;

@vertex
fn vs_main(in: UiVertex) -> @builtin(position) vec4f {
    // Convert screen coords to NDC
    let ndc = (in.position / uniforms.screen_size) * 2.0 - 1.0;
    return vec4f(ndc.x, -ndc.y, 0.0, 1.0);
}

@fragment
fn fs_main(in: UiVertex) -> @location(0) vec4f {
    let tex_color = textureSample(texture_atlas, texture_sampler, in.uv);
    return tex_color * in.color;
}
```

---

## Widget Examples

### Button

```rust
pub fn button(&mut self, id: &str, text: &str, bounds: Rect2D) -> bool {
    let id_hash = hash_id(id);
    let hovered = self.input.mouse_pos.is_inside(bounds);
    let clicked = hovered && self.input.mouse_clicked[0];

    // Determine style based on state
    let color = if self.input.focused_widget == Some(id_hash) {
        self.style.button_active
    } else if hovered {
        self.style.button_hovered
    } else {
        self.style.button_normal
    };

    // Draw button background
    self.rect(bounds, color);

    // Draw button text (centered)
    let text_size = self.measure_text(text, self.style.font_size);
    let text_pos = bounds.center() - text_size * 0.5;
    self.text(text, text_pos, self.style.text_color, self.style.font_size);

    // Track focus
    if clicked {
        self.input.focused_widget = Some(id_hash);
    }

    clicked
}
```

### Window

```rust
pub fn window(&mut self, id: &str, bounds: Rect2D) -> WindowBuilder<'_> {
    WindowBuilder {
        ctx: self,
        id,
        bounds,
        is_dragging: false,
    }
}

impl<'a> WindowBuilder<'a> {
    pub fn title(mut self, title: &str) -> Self {
        // Draw title bar
        let title_bar = Rect2D::new(
            self.bounds.min,
            Vec2::new(self.bounds.max.x(), self.bounds.min.y() + 24.0)
        );
        self.ctx.rect(title_bar, self.ctx.style.window_title_bg);
        self.ctx.text(title, title_bar.min + Vec2::new(8.0, 4.0),
                      self.ctx.style.text_color, 14.0);
        self
    }

    pub fn content<F: FnOnce(&mut UiContext)>(mut self, f: F) {
        // Handle dragging
        self.handle_dragging();

        // Push content clip rect
        let content_bounds = self.bounds.contract(8.0);
        self.ctx.clip_stack.push(content_bounds);

        // Draw window background
        self.ctx.rect(self.bounds, self.ctx.style.window_bg);

        // Call user content
        f(self.ctx);

        // Pop clip rect
        self.ctx.clip_stack.pop();
    }
}
```

---

## Usage Examples

### Debug Overlay

```rust
fn draw_debug_overlay(ui: &mut UiContext, stats: &FrameStats) {
    ui.begin(screen_size);

    // Stats window
    ui.window("debug", Rect2D::from_origin_size(Vec2::new(10.0, 10.0), Vec2::new(200.0, 150.0)))
        .title("Debug Info")
        .content(|ui| {
            ui.label(&format!("FPS: {:.1}", stats.fps), ui.next_item_bounds());
            ui.label(&format!("Frame: {:.2}ms", stats.frame_time), ui.next_item_bounds());
            ui.label(&format!("Draw Calls: {}", stats.draw_calls), ui.next_item_bounds());
            ui.label(&format!("Triangles: {}", stats.triangles), ui.next_item_bounds());
        });

    let draw_list = ui.end();
    ui_renderer.render(cmd, draw_list, screen_size);
}
```

### In-Game HUD

```rust
fn draw_hud(ui: &mut UiContext, player: &Player) {
    ui.begin(screen_size);

    // Health bar
    let health_bounds = Rect2D::from_origin_size(Vec2::new(20.0, screen_size.y - 40.0), Vec2::new(200.0, 20.0));
    ui.rect(health_bounds, Color::rgb(0.2, 0.2, 0.2)); // Background
    let health_fill = health_bounds.with_width(health_bounds.width() * player.health);
    ui.rect(health_fill, Color::rgb(0.8, 0.2, 0.2)); // Fill

    // Crosshair
    let center = screen_size * 0.5;
    ui.rect(Rect2D::from_center_size(center, Vec2::new(20.0, 2.0)), Color::WHITE);
    ui.rect(Rect2D::from_center_size(center, Vec2::new(2.0, 20.0)), Color::WHITE);

    ui.end();
}
```

### Settings Panel

```rust
fn draw_settings(ui: &mut UiContext, settings: &mut Settings) {
    ui.window("settings", Rect2D::from_center_size(screen_center, Vec2::new(400.0, 300.0)))
        .title("Settings")
        .content(|ui| {
            ui.layout_row(30.0, &[100.0, 200.0]);

            ui.label("Music Volume", ui.next_item_bounds());
            ui.slider("music_vol", &mut settings.music_volume, 0.0, 1.0, ui.next_item_bounds());

            ui.label("SFX Volume", ui.next_item_bounds());
            ui.slider("sfx_vol", &mut settings.sfx_volume, 0.0, 1.0, ui.next_item_bounds());

            ui.label("Fullscreen", ui.next_item_bounds());
            ui.checkbox("fullscreen", "", &mut settings.fullscreen, ui.next_item_bounds());

            ui.layout_row(40.0, &[100.0]);
            if ui.button("apply", "Apply", ui.next_item_bounds()) {
                settings.apply();
            }
        });
}
```

---

## Implementation Phases

### Phase 1: Core Infrastructure (Week 1)
- [ ] Create crate structure
- [ ] `UiContext` with `begin()`/`end()`
- [ ] `DrawList` and `UiVertex`
- [ ] Basic rectangle rendering
- [ ] Clip stack

### Phase 2: Text Rendering (Week 2)
- [ ] Font loading with `fontdue`
- [ ] Glyph cache and texture atlas
- [ ] Text measurement
- [ ] Text rendering to draw list

### Phase 3: Input & Interaction (Week 3)
- [ ] `UiInputState` from existing input
- [ ] Hit testing
- [ ] Focus management
- [ ] Drag handling

### Phase 4: Widgets (Week 4)
- [ ] Label
- [ ] Button
- [ ] Checkbox
- [ ] Slider
- [ ] Window

### Phase 5: Vulkan Renderer (Week 5)
- [ ] UI pipeline creation
- [ ] Texture atlas management
- [ ] Buffer management (dynamic)
- [ ] Integration with render graph

### Phase 6: Polish (Week 6)
- [ ] Theming system
- [ ] Layout helpers
- [ ] Performance optimization
- [ ] Documentation

---

## Integration Points

### katla_app Integration

```rust
// In Application
struct Application {
    // ... existing fields
    ui_context: katla_ui::UiContext,
    ui_renderer: katla_ui::UiRenderer,
}

impl ApplicationHandler for Application {
    fn window_event(&mut self, event: WindowEvent) {
        // Feed input to UI
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.ui_context.input.mouse_pos = Vec2::new(position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                // Update mouse state
            }
            // ...
        }
    }

    fn about_to_wait(&mut self) {
        // ... existing rendering

        // Draw UI overlay
        draw_debug_overlay(&mut self.ui_context, &self.stats);
        self.ui_renderer.render(cmd, self.ui_context.end(), screen_size);
    }
}
```

### Render Graph Integration

UI renders as a final overlay pass:

```rust
graph_builder.add_pass("ui_overlay", |pass| {
    pass.write(Attachment::Color(swapchain_resource))
        .execute("ui_overlay", |ctx| {
            // UI rendering happens here
            ui_renderer.render(ctx.command_buffer, draw_list, screen_size);
        });
});
```

---

## Performance Considerations

1. **Texture Atlas** - All glyphs in one texture to minimize bindings
2. **Vertex Buffer** - Dynamic buffer with orphaning pattern
3. **Draw Call Batching** - Batch by texture and clip rect
4. **Clipping** - GPU scissor test for clip rects
5. **Glyph Caching** - Cache rendered glyphs, only rasterize on first use

---

## Future Enhancements

- [ ] Text input with clipboard support
- [ ] Scrollable regions
- [ ] Dropdown menus
- [ ] Color picker
- [ ] Graph/plot widgets
- [ ] Docking system
- [ ] Custom widget macros
- [ ] Accessibility (screen reader support)

---

## Alternatives Considered

### egui Integration
- **Pros**: Feature-complete, well-tested, active development
- **Cons**: Large dependency, less control, may not match Katla's style

### imgui-rs Integration
- **Pros**: Classic immediate mode API
- **Cons**: C++ FFI overhead, aging codebase

### Custom Implementation (Chosen)
- **Pros**: Full control, minimal dependencies, matches Katla architecture
- **Cons**: More initial work, fewer features initially
