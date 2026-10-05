package ui
import "core:testing"

@(test)
test_full_width_content_has_no_false_horizontal_scrollbar :: proc(t:^testing.T) {
    ctx:Context; context_init(&ctx,fixture_fonts()); defer context_destroy(&ctx)
    root:=Descriptor{key=1,kind=.Scroll_Area,layout={width=pixels(150),height=pixels(100)},children={{key=2,kind=.Text,text="Fits",layout={width=percent(1),height=pixels(30)}}}}
    fixture_frame(&ctx,root)
    _,_,horizontal:=scrollbar(&ctx,ctx.nodes[1],true)
    _,_,vertical:=scrollbar(&ctx,ctx.nodes[1],false)
    testing.expect(t,!horizontal && !vertical)
    root.children[0].layout.width=pixels(300)
    fixture_frame(&ctx,root)
    _,_,horizontal=scrollbar(&ctx,ctx.nodes[1],true)
    testing.expect(t,horizontal)
}
