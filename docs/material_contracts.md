# Odin material authoring contracts

The agent owns portable typed requests and the canonical tool schemas. The
application owns ECS surface state, imported primitive selection, root/file
capabilities, decoded image generations, atomic mutation and shared history.
The renderer consumes those prepared values and owns native resources. Neither
agent producers nor material asset documents contain GPU handles.

## Factors and independent roles

Base RGB uses sRGB authoring values and alpha is linear. Metallic, roughness,
AO and occlusion strength lie in 0–1. Emission is nonnegative linear HDR RGB.
Normal scale is finite and signed. Coverage mode is opaque, mask or blend;
cutoff is finite/nonnegative and double-sided is explicit. Changing alpha alone
does not change coverage mode. Presets replace complete factors before explicit
patches and preserve image/sampling owners.

Each of the five roles owns independent UV coordinates, offset, rotation,
scale and sampler policy. Rotation is in radians; scale precedes rotation, then
offset. Zero and negative scale are valid. Requested anisotropy is 1–16, needs
linear minification/magnification above one and is bounded by device capability.
A partial patch preserves omitted properties, other roles, factors and images.
The application validates the resulting complete policy against every target's
actual UV availability before publication.

Image choices are inherit, neutral, file or glTF image index. Resource paths
resolve against project resources; Scene paths resolve against the open scene
or the material file when reading a `.katmat`; explicit File references retain
their authorized absolute capability. Assignment preserves factors, sampling
and other roles. Decode/preparation failure changes no target. A successful
batch retains exact prepared before/after image generations for undo and redo;
changing a source file does not mutate existing assigned generations.

## Reusable surfaces

`material_asset describe` returns a complete editable JSON example. On-disk
`.katmat` files use versioned RON. Read returns the complete JSON document;
validate checks a proposed document and referenced images before publication;
write publishes atomically. Capture resolves an object's effective factors,
sampling and image choices, turning inherited images into explicit portable
glTF references or neutral choices. Definitions contain all five roles when
textures are supplied and reject inherit. Omitted textures select five neutral
roles, preventing the target's unrelated original images from being substituted.

Apply preflights 1–256 distinct mesh targets and their required UV sets, prepares
all referenced images, then replaces full factors/sampling/image state as one
shared undo operation. Copies remain independently editable. Writing a material
file preserves live copies; reapply reads a revision. Scene persistence retains
portable references and reads files again when staging a reload.

## Admission, responses and evidence

Malformed fields, unsigned overflow and fractional index aliases fail before
mailbox admission. Decimal generational entity strings retain their exact u64
values. Sampling/image requests own only entity arrays and portable strings,
released with captured allocators. The owner preflights protected/stale targets
and editing mode; successful batches publish one owned command, and failures
change no target. Query and viewport `material_editable` flags reflect prepared
mesh/primitive policy, including noneditable model controllers.

MCP application failures carry `isError: true`, structured success-false/message
metadata and matching textual content. Successful CPU authoring receipts prove
actual state, files and ownership. They do not substitute for native rendering,
image upload, sampler, coverage or lighting validation on Vulkan and Metal.
Viewport PNG/picking evidence remains bound to the same accepted GPU submission;
frustum candidate metadata does not establish occlusion visibility.

The offline protocol consumer is `scripts/validate_odin_mcp.py`. It retains the
existing process/EOF/cancellation tests and exercises complete material factors,
role sampling, actual PNG assignment, atomic failure, capture/read/validate/write/
apply, independent copied revisions and scene reload through real stdio calls.
Package tests separately validate captured allocation and exact undo state.
