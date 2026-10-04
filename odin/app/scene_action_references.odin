//! Authored deletion removes dangling rules in the same undoable resource transaction.
package app

import scene "../agent/scene"
import ecs "../ecs"

@(private="package")
scene_action_prune_rules :: proc(rules:^Trigger_Rules,removed:map[ecs.Entity_Id]bool) {
    retained:=make([dynamic]scene.Trigger_Rule)
    defer { for rule in retained { delete(rule.actions) }; delete(retained) }
    fired:=make([dynamic]bool); defer delete(fired)
    for rule,i in rules.rules {
        if rule.has_other && removed[rule.other] { continue }
        actions:=make([dynamic]scene.Event_Action)
        for action in rule.actions {
            if action.kind!=.Emit && action.target.kind==.Entity && removed[action.target.entity] { continue }
            append(&actions,action)
        }
        if len(actions)==0 { delete(actions); continue }
        filtered:=rule; filtered.actions=actions[:]
        append(&retained,filtered)
        append(&fired,i<len(rules.fired) && rules.fired[i])
    }
    source:=Trigger_Rules{rules=retained[:],fired=fired[:],last_errors=rules.last_errors}
    replacement:Trigger_Rules; trigger_rules_clone(&replacement,&source)
    trigger_rules_destroy(rules); rules^=replacement
}
