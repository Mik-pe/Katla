# Material interface follow-up: same agent, prior study context

This episode used the newly built isolated editor through `/tmp/katla-material-interface-90621117.sock`. It is the same model/agent with the earlier baseline study in context, not a fresh independent participant, a general LLM preference study, or a controlled comparison of model performance. The investigator stated that shader sources would remain unchanged during this episode. I made 15 transport calls, rediscovered the schemas, read only tool/evidence output, and personally inspected all four native PNGs. I did not modify source, save a scene, delete shared cache content, or connect the external Scene assistant.

Evidence: [transcript.jsonl](transcript.jsonl), with one-based line references below. Comparison refers to this agent's [baseline observations](../baseline/observations.md) and [baseline transcript](../baseline/transcript.jsonl).

## Verified behavior

| Check | Result and evidence |
| --- | --- |
| Capability discovery | `material presets` now returns `capabilities`: alpha does not change render mode; RGB is sRGB and alpha linear; emission and texture editing are false; presets are isotropic factors with no directional brushing; batches are atomic with maximum size 256. Inspect and set repeat these limits. Transcript 1–2, 5–7. These are declared limits, not separate GPU tests for every unsupported feature. |
| Schema | Material schema explicitly rejects additional properties, bounds factor values to 0..1, requires decimal-string IDs, bounds batches to 1..256, and declares unique targets. The description explains alpha and preset limitations before any mutation. Transcript 1. I did not probe every schema constraint. |
| Editable target flags | Query returns `material_editable: true` for all 27 rendered studio objects and `false` for daylight `"28"`. Sphere `"11"` is distinguishable from porcelain plinth `"10"`; sphere `"15"` from bronze plinth `"14"`. View candidates also carry the flag. Transcript 3, 8. |
| Preset plus overrides | Set `"11"` with ceramic preset, dark-blue sRGB `[0.025,0.07,0.22,1]`, roughness 0.09. Receipt contains `before` (white ceramic, roughness 0.18), `entity_id`, and `values` (current blue, roughness 0.09). Set `"15"` with brushed-metal preset and roughness 0.4; receipt records the bronze before state and silver after state, with explicit roughness overriding preset 0.32. Inspects agree. Transcript 5–7, 15. |
| Native appearance | Initial frame shows white, steel and bronze spheres. After edits, the left sphere is dark navy with a small bright highlight; the right is silver metal with a broader round highlight. No directional brush pattern is visible, consistent with declared isotropic factors. [Initial native frame](view-0.png), frame 888; [edited samples](view-1.png), frame 1251. |
| Zero candidate observation | `editor_view observe` with `limit: 0` returns a native PNG and `candidates: []`. It retains total `candidate_count`, camera, frame, picks and `truncated: true`; there are no candidate rows. Verified twice. Transcript 4, 14; [initial PNG](view-0.png), [final PNG](view-3.png), frame 1741. |
| Invalid nonmesh batch | Deliberately sent a red/roughness-0.8 patch to `"11"` plus daylight `"28"`, despite the discovered false flag, as the requested negative test. Response has `isError: true`, structured `success: false` and `message: Entity Entity(28:0) has no rendered material; choose a mesh object`. Reinspect confirms the valid member stayed blue at roughness 0.09. Transcript 9–10. |
| Recovery and undo | After failure, a valid roughness-only patch to `"11"` succeeds; receipt shows 0.09 before and 0.15 after, preserving its blue color, metallic and AO. One undo restores roughness 0.09. Final inspect and native images confirm blue porcelain and silver right sphere remain intact. Transcript 11–15; [undo frame](view-2.png), frame 1734; [final image-only observation](view-3.png). |

## Comparison with this agent's baseline

The new capability statements resolve the key semantic uncertainties from the baseline. I no longer need to send an unsupported emissive probe or infer transparency from stored alpha. The `brushed_metal` name remains, but its limits are stated explicitly in schema, prose and results. RGB/alpha color semantics are also separated explicitly.

The `material_editable` flag supplies the missing target suitability signal. Both samples and plinths remain true, so it does not determine user intent; names, positions and the image still distinguish the desired object. A direct editable-only filter could reduce query output, but filtering the returned rows is now straightforward.

Before/current factor receipts make preset overrides and partial preservation reviewable without a separate pre-edit inspect. The current state is called `values`, rather than `after`, but its relationship to `before` was clear in this session. These receipts are practical evidence of the applied authoring change; native images remain necessary to assess its appearance.

The baseline nonmesh batch already preserved its valid member, but reported `isError: false`. The revised structured `isError: true` makes failure easier to detect. This tested failure still lacks a machine-readable error code or explicit offending ID field; it embeds `Entity(28:0)` in prose. Other tool envelopes still vary, for example query results retain nested `data`/`success`, while material uses one main data object. This episode does not establish global response-envelope consistency.

The baseline `limit: 0` unexpectedly returned one candidate. Both follow-up observations return zero rows while preserving the PNG. This is a useful reduction in output when I already know the scene targets. Undo still returns view data/image without an explicit operation receipt or affected IDs; the follow-up inspect confirms what changed.

Raw factor floats remain verbose. These are small interface issues compared with the resolved ambiguity about unsupported features. This is this agent's contextual assessment, not generalized preference evidence.

## How I would handle translucency, emission and brushing requests

Using only the discovered material interface, I would state the supported result before making an edit:

- **Translucency:** These tools can tint a surface and store an alpha multiplier, but alpha does not switch render mode. I cannot claim that lowering alpha creates a translucent surface. A render-mode/transmission authoring action is not exposed by the discovered material schema.
- **Emission:** Material capabilities explicitly say emission is not editable, and no emissive field is in the schema. I would report that an emissive surface cannot be authored through this material tool, without inventing fields or disguising a regular bright color as emission.
- **Brushed finish:** I can apply the preset's silver metallic/roughness factors as an isotropic metal approximation. I would explain that it does not create directional brushing; this session's native image supports that limit. If the requested outcome requires visible brush grain, the exposed texture/anisotropy controls are insufficient.

For supported edits, I would continue using the compact `action`, decimal-string `entity_ids`, preset and explicit override shape demonstrated here, choosing true material-editable rows, reviewing the before/current receipt, then checking a native image. The improved discovery lets me identify the unsupported portion of a request before trying ineffective material changes.
