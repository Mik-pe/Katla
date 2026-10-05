use super::*;
use crate::ValidationMode;

#[test]
#[ignore = "requires a Vulkan device"]
fn test_native_frame_uploads_preserve_aligned_ranges_and_reuse_grown_blocks() {
    let context = Rc::new(
        VulkanContext::init_headless(
            ValidationMode::Enabled,
            c"frame upload storage".into(),
            c"Katla".into(),
        )
        .unwrap(),
    );
    assert!(context.validation_active());
    let errors = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = errors.clone();
    context.set_validation_callback(move |message, level| {
        if level == crate::ValidationLevel::Error {
            captured.lock().unwrap().push(message.to_owned());
        }
    });
    let baseline = context.allocator.debug_allocation_stats();
    let mut resources = FrameResources::new(context.clone());
    assert!(resources.upload(&[]).is_err());
    assert_eq!(resources.upload_block_count(), 0);
    let mut ranges = Vec::new();
    for index in 0..1000 {
        let bytes = vec![(index % 251) as u8; 73];
        ranges.push((resources.upload(&bytes).unwrap(), bytes));
    }
    let large = vec![197; (UPLOAD_BLOCK_BYTES * 2) as usize];
    ranges.push((resources.upload(&large).unwrap(), large));
    for (index, (range, expected)) in ranges.iter().enumerate() {
        assert_eq!(
            range.offset % context.limits.min_uniform_buffer_offset_alignment.max(1),
            0
        );
        assert_eq!(
            range.offset % context.limits.min_storage_buffer_offset_alignment.max(1),
            0
        );
        for (previous, _) in &ranges[..index] {
            if previous.buffer == range.buffer {
                assert!(previous.offset + previous.range <= range.offset);
            }
        }
        let block = resources
            .uploads
            .iter()
            .find(|block| block.buffer.vk_buffer() == range.buffer)
            .unwrap();
        let pointer = context
            .map_buffer(block.buffer.allocation.as_ref().unwrap())
            .unwrap();
        let actual = unsafe {
            std::slice::from_raw_parts(pointer.add(range.offset as usize), range.range as usize)
        };
        assert_eq!(actual, expected);
    }
    let handles: Vec<_> = resources
        .uploads
        .iter()
        .map(|block| block.buffer.vk_buffer())
        .collect();
    let warm_allocations = context.allocator.debug_allocation_stats();
    resources.reset().unwrap();
    assert_eq!(resources.uploaded_ranges(), 0);
    for (previous, bytes) in ranges {
        let current = resources.upload(&bytes).unwrap();
        assert_eq!(
            (current.buffer, current.offset, current.range),
            (previous.buffer, previous.offset, previous.range)
        );
    }
    assert_eq!(
        resources
            .uploads
            .iter()
            .map(|block| block.buffer.vk_buffer())
            .collect::<Vec<_>>(),
        handles
    );
    assert_eq!(context.allocator.debug_allocation_stats(), warm_allocations);
    resources.clear();
    assert_eq!(context.allocator.debug_allocation_stats(), baseline);
    drop(resources);
    drop(context);
    let errors = errors.lock().unwrap();
    assert!(errors.is_empty(), "{errors:?}");
}
