# Odin UI rendering and committed picking

Retained UI descriptors and input policy live in `odin/ui`. Application modules
in `odin/app/render/ui_*` shape text, build immutable geometry, resolve opaque
texture identities and append ordinary graph packets. The GPU core contains no
editor, font, widget, picking or ECS policy.

The application keeps one stationary `UI_GPU` owner with its draw, decode and
encode stages, sampler and texture registry. All editor panels, floating docks,
popups, Markdown and code-editor text use the same retained draw list and native
renderer. There is no second widget-specific GPU path. Logical layout/input
coordinates and backing-pixel scale remain explicit at the host boundary.

## Fonts and input

`tools/font_native` builds a pinned FreeType/HarfBuzz/SheenBidi/libunibreak direct
C ABI. Roboto and ForkAwesome come from repository resources; licensed pinned
Noto fallbacks accompany the dependency. Production initialization requires real
measurement, caret, hit-test, visual navigation and grapheme callbacks. The
same UTF8 cluster layouts drive drawing, caret placement and measurement.
UAX14 wrapping reshapes each selected line; UAX29 boundaries drive deletion.
Visual bidi runs select fallback faces for whole graphemes before shaping.
Styled UTF8 byte ranges color shaped clusters without measuring or shaping each
syntax run separately; caret and wrapping continue to use the complete layout.

The atlas contains actual grayscale coverage. Immutable atlas versions and
vertex uploads can be created while a native frame token is acquired: these
operations create fresh allocations with independent staging ownership. They
neither acquire another frame nor mutate an accepted resource. A prepared UI
frame freezes its selected native image handles; later registry or atlas changes
cannot substitute resources into that frame. Accepted native execution retains
removed public handles until completion.

The primary byte-only caret API chooses one position at ambiguous bidi/wrap
boundaries; leading/trailing affinity is not exposed. The pinned Noto Emoji
fallback supplies monochrome glyphs. Color emoji rasterization is not available.

## Color and composition

Theme RGB and ordinary UNORM image RGB are display-encoded sRGB values; alpha
is straight coverage. `UI_Texture.encoding=Linear` marks already linear image
samples. Native RGBA/BGRA sRGB texture formats decode automatically and are never
decoded again. The glyph atlas uses linear white RGB and grayscale alpha.

Each prepared UI frame owns an RGBA16F composition target. The UI shader decodes
display RGB, and native fixed-function blending combines straight alpha in linear
space. Ordered clipped batches preserve image/text order and popup layering;
passes split the immutable sampled-image array into groups of at most 64 images.
A final transfer-only pass encodes the composition once into RGBA/BGRA8 UNORM.
It applies no exposure or tone map. The scene's already tone-mapped encoded
viewport image decodes when sampled and encodes after composition, preserving
its display values wherever the UI leaves it unobstructed.

`clear=false` first decodes the existing target into the linear composition;
that target must permit sampling. A native surface normally uses `clear=true`.
A host creates the owner from a compiled `UI_Shader` for its surface format,
acquires a frame, prepares scene graph roles, registers viewport
textures with their actual graph identities, uploads the prepared UI and appends
it to the combined graph. The whole graph is accepted by one submission.
Source reload prepares every UI stage with the complete editor shader family
before swapping owner handles; rejected candidates retain accepted rendering.
It preserves font/atlas/texture ownership. See [shader reload](odin-shader-reload.md).

## Picking and snapshots

Picking uses an independent exported R32Uint target plus depth. The app derives
exact geometry/model/camera handles, ranges, masks, UVs, vertex/base alpha and
cull variants from prepared scene/model owners. A nonzero ID maps to an owned
immutable generational ECS entity identity. Background is zero. A nonzero
unmapped ID does not imply a scene entity.
Picking depth compare and clear follow the explicit application depth sense;
neither backend transposes camera matrices. Authored transparent masks and
billboard glyph coverage must match their visible fragment policy. UI and gizmo
interaction policy remains in the editor host rather than the ID shader.

`picking_queue` clones the map and queues both color and ID sources from the
same accepted submission, extent and metadata. A capture owns pending tickets
and independently completed halves until the pair completes. A completed
snapshot owns CPU pixels and the entity map; later frames, resize, graph export
release, public handle removal and graph-tail truncation cannot replace them.
Pointer samples use physical coordinates and native top-to-bottom rows. Editor
metadata must distinguish candidates from actually mapped visible pixels.

Graph truncation validates every retained prefix reference before mutation,
releases all tail packet/resource owners and invalidates compiled plans. Hosts
discard removed logical IDs before declaring replacement graph roles.

## Actual native acceptance

`scripts/validate_odin_ui_gpu.py` builds CPU graph/font tests with LeakSanitizer
and a paired native Metal/Vulkan harness. The harness renders a real retained
text input through its production font provider, Swedish preedit/commit and
whole-grapheme deletion, visual RTL movement, shaped Unicode fallback glyphs,
scissored text, popup order and more than 64 sampled images. Readback checks
linear half-alpha blending, encoded/sRGB image passthrough and decoded BMP/TIFF
upload/sampling, plus foremost IDs and alpha masking. It removes public handles
and truncates the graph while accepted work is pending, rejects the stale plan,
and verifies the old paired snapshot across a later resized frame.

Metal API validation and Vulkan Khronos synchronization validation are enabled.
Only actual native GPU processes disable global LeakSanitizer at the external
CF/ObjC boundary; application allocation tracking still requires zero owners.
The current GPU acceptance harness requires macOS Apple Silicon. A build or an
unexecuted host does not establish another platform's GPU acceptance.
Commands and the separate complete-editor acceptance boundary are maintained in
[native GPU validation](metal4_validation.md).
