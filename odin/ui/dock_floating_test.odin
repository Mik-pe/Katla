package ui
import "core:testing"
import "core:mem"

@(test)
test_floating_dock_transaction_layer_bounds_persistence_and_redock :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    tree:Dock_Tree; testing.expect_value(t,dock_init(&tree,{1,2,3}),Dock_Error.None)
    testing.expect_value(t,dock_apply(&tree,{kind=.Undock,source=tree.root,tab=2,bounds={20,40,240,160}}),Dock_Error.None)
    floating:=tree.floating[0].root
    testing.expect(t,len(tree.floating)==1 && tree.nodes[tree.root].tabs[0]==1 && tree.nodes[floating].tabs[0]==2)
    testing.expect_value(t,dock_apply(&tree,{kind=.Move,source=tree.root,target=floating,tab=3,zone=.Right}),Dock_Error.None)
    testing.expect(t,tree.nodes[floating].kind==.Split)
    regions:=dock_bounds(&tree,{0,0,400,300}); testing.expect(t,len(regions)==4 && regions[0].floating_root==0 && regions[1].floating_root==floating && regions[3].floating_root==floating); delete(regions)
    snapshot:=dock_snapshot(&tree); testing.expect_value(t,dock_restore(&tree,snapshot),Dock_Error.None)
    restored:=dock_snapshot(&tree); testing.expect_value(t,restored,snapshot); delete(restored)
    testing.expect_value(t,dock_restore(&tree,`{"version":2,"root":{"type":"Leaf","tabs":[1],"active":0},"floating":[{"bounds":[20,40,240,160],"root":{"type":"Leaf","tabs":[1],"active":0}}]}`),Dock_Error.Duplicate_Tab)
    after:=dock_snapshot(&tree); testing.expect_value(t,after,snapshot); delete(after); delete(snapshot)
    first,_,_:=dock_find_tab(&tree,2); second,_,_:=dock_find_tab(&tree,3)
    testing.expect_value(t,dock_apply(&tree,{kind=.Move,source=first,target=tree.root,tab=2,index=1}),Dock_Error.None)
    second,_,_=dock_find_tab(&tree,3); testing.expect_value(t,dock_apply(&tree,{kind=.Move,source=second,target=tree.root,tab=3,index=2}),Dock_Error.None)
    testing.expect(t,len(tree.floating)==0 && len(tree.nodes)==1 && len(tree.nodes[tree.root].tabs)==3)
    testing.expect_value(t,dock_apply(&tree,{kind=.Undock,source=tree.root,tab=2,bounds={0,0,0,0}}),Dock_Error.Invalid_Bounds)
    dock_destroy(&tree); testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}

@(test)
test_floating_dock_ordered_pointer_capture_resize_and_undock :: proc(t:^testing.T) {
    ctx:Context; context_init(&ctx,fixture_fonts()); defer context_destroy(&ctx)
    tree:Dock_Tree; dock_init(&tree,{1,2}); defer dock_destroy(&tree)
    children:=[]Descriptor{{key=2,kind=.Dock_Space,dock=&tree,dock_tabs={{1,"Scene"},{2,"Inspector"}},has_fixed_bounds=true,fixed_bounds={0,0,200,300}}}
    root:=Descriptor{key=1,kind=.Stack,children=children}
    fixture_frame(&ctx,root); actions_clear(&ctx)
    result:=fixture_frame(&ctx,root,{Pointer_Down{position={70,15}},Pointer_Move{position={350,100}},Pointer_Up{position={350,100}}})
    testing.expect(t,result.consumed_pointer && !result.captured_pointer)
    actions:=actions_drain(&ctx,Dock_Action); testing.expect(t,len(actions)==2 && actions[0].kind==.Activate && actions[1].kind==.Undock && actions[1].tab==2)
    for action in actions { testing.expect_value(t,dock_apply(&tree,action),Dock_Error.None) }; delete(actions); actions_clear(&ctx)
    floating:=tree.floating[0].root; testing.expect_value(t,dock_apply(&tree,{kind=.Float_Bounds,target=floating,bounds={20,40,240,160}}),Dock_Error.None)
    float_nodes:=[]Descriptor{{key=3,kind=.Dock_Space,dock=&tree,dock_root=floating,dock_tabs={{2,"Inspector"}},has_fixed_bounds=true,fixed_bounds={20,40,240,160}}}
    all:=[]Descriptor{children[0],{key=4,kind=.Stack,dock=&tree,dock_root=floating,has_fixed_bounds=true,fixed_bounds={20,40,240,160},children=float_nodes}}; root.children=all
    fixture_frame(&ctx,root); actions_clear(&ctx)
    result=fixture_frame(&ctx,root,{Pointer_Down{position={180,55}},Pointer_Move{position={500,360}}})
    testing.expect(t,result.captured_pointer && ctx.dock_float==floating)
    actions=actions_drain(&ctx,Dock_Action); testing.expect(t,len(actions)==2 && actions[0].kind==.Raise && actions[1].kind==.Float_Bounds && actions[1].bounds.x==340 && actions[1].bounds.y==345); delete(actions); actions_clear(&ctx)
    fixture_frame(&ctx,root,{Pointer_Up{position={180,55}}}); actions_clear(&ctx)
    result=fixture_frame(&ctx,root,{Pointer_Down{position={258,198}},Pointer_Move{position={300,250}},Pointer_Up{position={300,250}}})
    testing.expect(t,result.consumed_pointer && !result.captured_pointer)
    actions=actions_drain(&ctx,Dock_Action); testing.expect(t,len(actions)==3 && actions[1].kind==.Float_Bounds && actions[1].bounds.width==282 && actions[1].bounds.height==212); delete(actions)
}
