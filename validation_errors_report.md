# Vulkan Validation Errors Report
**Generated:** 2026-03-17  
**Command:** `cargo run -- -s -v`  
**Total Frames:** 100  
**Device:** Intel(R) Iris(R) Plus Graphics

---

## Summary

**Total Unique VUID Errors:** 8  
**Total Error Instances:** 100+ (hit duplicate limits)  
**Affected Subsystems:** Descriptors, Command Buffers, Synchronization, Storage Buffers  

---

## CRITICAL ERRORS

### 1. Command Buffer Recording State Errors

**VUID Codes:**
- `VUID-vkCmdBindPipeline-commandBuffer-recording`
- `VUID-vkCmdBindDescriptorSets-commandBuffer-recording`
- `VUID-vkCmdPushDescriptorSet-commandBuffer-recording`
- `VUID-vkCmdDispatch-commandBuffer-recording`
- `VUID-vkCmdPipelineBarrier-commandBuffer-recording`
- `VUID-vkCmdPipelineBarrier2-commandBuffer-recording`
- `VUID-vkCmdCopyBuffer-commandBuffer-recording`
- `VUID-vkEndCommandBuffer-commandBuffer-00059`

**Error Message:**
```
vkCmdXXX(): was called in VkCommandBuffer 0x20de1e88488/0x20d812360d8 
which is now in an invalid state (instead of recording state) because 
the following objects bound to the command buffer were invalidated:
  VkDescriptorSet 0x2af00000002af was destroyed or updated without UPDATE_AFTER_BIND
```

**Subsystem:** Command Buffers / Descriptors  
**Frequency:** 100+ instances (hit duplicate limit of 10 per VUID)  
**Severity:** CRITICAL

**Root Cause:** 
Descriptor sets (0x2af00000002af, 0x2b100000002b1) are being updated while still in use by pending command buffers. The descriptor sets were created without `VK_DESCRIPTOR_BINDING_UPDATE_AFTER_BIND_BIT` or `VK_DESCRIPTOR_BINDING_UPDATE_UNUSED_WHILE_PENDING_BIT` flags.

**Impact:**
- All command recording operations fail
- Particle system compute passes cannot execute
- Render passes cannot complete
- Frame graph cannot record commands

---

### 2. Descriptor Set Update Errors

**VUID Code:** `VUID-vkUpdateDescriptorSets-None-03047`

**Error Message:**
```
vkUpdateDescriptorSets(): pDescriptorWrites[0].dstBinding (2) was created 
with VkDescriptorBindingFlags(0), but VkDescriptorSet 0x2b100000002b1/0x2af00000002af 
is in use by VkCommandBuffer 0x20de1e88488/0x20d812360d8. 
This is only possible with flags found in VK_EXT_descriptor_indexing.
```

**Subsystem:** Descriptors  
**Frequency:** 10+ instances (hit duplicate limit)  
**Severity:** CRITICAL

**Root Cause:**
Binding 2 of descriptor sets is being updated while the descriptor set is still in use by command buffers in the pending state. This violates Vulkan synchronization rules.

**Affected Descriptor Sets:**
- 0x2af00000002af (particle system descriptor set)
- 0x2b100000002b1 (compositing descriptor set)

**Impact:**
- Cannot update descriptor bindings between frames
- Prevents proper frame-in-flight resource management
- Blocks descriptor updates for bindless texture system

---

### 3. Storage Buffer Out-of-Bounds Access (Compute)

**VUID Code:** `VUID-vkCmdDispatch-storageBuffers-06936`

**Error Message:**
```
vkCmdDispatch(): (set = 0, binding = 0, index 0) access out of bounds. 
The descriptor buffer (VkBuffer 0x2930000000293) size is 67108864 bytes, 
0 bytes were bound, and the highest out of bounds access was at [49642367] bytes.
Stage = Compute. Global invocation ID (x, y, z) = (32-163, 0, 0)
```

**Subsystem:** Particles / Compute Shaders  
**Frequency:** 10+ instances (hit duplicate limit)  
**Severity:** CRITICAL

**Root Cause:**
Particle compute shader (emit/simulate) is accessing the particle buffer (VkBuffer 0x2930000000293, 64 MB) with zero bytes bound in the descriptor. The shader is trying to access particle data at offsets ~49-50 MB but the descriptor reports 0 bytes bound.

**Affected Shaders:**
- Shader Module ID 13 (particle emit)
- Shader Module ID 14 (particle simulate)

**Access Pattern:**
- Workgroups 32-37 accessing offset ~49.6 MB
- Workgroups 160-163 accessing offset ~49.7-50.0 MB

**Impact:**
- Particle system compute passes access invalid memory
- Undefined behavior in particle simulation
- Potential GPU hang or crash
- All particle operations fail

---

### 4. Storage Buffer Out-of-Bounds Access (Render)

**VUID Code:** `VUID-vkCmdDraw-storageBuffers-06936`

**Error Message:**
```
vkCmdDraw(): (set = 0, binding = 0, index 0) access out of bounds. 
The descriptor buffer (VkBuffer 0x2930000000293) size is 67108864 bytes, 
1 byte was bound, and the highest out of bounds access was at [49927727-50007455] bytes.
Stage = Vertex. Vertex Index = 48-1323 Instance Index = 0.
```

