struct Frame { view_projection:mat4x4f,camera_position:vec4f,light_direction:vec4f,light_color:vec4f,ambient:vec4f }
struct Vertex { position:vec4f,color:vec4f,uv:vec4f }
@group(0) @binding(0) var<uniform> frame:Frame;
@group(6) @binding(0) var<storage,read> vertices:array<Vertex>;
@group(6) @binding(1) var light_icon:texture_2d<f32>;
@group(6) @binding(2) var fire_icon:texture_2d<f32>;
@group(6) @binding(3) var icon_sampler:sampler;
struct Output { @builtin(position) clip:vec4f,@location(0) color:vec4f,@location(1) uv:vec2f,@location(2) @interpolate(flat) icon:u32 }
@vertex fn vs_overlay(@builtin(vertex_index) index:u32)->Output {
    let vertex=vertices[index]; var result:Output;
    result.clip=frame.view_projection*vertex.position; result.clip.y*=frame.ambient.w;
    result.color=vertex.color; result.uv=vertex.uv.xy; result.icon=u32(vertex.uv.z); return result;
}
@fragment fn fs_overlay(input:Output)->@location(0) vec4f {
    var coverage=1.0;
    if input.icon==1u { coverage=textureSample(light_icon,icon_sampler,input.uv).a; }
    if input.icon==2u { coverage=textureSample(fire_icon,icon_sampler,input.uv).a; }
    let alpha=input.color.a*coverage;
    if alpha<0.01 { discard; }
    return vec4f(input.color.rgb,alpha);
}
