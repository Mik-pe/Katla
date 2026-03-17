# WRITE_AFTER_WRITE (WAW) Errors Report
**Generated:** 2026-03-17
**Source:** `cargo run -- -s -v` validation output
**Total WAW Instances:** 10+ (hit duplicate limit)

---

## Executive Summary

**ALL WAW errors originate from the particle debug readback system.** The system is copying particle data from GPU to CPU without proper synchronization, causing multiple frames to write to the same readback buffers concurrently.

---

## Detailed WAW Error List

### Error Group 1: Frame 11 → Frame 12 Hazard
**Timestamp:** 2026-03-17T10:42:31Z
**Command Buffers:** 0x20d812360d8 (current) → 0x20de1e88488 (previous)
**Queue:** VkQueue 0x20de1acff08

#### Affected Buffers (4x):

**Buffer 1:** VkBuffer 0x2b600000002b6
```
vkQueueSubmit(): WRITE_AFTER_WRITE hazard detected. vkCmdCopyBuffer (from 
VkCommandBuffer 0x20d812360d8) writes to VkBuffer 0x2b600000002b6, which was 
previously written by another vkCmdCopyBuffer command (from VkCommandBuffer 
0x20de1e88488).

Current sync: VK_ACCESS_2_TRANSFER_READ_BIT at 
  VK_PIPELINE_STAGE_2_COPY_BIT|VK_PIPELINE_STAGE_2_RESOLVE_BIT|VK_PIPELINE_STAGE_2_BLIT_BIT

Required sync: VK_ACCESS_2_TRANSFER_WRITE_BIT at VK_PIPELINE_STAGE_2_COPY_BIT
```

**Buffer 2:** VkBuffer 0x2b700000002b7
```
vkQueueSubmit(): WRITE_AFTER_WRITE hazard detected. vkCmdCopyBuffer (from 
VkCommandBuffer 0x20d812360d8) writes to VkBuffer 0x2b700000002b7, which was 
previously written by another vkCmdCopyBuffer command (from VkCommandBuffer 
0x20de1e88488).

Current sync: VK_ACCESS_2_TRANSFER_READ_BIT at 
  VK_PIPELINE_STAGE_2_COPY_BIT|VK_PIPELINE_STAGE_2_RESOLVE_BIT|VK_PIPELINE_STAGE_2_BLIT_BIT

Required sync: VK_ACCESS_2_TRANSFER_WRITE_BIT at VK_PIPELINE_STAGE_2_COPY_BIT
```

**Buffer 3:** VkBuffer 0x2b900000002b9
```
vkQueueSubmit(): WRITE_AFTER_WRITE hazard detected. vkCmdCopyBuffer (from 
VkCommandBuffer 0x20d812360d8) writes to VkBuffer 0x2b900000002b9, which was 
previously written by another vkCmdCopyBuffer command (from VkCommandBuffer 
0x20de1e88488).

Current sync: VK_ACCESS_2_TRANSFER_READ_BIT at 
  VK_PIPELINE_STAGE_2_COPY_BIT|VK_PIPELINE_STAGE_2_RESOLVE_BIT|VK_PIPELINE_STAGE_2_BLIT_BIT

Required sync: VK_ACCESS_2_TRANSFER_WRITE_BIT at VK_PIPELINE_STAGE_2_COPY_BIT
```

**Buffer 4:** VkBuffer 0x2ba00000002ba
```
vkQueueSubmit(): WRITE_AFTER_WRITE hazard detected. vkCmdCopyBuffer (from 
VkCommandBuffer 0x20d812360d8) writes to VkBuffer 0x2ba00000002ba, which was 
previously written by another vkCmdCopyBuffer command (from VkCommandBuffer 
0x20de1e88488).

No sufficient synchronization is present to ensure that a write 
(VK_ACCESS_2_TRANSFER_WRITE_BIT) at VK_PIPELINE_STAGE_2_COPY_BIT does not 
conflict with a prior write of the same type at the same stage.
```

---

### Error Group 2: Frame 12 → Frame 13 Hazard
**Timestamp:** 2026-03-17T10:42:32Z
**Command Buffers:** 0x20de1e88488 (current) → 0x20d812360d8 (previous)
**Queue:** VkQueue 0x20de1acff08

#### Affected Buffers (4x):

**Buffer 1:** VkBuffer 0x2b600000002b6
```
vkQueueSubmit(): WRITE_AFTER_WRITE hazard detected. vkCmdCopyBuffer (from 
VkCommandBuffer 0x20de1e88488) writes to VkBuffer 0x2b600000002b6, which was 
previously written by another vkCmdCopyBuffer command (from VkCommandBuffer 
0x20d812360d8).

Current sync: VK_ACCESS_2_TRANSFER_READ_BIT at 
  VK_PIPELINE_STAGE_2_COPY_BIT|VK_PIPELINE_STAGE_2_RESOLVE_BIT|VK_PIPELINE_STAGE_2_BLIT_BIT

Required sync: VK_ACCESS_2_TRANSFER_WRITE_BIT at VK_PIPELINE_STAGE_2_COPY_BIT
```