**Subsystem:** Particles / Rendering  
**Frequency:** 10+ instances (hit duplicate limit)  
**Severity:** CRITICAL

**Root Cause:**
Particle vertex shader is trying to read particle data for rendering, but only 1 byte is bound in the descriptor instead of the full particle data structure (64 bytes per particle × max particles).

**Shader Details:**
- Shader Module ID 9 (particle render vertex shader)
- SPIR-V Instruction: `%91 = OpLoad %7 %90`
- Draw Index 35 (particle render pass)

**Access Pattern:**
- Vertex indices 48-54 accessing offset ~50 MB
- Vertex indices 1313-1323 accessing offset ~49.9 MB

**Impact:**
- Cannot render particles
- Vertex shader reads invalid memory
- Particle rendering completely broken

---

## HIGH SEVERITY ERRORS

### 5. Synchronization Hazards

**Error Type:** WRITE_AFTER_WRITE hazard (no VUID, GPU-assisted validation)

**Error Message:**
```
vkQueueSubmit(): WRITE_AFTER_WRITE hazard detected. 
vkCmdCopyBuffer (from VkCommandBuffer 0x20d812360d8) writes to VkBuffer 0x2b600000002b6, 
which was previously written by another vkCmdCopyBuffer command 
(from VkCommandBuffer 0x20de1e88488).

The current synchronization allows VK_ACCESS_2_TRANSFER_READ_BIT accesses at 
VK_PIPELINE_STAGE_2_COPY_BIT|VK_PIPELINE_STAGE_2_RESOLVE_BIT|VK_PIPELINE_STAGE_2_BLIT_BIT, 
but to prevent this hazard, it must allow VK_ACCESS_2_TRANSFER_WRITE_BIT accesses 
at VK_PIPELINE_STAGE_2_COPY_BIT.
```

**Subsystem:** Synchronization / Particle Debug Readback  
**Frequency:** 10+ instances  
**Severity:** HIGH

**Affected Buffers:**
- VkBuffer 0x2b600000002b6 (particle debug readback buffer 0)
- VkBuffer 0x2b700000002b7 (particle debug readback buffer 1)
- VkBuffer 0x2b900000002b9 (particle debug readback buffer 2)
- VkBuffer 0x2ba00000002ba (particle debug readback buffer 3)

**Root Cause:**
Particle debug readback system is copying data from GPU to CPU without proper synchronization. Multiple frames are writing to the same readback buffers without ensuring previous writes have completed.

**Synchronization Issue:**
- Current barrier: `VK_ACCESS_2_TRANSFER_READ_BIT` at copy/resolve/blit stages
- Required barrier: `VK_ACCESS_2_TRANSFER_WRITE_BIT` at copy stage

**Impact:**
- Data races in particle debug readback
- Readback data may be corrupted
- Potential for GPU hangs

---

## WARNINGS

### 6. Validation Configuration Warning

**MessageID:** 0x7f1922d7  
**Category:** VALIDATION-SETTINGS

**Message:**
```
vkCreateInstance(): Both GPU Assisted Validation and Normal Core Check 
Validation are enabled, this is not recommend as it will be very slow. 
Once all errors in Core Check are solved, please disable, then only use 
GPU-AV for best performance.
```

**Severity:** WARNING (performance)

**Recommendation:**
Fix all Core Validation errors first, then disable core checks and only use GPU-Assisted Validation for better performance.

---

## Analysis by Subsystem

### Particle System (MOST CRITICAL)
**Errors:** Storage buffer OOB (compute & render), sync hazards  
**Status:** Completely broken  
**Root Causes:**
1. Descriptor binding size is 0 or 1 byte instead of 64 MB
2. Missing UPDATE_AFTER_BIND flags on descriptor sets
3. Missing synchronization barriers for debug readback

**Fix Priority:** 1 (HIGHEST)

### Descriptor Management
**Errors:** Command buffer invalidation, descriptor update violations  
**Status:** Critical failure  
**Root Causes:**
1. Updating descriptors while in use by pending command buffers
2. Missing UPDATE_AFTER_BIND or UPDATE_UNUSED_WHILE_PENDING flags
3. Improper descriptor set lifecycle management

**Fix Priority:** 2 (HIGH)

### Synchronization
**Errors:** WRITE_AFTER_WRITE hazards  
**Status:** Data races  
**Root Causes:**
1. Insufficient pipeline barriers for readback buffers
2. Missing VK_ACCESS_2_TRANSFER_WRITE_BIT synchronization

**Fix Priority:** 3 (MEDIUM)

---

## Recommended Fix Order

1. **Fix descriptor binding sizes** - Ensure particle buffer descriptor has full 64 MB range bound
2. **Add UPDATE_AFTER_BIND flags** - Allow descriptor updates while in flight
3. **Fix synchronization** - Add proper pipeline barriers for debug readback
4. **Verify descriptor lifecycle** - Ensure descriptor sets aren't updated while in use
5. **Disable core validation** - After fixing, switch to GPU-AV only for performance

---

## Files Written
- `C:\dev\katla\validation_output.log` - Full validation output
- `C:\dev\katla\validation_errors_report.md` - This report
