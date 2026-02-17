//! Renderer subsystem - handles render graph setup and frame rendering.

use log::{error, info};

use katla_vulkan::{DrawCall, DrawList, FrameUniforms, ParticleDispatch, ParticleRender};

use crate::animation::Skeleton;
use crate::components::{DrawableComponent, ParticleEmitter, TransformComponent};
use crate::rendering::SkyMaterial;

use super::Application;

/// Setup the render graph with multiple framebuffers (one per swapchain image).
/// This creates the graph upfront during initialization to avoid
/// destroying Vulkan objects while the GPU is still using them.
pub fn setup_render_graph(app: &mut Application) {
    let renderer = match app.renderer.as_mut() {
        Some(r) => r,
        None => return,
    };

    // Create and set up sky material for procedural sky background
    let sky_material = SkyMaterial::new(renderer.context.clone());
    renderer.set_sky_pipeline(sky_material.pipeline());

    // Create and set up UI material for overlay rendering
    let ui_material = crate::rendering::UiMaterial::new(renderer.context.clone());
    renderer.set_ui_pipeline(ui_material.pipeline());

    // Initialize UI buffers (256KB vertex, 128KB index - enough for complex UIs)
    renderer.init_ui_buffers(256 * 1024, 128 * 1024);

    // Initialize UI textures (512x512 font atlas)
    renderer
        .init_ui_textures(512, 512)
        .expect("Failed to initialize UI textures");

    // Initialize viewport render target for game engine editor
    // This creates an offscreen texture the UI can sample for the viewport panel
    let viewport_size = app.window.as_ref().unwrap().inner_size();
    renderer
        .init_viewport_target(viewport_size.width, viewport_size.height)
        .expect("Failed to initialize viewport render target");

    // Initialize output render target for final UI composition
    // This is where UI renders, then present_pass copies to swapchain
    renderer
        .init_output_target(viewport_size.width, viewport_size.height)
        .expect("Failed to initialize output render target");

    // Set camera aspect ratio based on viewport texture size (not window size!)
    if let Some(viewport_extent) = renderer.viewport_extent() {
        let aspect = viewport_extent.width as f32 / viewport_extent.height as f32;
        app.camera
            .borrow_mut()
            .aspect_ratio_changed(&mut app.world, aspect);
    }

    // Setup render graph with multiple framebuffers
    renderer.setup_render_graph();
}

