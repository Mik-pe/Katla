//! Retained application descriptors remain independent of GPU and scene policies.
package ui
import "core:mem"

Node_Id :: struct { owner:rawptr,key,generation:u64 }
State_Id :: struct { node:Node_Id,slot:u32 }
Value :: union { bool,f32,i64,string,Vec2 }
Widget_Kind :: enum {
    Column,Row,Stack,Grid,Text,Button,Icon_Button,Menu_Bar,Menu_Item,Context_Menu,
    Text_Input,Code_Editor,Numeric_Input,Slider,Drag_Value,Checkbox,Combo,Tree_Row,Selectable,Scroll_Area,Tabs,
    Dock_Space,Modal,Splitter,Section,Image,Tooltip,Separator,Progress,Timeline,
}
Layer :: enum { Content,Overlay,Popup,Modal,Tooltip }
Align :: enum { Stretch,Start,Center,End }
Justify :: enum { Start,Center,End,Space_Between,Space_Around,Space_Evenly }
Length_Kind :: enum { Auto,Pixels,Percent }
Length :: struct { kind:Length_Kind,value:f32 }
Insets :: struct { top,right,bottom,left:f32 }
Layout :: struct {
    width,height,min_width,min_height,max_width,max_height:Length,
    padding,margin:Insets,gap:Vec2,grow:f32,shrink:f32,aspect_ratio:f32,
    align:Align,justify:Justify,wrap,absolute,no_shrink:bool,position,anchor:Vec2,
    columns:u32,cell_size:Vec2,
}
Theme :: struct { canvas,panel,control,hover,active,text,muted,disabled,accent,selection:Color,font:Font_Id,font_size,row_height,padding,radius:f32 }
/// Descriptor strings and children are borrowed only during frame reconciliation.
Descriptor :: struct {
    key:u64,kind:Widget_Kind,layout:Layout,children:[]Descriptor,
    text,placeholder:string,font:Font_Id,font_size:f32,
    state:State_Id,action,payload:u64,
    minimum,maximum,step,value:f32,
    texture:Texture_Id,uv:Rect,
    layer:Layer,hidden,disabled,focusable,multiline,clip_children,selected,expanded,has_children,draggable:bool,
    fixed_bounds:Rect,has_fixed_bounds:bool,
    background,foreground:Color,has_background,has_foreground:bool,
    syntax:[]Text_Run,
    options:[]string,dock:^Dock_Tree,dock_tabs:[]Dock_Tab,
}
Modifier :: enum { Shift,Control,Alt,Super }
Modifiers :: bit_set[Modifier]
Pointer_Button :: enum { Left,Right,Middle }
Key :: enum { None,Tab,Enter,Escape,Backspace,Delete,Left,Right,Up,Down,Home,End,Page_Up,Page_Down,A,B,C,D,E,F,G,H,I,J,K,L,M,N,O,P,Q,R,S,T,U,V,W,X,Y,Z,Num0,Num1,Num2,Num3,Num4,Num5,Num6,Num7,Num8,Num9,Comma,Period,Slash,Backslash,Minus,Equal,Left_Bracket,Right_Bracket,Quote,Semicolon,Backtick,Space }
Pointer_Move :: struct { position:Vec2 }
Pointer_Down :: struct { position:Vec2,button:Pointer_Button,modifiers:Modifiers,clicks:u32 }
Pointer_Up :: struct { position:Vec2,button:Pointer_Button,modifiers:Modifiers }
Scroll :: struct { position,delta:Vec2 }
Key_Down :: struct { key:Key,modifiers:Modifiers,repeat:bool }
Key_Up :: struct { key:Key,modifiers:Modifiers }
Text_Commit :: struct { text:string }
IME_Preedit :: struct { text:string,cursor,selection_end:int }
Window_Focus :: struct { focused:bool }
Input_Event :: union { Pointer_Move,Pointer_Down,Pointer_Up,Scroll,Key_Down,Key_Up,Text_Commit,IME_Preedit,Window_Focus }
Input :: struct { events:[]Input_Event,time:f64,pixel_scale:f32 }
Cursor :: enum { Default,Hand,Text,Horizontal_Resize,Vertical_Resize,Grab,Grabbing }
IME_Request :: struct { active:bool,cursor:Rect,selection_start,selection_end:int }
Frame_Error :: enum { None,Invalid_Descriptor,Duplicate_Key,Invalid_State,Invalid_Layout,Layout_Unavailable,Font_Unavailable,Closed }
Frame_Result :: struct { consumed_pointer,consumed_keyboard,captured_pointer,captured_keyboard:bool,cursor:Cursor,ime:IME_Request,error:Frame_Error }
Click_Action :: struct { node:Node_Id,action,payload:u64,modifiers:Modifiers,clicks:u32,button:Pointer_Button }
Text_Action :: struct { node:Node_Id,action,payload:u64,state:State_Id,submitted:bool,text:string }
Number_Action :: struct { node:Node_Id,action,payload:u64,state:State_Id,value:f32,started,finished:bool }
Toggle_Action :: struct { node:Node_Id,action,payload:u64,state:State_Id,value:bool }
Selection_Action :: struct { node:Node_Id,action,payload:u64,index:int }
Expand_Action :: struct { node:Node_Id,action,payload:u64,expanded:bool }
Pointer_Action :: struct { node:Node_Id,action,payload:u64,position,delta:Vec2,button:Pointer_Button,pressed,released:bool }
Key_Action :: struct { node:Node_Id,action,payload:u64,key:Key,modifiers:Modifiers,repeat:bool }
Scroll_Action :: struct { node:Node_Id,action,payload:u64,offset:Vec2 }
Dismiss_Reason :: enum { Outside,Escape }
Dismiss_Action :: struct { node:Node_Id,action,payload:u64,reason:Dismiss_Reason }
Clipboard_Provider :: struct { state:rawptr,read:proc(rawptr)->string,write:proc(rawptr,string) }
Action :: union { Dismiss_Action, Click_Action,Text_Action,Number_Action,Toggle_Action,Selection_Action,Expand_Action,Pointer_Action,Key_Action,Scroll_Action,Dock_Action }
@(private="package")
Text_Snapshot :: struct { text:string,cursor,anchor:int }
@(private="package")
State_Cell :: struct { value:Value,dirty:bool }
@(private="package")
Node :: struct {
    id:Node_Id,descriptor:Descriptor,parent:Node_Id,children:[dynamic]Node_Id,
    bounds,clip,content:Rect,state:map[u32]State_Cell,
    cursor,anchor:int,undo,redo:[dynamic]Text_Snapshot,preedit:string,preedit_cursor,preedit_end:int,text_offset:Vec2,
    field_text:string,numeric_invalid,text_dirty:bool,scroll:Vec2,seen,retained_until:u64,mounted,input_disabled:bool,
}
/// Stationary owner of reconciled nodes, typed state, capture, focus and frame outputs.
Context :: struct {
    nodes:map[u64]^Node,root,focused,captured,hovered:Node_Id,
    actions:[dynamic]Action,action_snapshots:[dynamic]string,commands:[dynamic]Draw_Command,order:[dynamic]Node_Id,
    generation,frame_index:u64,theme:Theme,fonts:Font_Provider,
    pointer:Vec2,capture_button:Pointer_Button,capture_start:Vec2,capture_value:f32,capture_modifiers:Modifiers,capture_clicks:u32,
    popup,modal,focus_before_modal,focus_before_popup:Node_Id,clipboard:string,clipboard_provider:Clipboard_Provider,
    dock_source,dock_split:Dock_Id,dock_tab:Tab_Id,dock_bounds:Rect,dock_dragging,scroll_drag,scroll_drag_horizontal,capture_draggable,capture_dragged,capture_text_lines:bool,capture_line_start,capture_line_end:int,
    logical_size:Vec2,window_focused,closed,initialized:bool,allocator:mem.Allocator,
}
