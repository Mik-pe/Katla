package ui
import "core:testing"
import "core:math"

// Deterministic fixture metrics test layout/input independently of native shaped-font acceptance.
fixture_measure :: proc(_:rawptr,_:Font_Id,text:string,size,wrap:f32)->Vec2 {
    width:f32=0; maximum:f32=0; lines:f32=1
    for r in text { if r=='\n' { maximum=max(maximum,width); width=0; lines+=1 } else { width+=size/2; if wrap>0 && width>wrap { maximum=max(maximum,wrap); width=size/2; lines+=1 } } }
    return {max(maximum,width),lines*size}
}
fixture_caret :: proc(_:rawptr,_:Font_Id,text:string,size,wrap:f32,offset:int)->Vec2 {
    position:Vec2; for r,byte in text { if byte>=offset { break }; if r=='\n' { position.x=0; position.y+=size } else { position.x+=size/2; if wrap>0 && position.x>wrap { position.x=size/2; position.y+=size } } }; return position
}
fixture_hit :: proc(_:rawptr,_:Font_Id,text:string,size,wrap:f32,point:Vec2)->int {
    best:=0; distance:=max(f32)
    for _,byte in text { p:=fixture_caret(nil,0,text,size,wrap,byte); d:=math.abs(p.x-point.x)+math.abs(p.y-point.y)*10; if d<distance { best=byte; distance=d } }
    end:=fixture_caret(nil,0,text,size,wrap,len(text)); if math.abs(end.x-point.x)+math.abs(end.y-point.y)*10<distance { best=len(text) }; return best
}
fixture_navigate :: proc(_:rawptr,_:Font_Id,text:string,_:f32,_:f32,offset,direction:int)->int { return text_previous(text,offset) if direction<0 else text_next(text,offset) }
fixture_grapheme :: proc(_:rawptr,text:string,offset,direction:int)->int { return text_previous(text,offset) if direction<0 else text_next(text,offset) }
fixture_fonts :: proc()->Font_Provider { return {measure=fixture_measure,caret=fixture_caret,hit_test=fixture_hit,navigate=fixture_navigate,grapheme=fixture_grapheme} }
fixture_frame :: proc(ctx:^Context,descriptor:Descriptor,events:[]Input_Event=nil)->Frame_Result { _,result:=frame(ctx,descriptor,{events=events},{400,300}); return result }