/// Render using the render graph system with draw call submission.
pub fn render_frame(app: &mut Application) {
    let renderer = match app.renderer.as_mut() {
        Some(r) => r,
        None => return,
    };

    // Get camera matrices
    let view = app
        .camera
        .borrow()
        .get_view_mat(&app.world)
        .clone()
        .inverse();
    let proj = app.camera.borrow().get_proj_mat(&app.world).clone();

    // Compute inverse view-projection matrix for camera-relative sky
    // VP = proj * view, then inv_VP = VP.inverse()
    let view_proj = &proj * &view;
    let inv_view_proj = view_proj.inverse();

    // Extract camera position from inverse view matrix
    let view_arr: [[f32; 4]; 4] = view.clone().into();
    let cam_x = -(view_arr[0][0] * view_arr[3][0]
        + view_arr[0][1] * view_arr[3][1]
        + view_arr[0][2] * view_arr[3][2]);
    let cam_y = -(view_arr[1][0] * view_arr[3][0]
        + view_arr[1][1] * view_arr[3][1]
        + view_arr[1][2] * view_arr[3][2]);
    let cam_z = -(view_arr[2][0] * view_arr[3][0]
        + view_arr[2][1] * view_arr[3][1]
        + view_arr[2][2] * view_arr[3][2]);

    // Set frame uniforms once per frame (view/proj/lighting shared by all draws)
    renderer.set_frame_uniforms(FrameUniforms {
        view_matrix: view.to_array(),
        proj_matrix: proj.to_array(),
        inv_view_proj_matrix: inv_view_proj.to_array(),
        camera_position: [cam_x, cam_y, cam_z, 0.0],
        light_direction: [-0.3, -1.0, -0.2, 0.0],
        light_color: [1.0, 0.95, 0.9, 0.0],
        light_intensity: 1.5,
    });

    // Check for material hot reload
    if let Ok(reloaded) = renderer
        .material_registry
        .borrow_mut()
        .check_hot_reload(renderer.context.clone())
    {
        if reloaded > 0 {
            info!("Hot reloaded {} material template(s)", reloaded);
        }
    }

    // Build a draw list from ECS entities using the handle-based rendering system
    // This provides separation between high-level (DrawCall) and low-level (CommandBuffer) rendering
    let mut draw_list = DrawList::new();

    // Query all drawable entities
    for (_entity, transform, drawable) in app
        .world
        .query::<(&TransformComponent, &DrawableComponent)>()
    {
        // Get the model matrix and convert to array
        let model_matrix = transform.transform.make_mat4();
        let model_array: [f32; 16] = model_matrix.to_array();

        if let (Some(mesh_handle), Some(material_handle)) =
            (drawable.mesh_handle, drawable.material_handle)
        {
            let mut draw_call =
                DrawCall::new(mesh_handle, material_handle).with_transform(model_array);

            // Add color override if specified in DrawableComponent
            if let Some(color) = drawable.color {
                draw_call = draw_call.with_color(color.to_array());
            }

            // Add skeleton handle for GPU skeletal animation
            if let Some(skeleton) = drawable.skeleton_handle {
                draw_call = draw_call.with_skeleton(skeleton);
            }

            draw_list.push(draw_call);
        }
        // Entity missing mesh or material handle - skip
    }

    // Collect particle dispatches and renders from particle emitters
    let delta_time = app.timer.get_delta() as f32;

    // Get storage descriptor for frame uniforms (before mutable borrow)
    let storage_descriptor = renderer.storage_descriptor_set.as_ref().map(|s| s.set());

    for (_entity, emitter) in app.world.query::<&mut ParticleEmitter>() {
        // Update emitter (updates frame data buffer)
        emitter.update(delta_time);

        // Add compute dispatch for particle simulation
        let dispatch = ParticleDispatch {
            pipeline: emitter.compute_pipeline(),
            pipeline_layout: emitter.compute_layout(),
            descriptor_set: emitter.compute_descriptor(),
            frame_data: [0.0; 4], // Frame data is in uniform buffer
            workgroup_count: emitter.workgroup_count(),
        };
        draw_list.push_particle(dispatch);

        // Add particle render if we have the storage descriptor
        if let Some(frame_descriptor) = storage_descriptor {
            let render = ParticleRender {
                pipeline: emitter.render_pipeline(),
                pipeline_layout: emitter.render_layout(),
                frame_descriptor_set: frame_descriptor,
                particle_descriptor_set: emitter.render_particle_descriptor(),
                particle_count: emitter.particle_count(),
            };
            draw_list.push_particle_render(render);
        }
    }

    // Render using the draw list
    if let Err(e) = renderer.render_frame(draw_list) {
        match e {
            katla_vulkan::RenderGraphError::SwapchainOutOfDate => {
                // Swapchain is out of date (e.g., window resize), skip this frame
                // The swapchain will be recreated on the next frame
            }
            _ => {
                error!("Render frame failed: {:?}", e);
            }
        }
    }
}

/// Upload skeleton joint transforms to GPU buffers.
///
/// This is called after world.update() which runs SkeletalAnimationSystem
/// to compute the joint transforms.
pub fn upload_skeleton_transforms(app: &mut Application) {
    // Convert Mat4 to GPU-friendly [[f32; 4]; 4] format
    fn mat4_to_array(matrix: &katla_math::Mat4) -> [[f32; 4]; 4] {
        let data: [[f32; 4]; 4] = matrix.clone().into();
        data
    }

    // For each entity with a stored skeleton buffer, upload the transforms
    for (entity, buffer) in &app.skeleton_buffers {
        if let Some(skeleton) = app.world.get_component::<Skeleton>(*entity) {
            // Convert Mat4 joint transforms to GPU format
            let joint_matrices: Vec<[[f32; 4]; 4]> = skeleton
                .joint_transforms
                .iter()
                .map(mat4_to_array)
                .collect();

            // Upload to GPU
            buffer.borrow_mut().upload(&joint_matrices);
        }
    }
}
