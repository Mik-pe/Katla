package app

import llm "../agent/llm"
import editor "../editor"
import "core:testing"
import "core:mem"
import "core:strings"
import "core:time"

@(test)
test_assistant_context_refresh_snapshots_next_turn_and_preserves_active_history :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker)
    captured:=mem.tracking_allocator(&tracker)
    mailbox:editor.Agent_Harness; editor.agent_harness_init(&mailbox,captured)
    config,error:=llm.config_parse(`provider="open_ai_compatible"
api_key="local-test-only"
base_url="http://127.0.0.1:1/v1"
rate_limit_min_interval_ms=0
timeout_ms=500`,captured); testing.expect_value(t,error,llm.Error.None)
    owner:Assistant; testing.expect_value(t,assistant_init(&owner,&mailbox,config,"[]","Selected entity_id=0",captured),llm.Error.None); llm.config_destroy(&config)
    previous:=owner.conversation.history[0]; revision:=owner.revision
    testing.expect_value(t,assistant_context_refresh(&owner,"Selected entity_id=0"),llm.Error.None); testing.expect(t,owner.revision==revision && !owner.context_changed)
    context.allocator=mem.nil_allocator()
    testing.expect_value(t,assistant_context_refresh(&owner,"Selected entity_id=4294967296"),llm.Error.None)
    testing.expect(t,owner.context_changed && owner.conversation.history[0]==previous && assistant_status(&owner)=="Ready")
    context.allocator=captured
    oversized:=make([]byte,llm.MAX_TEXT_BYTES+1); testing.expect_value(t,assistant_context_refresh(&owner,string(oversized)),llm.Error.Limit); delete(oversized)
    testing.expect_value(t,assistant_start(&owner,"Actual network turn"),llm.Error.None)
    testing.expect(t,!owner.context_changed)
    testing.expect_value(t,assistant_context_refresh(&owner,"Selected entity_id=8589934592"),llm.Error.None)
    testing.expect_value(t,assistant_reset(&owner),llm.Error.Busy)
    assistant_cancel(&owner)
    start:=time.tick_now()
    for owner.job.worker!=nil && time.tick_since(start)<2*time.Second { assistant_poll(&owner); time.sleep(time.Millisecond) }
    testing.expect(t,owner.job.worker==nil && owner.context_changed && owner.conversation.history[0]==previous && len(owner.conversation.history)==3)
    testing.expect(t,strings.contains(owner.conversation.history[1],"4294967296") && !strings.contains(owner.conversation.history[1],"8589934592") && strings.contains(owner.conversation.history[2],"Actual network turn"))
    testing.expect_value(t,assistant_reset(&owner),llm.Error.None)
    testing.expect(t,!owner.context_changed && strings.contains(owner.conversation.history[0],"8589934592"))
    testing.expect_value(t,assistant_destroy(&owner),u64(0)); editor.agent_harness_destroy(&mailbox)
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
