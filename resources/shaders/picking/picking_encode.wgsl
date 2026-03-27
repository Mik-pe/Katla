// Picking ID encoding compute shader.
// Simulates the fragment shader's object-ID encoding logic from object_id.wgsl.
// Each invocation reads an instance_index from the input buffer and writes
// instance_index + 1 to the output buffer (the picking ID encoding).
// An instance_index of 0xFFFFFFFF is treated as "no object" (background → 0).

@group(0) @binding(0)
var<storage, read> input: array<u32>;

@group(0) @binding(1)
var<storage, read_write> output: array<u32>;

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) gid: vec3u) {
    let idx = gid.x;
    if (idx >= arrayLength(&input)) {
        return;
    }

    let instance_idx = input[idx];

    // Match object_id.wgsl: encode instance_index + 1
    // 0xFFFFFFFF means "no object" → encode as 0 (matches cleared/background)
    if (instance_idx == 0xFFFFFFFFu) {
        output[idx] = 0u;
    } else {
        output[idx] = instance_idx + 1u;
    }
}
