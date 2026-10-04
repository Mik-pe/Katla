# Odin glTF model ownership

`app.gltf_load` reads actual glTF 2.0 JSON or GLB through an explicit retained
`resources.Root`. The pinned [C99 parser dependency](../tools/cgltf/README.md)
parses and validates the document. Every external buffer/image is resolved
relative to the model source and read through the same confined root; percent
encoding and parent segments are normalized before opening, and escaping the
root or following a symlink fails. Network URIs are rejected. Embedded GLB,
base64 data URIs, sparse accessors and normalized integer attributes are
supported. Parsed storage spans are checked before calling the C validator or
accessor converter.

`Gltf_Model` owns typed primitives, original node indices, selected scenes,
materials, images, textures, samplers, skins and canonical `Animation_Model`
timelines. It retains no parser pointers or native GPU handles. Primitives use
the canonical indexed `Mesh_Geometry` with separate skin influences, vertex
colors, UV sets and morph deltas. Source triangle strips/fans convert with their
exact winding. Static imported triangles preserve their authored units;
genuinely zero-area triangles remain rejected.

Material factors retain their glTF linear interpretation. Base color, normal,
metallic/roughness, occlusion and emissive texture views preserve texture index,
sampler/image identity, UV set, scale and `KHR_texture_transform`. Required
specular/glossiness materials preserve that distinct workflow, diffuse/specular
factors, glossiness and both texture views. Unlit and emissive-strength metadata
are retained. Images own their actual encoded PNG/JPEG source bytes; decoding,
color-space selection, uploads and native texture lifetime belong to the app
renderer. An importer success does not establish native support for every
material workflow.

## Live texture revisions

External images retain their confined, model-relative `source_path` through CPU
ownership and history cloning. `scene_model_image_read` reads that capability;
embedded GLB and data-URI images are re-extracted from a validated source model.
Refreshing images does not replace the authored `Scene_Model`, document,
selection, simulation state or Undo history.

The application calls `model_texture_reload_poll` once per owner after retiring
its accepted frame and before acquiring the next slot. The default interval is
one second. Exact encoded bytes define freshness, so equal-size edits and
unchanged timestamps are observed. Each source image is read once per poll
across the camera caches. Missing, malformed or native-rejected candidates stay
retryable; the complete previous accepted image revision continues rendering.

Changed images receive new native allocations, real transfer uploads and mip
generation before publication. All active caches prepare their textures,
samplers, resource mappings and graph plans together. Changed dimensions and
mip counts replace the same logical imported image declaration through
`graph_replace_image`; repeated resizing does not append unused resources.
Previously prepared recordings retain their immutable descriptors and native
owners. Publication replaces the whole batch, then removes old native handles.
A cleanup error is reported separately from acceptance.

`odin/examples/texture_reload` exercises four real model caches on Metal and
Vulkan. Strict ASan native validation proves malformed-image, first-allocation,
partial-batch and sampler failures preserve live red pixels, followed by exact
green/blue/red pixels, resized mip chains and bounded graph resource counts.
The same native fixture installs one aggregate scene participant: an unrelated
Transform edit, Undo and Redo rebuild all four model caches from actual sources
and preserve fresh green pixels while the immutable CPU image still contains
the original red bytes. CPU checks cover source identity cloning, embedded
GLB/data-URI re-extraction, exact-byte freshness and immutable
prepared recordings across descriptor replacement and rollback. Actual editor
host integration uses the same service; the headless fixture proves native
replacement rather than operating-system file notification delivery.

Every original node contributes its local TRS and exact column-major matrix.
Parents preserve ancestors that are not joints. Skins preserve their joint-to-
node mapping and inverse bind matrices. `gltf_world_matrices` samples canonical
timelines for all nodes. Static matrix-authored nodes retain exact affine
matrices, including shear and singular transforms; only authored TRS channels
replace local TRS. TRS animation targeting a matrix-authored node is rejected. `gltf_skin_matrices` returns mesh-local joint matrices
using `inverse(mesh world) * joint world * inverse bind`, without transposing
matrix storage. `gltf_deform_geometry` applies arbitrary sampled morph weights
and four skin influences to genuine indexed geometry, transforms normals and
tangents, and recomputes bounds for ordinary rendering consumers.

Animations retain STEP, LINEAR and CUBICSPLINE interpolation. TRS channels use
canonical vector/quaternion sampling; morph channels retain their complete
packed component stream. Missing morph channels sample authored bind weights.
Unnamed clips receive `Animation_<source index>` names; duplicate names receive
a source-index suffix so controls can identify one exact clip.

`Scene_Model` persists only `Gltf_Source {path, root}` with `Resource` or `Project`
origin. Its contextual decoder loads and validates the entire revision before
entity publication. ECS history clones the actual immutable CPU revision;
history replay does not reread a changed file. Source snapshot restoration
reloads the source through the retained root and preserves the current scene if
preparation fails. Native caches are separate participants in scene staging.

Budgets bound parser allocation, aggregate dependency/output bytes, nodes,
primitives, geometry, hierarchy depth and morph/animation cardinality. The
current application geometry contract supports indexed triangles, up to four
skin influences, 32 UV sets and PNG/JPEG images. Points/lines, additional skin or
color attribute sets, Draco/meshopt compression, Basis/KTX images, unknown
required extensions fail
explicitly. Supported optional extension fallback data follows glTF rules. A
skinned vertex with zero total influence is rejected; the repository's
`FoxBlender.glb` contains all-zero weights and is tested as malformed input.

Acceptance imports the shipped Box GLTF/external buffer, GLBs, interleaved Box,
DamagedHelmet textures, Avocado, Lantern, Fox/FoxFixed, Tiger specular/glossiness
and plane assets. Actual Fox animation deforms imported vertices. A generated
real glTF fixture proves sparse positions, normalized integer UVs/skin weights,
multiple morph components and exact mesh-local skinning. Source codec,
deep-clone/history, snapshot recovery, URI confinement and overflowing storage
rejection use the same production importer. These CPU checks do not replace
native rendering validation of the affected app path.

The format's authoritative contract is the [Khronos glTF 2.0 specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html).

Normal-map tangent generation selects the normal texture's UV set, including
KHR_texture_transform coordinate overrides, rotation and scale. Explicit source
tangents remain authoritative. The importer preserves canonical geometry UV0
separately from the transformed tangent coordinates. `models/TangentUV.gltf`
is a reproducible real asset with embedded PNG normal data, shared eight-vertex
accessors and two distinct indexed primitives. It proves authored tangent
retention and transformed UV1 generation. Renderer primitive bounds are computed
from referenced deformed triangle vertices, so shared accessors do not conflate
transparent primitive centers. Perspective camera depth is clip W at that
world-space center, independent of node origin and clip-Y adaptation.
