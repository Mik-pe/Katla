use super::*;
use crate::{ValidationLevel, ValidationMode, VulkanContext};

type ValidationErrors = std::sync::Arc<std::sync::Mutex<Vec<String>>>;

fn context() -> (VulkanContext, ValidationErrors) {
    let context = VulkanContext::init_headless(
        ValidationMode::Enabled,
        c"descriptor arena".into(),
        c"Katla".into(),
    )
    .unwrap();
    assert!(context.validation_active());
    let errors = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = errors.clone();
    context.set_validation_callback(move |message, level| {
        if level == ValidationLevel::Error {
            captured.lock().unwrap().push(message.to_owned());
        }
    });
    (context, errors)
}

fn layout(context: &VulkanContext, ty: vk::DescriptorType, count: u32) -> vk::DescriptorSetLayout {
    let bindings = [vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(ty)
        .descriptor_count(count)
        .stage_flags(vk::ShaderStageFlags::FRAGMENT)];
    unsafe {
        context.device.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
            None,
        )
    }
    .unwrap()
}

fn finish(
    context: VulkanContext,
    arena: DescriptorArena,
    layout: vk::DescriptorSetLayout,
    errors: ValidationErrors,
) {
    drop(arena);
    unsafe { context.device.destroy_descriptor_set_layout(layout, None) };
    drop(context);
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_native_descriptor_arena_grows_and_reuses_pools_for_a_thousand_sets() {
    let (context, errors) = context();
    let layout = layout(&context, vk::DescriptorType::UNIFORM_BUFFER, 1);
    let mut arena = DescriptorArena::new(context.gfx_cmdpool.owner.native.clone());
    let sizes = [vk::DescriptorPoolSize::default()
        .ty(vk::DescriptorType::UNIFORM_BUFFER)
        .descriptor_count(1)];
    for _ in 0..1000 {
        arena.allocate(layout, &sizes).unwrap();
    }
    assert_eq!(arena.pool_count(), 8);
    assert_eq!(arena.allocated_sets(), 1000);
    let pools: Vec<_> = arena.pools.iter().map(|pool| pool.handle).collect();
    arena.reset().unwrap();
    assert_eq!(arena.allocated_sets(), 0);
    for _ in 0..1000 {
        arena.allocate(layout, &sizes).unwrap();
    }
    assert_eq!(arena.allocated_sets(), 1000);
    assert_eq!(
        arena
            .pools
            .iter()
            .map(|pool| pool.handle)
            .collect::<Vec<_>>(),
        pools
    );
    finish(context, arena, layout, errors);
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_native_descriptor_arena_grows_when_descriptors_run_out_before_sets() {
    let (context, errors) = context();
    let layout = layout(&context, vk::DescriptorType::STORAGE_BUFFER, 3);
    let mut arena = DescriptorArena::new(context.gfx_cmdpool.owner.native.clone());
    let sizes = [vk::DescriptorPoolSize::default()
        .ty(vk::DescriptorType::STORAGE_BUFFER)
        .descriptor_count(3)];
    for _ in 0..100 {
        arena.allocate(layout, &sizes).unwrap();
    }
    assert_eq!(arena.pool_count(), 2);
    assert_eq!(arena.allocated_sets(), 100);
    arena.reset().unwrap();
    for _ in 0..100 {
        arena.allocate(layout, &sizes).unwrap();
    }
    assert_eq!(arena.pool_count(), 2);
    finish(context, arena, layout, errors);
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_native_descriptor_arena_preserves_failed_allocations_and_bounds_retry() {
    let (context, errors) = context();
    let layout = layout(&context, vk::DescriptorType::UNIFORM_BUFFER, 1);
    let mut arena = DescriptorArena::new(context.gfx_cmdpool.owner.native.clone());
    let sizes = [vk::DescriptorPoolSize::default()
        .ty(vk::DescriptorType::UNIFORM_BUFFER)
        .descriptor_count(1)];
    arena.allocate(layout, &sizes).unwrap();
    arena.allocation_failure = Some(vk::Result::ERROR_OUT_OF_HOST_MEMORY);
    assert!(matches!(
        arena.allocate(layout, &sizes),
        Err(RendererError::VulkanError(
            _,
            vk::Result::ERROR_OUT_OF_HOST_MEMORY
        ))
    ));
    assert_eq!(arena.pool_count(), 1);
    assert_eq!(arena.allocated_sets(), 1);
    arena.allocate(layout, &sizes).unwrap();
    for error in [
        vk::Result::ERROR_OUT_OF_POOL_MEMORY,
        vk::Result::ERROR_FRAGMENTED_POOL,
    ] {
        arena.allocation_failure = Some(error);
        arena.allocate(layout, &sizes).unwrap();
    }
    assert_eq!(arena.pool_count(), 3);
    assert_eq!(arena.allocated_sets(), 4);
    arena.clear();
    arena.allocation_failure = Some(vk::Result::ERROR_OUT_OF_POOL_MEMORY);
    assert!(matches!(
        arena.allocate(layout, &sizes),
        Err(RendererError::VulkanError(
            _,
            vk::Result::ERROR_OUT_OF_POOL_MEMORY
        ))
    ));
    assert_eq!(arena.pool_count(), 1);
    assert_eq!(arena.allocated_sets(), 0);
    arena.reset().unwrap();
    arena.allocate(layout, &sizes).unwrap();
    assert_eq!(arena.pool_count(), 1);
    finish(context, arena, layout, errors);
}

#[test]
#[ignore = "requires a Vulkan device"]
fn test_native_descriptor_arena_retains_its_device_after_context_drop() {
    let (context, errors) = context();
    let layout = layout(&context, vk::DescriptorType::UNIFORM_BUFFER, 1);
    let native = Rc::downgrade(&context.gfx_cmdpool.owner.native);
    let mut arena = DescriptorArena::new(context.gfx_cmdpool.owner.native.clone());
    let sizes = [vk::DescriptorPoolSize::default()
        .ty(vk::DescriptorType::UNIFORM_BUFFER)
        .descriptor_count(1)];
    arena.allocate(layout, &sizes).unwrap();
    unsafe { context.device.destroy_descriptor_set_layout(layout, None) };
    drop(context);
    assert!(native.upgrade().is_some());
    arena.reset().unwrap();
    drop(arena);
    assert!(native.upgrade().is_none());
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
}
