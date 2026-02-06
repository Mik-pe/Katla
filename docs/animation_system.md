# Animation System

## Overview

The animation system in Katla provides a comprehensive framework for skeletal and transform-based animations. It supports loading animation data from GLTF files, playing animations with various control options, and blending between multiple animation clips.

## Architecture

### Components

- **`AnimationPlayer`** - Controls animation playback on entities
  - Play/pause/stop controls
  - Playback speed adjustment
  - Looping support
  - Animation blending with weights
  - Time-based seeking

- **`AnimatedModel`** - Container for all animation clips of a model
  - HashMap of named animation clips
  - Animation sequences for combining multiple clips

- **`JointTransform`** - Transform data for skeletal joints
  - Translation (3D vector)
  - Rotation (quaternion)
  - Scale (3D vector)
  - Interpolation support (SLERP for rotations)

- **`MorphTargetWeights`** - Weights for mesh deformation
  - Array of weight values (0.0 - 1.0)
  - Used for facial animations and shape blending

### Core Structures

#### AnimationClip

A complete animation that can be played on an animated model:

```rust
pub struct AnimationClip {
    pub name: String,
    pub duration: f32,
    pub channels: Vec<AnimationChannel>,
}
```

Each clip consists of multiple channels, where each channel animates a specific property (translation, rotation, scale, or morph target weights).

#### AnimationChannel

Animates a single property of a node/joint:

```rust
pub struct AnimationChannel {
    pub target_node: usize,  // Index of the target joint
    pub path: ChannelPath,   // What property to animate
    pub sampler: AnimationSampler,  // Keyframe data
}
```

#### AnimationSampler

Contains keyframe data with timing and interpolation:

```rust
pub struct AnimationSampler {
    pub inputs: Vec<f32>,  // Time values for keyframes
    pub translations: Option<Vec<[f32; 3]>>,
    pub rotations: Option<Vec<[f32; 4]>>,
    pub scales: Option<Vec<[f32; 3]>>,
    pub weights: Option<Vec<f32>>,
    pub interpolation: Interpolation,
}
```

### Systems

#### AnimationUpdateSystem

Updates animation players based on elapsed time:

- Advances time for playing animations
- Handles looping behavior
- Stops non-looping animations when complete
- Respects playback speed

**Location:** `katla_app/src/animation/systems.rs`

#### SkeletalAnimationSystem

Applies skeletal animation transforms to the scene (TODO):

- Samples animation clips at current time
- Computes joint hierarchies
- Updates joint matrices for GPU skinning

#### MorphTargetSystem

Applies morph target animations to meshes (TODO):

- Samples weight animations
- Updates vertex positions based on morph targets
- Re-uploads vertex data to GPU

## Usage

### Creating an Animation Player

```rust
use katla::animation::AnimationPlayer;

// Create a player for a specific animation clip
let player = AnimationPlayer::new("Walk")
    .looping()
    .with_speed(1.5);

// Attach to an entity
world.add_component(entity, player);
```

### Builder Pattern Options

```rust
AnimationPlayer::new("Run")
    .looping()              // Enable looping
    .with_speed(2.0)        // 2x playback speed
    .with_clip("Jump");     // Change the animation clip
```

### Runtime Control

```rust
// Get a mutable reference to the player
if let Some(mut player) = world.get_component_mut::<AnimationPlayer>(entity) {
    player.play();        // Start playback
    player.pause();       // Pause playback
    player.stop();        // Stop and reset to beginning
    player.seek(1.5);     // Jump to specific time
}
```

### Working with Joint Transforms

```rust
use katla::animation::JointTransform;

// Create transforms
let start = JointTransform::identity();
let end = JointTransform::from_translation([10.0, 0.0, 0.0]);

// Interpolate between them (uses SLERP for rotations)
let blended = start.lerp(&end, 0.5);
```

### Morph Target Weights

