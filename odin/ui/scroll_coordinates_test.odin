package ui
import "core:testing"

@(test)
test_wheel_then_click_uses_same_scrolled_draw_geometry :: proc(t:^testing.T) {
    ctx:Context; context_init(&ctx,fixture_fonts()); defer context_destroy(&ctx)
    content:=[]Descriptor{{key=3,kind=.Text,layout={height=pixels(200),no_shrink=true}},{key=4,kind=.Button,text="visible target",action=41,layout={height=pixels(40),no_shrink=true}},{key=5,kind=.Text,layout={height=pixels(60),no_shrink=true}}}
    root:=Descriptor{key=1,kind=.Scroll_Area,layout={width=pixels(150),height=pixels(100)},children=content}
    fixture_frame(&ctx,root)
    testing.expect(t,ctx.nodes[4].clip.height==0)
    result:=fixture_frame(&ctx,root,[]Input_Event{Scroll{position={30,30},delta={0,150}},Pointer_Down{position={30,60}},Pointer_Up{position={30,60}}})
    clicks:=actions_drain(&ctx,Click_Action); defer delete(clicks)
    testing.expect(t,result.error==.None && len(clicks)==1 && clicks[0].action==41 && ctx.nodes[4].bounds.y==50 && ctx.nodes[4].clip.height==40)
    painted:=false
    for command in ctx.commands { if draw,ok:=command.(Text_Draw);ok && draw.text=="visible target" { painted=true; testing.expect(t,draw.position.y>=50 && draw.position.y<90 && draw.clip==ctx.nodes[4].clip) } }
    testing.expect(t,painted)
}

@(test)
test_scroll_percent_children_measure_against_viewport :: proc(t:^testing.T) {
    ctx:Context; context_init(&ctx,fixture_fonts()); defer context_destroy(&ctx)
    children:=[]Descriptor{{key=2,kind=.Text,layout={height=percent(0.5),no_shrink=true}},{key=3,kind=.Text,layout={height=pixels(200),no_shrink=true}}}
    root:=Descriptor{key=1,kind=.Scroll_Area,layout={width=pixels(150),height=pixels(100)},children=children}
    fixture_frame(&ctx,root)
    testing.expect(t,ctx.nodes[1].content.height==250)
    fixture_frame(&ctx,root,[]Input_Event{Scroll{position={30,30},delta={0,1000}}})
    testing.expect(t,ctx.nodes[1].scroll.y==150 && ctx.nodes[1].content.height==250)
    fixture_frame(&ctx,root)
    testing.expect(t,ctx.nodes[1].scroll.y==150 && ctx.nodes[1].content.height==250)
}
