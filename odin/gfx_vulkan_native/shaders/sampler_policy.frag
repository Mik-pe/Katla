#version 450
layout(set=0,binding=0) uniform Parameters { vec4 uv_lod; } parameters;
layout(set=2,binding=1) uniform texture2DArray pixels;
layout(set=2,binding=2) uniform sampler policy;
layout(location=0) out vec4 color;
void main() {
    color=textureLod(sampler2DArray(pixels,policy),vec3(parameters.uv_lod.xy,0.0),parameters.uv_lod.z);
}
