#version 450
layout(location = 0) in vec3 position;
layout(set = 0, binding = 0, std140) uniform Camera { mat4 projection; } camera;
void main() { gl_Position = camera.projection * vec4(position, 1.0); }
