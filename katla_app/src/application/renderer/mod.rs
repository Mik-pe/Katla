//! Renderer subsystem - handles render graph setup and frame rendering.

pub mod render_graph;

use log::{debug, error, info};

use katla_vulkan::{
    DrawCall, DrawList, FrameUniforms, IndexBuffer, IndexType, MaterialHandle, MeshHandle,
    ParticleDispatch, ParticleRender, VertexBuffer, VulkanContext,
};
use std::rc::Rc;

use crate::animation::Skeleton;
use crate::components::{DrawableComponent, ParticleEmitter, TransformComponent};
use crate::gizmo::{self, GizmoVertex};
use crate::rendering::GizmoMaterial;

use super::Application;

/// UI vertex buffer size in bytes (256KB - enough for complex UIs)
const UI_VERTEX_BUFFER_SIZE: usize = 256 * 1024;
/// UI index buffer size in bytes (128KB - enough for complex UIs)
const UI_INDEX_BUFFER_SIZE: usize = 128 * 1024;
/// Font atlas texture size (width and height in pixels)
const FONT_ATLAS_SIZE: u32 = 512;

/// Gizmo rendering resources stored in the application.
pub struct GizmoResources {
    pub mesh_handle: MeshHandle,
    pub material_handle: MaterialHandle,
}

/// Setup the render graph with multiple framebuffers (one per swapchain image).
/// This creates the graph upfront during initialization to avoid
/// destroying Vulkan objects while the GPU is still using them.
pub fn setup_render_graph(app: &mut Application) {
    let renderer = match app.renderer.as_mut() {
        Some(r) => r,
        None => return,
    };

    // Create fullscreen renderer (owns sky/grid pipelines internally)
    let fullscreen_renderer = {
        let mut cache = renderer.material_cache.borrow_mut();
        crate::rendering::FullscreenRenderer::new(&mut cache)
    };

    // Get pipelines from fullscreen renderer for render graph
    let sky_pipeline = fullscreen_renderer.sky_pipeline();
    let grid_pipeline = fullscreen_renderer.grid_pipeline(app.editor_ui.show_grid);

    // Store fullscreen renderer in app
    app.fullscreen_renderer = Some(fullscreen_renderer);

    // Create UI renderer (owns UI pipeline internally)
    let ui_renderer = crate::rendering::UIRenderer::new(
        renderer.context.clone(),
        renderer.material_cache.clone(),
        UI_VERTEX_BUFFER_SIZE as u64,
        UI_INDEX_BUFFER_SIZE as u64,
        FONT_ATLAS_SIZE,
        FONT_ATLAS_SIZE,
    )
    .expect("Failed to create UI renderer");
    app.ui_renderer = Some(ui_renderer);

    // Get window size for main viewport
    let viewport_size = app.window.as_ref().unwrap().inner_size();

    // Initialize main viewport using new unified ViewportBuilder API
    let main_builder = renderer
        .create_viewport()
        .size(viewport_size.width, viewport_size.height)
        .with_depth(katla_vulkan::DepthFormat::D32SfloatS8Uint)
        .clear_color(0.3, 0.5, 0.3, 1.0) // Dark green
        .label("main");

    let main_viewport = renderer
        .build_viewport(main_builder)
        .expect("Failed to create main viewport");

    // Store viewport handle in app for later use
    app.main_viewport = Some(main_viewport);

    // Register main viewport texture with UI renderer for sampling
    if let (Some(tex_id), Some(color_view)) = (
        renderer.viewport_texture_id(main_viewport),
        renderer.get_viewport_color_view(main_viewport),
    ) {
        // Register with UI renderer so it can sample the viewport texture
        // Use TextureId::custom() to get the same ID format the UI will use in draw commands
        let texture_id = katla_ui::TextureId::custom(tex_id);
        if let Some(ref mut ui_renderer) = app.ui_renderer {
            ui_renderer.register_texture(texture_id.0, color_view);
        }
        app.editor_ui.main_viewport_texture_id = texture_id;
        debug!(
            "Registered main viewport texture {} (raw: {}) with UI renderer",
            texture_id.0, tex_id
        );
    }

    // Initialize output render target for final UI composition
    // This is where UI renders, then present_pass copies to swapchain
    renderer
        .init_output_target(viewport_size.width, viewport_size.height)
        .expect("Failed to initialize output render target");

    // Set camera aspect ratio based on viewport texture size (not window size!)
    if let Some(extent) = renderer.get_viewport_extent(main_viewport) {
        let aspect = extent.width as f32 / extent.height as f32;
        app.camera
            .borrow_mut()
            .aspect_ratio_changed(&mut app.world, aspect);
    }

    // Setup render graph using the new application-layer API
    // Pass all pipelines at setup time - render graph stores them internally
    // UI callback is set at runtime via set_ui_callback() before each frame
    render_graph::build_render_graph(renderer, sky_pipeline, grid_pipeline);

    // Initialize preview viewport using new unified ViewportBuilder API
    let preview_builder = renderer
        .create_viewport()
        .size(512, 512)
        .with_depth(katla_vulkan::DepthFormat::D32SfloatS8Uint)
        .clear_color(0.15, 0.15, 0.18, 1.0) // Dark gray
        .label("preview");

    let preview_viewport = renderer
        .build_viewport(preview_builder)
        .expect("Failed to create preview viewport");

    // Store viewport handle in app for later use
    app.preview_viewport = Some(preview_viewport);

    // Register preview viewport texture with UI renderer for sampling
    if let (Some(tex_id), Some(color_view)) = (
        renderer.viewport_texture_id(preview_viewport),
        renderer.get_viewport_color_view(preview_viewport),
    ) {
        // Register with UI renderer so it can sample the preview texture
        // Use TextureId::custom() to get the same ID format the UI will use in draw commands
        let texture_id = katla_ui::TextureId::custom(tex_id);
        if let Some(ref mut ui_renderer) = app.ui_renderer {
            ui_renderer.register_texture(texture_id.0, color_view);
        }
        debug!(
            "Registered preview viewport texture {} (raw: {}) with UI renderer",
            texture_id.0, tex_id
        );
    }

    // Create and register gizmo material and mesh
    setup_gizmo_resources(app);
}

