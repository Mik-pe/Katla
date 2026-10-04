//! Feature binaries share canonical reflection while application raster policy stays explicit.
package render

import gfx "../../gfx"
import shader "../../gfx/shader"
import adapter "../../gfx/shader_adapter"
import "core:mem"
import "core:strings"

Feature_Descriptors :: struct { sky,grid:gfx.Graphics_Desc, cull:gfx.Compute_Desc, geometry:[2][5]gfx.Graphics_Desc,model_variants:[5][3]gfx.Graphics_Desc }
Feature_Shader :: struct { compiled:[5]shader.Compiled, graphics:[27]adapter.Graphics, compute:adapter.Compute, colors:[]gfx.Color_Target, allocator:mem.Allocator }
FEATURE_GEOMETRY :: #load("shaders/shadow_geometry.wgsl",string)
FEATURE_SKY :: #load("shaders/environment.wgsl",string)
FEATURE_GRID :: #load("shaders/grid.wgsl",string)
FEATURE_CULL :: #load("shaders/lighting_cull.wgsl",string)
feature_shader_descriptors :: proc(value:^Feature_Shader)->Feature_Descriptors {
    result:=Feature_Descriptors{sky=value.graphics[0].descriptor,grid=value.graphics[1].descriptor,cull=value.compute.descriptor}
    for kind in 0..<2 { for effect in 0..<5 { result.geometry[kind][effect]=value.graphics[2+kind*5+effect].descriptor } }
    for effect in 0..<5 { for extra in 0..<3 { result.model_variants[effect][extra]=value.graphics[12+effect*3+extra].descriptor } }
    return result
}
feature_shader_compile :: proc(compiler:^shader.Compiler,allocator:mem.Allocator=context.allocator)->(Feature_Shader,shader.Error) {
    result:=Feature_Shader{allocator=allocator,colors=make([]gfx.Color_Target,27,allocator)}
    success:=false; defer { if !success { feature_shader_destroy(&result) } }
    for source,i in ([3]string{FEATURE_SKY,FEATURE_GRID,FEATURE_CULL}) {
        selections:[]shader.Selection
        if i==0 { selections={{"vs_sky",.Vertex},{"fs_sky",.Fragment}} }
        else if i==1 { selections={{"vs_grid",.Vertex},{"fs_grid",.Fragment}} }
        else { selections={{"cs_lights",.Compute}} }
        error:shader.Error
        result.compiled[i],error=shader.compile(compiler,source,selections,allocator=allocator); if error!=.None { return {},error }
    }
    map_error:adapter.Error
    result.compute,map_error=adapter.compute(&result.compiled[2],"cs_lights",allocator); if map_error!=.None { return {},.Reflection }
    for i in 0..<2 {
        result.colors[i]={format=.RGBA16_Float,write_mask={.Red,.Green,.Blue,.Alpha}}
        result.graphics[i],map_error=adapter.graphics(&result.compiled[i],"vs_sky" if i==0 else "vs_grid","fs_sky" if i==0 else "fs_grid",{colors=result.colors[i:i+1],depth={enabled=true,test=i==1,write=false,compare=.Less_Equal,format=.D32_Float_S8_Uint},topology=.Triangle_List,cull=.None},allocator)
        if map_error!=.None { return {},.Reflection }
    }
    schemas:=[2]string{#load("shaders/geometry_types.wgsl",string),#load("shaders/model_geometry_types.wgsl",string)}
    for schema,kind in schemas {
        source:=strings.concatenate({schema,"\n",FEATURE_GEOMETRY},allocator); defer delete(source,allocator)
        error:shader.Error
        depth_entry:="fs_depth_model" if kind==1 else "fs_depth"
        outline_entry:="fs_outline_model" if kind==1 else "fs_outline"
        indicator_entry:="fs_indicator_model" if kind==1 else "fs_indicator"
        result.compiled[3+kind],error=shader.compile(compiler,source,{{"vs_shadow",.Vertex},{"vs_mark",.Vertex},{"vs_outline",.Vertex},{depth_entry,.Fragment},{outline_entry,.Fragment},{indicator_entry,.Fragment}},allocator=allocator)
        if error!=.None { return {},error }
        for effect in 0..<5 {
            index:=2+kind*5+effect
            mask:gfx.Color_Components; if effect>=3 { mask={.Red,.Green,.Blue,.Alpha} }
            result.colors[index]={format=.R8_Unorm if effect==4 else .RGBA16_Float,write_mask=mask}
            colors:=result.colors[index:index+1]; if effect==0 { colors=nil }
            stencil:gfx.Stencil_State
            face:=gfx.Stencil_Face{compare=.Always,fail=.Keep,depth_fail=.Keep,pass=.Replace}
            switch effect {
            case 1: stencil={true,face,face,1,1,1}
            case 2: face={compare=.Equal,fail=.Keep,depth_fail=.Replace,pass=.Keep}; stencil={true,face,face,2,1,2}
            case 3: face={compare=.Equal,fail=.Keep,depth_fail=.Keep,pass=.Keep}; stencil={true,face,face,0,3,0}
            case 4: face={compare=.Equal,fail=.Keep,depth_fail=.Keep,pass=.Keep}; stencil={true,face,face,2,2,0}
            }
            vertex:="vs_shadow" if effect==0 else ("vs_outline" if effect==3 else "vs_mark")
            fragment:=outline_entry if effect==3 else (indicator_entry if effect==4 else depth_entry)
            cull:=gfx.Cull_Mode.None; if effect==0 || effect==2 { cull=.Back }; if effect==3 { cull=.Front }
            result.graphics[index],map_error=adapter.graphics(&result.compiled[3+kind],vertex,fragment,{colors=colors,depth={enabled=true,test=effect!=4,write=effect==0,compare=.Less_Equal,format=.D32_Float if effect==0 else .D32_Float_S8_Uint},stencil=stencil,depth_bias={},topology=.Triangle_List,cull=cull,front_counter_clockwise=true},allocator)
            if map_error!=.None { return {},.Reflection }
            if kind==1 {
                for extra in 0..<3 {
                    variant:=extra+1;variant_index:=12+effect*3+extra
                    result.colors[variant_index]=result.colors[index]
                    descriptor:=result.graphics[index].descriptor
                    descriptor.colors=result.colors[variant_index:variant_index+1] if len(colors)>0 else nil
                    descriptor.cull=.None if variant%2==1 else cull
                    descriptor.front_counter_clockwise=variant<2
                    result.graphics[variant_index],map_error=adapter.graphics(&result.compiled[4],vertex,fragment,{colors=descriptor.colors,depth=descriptor.depth,stencil=descriptor.stencil,depth_bias=descriptor.depth_bias,topology=descriptor.topology,cull=descriptor.cull,front_counter_clockwise=descriptor.front_counter_clockwise},allocator)
                    if map_error!=.None { return {},.Reflection }
                }
            }
        }
    }
    success=true; return result,.None
}
feature_shader_destroy :: proc(value:^Feature_Shader) {
    for &mapping in value.graphics { adapter.graphics_destroy(&mapping) }
    adapter.compute_destroy(&value.compute)
    for &compiled in value.compiled { shader.compiled_destroy(&compiled) }
    delete(value.colors,value.allocator); value^={}
}
