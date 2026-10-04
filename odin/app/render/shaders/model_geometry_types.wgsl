struct Object { model:mat4x4f,normal_model:mat4x4f,color:vec4f,factors:vec4f,emissive:vec4f,specular:vec4f,flags:vec4u }
struct Vertex { position:vec4f,normal:vec4f,tangent:vec4f,color:vec4f,uvs:array<vec4f,5> }

fn coverage_uv(vertex:Vertex)->vec2f { return vertex.uvs[0].xy; }
fn coverage_vertex_alpha(vertex:Vertex)->f32 { return vertex.color.a; }
@group(1) @binding(0) var coverage_texture:texture_2d<f32>;
@group(1) @binding(5) var coverage_sampler:sampler;
fn model_coverage(input:CoverageOutput) {
    let surface=objects[input.object];
    let alpha=textureSample(coverage_texture,coverage_sampler,input.uv).a*input.alpha;
    if surface.flags.y==1u && alpha<surface.emissive.w { discard; }
    if surface.flags.y==2u && alpha<=0.0 { discard; }
}
@fragment fn fs_depth_model(input:CoverageOutput) { model_coverage(input); }
@fragment fn fs_indicator_model(input:CoverageOutput)->@location(0) f32 { model_coverage(input);return 1.0; }
@fragment fn fs_outline_model(input:CoverageOutput)->@location(0) vec4f { model_coverage(input);return outline.color; }
