# Material authoring study: one independent LLM session

This report describes one model/session using the disposable native editor through its real MCP tools. It is not a claim about all LLMs. I used 28 transport calls, including schema discovery. I did not read implementation source or validation walkthroughs, edit repository code, or connect the Scene assistant to an external conversation. Tool evidence is in [transcript.jsonl](transcript.jsonl); references below are one-based line numbers.

The parent investigator reported that shader sources were being modified concurrently and could be reloaded by the editor asset watcher. I observed no interrupted calls or explicit shader-reload failure. These images document the frames I saw; they are not reproducible visual goldens or an isolated shader comparison.

## Outcomes and evidence

| Task | Observed result | Evidence |
| --- | --- | --- |
| Find porcelain | Query distinguished `Materials / Porcelain` (`"11"`, sphere at x=-2, y=1.5) from its plinth (`"10"`). Inspect matched the ceramic preset. Initial image showed the white sample at that location. | Transcript 2–5; [initial native frame](view-0.png), frame 1102 |
| Glossy dark blue | Set ceramic preset plus sRGB `[0.025, 0.07, 0.22, 1]`, roughness 0.09. Inspect returned metallic 0 and the requested color/roughness. Native frame showed dark navy with a tight white highlight. | Transcript 6, 8, 10; [edited samples](view-1.png), frame 1425 |
| Brushed-metal variant | Applied discovered `brushed_metal` preset to the former bronze sample (`"15"`). Inspect returned sRGB `[0.68, 0.72, 0.76, 1]`, metallic 1, roughness 0.32. It visibly changed from bronze to silver with a broader highlight, similar to the existing steel sample. No directional brushing/striations were visible; the tool exposes no brush direction, anisotropy or texture authoring. Thus silver metal succeeded, while a specifically brushed finish was not visually established. | Transcript 7, 11; [initial](view-0.png), [silver result](view-1.png) |
| Translucent colored surface | Set the center sphere (`"13"`) to green sRGB `[0.06, 0.75, 0.38, 0.25]`, metallic 0, roughness 0.08. Set and inspect reported success and retained alpha 0.25. The native image showed an opaque green sphere with no wall visible through it. This request did not produce visible translucency in the observed frame. | Transcript 12–13, 16; [alpha experiment](view-2.png), frame 2798 |
| Emissive surface | Discovery exposed no emissive material field, and registered components exposed lights but no emissive surface component. An explicit negative capability probe on ornament `"27"` with `emissive` and `emissive_strength` was rejected: `unknown field emissive`, listing supported fields. Inspect and native image continued to show the ceramic ornament. No emissive surface was produced. | Transcript 1, 9, 14–16; [unchanged ornament](view-2.png) |
| Invalid batch recovery | Submitted a red material patch for blue porcelain `"11"` and daylight `"28"`. Rejected with `Entity Entity(28:0) has no rendered material; choose a mesh object`. Reinspect showed porcelain still dark blue and the silver sample unchanged. The batch caused no partial edit to its valid member. I recovered by continuing with valid history/save operations, without restoring or disturbing the completed edits. | Transcript 17–19 |
| Undo | One undo restored center sphere from the alpha experiment to the original steel factors, leaving porcelain and the silver variant intact. Reinspect confirmed alpha 1, metallic 1, roughness 0.32. The failed requests did not appear to consume an undo entry. | Transcript 20–21; [after undo](view-3.png), frame 3128 |
| Persistence | Saved and loaded scratch [material-study.katla](material-study.katla). Requeried after load: porcelain `"39"`, original steel `"41"`, former bronze/silver variant `"43"`. Inspects confirmed both completed edits and the undone center sample persisted. Native frame showed blue left and silver center/right. | Transcript 22–28; [after reload](view-4.png), frame 3534 |

## Interface observations

- Discovery was sufficient for choosing samples and completing factor edits. `material presets` explicitly requests first-use discovery; `preset` plus explicit overrides is a useful compact request. IDs remained decimal strings throughout and were requeried after load as required.
- Names are descriptive, but substring results include both sample and plinth. The combination of bounds, positions and viewport made the intended sphere clear. `Materials / Bronze` remains the object's name after its appearance becomes silver; names are identifiers supplied by the scene, not current material classifications.
- The base-color space was clear: tool description, presets, set results and inspect all say `srgb`. However, the single `color_space` label on an RGBA array does not explain alpha's role, render mode, blend mode, or transmission. Alpha acceptance without visible transparency was the main data-versus-image mismatch.
- `brushed_metal` is a stronger visual promise than these scalar factors established in this scene. A metal preset is useful, but creating a directional brushed appearance needs a discoverable capability or an explicit limitation.
- `query_entities` lists only `NameComponent` and `TransformComponent` on mesh objects. A component filter cannot be inferred for material-capable meshes from this output. A `has_material`/renderable capability filter would simplify safe batches.
- Error channels differ. Unsupported emissive arguments return `isError: true` with plain text and no `structuredContent`; a nonmesh material target returns `isError: false` with structured `success: false`. Scene query, component discovery and save/load also nest `data`/`success` differently from material results. A single response envelope and machine-readable error code would simplify independent tool use.
- Runtime rejects unknown material fields, but the displayed material JSON schema does not explicitly set `additionalProperties: false`. Adding it would make the rejection predictable before the call.
- `editor_view observe` with `limit: 0` returned one candidate, despite the schema allowing zero. `limit: 1` also returned one candidate. I wanted an image without the long candidate list. Documenting the clamp or supporting image-only observation would help.
- Inspected floats have verbose precision and small sRGB round-trip differences (blue green channel approximately 0.06999999 before reload versus 0.069999985 after). These were immaterial to visible persistence. Reporting concise author-facing factors alongside raw values would reduce noise.
- Undo returns a fresh image and camera/candidate metadata but no explicit operation label or affected IDs. The inspect call established what was undone. An undo result naming the operation would make recovery more reviewable.

## Preferred request shapes for this session

Keep the existing small action-discriminated material tool with `entity_ids` as decimal strings and partial patches. The working request was:

```json
{"action":"set","entity_ids":["11"],"preset":"ceramic","base_color":[0.025,0.07,0.22,1],"roughness":0.09}
```

Useful additions would be discoverable supported features/limits, an explicit transparency mode plus opacity/transmission semantics, emissive RGB with a documented color space and strength, and directional brush controls or texture binding. These are desired interfaces, not capabilities demonstrated here. Preserve atomic batches and return changed IDs, before/after factors and one consistent success/error envelope. An inspection response should identify effective render mode and whether the requested factors affect the native path, so stored alpha cannot be mistaken for verified transparency.
