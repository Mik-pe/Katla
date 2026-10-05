//! Viewport reference grid.
//!
//! Generates thin line geometry on the XZ plane at world Y=0, following the
//! same draw-call pattern as the physics-debug overlays: lines are instances
//! of a shared shaft mesh, colored through `ObjectUniforms`.
//!
//! The grid is editor chrome, so it never casts shadows. Unlike the other
//! editor overlays it is depth-tested but does not write depth, so scene
//! geometry occludes it while it stays out of the depth buffer.

use katla_gfx::renderer::{DrawCall, InstanceData};
use katla_gfx::{DepthState, GpuRenderer, MaterialHandle, MeshHandle};
use katla_math::{Mat4, Quat, Vec3};

/// Grid extent in units on each side of the origin. The default scene's ground
/// plane is 20x20, so the grid ends exactly at the floor's edge instead of
/// hanging in space past it.
const GRID_HALF_EXTENT: f32 = 10.0;
/// Minor line spacing in units.
const MINOR_STEP: f32 = 1.0;
/// Every tenth line is drawn as a major (brighter) line.
const MAJOR_EVERY: i32 = 10;
/// Line thickness in units, so lines stay thin on screen.
const LINE_THICKNESS: f32 = 0.008;

/// Minor line color, linear RGBA. Low contrast by design: the grid stays out
/// of the way (ui-design-brief §7).
const MINOR_COLOR: [f32; 4] = [0.10, 0.11, 0.13, 1.0];
/// Major line color (every tenth line): slightly brighter so the ten-meter
/// rhythm is readable without competing with scene geometry.
const MAJOR_COLOR: [f32; 4] = [0.16, 0.17, 0.20, 1.0];

/// GPU resources for the reference grid.
pub struct GridResources {
    pub shaft_mesh: MeshHandle,
    pub material: MaterialHandle,
    pub initialized: bool,
}

impl Default for GridResources {
    fn default() -> Self {
        Self {
            shaft_mesh: MeshHandle::NONE,
            material: MaterialHandle::NONE,
            initialized: false,
        }
    }
}

/// Compile the grid material and create its line mesh.
///
/// The material is the shared unlit shader with depth testing enabled and
/// depth writing disabled: the grid is occluded by scene geometry but never
/// occludes anything itself.
pub fn init_grid_resources(
    renderer: &mut impl GpuRenderer,
    resources: &mut GridResources,
    unlit_shader_path: &std::path::Path,
) {
    let shaft_mesh = match katla_gfx::primitives::create_cylinder(renderer, 1.0, 1.0, 8) {
        Ok(mesh) => mesh,
        Err(error) => {
            log::error!("Failed to create grid line mesh: {error}");
            return;
        }
    };

    let descriptor =
        katla_gfx::PipelineDescriptor::pbr(unlit_shader_path.to_string_lossy().into_owned())
            .with_color_format(katla_gfx::ImageFormat::R16G16B16A16Sfloat)
            .with_depth(DepthState {
                test: true,
                write: false,
                compare: katla_gfx::CompareOp::GreaterOrEqual,
            });

    let material = match renderer.compile_material(&descriptor) {
        Ok(material) => material,
        Err(error) => {
            log::error!("Failed to create grid material: {error}");
            return;
        }
    };

    *resources = GridResources {
        shaft_mesh,
        material,
        initialized: true,
    };
}

/// Generate the editor grid draw calls at `ground_height`.
///
/// One draw per line color: every line is an instance of the shared shaft
/// mesh, so the whole grid costs two draw calls.
pub fn generate_grid_draws(resources: &GridResources, ground_height: f32) -> Vec<DrawCall> {
    if !resources.initialized {
        return Vec::new();
    }

    let mut minor = Vec::new();
    let mut major = Vec::new();
    let steps = (GRID_HALF_EXTENT / MINOR_STEP).round() as i32;

    for step in -steps..=steps {
        let offset = step as f32 * MINOR_STEP;
        let target = if step % MAJOR_EVERY == 0 {
            &mut major
        } else {
            &mut minor
        };

        // Line along X at z = offset, then along Z at x = offset.
        target.push(line_instance(
            Vec3::new(-GRID_HALF_EXTENT, ground_height, offset),
            Vec3::new(GRID_HALF_EXTENT, ground_height, offset),
        ));
        target.push(line_instance(
            Vec3::new(offset, ground_height, -GRID_HALF_EXTENT),
            Vec3::new(offset, ground_height, GRID_HALF_EXTENT),
        ));
    }

    let mut draws = Vec::new();
    if !minor.is_empty() {
        draws.push(instanced_grid_draw(resources, minor, MINOR_COLOR));
    }
    if !major.is_empty() {
        draws.push(instanced_grid_draw(resources, major, MAJOR_COLOR));
    }
    draws
}

fn instanced_grid_draw(
    resources: &GridResources,
    instances: Vec<InstanceData>,
    color: [f32; 4],
) -> DrawCall {
    let instances = instances
        .into_iter()
        .map(|instance| instance.with_color(color))
        .collect();
    DrawCall::instanced(resources.shaft_mesh, resources.material, instances)
}

