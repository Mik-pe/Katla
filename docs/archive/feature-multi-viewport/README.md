# Multi-Viewport Frame Graph Feature

**Status:** 🚧 Ready for Implementation  
**Version:** 1.0  
**Date:** March 2026

---

## 📋 Overview

This feature adds multi-viewport support to Katla's render graph, enabling:
- Split-screen rendering
- Editor viewports (preview panels, minimaps)
- Multi-window scenarios
- Viewport compositing with overlapping/alpha blending

**Implementation Time:** 5-8 days  
**Approach:** Remove old viewport code, migrate to frame graph (no hybrid)

---

## 📚 Documentation

### Quick Start
👉 **Start here:** [`IMPLEMENTATION-GUIDE.md`](./IMPLEMENTATION-GUIDE.md)

This guide explains:
- How to use all the documents in this folder
- Quick start guides for different scenarios
- Implementation checklist
- Common questions

### Research & Best Practices
📄 [`vulkan-framegraph-ui-best-practices-2024-2026.md`](./vulkan-framegraph-ui-best-practices-2024-2026.md)

State-of-the-art Vulkan techniques (2024-2026):
- Dynamic rendering
- Bindless descriptors
- Multi-viewport architecture
- UI integration patterns
- Synchronization best practices
- WGSL-compatible shader examples

### Current Implementation Analysis
📊 [`katla-rendergraph-analysis.md`](./katla-rendergraph-analysis.md)

Detailed analysis of Katla's current state:
- ✅ Strengths (what we do well)
- ❌ Gaps (what's missing)
- 📊 Priority matrix
- 🎯 Action items

### Implementation Plan
🔧 [`multi-viewport-implementation-plan.md`](./multi-viewport-implementation-plan.md)

Step-by-step implementation guide:
- **Phase 1:** Compositing pass descriptor set (1 day)
- **Phase 2:** Multi-viewport compositing (2-3 days)
- **Phase 3:** Cleanup & migration (1 day)
- Testing strategy
- Rollback plan

---

## 🎯 Key Features

### What You'll Get

✅ **Multi-Viewport Rendering**
- Render 4+ viewports simultaneously
- Each viewport has independent camera, scene, resolution
- Viewport can be positioned anywhere on screen

✅ **Compositing Pass**
- Single pass combines all viewports
- Support for overlapping viewports with alpha blending
- Efficient bindless texture array sampling

✅ **Frame Graph Integration**
- All viewport rendering goes through frame graph
- Automatic barrier synchronization
- Optimal pass ordering

✅ **WGSL-Compatible**
- Uses uniform buffers (no push constants)
- Push descriptors for dynamic updates
- Full shader examples provided

### What You'll Remove

❌ Old viewport rendering code (manual, not integrated)
❌ Single-texture bindless API (replaced with array API)
❌ Direct viewport rendering functions

---

## 🚀 Quick Example

### Before (Old Approach)
```rust
// Create viewport
let viewport = renderer.create_viewport()
    .size(512, 512)
    .build(&mut renderer)?;

// Render to viewport
renderer.render_viewport(viewport, &camera, &draw_list);

// Get texture for UI
let texture_id = renderer.viewport_texture(viewport);
ui.image(texture_id);
```

### After (Frame Graph Approach)
```rust
// Create viewport (configuration only)
let viewport = renderer.create_viewport()
    .size(512, 512)
    .build(&mut renderer)?;

// Build frame graph with multi-viewport compositing
let graph = FrameGraph::builder()
    .add_pass(GeometryPass::new("viewport_0")
        .write_color("viewport_0", ImageFormat::R16G16B16A16Sfloat))
    .add_pass(CompositePass::new("composite")
        .viewport("viewport_0", ViewportRect::new(0.0, 0.0, 512.0, 512.0))
        .write("backbuffer"))
    .build(&renderer)?;

// Initialize transient textures
graph.initialize_transient_textures(&renderer)?;

// Create compositing descriptor set with viewport textures
let viewport_textures = vec![
    graph.transient_texture("viewport_0", 0).unwrap().image_view_vk(),
];
let compositing_set = CompositingDescriptorSet::new(&renderer.device(), viewport_textures)?;

// Execute frame
graph.execute(&renderer, |frame| {
    frame.submit("viewport_0", &draw_list);
})?;
```

---

## 📊 Implementation Timeline

| Phase | Duration | Description | Status |
|-------|----------|-------------|--------|
| **Phase 1** | 1 day | Compositing pass descriptor set | ⏳ Not started |
| **Phase 2** | 2-3 days | Multi-viewport compositing | ⏳ Not started |
| **Phase 3** | 1 day | Cleanup & migration | ⏳ Not started |
| **Testing** | 1-2 days | Comprehensive tests | ⏳ Not started |

**Total:** 5-7 days (reduced from 5-8 days due to simpler approach)

---

## 🎓 Learning Path

### For Implementation
1. Read [`IMPLEMENTATION-GUIDE.md`](./IMPLEMENTATION-GUIDE.md) → Quick start
2. Read [`katla-rendergraph-analysis.md`](./katla-rendergraph-analysis.md) → Understand current state
3. Follow [`multi-viewport-implementation-plan.md`](./multi-viewport-implementation-plan.md) → Implement

### For Context
1. Read [`vulkan-framegraph-ui-best-practices-2024-2026.md`](./vulkan-framegraph-ui-best-practices-2024-2026.md) → Learn best practices
2. Reference [`katla-rendergraph-analysis.md`](./katla-rendergraph-analysis.md) → Compare with Katla

### For Shaders
1. Go to [`multi-viewport-implementation-plan.md`](./multi-viewport-implementation-plan.md) → Phase 2
2. Copy compositing shader example
3. Adapt to your needs

---

## ✅ Success Criteria

### Functional
- ✅ Can render 4+ viewports in different positions
- ✅ Viewports can overlap with alpha blending
- ✅ No visual artifacts or black screens
- ✅ All existing usage migrated to frame graph
- ✅ Old viewport API completely removed

### Performance
- ✅ No regression vs single viewport
- ✅ Optimal barrier placement (automatic)
- ✅ Fast bindless lookups (single descriptor set)

### Code Quality
- ✅ All tests pass
- ✅ No hybrid approaches
- ✅ Documentation updated
- ✅ Examples work correctly

---

## 🔄 Related Features

This feature enables:
- **Split-screen multiplayer** - 2-4 player local multiplayer
- **Editor tooling** - Preview panels, asset browsers, minimaps
- **Debugging** - Multiple camera views, debug overlays
- **Performance profiling** - Per-viewport performance stats

Future enhancements:
- **Resource aliasing** - Reduce memory footprint (medium priority)
- **Async compute** - Parallel viewport rendering (low priority)
- **Viewport effects** - Per-viewport post-processing

---

## 📞 Support

### Questions?
1. Check [`IMPLEMENTATION-GUIDE.md`](./IMPLEMENTATION-GUIDE.md) → Common Questions section
2. Review [`multi-viewport-implementation-plan.md`](./multi-viewport-implementation-plan.md) → Rollback Plan
3. Reference [`vulkan-framegraph-ui-best-practices-2024-2026.md`](./vulkan-framegraph-ui-best-practices-2024-2026.md) for patterns

### Stuck?
- Each implementation phase has rollback strategies
- Phases are independent - can revert individually
- Git commits are separated per phase

---

## 📝 Changelog

### v1.0 (March 2026)
- Initial research and planning
- Best practices documentation
- Katla implementation analysis
- Detailed implementation plan
- Usage guide

---

## 📄 License

Part of the Katla project. See main LICENSE file.
