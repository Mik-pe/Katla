#version 450
layout(location=0) in uvec4 joints8;
layout(location=1) in uvec4 joints16;
layout(location=2) in vec4 weights16;
void main() {
    bool valid=all(equal(joints8,uvec4(0,1,127,255))) && all(equal(joints16,uvec4(0,256,32768,65535)));
    valid=valid && all(lessThan(abs(weights16-vec4(0.0,16384.0/65535.0,32768.0/65535.0,1.0)),vec4(0.00001)));
    vec2 positions[3]=vec2[3](vec2(-1,-1),vec2(3,-1),vec2(-1,3));
    gl_Position=valid?vec4(positions[gl_VertexIndex],0.5,1):vec4(0,0,0.5,1);
}
