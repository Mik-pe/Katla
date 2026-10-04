struct Frame { view_projection:mat4x4f,camera_position:vec4f,light_direction:vec4f,light_color:vec4f,ambient:vec4f }
struct Grid { parameters:vec4f,color:vec4f }
@group(0) @binding(0) var<uniform> frame:Frame;
@group(5) @binding(0) var<uniform> grid:Grid;
@vertex fn vs_grid(@builtin(vertex_index) vertex:u32,@builtin(instance_index) instance:u32)->@builtin(position) vec4f {
    let corners=array<vec2f,6>(vec2f(-1.0,-1.0),vec2f(1.0,-1.0),vec2f(-1.0,1.0),vec2f(-1.0,1.0),vec2f(1.0,-1.0),vec2f(1.0,1.0));
    let corner=corners[vertex]; let line=instance/2u; let along_x=instance%2u==0u;
    var step=f32(line)-9.0; if step>=0.0 { step+=1.0; }
    if grid.parameters.z!=0.0 { step=f32(line)*10.0-10.0; }
    var world=vec3f(corner.x*10.0,grid.parameters.x+grid.parameters.y*0.5,step+corner.y*grid.parameters.y*0.5);
    if !along_x { world=vec3f(step+corner.y*grid.parameters.y*0.5,grid.parameters.x+grid.parameters.y*0.5,corner.x*10.0); }
    var clip=frame.view_projection*vec4f(world,1.0); clip.y*=frame.ambient.w; return clip;
}
@fragment fn fs_grid()->@location(0) vec4f { return grid.color; }
