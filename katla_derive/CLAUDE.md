# katla_derive

Procedural macros for Katla engine.

## Derive Macros

### Component

Auto-implements `Component` trait for ECS:

```rust
#[derive(Component)]
pub struct MyComponent {
    pub value: f32,
}
```

### Material

Auto-implements `Material` trait for Vulkan materials:

```rust
#[derive(Material)]
pub struct PbrMaterial {
    #[uniform(0, 0)]
    pub base_color: Vec4,
    #[texture(0, 1)]
    pub albedo_map: TextureHandle,
}
```

## Implementation Notes

- Proc-macro crate (isolated from other crates)
- Generates boilerplate for reflection and binding
- Attributes control descriptor set layout

## Dependencies

As a proc-macro crate, this is isolated and doesn't depend on other Katla crates.
