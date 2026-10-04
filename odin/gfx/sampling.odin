//! Portable immutable sampler policy is independent of images and pipeline identity.
package gfx
import "core:math"

/// Validates portable policies and canonicalizes disabled mip sampling to level zero.
sampler_desc_normalize :: proc(desc:Sampler_Desc)->(Sampler_Desc,bool) {
    filters:=bit_set[Filter]{.Nearest,.Linear};mips:=bit_set[Mip_Filter]{.Nearest,.Linear,.None}
    addresses:=bit_set[Address_Mode]{.Repeat,.Mirror_Repeat,.Clamp_Edge,.Clamp_Border}
    compares:=bit_set[Compare_Op]{.Never,.Less,.Equal,.Less_Equal,.Greater,.Not_Equal,.Greater_Equal,.Always}
    if desc.min_filter not_in filters || desc.mag_filter not_in filters || desc.mip_filter not_in mips || desc.address_u not_in addresses || desc.address_v not_in addresses || desc.address_w not_in addresses || desc.compare not_in compares { return {},false }
    if math.is_nan(desc.min_lod) || math.is_inf(desc.min_lod) || math.is_nan(desc.max_lod) || math.is_inf(desc.max_lod) || desc.min_lod<0 || desc.max_lod<desc.min_lod || desc.max_anisotropy<1 || desc.max_anisotropy>16 { return {},false }
    if desc.max_anisotropy>1 && (desc.min_filter!=.Linear || desc.mag_filter!=.Linear) { return {},false }
    result:=desc;if result.mip_filter==.None { result.min_lod=0;result.max_lod=0 }
    return result,true
}
