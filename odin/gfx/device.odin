//! Portable resource creation inputs and native submission identities.
package gfx

/// Native failures and ownership errors returned by GPU adapters.
Gpu_Error :: enum { None, No_Device, Unsupported, Allocation_Failed, Invalid_Shader, Shader_Compile_Failed, Invalid_Resource, Invalid_Range, Busy, Invalid_Graph, Native_Failure }
/// The buffer class for one shader slot; native reflection supplies its byte layout.
Shader_Buffer :: struct { group,slot:u32, metal_index,size_index:i32, usage:Buffer_Usage, mode:Access_Mode, minimum_size:u64 }
/// Equivalent target binaries are supplied before frame encoding; no runtime compilation fallback.
Compute_Desc :: struct { entry,metal_entry,metal_source:string, spirv:[]u32, local_size:[3]u32, buffers:[]Shader_Buffer, images:[]Shader_Compute_Image, samplers:[]Shader_Compute_Sampler, runtime_sizes_index:i32, runtime_sizes_words:u32 }
/// Identifies one accepted submission and its exact native frame owner.
Submission :: struct { owner:rawptr, token:Frame_Token, id:u64 }

/// Compute-stage image mapping preserves the selected compiler's native index.
Shader_Compute_Image :: struct { group,slot:u32, metal_index:i32, usage:Texture_Usage, arrayed,depth:bool, dimension:Texture_Dimension, sample_type:Texture_Sample_Type, storage_format:Texture_Format, mode:Access_Mode }
Shader_Compute_Sampler :: struct { group,slot:u32, metal_index:i32, comparison:bool }
