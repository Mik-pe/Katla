# Render Graph Production-Ready - Medium-term Phase

## Objective
Integrate render graph with swapchain, add command buffer submission with proper synchronization, and handle window resize events (graph recompilation).

## Context
After the Short-term phase, the render graph can execute passes but:
- No swapchain integration for presenting to screen
- No command buffer submission pipeline
- No handling of window resize events

## Current State Analysis

### Existing Infrastructure

| Component | File | Status |
| ---------- | ---- | ------ |
| Swapchain | vulkan/swapchain.rs | Active - creates/destroys swapchains |
| SwapData | vulkan/swapdata.rs | Active - manages acquire/render semaphores, fences |
| VkSemaphore/VkFence | sync.rs | Active - wrapper types |
| FrameGraph | render_graph/builder.rs | Active - virtual resource declaration |
| CompiledGraph | render_graph/compiled.rs | Active - compilation and execution |
| GraphExecutor | render_graph/executor.rs | Active - executes compiled graphs |
| PhysicalAllocations | render_graph/allocation.rs | Active - physical resource allocation |
| BarrierGenerator | render_graph/barrier.rs | Active - automatic barrier generation |
| VulkanContext | vulkan/context.rs | Active - Vulkan device/queue management |
| VulkanFrameCtx | vulkan/context.rs | Active - Per-frame swapchain resources |
| VulkanRenderer | renderer.rs | Active - High-level renderer API |
| Queue | vulkan/queue.rs | Active - Basic queue submission |

### Current Frame Flow (Non-Graph)
```
SwapData::swap_images()         // Acquire semaphores + image index
      ↓
Queue::submit()                  // Command buffer + semaphores
      ↓
Present to                      // Signal render finished semaphore
      ↓
SwapData::step_frame()              // Advance to next frame
```

### Gap Analysis

| Gap | Current State | Required For Graph |
| --- | ------------- | ------------------ |
| Swapchain import | Manual acquire in app code | Graph import node with acquire semaphore |
| Command buffer submission | Direct Queue::submit() | Graph executor with sync semaphores |
| Present | Direct swapchain present | Graph export with present semaphore |
| Resize handling | Manual recreate_swapchain() | Graph recompilation trigger |
| Frame pacing | Manual in SwapData | Graph-managed frame resources |

## Architecture Design

### Overview

```
+-------------------------------------------------------------------------+
|                         Application Layer                              |
|   +---------------------+                      +-------------+         |
|   | katla_app           |                      |             |         |
|   +--------+------------+                      |             |         |
|            |                                 |             |         |
|            ▼                                 ▼             |         |
|   +---------------------+    +---------------------+    +-------------------+
|   |  Frame pacing       |--->|   GraphRuntime     |<---|   Present         |
|   +---------------------+    +---------------------+    +-------------------+
|            |                                 |             |         |
|            |  Frame resources            |  Graph executor          |         |
|            |  (semaphores, fences)     |  Command buffer          |         |
|            |                                 |             |         |
+-------------------------------------------------------------------------+
```

### New Component: GraphRuntime
Central coordinator for graph-based rendering:
- Owns compiled graph and manages graph lifetime
- Provides per-frame resources (semaphores, fences)
- Handles swapchain integration
- Submits and presents frames
- Manages resize events

### New Component: SwapchainImport
Imports swapchain images into render graph:
- Creates ImportedResource for swapchain image
- Handles per-swapchain-image image views
- Manages acquire/release synchronization

### New Component: FrameResources
Per-frame GPU resources:
- Per-frame command buffers (or pools)
- Per-frame uniform buffers
- Per-frame descriptor sets
- Synchronization primitives

## Implementation Steps

### Step 1: Swapchain Import (2-3 days)

**File**: katla_vulkan/src/render_graph/swapchain_import.rs

**Description**: Import swapchain images as render graph resources with proper synchronization.

```rust
//! Swapchain import for render graph integration.
//!
//! This module provides utilities for importing swapchain images
//! into the render graph system.
//!
//! # Synchronization
//!
//! When importing swapchain images, proper synchronization is critical:
//! - **Acquire semaphore**: Signals when image is ready for rendering
//! - **Release barrier**: Transitions image to PRESENT_SRC_KHR
//!
//! # Layout Transitions
//!
//! ```text
//! UNDEFINED/PRESENT_SRC -> (acquire) -> COLOR_ATTACHMENT -> (render) -> PRESENT_SRC
//! ```

use ash::vk;
use crate::render_graph::builder::FrameGraph;
use crate::render_graph::handle::ImportedResource;
use crate::render_graph::resource::ImportDescriptor;
use crate::sync::{VkImage, VkImageView, VkSemaphore};
use std::rc::Rc;

use super::allocation::PhysicalAllocations;

/// Swapchain image import for render graph.
///
/// This struct wraps a swapchain image and view with the
/// synchronization primitives needed for render graph integration.
pub struct SwapchainImport {
    /// The swapchain image handle.
    pub image: VkImage,
    /// Image view for rendering.
    pub view: VkImageView,
    /// Semaphore signaled when image is ready.
    pub acquire_semaphore: VkSemaphore,
    /// Semaphore to signal when rendering is complete.
    pub render_semaphore: VkSemaphore,
    /// Current swapchain image index.
    pub image_index: u32,
    /// Swapchain extent.
    pub extent: vk::Extent2D,
    /// Swapchain format.
    pub format: vk::Format,
}

impl SwapchainImport {
    /// Creates a new swapchain import.
    pub fn new(
        image: VkImage,
        view: VkImageView,
        acquire_semaphore: VkSemaphore,
        render_semaphore: VkSemaphore,
        image_index: u32,
        extent: vk::Extent2D,
        format: vk::Format,
    ) -> Self {
        Self {
            image,
            view,
            acquire_semaphore,
            render_semaphore,
            image_index,
            extent,
            format,
        }
    }

    /// Creates an ImportDescriptor for use with FrameGraph::import_image.
    pub fn to_import_descriptor(&self, name: &'static str) -> ImportDescriptor {
        ImportDescriptor {
            image: self.image.vk(),
            initial_layout: vk::ImageLayout::UNDEFINED,
            final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
            name,
        }
    }
}
```

**Tests**:
- Test SwapchainImport creation
- Test import descriptor generation
- Test layout transition values

### Step 2: Frame Resource Pooling (2-3 days)

**File**: katla_vulkan/src/render_graph/frame_pool.rs

**Description**: Manage per-frame GPU resources for graph execution.

Key components:
- FrameResources: Per-frame command buffer, semaphores, fence
- FramePool: Pool of FrameResources for double/triple buffering

### Step 3: Graph Runtime (3-4 days)

**File**: katla_vulkan/src/render_graph/runtime.rs

**Description**: Central coordinator for graph-based frame rendering.

Key methods:
- begin_frame(): Acquire swapchain image, return frame resources
- execute_graph(): Run compiled graph into command buffer
- submit_frame(): Submit command buffer with sync
- present(): Present to swapchain
- handle_resize(): Detect and handle swapchain resize

### Step 4: Modify Executor for Sync (1-2 days)

**File**: katla_vulkan/src/render_graph/executor.rs

**Changes**: Add execute_with_sync() method for external semaphore handling.

### Step 5: Integration Test (1 day)

**File**: tests/graph_runtime_test.rs

**Description**: End-to-end test of graph runtime.

## Verification Checklist

### Functional Tests
- [ ] SwapchainImport creates correct import descriptor
- [ ] FramePool correctly cycles through frames
- [ 
