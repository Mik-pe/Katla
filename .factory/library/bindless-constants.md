# Bindless Texture Constants

## DEFAULT_TEXTURE_COUNT

The `BindlessTextureManager` reserves the first 5 slots (indices 0-4) for default textures used by the rendering system.

**Value:** `5`

**Location:** `katla_gfx/src/vulkan/bindless_texture.rs:64`

**Usage:**
- Slot 0-4: Reserved for default textures (white pixel, black pixel, etc.)
- Slot 5+: Available for dynamic texture allocation

**Methods using this constant:**
- `is_default_slot(slot: u32) -> bool` - Checks if a slot index is in the default range
- `default_texture_count() -> usize` - Returns the constant value

**Example:**
```rust
// Check if a slot is reserved for default textures
if bindless_manager.is_default_slot(slot_index) {
    println!("Slot {} is reserved for default textures", slot_index);
}

// Get the count of default texture slots
let default_count = bindless_manager.default_texture_count();
assert_eq!(default_count, 5);
```

**Note:** This is an implementation detail of the bindless texture system. The default textures are automatically registered during `BindlessTextureManager::new()` and are always present.