```rust
use katla::animation::MorphTargetWeights;

// Create weights for 3 morph targets
let mut weights = MorphTargetWeights::new(3);

// Set individual weights (automatically clamped to 0.0-1.0)
weights.set_weight(0, 1.0);  // Full weight
weights.set_weight(1, 0.5);  // Half weight
weights.set_weight(2, 0.0);  // No weight
```

## GLTF Loading

The animation system includes GLTF loader functions in `gltf_loader.rs`:

- **`load_animations()`** - Parses animation data from GLTF files
- **`load_skins()`** - Loads skin data for skeletal animation
- **`build_skeleton()`** - Constructs joint hierarchies

Currently, the GLTF loader logs animation information but doesn't fully parse the data. This is a planned feature.

### Loading GLTF Animations

```rust
use katla::animation::AnimationManager;
use katla::util::GLTFModel;

// After loading a GLTF model
let model = GLTFModel::from_file("model.glb")?;

// Load animations into the world
AnimationManager::load_gltf_animations(&mut world, &model);
```

## Interpolation Types

The animation system supports three interpolation modes:

- **`Linear`** - Smooth interpolation between keyframes
- **`Step`** - Discrete jumps between keyframes
- **`CubicSpline`** - Smooth spline interpolation with tangents

## Implementation Status

### Completed

- [x] Component structures (AnimationPlayer, AnimatedModel, JointTransform, etc.)
- [x] Animation clip and channel structures
- [x] Animation sampler with keyframe storage
- [x] AnimationUpdateSystem for time advancement
- [x] Joint transform interpolation with SLERP
- [x] Morph target weights component
- [x] Skin and skeleton structures
- [x] GLTF animation logging

### In Progress

- [ ] Full GLTF animation parsing
- [ ] SkeletalAnimationSystem implementation
- [ ] MorphTargetSystem implementation
- [ ] Animation blending between multiple clips
- [ ] Animation events (footsteps, impacts, etc.)

### Future Enhancements

- [ ] Animation state machines
- [ ] Root motion extraction
- [ ] Inverse kinematics (IK)
- [ ] Additive animations
- [ ] Animation compression
- [ ] Runtime animation blending graphs
- [ ] Procedural animation (look-at, IK reaches)

## Testing

The animation system has comprehensive tests covering:

- Animation player creation and builder pattern
- Play/pause/stop controls
- Time seeking and clamping
- Joint transform interpolation (translation, rotation, scale)
- Morph target weight management
- Animation sampler creation and duration calculation
- Multi-channel animation clips

Run tests with:

```bash
cargo test -p katla animation
```

## Technical Details

### Quaternion Interpolation (SLERP)

The `JointTransform::lerp()` method uses SLERP (Spherical Linear Interpolation) for rotation interpolation, which provides smooth, constant-speed rotation paths:

```rust
let qa = Quat::new_from_xyzw(x1, y1, z1, w1);
let qb = Quat::new_from_xyzw(x2, y2, z2, w2);
let q_result = Quat::slerp(qa, qb, t);
```

### Joint Hierarchy

For skeletal animation, joints form a hierarchy where child joints inherit their parent's transforms. The system uses:

- **Inverse Bind Matrices** - Transform from mesh space to joint space
- **Joint Indices** - Map vertices to influencing joints
- **Joint Weights** - Blend influence of multiple joints per vertex

### Performance Considerations

- Animation updates use dirty flags to avoid unnecessary calculations
- Static scene optimization skips animation for non-moving entities
- Joint transforms are cached to avoid redundant calculations
- Keyframe sampling uses efficient interpolation algorithms

## Related Systems

- **Transform Hierarchy System** - Manages parent-child relationships for animated joints
- **Rendering System** - Applies skinning transforms to vertices
- **Material System** - Handles shaders that use joint matrices

## References

- [GLTF 2.0 Animation Spec](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#animations)
- [Bevy Animation System](https://github.com/bevyengine/bevy/tree/main/crates/bevy_animation)
- [Unity Animation System](https://docs.unity3d.com/Manual/AnimationSection.html)
