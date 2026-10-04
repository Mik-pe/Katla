//! Complete shader families publish only after every changed module and native pipeline prepares successfully.
package render

import shader "../../gfx/shader"
import "core:crypto/sha2"
import "core:encoding/json"
import "core:os"
import "core:path/filepath"
import "core:strings"
import "core:log"

/// Initializes a stationary owner and one existing asynchronous compiler worker.
shader_reload_init :: proc(service:^Shader_Reload_Service,compiler:^shader.Compiler,root:string,allocator:=context.allocator)->Shader_Reload_Error {
    if service.compiler!=nil || compiler==nil || compiler.executable=="" || root=="" { return .Invalid_Config }
    absolute,path_error:=filepath.abs(root,allocator);if path_error!=nil { return .Invalid_Config }
    info,error:=os.stat(absolute,allocator);if error!=nil { delete(absolute,allocator);return .Source };defer os.file_info_delete(info,allocator)
    if info.type!=.Directory { delete(absolute,allocator);return .Source }
    service.compiler=compiler;service.root=absolute;service.allocator=allocator;service.families=make([dynamic]Shader_Reload_Family_State,allocator)
    if shader.service_init(&service.worker,compiler,256,allocator)!=.None { delete(service.root,allocator);delete(service.families);service^={};return .Compiler }
    return .None
}
/// Registers immutable source/options and mandatory atomic native publication callbacks.
shader_reload_register :: proc(service:^Shader_Reload_Service,config:Shader_Reload_Family)->(int,Shader_Reload_Error) {
    if service.compiler==nil { return -1,.Closed }
    if config.name=="" || len(config.modules)==0 || len(config.modules)>32 || config.publisher.prepare==nil || config.publisher.publish==nil || config.publisher.destroy==nil { return -1,.Invalid_Config }
    count:=len(config.modules);for family in service.families { count+=len(family.modules);if family.name==config.name { return -1,.Invalid_Config } };if count>128 { return -1,.Invalid_Config }
    for module in config.modules {
        if module.path=="" || len(module.selections)==0 || len(module.selections)>16 || !shader_reload_constants_valid(module.constants) { return -1,.Invalid_Config }
        for selected,index in module.selections {
            if selected.name=="" || selected.stage<.Vertex || selected.stage>.Compute { return -1,.Invalid_Config }
            for previous in module.selections[:index] { if previous==selected { return -1,.Invalid_Config } }
        }
    }
    family:=Shader_Reload_Family_State{name=strings.clone(config.name,service.allocator),publisher=config.publisher,modules=make([]Shader_Reload_Module_State,len(config.modules),service.allocator),options=make([]byte,len(config.options),service.allocator)}
    copy(family.options,config.options)
    for module,index in config.modules {
        service.next_key+=1;item:=&family.modules[index];item.key=service.next_key
        item.config.path=strings.clone(module.path,service.allocator);item.config.selections=make([]shader.Selection,len(module.selections),service.allocator);item.config.constants=make([]shader.Constant,len(module.constants),service.allocator)
        for selected,i in module.selections { item.config.selections[i]={strings.clone(selected.name,service.allocator),selected.stage} }
        for constant,i in module.constants { item.config.constants[i]={strings.clone(constant.name,service.allocator),constant.value} }
    }
    append(&service.families,family);return len(service.families)-1,.None
}
/// Changes native preparation options atomically; shader cache reuse follows unchanged compiler requests.
shader_reload_set_options :: proc(service:^Shader_Reload_Service,family_index:int,options:[]byte)->Shader_Reload_Error {
    if service.compiler==nil { return .Closed };if family_index<0 || family_index>=len(service.families) { return .Invalid_Config }
    family:=&service.families[family_index]
    if len(options)==len(family.options) && mem_compare_options(options,family.options) { return .None }
    owned:=make([]byte,len(options),service.allocator);copy(owned,options)
    shader_reload_cancel(service,family);delete(family.options,service.allocator);family.options=owned;family.seen=false;return .None
}
@(private="package")
mem_compare_options :: proc(first,second:[]byte)->bool { for value,index in first { if value!=second[index] { return false } };return true }
@(private="package")
shader_reload_constants_valid :: proc(constants:[]shader.Constant)->bool {
    if len(constants)>1024 { return false }
    for value,index in constants {
        if value.name=="" || !(value.value>= -max(f64) && value.value<=max(f64)) { return false }
        for previous in constants[:index] { if previous.name==value.name { return false } }
    }
    return true
}
/// Changes compile overrides without retaining caller-owned strings; invalid overrides preserve the live request.
shader_reload_set_constants :: proc(service:^Shader_Reload_Service,family_index,module_index:int,constants:[]shader.Constant)->Shader_Reload_Error {
    if service.compiler==nil { return .Closed };if family_index<0 || family_index>=len(service.families) || module_index<0 || module_index>=len(service.families[family_index].modules) || !shader_reload_constants_valid(constants) { return .Invalid_Config }
    family:=&service.families[family_index];module:=&family.modules[module_index]
    same:=len(constants)==len(module.config.constants)
    if same {
        for value in constants {
            found:=false;for current in module.config.constants { if value==current { found=true;break } };if !found { same=false;break }
        }
        if same { return .None }
    }
    owned:=make([]shader.Constant,len(constants),service.allocator);for value,index in constants { owned[index]={strings.clone(value.name,service.allocator),value.value} }
    shader_reload_cancel(service,family)
    for value in module.config.constants { delete(value.name,service.allocator) };delete(module.config.constants,service.allocator);module.config.constants=owned;family.seen=false;return .None
}
@(private="package")
shader_reload_cancel :: proc(service:^Shader_Reload_Service,family:^Shader_Reload_Family_State) {
    for &module in family.modules { shader.service_cancel(&service.worker,module.key);shader.compiled_destroy(&module.compiled);module.ready=false;module.revision=0 }
    family.pending=false
}
@(private="package")
shader_reload_failure :: proc(family:^Shader_Reload_Family_State,error:Shader_Reload_Error,status:^Shader_Reload_Status) {
    if family.last_error!=error { status.failed+=1;log.warn("Shader family reload rejected; retaining accepted native pipelines",family.name,error) }
    family.last_error=error;status.error=error
}
/// Watches a coherent source snapshot once per owner frame and publishes complete ready families.
/// The caller invokes this before frame acquisition; publication cannot alter partially authored frames.
shader_reload_poll :: proc(service:^Shader_Reload_Service,owner_frame:u64)->Shader_Reload_Status {
    if service.compiler==nil { return {error=.Closed} }
    if service.polled && owner_frame<=service.last_frame { return {} };service.last_frame=owner_frame;service.polled=true
    status:Shader_Reload_Status
    source:=Shader_Reload_Read{root=service.root,files=make(map[string][]byte,service.allocator)};defer shader_reload_read_destroy(&source,service.allocator)
    compiler_ready:=shader_reload_compiler_identity(service)
    for &family in service.families {
        if !compiler_ready { shader_reload_cancel(service,&family);family.seen=false;shader_reload_failure(&family,.Compiler,&status);continue }
        expanded:=make([]string,len(family.modules),service.allocator)
        valid:=true;digest:sha2.Context_256;sha2.init_256(&digest);sha2.update(&digest,service.compiler_digest[:]);sha2.update(&digest,family.options)
        for module,index in family.modules {
            error:shader.Error;expanded[index],error=shader.resolve_source({&source,shader_reload_read},module.config.path,service.allocator)
            if error!=.None { valid=false;break }
            options,marshal_error:=json.marshal(module.config,opt={spec=.JSON,use_enum_names=true,sort_maps_by_key=true},allocator=service.allocator)
            if marshal_error!=nil { valid=false;delete(options,service.allocator);break }
            sha2.update(&digest,options);delete(options,service.allocator);sha2.update(&digest,transmute([]byte)expanded[index])
        }
        fingerprint:[32]byte;sha2.final(&digest,fingerprint[:])
        if !valid { shader_reload_cancel(service,&family);family.seen=false;shader_reload_failure(&family,.Source,&status) }
        else if !family.seen || fingerprint!=family.observed {
            shader_reload_cancel(service,&family);accepted:=true
            for &module,index in family.modules {
                revision,error:=shader.service_submit(&service.worker,module.key,expanded[index],module.config.selections,module.config.constants)
                if error!=.None { accepted=false;break };module.revision=revision
            }
            if accepted { family.observed=fingerprint;family.seen=true;family.pending=true;family.last_error=.None;status.changed+=1 }
            else { shader_reload_cancel(service,&family);status.error=.Busy }
        }
        for text in expanded { delete(text,service.allocator) };delete(expanded,service.allocator)
    }
    for {
        replacement,present:=shader.service_take_result(&service.worker);if !present { break }
        handled:=false
        for &family in service.families {
            for &module in family.modules {
                if module.key!=replacement.key || module.revision!=replacement.revision || !family.pending { continue }
                handled=true
                if replacement.error!=.None {
                    log.warn("Shader compile diagnostic",family.name,replacement.error,replacement.compiled.message)
                    shader_reload_cancel(service,&family);shader_reload_failure(&family,.Compile,&status)
                } else { module.compiled=replacement.compiled;replacement.compiled={};module.ready=true }
                break
            }
            if handled { break }
        }
        shader.replacement_destroy(&replacement)
    }
    for &family in service.families {
        if !family.pending { continue };ready:=true;for module in family.modules { if !module.ready { ready=false;break } }
        if !ready { status.pending+=1;continue }
        artifacts:=make([]shader.Compiled,len(family.modules),service.allocator);for module,index in family.modules { artifacts[index]=module.compiled }
        candidate,error:=family.publisher.prepare(family.publisher.state,artifacts);delete(artifacts,service.allocator)
        if error==.Busy { if candidate!=nil { family.publisher.destroy(family.publisher.state,candidate) };status.pending+=1;continue }
        if error!=.None || candidate==nil {
            if candidate!=nil { family.publisher.destroy(family.publisher.state,candidate) }
            shader_reload_failure(&family,.Prepare,&status)
        } else {
            previous:=family.publisher.publish(family.publisher.state,candidate)
            if previous!=nil { family.publisher.destroy(family.publisher.state,previous) }
            status.published+=1;family.last_error=.None;log.info("Shader family reloaded",family.name)
        }
        shader_reload_cancel(service,&family)
    }
    return status
}
/// Joins the existing worker before releasing owned source options and unpublished candidates.
/// Live native pipeline families remain owned by their application consumers.
shader_reload_destroy :: proc(service:^Shader_Reload_Service)->Shader_Reload_Error {
    if service.compiler==nil { return .Closed }
    if shader.service_destroy(&service.worker)!=.None { return .Busy }
    for &family in service.families {
        for &module in family.modules {
            shader.compiled_destroy(&module.compiled);delete(module.config.path,service.allocator)
            for selected in module.config.selections { delete(selected.name,service.allocator) };delete(module.config.selections,service.allocator)
            for constant in module.config.constants { delete(constant.name,service.allocator) };delete(module.config.constants,service.allocator)
        }
        delete(family.modules,service.allocator);delete(family.name,service.allocator);delete(family.options,service.allocator)
    }
    delete(service.families);delete(service.root,service.allocator);service^={};return .None
}
