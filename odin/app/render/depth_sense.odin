//! Camera depth sense is explicit application state, independent of backend or matrix inspection.
package render

import gfx "../../gfx"

Depth_Sense :: enum { Forward, Reverse }
/// Invalid transport values are rejected before graph declarations or frame uploads.
depth_sense_valid :: proc(sense:Depth_Sense)->bool { return sense==.Forward || sense==.Reverse }
/// Untouched pixels retain the far depth of the selected camera convention.
depth_clear :: proc(sense:Depth_Sense)->f32 { return 0 if sense==.Reverse else 1 }
/// Copies only camera-depth comparisons; independent forward shadow descriptors never use this helper.
depth_descriptor :: proc(descriptor:gfx.Graphics_Desc,sense:Depth_Sense)->gfx.Graphics_Desc {
    result:=descriptor
    if sense==.Reverse {
        switch result.depth.compare {
        case .Less: result.depth.compare=.Greater
        case .Less_Equal: result.depth.compare=.Greater_Equal
        case .Greater: result.depth.compare=.Less
        case .Greater_Equal: result.depth.compare=.Less_Equal
        case .Never,.Equal,.Not_Equal,.Always:
        }
    }
    return result
}
