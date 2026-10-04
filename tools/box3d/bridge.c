// SPDX-License-Identifier: MIT
// Layout-stable accessors for the pinned Box3D dependency; Odin owns scene policy.
#include <box3d/box3d.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <stdatomic.h>
#include <math.h>
#include <box3d/constants.h>

#ifdef _WIN32
#define KATLA_API __declspec(dllexport)
#else
#define KATLA_API __attribute__((visibility("default")))
#endif
typedef struct {
    uint64_t id;
    uint32_t type, shape;
    float position[3], rotation[4], velocity[3], half_extents[3];
    float radius, half_height, gravity_scale, friction, restitution;
    uint32_t layers, mask, sensor, ccd;
    float density;
    const float (*vertices)[3];
    const uint32_t* indices;
    uint32_t vertex_count, index_count;
} KatlaBodySpec;
typedef struct { uint64_t id; float position[3], rotation[4], velocity[3]; } KatlaPose;
typedef struct KatlaJoint KatlaJoint;
typedef struct { b3WorldId id; KatlaJoint* joints; } KatlaWorld;
typedef struct { KatlaBodySpec spec; b3BodyId id; b3ShapeId shape; b3MeshData* mesh; b3HullData* hull; } KatlaBody;
typedef struct { uint64_t id,a,b; uint32_t kind,has_limits; float anchor_a[3],anchor_b[3],limits[2]; } KatlaJointSpec;
struct KatlaJoint { KatlaJointSpec spec; KatlaWorld* world; KatlaBody* a; KatlaBody* b; b3JointId id; bool awake_a,awake_b; KatlaJoint* next; };
static void refresh_spring(KatlaJoint* joint);
static atomic_flag world_lifecycle_lock = ATOMIC_FLAG_INIT;
static void lifecycle_lock(void) {
    while (atomic_flag_test_and_set_explicit(&world_lifecycle_lock,memory_order_acquire)) {}
}
static void lifecycle_unlock(void) { atomic_flag_clear_explicit(&world_lifecycle_lock,memory_order_release); }
_Static_assert(sizeof(KatlaBodySpec) == 136, "Body ABI");
_Static_assert(sizeof(KatlaPose) == 48, "Pose ABI");
_Static_assert(sizeof(KatlaJointSpec) == 64, "Joint ABI");
KATLA_API uint32_t katla_box3d_abi(void) { return b3IsDoublePrecision() ? 0 : 4; }
KATLA_API int64_t katla_box3d_bytes(void) { return b3GetByteCount(); }
KATLA_API void* katla_box3d_create(void) {
    KatlaWorld* world = calloc(1,sizeof(*world));
    if (!world) return NULL;
    b3WorldDef def = b3DefaultWorldDef();
    def.gravity = (b3Vec3){0, -9.81f, 0};
    lifecycle_lock(); world->id = b3CreateWorld(&def); lifecycle_unlock();
    if (B3_IS_NULL(world->id)) { free(world); return NULL; }
    return world;
}
KATLA_API void katla_box3d_destroy(void* owner) {
    KatlaWorld* world = owner;
    if (!world) return;
    lifecycle_lock(); b3DestroyWorld(world->id); lifecycle_unlock(); free(world);
}
static b3BodyType body_type(uint32_t type) {
    return type == 0 ? b3_dynamicBody : type == 1 ? b3_kinematicBody : b3_staticBody;
}
static b3ShapeId create_shape(KatlaBody* body) {
    const KatlaBodySpec* s = &body->spec;
    if (s->shape == 5) return (b3ShapeId){0};
    b3ShapeDef def = b3DefaultShapeDef();
    def.userData = body; def.density = s->density;
    def.baseMaterial.friction = s->friction; def.baseMaterial.restitution = s->restitution;
    def.isSensor = s->sensor != 0; def.enableSensorEvents = true;
    def.filter.categoryBits = s->layers; def.filter.maskBits = s->mask;
    if (s->shape == 0) {
        b3BoxHull box = b3MakeBoxHull(s->half_extents[0],s->half_extents[1],s->half_extents[2]);
        return b3CreateHullShape(body->id,&def,&box.base);
    }
    if (s->shape == 3) return b3CreateMeshShape(body->id,&def,body->mesh,b3Vec3_one);
    if (s->shape == 4) return b3CreateHullShape(body->id,&def,body->hull);
    if (s->shape == 1 || s->half_height == 0) {
        b3Sphere sphere = {.center = {0,0,0}, .radius = s->radius};
        return b3CreateSphereShape(body->id,&def,&sphere);
    }
    if (s->shape != 2) return (b3ShapeId){0};
    b3Capsule capsule = {.center1 = {0,-s->half_height,0}, .center2 = {0,s->half_height,0}, .radius = s->radius};
    return b3CreateCapsuleShape(body->id,&def,&capsule);
}
KATLA_API void* katla_box3d_body_create(void* owner, const KatlaBodySpec* spec) {
    KatlaWorld* world = owner;
    if (!world || !spec || spec->type > 2 || spec->shape > 5) return NULL;
    KatlaBody* body = calloc(1,sizeof(*body));
    if (!body) return NULL;
    body->spec = *spec;
    if (spec->shape == 3) {
        b3MeshDef mesh = {0}; mesh.vertices = (b3Vec3*)spec->vertices;
        mesh.indices = (int32_t*)spec->indices; mesh.vertexCount = (int)spec->vertex_count;
        mesh.triangleCount = (int)(spec->index_count/3); mesh.identifyEdges = true;
        body->mesh = b3CreateMesh(&mesh,NULL,0);
        if (!body->mesh) { free(body); return NULL; }
        if (body->mesh->degenerateCount || body->mesh->triangleCount != mesh.triangleCount) { b3DestroyMesh(body->mesh); free(body); return NULL; }
    }
    if (spec->shape == 4) {
        body->hull = b3CreateHull((const b3Vec3*)spec->vertices,(int)spec->vertex_count,(int)spec->vertex_count);
        if (!body->hull) { free(body); return NULL; }
    }
    b3BodyDef def = b3DefaultBodyDef();
    def.type = body_type(spec->type); def.userData = body;
    def.position = (b3Pos){spec->position[0],spec->position[1],spec->position[2]};
    def.rotation = (b3Quat){{spec->rotation[0],spec->rotation[1],spec->rotation[2]},spec->rotation[3]};
    def.linearVelocity = (b3Vec3){spec->velocity[0],spec->velocity[1],spec->velocity[2]};
    def.gravityScale = spec->gravity_scale; def.isBullet = spec->ccd != 0;
    body->id = b3CreateBody(world->id,&def);
    if (B3_IS_NULL(body->id)) { if (body->mesh) b3DestroyMesh(body->mesh); if (body->hull) b3DestroyHull(body->hull); free(body); return NULL; }
    if (spec->shape != 5) {
        body->shape = create_shape(body);
        if (B3_IS_NULL(body->shape)) { b3DestroyBody(body->id); if (body->mesh) b3DestroyMesh(body->mesh); if (body->hull) b3DestroyHull(body->hull); free(body); return NULL; }
    }
    return body;
}
KATLA_API void katla_box3d_body_destroy(void* owner) {
    KatlaBody* body = owner;
    if (!body) return;
    b3DestroyBody(body->id); if (body->mesh) b3DestroyMesh(body->mesh); if (body->hull) b3DestroyHull(body->hull); free(body);
}
KATLA_API void katla_box3d_body_update(void* owner, const KatlaBodySpec* next) {
    KatlaBody* body = owner;
    KatlaBodySpec* old = &body->spec;
    if (old->type != next->type) b3Body_SetType(body->id,body_type(next->type));
    if (memcmp(old->position,next->position,sizeof(old->position)) || memcmp(old->rotation,next->rotation,sizeof(old->rotation))) {
        b3Body_SetTransform(body->id,(b3Pos){next->position[0],next->position[1],next->position[2]},
            (b3Quat){{next->rotation[0],next->rotation[1],next->rotation[2]},next->rotation[3]});
    }
    if (memcmp(old->velocity,next->velocity,sizeof(old->velocity)))
        b3Body_SetLinearVelocity(body->id,(b3Vec3){next->velocity[0],next->velocity[1],next->velocity[2]});
    if (old->gravity_scale != next->gravity_scale) b3Body_SetGravityScale(body->id,next->gravity_scale);
    if (old->ccd != next->ccd) b3Body_SetBullet(body->id,next->ccd != 0);
    if (!B3_IS_NULL(body->shape)) {
        if (old->density != next->density) b3Shape_SetDensity(body->shape,next->density,true);
        if (old->friction != next->friction) b3Shape_SetFriction(body->shape,next->friction);
        if (old->restitution != next->restitution) b3Shape_SetRestitution(body->shape,next->restitution);
        if (old->layers != next->layers || old->mask != next->mask) {
            b3Filter filter = b3DefaultFilter(); filter.categoryBits = next->layers; filter.maskBits = next->mask;
            b3Shape_SetFilter(body->shape,filter,false);
        }
    }
    body->spec = *next;
}
KATLA_API void katla_box3d_step(void* owner, float delta) {
    KatlaWorld* world = owner;
    for (KatlaJoint* joint=world->joints; joint; joint=joint->next) if (joint->spec.kind==2) refresh_spring(joint);
    b3World_Step(world->id,delta,4);
}
KATLA_API void katla_box3d_pose(void* owner, KatlaPose* out) {
    KatlaBody* body = owner;
    b3Pos p = b3Body_GetPosition(body->id); b3Quat q = b3Body_GetRotation(body->id);
    b3Vec3 v = b3Body_GetLinearVelocity(body->id);
    *out = (KatlaPose){body->spec.id,{p.x,p.y,p.z},{q.v.x,q.v.y,q.v.z,q.s},{v.x,v.y,v.z}};
}
KATLA_API int32_t katla_box3d_overlaps(void* owner, uint64_t* ids, int32_t capacity) {
    KatlaBody* body = owner;
    if (!body->spec.sensor || B3_IS_NULL(body->shape)) return 0;
    int count = b3Shape_GetSensorCapacity(body->shape);
    if (capacity < count || !ids) return count;
    if (count == 0) return 0;
    b3ShapeId* visitors = malloc((size_t)count*sizeof(*visitors));
    if (!visitors) return -1;
    int written = b3Shape_GetSensorData(body->shape,visitors,count), valid = 0;
    for (int i=0;i<written;i++) {
        if (!b3Shape_IsValid(visitors[i])) continue;
        KatlaBody* other = b3Shape_GetUserData(visitors[i]);
        if (other) ids[valid++] = other->spec.id;
    }
    free(visitors); return valid;
}

