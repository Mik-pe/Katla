# Metal 4 delivery validation

The graphics batch covers issues #31, #32, #33, #35, #36, #53, #54, #55 and #58. The ECS redesign in #138 was delivered first. This report records physical Metal validation and actual Vulkan execution, rather than inferring GPU behavior from compilation.

## Native acceptance

Local Metal runs used Apple M5 (10 CPU and GPU cores, 24 GiB), macOS 27.0 build 26A428, with `MTL_DEBUG_LAYER=1` and `METAL_DEVICE_WRAPPER_TYPE=1` set before process launch. CI targets the explicitly supported `macos-26` runner. Its hosted virtual GPU was observed to lack `MTLGPUFamilyMetal4` support; CI explicitly reports native acceptance BLOCKED on such devices, exercises typed rejection, and retains all 72 Metal CPU/reflection/plan tests alongside the portable graph tests. The portable selection contains 628 runnable tests and nine already ignored Vulkan hardware tests; it filters 110 Metal GPU tests, and the rejection test runs separately. The native output/lifetime evidence below comes from physical M5 hardware, not from that virtual GPU.

The complete local workspace library suite passed **2,179 tests** (15 ignored). The Metal contract suite passed all **13 scenarios**, and the application passed all **8 interaction checks** with 13 captures.

Coverage includes direct and indirect WGSL compute with same-pass visibility; animation matrices across queued slots; a small particle pool with actual native array bounds; imported attachments and undefined-content rejection; placement-heap aliasing; immutable mesh, UI and bindless publication; private texture uploads, sampling and readback; asynchronous pipeline replacement; native archive hits; completion-owned timestamp results; and abort without submission. Picking reads the last committed graph attachment and preserves global nonzero instance IDs across instanced draws. The editor interaction test selects the exact expected object and exercises hierarchy scrolling, preferences, deselection and component changes.

Linux acceptance used Rust 1.99, Mesa 25 lavapipe and active Khronos synchronization validation in a disposable ARM64 Linux container. The graphics library passed 560 tests (9 ignored), the contract suite passed seven supported scenarios, and both transient aliasing tests passed eight resize cycles with two queued frame slots. The six PBR contract scenarios retain the existing lavapipe driver-crash exclusion; they run on native Metal. Vulkan tests verify that the validation messenger is actually enabled, allocation aliases are physically shared, edge pixels survive resize, and submitted results have the expected extents. Strict production Clippy passed on both platforms.

## Reproducible scene observations

The baseline is commit `40743ce9a565299072b461fe32eb8205559dbc9c`: the delivered ECS implementation with the previous Metal renderer. It was rebuilt separately, using its own shaders. Baseline instrumentation measures entry to native submission, sums all submissions within a frame, and includes the previous global completion wait in the frame duration. Current instrumentation measures completion/residency registration through native queue commit, the exact reused-slot wait, and completed prior-slot GPU timing.

Both renderers use the default editor scene at 2560 × 1440 for 130 headless frames with API validation. The first five samples are excluded from the summaries. The build profile is development with optimization level 1. Each cache observation is a separate application process; the cold process starts with an empty cache and the warm process uses the resulting archive. There are no pipeline compilations after frame rendering begins.

| Observation | Previous renderer | Metal 4 cold | Metal 4 warm |
| --- | ---: | ---: | ---: |
| Whole-frame median | 6.215 ms | 6.986 ms | 6.605 ms |
| Whole-frame p95 | 7.035 ms | 9.792 ms | 7.997 ms |
| Aggregate CPU submit median | 11.501 µs | 53.959 µs | 47.875 µs |
| Reused-slot wait median | Included in frame | 0.387 ms | 0.351 ms |
| Completed prior-slot GPU median | Not instrumented | 3.926 ms | 3.931 ms |
| Native submissions per steady frame | 3 | 1 | 1 |
| Process launch to frame-loop entry | Not instrumented | 2.755 s | 0.360 s |
| Native archive hit / miss counters | Not instrumented | 7 / 62 | 69 / 0 |



