struct DisplaySettings { exposure: f32, mode: u32, _pad: vec2<u32> }
@group(0) @binding(0) var hdr: texture_2d<f32>;
@group(0) @binding(2) var indicator:texture_2d<f32>;
@group(0) @binding(1) var<uniform> settings: DisplaySettings;
@vertex fn vs_display(@builtin(vertex_index) vertex:u32)->@builtin(position) vec4<f32> {
    let p=array<vec2<f32>,3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0));
    return vec4(p[vertex],0.0,1.0);
}
fn aces(x:vec3<f32>)->vec3<f32> { return clamp((x*(2.51*x+0.03))/(x*(2.43*x+0.59)+0.14),vec3(0.0),vec3(1.0)); }
fn encode(x:vec3<f32>)->vec3<f32> { return select(1.055*pow(x,vec3(1.0/2.4))-0.055,x*12.92,x<=vec3(0.0031308)); }
@fragment fn fs_display(@builtin(position) p:vec4<f32>)->@location(0) vec4<f32> {
    let x=max(textureLoad(hdr,vec2<i32>(p.xy),0).rgb*settings.exposure,vec3(0.0));
    var mapped=aces(x);
    switch settings.mode {
    case 1u: { mapped=x/(x+vec3(1.0)); }
    case 2u: { mapped=clamp(vec3(1.0)-exp(-pow(aces(x),vec3(1.2))*1.5),vec3(0.0),vec3(1.0)); }
    case 3u: { mapped=clamp(x,vec3(0.0),vec3(1.0)); }
    default: {}
    }
    if settings._pad.x!=0u && textureLoad(indicator,vec2<i32>(p.xy),0).r>0.5 { mapped=mix(mapped,vec3(1.0,0.55,0.0),0.4); }
    return vec4(encode(mapped),1.0);
}
