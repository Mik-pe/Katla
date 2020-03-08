#version 450
layout(binding=0) uniform sampler2D albedo_sampler;
layout(binding=1) uniform sampler2D normal_sampler;
layout(binding=2) uniform sampler2D roughness_sampler;
layout(binding=3) uniform sampler2D emissive_sampler;

const vec3 light_color = vec3(1.0, 1.0, 1.0);
const float spec_strength = 0.5;

uniform uint u_texVis;
uniform vec3 u_camPos;

in vec2 tex_coords;
in vec3 vs_pos;
in vec3 light_pos;
in vec3 vs_normal;
in vec3 vs_tangent;
in vec3 vs_bitangent;

out vec4 out_col;

void main()
{
    vec3 ambient_color = 0.1 * light_color;

    vec3 light_dir   = normalize(light_pos - vs_pos);
    vec3 view_dir    = normalize(u_camPos - vs_pos);
    vec3 halfway_dir = normalize(light_dir + view_dir);

    mat3 TBN = mat3(vs_tangent, vs_bitangent, vs_normal);

    vec3 normalVector = TBN * normalize(
        (texture(normal_sampler, tex_coords) * 2.0 - 1.0).rgb
    );

    float diff = max(dot(normalVector, light_dir), 0.0);
    vec3 diffuse_color = diff * light_color;


    vec3 reflect_dir = reflect(-light_dir, normalVector);
    float spec = pow(max(dot(view_dir, reflect_dir), 0.0), 32);
    vec3 specular = spec_strength * spec * light_color;
    
    vec3 light_color_factor = specular + diffuse_color + ambient_color;
    if(u_texVis == 1){
        vec4 color = texture(albedo_sampler, tex_coords);
        out_col = color;
    }else if(u_texVis == 2){
        out_col = vec4(normalVector, 1.0);
    }else if(u_texVis == 3){
        vec4 color = texture(roughness_sampler, tex_coords);
        out_col = color;
    }else if(u_texVis == 4){
        vec3 normal_centered = (vs_normal * 0.5) + 0.5;
        out_col = vec4(normal_centered, 1.0);
    }else if(u_texVis == 5){
        vec3 tangent_centered = (vs_tangent.xyz * 0.5) + 0.5;
        out_col = vec4(tangent_centered, 1.0);
    }else {
        vec4 color = texture(albedo_sampler, tex_coords);
        out_col = vec4(light_color_factor, 1.0) * color;
    }
}
