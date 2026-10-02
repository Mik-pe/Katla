-- Attach to a prefab trigger with particles; emit prefab_activated on enter.
function on_spawn(entity, world)
    world:on_event("prefab_activated", function(name, data, current_world)
        if data.trigger ~= entity then return end
        current_world:set_particles_active(entity, true)
        current_world:burst_particles(entity, 32)
    end)
end
