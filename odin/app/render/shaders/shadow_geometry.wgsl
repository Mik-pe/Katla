struct Frame { view_projection:mat4x4f,camera_position:vec4f,light_direction:vec4f,light_color:vec4f,ambient:vec4f }
struct ShadowCascade { view_projection:mat4x4f,split_texel:vec4f }
struct ShadowFrame { cascades:array<ShadowCascade,4>,direction:vec4f,bias:vec4f }
struct Phase { index:u32,clip_sign:f32,pad:vec2u }
struct Outline { parameters:vec4f,color:vec4f }
@group(0) @binding(0) var<uniform> frame:Frame;
@group(0) @binding(1) var<storage,read> objects:array<Object>;
@group(0) @binding(2) var<storage,read> geometry:array<Vertex>;
@group(4) @binding(0) var<storage,read> shadow:ShadowFrame;
@group(4) @binding(3) var<uniform> phase:Phase;
@group(5) @binding(0) var<uniform> outline:Outline;
struct CoverageOutput { @builtin(position) position:vec4f,@location(0) uv:vec2f,@location(1) alpha:f32,@location(2) @interpolate(flat) object:u32 }
fn coverage_output(clip:vec4f,vertex:u32,object:u32)->CoverageOutput { return CoverageOutput(clip,coverage_uv(geometry[vertex]),objects[object].color.a*coverage_vertex_alpha(geometry[vertex]),object); }
@vertex fn vs_shadow(@builtin(vertex_index) vertex:u32,@builtin(instance_index) object:u32)->CoverageOutput {
    let world=objects[object].model*geometry[vertex].position;
    var clip=shadow.cascades[phase.index].view_projection*world;
    clip.y*=phase.clip_sign; clip.z=max(clip.z,0.0); return coverage_output(clip,vertex,object);
}
@vertex fn vs_mark(@builtin(vertex_index) vertex:u32,@builtin(instance_index) object:u32)->CoverageOutput {
    let world=objects[object].model*geometry[vertex].position;
    var clip=frame.view_projection*world; clip.y*=frame.ambient.w; return coverage_output(clip,vertex,object);
}
@vertex fn vs_outline(@builtin(vertex_index) vertex:u32,@builtin(instance_index) object:u32)->CoverageOutput {
    let surface=objects[object]; let world=surface.model*geometry[vertex].position;
    var clip=frame.view_projection*world;
    let origin=frame.view_projection*surface.model[3]; let direction=clip.xy-origin.xy;
    if length(direction)>0.001 { clip=vec4f(clip.xy+normalize(direction)*outline.parameters.x*clip.w,clip.zw); }
    clip.y*=frame.ambient.w; return coverage_output(clip,vertex,object);
}
@fragment fn fs_depth() {}
@fragment fn fs_outline()->@location(0) vec4f { return outline.color; }
@fragment fn fs_indicator()->@location(0) f32 { return 1.0; }
