# Project Brief

## What

Katla is a Vulkan/Metal game engine written in Rust, designed as a playground for graphics experiments and vibecoding.

## Why

To explore real-time rendering, ECS architecture, declarative UI, scripting, and physics integration in a single codebase. The explicit goal is to push what's possible with AI-assisted ("vibecoded") development.

## Scope

- 3D rendering with PBR materials, skeletal animation, and GLTF support
- Cross-platform GPU backends (Vulkan via ash, Metal via objc2-metal)
- In-engine editor with dockable panels, asset browser, entity inspector, transform gizmos
- Luau scripting system for game logic
- Rapier3D physics
- Audio system with SFX/Music/Ambient channels
