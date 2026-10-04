//! Readback identifies an exact accepted image source and owns completed bytes.
package gfx

import "core:mem"

/// Snapshot identity published only after native submission accepts its export.
Texture_Source :: struct { owner:rawptr, resource:Image_Id, texture:Texture_Handle, submission:Submission, generation:u64, desc:Texture_Desc }
/// A ticket is meaningful only in its stationary native readback registry.
Readback_Kind :: struct {}
Readback_Ticket :: Handle(Readback_Kind)
/// Completed readback transfers its byte allocation to the caller exactly once.
Readback_Data :: struct { source:Texture_Source, region:Image_Region, row_pitch:u64, bytes:[]byte, allocator:mem.Allocator, image_pitch:u64 }
/// Releases completed pixels with their captured allocator.
readback_data_destroy :: proc(data:^Readback_Data) { delete(data.bytes,data.allocator); data^={} }
/// Presentation outcomes distinguish queue acceptance from the surface result.
Surface_Result :: enum { Presented, Unavailable, Recreate, Fatal }
/// Accepted GPU work remains committed even if presenting its image fails.
Present_Outcome :: struct { submission:Submission, surface:Surface_Result }
/// Window handles stay outside scene policy and never enter shader interfaces.
Surface_Kind :: enum { Native,Xlib,Wayland,Win32 }
Surface_Desc :: struct { view,display:rawptr, width,height:u32,kind:Surface_Kind }
/// A borrowed presentation image belongs to one native surface generation.
Surface_Frame :: struct { owner:rawptr, generation:u64, texture:Texture_Handle, width,height:u32 }
