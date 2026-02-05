# Mesh API Guide

This guide demonstrates how to use the new MeshBuilder API for creating and adding meshes to your scene.

## Creating a Simple Cube

```rust
use katla_app::rendering::MeshBuilder;
use katla_math::Vec3;

// Create a cube with default size (1.0, 1.0, 1.0) at the origin
let cube = MeshBuilder::new(&mut world, context.clone(), &render_pass)
    .create_cube();

// Create a cube with custom size
let cube = MeshBuilder::new(&mut world, context.clone(), &render_pass)
    .size(Vec3::new(2.0, 2.0, 2.0))
    .create_cube();

// Create a cube at a specific position
let cube = MeshBuilder::new(&mut world, context.clone(), &render_pass)
    .position(Vec3::new(10.0, 0.0, 0.0))
    .create_cube();
```

## Creating a Sphere

```rust
// Create a sphere with default radius (0.5) and 32 segments
let sphere = MeshBuilder::new(&mut world, context.clone(), &render_pass)
    .create_sphere();

// Create a sphere with custom radius and resolution
let sphere = MeshBuilder::new(&mut world, context.clone(), &render_pass)
    .radius(1.0)
    .segments(64)
    .rings(64)
    .create_sphere();
```

## Creating a Cylinder

```rust
// Create a cylinder with default dimensions
let cylinder = MeshBuilder::new(&mut world, context.clone(), &render_pass)
    .create_cylinder();

// Create a cylinder with custom dimensions
let cylinder = MeshBuilder::new(&mut world, context.clone(), &render_pass)
    .height(2.0)
    .radius(0.5)
    .segments(64)
    .create_cylinder();
```

## Creating a Plane

```rust
// Create a plane with default size (1.0 x 1.0)
let plane = MeshBuilder::new(&mut world, context.clone(), &render_pass)
    .create_plane();

// Create a plane with custom dimensions
let plane = MeshBuilder::new(&mut world, context.clone(), &render_pass)
    .size(Vec3::new(10.0, 10.0, 1.0))
    .segments(64)
    .create_plane();
```

## Creating a Torus

```rust
// Create a torus with default dimensions
let torus = MeshBuilder::new(&mut world, context.clone(), &render_pass)
    .create_torus();

// Create a torus with custom dimensions
let torus = MeshBuilder::new(&mut world, context.clone(), &render_pass)
    .radius(1.0)
    .segments(64)
    .rings(64)
    .create_torus();
```

## Setting Mesh Color

```rust
// Create a red cube
let red_cube = MeshBuilder::new(&mut world, context.clone(), &render_pass)
    .color([1.0, 0.0, 0.0])
    .create_cube();

// Create a green sphere
let green_sphere = MeshBuilder::new(&mut world, context.clone(), &render_pass)
    .color([0.0, 1.0, 0.0])
    .create_sphere();
```

## Combining Options

You can combine multiple options for more specific mesh creation:

```rust
let mesh = MeshBuilder::new(&mut world, context.clone(), &render_pass)
    .position(Vec3::new(5.0, 5.0, 5.0))
    .color([0.8, 0.6, 0.2])
    .radius(0.8)
    .segments(128)
    .create_sphere();
```

## Available Mesh Types

- **Cube**: 3D box shape
- **Sphere**: 3D sphere with adjustable resolution
- **Cylinder**: 3D cylinder with adjustable height and radius
- **Plane**: 2D plane with adjustable size
- **Torus**: 3D torus with adjustable major and minor radius

## API Methods

All mesh types share the following builder methods:

- `size(size: Vec3)`: Set mesh dimensions for cube and plane
- `radius(radius: f32)`: Set radius for sphere, cylinder, and torus
- `height(height: f32)`: Set height for cylinder
- `segments(segments: u32)`: Set vertex segments for sphere, cylinder, and plane
- `rings(rings: u32)`: Set vertex rings for sphere and torus
- `position(position: Vec3)`: Set mesh position in the world
- `color(color: [f32; 3])`: Set mesh color (RGB)
