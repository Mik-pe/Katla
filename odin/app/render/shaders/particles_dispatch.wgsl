@group(0) @binding(0) var<storage,read> counters:array<u32,4>;
@group(0) @binding(1) var<storage,read_write> command:array<u32,3>;
@compute @workgroup_size(1) fn cs_main() {
    command[0]=(counters[2]+63u)/64u;
    command[1]=1u;
    command[2]=1u;
}
