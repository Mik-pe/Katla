# Live image-assignment evidence

The contextual participant used a disposable Linux/Vulkan editor built from
`573ea608` plus the image-assignment implementation committed with this report.
Shader sources stayed fixed. The parent supplied the 8×8 checker fixture and
listed tools; it did not edit the participant’s scene.

The [report](observations.md) preserves 42 participant calls, 43 transcript
entries and ten personally inspected native frames. The [scene](assigned.katla)
and [checker](checker.png) retain the choices and fixture; scratch absolute paths
in the captured scene identify the original session and require rebasing for
reproduction elsewhere. This is one contextual episode, not a preference survey.

After the study, discovery gained a precise AssetRef schema and role-specific
neutral values; receipts gained the requested source alongside resolved paths;
authored metadata now distinguishes decoded and GPU formats. These improvements
were not re-tested by the participant in this episode.

Numeric Vulkan acceptance separately verifies color interpretation, independent
image generations, image-only undo/redo, original material isolation, nested
SaveAs rebasing, failed staging rollback and last-owner retirement. Native Metal
acceptance is unavailable on the Linux host.