@(test)
test_retained_identity_reorder_and_invalid_frame :: proc(t:^testing.T) {
    ctx:Context; testing.expect(t,context_init(&ctx,fixture_fonts())==.None); defer context_destroy(&ctx)
    cell:=state(&ctx,2,0,string("första")); children:=[]Descriptor{{key=2,kind=.Text},{key=3,kind=.Text}}; root:=Descriptor{key=1,kind=.Column,children=children}
    testing.expect(t,fixture_frame(&ctx,root).error==.None); id:=ctx.nodes[2].id
    children[0],children[1]=children[1],children[0]; fixture_frame(&ctx,root)
    testing.expect(t,ctx.nodes[2].id==id); text,ok:=state_get(&ctx,cell,string); testing.expect(t,ok && text=="första")
    children[1].key=3; testing.expect(t,fixture_frame(&ctx,root).error==.Duplicate_Key); testing.expect(t,ctx.nodes[2].id==id)
    root.children=nil; fixture_frame(&ctx,root); _,live:=state_get(&ctx,cell,string); testing.expect(t,!live)
}
@(test)
test_flex_min_max_freeze_grid_percent_and_wrapping :: proc(t:^testing.T) {
    ctx:Context; context_init(&ctx,fixture_fonts()); defer context_destroy(&ctx)
    children:=[]Descriptor{{key=2,kind=.Text,layout={width=pixels(40),grow=1,max_width=pixels(80)}},{key=3,kind=.Text,layout={width=pixels(40),grow=1}},{key=4,kind=.Text,layout={width=percent(0.25)}}}
    root:=Descriptor{key=1,kind=.Row,layout={gap={10,0},align=.Stretch},children=children}; fixture_frame(&ctx,root)
    a,_:=bounds(&ctx,2); b,_:=bounds(&ctx,3); c,_:=bounds(&ctx,4)
    testing.expect(t,a.width==80 && b.width==200 && c.width==100); testing.expect(t,b.x==90 && c.x==300)
    root.kind=.Grid; root.layout={columns=2,gap={8,6},cell_size={100,50}}; fixture_frame(&ctx,root)
    c,_=bounds(&ctx,4); testing.expect(t,c.x==0 && c.y==56 && c.width==100 && c.height==50)
    root.kind=.Row; root.layout={wrap=true,gap={10,5}}; for &child in children { child.layout={width=pixels(190),height=pixels(20),no_shrink=true} }; fixture_frame(&ctx,root)
    c,_=bounds(&ctx,4); testing.expect(t,c.y==25 && c.x==0)
}
@(test)
test_slider_capture_outside_and_exact_track_endpoints :: proc(t:^testing.T) {
    ctx:Context; context_init(&ctx,fixture_fonts()); defer context_destroy(&ctx)
    value:=state(&ctx,1,0,f32(0)); root:=Descriptor{key=1,kind=.Slider,text="Värde",state=value,minimum=0,maximum=1,has_fixed_bounds=true,fixed_bounds={20,20,200,30}}
    fixture_frame(&ctx,root); track:=slider_track(&ctx,ctx.nodes[1])
    result:=fixture_frame(&ctx,root,[]Input_Event{Pointer_Down{position={track.x,30}},Pointer_Move{position={1000,1000}},Pointer_Up{position={1000,1000}}})
    number,ok:=state_get(&ctx,value,f32); testing.expect(t,ok && number==1 && result.consumed_pointer && ctx.captured.key==0)
    actions:=actions_drain(&ctx,Number_Action); defer delete(actions); testing.expect(t,len(actions)==3 && actions[0].started && actions[2].finished && actions[2].value==1)
}
@(test)
test_utf8_ime_commit_submit_selection_clipboard :: proc(t:^testing.T) {
    ctx:Context; context_init(&ctx,fixture_fonts()); defer context_destroy(&ctx)
    value:=state(&ctx,1,0,string("Å")); root:=Descriptor{key=1,kind=.Text_Input,state=value,action=9}
    fixture_frame(&ctx,root); focus(&ctx,ctx.nodes[1].id)
    result:=fixture_frame(&ctx,root,[]Input_Event{Key_Down{key=.End},IME_Preedit{text="ä",cursor=2},Text_Commit{text="ä😊"},Key_Down{key=.Enter}})
    text,ok:=state_get(&ctx,value,string); testing.expect(t,ok && text=="Åä😊" && result.ime.active && ctx.nodes[1].preedit=="")
    actions:=actions_drain(&ctx,Text_Action); defer delete(actions); testing.expect(t,len(actions)==2 && actions[1].submitted && actions[1].state==value)
    fixture_frame(&ctx,root,[]Input_Event{Key_Down{key=.Left},Key_Down{key=.Backspace}}); text,_=state_get(&ctx,value,string); testing.expect(t,text=="Å😊")
    fixture_frame(&ctx,root,[]Input_Event{Key_Down{key=.A,modifiers={.Control}},Key_Down{key=.C,modifiers={.Control}},Key_Down{key=.X,modifiers={.Control}},Key_Down{key=.V,modifiers={.Control}}}); text,_=state_get(&ctx,value,string); testing.expect(t,text=="Å😊")
}
@(test)
test_modal_focus_popup_close_consumes_and_hidden_hit :: proc(t:^testing.T) {
    ctx:Context; context_init(&ctx,fixture_fonts()); defer context_destroy(&ctx)
    children:=[]Descriptor{{key=2,kind=.Button,action=2,has_fixed_bounds=true,fixed_bounds={0,0,400,300}},{key=3,kind=.Context_Menu,action=3,has_fixed_bounds=true,fixed_bounds={100,100,100,80}}}
    root:=Descriptor{key=1,kind=.Stack,children=children}; fixture_frame(&ctx,root)
    result:=fixture_frame(&ctx,root,[]Input_Event{Pointer_Down{position={20,20}},Pointer_Up{position={20,20}}}); dismissed:=actions_drain(&ctx,Dismiss_Action); defer delete(dismissed); clicks:=actions_drain(&ctx,Click_Action); defer delete(clicks)
    testing.expect(t,result.consumed_pointer && len(dismissed)==1 && len(clicks)==0)
    children[1].kind=.Modal; modal_children:=[]Descriptor{{key=4,kind=.Button},{key=5,kind=.Button}}; children[1].children=modal_children; children[1].layout={}
    fixture_frame(&ctx,root); testing.expect(t,ctx.focused.key==4); fixture_frame(&ctx,root,[]Input_Event{Key_Down{key=.Tab}}); testing.expect(t,ctx.focused.key==5); fixture_frame(&ctx,root,[]Input_Event{Key_Down{key=.Tab}}); testing.expect(t,ctx.focused.key==4)
    children[1].hidden=true; fixture_frame(&ctx,root,[]Input_Event{Pointer_Down{position={20,20}},Pointer_Up{position={20,20}}}); final:=actions_drain(&ctx,Click_Action); defer delete(final); testing.expect(t,len(final)==1 && final[0].action==2)
}
@(test)
test_dock_exact_tab_moves_local_ratio_and_json_rollback :: proc(t:^testing.T) {
    tree:Dock_Tree; testing.expect(t,dock_init(&tree,[]Tab_Id{1,2,3})==.None); defer dock_destroy(&tree)
    root:=tree.root; testing.expect(t,dock_apply(&tree,{kind=.Move,source=root,target=root,tab=2,zone=.Right})==.None)
    split:=tree.nodes[root]; testing.expect(t,split.kind==.Split); left,right:=split.children[0],split.children[1]
    testing.expect(t,tree.nodes[left].tabs[0]==1 && tree.nodes[right].tabs[0]==2)
    testing.expect(t,dock_apply(&tree,{kind=.Resize,target=root,ratio=0.25})==.None); regions:=dock_bounds(&tree,{100,40,404,200}); defer delete(regions)
    testing.expect(t,regions[1].bounds.x==100 && regions[1].bounds.width==100 && regions[2].bounds.x==204)
    snapshot:=dock_snapshot(&tree); defer delete(snapshot)
    testing.expect(t,dock_restore(&tree,snapshot)==.None); before:=dock_snapshot(&tree); defer delete(before)
    testing.expect(t,dock_restore(&tree,"{\"type\":\"Leaf\",\"tabs\":[1,1],\"active\":0}")==.Duplicate_Tab)
    after:=dock_snapshot(&tree); defer delete(after); testing.expect(t,after==before)
    left,right=tree.nodes[tree.root].children[0],tree.nodes[tree.root].children[1]
    testing.expect(t,dock_apply(&tree,{kind=.Move,source=right,target=left,tab=2,zone=.Center,index=1})==.None)
    testing.expect(t,tree.nodes[tree.root].kind==.Leaf && len(tree.nodes[tree.root].tabs)==3 && tree.nodes[tree.root].tabs[1]==2)
}

