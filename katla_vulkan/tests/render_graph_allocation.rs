//! Integration tests for GPU resource allocation in the render graph.
//!
//! These tests verify that actual GPU resources are created
//! when `compile_graph_with_context()` is called with a VulkanContext.

mod common;

use ash::vk::{self, Handle};
use common::create_headless_context;
use katla_vulkan::render_graph::*;
use katla_vulkan::VulkanContext;
use slotmap::KeyData;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

// ============================================================================
// Test Utilities
// ============================================================================

/// Helper to create a Rc<VulkanContext> for allocation tests.
fn create_context() -> Rc<VulkanContext> {
    Rc::new(create_headless_context(true))
}

/// Helper to convert a VirtualImage/VirtualBuffer index to VirtualResourceId.
fn resource_id_from_index(index: u32) -> VirtualResourceId {
    VirtualResourceId::from(KeyData::from_ffi(index as u64))
}

/// Helper to capture validation errors during a test.
fn capture_validation_errors<F>(f: F) -> Vec<String>
where
    F: FnOnce(&Rc<VulkanContext>),
{
    let messages: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    {
        let context = create_context();

        // Set up error collector callback
        let msgs = Arc::clone(&messages);
        context.set_validation_callback(Box::new(move |msg| {
            if msg.severity == katla_vulkan::ValidationSeverity::Error {
                let formatted = if let Some(ref vuid) = msg.vuid {
                    format!("[{}] {}", vuid, msg.message)
                } else {
                    msg.message.clone()
                };
                msgs.lock().unwrap().push(formatted);
            }
            false // Don't break
        }));

        // Wait for device idle
        unsafe {
            context.device.device_wait_idle().ok();
        }

        // Run test
        f(&context);

        // Wait for device idle before destruction
        unsafe {
            context.device.device_wait_idle().ok();
        }
    }

    // Context is dropped here, callback writes to shared storage
    let result = messages.lock().unwrap().clone();
    result
}

// ============================================================================
// Basic Allocation Tests
// ============================================================================

/// Test: Create a graph with one image, verify GPU handles are non-null.
#[test]
fn test_single_image_allocation() {
    let errors = capture_validation_errors(|context| {
        let mut graph = FrameGraph::new("single_image_test");

        let color = graph.create_image(ImageDescriptor {
            format: vk::Format::R8G8B8A8_SRGB,
            extent: vk::Extent2D {
                width: 512,
                height: 512,
            },
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            name: "color",
            aliasable: true,
        });

        graph
            .add_pass("write")
            .write_attachment(color, AttachmentType::Color)
            .build();

        graph.add_pass("read").read_image(color).build();

        // Compile with context to get real allocations
        let compiled = compile_graph_with_context(graph, &context).expect("Graph should compile");

        // Verify allocation counts
        assert_eq!(
            compiled.allocations().image_count(),
            1,
            "Should have allocated 1 image"
        );
        assert_eq!(
            compiled.allocations().buffer_count(),
            0,
            "Should have allocated 0 buffers"
        );

        // Verify physical handle is non-null
        let color_id = resource_id_from_index(color.index());
        if let Some(image) = compiled.allocations().get_image(color_id) {
            assert!(
                !image.image.is_null(),
                "Physical image should have non-null handle"
            );
            assert!(
                !image.view.is_null(),
                "Physical image view should have non-null handle"
            );
        } else {
            panic!("Image should be allocated");
        }

        // Clean up
        // Allocations cleaned up when CompiledGraph is dropped
    });

    if !errors.is_empty() {
        eprintln!("Validation errors during single image allocation:");
        for error in &errors {
            eprintln!("  - {}", error);
        }
        panic!(
            "Single image allocation produced {} validation errors",
            errors.len()
        );
    }
}

/// Test: Create a graph with one buffer, verify GPU handle is non-null.
#[test]
fn test_single_buffer_allocation() {
    let errors = capture_validation_errors(|context| {
        let mut graph = FrameGraph::new("single_buffer_test");

        let uniform_buffer = graph.create_buffer(BufferDescriptor {
            size: 1024,
            usage: vk::BufferUsageFlags::UNIFORM_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            name: "uniforms",
            aliasable: true,
        });

        graph
            .add_pass("upload")
            .transfer_dst_buffer(uniform_buffer)
            .build();

        graph.add_pass("use").read_buffer(uniform_buffer).build();

        // Compile with context
        let compiled = compile_graph_with_context(graph, &context).expect("Graph should compile");

        // Verify allocation counts
        assert_eq!(
            compiled.allocations().buffer_count(),
            1,
            "Should have allocated 1 buffer"
        );
        assert_eq!(
            compiled.allocations().image_count(),
            0,
            "Should have allocated 0 images"
        );

        // Verify physical handle is non-null
        let buffer_id = resource_id_from_index(uniform_buffer.index());
        if let Some(buffer) = compiled.allocations().get_buffer(buffer_id) {
            assert!(
                !buffer.buffer.is_null(),
                "Physical buffer should have non-null handle"
            );
        } else {
            panic!("Buffer should be allocated");
        }

        // Clean up
        // Allocations cleaned up when CompiledGraph is dropped
    });

    if !errors.is_empty() {
        eprintln!("Validation errors during single buffer allocation:");

        for error in &errors {
            eprintln!("  - {}", error);
        }
        panic!(
            "Single buffer allocation produced {} validation errors",
            errors.len()
        );
    }
}

