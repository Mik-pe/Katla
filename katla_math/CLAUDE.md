# katla_math

Custom math library for 3D graphics.

## Types

- `Vec2`, `Vec3`, `Vec4` - Vector types
- `Mat3`, `Mat4` - Matrix types
- `Quat` - Quaternion for rotations
- `Transform` - Position, rotation, scale combined

## Design Principles

- No external dependencies
- SIMD-optimized where practical
- GLSL-compatible naming conventions

## Common Operations

```rust
// Vector math
let direction = (target - source).normalize();
let distance = (a - b).length();

// Transform composition
let transform = Transform::from_position_rotation_scale(pos, rot, scale);
let matrix = transform.to_matrix();

// Quaternion rotation
let rotation = Quat::from_axis_angle(Vec3::Y, angle);
```

## Dependencies

Must NOT depend on: ANY other crate