@(test)
test_inactive_retention_preserves_code_undo_and_exact_state :: proc(t:^testing.T) {
    ctx:Context; context_init(&ctx,fixture_fonts()); defer context_destroy(&ctx)
    text:=state(&ctx,2,0,string("a")); children:=[]Descriptor{{key=2,kind=.Code_Editor,state=text,layout={height=pixels(100)}}}; root:=Descriptor{key=1,kind=.Column,children=children}
    fixture_frame(&ctx,root); id:=ctx.nodes[2].id; focus(&ctx,id)
    fixture_frame(&ctx,root,[]Input_Event{Key_Down{key=.End},Text_Commit{text="å"}}); ctx.nodes[2].scroll={0,25}
    testing.expect(t,retain(&ctx,2)); root.children=nil; fixture_frame(&ctx,root)
    testing.expect(t,ctx.focused.key==0 && !ctx.nodes[2].mounted && ctx.nodes[2].scroll.y==25)
    retained,ok:=state_get(&ctx,text,string); testing.expect(t,ok && retained=="aå" && len(ctx.nodes[2].undo)==1)
    retain(&ctx,2); fixture_frame(&ctx,root); root.children=children; fixture_frame(&ctx,root); testing.expect(t,ctx.nodes[2].id==id); focus(&ctx,id)
    fixture_frame(&ctx,root,[]Input_Event{Key_Down{key=.Z,modifiers={.Super}}}); retained,_=state_get(&ctx,text,string); testing.expect(t,retained=="a")
    fixture_frame(&ctx,root,[]Input_Event{Key_Down{key=.Z,modifiers={.Super,.Shift}}}); retained,_=state_get(&ctx,text,string); testing.expect(t,retained=="aå")
    retain(&ctx,2); root.children=nil; fixture_frame(&ctx,root); testing.expect(t,forget(&ctx,2)); _,ok=state_get(&ctx,text,string); testing.expect(t,!ok)
}
@(test)
test_code_indent_syntax_runs_and_numeric_atomic_validation :: proc(t:^testing.T) {
    ctx:Context; context_init(&ctx,fixture_fonts()); defer context_destroy(&ctx)
    text:=state(&ctx,1,0,string("let å = 1\nprint(å)")); root:=Descriptor{key=1,kind=.Code_Editor,state=text,syntax=[]Text_Run{{0,3,{1,0,0,1}}}}
    fixture_frame(&ctx,root); focus(&ctx,ctx.nodes[1].id)
    fixture_frame(&ctx,root,[]Input_Event{Key_Down{key=.A,modifiers={.Control}},Key_Down{key=.Tab}})
    value,_:=state_get(&ctx,text,string); testing.expect(t,value=="\tlet å = 1\n\tprint(å)")
    fixture_frame(&ctx,root,[]Input_Event{Key_Down{key=.Z,modifiers={.Control}}}); value,_=state_get(&ctx,text,string); testing.expect(t,value=="let å = 1\nprint(å)")
    has_syntax:=false; for command in ctx.commands { if draw,present:=command.(Text_Draw); present && len(draw.runs)==1 { has_syntax=true; testing.expect(t,draw.runs[0].start==0 && draw.runs[0].end==3) } }; testing.expect(t,has_syntax)
    number:=state(&ctx,3,0,f32(2)); root={key=3,kind=.Numeric_Input,state=number,minimum=0,maximum=10}; fixture_frame(&ctx,root); focus(&ctx,ctx.nodes[3].id); actions_clear(&ctx)
    fixture_frame(&ctx,root,[]Input_Event{Key_Down{key=.A,modifiers={.Control}},Text_Commit{text="nan"},Key_Down{key=.Enter}})
    current,_:=state_get(&ctx,number,f32); testing.expect(t,current==2 && ctx.nodes[3].numeric_invalid); none:=actions_drain(&ctx,Number_Action); defer delete(none); testing.expect(t,len(none)==0)
    fixture_frame(&ctx,root,[]Input_Event{Key_Down{key=.A,modifiers={.Control}},Text_Commit{text="7.5"},Key_Down{key=.Enter}})
    current,_=state_get(&ctx,number,f32); testing.expect(t,current==7.5 && !ctx.nodes[3].numeric_invalid); accepted:=actions_drain(&ctx,Number_Action); defer delete(accepted); testing.expect(t,len(accepted)==1 && accepted[0].started && accepted[0].finished)
}
@(test)
test_scroll_capture_nested_clip_and_popup_overlay :: proc(t:^testing.T) {
    ctx:Context; context_init(&ctx,fixture_fonts()); defer context_destroy(&ctx)
    leaf:=[]Descriptor{{key=4,kind=.Button,has_fixed_bounds=true,fixed_bounds={20,500,100,30}}}; content:=[]Descriptor{{key=3,kind=.Column,layout={height=pixels(900),no_shrink=true},children=leaf}}
    children:=[]Descriptor{{key=2,kind=.Scroll_Area,has_fixed_bounds=true,fixed_bounds={0,0,200,200},children=content},{key=5,kind=.Context_Menu,has_fixed_bounds=true,fixed_bounds={180,0,20,100}}}; root:=Descriptor{key=1,kind=.Stack,children=children}
    fixture_frame(&ctx,root); testing.expect(t,ctx.nodes[4].clip.height==0); testing.expect(t,hit_node(&ctx,{195,10})==ctx.nodes[5])
    children[1].hidden=true; fixture_frame(&ctx,root); track,thumb,present:=scrollbar(&ctx,ctx.nodes[2],false); testing.expect(t,present)
    result:=fixture_frame(&ctx,root,[]Input_Event{Pointer_Down{position={track.x+3,thumb.y+3}},Pointer_Move{position={track.x+3,800}},Pointer_Up{position={track.x+3,800}}})
    testing.expect(t,result.consumed_pointer && ctx.nodes[2].scroll.y==700 && ctx.captured.key==0)
    fixture_frame(&ctx,root,[]Input_Event{Scroll{position={40,40},delta={0,-300}}}); testing.expect(t,ctx.nodes[2].scroll.y==400)
    for command in ctx.commands {
        switch draw in command {
        case Rect_Draw: if draw.bounds.y>=500 { testing.expect(t,draw.clip.height==0) }
        case Text_Draw,Image_Draw,Mesh_Draw:
        }
    }
}
@(test)
test_dock_reopen_same_leaf_order_stale_ids_and_legacy_ratio :: proc(t:^testing.T) {
    tree:Dock_Tree; dock_init(&tree,[]Tab_Id{1,2,3}); defer dock_destroy(&tree)
    root:=tree.root; testing.expect(t,dock_apply(&tree,{kind=.Move,source=root,target=root,tab=1,zone=.Center,index=2})==.None)
    testing.expect(t,tree.nodes[root].tabs[0]==2 && tree.nodes[root].tabs[1]==1)
    testing.expect(t,dock_apply(&tree,{kind=.Close,source=root,tab=1})==.None); testing.expect(t,dock_apply(&tree,{kind=.Insert,target=root,tab=1,index=0})==.None)
    testing.expect(t,tree.nodes[root].tabs[0]==1); testing.expect(t,dock_apply(&tree,{kind=.Insert,target=root,tab=1})==.Duplicate_Tab)
    saved:=dock_snapshot(&tree); defer delete(saved); testing.expect(t,dock_restore(&tree,saved)==.None && tree.root!=root)
    testing.expect(t,dock_apply(&tree,{kind=.Activate,source=root,tab=1})==.Not_Leaf)
    legacy:="{\"type\":\"Split\",\"direction\":\"Horizontal\",\"ratio\":0,\"children\":[{\"type\":\"Leaf\",\"tabs\":[1],\"active\":0},{\"type\":\"Leaf\",\"tabs\":[2],\"active\":0}]}"
    testing.expect(t,dock_restore(&tree,legacy)==.None && tree.nodes[tree.root].ratio==0)
}