/// Test: Create graph with multiple images, verify all allocated.
#[test]
fn test_multiple_image_allocations() {
    let errors = capture_validation_errors(|context| {
        let mut graph = FrameGraph::new("multi_image_test");

        let albedo = graph.create_image(ImageDescriptor {
            format: vk::Format::R8G8B8A8_SRGB,
            extent: vk::Extent2D {
                width: 512,
                height: 512,
            },
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            name: "albedo",
            aliasable: true,
        });

        let normal = graph.create_image(ImageDescriptor {
            format: vk::Format::R16G16B16A16_SFLOAT,
            extent: vk::Extent2D {
                width: 512,
                height: 512,
            },
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            name: "normal",
            aliasable: true,
        });

        let depth = graph.create_image(ImageDescriptor {
            format: vk::Format::D32_SFLOAT,
            extent: vk::Extent2D {
                width: 512,
                height: 512,
            },
            usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            name: "depth",
            aliasable: false, // Depth buffers typically shouldn't alias
        });

        graph
            .add_pass("geometry")
            .write_attachment(albedo, AttachmentType::Color)
            .write_attachment(normal, AttachmentType::Color)
            .write_attachment(depth, AttachmentType::Depth)
            .build();

        graph
            .add_pass("lighting")
            .read_image(albedo)
            .read_image(normal)
            .build();

        // Compile with context
        let compiled = compile_graph_with_context(graph, &context).expect("Graph should compile");

        // Verify allocation counts
        assert_eq!(
            compiled.allocations().image_count(),
            3,
            "Should have allocated 3 images"
        );
        assert_eq!(
            compiled.allocations().buffer_count(),
            0,
            "Should have allocated 0 buffers"
        );

        // Verify all physical handles are non-null
        let albedo_id = resource_id_from_index(albedo.index());
        let normal_id = resource_id_from_index(normal.index());
        let depth_id = resource_id_from_index(depth.index());

        let albedo_image = compiled
            .allocations()
            .get_image(albedo_id)
            .expect("albedo image should be allocated");
        let normal_image = compiled
            .allocations()
            .get_image(normal_id)
            .expect("normal image should be allocated");
        let depth_image = compiled
            .allocations()
            .get_image(depth_id)
            .expect("depth image should be allocated");

        assert!(
            !albedo_image.image.is_null(),
            "Albedo image should have non-null handle"
        );
        assert!(
            !albedo_image.view.is_null(),
            "Albedo view should have non-null handle"
        );

        assert!(
            !normal_image.image.is_null(),
            "Normal image should have non-null handle"
        );
        assert!(
            !normal_image.view.is_null(),
            "Normal view should have non-null handle"
        );

        assert!(
            !depth_image.image.is_null(),
            "Depth image should have non-null handle"
        );
        assert!(
            !depth_image.view.is_null(),
            "Depth view should have non-null handle"
        );

        // Clean up
        // Allocations cleaned up when CompiledGraph is dropped
    });

    if !errors.is_empty() {
        eprintln!("Validation errors during multiple image allocation:");
        for error in &errors {
            eprintln!("  - {}", error);
        }
        panic!(
            "Multiple image allocation produced {} validation errors",
            errors.len()
        );
    }
}

