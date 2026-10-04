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
fn pbr_light(normal:vec3f,view:vec3f,light:vec3f,diffuse_color:vec3f,f0:vec3f,perceptual_roughness:f32)->vec3f {
    let nv=clamp(dot(normal,view),0.0,1.0); let nl=clamp(dot(normal,light),0.0,1.0);
    if nv<=0.0 || nl<=0.0 { return vec3f(0.0); }
    let half_vector=normalize(view+light);
    let nh=clamp(dot(normal,half_vector),0.0,1.0); let vh=clamp(dot(view,half_vector),0.0,1.0);
    let roughness=clamp(perceptual_roughness,0.04,1.0);
    let alpha=roughness*roughness; let alpha_sq=alpha*alpha;
    let nh_sq=nh*nh; let denominator=(1.0-nh_sq)+nh_sq*alpha_sq;
    let distribution=alpha_sq/(3.14159265359*denominator*denominator);
    let view_mask=nl*sqrt(nv*nv*(1.0-alpha_sq)+alpha_sq);
    let light_mask=nv*sqrt(nl*nl*(1.0-alpha_sq)+alpha_sq);
    let visibility=0.5/max(view_mask+light_mask,0.00000001);
    let fresnel=f0+(vec3f(1.0)-f0)*pow(1.0-vh,5.0);
    return ((vec3f(1.0)-fresnel)*diffuse_color/3.14159265359+distribution*visibility*fresnel)*nl;
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

fn normalized_or(vector:vec3f,fallback:vec3f)->vec3f {
    let magnitude_sq=dot(vector,vector);
    if magnitude_sq>0.000000000001 { return vector*inverseSqrt(magnitude_sq); }
    return fallback;
}
struct TangentFrame { normal:vec3f,tangent:vec3f,bitangent:vec3f }
fn transformed_tangent_frame(transform:mat3x3f,normal:vec3f,tangent:vec4f)->TangentFrame {
    let cofactors=mat3x3f(cross(transform[1],transform[2]),cross(transform[2],transform[0]),cross(transform[0],transform[1]));
    let orientation=select(1.0,-1.0,dot(transform[0],cofactors[0])<0.0);
    let n=normalized_or(cofactors*normal*orientation,normalized_or(transform*normal,vec3f(0.0,1.0,0.0)));
    let direction=transform*tangent.xyz;
    let axis=select(vec3f(1.0,0.0,0.0),vec3f(0.0,0.0,1.0),abs(n.x)>0.9);
    let t=normalized_or(direction-n*dot(n,direction),normalize(cross(axis,n)));
    return TangentFrame(n,t,cross(n,t)*tangent.w*orientation);
}
fn material_tangent_basis(position:vec3f,uv:vec2f,normal:vec3f,tangent:vec4f,regenerate:bool)->mat3x3f {
    let dx=dpdx(position);let dy=dpdy(position);let ux=dpdx(uv);let uy=dpdy(uv);
    let uv_determinant=ux.x*uy.y-ux.y*uy.x;
    let n=normalized_or(normal,vec3f(0.0,1.0,0.0));
    let axis=select(vec3f(1.0,0.0,0.0),vec3f(0.0,0.0,1.0),abs(n.x)>0.9);
    var t=normalized_or(tangent.xyz-n*dot(n,tangent.xyz),normalize(cross(axis,n)));
    var b=cross(n,t)*tangent.w;
    if regenerate && abs(uv_determinant)>0.000000000001 {
        let direction=(dx*uy.y-dy*ux.y)/uv_determinant;
        let orthogonal=direction-n*dot(n,direction);
        if dot(orthogonal,orthogonal)>0.000000000001 {
            t=normalize(orthogonal);
            let bitangent_direction=(dy*ux.x-dx*uy.x)/uv_determinant;
            b=cross(n,t)*select(-1.0,1.0,dot(cross(n,t),bitangent_direction)>=0.0);
        }
    }
    return mat3x3f(t,b,n);
}
fn surface_tangent_normal(sample:vec3f,scale:f32)->vec3f {
    let unpacked=sample*2.0-1.0;
    return normalized_or(vec3f(unpacked.xy*scale,unpacked.z),vec3f(0.0,0.0,1.0));
}