@(test)
test_blur_removed_text_snapshot_and_dragged_row_click_suppression :: proc(t:^testing.T) {
    ctx:Context; context_init(&ctx,fixture_fonts()); defer context_destroy(&ctx)
    text:=state(&ctx,2,0,string("old")); children:=[]Descriptor{{key=2,kind=.Text_Input,state=text,action=12},{key=3,kind=.Selectable,action=13,draggable=true}}
    root:=Descriptor{key=1,kind=.Column,children=children}; fixture_frame(&ctx,root); focus(&ctx,ctx.nodes[2].id)
    fixture_frame(&ctx,root,[]Input_Event{Key_Down{key=.A,modifiers={.Control}},Text_Commit{text="ändrad"}})
    root.children=children[1:]; fixture_frame(&ctx,root); changes:=actions_drain(&ctx,Text_Action); defer delete(changes)
    testing.expect(t,len(changes)==2 && changes[1].submitted && action_text(&ctx,changes[1])=="ändrad"); _,valid:=state_get(&ctx,text,string); testing.expect(t,!valid)
    row:=ctx.nodes[3].bounds; p:=Vec2{row.x+20,row.y+10}; fixture_frame(&ctx,root,[]Input_Event{Pointer_Down{position=p,modifiers={.Control},clicks=1},Pointer_Move{position=p+Vec2{30,0}},Pointer_Up{position=p+Vec2{30,0}}})
    pointers:=actions_drain(&ctx,Pointer_Action); defer delete(pointers); clicks:=actions_drain(&ctx,Click_Action); defer delete(clicks)
    testing.expect(t,len(pointers)==3 && pointers[0].pressed && pointers[2].released && len(clicks)==0)
    fixture_frame(&ctx,root,[]Input_Event{Pointer_Down{position=p,modifiers={.Control},clicks=2},Pointer_Up{position=p}})
    clicked:=actions_drain(&ctx,Click_Action); defer delete(clicked); testing.expect(t,len(clicked)==1 && clicked[0].clicks==2 && .Control in clicked[0].modifiers)
}
@(test)
test_popup_keyboard_disabled_ancestors_and_blur_capture_finish :: proc(t:^testing.T) {
    ctx:Context; context_init(&ctx,fixture_fonts()); defer context_destroy(&ctx)
    menu:=[]Descriptor{{key=4,kind=.Menu_Item,text="A"},{key=5,kind=.Menu_Item,text="B"}}
    children:=[]Descriptor{{key=2,kind=.Button,text="Open"},{key=3,kind=.Context_Menu,children=menu,has_fixed_bounds=true,fixed_bounds={50,50,100,90}}}; root:=Descriptor{key=1,kind=.Column,children=children}
    fixture_frame(&ctx,root); testing.expect(t,ctx.focused.key==4 && ctx.nodes[5].bounds.y>ctx.nodes[4].bounds.y)
    fixture_frame(&ctx,root,[]Input_Event{Key_Down{key=.Down}}); testing.expect(t,ctx.focused.key==5); fixture_frame(&ctx,root,[]Input_Event{Key_Down{key=.Tab}}); testing.expect(t,ctx.focused.key==4)
    children[1].disabled=true; fixture_frame(&ctx,root); testing.expect(t,!focusable(ctx.nodes[4]) && ctx.focused.key==0)
    number:=state(&ctx,6,0,f32(0)); root={key=6,kind=.Slider,state=number,minimum=0,maximum=1}; fixture_frame(&ctx,root); actions_clear(&ctx)
    result:=fixture_frame(&ctx,root,[]Input_Event{Pointer_Down{position={100,20}},Pointer_Move{position={300,20}},Window_Focus{focused=false}})
    numbers:=actions_drain(&ctx,Number_Action); defer delete(numbers); testing.expect(t,len(numbers)==3 && numbers[2].finished && ctx.captured.key==0 && !result.ime.active)
    invalid:=Descriptor{key=7,kind=.Text_Input}; testing.expect(t,fixture_frame(&ctx,invalid).error==.Invalid_State && ctx.nodes[6]!=nil)
}

