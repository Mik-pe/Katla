#version 450

// layout(binding=0) uniform sampler2D albedo_sampler;
// layout(binding=1) uniform sampler2D normal_sampler;
// layout(binding=2) uniform sampler2D roughness_sampler;
// layout(binding=3) uniform sampler2D emissive_sampler;

const vec3 light_color = vec3(1.0, 1.0, 1.0);
const float spec_strength = 0.5;

// layout(set = 0, binding = 0) uniform FragData {
//     uint u_texVis;
//     vec3 u_camPos;
//     vec3 u_lightpos;
// } modeldatas;

layout(location=0) in vec3 vs_pos;
layout(location=1) in vec2 tex_coords;
// layout(location=2) in mat3 vs_TBN;

layout(location=0) out vec4 out_col;

void main()
{
    out_col = vec4(vs_pos, 1.0);

    // vec3 ambient_color = 0.1 * light_color;

    // vec3 light_dir   = normalize(modeldatas.u_lightpos - vs_pos);
    // vec3 view_dir    = normalize(modeldatas.u_camPos - vs_pos);
    // vec3 halfway_dir = normalize(light_dir + view_dir);

    // vec3 normalVector = vs_TBN * normalize(
    //     (texture(normal_sampler, tex_coords) * 2.0 - 1.0).rgb
    // );

    // float diff = max(dot(normalVector, light_dir), 0.0);
    // vec3 diffuse_color = diff * light_color;


    // vec3 reflect_dir = reflect(-light_dir, normalVector);
    // float spec = pow(max(dot(view_dir, reflect_dir), 0.0), 4);
    // vec3 specular = spec_strength * spec * light_color;
    
    // vec3 light_color_factor = specular + diffuse_color + ambient_color;
    // if(modeldatas.u_texVis == 1){
    //     vec4 color = texture(albedo_sampler, tex_coords);
    //     out_col = color;
    // }else if(modeldatas.u_texVis == 2){
    //     out_col = vec4(normalVector, 1.0);
    // }else if(modeldatas.u_texVis == 3){
    //     vec4 color = texture(roughness_sampler, tex_coords);
    //     out_col = color;
    // }else if(modeldatas.u_texVis == 4){
    //     vec3 normal = vs_TBN[2];
    //     vec3 normal_centered = (normal * 0.5) + 0.5;
    //     out_col = vec4(normal_centered, 1.0);
    // }else if(modeldatas.u_texVis == 5){
    //     vec3 tangent = vs_TBN[0];
    //     vec3 tangent_centered = (tangent * 0.5) + 0.5;
    //     out_col = vec4(tangent_centered, 1.0);
    // }else {
    //     vec4 color = texture(albedo_sampler, tex_coords);
    //     out_col = vec4(light_color_factor, 1.0) * color;
    // }
}
