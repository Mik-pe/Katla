//! Logical geometry helpers share drawing, hit testing and pointer-capture ranges.
package ui
import "core:math"
rect_contains :: #force_inline proc(rect:Rect,point:Vec2)->bool { return rect.width>0 && rect.height>0 && point[0]>=rect.x && point[1]>=rect.y && point[0]<rect.x+rect.width && point[1]<rect.y+rect.height }
rect_intersection :: #force_inline proc(a,b:Rect)->Rect { x,y:=max(a.x,b.x),max(a.y,b.y); return {x,y,max(f32(0),min(a.x+a.width,b.x+b.width)-x),max(f32(0),min(a.y+a.height,b.y+b.height)-y)} }
rect_inset :: #force_inline proc(rect:Rect,padding:f32)->Rect { return {rect.x+padding,rect.y+padding,max(f32(0),rect.width-2*padding),max(f32(0),rect.height-2*padding)} }
@(private="package")
finite :: #force_inline proc(value:f32)->bool { return !math.is_nan(value) && !math.is_inf(value) }
@(private="package")
rect_valid :: #force_inline proc(rect:Rect)->bool { return finite(rect.x) && finite(rect.y) && finite(rect.width) && finite(rect.height) && rect.width>=0 && rect.height>=0 }
