//! Graphics contracts: indexed draw widths, per-object transform data,
//! instancing, dynamic mesh transitions, and material reuse across graph
//! configurations.
//!
//! Every scenario here compiles PBR pipelines, so it opens the renderer
//! without Vulkan-layer validation capture (driver caveat in the harness
//! docs). Contracts assert readback pixels — whole-frame scans and
//! left/right probes where possible, so a vertical flip cannot flip the
//! verdict.

use katla_gfx::render_graph::any_frame_graph::AnyFrameGraph;
use katla_gfx::render_graph::{GeometryPass, PassId, UIPass};
use katla_gfx::render_pass::{AttachmentOps, ClearValue};
use katla_gfx::renderer::{DrawCall, DrawList, InstanceData};
use katla_gfx::texture::ImageFormat;
use katla_gfx::vertex::{VertexPBR, VertexUIInstance};
use katla_gfx::{
    FrameUniforms, GpuRenderer, IndexType, MaterialHandle, MeshDescriptor, MeshHandle, MeshUsage,
    PrimitiveTopology, UIDrawList, UiDrawCommand, Vertex,
};

use crate::harness::{self, CHANNEL_BLUE, CHANNEL_GREEN, CHANNEL_RED, pixel_offset};
use harness::ContractRenderer;

/// Scenarios that compile PBR pipelines. They run on every backend locally
/// and in the macOS CI job, but the Linux CI step skips the whole module:
/// lavapipe segfaults inside the PBR pipeline path (driver-side; the same
/// suite passes on real Intel Vulkan hardware).
mod pbr {
    use super::*;

    /// Shared setup for PBR scenarios: renderer, frame pipelines, shadow
    /// layouts, the contract PBR material, a center triangle, and a
    /// black-clear graph.
    struct GeometryScenario {
        renderer: ContractRenderer,
        material: MaterialHandle,
        mesh: MeshHandle,
        graph: AnyFrameGraph,
        geometry_pass: PassId,
        uniforms: FrameUniforms,
    }

    fn geometry_scenario(label: &str) -> GeometryScenario {
        let mut renderer = ContractRenderer::open_without_api_validation(label);
        renderer.init_frame_pipelines();
        renderer.init_shadow_layouts();
        let material = harness::compile_pbr_material(renderer.gfx());
        let vertices = harness::clip_triangle();
        let mesh = renderer
            .gfx()
            .create_mesh(&vertices, &[0u32, 1, 2], PrimitiveTopology::TriangleList)
            .expect("contract triangle mesh");
        let graph = harness::build_graph(|builder| {
            builder.add_pass(
                GeometryPass::new("geometry")
                    .without_depth()
                    .write_color("backbuffer", ImageFormat::B8G8R8A8Srgb)
                    .clear_color([0.0, 0.0, 0.0, 1.0])
                    .material(material),
            )
        });
        let geometry_pass = harness::pass_id(&graph, "geometry");
        GeometryScenario {
            renderer,
            material,
            mesh,
            graph,
            geometry_pass,
            uniforms: harness::clip_uniforms(),
        }
    }

    fn draw_list_of(calls: Vec<DrawCall>) -> DrawList {
        let mut list = DrawList::new();
        for call in calls {
            list.push(call);
        }
        list
    }

    /// Submit one draw list to the scenario's own graph and capture the frame.
    fn render_list(scenario: &mut GeometryScenario, list: &DrawList) -> Vec<u8> {
        let GeometryScenario {
            renderer,
            graph,
            geometry_pass,
            uniforms,
            ..
        } = scenario;
        renderer.render_frame(graph, Some(&*uniforms), Some(list), |frame| {
            frame.submit(*geometry_pass, list);
        })
    }

    /// Submit one draw list to an alternate graph and capture the frame.
    fn render_list_into(
        scenario: &mut GeometryScenario,
        graph: &mut AnyFrameGraph,
        pass: PassId,
        list: &DrawList,
    ) -> Vec<u8> {
        let uniforms = scenario.uniforms.clone();
        scenario
            .renderer
            .render_frame(graph, Some(&uniforms), Some(list), |frame| {
                frame.submit(pass, list);
            })
    }

