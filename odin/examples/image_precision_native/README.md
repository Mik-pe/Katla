# Native precision image acceptance

The canonical `odin run tools/build -- validate render` runs this fixture when Metal is
selected, and also on Vulkan when both backends are selected. Pass `--metal-only`
to the executable for Metal alone. It uses the same Odin image decoder and native
backend operations as material image admission.

Actual PNG16, big-endian TIFF16, floating TIFF32 and progressive JPEG inputs
(4:4:4, 4:2:2, 4:2:0 and restart markers) pass through the ordinary
role-aware upload and mip-generation graph. A second exported graph copies base
samples into CPU-visible readback buffers and compares every byte against the
accepted precision conversion. Integer data retains all sixteen bits; integer
color becomes linear half-float with linear alpha; floating HDR retains signed
values and the finite half-float endpoint. Native validation and Odin ownership
tracking must succeed on both backends. Independent CPU regressions check the
color-transfer and half conversion numerically.

Native GPU harnesses use speed optimization with ASan address checks when
requested; CPU suites retain their configured optimization. GPU launches use the canonical ASan address-check configuration, with external
CF/ObjC/driver process-exit leak detection disabled. Configured CPU tests retain
LeakSanitizer and allocation tracking. This fixture requires macOS arm64 hardware;
portable compilation alone does not establish GPU acceptance.
