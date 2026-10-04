struct LightingFrame { view:mat4x4f,inverse_view_projection:mat4x4f,viewport:vec4f,settings:vec4f }
struct PointLight { position:vec3f,range:f32,color:vec3f,intensity:f32 }
@group(3) @binding(0) var<uniform> lighting:LightingFrame;
@group(3) @binding(1) var<storage,read> lights:array<PointLight,256>;
@group(3) @binding(2) var<storage,read_write> indices:array<u32>;
@group(3) @binding(3) var<storage,read_write> counts:array<u32>;
@compute @workgroup_size(1,1,1) fn cs_lights(@builtin(global_invocation_id) tile:vec3u) {
    let tiles=vec2u(lighting.viewport.zw); if tile.x>=tiles.x || tile.y>=tiles.y { return; }
    let index=tile.y*tiles.x+tile.x;
    let low=vec2f(tile.xy*16u); let high=low+16.0;
    var count=0u;
    // A tile frustum is tested in world space so backend clip conventions do not affect culling.
    let ndc_low=low/lighting.viewport.xy*2.0-1.0;
    let ndc_high=high/lighting.viewport.xy*2.0-1.0;
    for(var i=0u;i<u32(lighting.settings.x);i++) {
        let light=lights[i];
        let view_center=(lighting.view*vec4f(light.position,1.0)).xyz;
        if view_center.z>light.range { continue; }
        var visible=true;
        let camera_to_world=transpose(mat3x3f(lighting.view[0].xyz,lighting.view[1].xyz,lighting.view[2].xyz));
        // Four side planes through the camera, oriented toward the tile midpoint.
        let corners=array<vec2f,4>(vec2f(ndc_low.x,ndc_low.y),vec2f(ndc_high.x,ndc_low.y),vec2f(ndc_high.x,ndc_high.y),vec2f(ndc_low.x,ndc_high.y));
        var rays:array<vec3f,4>;
        let camera=-(camera_to_world*lighting.view[3].xyz);
        for(var c=0u;c<4u;c++) { let world=lighting.inverse_view_projection*vec4f(corners[c],0.5,1.0); rays[c]=normalize(world.xyz/world.w-camera); }
        let center_ray=normalize(rays[0]+rays[1]+rays[2]+rays[3]);
        for(var c=0u;c<4u;c++) { var plane=normalize(cross(rays[c],rays[(c+1u)%4u])); plane*=select(-1.0,1.0,dot(plane,center_ray)>=0.0); if dot(plane,light.position-camera)<-light.range { visible=false; } }
        if visible && count<128u { indices[index*128u+count]=i; count++; }
    }
    counts[index]=count;
}