The warm sequence has median CPU lead/in-flight depth 1 and maximum 2. Native stress tests queue three distinct slots before reuse and validate their independent contents.

Raw samples are in [baseline](benchmarks/metal4-frame-baseline.csv), [cold](benchmarks/metal4-frame-cold.csv), and [warm](benchmarks/metal4-frame-warm.csv). [Environment and provenance](benchmarks/metal4-environment.json) records source and binary hashes, capture hashes, compiler/OS details, validation flags, instrumentation scope and exact compared image regions.

The baseline makes three native submissions per steady frame; the new path makes one. CPU submission cost can increase with explicit resource ownership, residency and feedback registration. The final observed whole-frame duration also increased relative to this baseline; neither throughput nor lower CPU submission cost is an acceptance claim. These short measurements share a busy host, use validation, and contain animation driven by wall-clock time. They demonstrate the changed execution behavior and observed durations; they do not establish an isolated Metal 4 speedup or a production FPS guarantee.

## Image comparison

The baseline and current warm process use identical saved editor preferences. Eight checked RGB regions cover static materials, shadows, ground, sky, top toolbar, hierarchy and inspector. All eight regions have zero mean and maximum RGB error.

Animated particle overlays, animated poses and the footer's performance values are excluded because they depend on elapsed time. Native upload tests separately prove texel identity, format/subresource handling and direct/bindless sampling after shared versus staged private publication. Windowed limited-frame execution and the complete editor interaction sequence also run under Metal API validation.

## Follow-up scope

At this delivery, #37 still tracked joining native residency/binding/feedback information into the portable graph-capture bundle, and #93 tracked separating the graphics core from scene/editor features. Neither was counted in this ten-issue batch. The subsequent implementation is described in [graphics core ownership](graphics_core.md) and [render-graph capture](render_graph_capture.md).

## Graphics-core and capture follow-up acceptance

The follow-up above `144aefd452fea6e342b23f20fb8c20b5847f5612` implements #93 and #37 and cleans ECS/gfx responsibilities. These observations are separate from the preceding ten-issue measurements. Rust 1.99 strict Clippy passes ECS, gfx and app across all targets and features. The full all-feature workspace library suite passes 2,164 tests (23 ignored), and the native Metal contract suite passes all 13 scenarios. Two native app builder tests prove that GraphOnly needs no fonts or scene initialization and that the default ambient-occlusion texture is independent of the font atlas. ECS additionally passes integration/doctests and 48 focused Miri tests.

Native Metal regressions cover foreign-renderer acquisition tokens, discarded command allocator reuse, colorless warmup, compute consumers, passive capture equivalence and draining retained readback tickets. Native Vulkan tests cover submission rejection/recovery, post-submit surface outcomes, completed-buffer ownership across fence reuse, headless resize, retained source tickets, actual compute ranges, resource handles and aliasing. The final Linux graphics library passes 553 tests (15 ignored), the supported contract selection passes seven scenarios, and 35 public native resource/render fixtures pass. The existing six lavapipe PBR driver-crash exclusions remain; all corresponding Metal scenarios pass.

The final editor executes 130 headless and windowed frames with both Metal validation environment flags. All eight UI interaction checks pass, including exact GPU selection, deselection, theme controls and component addition/removal. The final image has exact RGB equality with the corrected intermediate build in all eight measured static regions. Against the preceding delivery, sky and static editor regions remain identical; material and ground regions intentionally differ because the old default AO index selected the font atlas. The app now supplies an explicit white AO texture, verified by the native builder regression. No throughput or performance improvement is claimed. [Follow-up provenance](benchmarks/graphics-core-environment.json) records source/binary/image hashes, region differences, commands and local receipts.

Hosted macos-26 native GPU acceptance remains capability-dependent. A successful portable CI job on a virtual device without Metal 4 does not replace the physical M5 acceptance above.
