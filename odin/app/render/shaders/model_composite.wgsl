@group(0) @binding(0) var source: texture_2d<f32>;
@vertex fn vs_composite(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let points = array<vec2<f32>,3>(vec2<f32>(-1.0,-1.0),vec2<f32>(3.0,-1.0),vec2<f32>(-1.0,3.0));
    return vec4<f32>(points[index],0.0,1.0);
}
fn linear(value:f32)->f32 { return select(pow((value+0.055)/1.055,2.4),value/12.92,value<=0.04045); }
fn srgb(value:f32)->f32 { return select(1.055*pow(max(value,0.0),1.0/2.4)-0.055,value*12.92,value<=0.0031308); }
@fragment fn fs_decode(@builtin(position) position:vec4<f32>)->@location(0) vec4<f32> {
    let color=textureLoad(source,vec2<i32>(position.xy),0);
    return vec4<f32>(linear(color.r),linear(color.g),linear(color.b),color.a);
}
@fragment fn fs_encode(@builtin(position) position:vec4<f32>)->@location(0) vec4<f32> {
    let color=textureLoad(source,vec2<i32>(position.xy),0);
    return vec4<f32>(srgb(color.r),srgb(color.g),srgb(color.b),color.a);
}
