//! Tests for Vulkan resource cleanup validation.
//!
//! These tests verify that resources are properly cleaned up when destroyed,
//! using Vulkan validation layers to detect memory leaks and resource leaks.

mod common;

use ash::vk;
use common::create_headless_context;
use katla_vulkan::{ValidationSeverity, VulkanContext};
use std::sync::{Arc, Mutex};

/// Shared storage for validation messages that survives context destruction.
type SharedMessages = Arc<Mutex<Vec<String>>>;

/// Helper to create a validation callback that writes to shared storage.
fn create_error_collector(
    messages: SharedMessages,
) -> Box<dyn FnMut(&katla_vulkan::ValidationMessage) -> bool + Send + Sync> {
    Box::new(move |msg| {
        if msg.severity == ValidationSeverity::Error {
            let formatted = if let Some(ref vuid) = msg.vuid {
                format!("[{}] {}", vuid, msg.message)
            } else {
                msg.message.clone()
            };
            messages.lock().unwrap().push(formatted);
        }
        false // Don't break
    })
}

/// Helper to check for validation errors during context destruction.
///
/// Uses shared storage to capture messages even during context destruction.
fn capture_validation_errors_during_drop<F>(f: F) -> Vec<String>
where
    F: FnOnce(&VulkanContext),
{
    let messages: SharedMessages = Arc::new(Mutex::new(Vec::new()));

    {
        let context = create_headless_context(true);

        // Set up callback that writes to shared storage
        let callback = create_error_collector(Arc::clone(&messages));
        context.set_validation_callback(callback);

        // Wait for device to be idle before any operations
        unsafe {
            context.device.device_wait_idle().ok();
        }

        // Run test code with access to context
        f(&context);

        // Wait for device to be idle before destruction
        unsafe {
            context.device.device_wait_idle().ok();
        }

        // Context is dropped here - callback writes to shared storage
    }

    // Return collected messages (clone from the Arc since callback may still hold a reference)
    let result = messages.lock().unwrap().clone();
    result
}

/// Test that VulkanContext can be created and destroyed without validation errors.
///
/// This is a basic sanity check that the headless context cleanup works correctly.
#[test]
fn test_context_cleanup_no_errors() {
    let errors = capture_validation_errors_during_drop(|_context| {
        // Just create and destroy - the drop happens at end of scope
    });

    if !errors.is_empty() {
        eprintln!("Validation errors during context cleanup:");
        for error in &errors {
            eprintln!("  - {}", error);
        }
        panic!(
            "Context cleanup produced {} validation errors",
            errors.len()
        );
    }
}

/// Test that render graph cleanup doesn't leak resources.
/// Note: This test creates its own Rc<VulkanContext> to work with the render graph API.
#[test]
fn test_render_graph_cleanup_no_errors() {
    use katla_vulkan::render_graph::{
        types::{Extent3D, ImageFormat, ImageLayout, ImageTiling, ImageUsage, SampleCount},
        Attachment, RenderGraphBuilder, ResourceKind,
    };
    use std::rc::Rc;

    let messages: SharedMessages = Arc::new(Mutex::new(Vec::new()));

    {
        let context = Rc::new(create_headless_context(true));

        // Set up callback that writes to shared storage
        let callback = create_error_collector(Arc::clone(&messages));
        context.set_validation_callback(callback);

        // Wait for device to be idle
        unsafe {
            context.device.device_wait_idle().ok();
        }

        // Build and compile a render graph
        let mut graph_builder = RenderGraphBuilder::new();

        let color_target = graph_builder.add_resource(
            "color_target",
            ResourceKind::Image {
                extent: Extent3D {
                    width: 256,
                    height: 256,
                    depth: 1,
                },
                format: ImageFormat::R8G8B8A8Srgb,
                usage: vec![ImageUsage::ColorAttachment],
                samples: SampleCount::Sample1,
                tiling: ImageTiling::Optimal,
                initial_layout: ImageLayout::Undefined,
                final_layout: ImageLayout::ShaderReadOnlyOptimal,
            },
        );

        graph_builder.add_pass("test_pass", |pass| {
            pass.write(Attachment::Color(color_target))
                .clear_color(color_target, [0.0, 0.0, 0.0, 1.0])
                .execute("test_pass", |_ctx| {});
        });

        // Compile and then drop
        let _graph = graph_builder
            .build(&context)
            .expect("Failed to compile graph");

        // Wait for device to be idle before destruction
        unsafe {
            context.device.device_wait_idle().ok();
        }

        // Graph and context are dropped here
    }

    let errors = messages.lock().unwrap().clone();

    if !errors.is_empty() {
        eprintln!("Validation errors during render graph cleanup:");
        for error in &errors {
            eprintln!("  - {}", error);
        }
        panic!(
            "Render graph cleanup produced {} validation errors",
            errors.len()
        );
    }
}