@(test)
test_rust_layout_reference_padding_flex_grid_stack_and_resize :: proc(t:^testing.T) {
    ctx:Context; context_init(&ctx,fixture_fonts()); defer context_destroy(&ctx)
    // These fixtures preserve existing katla_ui declarative/layout.rs migration assertions.
    children:=[]Descriptor{{key=2,kind=.Column,layout={width=pixels(100)},children=[]Descriptor{{key=3,kind=.Text,text="A"}}},{key=4,kind=.Text,text="B"}}
    root:=Descriptor{key=1,kind=.Row,layout={padding={20,20,20,20},align=.Start},children=children}; fixture_frame(&ctx,root)
    a,_:=bounds(&ctx,2); testing.expect(t,a.width==100 && a.x==20 && a.y==20)
    children[0].layout.width=pixels(200); fixture_frame(&ctx,root); a,_=bounds(&ctx,2); testing.expect(t,a.width==200)
    root.kind=.Grid; root.layout={columns=3,cell_size={80,40},gap={8,6}}; root.children=[]Descriptor{{key=2,kind=.Text,text="A"},{key=3,kind=.Text,text="B"},{key=4,kind=.Text,text="C"},{key=5,kind=.Text,text="D"}}; fixture_frame(&ctx,root)
    d,_:=bounds(&ctx,5); testing.expect(t,d.x==0 && d.y==46 && d.width==80 && d.height==40)
    root.kind=.Stack; root.layout={width=pixels(200),height=pixels(100)}; root.children=[]Descriptor{{key=6,kind=.Text,text="A",layout={width=pixels(20),height=pixels(10),anchor={1,1}}}}; fixture_frame(&ctx,root)
    bottom,_:=bounds(&ctx,6); testing.expect(t,bottom.x==180 && bottom.y==90)
    root.layout={}; fixture_frame(&ctx,root); bottom,_=bounds(&ctx,6); testing.expect(t,bottom.x==380 && bottom.y==290)
    _,result:=frame(&ctx,root,{},{}); testing.expect(t,result.error==.None); bottom,_=bounds(&ctx,6); testing.expect(t,bottom.width>=0 && bottom.height>=0)
}

