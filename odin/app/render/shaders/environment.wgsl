struct Frame { view_projection:mat4x4f,camera_position:vec4f,light_direction:vec4f,light_color:vec4f,ambient:vec4f }
struct LightingFrame { view:mat4x4f,inverse_view_projection:mat4x4f,viewport:vec4f,settings:vec4f }
@group(0) @binding(0) var<uniform> frame:Frame;
@group(3) @binding(0) var<uniform> lighting:LightingFrame;
@vertex fn vs_sky(@builtin(vertex_index) vertex:u32)->@builtin(position) vec4f {
    let p=array<vec2f,3>(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0)); return vec4f(p[vertex],0.5,1.0);
}
@fragment fn fs_sky(@builtin(position) p:vec4f)->@location(0) vec4f {
    if lighting.settings.y==0.0 { return vec4f(0.035,0.04,0.05,1.0); }
    let ndc=p.xy/lighting.viewport.xy*2.0-1.0;
    let world=lighting.inverse_view_projection*vec4f(ndc,0.5,1.0);
    let direction=normalize(world.xyz/world.w-frame.camera_position.xyz);
    let zenith=vec3f(0.07,0.14,0.32); let horizon=vec3f(0.30,0.40,0.56); let ground=vec3f(0.03,0.035,0.045);
    var color=mix(horizon,zenith,pow(max(direction.y,0.0),0.7));
    if direction.y<0.0 { color=mix(horizon,ground,pow(-direction.y,0.5)); }
    let alignment=max(dot(direction,normalize(-frame.light_direction.xyz)),0.0);
    let sun=smoothstep(0.9992,0.9997,alignment)+pow(alignment,32.0)*0.12+pow(alignment,4.0)*0.04;
    return vec4f(color+frame.light_color.rgb*frame.light_color.w*sun,1.0);
}
