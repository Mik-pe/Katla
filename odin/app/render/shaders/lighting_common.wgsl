struct LightingFrame { view:mat4x4f, inverse_view_projection:mat4x4f, viewport:vec4f, settings:vec4f }
struct PointLight { position:vec3f, range:f32, color:vec3f, intensity:f32 }
struct ShadowCascade { view_projection:mat4x4f, split_texel:vec4f }
struct ShadowFrame { cascades:array<ShadowCascade,4>, direction:vec4f, bias:vec4f }
@group(3) @binding(0) var<uniform> lighting:LightingFrame;
@group(3) @binding(1) var<storage,read> point_lights:array<PointLight,256>;
@group(3) @binding(2) var<storage,read> tile_indices:array<u32>;
@group(3) @binding(3) var<storage,read> tile_counts:array<u32>;
@group(4) @binding(0) var<storage,read> shadow:ShadowFrame;
@group(4) @binding(1) var shadow_atlas:texture_depth_2d;
@group(4) @binding(2) var shadow_sampler:sampler_comparison;
fn shadow_visibility_at(world:vec3f,normal:vec3f,cascade:u32)->f32 {
    let projected=shadow.cascades[cascade].view_projection*vec4f(world,1.0);
    let ndc=projected.xyz/projected.w;
    let local_uv=ndc.xy*0.5+0.5;
    if any(local_uv<vec2f(0.0)) || any(local_uv>vec2f(1.0)) || ndc.z<0.0 || ndc.z>1.0 { return 1.0; }
    let origin=vec2f(f32(cascade%2u),f32(cascade/2u))*0.5;
    let uv=origin+local_uv*0.5;
    let step=shadow.cascades[cascade].split_texel.y;
    let cosine=clamp(abs(dot(normal,normalize(-shadow.direction.xyz))),0.05,1.0);
    let slope=sqrt(max(1.0-cosine*cosine,0.0))/cosine;
    let bias=shadow.bias.x+shadow.cascades[cascade].split_texel.z*shadow.bias.y*min(slope,8.0);
    var result=0.0;
    for(var y=0u;y<4u;y++) { for(var x=0u;x<4u;x++) {
        let offset=(vec2f(f32(x),f32(y))-1.5)*step;
        let coordinate=clamp(uv+offset,origin+vec2f(step*0.5),origin+vec2f(0.5-step*0.5));
        result+=textureSampleCompareLevel(shadow_atlas,shadow_sampler,coordinate,ndc.z-bias);
    }}
    return result/16.0;
}
fn shadow_visibility(world:vec3f,normal:vec3f)->f32 {
    if lighting.settings.z==0.0 { return 1.0; }
    let distance=-(lighting.view*vec4f(world,1.0)).z;
    var selected=3u;
    for(var i=0u;i<4u;i++) { if distance<=shadow.cascades[i].split_texel.x { selected=i; break; } }
    let current=shadow_visibility_at(world,normal,selected);
    if selected==3u { return current; }
    let split=shadow.cascades[selected].split_texel.x;
    let zone=(shadow.cascades[selected+1u].split_texel.x-split)*0.05;
    if distance<split-zone { return current; }
    return mix(current,shadow_visibility_at(world,normal,selected+1u),clamp((distance-split+zone)/zone,0.0,1.0));
}
fn pbr_light(normal:vec3f,view:vec3f,light:vec3f,diffuse_color:vec3f,f0:vec3f,roughness:f32)->vec3f {
    let half_vector=normalize(view+light);
    let nv=max(dot(normal,view),0.0); let nl=max(dot(normal,light),0.0);
    let nh=max(dot(normal,half_vector),0.0); let vh=max(dot(view,half_vector),0.0);
    let a=roughness*roughness; let a2=a*a; let denominator=nh*nh*(a2-1.0)+1.0;
    let distribution=a2/max(3.14159265359*denominator*denominator,0.00001);
    let k=(roughness+1.0)*(roughness+1.0)/8.0;
    let geometry=nv/max(nv*(1.0-k)+k,0.00001)*nl/max(nl*(1.0-k)+k,0.00001);
    let fresnel=f0+(vec3f(1.0)-f0)*pow(1.0-vh,5.0);
    let specular=distribution*geometry*fresnel/max(4.0*nv*nl,0.00001);
    return ((vec3f(1.0)-fresnel)*diffuse_color/3.14159265359+specular)*nl;
}
fn point_illumination(pixel:vec2f,world:vec3f,normal:vec3f,view:vec3f,diffuse_color:vec3f,f0:vec3f,roughness:f32)->vec3f {
    let tile=vec2u(pixel)/16u;
    let index=tile.y*u32(lighting.viewport.z)+tile.x;
    let count=min(tile_counts[index],128u);
    var result=vec3f(0.0);
    for(var slot=0u;slot<count;slot++) {
        let light=point_lights[tile_indices[index*128u+slot]];
        let delta=light.position-world; let distance=length(delta);
        let attenuation=pow(max(1.0-distance/light.range,0.0),2.0);
        result+=pbr_light(normal,view,delta/max(distance,0.001),diffuse_color,f0,roughness)*light.color*light.intensity*attenuation;
    }
    return result;
}