    /// The indexed-draw width must not change what reaches the screen: the same
    /// triangle uploaded with u16 and u32 indices renders byte-identically, frame
    /// after frame, including frame-slot recycling between captures.
    #[test]
    #[ignore = "requires a graphics device"]
    fn test_contract_indexed_draw_widths_render_identically() {
        let mut scenario = geometry_scenario("contract: indexed draw widths");
        let vertices = harness::clip_triangle();
        let mesh_u16 = scenario
            .renderer
            .gfx()
            .create_mesh(&vertices, &[0u16, 1, 2], PrimitiveTopology::TriangleList)
            .expect("u16 contract mesh");
        let mesh_u32 = scenario.mesh;
        if scenario.renderer.caps().preserves_index_width {
            assert_eq!(
                scenario.renderer.gfx().mesh_index_format(mesh_u16),
                Some(IndexType::Uint16)
            );
            assert_eq!(
                scenario.renderer.gfx().mesh_index_format(mesh_u32),
                Some(IndexType::Uint32)
            );
        } else {
            // Metal normalizes both widths to u32 at upload; the created mesh
            // must still report a live recorded format.
            assert!(
                scenario
                    .renderer
                    .gfx()
                    .mesh_index_format(mesh_u16)
                    .is_some()
            );
            assert!(
                scenario
                    .renderer
                    .gfx()
                    .mesh_index_format(mesh_u32)
                    .is_some()
            );
        }

        let center = pixel_offset(0.0, 0.0);
        let corner = pixel_offset(-0.9, -0.9);
        let mut captured = Vec::new();
        for mesh in [mesh_u16, mesh_u32, mesh_u16, mesh_u32] {
            let list = draw_list_of(vec![DrawCall::new(mesh, scenario.material)]);
            let pixels = render_list(&mut scenario, &list);
            assert_ne!(
                &pixels[center..center + 4],
                &pixels[corner..corner + 4],
                "the triangle must be rasterized"
            );
            assert_eq!(pixels[center + 3], 255, "the triangle must be opaque");
            captured.push(pixels);
        }
        for (frame, pixels) in captured.iter().enumerate().skip(1) {
            assert_eq!(
                pixels, &captured[0],
                "u16 and u32 renders must match (frame {frame})"
            );
        }
        harness::cleanup_graph(scenario.graph);
        scenario.renderer.finish();
    }

    /// Per-object data must follow its draw: two draws of one mesh with different
    /// model matrices and tint colors cover disjoint halves of the frame, and
    /// swapping the transforms swaps the pixels.
    #[test]
    #[ignore = "requires a graphics device"]
    fn test_contract_object_transforms_reach_each_draw() {
        let mut scenario = geometry_scenario("contract: object transforms");
        let left_probe = pixel_offset(-0.5, 0.0);
        let right_probe = pixel_offset(0.5, 0.0);
        let center_probe = pixel_offset(0.0, 0.0);

        for swap in [false, true] {
            let (left, right) = if swap {
                ([0.5, 0.0], [-0.5, 0.0])
            } else {
                ([-0.5, 0.0], [0.5, 0.0])
            };
            let list = draw_list_of(vec![
                DrawCall::new(scenario.mesh, scenario.material)
                    .with_transform(harness::translation(left))
                    .with_color([1.0, 0.0, 0.0, 1.0]),
                DrawCall::new(scenario.mesh, scenario.material)
                    .with_transform(harness::translation(right))
                    .with_color([0.0, 1.0, 0.0, 1.0]),
            ]);
            let pixels = render_list(&mut scenario, &list);
            let (expected_left, expected_right) = if swap {
                (CHANNEL_GREEN, CHANNEL_RED)
            } else {
                (CHANNEL_RED, CHANNEL_GREEN)
            };
            assert_eq!(
                harness::dominant_channel(&pixels, left_probe),
                Some(expected_left),
                "swap={swap}: the left draw must carry its own transform and color"
            );
            assert_eq!(
                harness::dominant_channel(&pixels, right_probe),
                Some(expected_right),
                "swap={swap}: the right draw must carry its own transform and color"
            );
            assert_eq!(
                harness::dominant_channel(&pixels, center_probe),
                None,
                "swap={swap}: no draw may cover the gap between the transforms"
            );
        }
        harness::cleanup_graph(scenario.graph);
        scenario.renderer.finish();
    }

