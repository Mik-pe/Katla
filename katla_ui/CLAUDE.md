# katla_ui

Immediate mode UI system for debug overlays and in-game HUDs.

## Key Features

- Immediate mode API (no retained state)
- Built-in ForkAwesome icon support
- Text rendering with caching
- Widget library (buttons, sliders, trees, etc.)

## Common Icons

```rust
use katla_ui::icons::ForkAwesome;

ForkAwesome::FOLDER         // Folder
ForkAwesome::FOLDER_OPEN    // Open folder
ForkAwesome::FILE           // File
ForkAwesome::PENCIL         // Edit/rename
ForkAwesome::TRASH          // Delete
ForkAwesome::COPY           // Copy/duplicate
ForkAwesome::REFRESH        // Refresh
ForkAwesome::EXTERNAL_LINK  // Open in explorer
```

## Usage Pattern

```rust
ui.begin_frame();

if ui.button("Click Me") {
    // handle click
}

ui.text("Hello World");
ui.slider("Volume", &mut volume, 0.0, 1.0);

ui.end_frame();
```

## Performance Tips

- Make small structs `Copy` (e.g., `CachedGlyph`)
- Use `for &x in &collection` to avoid cloning
- Extract helper functions for repeated patterns

## Dependencies

Must NOT depend on: `katla_ecs`, `katla_app`
CAN depend on: `katla_math`, `katla_vulkan`
