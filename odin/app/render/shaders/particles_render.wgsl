#include "common.wgsl"
struct ParticleCamera {
    view_projection: mat4x4f,
    right: vec4f,
    up: vec4f,
    clip: vec4f,
}
@group(0) @binding(0) var<storage, read> particles: array<ParticleData>;
@group(0) @binding(2) var<storage, read> alive_list: array<u32>;
@group(1) @binding(0) var<uniform> camera: ParticleCamera;
struct ParticleVertex {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
    @location(1) color: vec4f,
}
@vertex fn vs_main(@builtin(vertex_index) vertex: u32)->ParticleVertex {
    var result: ParticleVertex;
    result.position=vec4f(0.0,0.0,2.0,1.0);
    let index=vertex/6u;
    if index>=arrayLength(&alive_list) { return result; }
    let particle_index=alive_list[index];
    if particle_index>=arrayLength(&particles) { return result; }
    let particle=particles[particle_index];
    let corners=array<vec2f,6>(vec2f(-1.0,1.0),vec2f(1.0,1.0),vec2f(-1.0,-1.0),vec2f(-1.0,-1.0),vec2f(1.0,1.0),vec2f(1.0,-1.0));
    let corner=corners[vertex%6u];
    let offset=(corner.x*camera.right.xyz+corner.y*camera.up.xyz)*particle.scale*0.5;
    result.position=camera.view_projection*vec4f(particle.position+offset,1.0);
    result.position.y*=camera.clip.x;
    result.uv=corner;
    result.color=particle.color;
    return result;
}
@fragment fn fs_main(vertex:ParticleVertex)->@location(0) vec4f {
    let distance=length(vertex.uv);
    if distance>1.0 { discard; }
    let falloff=1.0-smoothstep(0.3,1.0,distance);
    return vec4f(vertex.color.rgb,vertex.color.a*falloff);
}
