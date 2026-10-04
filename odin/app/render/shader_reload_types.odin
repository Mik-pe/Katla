//! Source watching belongs to the application; generic compilation remains in the existing bounded worker.
package render

import shader "../../gfx/shader"
import "core:mem"
import "core:time"

/// Rejected replacements preserve the application's previously accepted native family.
Shader_Reload_Error :: enum { None,Invalid_Config,Source,Compiler,Compile,Prepare,Busy,Closed }
/// One confined canonical source path with exact selected entries and compile overrides.
Shader_Reload_Module :: struct { path:string,selections:[]shader.Selection,constants:[]shader.Constant }
/// Preparation owns every new native pipeline before infallible publication changes any live owner.
/// Publication updates graph references and returns the previous complete family for destruction.
/// Compiled artifacts are borrowed during prepare; future descriptors must own deep snapshots.
Shader_Reload_Publisher :: struct {
    state:rawptr,
    prepare:proc(rawptr,[]shader.Compiled)->(rawptr,Shader_Reload_Error),
    publish:proc(rawptr,rawptr)->rawptr,
    destroy:proc(rawptr,rawptr),
}
/// Every module and consumer in a family publishes together after complete native preparation.
Shader_Reload_Family :: struct { name:string,modules:[]Shader_Reload_Module,options:[]byte,publisher:Shader_Reload_Publisher }
/// A stationary service is polled once per monotonically increasing owner frame, before acquisition.
Shader_Reload_Service :: struct {
    compiler:^shader.Compiler,
    worker:shader.Service,
    root:string,
    families:[dynamic]Shader_Reload_Family_State,
    next_key,last_frame:u64,
    compiler_digest:[32]byte,
    compiler_size:i64,
    compiler_inode:u128,
    compiler_device:u64,
    compiler_modified,compiler_created:time.Time,
    compiler_seen,polled:bool,
    allocator:mem.Allocator,
}
/// Per-poll family counts; failed increments only when the reported failure changes.
Shader_Reload_Status :: struct { changed,pending,published,failed:int,error:Shader_Reload_Error }
@(private="package")
Shader_Reload_Module_State :: struct { config:Shader_Reload_Module,key,revision:u64,compiled:shader.Compiled,ready:bool }
@(private="package")
Shader_Reload_Family_State :: struct {
    name:string,options:[]byte,publisher:Shader_Reload_Publisher,
    modules:[]Shader_Reload_Module_State,
    observed:[32]byte,seen,pending:bool,
    last_error:Shader_Reload_Error,
}
