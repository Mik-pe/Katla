#+test
package gfx
import "core:testing"

@(test)
test_graphics_compilation_key_owns_binary_reflection_and_complete_state :: proc(t:^testing.T) {
    name:=[4]byte{'m','a','i','n'};binary:=[3]u32{1,2,3}
    buffers:=[1]Shader_Stage_Buffer{{group=3,slot=4,stages={.Vertex},minimum_size=32}}
    attributes:=[2]Vertex_Attribute{{0,0,0,.Float4},{7,0,16,.Float2}};layouts:=[1]Vertex_Layout_Binding{{0,24,.Vertex}}
    colors:=[1]Color_Target{{format=.RGBA16_Unorm,write_mask={.Red,.Green,.Blue,.Alpha}}}
    source:=Graphics_Desc{vertex_entry=string(name[:]),fragment_entry="fragment",vertex_metal_source="vertex source",fragment_metal_source="fragment source",vertex_spirv=binary[:],buffers=buffers[:],vertex={attributes[:],layouts[:]},colors=colors[:],front_counter_clockwise=true}
    a:=graphics_desc_clone(source);defer graphics_desc_destroy(&a)
    b:=graphics_desc_clone(a);defer graphics_desc_destroy(&b)
    testing.expect(t,graphics_desc_equal(source,a) && graphics_desc_equal(a,b))
    name[0]='X';binary[0]=99;buffers[0].minimum_size=64;attributes[1].offset=8;colors[0].blend_enabled=true
    testing.expect(t,!graphics_desc_equal(source,a));testing.expect_value(t,a.vertex_entry,"main");testing.expect_value(t,a.vertex_spirv[0],u32(1));testing.expect_value(t,a.buffers[0].minimum_size,u64(32))
    b.depth_bias.constant=1;testing.expect(t,!graphics_desc_equal(a,b));b.depth_bias=a.depth_bias
    b.vertex_sizes_words=1;testing.expect(t,!graphics_desc_equal(a,b));b.vertex_sizes_words=0
    b.cull=.Back;testing.expect(t,!graphics_desc_equal(a,b));b.cull=a.cull
    b.colors[0].source_alpha=.Source_Alpha;testing.expect(t,!graphics_desc_equal(a,b))
}
