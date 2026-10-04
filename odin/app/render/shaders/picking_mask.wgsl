// Picking reuses the exact immutable geometry and model data accepted by the scene.
struct Frame { view_projection:mat4x4<f32>,camera:vec4<f32>,light_direction:vec4<f32>,light_color:vec4<f32>,ambient:vec4<f32> }
struct Picking { vertex_stride:u32,object_stride:u32,position_offset:u32,model_offset:u32,object_index:u32,encoded:u32,padding:vec2<u32>,uv_offset:u32,vertex_alpha_offset:u32,object_alpha_offset:u32,vertex_alpha_enabled:u32,cutoff:f32,alpha_mode:u32,pad1:u32,pad2:u32 }
@group(0) @binding(0) var<uniform> frame:Frame;
@group(0) @binding(1) var<storage,read> objects:array<u32>;
@group(0) @binding(2) var<storage,read> geometry:array<u32>;
@group(0) @binding(3) var<uniform> picking:Picking;
@group(1) @binding(0) var base_texture:texture_2d<f32>;
@group(1) @binding(1) var base_sampler:sampler;
struct Output { @builtin(position) position:vec4<f32>,@location(0) uv:vec2<f32>,@location(1) alpha:f32 }
fn object_vec4(offset:u32)->vec4<f32> { return bitcast<vec4<f32>>(vec4<u32>(objects[offset],objects[offset+1],objects[offset+2],objects[offset+3])); }
@vertex fn vs_pick(@builtin(vertex_index) vertex:u32)->Output {
    let p=vertex*picking.vertex_stride+picking.position_offset;
    let position=bitcast<vec4<f32>>(vec4<u32>(geometry[p],geometry[p+1],geometry[p+2],geometry[p+3]));
    let o=picking.object_index*picking.object_stride+picking.model_offset;
    let model=mat4x4<f32>(object_vec4(o),object_vec4(o+4),object_vec4(o+8),object_vec4(o+12));
    let world=model*position;
    var output=frame.view_projection*world;
    output.y*=frame.ambient.w;
    let v=vertex*picking.vertex_stride;
    let uv=v+picking.uv_offset;
    var vertex_alpha=1.0;
    if picking.vertex_alpha_enabled!=0u { vertex_alpha=bitcast<f32>(geometry[v+picking.vertex_alpha_offset]); }
    var result:Output;result.position=output;result.uv=bitcast<vec2<f32>>(vec2<u32>(geometry[uv],geometry[uv+1]));
    result.alpha=vertex_alpha*bitcast<f32>(objects[picking.object_index*picking.object_stride+picking.object_alpha_offset]);
    return result;
}
@fragment fn fs_pick(input:Output)->@location(0) u32 {
    let alpha=textureSample(base_texture,base_sampler,input.uv).a*input.alpha;
    if picking.alpha_mode==1u { if alpha<=0.0 { discard; } }
    else if alpha<picking.cutoff { discard; }
    return picking.encoded;
}
