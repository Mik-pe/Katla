-- Player Controller Script
-- Attach to any entity to make it controllable with WASD

local speed = 5.0

function on_spawn(entity, world)
    print("Player controller attached to entity " .. tostring(entity))
end

function on_update(entity, world, dt)
    local transform = world:get_transform(entity)
    if not transform then
        return
    end

    local pos = transform.position
    local dir = Vec3.new(0, 0, 0)

    if world:is_action_pressed("move_forward") then
        dir = dir + Vec3.new(0, 0, -1)
    end
    if world:is_action_pressed("move_backward") then
        dir = dir + Vec3.new(0, 0, 1)
    end
    if world:is_action_pressed("move_left") then
        dir = dir + Vec3.new(-1, 0, 0)
    end
    if world:is_action_pressed("move_right") then
        dir = dir + Vec3.new(1, 0, 0)
    end

    local len = dir:length()
    if len > 0 then
        dir = dir:normalize()
        local new_pos = pos + dir * speed * dt
        world:set_position(entity, new_pos)
    end
end

function on_destroy(entity, world)
    print("Player controller destroyed for entity " .. tostring(entity))
end
