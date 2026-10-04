#+test
package shader_tests

import shader "../shader"
import adapter "../shader_adapter"
import gfx ".."
import "core:testing"

@(test)
test_compiler_adapter_maps_exact_stage_bindings_and_runtime_sizes :: proc(t:^testing.T) {
    compiler:shader.Compiler; testing.expect_value(t,shader.compiler_init(&compiler,NAGA_LIBRARY),shader.Error.None); defer shader.compiler_destroy(&compiler)
    artifact,err:=shader.compile(&compiler,SOURCE,{{"vs",.Vertex},{"fs",.Fragment},{"cs",.Compute}}); defer shader.compiled_destroy(&artifact)
    testing.expect(t,err==.None,artifact.message)
    if err!=.None { return }
    compute,compute_error:=adapter.compute(&artifact,"cs"); defer adapter.compute_destroy(&compute)
    testing.expect_value(t,compute_error,adapter.Error.None)
    if compute_error==.None {
        desc:=compute.descriptor
        testing.expect(t,len(desc.buffers)==1 && len(desc.images)==0 && len(desc.samplers)==0)
        testing.expect(t,desc.buffers[0].group==2 && desc.buffers[0].slot==3 && desc.buffers[0].mode==.Write && desc.buffers[0].minimum_size==4)
        testing.expect(t,desc.runtime_sizes_index==8 && desc.runtime_sizes_words==1 && desc.buffers[0].size_index==0 && desc.metal_entry==artifact.entries[2].metal_name)
    }
    colors:=[3]gfx.Color_Target{{format=.RGBA8_Unorm},{format=.RGBA8_Unorm},{format=.RGBA8_Unorm}}
    graphics,graphics_error:=adapter.graphics(&artifact,"vs","fs",{colors=colors[:]}); defer adapter.graphics_destroy(&graphics)
    testing.expect_value(t,graphics_error,adapter.Error.None)
    if graphics_error==.None {
        desc:=graphics.descriptor
        testing.expect(t,len(desc.buffers)==1 && len(desc.images)==1 && len(desc.samplers)==1)
        testing.expect(t,desc.buffers[0].stages=={.Vertex,.Fragment} && desc.buffers[0].mode==.Read && desc.buffers[0].minimum_size==80)
        testing.expect(t,desc.images[0].group==3 && desc.images[0].slot==2 && desc.images[0].stages=={.Fragment} && desc.images[0].vertex_index== -1 && desc.images[0].sample_type==.Float)
        testing.expect(t,desc.samplers[0].group==3 && desc.samplers[0].slot==5 && !desc.samplers[0].comparison && desc.samplers[0].stages=={.Fragment})
    }
    missing,missing_error:=adapter.compute(&artifact,"absent"); defer adapter.compute_destroy(&missing)
    testing.expect_value(t,missing_error,adapter.Error.Missing_Entry)
    invalid,invalid_error:=adapter.graphics(&artifact,"vs","fs",{colors=colors[:2]}); defer adapter.graphics_destroy(&invalid)
    testing.expect_value(t,invalid_error,adapter.Error.Invalid_Interface)
}

@(test)
test_compiler_adapter_rejects_incompatible_stage_linkage :: proc(t:^testing.T) {
    compiler:shader.Compiler; testing.expect_value(t,shader.compiler_init(&compiler,NAGA_LIBRARY),shader.Error.None); defer shader.compiler_destroy(&compiler)
    source:=`
struct Out {@builtin(position) position:vec4f,@location(0) uv:vec3f}
@vertex fn vs(@location(0) position:vec3f)->Out {return Out(vec4f(position,1),position);}
@fragment fn fs(@location(0) uv:vec2f)->@location(0) vec4f {return vec4f(uv,0,1);}
`
    artifact,err:=shader.compile(&compiler,source,{{"vs",.Vertex},{"fs",.Fragment}}); defer shader.compiled_destroy(&artifact)
    testing.expect(t,err==.None,artifact.message)
    if err!=.None { return }
    layout:=gfx.Vertex_Layout{buffers={{binding=0,stride=12}},attributes={{location=0,binding=0,format=.Float3}}}
    invalid,invalid_error:=adapter.graphics(&artifact,"vs","fs",{vertex=layout,colors={{format=.RGBA8_Unorm}}}); defer adapter.graphics_destroy(&invalid)
    testing.expect_value(t,invalid_error,adapter.Error.Invalid_Interface)
    wrong_vertex,wrong_vertex_error:=adapter.graphics(&artifact,"vs","fs",{colors={{format=.RGBA8_Unorm}}}); defer adapter.graphics_destroy(&wrong_vertex)
    testing.expect_value(t,wrong_vertex_error,adapter.Error.Invalid_Vertex_Layout)
}