**Buffer 2:** VkBuffer 0x2b700000002b7
```
vkQueueSubmit(): WRITE_AFTER_WRITE hazard detected. vkCmdCopyBuffer (from 
VkCommandBuffer 0x20de1e88488) writes to VkBuffer 0x2b700000002b7, which was 
previously written by another vkCmdCopyBuffer command (from VkCommandBuffer 
0x20d812360d8).

Current sync: VK_ACCESS_2_TRANSFER_READ_BIT at 
  VK_PIPELINE_STAGE_2_COPY_BIT|VK_PIPELINE_STAGE_2_RESOLVE_BIT|VK_PIPELINE_STAGE_2_BLIT_BIT

Required sync: VK_ACCESS_2_TRANSFER_WRITE_BIT at VK_PIPELINE_STAGE_2_COPY_BIT
```

**Buffer 3:** VkBuffer 0x2b900000002b9
```
vkQueueSubmit(): WRITE_AFTER_WRITE hazard detected. vkCmdCopyBuffer (from 
VkCommandBuffer 0x20de1e88488) writes to VkBuffer 0x2b900000002b9, which was 
previously written by another vkCmdCopyBuffer command (from VkCommandBuffer 
0x20d812360d8).

Current sync: VK_ACCESS_2_TRANSFER_READ_BIT at 
  VK_PIPELINE_STAGE_2_COPY_BIT|VK_PIPELINE_STAGE_2_RESOLVE_BIT|VK_PIPELINE_STAGE_2_BLIT_BIT

Required sync: VK_ACCESS_2_TRANSFER_WRITE_BIT at VK_PIPELINE_STAGE_2_COPY_BIT
```

**Buffer 4:** VkBuffer 0x2ba00000002ba
```
vkQueueSubmit(): WRITE_AFTER_WRITE hazard detected. vkCmdCopyBuffer (from 
VkCommandBuffer 0x20de1e88488) writes to VkBuffer 0x2ba00000002ba, which was 
previously written by another vkCmdCopyBuffer command (from VkCommandBuffer 
0x20d812360d8).

No sufficient synchronization is present to ensure that a write 
(VK_ACCESS_2_TRANSFER_WRITE_BIT) at VK_PIPELINE_STAGE_2_COPY_BIT does not 
conflict with a prior write of the same type at the same stage.
```

---

### Error Group 3: Frame 13 → Frame 14 Hazard (Partial)
**Timestamp:** 2026-03-17T10:42:33Z
**Command Buffers:** 0x20d812360d8 (current) → 0x20de1e88488 (previous)
**Queue:** VkQueue 0x20de1acff08
**Note:** Hit duplicate limit at this point (reported 10 times)

#### Affected Buffers (2x shown before limit):

**Buffer 1:** VkBuffer 0x2b600000002b6
```
vkQueueSubmit(): WRITE_AFTER_WRITE hazard detected. vkCmdCopyBuffer (from 
VkCommandBuffer 0x20d812360d8) writes to VkBuffer 0x2b600000002b6, which was 
previously written by another vkCmdCopyBuffer command (from VkCommandBuffer 
0x20de1e88488).

Current sync: VK_ACCESS_2_TRANSFER_READ_BIT at 
  VK_PIPELINE_STAGE_2_COPY_BIT|VK_PIPELINE_STAGE_2_RESOLVE_BIT|VK_PIPELINE_STAGE_2_BLIT_BIT

Required sync: VK_ACCESS_2_TRANSFER_WRITE_BIT at VK_PIPELINE_STAGE_2_COPY_BIT
```

**Buffer 2:** VkBuffer 0x2b700000002b7
```
vkQueueSubmit(): WRITE_AFTER_WRITE hazard detected. vkCmdCopyBuffer (from 
VkCommandBuffer 0x20d812360d8) writes to VkBuffer 0x2b700000002b7, which was 
previously written by another vkCmdCopyBuffer command (from VkCommandBuffer 
0x20de1e88488).

Current sync: VK_ACCESS_2_TRANSFER_READ_BIT at 
  VK_PIPELINE_STAGE_2_COPY_BIT|VK_PIPELINE_STAGE_2_RESOLVE_BIT|VK_PIPELINE_STAGE_2_BLIT_BIT

Required sync: VK_ACCESS_2_TRANSFER_WRITE_BIT at VK_PIPELINE_STAGE_2_COPY_BIT
```

---

## Pattern Analysis

### Affected Resources
All 4 particle debug readback buffers:
- **0x2b600000002b6** - Readback buffer 0
- **0x2b700000002b7** - Readback buffer 1
- **0x2b900000002b9** - Readback buffer 2
- **0x2ba00000002ba** - Readback buffer 3

### Command Buffer Pattern
Two command buffers alternate in a ping-pong pattern:
- **0x20d812360d8** - Frame A command buffer
- **0x20de1e88488** - Frame B command buffer

