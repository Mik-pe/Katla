package ui
import "core:testing"
import "base:runtime"
import "core:unicode/utf8"

@(test)
test_ellipsis_shaped_width_and_owned_hover_text :: proc(t:^testing.T) {
    ctx:Context; testing.expect(t,context_init(&ctx,fixture_fonts(),allocator=runtime.heap_allocator())==.None); defer context_destroy(&ctx)
    descriptor:=Descriptor{key=1,kind=.Text,text="Åä😊 filename.png",font_size=20,text_max_width=40,has_fixed_bounds=true,fixed_bounds={20,20,40,20}}
    draw,result:=frame(&ctx,descriptor,{events=[]Input_Event{Pointer_Move{position={25,25}}}},{120,80})
    testing.expect(t,result.error==.None && !result.consumed_pointer && len(draw.commands)==3)
    label,ok:=draw.commands[0].(Text_Draw); testing.expect(t,ok && label.text=="Åä😊…" && label.wrap_width==0 && utf8.valid_string(label.text))
    full,full_ok:=draw.commands[2].(Text_Draw); testing.expect(t,full_ok && full.text==descriptor.text && rect_contains(full.clip,full.position))
    fixture_frame(&ctx,descriptor,[]Input_Event{Pointer_Move{position={100,70}}})
    testing.expect(t,len(ctx.commands)==1)
    short:=descriptor; short.text="Åä"; fixture_frame(&ctx,short,[]Input_Event{Pointer_Move{position={25,25}}})
    complete,complete_ok:=ctx.commands[0].(Text_Draw); testing.expect(t,complete_ok && complete.text=="Åä" && len(ctx.commands)==1)
}

@(test)
test_ellipsis_narrow_newline_validation_and_overlay_scope :: proc(t:^testing.T) {
    ctx:Context; context_init(&ctx,fixture_fonts()); defer context_destroy(&ctx)
    children:=[]Descriptor{{key=2,kind=.Text,text="first\nsecond",text_max_width=80,has_fixed_bounds=true,fixed_bounds={0,0,80,20}},{key=3,kind=.Button,layer=.Overlay,has_fixed_bounds=true,fixed_bounds={0,0,80,20}}}
    root:=Descriptor{key=1,kind=.Stack,children=children}
    fixture_frame(&ctx,root,[]Input_Event{Pointer_Move{position={10,10}}})
    first,_:=ctx.commands[0].(Text_Draw); testing.expect(t,first.text=="first…")
    for command in ctx.commands { if text,ok:=command.(Text_Draw);ok { testing.expect(t,text.text!="first\nsecond") } }
    children[0].text_max_width=1; children[1].hidden=true; fixture_frame(&ctx,root,[]Input_Event{Pointer_Move{position={200,200}}})
    testing.expect(t,len(ctx.commands)==0)
    children[0].text_max_width=-1; testing.expect(t,fixture_frame(&ctx,root).error==.Invalid_Descriptor)
}
