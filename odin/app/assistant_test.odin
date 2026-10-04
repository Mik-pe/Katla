package app

import llm "../agent/llm"
import editor "../editor"
import "core:testing"
import "core:mem"
import "core:time"

@(test)
test_assistant_disabled_owns_no_producer :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker)
    context.allocator=mem.tracking_allocator(&tracker)
    mailbox:editor.Agent_Harness; editor.agent_harness_init(&mailbox)
    config:=llm.config_default(); owner:Assistant
    testing.expect(t,assistant_init(&owner,&mailbox,config,"[]","Scene")==.Disabled)
    llm.config_destroy(&config)
    testing.expect(t,owner.initialized && owner.state==.Disabled && owner.job.worker==nil)
    testing.expect(t,assistant_start(&owner,"Edit")==.Disabled && assistant_reset(&owner)==.Disabled)
    assistant_cancel(&owner); testing.expect(t,!assistant_poll(&owner) && mailbox.outstanding==0)
    testing.expect(t,assistant_destroy(&owner)==0); editor.agent_harness_destroy(&mailbox)
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
@(test)
test_assistant_error_preserves_choice_and_requires_reset :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker)
    context.allocator=mem.tracking_allocator(&tracker)
    mailbox:editor.Agent_Harness; editor.agent_harness_init(&mailbox)
    config,error:=llm.config_parse(`provider="open_ai_compatible"
api="responses"
api_key="local-test-only"
base_url="http://127.0.0.1:1/v1"
model="chosen-model"
timeout_ms=500
rate_limit_min_interval_ms=0`)
    testing.expect(t,error==.None)
    owner:Assistant; testing.expect(t,assistant_init(&owner,&mailbox,config,"[]","Scene")==.None); llm.config_destroy(&config)
    testing.expect(t,owner.config.model=="chosen-model" && owner.conversation.config.model=="chosen-model")
    testing.expect(t,assistant_start(&owner," ")==.Config && owner.state==.Idle)
    testing.expect(t,assistant_start(&owner,"Edit")==.None && assistant_start(&owner,"Duplicate")==.Busy && assistant_reset(&owner)==.Busy)
    deadline:=time.tick_now()
    for owner.job.worker!=nil && time.duration_seconds(time.tick_since(deadline))<2 { assistant_poll(&owner); time.sleep(time.Millisecond) }
    testing.expect(t,owner.job.worker==nil && owner.state==.Failed && owner.error==.Network && owner.conversation.failed)
    testing.expect(t,assistant_start(&owner,"Silent retry")==.Network && mailbox.outstanding==0)
    testing.expect(t,assistant_reset(&owner)==.None && owner.state==.Idle && !owner.conversation.failed && len(owner.conversation.history)==1)
    testing.expect(t,assistant_start(&owner,"Cancel")==.None)
    assistant_cancel(&owner); testing.expect(t,owner.state==.Cancelling)
    testing.expect(t,assistant_destroy(&owner)==0); editor.agent_harness_destroy(&mailbox)
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
@(test)
test_assistant_invalid_schema_and_missing_config_cleanup :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker)
    context.allocator=mem.tracking_allocator(&tracker)
    mailbox:editor.Agent_Harness; editor.agent_harness_init(&mailbox)
    config,error:=llm.config_parse(`provider="open_ai_compatible"
api_key="local-test-only"
base_url="http://127.0.0.1:1/v1"`); testing.expect(t,error==.None)
    owner:Assistant; testing.expect(t,assistant_init(&owner,&mailbox,config,"{}","Scene")==.Config); llm.config_destroy(&config)
    testing.expect(t,owner.state==.Failed && owner.error==.Config && assistant_start(&owner,"Edit")==.Config)
    assistant_destroy(&owner)
    testing.expect(t,assistant_init_path(&owner,&mailbox,"/unavailable/katla-llm.toml","[]","Scene")==.Config && owner.initialized && owner.state==.Failed && owner.error==.Config)
    assistant_destroy(&owner); editor.agent_harness_destroy(&mailbox)
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