    /// One instanced draw of four instances must render byte-identically to four
    /// direct draws carrying the same per-object data, in any list order, with
    /// rewritten instance data reaching the GPU and capacity exhaustion failing
    /// typed instead of overwriting.
    #[test]
    #[ignore = "requires a graphics device"]
    fn test_contract_instanced_draw_matches_direct_draws() {
        let mut scenario = geometry_scenario("contract: instanced draws");
        let instance = |i: usize| {
            InstanceData::default()
                .with_transform(instance_transform(i))
                .with_color(INSTANCE_COLORS[i])
        };
        fn direct_call(
            mesh: katla_gfx::MeshHandle,
            material: katla_gfx::MaterialHandle,
            i: usize,
        ) -> DrawCall {
            DrawCall::new(mesh, material)
                .with_transform(instance_transform(i))
                .with_color(INSTANCE_COLORS[i])
        }
        let quadrant_probe = |i: usize| pixel_offset(QUADRANTS[i][0], QUADRANTS[i][1] - 1.0 / 6.0);

        let instanced = draw_list_of(vec![DrawCall::instanced(
            scenario.mesh,
            scenario.material,
            (0..4).map(instance).collect(),
        )]);
        let instanced_pixels = render_list(&mut scenario, &instanced);
        for (i, tint) in INSTANCE_COLORS.iter().enumerate() {
            let probe = quadrant_probe(i);
            let bgra = &instanced_pixels[probe..probe + 4];
            // Quadrant 3's tint (yellow) lights two channels; the rest are pure.
            let matches_own_color = if tint[0] > 0.5 && tint[1] > 0.5 {
                bgra[2] > 120 && bgra[1] > 120 && bgra[0] < 120
            } else {
                harness::dominant_channel(&instanced_pixels, probe)
                    == Some(dominant_channel_of(*tint))
            };
            assert!(
                matches_own_color,
                "quadrant {i} must show its own instance's color, got {bgra:?}"
            );
        }

        let direct = draw_list_of(
            (0..4)
                .map(|i| direct_call(scenario.mesh, scenario.material, i))
                .collect(),
        );
        let direct_pixels = render_list(&mut scenario, &direct);
        assert_eq!(
            instanced_pixels, direct_pixels,
            "one instanced draw must render identically to four direct draws"
        );

        let mut mixed = draw_list_of(vec![direct_call(scenario.mesh, scenario.material, 0)]);
        mixed.push(DrawCall::instanced(
            scenario.mesh,
            scenario.material,
            (1..4).map(instance).collect(),
        ));
        let mixed_pixels = render_list(&mut scenario, &mixed);
        assert_eq!(mixed_pixels, instanced_pixels, "mixed list must match");

        let mut instances: Vec<InstanceData> = (0..4).map(instance).collect();
        instances[3].color = [0.0, 1.0, 1.0, 1.0];
        let recolored = draw_list_of(vec![DrawCall::instanced(
            scenario.mesh,
            scenario.material,
            instances,
        )]);
        let recolored_pixels = render_list(&mut scenario, &recolored);
        let px3 = quadrant_probe(3);
        assert_ne!(
            &recolored_pixels[px3..px3 + 4],
            &instanced_pixels[px3..px3 + 4],
            "the rewritten late instance color must reach the GPU"
        );

        let oversized = draw_list_of(
            (0..300)
                .map(|_| DrawCall::new(scenario.mesh, scenario.material))
                .collect(),
        );
        scenario.renderer.gfx().wait_for_frame().expect("idle");
        let error = scenario
            .renderer
            .gfx()
            .execute_draw_calls(&oversized)
            .expect_err("a draw range past the per-frame object limit must fail");
        assert!(
            error.to_string().contains("MAX_OBJECTS_PER_FRAME"),
            "capacity exhaustion must name the limit, got {error}"
        );

        harness::cleanup_graph(scenario.graph);
        scenario.renderer.finish();
    }

    fn instance_transform(i: usize) -> [f32; 16] {
        // Column-major T * S: scale columns, translation in column 3. The
        // 0.5-scaled triangle spans ±0.25 around its centroid, inside its quadrant.
        let offset = QUADRANTS[i];
        let scale = 0.5;
        [
            scale, 0.0, 0.0, 0.0, //
            0.0, scale, 0.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            offset[0], offset[1], 0.0, 1.0,
        ]
    }

    const QUADRANTS: [[f32; 2]; 4] = [[0.5, 0.6], [-0.5, 0.6], [-0.5, -0.6], [0.5, -0.6]];

