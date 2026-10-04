//! Portable resource creation inputs and native submission identities.
package gfx

/// Native failures and ownership errors returned by GPU adapters.
Gpu_Error :: enum { None, No_Device, Unsupported, Allocation_Failed, Invalid_Shader, Shader_Compile_Failed, Invalid_Resource, Invalid_Range, Busy, Invalid_Graph, Native_Failure }
/// The buffer class for one shader slot; native reflection supplies its byte layout.
Shader_Buffer :: struct { slot:u32, usage:Buffer_Usage }
/// Equivalent target binaries are supplied before frame encoding; no runtime compilation fallback.
Compute_Desc :: struct { entry,metal_source:string, spirv:[]u32, local_size:[3]u32, buffers:[]Shader_Buffer }
/// Identifies one accepted submission and its exact native frame owner.
Submission :: struct { owner:rawptr, token:Frame_Token, id:u64 }
