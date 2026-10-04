//! Stable dock leaves preserve the exact dragged tab identity through structural changes.
package ui
import "core:mem"

Dock_Id :: distinct u64
Tab_Id :: distinct u64
Dock_Kind :: enum { Empty,Leaf,Split }
Split_Direction :: enum { Horizontal,Vertical }
Dock_Zone :: enum { Center,Left,Right,Top,Bottom }
Dock_Node :: struct { id:Dock_Id,kind:Dock_Kind,direction:Split_Direction,ratio:f32,children:[2]Dock_Id,tabs:[dynamic]Tab_Id,active:int }
Dock_Tab :: struct { tab:Tab_Id,label:string }
Dock_Tree :: struct { nodes:map[Dock_Id]^Dock_Node,root:Dock_Id,next_id:u64,allocator:mem.Allocator }
Dock_Action_Kind :: enum { Activate,Move,Resize,Close,Insert }
Dock_Action :: struct { kind:Dock_Action_Kind,source,target:Dock_Id,tab:Tab_Id,zone:Dock_Zone,ratio:f32,index:int }
Dock_Bounds :: struct { node:Dock_Id,bounds,content,tab_bar:Rect,active:Tab_Id,has_active:bool }
Dock_Error :: enum { None,Invalid_Id,Invalid_Tab,Invalid_Ratio,Invalid_Snapshot,Duplicate_Tab,Not_Leaf }
