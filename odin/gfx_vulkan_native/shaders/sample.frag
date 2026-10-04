#version 450
layout(set=2,binding=1) uniform texture2DArray pixels;
layout(set=2,binding=2) uniform sampler point_sampler;
layout(location=0) out vec4 color;
void main() {
    vec2 uv=(floor(gl_FragCoord.xy/4.0)+vec2(0.5))/4.0;
    color=texture(sampler2DArray(pixels,point_sampler),vec3(uv,0.0));
}