/// Test: Verify aliasing works with real resources (non-overlapping resources share memory).
#[test]
fn test_aliasing_creates_shared_memory() {
    let errors = capture_validation_errors(|context| {
        let mut graph = FrameGraph::new("aliasing_test");

        // Create resources with non-overlapping lifetimes
        // We need 4 passes with an intermediate resource to ensure non-overlapping:
        // temp_a: [0, 1] (pass 0 writes, pass 1 reads)
        // intermediate: [1, 2] (pass 1 writes, pass 2 reads)
        // temp_b: [2, 3] (pass 2 writes, pass 3 reads)
        // Therefore temp_a and temp_b CAN alias (non-overlapping)
        let temp_a = graph.create_image(ImageDescriptor {
            format: vk::Format::R8G8B8A8_SRGB,
            extent: vk::Extent2D {
                width: 256,
                height: 256,
            },
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            name: "temp_a",
            aliasable: true,
        });

        let intermediate = graph.create_image(ImageDescriptor {
            format: vk::Format::R8G8B8A8_SRGB,
            extent: vk::Extent2D {
                width: 256,
                height: 256,
            },
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            name: "intermediate",
            aliasable: true,
        });

        let temp_b = graph.create_image(ImageDescriptor {
            format: vk::Format::R8G8B8A8_SRGB,
            extent: vk::Extent2D {
                width: 256,
                height: 256,
            },
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            name: "temp_b",
            aliasable: true,
        });

        // Build a dependency chain with 4 passes to ensure non-overlapping lifetimes:
        // Pass 0: write temp_a
        // Pass 1: read temp_a, write intermediate
        // Pass 2: read intermediate, write temp_b
        // Pass 3: read temp_b
        graph
            .add_pass("write_a")
            .write_attachment(temp_a, AttachmentType::Color)
            .build();

        graph
            .add_pass("handoff_ab")
            .read_image(temp_a)
            .write_attachment(intermediate, AttachmentType::Color)
            .build();

        graph
            .add_pass("handoff_bc")
            .read_image(intermediate)
            .write_attachment(temp_b, AttachmentType::Color)
            .build();

        graph.add_pass("read_b").read_image(temp_b).build();

        // Compile with context
        let compiled = compile_graph_with_context(graph, &context).expect("Graph should compile");

        // Verify that aliasing analysis found resources that can alias
        let temp_a_id = resource_id_from_index(temp_a.index());
        let temp_b_id = resource_id_from_index(temp_b.index());

        assert!(
            compiled.aliasing().can_alias(temp_a_id, temp_b_id),
            "temp_a and temp_b should be able to alias (non-overlapping lifetimes)"
        );

        // Verify that there are alias groups
        assert!(
            compiled.aliasing().group_count() >= 1,
            "Should have at least one alias group for aliasing to work"
        );

        // For aliased resources, verify they alias_group field is set
        if let Some(image_a) = compiled.allocations().get_image(temp_a_id) {
            if let Some(image_b) = compiled.allocations().get_image(temp_b_id) {
                // Both should point to the same alias group
                assert_eq!(
                    image_a.alias_group, image_b.alias_group,
                    "Aliased images should point to the same alias group"
                );
            }
        }

        // Clean up
        // Allocations cleaned up when CompiledGraph is dropped
    });

    if !errors.is_empty() {
        eprintln!("Validation errors during aliasing test:");
        for error in &errors {
            eprintln!("  - {}", error);
        }
        panic!("Aliasing test produced {} validation errors", errors.len());
    }
}

/// Test: Full deferred rendering pipeline test (GBuffer + depth + lighting).
#[test]
fn test_deferred_pipeline_allocation() {
    let errors = capture_validation_errors(|context| {
        let mut graph = FrameGraph::new("deferred_pipeline");

        // GBuffer images
        let albedo = graph.create_image(ImageDescriptor {
            format: vk::Format::R8G8B8A8_SRGB,
            extent: vk::Extent2D {
                width: 512,
                height: 512,
            },
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            name: "albedo",
            aliasable: true,
        });

        let normal = graph.create_image(ImageDescriptor {
            format: vk::Format::R16G16B16A16_SFLOAT,
            extent: vk::Extent2D {
                width: 512,
                height: 512,
            },
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            name: "normal",
            aliasable: true,
        });

        let depth = graph.create_image(ImageDescriptor {
            format: vk::Format::D32_SFLOAT,
            extent: vk::Extent2D {
                width: 512,
                height: 512,
            },
            usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            name: "depth",
            aliasable: false, // Depth typically shouldn't alias
        });

        // HDR output
        let hdr = graph.create_image(ImageDescriptor {
            format: vk::Format::R16G16B16A16_SFLOAT,
            extent: vk::Extent2D {
                width: 512,
                height: 512,
            },
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            name: "hdr",
            aliasable: true,
        });

        // Geometry pass - writes to gbuffer
        graph
            .add_pass("geometry")
            .write_attachment(albedo, AttachmentType::Color)
            .write_attachment(normal, AttachmentType::Color)
            .write_attachment(depth, AttachmentType::Depth)
            .build();

        // Lighting pass - reads gbuffer, writes HDR
        graph
            .add_pass("lighting")
            .read_image(albedo)
            .read_image(normal)
            .write_attachment(hdr, AttachmentType::Color)
            .build();

        // Post-process pass - reads HDR
        graph.add_pass("postprocess").read_image(hdr).build();

        // Compile with context
        let compiled = compile_graph_with_context(graph, &context).expect("Graph should compile");

        // Verify allocation counts
        assert_eq!(
            compiled.allocations().image_count(),
            4,
            "Should have allocated 4 images"
        );
        assert_eq!(
            compiled.allocations().buffer_count(),
            0,
            "Should have allocated 0 buffers"
        );

        // Verify all physical handles are non-null
        let albedo_id = resource_id_from_index(albedo.index());
        let normal_id = resource_id_from_index(normal.index());
        let depth_id = resource_id_from_index(depth.index());
        let hdr_id = resource_id_from_index(hdr.index());

        for (name, id) in [
            ("albedo", albedo_id),
            ("normal", normal_id),
            ("depth", depth_id),
            ("hdr", hdr_id),
        ] {
            if let Some(image) = compiled.allocations().get_image(id) {
                assert!(
                    !image.image.is_null(),
                    "{} image should have non-null handle",
                    name
                );
                assert!(
                    !image.view.is_null(),
                    "{} view should have non-null handle",
                    name
                );
            } else {
                panic!("{} should be allocated", name);
            }
        }

        // Verify execution order
        assert_eq!(compiled.pass_count(), 3, "Should have 3 passes");

        let order = compiled.execution_order();
        let geo_pos = compiled
            .pass_id("geometry")
            .and_then(|id| order.iter().position(|&p| p == id))
            .expect("geometry should be in order");
        let light_pos = compiled
            .pass_id("lighting")
            .and_then(|id| order.iter().position(|&p| p == id))
            .expect("lighting should be in order");
        let post_pos = compiled
            .pass_id("postprocess")
            .and_then(|id| order.iter().position(|&p| p == id))
            .expect("postprocess should be in order");

        assert!(geo_pos < light_pos, "geometry should come before lighting");
        assert!(
            light_pos < post_pos,
            "lighting should come before postprocess"
        );

        // Clean up
        // Allocations cleaned up when CompiledGraph is dropped
    });

    if !errors.is_empty() {
        eprintln!("Validation errors during deferred pipeline allocation:");
        for error in &errors {
            eprintln!("  - {}", error);
        }
        panic!(
            "Deferred pipeline allocation produced {} validation errors",
            errors.len()
        );
    }
}

