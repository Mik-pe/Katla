//! UI commands carry logical geometry and opaque application texture identities.
package ui

Vec2 :: [2]f32
Color :: [4]f32
Rect :: struct { x,y,width,height:f32 }
/// The application owns the mapping to its native texture resources.
Texture_Id :: distinct u64
Font_Id :: distinct u64
Vertex :: struct { position,uv:Vec2,color:Color }
Rect_Draw :: struct { bounds,clip:Rect,color:Color,radius:f32 }
Image_Draw :: struct { texture:Texture_Id,bounds,uv,clip:Rect,tint:Color }
Text_Run :: struct { start,end:int,color:Color }
Text_Draw :: struct { font:Font_Id,text:string,position:Vec2,size,wrap_width:f32,clip:Rect,color:Color,runs:[]Text_Run }
Mesh_Draw :: struct { vertices:[]Vertex,indices:[]u32,texture:Texture_Id,clip:Rect }
Draw_Command :: union { Rect_Draw,Image_Draw,Text_Draw,Mesh_Draw }
/// Commands and nested slices remain immutable until the context's next frame or destruction.
Draw_List :: struct { commands:[]Draw_Command,logical_size:Vec2,pixel_scale:f32 }
/// The same shaped layout must implement measurement, drawing, caret positions and pointer hits.
Font_Provider :: struct {
    state:rawptr,
    measure:proc(rawptr,Font_Id,string,f32,f32)->Vec2,
    caret:proc(rawptr,Font_Id,string,f32,f32,int)->Vec2,
    hit_test:proc(rawptr,Font_Id,string,f32,f32,Vec2)->int,
    navigate:proc(rawptr,Font_Id,string,f32,f32,int,int)->int,
    grapheme:proc(rawptr,string,int,int)->int,
}
