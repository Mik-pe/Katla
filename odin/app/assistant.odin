//! The application owns a stationary assistant; its producer receives only the agent mailbox.
package app

import llm "../agent/llm"
import editor "../editor"
import "core:mem"
import "core:strings"

/// Terminal failures require an explicit new conversation before another request.
Assistant_State :: enum { Disabled, Idle, Running, Cancelling, Completed, Failed }
/// Main-thread service state. Join it before releasing the authoring mailbox.
Assistant :: struct {
    config:llm.Config,
    runtime:llm.Runtime,
    conversation:llm.Conversation,
    job:llm.Job,
    mailbox:^editor.Agent_Harness,
    schemas,system_prompt:string,
    output:[dynamic]byte,
    state:Assistant_State,
    error:llm.Error,
    revision:u64,
    initialized,context_changed:bool,
    allocator:mem.Allocator,
}
/// Captures the explicit provider choice and only the tools supported by this consumer.
assistant_init :: proc(owner:^Assistant,mailbox:^editor.Agent_Harness,config:llm.Config,schemas,system_prompt:string,allocator:=context.allocator)->llm.Error {
    if owner.initialized || mailbox==nil { return .Config }
    owner.initialized=true; owner.allocator=allocator; owner.mailbox=mailbox
    owner.config=llm.config_clone(config,allocator)
    owner.schemas=strings.clone(schemas,allocator); owner.system_prompt=strings.clone(system_prompt,allocator)
    owner.output=make([dynamic]byte,allocator); owner.revision=1
    if config.provider==.Disabled { owner.state=.Disabled; owner.error=.Disabled; return .Disabled }
    error:=llm.runtime_init(&owner.runtime,owner.config,allocator)
    if error==.None { error=llm.conversation_init(&owner.conversation,&owner.runtime,&owner.config,mailbox,schemas,system_prompt,allocator) }
    owner.error=error; owner.state=.Idle if error==.None else .Failed
    return error
}
/// Empty paths disable the service; configuration files are used only when explicitly selected.
assistant_init_path :: proc(owner:^Assistant,mailbox:^editor.Agent_Harness,path,schemas,system_prompt:string,allocator:=context.allocator)->llm.Error {
    config:=llm.config_default(allocator)
    if path!="" {
        llm.config_destroy(&config)
        error:llm.Error; config,error=llm.config_load(path,allocator)
        if error!=.None {
            config=llm.config_default(allocator); defer llm.config_destroy(&config)
            init_error:=assistant_init(owner,mailbox,config,schemas,system_prompt,allocator)
            if init_error!=.Disabled { return init_error }
            owner.state=.Failed; owner.error=error; return error
        }
    }
    defer llm.config_destroy(&config)
    return assistant_init(owner,mailbox,config,schemas,system_prompt,allocator)
}
/// Starts one turn without handing the worker an application or world pointer.
assistant_start :: proc(owner:^Assistant,prompt:string)->llm.Error {
    if !owner.initialized { return .Config }
    if owner.state==.Disabled { return .Disabled }
    if owner.job.worker!=nil || owner.conversation.pending!=0 { return .Busy }
    if owner.state==.Failed { return owner.error }
    if strings.trim_space(prompt)=="" { return .Config }
    if owner.context_changed {
        error:=llm.conversation_context_snapshot(&owner.conversation,owner.system_prompt)
        if error!=.None { return error }
        owner.context_changed=false
    }
    error:=llm.job_start(&owner.job,&owner.conversation,prompt,64,owner.allocator)
    if error!=.None { return error }
    clear(&owner.output); owner.state=.Running; owner.error=.None; owner.revision+=1
    return .None
}
@(private="package")
assistant_drain :: proc(owner:^Assistant)->bool {
    changed:=false
    for {
        chunk,present:=llm.job_poll_text(&owner.job); if !present { break }
        if len(owner.output)+len(chunk)>llm.MAX_TEXT_BYTES {
            llm.job_cancel(&owner.job); owner.error=.Limit; owner.state=.Cancelling
        } else { append(&owner.output,..transmute([]byte)chunk) }
        delete(chunk,owner.allocator); changed=true
    }
    return changed
}
/// Drains bounded stream text and terminal results on the application thread.
assistant_poll :: proc(owner:^Assistant)->bool {
    if !owner.initialized { return false }
    changed:=false
    if owner.job.worker!=nil {
        changed=assistant_drain(owner)
        response,error,done:=llm.job_poll(&owner.job)
        if done {
            assistant_drain(owner)
            if owner.error==.Limit { error=.Limit }
            if len(owner.output)==0 && len(response.text)>0 { append(&owner.output,..response.text[:]) }
            llm.response_destroy(&response); llm.job_destroy(&owner.job)
            owner.error=error; owner.state=.Completed if error==.None else .Failed; changed=true
        }
    } else if owner.conversation.pending!=0 {
        changed=llm.conversation_reap(&owner.conversation)
    }
    if changed { owner.revision+=1 }
    return changed
}
/// Requests cancellation; accepted editor operations remain in the shared undo history.
assistant_cancel :: proc(owner:^Assistant) {
    if owner.job.worker==nil { return }
    llm.job_cancel(&owner.job); owner.state=.Cancelling; owner.revision+=1
}
/// Captures changed scene context for the next turn without touching the active worker or prior messages.
assistant_context_refresh :: proc(owner:^Assistant,system_prompt:string)->llm.Error {
    if !owner.initialized { return .Config }
    if len(system_prompt)>llm.MAX_TEXT_BYTES { return .Limit }
    if owner.system_prompt==system_prompt { return .None }
    updated:=strings.clone(system_prompt,owner.allocator)
    delete(owner.system_prompt,owner.allocator); owner.system_prompt=updated
    owner.context_changed=true; owner.revision+=1
    return .None
}
/// Explicitly clears provider history after failure or when the user chooses a new conversation.
assistant_reset :: proc(owner:^Assistant)->llm.Error {
    if !owner.initialized { return .Config }
    if owner.job.worker!=nil || !llm.conversation_reap(&owner.conversation) { return .Busy }
    if owner.config.provider==.Disabled { return .Disabled }
    if !owner.runtime.initialized { return owner.error }
    llm.conversation_destroy(&owner.conversation)
    error:=llm.conversation_init(&owner.conversation,&owner.runtime,&owner.config,owner.mailbox,owner.schemas,owner.system_prompt,owner.allocator)
    owner.error=error; owner.state=.Idle if error==.None else .Failed; clear(&owner.output); owner.revision+=1
    if error==.None { owner.context_changed=false }
    return error
}
/// Cancels and joins before releasing owned provider state; a late executing ticket stays mailbox-owned.
assistant_destroy :: proc(owner:^Assistant)->u64 {
    llm.job_destroy(&owner.job)
    ticket:=llm.conversation_destroy(&owner.conversation)
    llm.runtime_destroy(&owner.runtime); llm.config_destroy(&owner.config)
    delete(owner.schemas,owner.allocator); delete(owner.system_prompt,owner.allocator); delete(owner.output)
    owner^={}; return ticket
}
/// Supplies safe user-facing status without provider bodies, endpoints or credentials.
assistant_status :: proc(owner:^Assistant)->string {
    switch owner.state {
    case .Disabled: return "Choose an LLM configuration to enable the assistant."
    case .Idle: return "Ready"
    case .Running: return "Working…"
    case .Cancelling: return "Cancelling… Accepted edits remain in Undo."
    case .Completed: return "Completed"
    case .Failed:
        switch owner.error {
        case .Cancelled: return "Cancelled. Start a new conversation to continue."
        case .Credentials: return "Credentials unavailable. Check the configured key reference."
        case .Timeout: return "Request timed out. Start a new conversation to retry."
        case .Network: return "Connection failed. Check the provider and start a new conversation."
        case .Rate_Limited: return "Provider rate limit reached. Wait, then start a new conversation."
        case .HTTP: return "Provider rejected the request. Check settings before starting again."
        case .Config: return "Invalid configuration. Check the selected configuration file."
        case .Tool: return "Editor tool failed. Start a new conversation to continue."
        case .Limit,.Conversation_Limit: return "Conversation limit reached. Start a new conversation."
        case .Refused: return "Provider declined this request. Start a new conversation."
        case .Protocol,.Truncated: return "Incomplete provider response. Start a new conversation to retry."
        case .Disabled: return "Choose an LLM configuration to enable the assistant."
        case .None,.Busy: return "Request unavailable. Start a new conversation."
        }
    }
    return "Request unavailable"
}
