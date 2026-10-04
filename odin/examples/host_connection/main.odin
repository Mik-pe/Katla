//! Local API fixture consumer; never run against an actual user's conversation for acceptance.
package main

import host "../../agent/host"
import "core:os"
import "core:time"
import "core:fmt"
import "core:mem"

PNG :: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
main :: proc() {
    assert(len(os.args)==2 || len(os.args)==3)
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator)
    allocator:=mem.tracking_allocator(&tracker)
    b:host.Bridge
    assert(host.bridge_connect(&b,{socket=os.args[1],thread_id="fixture-existing-thread"},allocator)==.None)
    mode:="journey"; if len(os.args)==3 { mode=os.args[2] }
    if mode=="cancel_wait" {
        time.sleep(100*time.Millisecond)
        cancel_started:=time.tick_now(); host.bridge_destroy(&b)
        assert(time.tick_since(cancel_started)<time.Second && len(tracker.allocation_map)==0)
        mem.tracking_allocator_destroy(&tracker); fmt.println("PASS host shutdown cancels stalled API read without touching an external turn"); return
    }
    if mode=="disconnect" {
        wait_started:=time.tick_now(); seen:=false
        for time.tick_since(wait_started)<3*time.Second {
            event,ready:=host.bridge_poll(&b)
            if !ready { time.sleep(time.Millisecond); continue }
            if event.kind==.Disconnected { seen=true }; host.event_destroy(&event)
            if seen { break }
        }
        assert(seen && !host.bridge_connected(&b)); host.bridge_destroy(&b)
        assert(len(tracker.allocation_map)==0); mem.tracking_allocator_destroy(&tracker)
        fmt.println("PASS host malformed/missing/closed connection fails explicitly"); return
    }
    started:=time.tick_now(); connected,first_sent,second_sent,cancel_sent,attention,finished,disconnected:bool
    accepted,texts,completions:int
    for time.tick_since(started)<20*time.Second {
        event,ready:=host.bridge_poll(&b)
        if !ready { time.sleep(time.Millisecond); continue }
        switch event.kind {
        case .Connected:
            assert(!connected && event.name=="Existing fixture conversation"); connected=true
            assert(host.bridge_submit(&b,"First viewport question",`{"submission":7,"selected":null}`,PNG)==.None); first_sent=true
        case .Accepted:
            accepted+=1
            if event.turn_id=="new-turn" { assert(first_sent) }
            else { assert(event.turn_id=="existing-active-turn" && second_sent); assert(host.bridge_cancel(&b,event.turn_id)==.None); cancel_sent=true }
        case .Text:
            assert(event.turn_id=="new-turn" && event.item_id=="agent-item" && event.text=="Viewport fixture reply"); texts+=1
        case .Finished:
            completions+=1
            if event.turn_id=="new-turn" { assert(event.status=="completed"); assert(host.bridge_submit(&b,"Continue existing conversation",`{"submission":8,"selected":"0"}`,PNG)==.None); second_sent=true }
            else { assert(event.turn_id=="existing-active-turn" && event.status=="interrupted" && cancel_sent); finished=true }
        case .Attention: attention=true
        case .Error: panic(event.text)
        case .Disconnected: disconnected=true
        }
        host.event_destroy(&event)
        if disconnected { break }
    }
    assert(connected && first_sent && second_sent && cancel_sent && attention && finished && disconnected)
    assert(accepted==2 && texts==1 && completions==2 && !host.bridge_connected(&b))
    assert(host.bridge_submit(&b,"closed",`{}`,PNG)==.Closed)
    host.bridge_destroy(&b)
    assert(len(tracker.allocation_map)==0)
    mem.tracking_allocator_destroy(&tracker)
    fmt.println("PASS existing-thread Unix transport: start/steer scoped events/attention/interrupt/EOF and captured ownership")
}
