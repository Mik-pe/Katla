# Native precision image acceptance

The canonical `scripts/validate_odin_render.py` runs this fixture after the material
BRDF fixture when both Metal and Vulkan are selected. It uses the same pinned
image decoder and native backend operations as material image admission.

Actual PNG16, big-endian TIFF16 and floating TIFF32 inputs pass through the ordinary
role-aware upload and mip-generation graph. A second exported graph copies base
samples into CPU-visible readback buffers and compares every byte against the
accepted precision conversion. Integer data retains all sixteen bits; integer
color becomes linear half-float with linear alpha; floating HDR retains signed
values and the finite half-float endpoint. Native validation and Odin ownership
tracking must succeed on both backends. Independent CPU regressions check the
color-transfer and half conversion numerically.

GPU launches use the canonical ASan address-check configuration, with external
CF/ObjC/driver process-exit leak detection disabled. Configured CPU tests retain
LeakSanitizer and allocation tracking. This fixture requires macOS arm64 hardware;
portable compilation alone does not establish GPU acceptance.
