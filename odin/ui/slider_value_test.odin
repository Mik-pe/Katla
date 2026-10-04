package ui
import "core:testing"

@(test)
test_slider_actual_numeric_value_keeps_track_geometry :: proc(t:^testing.T) {
    ctx:Context; context_init(&ctx,fixture_fonts()); defer context_destroy(&ctx)
    descriptor:=Descriptor{key=1,kind=.Slider,text="Scale",value=-0.25,minimum=-1,maximum=1,has_fixed_bounds=true,fixed_bounds={0,0,180,30}}
    fixture_frame(&ctx,descriptor)
    track:=slider_track(&ctx,ctx.nodes[1]); found:=false
    for command in ctx.commands { if draw,ok:=command.(Text_Draw);ok && draw.text=="-0.25" { found=true; testing.expect(t,draw.position.x>=track.x+track.width && draw.clip.x>=track.x+track.width && draw.clip.width>0) } }
    testing.expect(t,found && track.width>0)
    descriptor.value=1; fixture_frame(&ctx,descriptor)
    testing.expect(t,slider_track(&ctx,ctx.nodes[1])==track)
    found=false; for command in ctx.commands { if draw,ok:=command.(Text_Draw);ok && draw.text=="1" { found=true } }; testing.expect(t,found)
    result:=fixture_frame(&ctx,descriptor,[]Input_Event{Pointer_Down{position={track.x,15}},Pointer_Up{position={track.x,15}}})
    numbers:=actions_drain(&ctx,Number_Action); defer delete(numbers)
    testing.expect(t,result.error==.None && len(numbers)==2 && numbers[0].value==-1 && numbers[1].finished && numbers[1].value==-1)
}

@(test)
test_numeric_commit_preserves_small_values_in_full_float_range :: proc(t:^testing.T) {
    ctx:Context; context_init(&ctx,fixture_fonts()); defer context_destroy(&ctx)
    number:=state(&ctx,1,0,f32(0))
    descriptor:=Descriptor{key=1,kind=.Numeric_Input,state=number,minimum=-max(f32),maximum=max(f32),step=.01}
    fixture_frame(&ctx,descriptor); focus(&ctx,ctx.nodes[1].id); actions_clear(&ctx)
    fixture_frame(&ctx,descriptor,[]Input_Event{Key_Down{key=.A,modifiers={.Control}},Text_Commit{text="-1.253"},Key_Down{key=.Enter}})
    current,present:=state_get(&ctx,number,f32); testing.expect(t,present && current== -1.25)
    actions:=actions_drain(&ctx,Number_Action); defer delete(actions)
    testing.expect(t,len(actions)==1 && actions[0].started && actions[0].finished && actions[0].value== -1.25)
    descriptor.minimum=.03; descriptor.maximum=.63; descriptor.step=.1; state_set(&ctx,number,f32(.03))
    fixture_frame(&ctx,descriptor); focus(&ctx,ctx.nodes[1].id)
    fixture_frame(&ctx,descriptor,[]Input_Event{Key_Down{key=.A,modifiers={.Control}},Text_Commit{text=".14"},Key_Down{key=.Enter}})
    current,_=state_get(&ctx,number,f32); testing.expect(t,current>=.129999 && current<=.130001)
}
