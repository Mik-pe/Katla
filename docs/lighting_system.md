# Lighting System - Implementation Notes

## What Was Built ✅

### Light Components
Located in `katla_app/src/components/lighting.rs`:

1. **DirectionalLight** - Sun-like lights with direction but no position
   - Properties: `direction`, `color`, `intensity`
   - Use for: Sun, moon, global illumination

2. **PointLight** - Lights that emit in all directions from a point
   - Properties: `color`, `intensity`, `range`, attenuation factors
   - Use for: Light bulbs, torches, explosions

3. **SpotLight** - Cone-shaped lights with position and direction
   - Properties: `color`, `intensity`, `range`, `direction`, cutoff angles
   - Use for: Flashlights, car headlights, stage lights

4. **AmbientLight** - Global ambient illumination (resource, not component)
   - Properties: `color`, `intensity`
   - Use for: Base fill lighting so shadows aren't pure black

### LightingSystem
Located in `katla_app/src/systems/lighting_system.rs`:

- Collects all active lights from the ECS every frame
- Stores them in `LightCollection` resource
- Enforces maximum limits:
  - 4 directional lights
  - 16 point lights
  - 8 spot lights
- Extracts world positions for point/spot lights

### Scene Setup
Added to `katla_app/src/application/mod.rs`:
- 1 directional light (sun) angled down
- 1 red point light at (10, 10, 10)
- 1 blue point light at (-10, 8, 10)
- Ambient light (15% gray)

### Test Coverage
4 tests covering:
- ✅ Directional light collection
- ✅ Point light collection with position
- ✅ Multiple light types
- ✅ Maximum limits enforced

## What's Left To Do (For Actual Lighting) 🔧

The lights are now being collected and stored, but they won't affect rendering yet. To make them visible, you need to:

### 1. Create a Lighting Shader

You need to implement Phong or PBR lighting calculations in your shaders. Basic Phong model:

```glsl
// Fragment shader
struct DirectionalLight {
    vec3 direction;
    vec3 color;
    float intensity;
};

struct PointLight {
    vec3 position;
    vec3 color;
    float intensity;
    float range;
    float constant;
    float linear;
    float quadratic;
};

struct Material {
    vec3 ambient;
    vec3 diffuse;
    vec3 specular;
    float shininess;
};

vec3 calculate_phong(
    vec3 frag_pos, vec3 normal, vec3 view_dir,
    DirectionalLight dir_light, PointLight point_light,
    Material material
) {
    // Ambient
    vec3 ambient = material.ambient * light.color * light.intensity;

    // Diffuse
    float diff = max(dot(normal, light_dir), 0.0);
    vec3 diffuse = material.diffuse * diff * light.color * light.intensity;

    // Specular
    vec3 reflect_dir = reflect(-light_dir, normal);
    float spec = pow(max(dot(view_dir, reflect_dir), 0.0), material.shininess);
    vec3 specular = material.specular * spec * light.color * light.intensity;

    return ambient + diffuse + specular;
}
```

### 2. Create Uniform Buffers

In `katla_vulkan`, you need to:

1. Define uniform buffer structures matching your shader layout
2. Create a `LightingUniform` struct with arrays of lights
3. Upload light data from `LightCollection` resource to GPU each frame

```rust
// In katla_vulkan
pub struct LightingUniform {
    pub directional_light_count: u32,
    pub point_light_count: u32,
    pub spot_light_count: u32,
    pub ambient_color: [f32; 3],
    pub directional_lights: [DirectionalLightUniform; 4],
    pub point_lights: [PointLightUniform; 16],
    pub spot_lights: [SpotLightUniform; 8],
}
```

### 3. Update Pipeline Creation

Modify `create_pipeline()` in katla_vulkan to:
1. Add lighting uniform buffer descriptor set
2. Bind vertex and fragment shaders with lighting code
3. Configure push constants for camera position

### 4. Update Rendering Loop

In `render_frame()` or your draw call generation:
1. Get `LightCollection` from world
2. Copy light data to uniform buffer
3. Bind the lighting descriptor set before drawing

## API Reference

### Creating Lights

```rust
// Directional light (sun)
let sun = world.create_entity();
world.add_component(sun, DirectionalLight::new(
    Vec3::new(-0.3, -1.0, -0.2),  // direction
    [1.0, 0.95, 0.8],              // warm white color
    1.0,                           // intensity
));

// Point light (light bulb)
let bulb = world.create_entity();
world.add_component(bulb, TransformComponent {
    transform: Transform::new_from_position(Vec3::new(0.0, 2.0, 0.0)),
});
world.add_component(bulb, PointLight::new(
    [1.0, 0.9, 0.8],  // warm white
    10.0,             // intensity
    15.0,             // range
));

// Spot light (flashlight)
let flashlight = world.create_entity();
world.add_component(flashlight, TransformComponent {
    transform: Transform::new_from_position(Vec3::new(0.0, 0.0, 0.0)),
});
world.add_component(flashlight, SpotLight::new(
    [1.0, 1.0, 1.0],                  // white
    5.0,                               // intensity
    Vec3::new(0.0, 0.0, -1.0),         // pointing forward
    20.0,                              // range
    std::f32::consts::FRAC_PI_6,       // 30 degree cone
));

// Ambient light (global, one per scene)
world.insert_resource(AmbientLight::gray(0.15));
```

### Accessing Light Data

```rust
// In a system
fn update(&mut self, world: &mut World, _dt: f32) {
    if let Some(lights) = world.get_resource::<LightCollection>() {
        println!("There are {} lights in the scene", lights.total_lights());
        println!("  {} directional", lights.directional_lights.len());
        println!("  {} point", lights.point_lights.len());
        println!("  {} spot", lights.spot_lights.len());
    }
}
```

## Performance Considerations

- **Light limits enforced**: Prevents shader from becoming too complex
- **Spatial partitioning**: For many lights, consider using tiled/clustered forward rendering
- **Shadow mapping**: Not implemented yet; would require shadow map atlases
- **Light culling**: Currently all lights are passed to shader; could optimize by only sending lights affecting each object

## Future Enhancements

1. **Shadow Mapping** - Directional/spot light shadows
2. **Light Probes** - Baked global illumination
3. **IES Profiles** - Real-world light intensity distributions
4. **Volumetric Lighting** - Light shafts, god rays
5. **Deferred Rendering** - Handle many more lights
6. **Light Cookies** - Projected textures for spotlights
7. **Light Animation** - Pulsing, flickering, color cycling

## Files Modified

- `katla_app/src/components/lighting.rs` - NEW
- `katla_app/src/components/mod.rs` - Export lighting components
- `katla_app/src/systems/lighting_system.rs` - NEW
- `katla_app/src/systems/mod.rs` - Export lighting system
- `katla_app/src/systems/lighting_system_tests.rs` - NEW
- `katla_app/src/main.rs` - Register LightingSystem
- `katla_app/src/application/mod.rs` - Add lights to scene

## Next Steps

To make lighting visible, you need to work on the Vulkan/shader side:

1. Create a shader with Phong lighting calculations
2. Add lighting uniform buffers to `katla_vulkan`
3. Update pipeline creation to bind lighting resources
4. Upload light data each frame in `render_frame()`

This requires familiarity with Vulkan descriptor sets, uniform buffers, and GLSL shader programming.

Would you like me to help with any of these steps?