@(test)
test_ordered_actions_finish_before_next_click_and_dock_input_identity :: proc(t:^testing.T) {
    ctx:Context; context_init(&ctx,fixture_fonts()); defer context_destroy(&ctx)
    number:=state(&ctx,2,0,f32(0)); children:=[]Descriptor{{key=2,kind=.Slider,state=number,minimum=0,maximum=1,has_fixed_bounds=true,fixed_bounds={0,0,200,30}},{key=3,kind=.Button,action=3,has_fixed_bounds=true,fixed_bounds={0,40,200,30}}}; root:=Descriptor{key=1,kind=.Stack,children=children}
    fixture_frame(&ctx,root); actions_clear(&ctx)
    fixture_frame(&ctx,root,[]Input_Event{Pointer_Down{position={20,15}},Pointer_Up{position={100,15}},Pointer_Down{position={20,50}},Pointer_Up{position={20,50}}})
    ordered:=actions_drain_all(&ctx); defer delete(ordered); testing.expect(t,len(ordered)==3)
    final,is_number:=ordered[1].(Number_Action); clicked,is_click:=ordered[2].(Click_Action); testing.expect(t,is_number && final.finished && is_click && clicked.action==3)
    tree:Dock_Tree; dock_init(&tree,[]Tab_Id{1,2}); defer dock_destroy(&tree)
    root={key=10,kind=.Dock_Space,dock=&tree,dock_tabs=[]Dock_Tab{{1,"Scene"},{2,"Inspector"}}}; fixture_frame(&ctx,root); actions_clear(&ctx)
    fixture_frame(&ctx,root,[]Input_Event{Pointer_Down{position={70,15}},Pointer_Move{position={390,140}},Pointer_Up{position={390,140}}})
    moves:=actions_drain(&ctx,Dock_Action); defer delete(moves); testing.expect(t,len(moves)==2 && moves[0].kind==.Activate && moves[0].tab==2 && moves[1].kind==.Move && moves[1].tab==2 && moves[1].zone==.Right)
    for action in moves { testing.expect(t,dock_apply(&tree,action)==.None) }; leaf,_,present:=dock_find_tab(&tree,2); testing.expect(t,present && tree.nodes[leaf].tabs[0]==2)
}

@(test)
test_control_custom_background_is_single_layer_and_survives_hover :: proc(t:^testing.T) {
    ctx:Context; context_init(&ctx,fixture_fonts()); defer context_destroy(&ctx)
    button:=Descriptor{key=1,kind=.Button,has_background=true,background={1,0,0,0.5},has_fixed_bounds=true,fixed_bounds={0,0,100,30}}
    fixture_frame(&ctx,button,{Pointer_Move{position={10,10}}})
    count:=0
    for command in ctx.commands { if draw,present:=command.(Rect_Draw); present && draw.bounds==button.fixed_bounds { count+=1; testing.expect(t,draw.color==button.background) } }
    testing.expect_value(t,count,1)
}