/// Test that creating and destroying images doesn't leak.
#[test]
fn test_image_cleanup_via_context() {
    let errors = capture_validation_errors_during_drop(|context| {
        // Create an image using the context's create_image method
        let extent = vk::Extent3D {
            width: 256,
            height: 256,
            depth: 1,
        };

        let create_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(extent)
            .mip_levels(1)
            .array_layers(1)
            .format(vk::Format::R8G8B8A8_SRGB)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::TYPE_1);

        let (image, allocation) =
            context.create_image(create_info, gpu_allocator::MemoryLocation::GpuOnly);

        // Manually destroy the image and free memory
        // This tests that the context's cleanup path works
        unsafe {
            context.device.destroy_image(image, None);
        }
        context
            .allocator
            .borrow_mut()
            .free(allocation)
            .expect("Failed to free allocation");
    });

    if !errors.is_empty() {
        eprintln!("Validation errors during image cleanup:");
        for error in &errors {
            eprintln!("  - {}", error);
        }
        panic!("Image cleanup produced {} validation errors", errors.len());
    }
}

/// Test that creating and destroying semaphores doesn't leak.
#[test]
fn test_semaphore_cleanup() {
    let errors = capture_validation_errors_during_drop(|context| {
        // Create semaphores directly using Vulkan API
        let create_info = vk::SemaphoreCreateInfo::default();

        unsafe {
            let semaphore = context.device.create_semaphore(&create_info, None).unwrap();
            // Destroy the semaphore
            context.device.destroy_semaphore(semaphore, None);
        }
    });

    if !errors.is_empty() {
        eprintln!("Validation errors during semaphore cleanup:");
        for error in &errors {
            eprintln!("  - {}", error);
        }
        panic!(
            "Semaphore cleanup produced {} validation errors",
            errors.len()
        );
    }
}

/// Test that creating and destroying fences doesn't leak.
#[test]
fn test_fence_cleanup() {
    let errors = capture_validation_errors_during_drop(|context| {
        // Create fences directly using Vulkan API
        let create_info = vk::FenceCreateInfo::default();

        unsafe {
            let fence = context.device.create_fence(&create_info, None).unwrap();
            // Destroy the fence
            context.device.destroy_fence(fence, None);
        }
    });

    if !errors.is_empty() {
        eprintln!("Validation errors during fence cleanup:");
        for error in &errors {
            eprintln!("  - {}", error);
        }
        panic!("Fence cleanup produced {} validation errors", errors.len());
    }
}

/// Test that command buffer cleanup works correctly.
#[test]
fn test_command_buffer_cleanup() {
    use katla_vulkan::CommandBuffer;

    let errors = capture_validation_errors_during_drop(|context| {
        // Create and drop a command buffer
        let cmd_buffer = CommandBuffer::new(&context.device, &context.gfx_cmdpool);
        cmd_buffer.begin_single_time_command();
        cmd_buffer.end_single_time_command();
        // Command buffer is dropped here
    });

    if !errors.is_empty() {
        eprintln!("Validation errors during command buffer cleanup:");
        for error in &errors {
            eprintln!("  - {}", error);
        }
        panic!(
            "Command buffer cleanup produced {} validation errors",
            errors.len()
        );
    }
}
