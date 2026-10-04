// Picking reuses the exact immutable geometry and model data accepted by the scene.
struct Frame { view_projection:mat4x4<f32>,camera:vec4<f32>,light_direction:vec4<f32>,light_color:vec4<f32>,ambient:vec4<f32> }
struct Picking { vertex_stride:u32,object_stride:u32,position_offset:u32,model_offset:u32,object_index:u32,encoded:u32,padding:vec2<u32> }
@group(0) @binding(0) var<uniform> frame:Frame;
@group(0) @binding(1) var<storage,read> objects:array<u32>;
@group(0) @binding(2) var<storage,read> geometry:array<u32>;
@group(0) @binding(3) var<uniform> picking:Picking;
fn object_vec4(offset:u32)->vec4<f32> { return bitcast<vec4<f32>>(vec4<u32>(objects[offset],objects[offset+1],objects[offset+2],objects[offset+3])); }
@vertex fn vs_pick(@builtin(vertex_index) vertex:u32)->@builtin(position) vec4<f32> {
    let p=vertex*picking.vertex_stride+picking.position_offset;
    let position=bitcast<vec4<f32>>(vec4<u32>(geometry[p],geometry[p+1],geometry[p+2],geometry[p+3]));
    let o=picking.object_index*picking.object_stride+picking.model_offset;
    let model=mat4x4<f32>(object_vec4(o),object_vec4(o+4),object_vec4(o+8),object_vec4(o+12));
    let world=model*position;
    var output=frame.view_projection*world;
    output.y*=frame.ambient.w;
    return output;
}
@fragment fn fs_pick()->@location(0) u32 { return picking.encoded; }
