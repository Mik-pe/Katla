#+build linux, windows
#+test
package window

import wn "../../deps/window_native"
import ui "../../ui"
import "core:testing"

Portable_Input_Fixture :: struct { active:u32,rect:[4]i32 }
portable_test_state :: proc "c" (_:rawptr,value:^wn.State)->i32 { value^={width=800,height=480,scale=2,density=1,visible=1,focused=1};return 1 }
portable_test_ime :: proc "c" (state:rawptr,active:u32,x,y,width,height:i32)->i32 { fixture:=cast(^Portable_Input_Fixture)state;fixture.active=active;fixture.rect={x,y,width,height};return 1 }

@(test)
test_portable_utf8_preedit_key_focus_and_dpi_coordinates :: proc(t:^testing.T) {
    fixture:Portable_Input_Fixture
    window:=Window{native=&fixture,api={state=portable_test_state,ime=portable_test_ime}}
    owner:Native_Input;testing.expect_value(t,native_input_init(&owner,&window),Window_Error.None);defer native_input_destroy(&owner)
    input_focus(&owner.input,true)
    key:=wn.Event{kind=.Key_Down,code=26,modifiers=3};native_input_event(&owner,&key)
    testing.expect(t,.W in owner.input.keys && owner.input.modifiers==ui.Modifiers{.Shift,.Control})
    button:=wn.Event{kind=.Button_Down,button=1,x=40,y=20,clicks=2};native_input_event(&owner,&button)
    testing.expect(t,owner.input.pointer==ui.Vec2{20,10} && .Left in owner.input.buttons)
    edit:=wn.Event{kind=.Editing,text="é😊",start=2,length=1};native_input_event(&owner,&edit)
    preedit,matched:=owner.input.events[len(owner.input.events)-1].(ui.IME_Preedit)
    testing.expect(t,matched && preedit.text=="é😊" && preedit.cursor==3 && preedit.selection_end==7)
    committed:=wn.Event{kind=.Text,text="Åäö välj dörren"};native_input_event(&owner,&committed)
    commit,typed:=owner.input.events[len(owner.input.events)-1].(ui.Text_Commit);testing.expect(t,typed && commit.text=="Åäö välj dörren")
    native_input_ime(&owner,{active=true,cursor={4,8,2,10}})
    testing.expect(t,fixture.active==1 && fixture.rect==[4]i32{8,16,4,20})
    blur:=wn.Event{kind=.Focus,code=0};native_input_event(&owner,&blur)
    testing.expect(t,!owner.input.focused && owner.input.keys=={} && owner.input.buttons=={})
    input_begin(&owner.input);testing.expect_value(t,len(owner.input.events),0)
}
