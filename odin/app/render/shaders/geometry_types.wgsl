struct Object { model:mat4x4f,normal_model:mat4x4f,color:vec4f,factors:vec4f }
struct Vertex { position:vec4f,normal:vec4f,uv:vec4f }

fn coverage_uv(vertex:Vertex)->vec2f { return vertex.uv.xy; }
fn coverage_vertex_alpha(vertex:Vertex)->f32 { return 1.0; }
