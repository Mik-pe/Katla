struct Settings { decode:u32, _pad:vec3<u32> }
@group(0) @binding(0) var source:texture_2d<f32>;
@vertex fn vs_transfer(@builtin(vertex_index) index:u32)->@builtin(position) vec4f {
    let positions=array<vec2f,3>(vec2f(-1.0,-1.0),vec2f(3.0,-1.0),vec2f(-1.0,3.0));
    return vec4f(positions[index],0.0,1.0);
}
fn decode(value:vec3f)->vec3f { return select(pow((value+0.055)/1.055,vec3f(2.4)),value/12.92,value<=vec3f(0.04045)); }
fn encode(value:vec3f)->vec3f { let x=max(value,vec3f(0.0)); return select(1.055*pow(x,vec3f(1.0/2.4))-0.055,x*12.92,x<=vec3f(0.0031308)); }
@fragment fn fs_encode(@builtin(position) p:vec4f)->@location(0) vec4f {
    let value=textureLoad(source,vec2i(p.xy),0); return vec4f(encode(value.rgb),value.a);
}
@fragment fn fs_decode(@builtin(position) p:vec4f)->@location(0) vec4f {
    let value=textureLoad(source,vec2i(p.xy),0); return vec4f(decode(value.rgb),value.a);
}
