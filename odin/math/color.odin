// RGBA color values and explicit sRGB/linear conversion. Alpha is not gamma-corrected.
package katla_math

import m "core:math"

Color :: struct { r,g,b,a: f32 }
HSV :: struct { h,s,v: f32 }
COLOR_BLACK :: Color{0,0,0,1}
COLOR_WHITE :: Color{1,1,1,1}
COLOR_RED :: Color{1,0,0,1}
COLOR_GREEN :: Color{0,1,0,1}
COLOR_BLUE :: Color{0,0,1,1}
COLOR_YELLOW :: Color{1,1,0,1}
COLOR_CYAN :: Color{0,1,1,1}
COLOR_MAGENTA :: Color{1,0,1,1}
COLOR_TRANSPARENT :: Color{0,0,0,0}

/// RGB constructor with opaque alpha.
color_rgb :: #force_inline proc(r,g,b: f32) -> Color { return {r,g,b,1} }
/// Normalize byte components; alpha defaults to opaque.
color_from_u8 :: #force_inline proc(r,g,b: u8,a: u8 = 255) -> Color { return {f32(r)/255,f32(g)/255,f32(b)/255,f32(a)/255} }
/// Decode 0xRRGGBB.
color_from_rgb_hex :: #force_inline proc(hex: u32) -> Color { return color_from_u8(u8((hex>>16)&255),u8((hex>>8)&255),u8(hex&255)) }
/// Decode 0xRRGGBBAA.
color_from_rgba_hex :: #force_inline proc(hex: u32) -> Color { return color_from_u8(u8((hex>>24)&255),u8((hex>>16)&255),u8((hex>>8)&255),u8(hex&255)) }
/// RGBA scalar array for uploads and element-wise arithmetic.
color_to_array :: #force_inline proc(c: Color) -> Vec4 { return {c.r,c.g,c.b,c.a} }
/// RGBA scalar array conversion.
color_from_array :: #force_inline proc(v: Vec4) -> Color { return {v[0],v[1],v[2],v[3]} }
/// Component-wise RGBA addition, including alpha.
color_add :: #force_inline proc(a,b: Color) -> Color { return color_from_array(color_to_array(a)+color_to_array(b)) }
/// Component-wise RGBA subtraction, including alpha.
color_sub :: #force_inline proc(a,b: Color) -> Color { return color_from_array(color_to_array(a)-color_to_array(b)) }
/// Component-wise RGBA product, including alpha.
color_modulate :: #force_inline proc(a,b: Color) -> Color { return color_from_array(color_to_array(a)*color_to_array(b)) }
/// Scale all RGBA components, unlike color_brightness which preserves alpha.
color_scale :: #force_inline proc(c: Color,factor: f32) -> Color { return color_from_array(color_to_array(c)*factor) }
/// Saturate [0,1], round nearest, NaN to zero as Rust float-to-byte casting does.
color_to_bytes :: proc(c: Color) -> [4]u8 {
    result: [4]u8
    for v,i in color_to_array(c) { if !m.is_nan(v) { result[i] = u8(m.round(clamp(v,0,1)*255)) } }
    return result
}
/// Unclamped RGBA interpolation.
color_lerp :: #force_inline proc(a,b: Color,t: f32) -> Color { return color_from_array(lerp(color_to_array(a),color_to_array(b),t)) }
/// RGB brightness adjustment, preserving alpha.
color_brightness :: #force_inline proc(c: Color,factor: f32) -> Color { return {c.r*factor,c.g*factor,c.b*factor,c.a} }
/// Replace alpha only.
color_with_alpha :: #force_inline proc(c: Color,alpha: f32) -> Color { return {c.r,c.g,c.b,alpha} }
/// Luma-weighted saturation; factor zero is grayscale.
color_saturate :: #force_inline proc(c: Color,factor: f32) -> Color {
    gray := c.r*0.299+c.g*0.587+c.b*0.114
    return {gray+(c.r-gray)*factor,gray+(c.g-gray)*factor,gray+(c.b-gray)*factor,c.a}
}
/// Hue in degrees [0,360) for valid RGB inputs.
color_to_hsv :: proc(c: Color) -> HSV {
    hi,lo := max(c.r,c.g,c.b),min(c.r,c.g,c.b)
    delta := hi-lo
    h,s: f32
    if delta != 0 {
        if hi == c.r { h = 60*m.mod((c.g-c.b)/delta,6) }
        else if hi == c.g { h = 60*((c.b-c.r)/delta+2) }
        else { h = 60*((c.r-c.g)/delta+4) }
    }
    if h < 0 { h += 360 }
    if hi != 0 { s = delta/hi }
    return {h,s,hi}
}
/// HSV conversion preserves Rust's unwrapped hue behavior; output alpha is opaque.
color_from_hsv :: proc(hsv: HSV) -> Color {
    c := hsv.v*hsv.s
    x := c*(1-abs(m.mod(hsv.h/60,2)-1))
    offset := hsv.v-c
    v: Vec3
    if hsv.h < 60 { v = {c,x,0} } else if hsv.h < 120 { v = {x,c,0} } else if hsv.h < 180 { v = {0,c,x} } else if hsv.h < 240 { v = {0,x,c} } else if hsv.h < 300 { v = {x,0,c} } else { v = {c,0,x} }
    v += offset
    return {v[0],v[1],v[2],1}
}
@(private)
srgb_to_linear :: #force_inline proc(c: f32) -> f32 { if c <= 0.04045 { return c/12.92 }; return m.pow((c+0.055)/1.055,2.4) }
@(private)
linear_to_srgb :: #force_inline proc(c: f32) -> f32 { if c <= 0.0031308 { return c*12.92 }; return 1.055*m.pow(c,1/f32(2.4))-0.055 }
/// Piecewise sRGB to linear RGB; alpha unchanged.
color_to_linear :: #force_inline proc(c: Color) -> Color { return {srgb_to_linear(c.r),srgb_to_linear(c.g),srgb_to_linear(c.b),c.a} }
/// Piecewise linear RGB to sRGB; alpha unchanged.
color_to_srgb :: #force_inline proc(c: Color) -> Color { return {linear_to_srgb(c.r),linear_to_srgb(c.g),linear_to_srgb(c.b),c.a} }
/// Clamp each component to [0,1].
color_clamped :: #force_inline proc(c: Color) -> Color { return {clamp(c.r,0,1),clamp(c.g,0,1),clamp(c.b,0,1),clamp(c.a,0,1)} }
/// Reject out-of-range and NaN components.
color_is_valid :: #force_inline proc(c: Color) -> bool { for v in color_to_array(c) { if !(v>=0 && v<=1) { return false } }; return true }