/// Setup gizmo rendering resources.
fn setup_gizmo_resources(app: &mut Application) {
    let renderer = match app.renderer.as_mut() {
        Some(r) => r,
        None => return,
    };
    let context = renderer.context.clone();

    // Create gizmo pipeline from pure config using the renderer's cache
    let gizmo_pipeline = {
        let mut material_cache = renderer.material_cache.borrow_mut();
        let gizmo_material = GizmoMaterial::default();
        material_cache
            .get_or_create(&gizmo_material)
            .expect("Failed to create gizmo pipeline")
    };

    // Register material using VulkanRenderer's method
    // Gizmo doesn't use bindless textures
    let vertex_binding = gizmo::gizmo_vertex_binding();
    let material_handle = renderer.register_material_full(
        gizmo_pipeline,
        vertex_binding,
        false, // not bindless
        [0; 4],
        0,
    );
    debug!(
        "Registered gizmo material with handle {:?}",
        material_handle
    );

    // Create gizmo mesh
    let (vertices, indices) = gizmo::generate_translate_gizmo(1.0);

    // Create vertex buffer
    let vertex_buffer = create_gizmo_vertex_buffer(&context, vertices);

    // Create index buffer
    let index_buffer = create_gizmo_index_buffer(&context, indices);

    // Register mesh using VulkanRenderer's method
    let mesh_handle = renderer.register_mesh(Some(vertex_buffer), Some(index_buffer));
    debug!("Registered gizmo mesh with handle {:?}", mesh_handle);

    // Store in application
    app.gizmo_resources = Some(GizmoResources {
        mesh_handle,
        material_handle,
    });
}

/// Create a vertex buffer for gizmo geometry.
fn create_gizmo_vertex_buffer(
    context: &Rc<VulkanContext>,
    vertices: Vec<GizmoVertex>,
) -> VertexBuffer {
    let data_slice = unsafe {
        std::slice::from_raw_parts(
            vertices.as_ptr() as *const u8,
            vertices.len() * std::mem::size_of::<GizmoVertex>(),
        )
    };

    let count = vertices.len() as u32;
    let mut vertex_buffer = VertexBuffer::new(context.clone(), data_slice.len() as u64, count);
    vertex_buffer.upload_data(data_slice);
    vertex_buffer
}

