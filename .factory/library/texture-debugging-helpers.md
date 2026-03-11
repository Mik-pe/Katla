# Texture Debugging Helpers

This document describes the debugging utilities added to the bindless texture system for tracking and inspecting texture state.

## Overview

The texture debugging helpers provide utilities for:
- Listing all registered textures with their bindless slots
- Showing slot allocation state (occupied vs free)
- Querying which texture occupies a specific slot
- Finding textures that aren't registered with the bindless system
- Getting statistics on texture registration

## API Methods

### BindlessTextureManager Methods

Located in `katla_gfx/src/vulkan/bindless_texture.rs`:

#### `debug_slot_allocation() -> String`
Returns a formatted string showing slot allocation state.

Example output:
```
Bindless Slot Allocation:
Slots 0-4: [DEFAULT] (reserved for default textures)
Slot 5: [OCCUPIED]
Slot 6: [OCCUPIED]
Slots 7-4095: [FREE]

Total: 6 occupied, 4090 available, 4096 total
```

#### `list_occupied_slots() -> Vec<(u32, vk::ImageView)>`
Returns a list of all occupied slots with their Vulkan image view handles.

```rust
for (slot, image_view) in bindless_manager.list_occupied_slots() {
    println!("Slot {}: ImageView({:?})", slot, image_view);
}
```

#### `debug_slot_info(slot: u32) -> String`
Returns detailed information about a specific slot.

```rust
println!("{}", bindless_manager.debug_slot_info(5));
// Output: "Slot 5: [OCCUPIED] ImageView(0x1234567890)"
```

#### `is_default_slot(slot: u32) -> bool`
Checks if a slot is reserved for default textures (slots 0-4).

#### `default_texture_count() -> u32`
Returns the number of slots reserved for default textures (5).

### TextureManager Methods

Located in `katla_gfx/src/texture/manager.rs`:

#### `debug_bindless_textures() -> String`
Returns a formatted string listing all registered textures with their slots.

Example output:
```
Registered Bindless Textures (3):
  TextureHandle(42) -> Slot 5
  TextureHandle(43) -> Slot 6
  TextureHandle(44) -> Slot 7
```

#### `list_unregistered_textures() -> Vec<TextureHandle>`
Returns texture handles that exist but don't have a bindless slot assigned.

```rust
for handle in texture_manager.list_unregistered_textures() {
    println!("Texture {:?} is not registered with bindless", handle);
}
```

#### `is_bindless_registered(handle: TextureHandle) -> bool`
Checks if a texture has a bindless slot assigned.

```rust
if !texture_manager.is_bindless_registered(texture_handle) {
    println!("Texture is not registered with bindless system");
}
```

#### `bindless_stats() -> (usize, usize, usize)`
Returns registration statistics: (registered_count, unregistered_count, total_count).

```rust
let (registered, unregistered, total) = texture_manager.bindless_stats();
println!("Bindless: {}/{} registered", registered, total);
```

### VulkanRenderer Methods

Located in `katla_gfx/src/renderer.rs`:

All TextureManager and BindlessTextureManager debugging methods are exposed through VulkanRenderer:

- `debug_bindless_slot_allocation() -> String`
- `list_occupied_bindless_slots() -> Vec<(u32, ash::vk::ImageView)>`
- `debug_bindless_slot_info(slot: u32) -> String`
- `debug_bindless_textures() -> String`
- `list_unregistered_textures() -> Vec<TextureHandle>`
- `is_bindless_registered(handle: TextureHandle) -> bool`
- `get_bindless_registration_stats() -> (usize, usize, usize)`

## Usage Examples

### Example 1: Debug texture allocation issues

```rust
// In your rendering code or debug console
let allocation = renderer.debug_bindless_slot_allocation();
log::info!("{}", allocation);

// Output shows which slots are occupied and which are free
// Useful for tracking down texture leaks or allocation problems
```

### Example 2: Find unregistered textures

```rust
// Check if any textures weren't registered with bindless
let unregistered = renderer.list_unregistered_textures();
if !unregistered.is_empty() {
    log::warn!("Found {} textures not registered with bindless:", unregistered.len());
    for handle in unregistered {
        log::warn!("  - Texture {:?}", handle);
    }
}
```

### Example 3: Query specific slot information

```rust
// Check what's in a specific slot
let slot_info = renderer.debug_bindless_slot_info(10);
println!("{}", slot_info);

// Output: "Slot 10: [OCCUPIED] ImageView(0x...)"
// Or: "Slot 10: [FREE] - no texture bound"
```

### Example 4: Get registration statistics

```rust
let (registered, unregistered, total) = renderer.get_bindless_registration_stats();
println!("Texture Registration: {}/{} registered, {} unregistered",
         registered, total, unregistered);

let (occupied, available, total) = renderer.get_bindless_stats();
println!("Slot Allocation: {}/{} used", occupied, total);
```

## Testing

The debugging utilities have comprehensive test coverage:

```bash
# Run bindless texture tests
cargo test -p katla_gfx -- bindless

# Output:
# running 14 tests (unit tests)
# running 10 tests (integration tests)
# test result: ok. 24 passed; 0 failed
```

## Implementation Notes

- All debugging methods are `pub` and accessible through the public API
- Methods that return strings format output for human readability
- Methods that return vectors allow programmatic inspection
- No performance impact on rendering (debugging is opt-in)
- All methods handle edge cases (invalid slots, empty lists, etc.)

## Future Enhancements

These debugging utilities provide a foundation for:
- Interactive texture inspection tools
- Visual texture browser/debugger UI
- Automated texture leak detection
- Performance profiling for texture allocations
- Hot-reload validation for texture changes
