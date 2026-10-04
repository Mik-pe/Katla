//! Committed viewport pixels keep their exact object-ID map independently of later frames.
package render

import gfx "../../gfx"
import ecs "../../ecs"
import "core:mem"

/// Encoded zero denotes background; nonzero unmapped values may identify editor overlays.
Picking_Entry :: struct { encoded:u32, entity:ecs.Entity_Id }
/// Main-thread capture metadata travels with both queued image copies.
Picking_Metadata :: struct { frame,serial:u64, capture_time_ns:i64, width,height:u32, pointer:[2]i32, has_pointer:bool }
/// A published snapshot owns pixels, mapping and exact submission provenance.
Picking_Snapshot :: struct {
    metadata:Picking_Metadata,
    submission:gfx.Submission,
    color_source,id_source:gfx.Texture_Source,
    color,id_pixels:gfx.Readback_Data,
    entries:[]Picking_Entry,
    allocator:mem.Allocator,
}
/// A pending capture owns immutable metadata and map as soon as its source copies are queued.
Picking_Capture :: struct {
    metadata:Picking_Metadata,
    submission:gfx.Submission,
    color_source,id_source:gfx.Texture_Source,
    color_ticket,id_ticket:gfx.Readback_Ticket,
    color,id_pixels:gfx.Readback_Data,
    entries:[]Picking_Entry,
    allocator:mem.Allocator,
}
/// Readback operations remain generic GPU ownership rather than selection policy.
Picking_Ops :: struct($Renderer:typeid) {
    source:proc(^Renderer,gfx.Submission,gfx.Image_Id)->(gfx.Texture_Source,gfx.Gpu_Error),
    queue:proc(^Renderer,gfx.Texture_Source,gfx.Image_Region)->(gfx.Readback_Ticket,gfx.Gpu_Error),
    poll:proc(^Renderer,gfx.Readback_Ticket)->(gfx.Readback_Data,bool,gfx.Gpu_Error),
    destroy:proc(^Renderer,gfx.Readback_Ticket)->gfx.Gpu_Error,
}
/// Resolves a raw committed sample without consulting a newer scene map.
Picking_Sample :: struct { encoded:u32, entity:ecs.Entity_Id, mapped:bool }
