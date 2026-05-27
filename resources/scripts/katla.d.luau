-- Katla Engine API Type Definitions
-- Auto-generated type stubs for Luau autocomplete support

export type EntityId = {
	id: () -> number,
	__tostring: (self: EntityId) -> string,
	__eq: (self: EntityId, other: EntityId) -> boolean,
}

export type Vec3 = {
	x: number,
	y: number,
	z: number,
	length: (self: Vec3) -> number,
	length_squared: (self: Vec3) -> number,
	normalize: (self: Vec3) -> Vec3,
	dot: (self: Vec3, other: Vec3) -> number,
	cross: (self: Vec3, other: Vec3) -> Vec3,
	lerp: (self: Vec3, other: Vec3, t: number) -> Vec3,
	distance: (self: Vec3, other: Vec3) -> number,
}

export type Quat = {
	x: number,
	y: number,
	z: number,
	w: number,
}

export type Color = {
	r: number,
	g: number,
	b: number,
	a: number,
}

export type Transform = {
	position: Vec3,
	rotation: Quat,
	scale: Vec3,
}

export type RaycastResult = {
	entity: EntityId?,
	point: Vec3,
	normal: Vec3,
	distance: number,
}

export type World = {
	get_transform: (self: World, entity: EntityId) -> Transform?,
	set_transform: (self: World, entity: EntityId, transform: Transform) -> (),
	set_position: (self: World, entity: EntityId, position: Vec3) -> (),
	spawn_entity: (self: World) -> number,
	destroy_entity: (self: World, entity: EntityId) -> (),
	entity_exists: (self: World, entity: EntityId) -> boolean,
	get_all_with: (self: World, component_name: string) -> {EntityId},
	is_action_pressed: (self: World, action: string) -> boolean,
	get_mouse_delta: (self: World) -> (number, number),
	get_mouse_wheel: (self: World) -> number,
	emit: (self: World, event_name: string, data: any) -> (),
	on_event: (self: World, event_name: string, callback: (string, any) -> ()) -> (),
	play_sound: (self: World, path: string, volume: number?, looping: boolean?) -> (),
	play_sound_at: (self: World, path: string, position: Vec3, volume: number?, looping: boolean?) -> (),
	play_sound_cue: (self: World, cue_name: string) -> (),
	raycast: (self: World, origin: Vec3, direction: Vec3, max_distance: number) -> number,
	get_raycast_result: (self: World, index: number) -> RaycastResult?,
}

-- Global constructors

declare function Vec3_new(x: number, y: number, z: number): Vec3
declare function Quat_new(x: number, y: number, z: number, w: number): Quat
declare function Quat_identity(): Quat
declare function Quat_from_axis_angle(axis: Vec3, angle: number): Quat
declare function Color_new(r: number, g: number, b: number, a: number): Color
declare function Color_rgb(r: number, g: number, b: number): Color
declare function Color_from_rgb_hex(hex: number): Color

-- Script lifecycle hooks
-- Define these in your script to receive callbacks:
--
-- function on_update(entity: EntityId, world: World, dt: number)
-- function on_spawn(entity: EntityId, world: World)
-- function on_destroy(entity: EntityId)

return {}
