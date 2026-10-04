struct Frame { logical_size:vec2f, texture_index:u32, clip_y:f32, decode_sample:u32, _pad:vec3<u32> }
struct Vertex { position:vec2f, uv:vec2f, color:vec4f }
@group(0) @binding(0) var<uniform> frame:Frame;
@group(0) @binding(1) var<storage,read> vertices:array<Vertex>;
@group(1) @binding(0) var images:binding_array<texture_2d<f32>,64>;
@group(1) @binding(1) var image_sampler:sampler;
struct Output { @builtin(position) clip:vec4f, @location(0) uv:vec2f, @location(1) color:vec4f }
@vertex fn vs_ui(@builtin(vertex_index) index:u32)->Output {
    let vertex=vertices[index];
    var result:Output;
    result.clip=vec4f(vertex.position.x/frame.logical_size.x*2.0-1.0,(1.0-vertex.position.y/frame.logical_size.y*2.0)*frame.clip_y,0.0,1.0);
    result.uv=vertex.uv;result.color=vertex.color;return result;
}
fn decode(value:vec3f)->vec3f { return select(pow((value+0.055)/1.055,vec3f(2.4)),value/12.92,value<=vec3f(0.04045)); }
@fragment fn fs_ui(input:Output)->@location(0) vec4f {
    let sampled=textureSample(images[frame.texture_index],image_sampler,input.uv);
    let rgb=select(sampled.rgb,decode(sampled.rgb),frame.decode_sample!=0u);
    return vec4f(rgb*decode(input.color.rgb),sampled.a*input.color.a);
}