/// Create an index buffer for gizmo geometry.
fn create_gizmo_index_buffer(context: &Rc<VulkanContext>, indices: Vec<u32>) -> IndexBuffer {
    let data_slice = unsafe {
        std::slice::from_raw_parts(
            indices.as_ptr() as *const u8,
            indices.len() * std::mem::size_of::<u32>(),
        )
    };

    let count = (data_slice.len() as u32) / 4; // u32 indices
    let mut index_buffer = IndexBuffer::new(
        context.clone(),
        data_slice.len() as u64,
        IndexType::Uint32,
        count,
    );
    index_buffer.upload_data(data_slice);
    index_buffer
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
        light_direction: [0.3, 1.0, 0.2, 0.0], // Points UP toward sun (not down!)
        light_color: [1.0, 0.98, 0.95, 0.0],   // Warm white sunlight
        light_intensity: 3.0,                  // HDR intensity for PBR
    });

    // Check for material hot reload
    {
        let mut registry = renderer.material_registry.borrow_mut();
        let mut cache = renderer.material_cache.borrow_mut();
        if let Ok(reloaded) = registry.check_hot_reload(&renderer.context, &mut cache) {
            if reloaded > 0 {
                info!("Hot reloaded {} material template(s)", reloaded);
            }
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
            let mut draw_call = DrawCall::new(mesh_handle, material_handle)
                .with_transform(model_array)
                .with_pbr(drawable.metallic, drawable.roughness, drawable.ao);

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

    // Render gizmo for selected entity
    if let (Some(gizmo_res), Some(selected_entity)) =
        (&app.gizmo_resources, app.editor_ui.selected_entity)
    {
        if let Some(transform) = app
            .world
            .get_component::<TransformComponent>(selected_entity)
        {
            let model_matrix = transform.transform.make_mat4();
            let model_array: [f32; 16] = model_matrix.to_array();

            let gizmo_draw_call = DrawCall::new(gizmo_res.mesh_handle, gizmo_res.material_handle)
                .with_transform(model_array)
                .with_color([1.0, 1.0, 1.0, 1.0]);

            draw_list.push(gizmo_draw_call);
            debug!("Rendering gizmo for entity {:?}", selected_entity);
        }
    }

    // === PREVIEW RENDERING ===
    // Render model preview if active
    if app.editor_ui.model_preview.is_ready() {
        // Upload model to GPU if not already done (needs mutable borrow)
        {
            let preview = &app.editor_ui.model_preview;
            let needs_upload = !preview.has_gpu_resources();
            // Drop immutable borrow by exiting the block
            if needs_upload {
                let preview = &mut app.editor_ui.model_preview;
                preview.upload_to_gpu(
                    &renderer.context.clone(),
                    renderer,
                    &renderer.material_registry.clone(),
                );
            }
        }

        // Get preview state (immutable borrow is fine now)
        let preview = &app.editor_ui.model_preview;

        // Get handles
        if let (Some(mesh_handle), Some(material_handle)) =
            (preview.mesh_handle, preview.material_handle)
        {
            // Compute preview camera matrices
            let view = preview.camera.view_matrix();
            let proj = katla_math::Mat4::create_proj(
                45.0, // 45 degree FOV (in degrees)
                1.0,  // Square aspect ratio for preview
                0.1,
            );

            let view_proj = &proj * &view;
            let inv_view_proj = view_proj.inverse();

            // Extract camera position
            let cam_pos = preview.camera.position();

            // Update viewport camera using new unified API
            if let Some(viewport_handle) = app.preview_viewport {
                // Matrices are already column-major from to_array()
                renderer.update_viewport_camera(
                    viewport_handle,
                    &view.to_array(),
                    &proj.to_array(),
                    &inv_view_proj.to_array(),
                    &[cam_pos.x(), cam_pos.y(), cam_pos.z(), 0.0],
                    &[0.3, 1.0, 0.2, 0.0],
                    &[1.0, 0.98, 0.95, 0.0],
                    3.0,
                );

                // Create preview draw list
                let preview_draw = DrawCall::new(mesh_handle, material_handle)
                    .with_transform(katla_math::Mat4::identity().to_array());

                let mut preview_draw_list = DrawList::new();
                preview_draw_list.push(preview_draw);

                renderer.set_viewport_draw_list(viewport_handle, preview_draw_list);
            }
        }
    } else {
        // Clear preview draw list when not active
        if let Some(viewport_handle) = app.preview_viewport {
            renderer.clear_viewport_draw_list(viewport_handle);
        }
    }

    // === MAIN VIEWPORT RENDERING ===
    // The main render graph renders the scene (sky/grid/geometry) to viewport_color,
    // then composites to output_color, renders UI on top, and presents to swapchain.

    // Set UI callback for the render graph (UI pass will invoke this)
    if let Some(ref ui_renderer) = app.ui_renderer {
        let draw_data = app.ui_draw_data.borrow().clone();
        if let Some(draw_data) = draw_data {
            // Create a raw pointer to the UI renderer for the callback
            let ui_renderer_ptr: *const crate::rendering::UIRenderer = ui_renderer as *const _;
            let callback = std::rc::Rc::new(move |ctx: &katla_vulkan::PassExecutionContext| {
                // SAFETY: The UI renderer is valid for the lifetime of the frame
                unsafe {
                    (*ui_renderer_ptr).draw(ctx, &draw_data);
                }
            });
            renderer.set_ui_callback(callback);
        } else {
            // No UI data this frame - clear the callback to prevent stale rendering
            renderer.clear_ui_callback();
        }
    }

    // Pass the draw list and render
    let result = renderer.render_frame(draw_list);

    // Handle render errors
    if let Err(e) = result {
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
    // Convert Mat4 to GPU-friendly column-major [f32; 16] format
    fn mat4_to_array(matrix: &katla_math::Mat4) -> [f32; 16] {
        matrix.to_array()
    }

    // For each entity with a stored skeleton buffer, upload the transforms
    for (entity, buffer) in &app.skeleton_buffers {
        if let Some(skeleton) = app.world.get_component::<Skeleton>(*entity) {
            // Convert Mat4 joint transforms to GPU format
            let joint_matrices: Vec<[f32; 16]> = skeleton
                .joint_transforms
                .iter()
                .map(mat4_to_array)
                .collect();

            // Upload to GPU
            buffer.borrow_mut().upload(&joint_matrices);
        }
    }
}
