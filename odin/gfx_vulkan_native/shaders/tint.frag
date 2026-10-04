#version 450
layout(set = 2, binding = 3, std140) uniform Tint { vec4 rgba; } tint;
layout(location = 0) out vec4 color;
void main() { color = tint.rgba; }