/// Test: Test empty graphs and error cases.
#[test]
fn test_allocation_error_handling() {
    let errors = capture_validation_errors(|context| {
        // Test 1: Empty graph should compile successfully with empty allocations
        {
            let graph = FrameGraph::new("empty_graph");
            let compiled =
                compile_graph_with_context(graph, context).expect("Empty graph should compile");

            assert!(
                compiled.allocations().is_empty(),
                "Empty graph should have empty allocations"
            );
            assert_eq!(
                compiled.allocations().image_count(),
                0,
                "Empty graph should have 0 images"
            );
            assert_eq!(
                compiled.allocations().buffer_count(),
                0,
                "Empty graph should have 0 buffers"
            );
            assert_eq!(compiled.pass_count(), 0, "Empty graph should have 0 passes");
        }

        // Test 2: Graph with pass but no resources
        {
            let mut graph = FrameGraph::new("pass_only");
            graph.add_pass("empty_pass").build();

            let compiled =
                compile_graph_with_context(graph, context).expect("Pass-only graph should compile");

            assert!(
                compiled.allocations().is_empty(),
                "Pass-only graph should have empty allocations"
            );
            assert_eq!(
                compiled.pass_count(),
                1,
                "Pass-only graph should have 1 pass"
            );
        }
    });

    if !errors.is_empty() {
        eprintln!("Validation errors during allocation error handling:");
        for error in &errors {
            eprintln!("  - {}", error);
        }
        panic!(
            "Allocation error handling produced {} validation errors",
            errors.len()
        );
    }
}

/// Test: Verify no Vulkan validation errors during allocation.
#[test]
fn test_no_validation_errors_during_allocation() {
    let errors = capture_validation_errors(|context| {
        // Run a comprehensive allocation scenario
        let mut graph = FrameGraph::new("validation_test");

        let color = graph.create_image(ImageDescriptor {
            format: vk::Format::B8G8R8A8_SRGB,
            extent: vk::Extent2D {
                width: 256,
                height: 256,
            },
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
            name: "color",
            aliasable: true,
        });

        graph
            .add_pass("write")
            .write_attachment(color, AttachmentType::Color)
            .build();

        graph.add_pass("read").read_image(color).build();

        // Compile with context
        let compiled = compile_graph_with_context(graph, &context).expect("Graph should compile");

        // Verify allocation succeeded
        assert_eq!(
            compiled.allocations().image_count(),
            1,
            "Should have allocated 1 image"
        );

        // Clean up
        // Allocations cleaned up when CompiledGraph is dropped
    });

    if !errors.is_empty() {
        eprintln!("Validation errors during allocation test:");
        for error in &errors {
            eprintln!("  - {}", error);
        }
        panic!(
            "Allocation test produced {} validation errors",
            errors.len()
        );
    }
}
