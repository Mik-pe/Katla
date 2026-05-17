-- Spinning Cube Script
-- Rotates the entity around the Y axis

local rotation_speed = 90.0 -- degrees per second

function on_spawn(entity, world)
    print("Spinning cube attached to entity " .. tostring(entity))
end

function on_update(entity, world, dt)
    local transform = world:get_transform(entity)
    if not transform then
        return
    end

    local pos = transform.position
    local angle_rad = math.rad(rotation_speed) * dt
    local radius = 2.0

    local new_x = pos.x * math.cos(angle_rad) - pos.z * math.sin(angle_rad)
    local new_z = pos.x * math.sin(angle_rad) + pos.z * math.cos(angle_rad)

    world:set_position(entity, Vec3.new(new_x, pos.y, new_z))
end

function on_destroy(entity, world)
    print("Spinning cube destroyed for entity " .. tostring(entity))
end
