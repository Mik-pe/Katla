# Active Context

## Current Work

- Windowed Vulkan limited-frame follow-up (2026-09-05): `-s` means 100 frames. Wayland now uses the actual window extent when the surface leaves sizing to the application. Swapchain recreation rebuilds synchronization objects; explicit renderer shutdown releases the surface before the native display disappears. System-wide Khronos layers are absent; validation runs use the extracted package under `/tmp/katla-vulkan-layers` with explicit loader/library paths. Installing the pacman package requires the user's sudo password.

- Vulkan headless rendering and visual fixes (2026-09-05) are in the working tree. The same scene/editor graph now renders to offscreen targets on Linux; PNG readback needs no display server. Default, playground, selection, hierarchy, asset browser, and Preferences captures are under `.zcode/vulkan-captures/`.
- The visual audit restored missing UI primitives/textures, imported mesh attributes, animation, particles, local lighting, and cascaded shadows. Scene attachments use panel dimensions; editor layout fixes cover hierarchy/inspector width and Preferences controls. Gizmo meshes use unit dimensions and share a smaller screen size between rendering and hit-testing.
- Vulkan shadow encoding now uses one atlas render pass with separate cascade parameters per frame. The unsafe secondary-command-buffer shadow path was removed.
- Shared sky reconstruction follows the actual rasterized NDC and handles an infinite far plane; the ground/horizon gradient is continuous. This supersedes the earlier clip-Y-only fix. Shared shader and widget changes still need macOS/Metal verification on its native runner.
- GPU regression coverage exercises frame-slot reuse, repeated waits, PNG source readback, mixed UI draws, distinct textures, scissor clipping, and material recreation. Final validation results are recorded in `progress.md`.

## Ongoing Architecture Work

- Complete render-graph execution plans (#56): graph-declared attachment/load/store/clear policy, viewport/scissor, and generic executable payloads still need to reach all native handlers. Exact pass identity, application-owned topology, pass-local submissions, explicit picking, and dead-pass culling already exist.
- Preserve the engine/application boundary: custom graphs may be empty, UI-only, reordered, or repeated. Never invent editor topology in a backend.
- Shadow and depth work remain explicit side-effect roots while their native targets are backend-owned. Vulkan particle emission/simulation, animation, and light-culling work also require explicit side effects until their buffers are graph resources.
- The Metal frame-uniform preparation bridge belongs in the eventual frame-slot/buffer ownership design (#36/#31). Some Metal handlers still resolve backend-owned textures; transient allocation is not live-range aliased.
- Metal particle reset and entity-destruction cleanup still need routing through the common emitter driver.
- Private Metal texture storage sampling still needs an Xcode GPU capture; the storage-mode probe is the starting point. Staged uploads and shared storage already work.

## Conventions and Validation Limits

- Reserve declarative editor state slots unconditionally in a stable order. Conditional slots cause cross-view type confusion.
- Use UI design tokens for chrome dimensions. Docked content uses panel bodies; dock tab strips provide titles.
- Vulkan frame waits do not reset fences; reset only immediately before submission. Offscreen submissions complete before returning for deterministic readback ownership.
- Canonical Linux and macOS 26 CI are required before merging. This Linux session cannot validate native Metal rendering.