    const INSTANCE_COLORS: [[f32; 4]; 4] = [
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 1.0, 0.0, 1.0],
        [0.0, 0.0, 1.0, 1.0],
        [1.0, 1.0, 0.0, 1.0],
    ];

    /// Which BGRA channel a linear test tint ends up dominating.
    fn dominant_channel_of(tint: [f32; 4]) -> usize {
        if tint[0] > 0.5 {
            CHANNEL_RED
        } else if tint[1] > 0.5 {
            CHANNEL_GREEN
        } else {
            CHANNEL_BLUE
        }
    }

    /// A dynamic mesh must stay internally consistent across same-size updates,
    /// shrink, grow past capacity (retiring replaced buffers), the empty
    /// transition, and repopulation — while rendering exactly its current
    /// geometry.
    #[test]
    #[ignore = "requires a graphics device"]
    fn test_contract_dynamic_mesh_lifecycle_transitions() {
        let mut scenario = geometry_scenario("contract: dynamic mesh lifecycle");

        let mesh = {
            let (vertices, indices) = half_geometry(Half::Left);
            let descriptor = MeshDescriptor {
                layout: VertexPBR::layout(),
                attributes: VertexPBR::attribute_kinds(),
                topology: PrimitiveTopology::TriangleList,
                usage: MeshUsage::Dynamic,
                vertex_count: vertices.len() as u32,
                index_count: indices.len() as u32,
                index_format: IndexType::Uint32,
            };
            scenario
                .renderer
                .gfx()
                .create_mesh_dynamic(&descriptor, bytemuck::cast_slice(&vertices), &indices)
                .expect("dynamic contract mesh")
        };

        let update_to =
            |scenario: &mut GeometryScenario, vertices: &[VertexPBR], indices: &[u32]| {
                scenario
                    .renderer
                    .gfx()
                    .update_mesh_dynamic(
                        mesh,
                        bytemuck::cast_slice(vertices),
                        vertices.len() as u32,
                        indices,
                    )
                    .expect("dynamic mesh update");
            };
        let frame_with = |scenario: &mut GeometryScenario| {
            let list = draw_list_of(vec![
                DrawCall::new(mesh, scenario.material).with_color([1.0, 0.0, 0.0, 1.0]),
            ]);
            render_list(scenario, &list)
        };

        let left_probe = pixel_offset(-0.5, 0.0);
        let right_probe = pixel_offset(0.5, 0.0);
        // The shrunk geometry is the lower-left triangle of the left half; this
        // probe stays strictly inside it (the center-row probe sits on its edge).
        let shrunk_probe = pixel_offset(-0.75, -0.25);

        let pixels = frame_with(&mut scenario);
        assert_eq!(
            harness::dominant_channel(&pixels, left_probe),
            Some(CHANNEL_RED),
            "the created left-half geometry must render"
        );
        assert_eq!(
            harness::dominant_channel(&pixels, right_probe),
            None,
            "the right half must stay background before growth"
        );

        // Same-size update with rewritten vertex data (z pushed toward the camera).
        {
            let (mut vertices, indices) = half_geometry(Half::Left);
            for vertex in &mut vertices {
                vertex.position[2] = 0.25;
            }
            update_to(&mut scenario, &vertices, &indices);
        }
        assert_eq!(scenario.renderer.gfx().mesh_vertex_count(mesh), Some(6));
        let pixels = frame_with(&mut scenario);
        assert_eq!(
            harness::dominant_channel(&pixels, left_probe),
            Some(CHANNEL_RED),
            "the rewritten same-size geometry must render"
        );

        // Shrink to one triangle: counts track down, no reallocation, the rest of
        // the frame reverts to background.
        let (shrunk, shrunk_indices) = {
            let (vertices, _) = half_geometry(Half::Left);
            (vertices[..3].to_vec(), vec![0u32, 1, 2])
        };
        update_to(&mut scenario, &shrunk, &shrunk_indices);
        assert_eq!(scenario.renderer.gfx().mesh_vertex_count(mesh), Some(3));
        assert_eq!(scenario.renderer.gfx().mesh_index_count(mesh), Some(3));
        let pixels = frame_with(&mut scenario);
        assert_eq!(
            harness::dominant_channel(&pixels, shrunk_probe),
            Some(CHANNEL_RED),
            "the shrunk triangle must render"
        );

        // Grow past the created capacity to the whole frame: replaced buffers
        // retire instead of freeing under in-flight submissions, and drain once
        // their frames completed.
        let (whole, whole_indices) = whole_geometry();
        update_to(&mut scenario, &whole, &whole_indices);
        assert_eq!(scenario.renderer.gfx().mesh_vertex_count(mesh), Some(12));
        let pixels = frame_with(&mut scenario);
        assert_eq!(
            harness::dominant_channel(&pixels, left_probe),
            Some(CHANNEL_RED)
        );
        assert_eq!(
            harness::dominant_channel(&pixels, right_probe),
            Some(CHANNEL_RED),
            "the grown half must render"
        );

        // Populated -> empty -> populated round trip.
        update_to(&mut scenario, &[], &[]);
        assert_eq!(scenario.renderer.gfx().mesh_vertex_count(mesh), Some(0));
        let pixels = frame_with(&mut scenario);
        assert!(
            harness::no_pixel_dominates(&pixels, CHANNEL_RED),
            "an empty dynamic mesh must draw nothing"
        );
        update_to(&mut scenario, &shrunk, &shrunk_indices);
        let pixels = frame_with(&mut scenario);
        assert_eq!(
            harness::dominant_channel(&pixels, shrunk_probe),
            Some(CHANNEL_RED),
            "the repopulated mesh must render again"
        );
        assert_eq!(
            scenario.renderer.gfx().mesh_index_format(mesh),
            Some(IndexType::Uint32),
            "the recorded index width survives every update"
        );

        if scenario.renderer.caps().retirement_diagnostics {
            let mut snapshot = scenario
                .renderer
                .pending_retirements()
                .expect("snapshots available on this backend");
            for _ in 0..8 {
                if snapshot.total() == 0 {
                    break;
                }
                frame_with(&mut scenario);
                snapshot = scenario
                    .renderer
                    .pending_retirements()
                    .expect("snapshots available on this backend");
            }
            assert_eq!(
                snapshot.total(),
                0,
                "retirements must drain once their frames completed: {}",
                snapshot.summary()
            );
        }

        harness::cleanup_graph(scenario.graph);
        scenario.renderer.finish();
    }

    /// Which half of the frame a geometry helper covers.
    #[derive(Clone, Copy)]
    enum Half {
        Left,
        Right,
    }

    /// Two full-height triangles covering one half of the frame. Full-height
    /// geometry keeps every probe robust to a vertical readback flip.
    fn half_geometry(half: Half) -> (Vec<VertexPBR>, Vec<u32>) {
        let (outer, inner) = match half {
            Half::Left => (-1.0f32, 0.0),
            Half::Right => (1.0f32, 0.0),
        };
        let vertices = vec![
            vertex([outer, -1.0, 0.5]),
            vertex([inner, -1.0, 0.5]),
            vertex([outer, 1.0, 0.5]),
            vertex([inner, -1.0, 0.5]),
            vertex([inner, 1.0, 0.5]),
            vertex([outer, 1.0, 0.5]),
        ];
        (vertices, vec![0, 1, 2, 3, 4, 5])
    }

    /// Both halves: four triangles covering the whole frame.
    fn whole_geometry() -> (Vec<VertexPBR>, Vec<u32>) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        for half in [Half::Left, Half::Right] {
            let (half_vertices, _) = half_geometry(half);
            let base = vertices.len() as u32;
            vertices.extend(half_vertices);
            indices.extend([base, base + 1, base + 2, base + 3, base + 4, base + 5]);
        }
        (vertices, indices)
    }

    fn vertex(position: [f32; 3]) -> VertexPBR {
        VertexPBR {
            position,
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            tex_coord0: [0.0, 0.0],
        }
    }

    /// One material must keep rendering correctly when the same geometry drives
    /// three graph configurations in alternation: pipelines and per-pass state
    /// must not bleed across graphs, and repeated uses must hit the compiled
    /// pipeline deterministically.
    #[test]
    #[ignore = "requires a graphics device"]
    fn test_contract_material_renders_consistently_across_graph_configs() {
        let mut scenario = geometry_scenario("contract: material across graph configs");
        let material = scenario.material;
        let mut dark_graph = harness::build_graph(|builder| {
            builder.add_pass(
                GeometryPass::new("geometry")
                    .without_depth()
                    .write_color("backbuffer", ImageFormat::B8G8R8A8Srgb)
                    .clear_color([0.0, 0.0, 0.0, 1.0])
                    .material(material),
            )
        });
        let mut blue_graph = harness::build_graph(|builder| {
            builder.add_pass(
                GeometryPass::new("geometry")
                    .without_depth()
                    .write_color("backbuffer", ImageFormat::B8G8R8A8Srgb)
                    .clear_color([0.0, 0.0, 0.5, 1.0])
                    .material(material),
            )
        });
        let dark_pass = harness::pass_id(&dark_graph, "geometry");
        let blue_pass = harness::pass_id(&blue_graph, "geometry");

        let list = draw_list_of(vec![
            DrawCall::new(scenario.mesh, material).with_color([1.0, 0.0, 0.0, 1.0]),
        ]);
        let center = pixel_offset(0.0, 0.0);
        let corner = pixel_offset(-0.9, -0.9);

        for round in 0..2 {
            for (config, expect_blue) in [
                (Config::Base, false),
                (Config::Dark, false),
                (Config::Blue, true),
            ] {
                let pixels = match config {
                    Config::Base => render_list(&mut scenario, &list),
                    Config::Dark => {
                        render_list_into(&mut scenario, &mut dark_graph, dark_pass, &list)
                    }
                    Config::Blue => {
                        render_list_into(&mut scenario, &mut blue_graph, blue_pass, &list)
                    }
                };
                assert_eq!(
                    harness::dominant_channel(&pixels, center),
                    Some(CHANNEL_RED),
                    "round {round}: the material must render into each graph config"
                );
                assert_eq!(
                    pixels[corner] > 150,
                    expect_blue,
                    "round {round}: each graph must keep its own declared clear color"
                );
            }
        }
        harness::cleanup_graph(scenario.graph);
        harness::cleanup_graph(dark_graph);
        harness::cleanup_graph(blue_graph);
        scenario.renderer.finish();
    }

    enum Config {
        Base,
        Dark,
        Blue,
    }
} // mod pbr (PBR pipeline scenarios)

