//! Material receipts describe the canonical authored controls and transport units.
package app

@(private="package")
Material_Capabilities :: struct {
    maximum_batch_size:int,batch_atomic:bool,
    sampling_editable,textures_editable,alpha_changes_render_mode:bool,
    alpha_mode_editable,alpha_cutoff_editable,double_sided_editable:bool,
    emission_editable,normal_scale_editable,occlusion_strength_editable:bool,
    rotation_unit,emission_color_space,base_color:string,
    alpha_modes:[3]string,texture_sources:[4]string,sampling_roles:[5]string,
    neutral_images:struct {albedo,normal,metallic_roughness,occlusion,emission:string},
    blend_depth_policy,blend_shadow_policy,blend_picking_policy:string,
    image_reference_policy,anisotropy_limit_policy,sampling_persistence,presets:string,
}
@(private="package")
material_capabilities :: proc()->Material_Capabilities {
    return {maximum_batch_size=256,batch_atomic=true,sampling_editable=true,textures_editable=true,
        alpha_changes_render_mode=false,alpha_mode_editable=true,alpha_cutoff_editable=true,double_sided_editable=true,
        emission_editable=true,normal_scale_editable=true,occlusion_strength_editable=true,rotation_unit="radians",emission_color_space="linear RGB; HDR values allowed; missing emissive texture samples white",
        base_color="sRGB RGB and linear alpha texture multiplier",alpha_modes={"opaque","mask","blend"},texture_sources={"inherit","neutral","file","gltf_image"},sampling_roles={"albedo","normal","metallic_roughness","occlusion","emission"},
        neutral_images={"white RGBA","exact linear (0.5,0.5,1,1)","white; factors remain unchanged","white; factors remain unchanged","white; multiplied by the current emissive factor"},
        blend_depth_policy="sorted back-to-front, depth test enabled, scene depth writes disabled",blend_shadow_policy="blended surfaces do not cast binary shadow-map shadows",blend_picking_policy="nonzero-alpha surfaces are pickable; picking has its own depth buffer",
        image_reference_policy="Receipts resolve runtime paths; scene capture/Save As identifies portable Resource/Scene roots where possible",anisotropy_limit_policy="requested value is clamped to the device maximum",sampling_persistence="scene drawable sampling; omission preserves imported settings",presets="isotropic metallic/roughness factors; no texture or directional brushing"}
}
