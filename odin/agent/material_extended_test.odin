#+test
package agent

import editor "../editor"
import "core:encoding/json"
import "core:testing"
import "core:mem"

@(test)
test_material_complete_surface_factors_keep_presence_and_units :: proc(t:^testing.T) {
    text:=`{"action":"set","entity_ids":["18446744073709551615"],"preset":"ceramic","emissive_factor":[0,4,20],"normal_scale":-2,"occlusion_strength":0,"alpha_mode":"blend","alpha_cutoff":1.5,"double_sided":false,"metallic":null}`
    decoded,error:=decode_material(transmute([]byte)text); defer decoded_material_destroy(&decoded)
    testing.expect_value(t,error,Call_Error.None); if error!=.None { return }
    op:=decoded.operation.(Material_Set)
    testing.expect(t,u64(op.entities[0])==max(u64) && op.has_preset && op.preset==.Ceramic)
    testing.expect(t,op.fields=={.Emissive_Factor,.Normal_Scale,.Occlusion_Strength,.Alpha_Mode,.Alpha_Cutoff,.Double_Sided})
    testing.expect(t,op.values.emissive_factor==[3]f32{0,4,20} && op.values.normal_scale==-2 && op.values.occlusion_strength==0)
    testing.expect(t,op.values.alpha_mode==.Blend && op.values.alpha_cutoff==1.5 && !op.values.double_sided)
    for preset in Material_Preset {
        values:=material_preset_values(preset)
        testing.expect(t,material_values_valid(values) && values.normal_scale==1 && values.occlusion_strength==1 && values.alpha_mode==.Opaque && values.alpha_cutoff==0.5 && !values.double_sided)
    }
}
@(test)
test_material_sampling_exact_integer_bounds_signed_uv_and_nullable_patch :: proc(t:^testing.T) {
    text:=`{"action":"set_sampling","entity_ids":["9007199254740993","18446744073709551615"],"role":"normal","patch":{"tex_coord":1,"offset":[-0.25,0.5],"rotation":3.1415927,"scale":[-2,0],"minification":"linear_mipmap_linear","magnification":"linear","wrap_u":"mirrored_repeat","wrap_v":"clamp_to_edge","anisotropy":16}}`
    decoded,error:=decode_material(transmute([]byte)text); defer decoded_material_destroy(&decoded)
    testing.expect_value(t,error,Call_Error.None); if error!=.None { return }
    op:=decoded.operation.(Material_Set_Sampling)
    testing.expect(t,u64(op.entities[0])==9007199254740993 && u64(op.entities[1])==max(u64) && op.role==.Normal)
    testing.expect(t,op.patch.tex_coord==1 && op.patch.offset==[2]f32{-0.25,0.5} && op.patch.scale==[2]f32{-2,0})
    testing.expect(t,op.patch.rotation>3.14 && op.patch.rotation<3.15 && op.patch.minification==.Linear_Mipmap_Linear && op.patch.magnification==.Linear && op.patch.wrap_u==.Mirrored_Repeat && op.patch.wrap_v==.Clamp_To_Edge && op.patch.anisotropy==16)
    nullable,nullable_error:=decode_material(transmute([]byte)string(`{"action":"set_sampling","entity_ids":["0"],"role":"emission","patch":{"tex_coord":null,"rotation":0,"scale":null}}`)); defer decoded_material_destroy(&nullable)
    testing.expect_value(t,nullable_error,Call_Error.None)
    if nullable_error==.None { testing.expect(t,nullable.operation.(Material_Set_Sampling).patch.fields=={.Rotation}) }
}
@(test)
test_material_image_sources_capture_owned_path_and_max_u32_index :: proc(t:^testing.T) {
    allocator:=context.allocator
    data:=make([]byte,len(`{"action":"set_texture","entity_ids":["0"],"role":"albedo","source":{"kind":"gltf_image","asset":{"Resource":"models/Ångström.glb"},"image_index":4294967295}}`))
    copy(data,transmute([]byte)string(`{"action":"set_texture","entity_ids":["0"],"role":"albedo","source":{"kind":"gltf_image","asset":{"Resource":"models/Ångström.glb"},"image_index":4294967295}}`))
    decoded,error:=decode_material(data,allocator); delete(data)
    testing.expect_value(t,error,Call_Error.None)
    if error==.None {
        op:=decoded.operation.(Material_Set_Texture)
        testing.expect(t,op.role==.Albedo && op.source.kind==.Gltf_Image && op.source.root==.Resource && op.source.path=="models/Ångström.glb" && op.source.image_index==max(u32))
        context.allocator=mem.nil_allocator(); decoded_material_destroy(&decoded); context.allocator=allocator
    }
    for text in ([]string{`{"kind":"inherit"}`,`{"kind":"neutral"}`,`{"kind":"file","asset":{"Scene":"image.png"}}`,`{"kind":"file","asset":{"File":"/private/tmp/image.png"}}`}) {
        source,source_error:=material_texture_source_decode(transmute([]byte)text,allocator)
        testing.expect_value(t,source_error,Call_Error.None); material_texture_source_destroy(&source,allocator)
    }
}
@(test)
test_material_extended_rejections_do_not_admit_mailbox_calls :: proc(t:^testing.T) {
    h:editor.Agent_Harness; editor.agent_harness_init(&h); defer editor.agent_harness_destroy(&h)
    for text in ([]string{
        `{"action":"set","entity_ids":["0"],"emissive_factor":[0,-1,1]}`,
        `{"action":"set","entity_ids":["0"],"normal_scale":1e100}`,
        `{"action":"set","entity_ids":["0"],"occlusion_strength":1.001}`,
        `{"action":"set","entity_ids":["0"],"alpha_mode":"transparent"}`,
        `{"action":"set","entity_ids":["0"],"alpha_cutoff":-1}`,
        `{"action":"set","entity_ids":["0"],"double_sided":1}`,
        `{"action":"set_sampling","entity_ids":["0"],"role":"normal","patch":{}}`,
        `{"action":"set_sampling","entity_ids":["0"],"role":"color","patch":{"rotation":0}}`,
        `{"action":"set_sampling","entity_ids":["0","00"],"role":"normal","patch":{"rotation":0}}`,
        `{"action":"set_sampling","entity_ids":["0"],"role":"normal","patch":{"tex_coord":18446744073709551616}}`,
        `{"action":"set_sampling","entity_ids":["0"],"role":"normal","patch":{"tex_coord":1.0}}`,
        `{"action":"set_sampling","entity_ids":["0"],"role":"normal","patch":{"anisotropy":0}}`,
        `{"action":"set_sampling","entity_ids":["0"],"role":"normal","patch":{"anisotropy":17}}`,
        `{"action":"set_sampling","entity_ids":["0"],"role":"normal","patch":{"offset":[1,2,3]}}`,
        `{"action":"set_sampling","entity_ids":["0"],"role":"normal","patch":{"rotatoin":0}}`,
        `{"action":"set_texture","entity_ids":["0"],"role":"normal","source":{"kind":"gltf_image","asset":{"Resource":"a.glb"},"image_index":4294967296}}`,
        `{"action":"set_texture","entity_ids":["0"],"role":"normal","source":{"kind":"gltf_image","asset":{"Resource":"a.glb"},"image_index":18446744073709551616}}`,
        `{"action":"set_texture","entity_ids":["0"],"role":"normal","source":{"kind":"gltf_image","asset":{"Resource":"a.glb"},"image_index":0e0}}`,
        `{"action":"set_texture","entity_ids":["0"],"role":"normal","source":{"kind":"gltf_image","asset":{"Resource":"a.glb"}}}`,
        `{"action":"set_texture","entity_ids":["0"],"role":"normal","source":{"kind":"neutral","asset":{"Resource":"a.png"}}}`,
        `{"action":"set_texture","entity_ids":["0"],"role":"normal","source":{"kind":"file","asset":{"Resource":"a.png","Scene":"b.png"}}}`,
    }) {
        ticket,error:=submit_call(&h,{"invalid","material",transmute([]byte)text})
        testing.expect(t,ticket==0 && error==.Invalid_Arguments)
    }
    testing.expect(t,len(h.requests)==0 && h.outstanding==0 && len(h.session.actions)==0)
}
@(test)
test_material_registry_discovery_matches_all_typed_actions :: proc(t:^testing.T) {
    value,error:=json.parse(TOOLS_JSON); testing.expect(t,error==nil); if error!=nil { return }; defer json.destroy_value(value)
    rows:=value.(json.Array); material_branches,asset_branches:=0,0
    for row in rows {
        object:=row.(json.Object); name:=object["name"].(string)
        if name!="material" && name!="material_asset" { continue }
        schema:=object["inputSchema"].(json.Object); branches:=schema["oneOf"].(json.Array)
        for branch in branches { testing.expect(t,branch.(json.Object)["additionalProperties"].(bool)==false) }
        if name=="material" { material_branches=len(branches) } else { asset_branches=len(branches) }
    }
    testing.expect(t,len(rows)==28 && material_branches==5 && asset_branches==6)
    decoded,decode_error:=decode_call({"asset","material_asset",transmute([]byte)string(`{"action":"apply","path":"surfaces/a.katmat","entity_ids":["18446744073709551615"]}`)})
    defer decoded_call_destroy(&decoded)
    testing.expect(t,decode_error==.None && decoded.operation.kind==.Application && decoded.operation.tool_name=="material_asset")
}
