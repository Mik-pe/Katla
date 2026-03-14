# Katla Documentation

This directory contains documentation for the Katla 3D render engine.

---

## 📂 Feature Documentation

### [Multi-Viewport Frame Graph](./feature-multi-viewport/README.md)
**Status:** 🚧 Ready for Implementation  
Implements multi-viewport support with frame graph integration.

**What's inside:**
- Best practices research (Vulkan 2024-2026)
- Current implementation analysis
- Detailed implementation plan
- Usage guide

**Quick link:** [`feature-multi-viewport/README.md`](./feature-multi-viewport/README.md)

---

## 📚 Additional Documentation

### Architecture Documentation
- **Frame Graph:** `../katla_gfx/src/render_graph/README.md` (if exists)
- **Renderer:** `../katla_gfx/src/renderer.rs` (inline docs)
- **Viewport System:** `../katla_gfx/src/viewport.rs` (inline docs)

### Build & Development
- **Building:** See `../README.md` in root directory
- **Testing:** Run `cargo test --workspace`
- **Examples:** See `../examples/` directory

---

## 🚀 Quick Start

### I want to implement multi-viewport support:
```bash
cd docs/feature-multi-viewport
ls -la
# Read IMPLEMENTATION-GUIDE.md first
```

### I want to understand the render graph:
```bash
# Read the source code with inline documentation
cd ../katla_gfx/src/render_graph
# Start with graph.rs, then compiler.rs
```

### I want to learn Vulkan best practices:
```bash
cd docs/feature-multi-viewport
# Read vulkan-framegraph-ui-best-practices-2024-2026.md
```

---

## 📖 Documentation Index

| Document | Description | Location |
|----------|-------------|----------|
| **Multi-Viewport Feature** | Complete multi-viewport implementation | [`feature-multi-viewport/`](./feature-multi-viewport/) |
| **Best Practices** | Vulkan 2024-2026 state-of-the-art | [`feature-multi-viewport/vulkan-framegraph-ui-best-practices-2024-2026.md`](./feature-multi-viewport/vulkan-framegraph-ui-best-practices-2024-2026.md) |
| **Katla Analysis** | Current implementation gaps | [`feature-multi-viewport/katla-rendergraph-analysis.md`](./feature-multi-viewport/katla-rendergraph-analysis.md) |
| **Implementation Plan** | Step-by-step guide | [`feature-multi-viewport/multi-viewport-implementation-plan.md`](./feature-multi-viewport/multi-viewport-implementation-plan.md) |
| **Usage Guide** | How to use these docs | [`feature-multi-viewport/IMPLEMENTATION-GUIDE.md`](./feature-multi-viewport/IMPLEMENTATION-GUIDE.md) |

---

## 🎯 For Contributors

### Adding New Features
1. Create a new folder: `docs/feature-<name>/`
2. Add a `README.md` with overview
3. Include:
   - Research/best practices document
   - Implementation analysis
   - Implementation plan
   - Usage guide
4. Update this `README.md` with link to your feature

### Documentation Style
- Use Markdown (`.md` files)
- Include code examples (Rust, WGSL)
- Add diagrams where helpful (ASCII art or mermaid)
- Keep sections concise and scannable
- Use emoji for visual hierarchy (✅, ❌, 🎯, etc.)

---

## 🔗 External Resources

- **WGSL Spec:** https://www.w3.org/TR/WGSL/
- **Vulkan Spec:** https://registry.khronos.org/vulkan/specs/1.3/html/
- **Vulkan Guide:** https://vkguide.dev/
- **Granite Engine:** https://github.com/Themaister/Granite (render graph reference)

---

## 📝 Contributing

When updating documentation:
1. Keep it concise and actionable
2. Include code examples
3. Update the index (this file)
4. Test code examples compile/run
5. Use consistent formatting

---

**Last updated:** March 2026  
**Katla version:** Main branch
