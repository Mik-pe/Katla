//! Independent double-precision Cook-Torrance reference for production model pixels.
package main

import "core:math"

Vec3d :: [3]f64
BRDF_Case :: struct { roughness,metallic:f32,view,light:[3]f32 }
dot3 :: proc(a,b:Vec3d)->f64 { return a[0]*b[0]+a[1]*b[1]+a[2]*b[2] }
unit3 :: proc(v:Vec3d)->Vec3d { length:=math.sqrt(dot3(v,v)); return {v[0]/length,v[1]/length,v[2]/length} }
reference :: proc(sample:BRDF_Case)->[3]f64 {
    view:=unit3({f64(sample.view[0]),f64(sample.view[1]),f64(sample.view[2])});light:=unit3({f64(sample.light[0]),f64(sample.light[1]),f64(sample.light[2])})
    nv,nl:=max(view[2],0),max(light[2],0); if nv==0 || nl==0 { return {} }
    half:=unit3({view[0]+light[0],view[1]+light[1],view[2]+light[2]})
    nh,vh:=max(half[2],0),clamp(dot3(view,half),0,1)
    rough:=clamp(f64(sample.roughness),f64(f32(.04)),1);alpha_sq:=rough*rough*rough*rough
    denominator:=(1-nh*nh)+nh*nh*alpha_sq
    distribution:=alpha_sq/(math.PI*denominator*denominator)
    visibility:=.5/(nl*math.sqrt(nv*nv*(1-alpha_sq)+alpha_sq)+nv*math.sqrt(nl*nl*(1-alpha_sq)+alpha_sq))
    base:=[3]f64{f64(f32(.8)),f64(f32(.3)),f64(f32(.1))};light_color:=[3]f64{1,f64(f32(.8)),f64(f32(.6))}
    result:[3]f64
    for color,i in base {
        metal:=f64(sample.metallic);f0:=.04*(1-metal)+color*metal
        fresnel:=f0+(1-f0)*math.pow(1-vh,5)
        result[i]=((1-fresnel)*(1-metal)*color/math.PI+distribution*visibility*fresnel)*nl*light_color[i]
    }
    return result
}
parameter_cases :: proc()->[120]BRDF_Case {
    views:=[6]Vec3d{{0,0,1},{.6,0,.8},{.8,0,.6},{.995,0,.1},{0,0,-1},{0,0,1}}
    lights:=[6]Vec3d{{0,0,1},{-.6,0,.8},{0,.6,.8},{.3,-.4,.866},{0,0,1},{0,0,-1}}
    samples:[120]BRDF_Case;index:=0
    for rough in ([5]f32{.04,.1,.25,.5,1}) { for metal in ([4]f32{0,.3,.7,1}) { for view,i in views {
        v,l:=unit3(view),unit3(lights[i]);samples[index]={rough,metal,{f32(v[0]),f32(v[1]),f32(v[2])},{f32(l[0]),f32(l[1]),f32(l[2])}};index+=1
    } } }
    return samples
}