/// Declared attachment semantics are the only source of attachment behavior:
/// a declared Clear replaces the target, and a declared Load extends whatever
/// earlier passes painted. (UI-material scenario: API validation captured.)
#[test]
#[ignore = "requires a graphics device"]
fn test_contract_declared_load_extends_and_clear_replaces() {
    let mut renderer = ContractRenderer::open("contract: attachment load/store");
    renderer.init_frame_pipelines();
    let scene = harness::init_ui_scene(renderer.gfx());

    let ui = {
        let mut ui = UIDrawList {
            screen_size: [64.0, 48.0],
            scale_factor: 1.0,
            ..Default::default()
        };
        ui.instances.push(VertexUIInstance {
            position: [4.0, 4.0],
            size: [12.0, 16.0],
            uv_min: [0.0; 2],
            uv_max: [1.0; 2],
            color: [0, 255, 0, 255],
            texture_index: scene.white_slot,
            clip_rect: [0.0, 0.0, 64.0, 48.0],
        });
        ui.commands = vec![UiDrawCommand::instanced(0, 1, None)];
        ui
    };

    // ClearValue colors are target-order BGRA here: [1,0,0,1] reads back red
    // (byte 2), [0,1,0,1] reads back green (byte 1).
    for (clear, preserved) in [
        (ClearValue::Color([1.0, 0.0, 0.0, 1.0]), CHANNEL_RED),
        (ClearValue::Color([0.0, 1.0, 0.0, 1.0]), CHANNEL_GREEN),
    ] {
        let mut graph = harness::build_graph(|builder| {
            builder
                .add_pass(
                    GeometryPass::new("background")
                        .without_depth()
                        .write_color_ops(
                            "backbuffer",
                            ImageFormat::B8G8R8A8Srgb,
                            AttachmentOps::clear(clear),
                        ),
                )
                .add_pass(
                    UIPass::new("ui")
                        .write("backbuffer")
                        .material(scene.material),
                )
        });
        let ui_pass = harness::pass_id(&graph, "ui");
        for _ in 0..2 {
            let pixels = renderer.render_frame(&mut graph, None, None, |frame| {
                frame.submit_ui(ui_pass, &ui);
            });
            assert_eq!(
                harness::dominant_channel(&pixels, harness::ui_pixel_offset(8, 8)),
                Some(CHANNEL_GREEN),
                "the UI quad must draw on the left"
            );
            assert_eq!(
                harness::dominant_channel(&pixels, harness::ui_pixel_offset(40, 24)),
                Some(preserved),
                "the UI pass's declared Load op must preserve the first pass"
            );
        }
        harness::cleanup_graph(graph);
    }
    renderer.finish();
}