static b3Vec3 local_anchor(const float anchor[3]) { return (b3Vec3){anchor[0],anchor[1],anchor[2]}; }
static b3JointDef joint_base(const KatlaJoint* joint) {
    b3JointDef def = b3DefaultSphericalJointDef().base;
    def.bodyIdA = joint->a->id; def.bodyIdB = joint->b->id;
    def.localFrameA.p = local_anchor(joint->spec.anchor_a);
    def.localFrameB.p = local_anchor(joint->spec.anchor_b);
    if (joint->spec.kind==1) {
        def.localFrameA.q = (b3Quat){{-0.7071067811865475f,0,0},0.7071067811865475f};
        def.localFrameB.q = def.localFrameA.q;
    }
    return def;
}
static void spring_factors(const KatlaJoint* joint,float* hertz,float* damping) {
    b3BodyId a=joint->a->id,b=joint->b->id;
    b3WorldTransform ta=b3Body_GetTransform(a),tb=b3Body_GetTransform(b);
    b3Pos pa=b3TransformWorldPoint(ta,local_anchor(joint->spec.anchor_a));
    b3Pos pb=b3TransformWorldPoint(tb,local_anchor(joint->spec.anchor_b));
    b3Vec3 axis=b3Normalize(b3SubPos(pb,pa));
    b3Vec3 ra=b3SubPos(pa,b3Body_GetWorldCenterOfMass(a)),rb=b3SubPos(pb,b3Body_GetWorldCenterOfMass(b));
    b3Vec3 ca=b3Cross(ra,axis),cb=b3Cross(rb,axis);
    float inverse=b3Body_GetInverseMass(a)+b3Body_GetInverseMass(b)
        +b3Dot(ca,b3MulMV(b3Body_GetWorldInverseRotationalInertia(a),ca))
        +b3Dot(cb,b3MulMV(b3Body_GetWorldInverseRotationalInertia(b),cb));
    float omega=sqrtf(fmaxf(inverse,0));
    *hertz=omega/(2*B3_PI); *damping=0.25f*omega;
}
static void refresh_spring(KatlaJoint* joint) {
    float hertz,damping; spring_factors(joint,&hertz,&damping);
    b3DistanceJoint_SetSpringHertz(joint->id,hertz);
    b3DistanceJoint_SetSpringDampingRatio(joint->id,damping);
}
KATLA_API void katla_box3d_body_copy_angular_motion(void* destination,void* source) {
    KatlaBody* dst=destination; KatlaBody* src=source;
    b3Body_SetAngularVelocity(dst->id,b3Body_GetAngularVelocity(src->id));
}
KATLA_API void* katla_box3d_joint_prepare(void* owner,const KatlaJointSpec* spec,void* endpoint_a,void* endpoint_b) {
    KatlaWorld* world=owner; KatlaBody* a=endpoint_a; KatlaBody* b=endpoint_b;
    if (!world || !spec || !a || !b || a==b || spec->a!=a->spec.id || spec->b!=b->spec.id || spec->kind>3 || spec->has_limits>1) return NULL;
    if (!b3World_IsValid(world->id) || !b3Body_IsValid(a->id) || !b3Body_IsValid(b->id) || B3_IS_NULL(a->shape) || B3_IS_NULL(b->shape)) return NULL;
    if (a->id.world0!=world->id.index1-1 || b->id.world0!=world->id.index1-1) return NULL;
    for (int i=0;i<3;i++) if (!isfinite(spec->anchor_a[i]) || !isfinite(spec->anchor_b[i])) return NULL;
    if (spec->has_limits && (!isfinite(spec->limits[0]) || !isfinite(spec->limits[1]) || spec->limits[0]>spec->limits[1])) return NULL;
    if (spec->kind==1 && spec->has_limits && (spec->limits[0]<-0.99f*B3_PI || spec->limits[1]>0.99f*B3_PI)) return NULL;
    float rest=spec->has_limits ? (spec->limits[0]+spec->limits[1])*0.5f : 0.5f;
    if (spec->kind==2 && (!isfinite(rest) || rest<B3_LINEAR_SLOP)) return NULL;
    KatlaJoint* joint=calloc(1,sizeof(*joint)); if (!joint) return NULL;
    joint->spec=*spec; joint->world=world; joint->a=a; joint->b=b; joint->awake_a=b3Body_IsAwake(a->id); joint->awake_b=b3Body_IsAwake(b->id);
    if (spec->kind==2) { float hertz,damping; spring_factors(joint,&hertz,&damping); if (!isfinite(hertz) || !isfinite(damping)) { free(joint); return NULL; } }
    return joint;
}
KATLA_API int32_t katla_box3d_joint_publish(void* owner) {
    KatlaJoint* joint=owner; if (!joint || !B3_IS_NULL(joint->id)) return 0;
    b3JointDef base=joint_base(joint);
    switch (joint->spec.kind) {
    case 0: { b3SphericalJointDef def=b3DefaultSphericalJointDef(); def.base=base; joint->id=b3CreateSphericalJoint(joint->world->id,&def); break; }
    case 1: { b3RevoluteJointDef def=b3DefaultRevoluteJointDef(); def.base=base; def.enableLimit=joint->spec.has_limits!=0;
        def.lowerAngle=joint->spec.limits[0]; def.upperAngle=joint->spec.limits[1]; joint->id=b3CreateRevoluteJoint(joint->world->id,&def); break; }
    case 2: { b3DistanceJointDef def=b3DefaultDistanceJointDef(); def.base=base; def.enableSpring=true; def.enableLimit=false;
        def.length=joint->spec.has_limits ? (joint->spec.limits[0]+joint->spec.limits[1])*0.5f : 0.5f;
        spring_factors(joint,&def.hertz,&def.dampingRatio); joint->id=b3CreateDistanceJoint(joint->world->id,&def); break; }
    case 3: { b3WeldJointDef def=b3DefaultWeldJointDef(); def.base=base; joint->id=b3CreateWeldJoint(joint->world->id,&def); break; }
    default: return 0;
    }
    if (B3_IS_NULL(joint->id)) return 0;
    joint->next=joint->world->joints; joint->world->joints=joint; return 1;
}
static void detach_joint(KatlaJoint* joint,bool wake) {
    if (!joint) return;
    if (!B3_IS_NULL(joint->id)) {
        KatlaJoint** cursor=&joint->world->joints;
        while (*cursor && *cursor!=joint) cursor=&(*cursor)->next;
        if (*cursor) *cursor=joint->next;
        if (b3Joint_IsValid(joint->id)) b3DestroyJoint(joint->id,wake);
    }
    joint->id=b3_nullJointId;
}
KATLA_API void katla_box3d_joint_destroy(void* owner) { detach_joint(owner,true); free(owner); }
KATLA_API void katla_box3d_joint_rollback(void* owner) {
    detach_joint(owner,false);
}
KATLA_API void katla_box3d_joint_restore(void* owner) {
    KatlaJoint* joint=owner; if (!joint) return;
    b3BodyId a=joint->a->id,b=joint->b->id; bool awake_a=joint->awake_a,awake_b=joint->awake_b;
    free(joint);
    if (b3Body_IsValid(a)) b3Body_SetAwake(a,awake_a);
    if (b3Body_IsValid(b)) b3Body_SetAwake(b,awake_b);
}
KATLA_API int32_t katla_box3d_joint_valid(void* owner) { KatlaJoint* joint=owner; return joint && !B3_IS_NULL(joint->id) && b3Joint_IsValid(joint->id); }