This suggests the particle debug readback is happening every frame without proper synchronization.

### Synchronization Issue
**Current (Incorrect) Barrier:**
```rust
VkAccessFlags2: VK_ACCESS_2_TRANSFER_READ_BIT
VkPipelineStageFlags2: VK_PIPELINE_STAGE_2_COPY_BIT | 
                       VK_PIPELINE_STAGE_2_RESOLVE_BIT | 
                       VK_PIPELINE_STAGE_2_BLIT_BIT
```

**Required (Correct) Barrier:**
```rust
VkAccessFlags2: VK_ACCESS_2_TRANSFER_WRITE_BIT
VkPipelineStageFlags2: VK_PIPELINE_STAGE_2_COPY_BIT
```

---

## Root Cause

The particle debug readback system is using **double-buffering** (4 buffers for 2 frames in flight) but is **missing a pipeline barrier** to ensure the previous frame's copy operation has completed before the next frame starts writing to the same buffer.

**Current behavior:**
1. Frame 11: Copies particle data to readback buffers 0-3
2. Frame 12: Immediately starts copying to readback buffers 0-3 **BEFORE** Frame 11's copy completes
3. Result: WRITE_AFTER_WRITE hazard

**Required behavior:**
1. Frame 11: Copies particle data to readback buffers 0-3
2. **Barrier:** Wait for Frame 11's copy to complete (VK_ACCESS_2_TRANSFER_WRITE_BIT)
3. Frame 12: Now safe to copy to readback buffers 0-3

---

## Context from Logs

**Frame 11 Readback:**
```
[2026-03-17T10:42:32Z INFO] === PARTICLE DEBUG READBACK ===
Particles: 46 alive, 1038294 dead, 1048576 total capacity
Particle 0: pos=(10.21,-12.06,4.23) vel=(0.10,-33.04,0.30) lifetime=1.11
```

**Frame 12 Readback:**
```
[2026-03-17T10:42:33Z INFO] === PARTICLE DEBUG READBACK ===
Particles: 1624 alive, 1036730 dead, 1048576 total capacity
Particle 0: pos=(10.21,-12.06,4.23) vel=(0.10,-33.04,0.30) lifetime=1.11
```

**Frame 13 Readback:**
```
[2026-03-17T10:42:33Z INFO] === PARTICLE DEBUG READBACK ===
Particles: 277 alive, 1035452 dead, 1048576 total capacity
Particle 0: pos=(10.29,-37.89,4.46) vel=(0.10,-40.70,0.30) lifetime=0.33
```

The readback is triggered **every frame** (frames 11, 12, 13 shown), causing continuous WAW hazards.

---

## Synchronization Hazards Summary

| Hazard Type | Count | Affected Buffers | Root Cause |
|-------------|-------|------------------|------------|
| WRITE_AFTER_WRITE | 10+ | All 4 readback buffers | Missing TRANSFER_WRITE_BIT barrier |

---

## VUID Codes

No specific VUID code - these are detected by **GPU-Assisted Validation** (synchronization hazard detection), not core validation checks.

---

## Operations Involved

1. **vkCmdCopyBuffer** - Copying particle data from GPU storage buffer to CPU-visible readback buffer
2. **vkQueueSubmit** - Submitting command buffers with unsynchronized writes
3. **Frame Graph** - "Recording particle debug readback after simulate pass" happens every frame

---

## Files Involved

Based on log messages:
- **katla_gfx/particles/debug_readback** - Particle debug readback implementation
- **katla_gfx/render_graph/graph** - Frame graph recording the readback pass
- **katla_app/application/renderer** - Triggering readback every frame

---

## Impact

1. **Data Corruption:** Readback data may contain partial/mixed data from two frames
2. **Undefined Behavior:** GPU may hang or crash due to unsynchronized memory access
3. **Debugging Issues:** Particle debug output cannot be trusted
4. **Performance:** May trigger GPU watchdog or driver workarounds

---

## Fix Required

Add a pipeline barrier before the vkCmdCopyBuffer operation to ensure previous writes have completed:

```rust
vkCmdPipelineBarrier2(
    command_buffer,
    &VkDependencyInfo {
        srcStageMask: VK_PIPELINE_STAGE_2_COPY_BIT,
        srcAccessMask: VK_ACCESS_2_TRANSFER_WRITE_BIT,
        dstStageMask: VK_PIPELINE_STAGE_2_COPY_BIT,
        dstAccessMask: VK_ACCESS_2_TRANSFER_WRITE_BIT,
        bufferMemoryBarrierCount: 4,
        pBufferMemoryBarriers: &barriers,  // One for each readback buffer
        ...
    }
);
```

---

## Conclusion

**ALL WAW errors are caused by the particle debug readback system.** The fix is straightforward: add proper pipeline barriers with `VK_ACCESS_2_TRANSFER_WRITE_BIT` synchronization before each copy operation.