/// Build the instance transform for one grid line.
///
/// The shared shaft mesh is a unit cylinder along +Y spanning `y = 0..1`, so
/// each line rotates that axis onto its direction, scales the length along
/// local Y and the perpendicular axes down to the line thickness, and offsets
/// the half-length so the mesh's base-span lands centered on the line.
fn line_instance(start: Vec3, end: Vec3) -> InstanceData {
    let mid = (start + end) * 0.5;
    let diff = end - start;
    let length = diff.length();
    let dir = if length < 1e-6 {
        Vec3::new(0.0, 1.0, 0.0)
    } else {
        diff * (1.0 / length)
    };

    let rotation = Quat::from_rotation_between(Vec3::new(0.0, 1.0, 0.0), dir);
    let center = mid - dir * (0.5 * length);
    let scale = Vec3::new(LINE_THICKNESS, length, LINE_THICKNESS);

    InstanceData::new().with_transform(Mat4::from_trs(center, rotation, scale).to_array())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn initialized_resources() -> GridResources {
        GridResources {
            shaft_mesh: MeshHandle::from_raw(1, 0),
            material: MaterialHandle::from_raw(1, 0),
            initialized: true,
        }
    }

    #[test]
    fn test_uninitialized_resources_generate_nothing() {
        let draws = generate_grid_draws(&GridResources::default(), 0.0);
        assert!(draws.is_empty());
    }

    #[test]
    fn test_grid_costs_two_draws_with_expected_instance_counts() {
        let resources = initialized_resources();
        let draws = generate_grid_draws(&resources, 0.0);

        assert_eq!(draws.len(), 2, "one draw per line color");
        // 21 lines per direction at unit spacing over a 20-unit extent = 42
        // lines. Three steps (-10, 0, 10) are majors, so they cover 6
        // instances; the remaining 18 lines cover 36.
        let mut instance_counts = draws
            .iter()
            .map(|draw| draw.instance_count())
            .collect::<Vec<_>>();
        assert_eq!(instance_counts.iter().sum::<u32>(), 42);
        instance_counts.sort_unstable();
        assert_eq!(instance_counts, vec![6, 36]);
    }

    #[test]
    fn test_grid_lines_are_planar_and_centered_on_the_line() {
        // A line from (-10,0,-1) to (10,0,-1): the shaft mesh spans local
        // Y=0..1, so the instance must be offset half a length back along the
        // line direction and scaled by the line length, landing both ends
        // exactly on the requested endpoints.
        let start = Vec3::new(-10.0, 0.0, -1.0);
        let end = Vec3::new(10.0, 0.0, -1.0);
        let instance = line_instance(start, end);
        let model = mat4_of(&instance);

        let base = model * Vec3::new(0.0, 0.0, 0.0);
        assert!((base.x() - start.x()).abs() < 1e-4, "base x: {}", base.x());
        assert!((base.z() - start.z()).abs() < 1e-4, "base z: {}", base.z());

        let tip = model * Vec3::new(0.0, 1.0, 0.0);
        assert!((tip.x() - end.x()).abs() < 1e-4, "tip x: {}", tip.x());
        assert!((tip.z() - end.z()).abs() < 1e-4, "tip z: {}", tip.z());

        // The perpendicular axes stay at line thickness, so lines read thin.
        let width = (model * Vec3::new(1.0, 0.0, 0.0)) - base;
        assert!((width.length() - LINE_THICKNESS).abs() < 1e-5);
    }

    #[test]
    fn test_grid_lies_on_the_requested_ground_height() {
        let resources = initialized_resources();
        let draws = generate_grid_draws(&resources, -1.0);

        for draw in &draws {
            for instance in &draw.instances {
                let model = mat4_of(instance);
                // The shaft mesh spans local Y = 0..1, so both ends of every
                // line must sit exactly on the ground height.
                let base = model * Vec3::new(0.0, 0.0, 0.0);
                let tip = model * Vec3::new(0.0, 1.0, 0.0);
                assert!((base.y() + 1.0).abs() < 1e-4, "base y: {}", base.y());
                assert!((tip.y() + 1.0).abs() < 1e-4, "tip y: {}", tip.y());
            }
        }
    }

    #[test]
    fn test_major_lines_are_brighter_than_minor_lines() {
        let minor_luma = MINOR_COLOR[0] + MINOR_COLOR[1] + MINOR_COLOR[2];
        let major_luma = MAJOR_COLOR[0] + MAJOR_COLOR[1] + MAJOR_COLOR[2];
        assert!(major_luma > minor_luma);
        // Both stay low contrast against the scene (ui-design-brief §7).
        assert!(major_luma < 0.6, "grid must not compete with geometry");
    }

    /// Rebuild a `Mat4` from an instance's flat column-major array.
    fn mat4_of(instance: &InstanceData) -> Mat4 {
        let m = instance.model_matrix;
        Mat4([
            katla_math::Vec4::new(m[0], m[1], m[2], m[3]),
            katla_math::Vec4::new(m[4], m[5], m[6], m[7]),
            katla_math::Vec4::new(m[8], m[9], m[10], m[11]),
            katla_math::Vec4::new(m[12], m[13], m[14], m[15]),
        ])
    }
}
