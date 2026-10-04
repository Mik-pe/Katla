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
    const float* heights;
    uint32_t rows, cols;
    float height_scale[3];
} KatlaBodySpec;
typedef struct { uint64_t id; float position[3], rotation[4], velocity[3]; } KatlaPose;
typedef struct KatlaJoint KatlaJoint;
typedef struct { b3WorldId id; KatlaJoint* joints; } KatlaWorld;
typedef struct { KatlaBodySpec spec; b3BodyId id; b3ShapeId shape; b3MeshData* mesh; b3HullData* hull; b3HeightFieldData* heightfield; b3ShapeId* pieces; uint32_t piece_count; } KatlaBody;
typedef struct { uint64_t id,a,b; uint32_t kind,has_limits; float anchor_a[3],anchor_b[3],limits[2]; } KatlaJointSpec;
struct KatlaJoint { KatlaJointSpec spec; KatlaWorld* world; KatlaBody* a; KatlaBody* b; b3JointId id; bool awake_a,awake_b; KatlaJoint* next; };
static void refresh_spring(KatlaJoint* joint);
static atomic_flag world_lifecycle_lock = ATOMIC_FLAG_INIT;
static void lifecycle_lock(void) {
    while (atomic_flag_test_and_set_explicit(&world_lifecycle_lock,memory_order_acquire)) {}
}
static void lifecycle_unlock(void) { atomic_flag_clear_explicit(&world_lifecycle_lock,memory_order_release); }
_Static_assert(sizeof(KatlaBodySpec) == 168, "Body ABI");
_Static_assert(sizeof(KatlaPose) == 48, "Pose ABI");
_Static_assert(sizeof(KatlaJointSpec) == 64, "Joint ABI");
KATLA_API uint32_t katla_box3d_abi(void) { return b3IsDoublePrecision() ? 0 : 8; }
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
static b3Vec3 grid_offset(const KatlaBodySpec* spec,b3Quat rotation) {
    if (spec->shape!=6 || spec->type!=2) return b3Vec3_zero;
    return b3RotateVector(rotation,(b3Vec3){-.5f*(spec->cols-1)*spec->height_scale[0],0,-.5f*(spec->rows-1)*spec->height_scale[2]});
}
static b3Pos native_position(const KatlaBodySpec* spec,b3Quat rotation) {
    return b3Add((b3Pos){spec->position[0],spec->position[1],spec->position[2]},grid_offset(spec,rotation));
}
#include <stddef.h>
typedef struct {
    b3HullData base; b3HullVertex vertices[3]; uint8_t vertex_padding[5];
    b3Vec3 points[3]; uint8_t point_padding[4]; b3HullHalfEdge edges[6];
    b3HullFace faces[2]; uint8_t face_padding[6]; b3Plane planes[2];
} KatlaTriangleHull;
static b3ShapeId create_triangle(KatlaBody* body,const b3ShapeDef* def,b3Vec3 a,b3Vec3 b,b3Vec3 c) {
    b3Vec3 cross=b3Cross(b3Sub(b,a),b3Sub(c,a)); float area=b3Length(cross);
    if (!(area>0) || !isfinite(area)) return b3_nullShapeId;
    KatlaTriangleHull hull; memset(&hull,0,sizeof(hull));
    hull.base.version=B3_HULL_VERSION; hull.base.byteCount=sizeof(hull);
    hull.base.vertexCount=3; hull.base.edgeCount=6; hull.base.faceCount=2;
    hull.base.vertexOffset=offsetof(KatlaTriangleHull,vertices); hull.base.pointOffset=offsetof(KatlaTriangleHull,points);
    hull.base.edgeOffset=offsetof(KatlaTriangleHull,edges); hull.base.faceOffset=offsetof(KatlaTriangleHull,faces); hull.base.planeOffset=offsetof(KatlaTriangleHull,planes);
    hull.points[0]=a; hull.points[1]=b; hull.points[2]=c;
    hull.base.aabb=(b3AABB){b3Min(a,b3Min(b,c)),b3Max(a,b3Max(b,c))}; hull.base.center=b3MulSV(1.0f/3,b3Add(a,b3Add(b,c))); hull.base.surfaceArea=area;
    hull.vertices[0].edge=0; hull.vertices[1].edge=2; hull.vertices[2].edge=4;
    hull.edges[0]=(b3HullHalfEdge){2,1,0,0}; hull.edges[1]=(b3HullHalfEdge){5,0,1,1};
    hull.edges[2]=(b3HullHalfEdge){4,3,1,0}; hull.edges[3]=(b3HullHalfEdge){1,2,2,1};
    hull.edges[4]=(b3HullHalfEdge){0,5,2,0}; hull.edges[5]=(b3HullHalfEdge){3,4,0,1};
    hull.faces[0].edge=0; hull.faces[1].edge=1;
    b3Vec3 normal=b3MulSV(1/area,cross); float offset=b3Dot(normal,a);
    hull.planes[0]=(b3Plane){normal,offset}; hull.planes[1]=(b3Plane){b3Neg(normal),-offset};
    hull.base.hash=b3Hash(B3_HASH_INIT,(const uint8_t*)&hull,sizeof(hull)); if (!hull.base.hash) hull.base.hash=1;
    return b3CreateHullShape(body->id,def,&hull.base);
}

static b3Vec3 height_point(const KatlaBodySpec* shape,uint32_t row,uint32_t col);
static b3ShapeId create_shape(KatlaBody* body) {
    const KatlaBodySpec* s = &body->spec;
    if (s->shape == 5) return (b3ShapeId){0};
    b3ShapeDef def = b3DefaultShapeDef();
    def.userData = body; def.density = s->density;
    def.baseMaterial.friction = s->friction; def.baseMaterial.restitution = s->restitution;
    def.isSensor = s->sensor != 0; def.enableSensorEvents = true;
    def.filter.categoryBits = s->layers; def.filter.maskBits = s->mask;
    if ((s->shape==3 || s->shape==6) && s->type!=2) {
        uint64_t count=s->shape==3 ? s->index_count/3 : (uint64_t)(s->rows-1)*(s->cols-1)*2;
        if (!count || count>1000000) return b3_nullShapeId;
        body->pieces=calloc((size_t)count,sizeof(*body->pieces)); if (!body->pieces) return b3_nullShapeId;
        body->piece_count=(uint32_t)count;
        for (uint32_t i=0;i<body->piece_count;i++) {
            b3Vec3 a,b,c;
            if (s->shape==3) {
                const float* p=s->vertices[s->indices[i*3]],*q=s->vertices[s->indices[i*3+1]],*r=s->vertices[s->indices[i*3+2]];
                a=(b3Vec3){p[0],p[1],p[2]}; b=(b3Vec3){q[0],q[1],q[2]}; c=(b3Vec3){r[0],r[1],r[2]};
            } else {
                uint32_t cell=i/2,row=cell/(s->cols-1),col=cell%(s->cols-1);
                if (i%2==0) { a=height_point(s,row,col); b=height_point(s,row+1,col); c=height_point(s,row,col+1); }
                else { a=height_point(s,row+1,col+1); b=height_point(s,row,col+1); c=height_point(s,row+1,col); }
            }
            body->pieces[i]=create_triangle(body,&def,a,b,c); if (B3_IS_NULL(body->pieces[i])) return b3_nullShapeId;
        }
        return body->pieces[0];
    }
    if (s->shape == 0) {
        b3BoxHull box = b3MakeBoxHull(s->half_extents[0],s->half_extents[1],s->half_extents[2]);
        return b3CreateHullShape(body->id,&def,&box.base);
    }
    if (s->shape == 3) return b3CreateMeshShape(body->id,&def,body->mesh,b3Vec3_one);
    if (s->shape == 4) return b3CreateHullShape(body->id,&def,body->hull);
    if (s->shape == 6) return b3CreateHeightFieldShape(body->id,&def,body->heightfield);
    if (s->shape == 1 || s->half_height == 0) {
        b3Sphere sphere = {.center = {0,0,0}, .radius = s->radius};
        return b3CreateSphereShape(body->id,&def,&sphere);
    }
    if (s->shape != 2) return (b3ShapeId){0};
    b3Capsule capsule = {.center1 = {0,-s->half_height,0}, .center2 = {0,s->half_height,0}, .radius = s->radius};
    return b3CreateCapsuleShape(body->id,&def,&capsule);
}
static void apply_mesh_mass(KatlaBody* body,const KatlaBodySpec* spec) {
    if (spec->shape!=3 || spec->type!=0) return;
    double origin[3]={0},center[3]={0},volume=0;
    for (uint32_t i=0;i<spec->vertex_count;i++) for (int axis=0;axis<3;axis++) origin[axis]+=spec->vertices[i][axis]/(double)spec->vertex_count;
    for (uint32_t i=0;i<spec->index_count;i+=3) {
        double v[3][3],sum[3]={0};
        for (int j=0;j<3;j++) for (int axis=0;axis<3;axis++) { v[j][axis]=spec->vertices[spec->indices[i+j]][axis]-origin[axis]; sum[axis]+=v[j][axis]; }
        double det=v[0][0]*(v[1][1]*v[2][2]-v[1][2]*v[2][1])+v[0][1]*(v[1][2]*v[2][0]-v[1][0]*v[2][2])+v[0][2]*(v[1][0]*v[2][1]-v[1][1]*v[2][0]);
        volume+=det; for (int axis=0;axis<3;axis++) center[axis]+=det*(origin[axis]+sum[axis]*.25);
    }
    if (volume==0) return;
    for (int axis=0;axis<3;axis++) center[axis]/=volume;
    double integrals[3][3]={{0}};
    for (uint32_t i=0;i<spec->index_count;i+=3) {
        double v[3][3],sum[3]={0};
        for (int j=0;j<3;j++) for (int axis=0;axis<3;axis++) { v[j][axis]=spec->vertices[spec->indices[i+j]][axis]-center[axis]; sum[axis]+=v[j][axis]; }
        double det=v[0][0]*(v[1][1]*v[2][2]-v[1][2]*v[2][1])+v[0][1]*(v[1][2]*v[2][0]-v[1][0]*v[2][2])+v[0][2]*(v[1][0]*v[2][1]-v[1][1]*v[2][0]);
        for (int a=0;a<3;a++) for (int b=0;b<3;b++) {
            double entry=sum[a]*sum[b]; for (int j=0;j<3;j++) entry+=v[j][a]*v[j][b]; integrals[a][b]+=det*entry/120;
        }
    }
    double factor=spec->density*(volume>0 ? 1:-1); b3Matrix3 inertia;
    inertia.cx=(b3Vec3){(float)((integrals[1][1]+integrals[2][2])*factor),(float)(-integrals[0][1]*factor),(float)(-integrals[0][2]*factor)};
    inertia.cy=(b3Vec3){(float)(-integrals[0][1]*factor),(float)((integrals[0][0]+integrals[2][2])*factor),(float)(-integrals[1][2]*factor)};
    inertia.cz=(b3Vec3){(float)(-integrals[0][2]*factor),(float)(-integrals[1][2]*factor),(float)((integrals[0][0]+integrals[1][1])*factor)};
    b3Body_SetMassData(body->id,(b3MassData){(float)(fabs(volume)/6*spec->density),(b3Vec3){(float)center[0],(float)center[1],(float)center[2]},inertia});
}

KATLA_API void* katla_box3d_body_create(void* owner, const KatlaBodySpec* spec) {
    KatlaWorld* world = owner;
    if (!world || !spec || spec->type > 2 || spec->shape > 6) return NULL;
    KatlaBody* body = calloc(1,sizeof(*body));
    if (!body) return NULL;
    body->spec = *spec;
    if (spec->shape == 3 && spec->type==2) {
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
    if (spec->shape == 6 && spec->type==2) {
        if (spec->type!=2 || !spec->heights || spec->rows<2 || spec->cols<2 || (uint64_t)spec->rows*spec->cols>1000000) { free(body); return NULL; }
        b3HeightFieldDef field={0}; field.heights=(float*)spec->heights;
        for (int axis=0;axis<3;axis++) if (!isfinite(spec->height_scale[axis]) || spec->height_scale[axis]<=0) { free(body); return NULL; }
        field.countX=(int)spec->cols; field.countZ=(int)spec->rows;
        field.scale=(b3Vec3){spec->height_scale[0],spec->height_scale[1],spec->height_scale[2]};
        field.globalMinimumHeight=field.globalMaximumHeight=spec->heights[0];
        for (uint64_t i=0;i<(uint64_t)spec->rows*spec->cols;i++) {
            if (!isfinite(spec->heights[i])) { free(body); return NULL; }
            field.globalMinimumHeight=fminf(field.globalMinimumHeight,spec->heights[i]);
            field.globalMaximumHeight=fmaxf(field.globalMaximumHeight,spec->heights[i]);
        }
        body->heightfield=b3CreateHeightField(&field);
        if (!body->heightfield) { free(body); return NULL; }
    }
    b3BodyDef def = b3DefaultBodyDef();
    def.type = body_type(spec->type); def.userData = body;
    def.rotation = (b3Quat){{spec->rotation[0],spec->rotation[1],spec->rotation[2]},spec->rotation[3]};
    def.position = native_position(spec,def.rotation);
    def.linearVelocity = (b3Vec3){spec->velocity[0],spec->velocity[1],spec->velocity[2]};
    def.gravityScale = spec->gravity_scale; def.isBullet = spec->ccd != 0;
    body->id = b3CreateBody(world->id,&def);
    if (B3_IS_NULL(body->id)) { if (body->mesh) b3DestroyMesh(body->mesh); if (body->hull) b3DestroyHull(body->hull); if (body->heightfield) b3DestroyHeightField(body->heightfield); free(body->pieces); free(body); return NULL; }
    if (spec->shape != 5) {
        body->shape = create_shape(body);
        if (B3_IS_NULL(body->shape)) { b3DestroyBody(body->id); if (body->mesh) b3DestroyMesh(body->mesh); if (body->hull) b3DestroyHull(body->hull); if (body->heightfield) b3DestroyHeightField(body->heightfield); free(body->pieces); free(body); return NULL; }
    }
    apply_mesh_mass(body,spec);
    return body;
}
KATLA_API void katla_box3d_body_destroy(void* owner) {
    KatlaBody* body = owner;
    if (!body) return;
    b3DestroyBody(body->id); if (body->mesh) b3DestroyMesh(body->mesh); if (body->hull) b3DestroyHull(body->hull); if (body->heightfield) b3DestroyHeightField(body->heightfield); free(body->pieces); free(body);
}
KATLA_API void katla_box3d_body_update(void* owner, const KatlaBodySpec* next) {
    KatlaBody* body = owner;
    KatlaBodySpec* old = &body->spec;
    if (old->type != next->type) b3Body_SetType(body->id,body_type(next->type));
    if (memcmp(old->position,next->position,sizeof(old->position)) || memcmp(old->rotation,next->rotation,sizeof(old->rotation))) {
        b3Quat rotation={{next->rotation[0],next->rotation[1],next->rotation[2]},next->rotation[3]};
        b3Body_SetTransform(body->id,native_position(next,rotation),rotation);
    }
    if (memcmp(old->velocity,next->velocity,sizeof(old->velocity)))
        b3Body_SetLinearVelocity(body->id,(b3Vec3){next->velocity[0],next->velocity[1],next->velocity[2]});
    if (old->gravity_scale != next->gravity_scale) b3Body_SetGravityScale(body->id,next->gravity_scale);
    if (old->ccd != next->ccd) b3Body_SetBullet(body->id,next->ccd != 0);
    uint32_t count=body->pieces ? body->piece_count : B3_IS_NULL(body->shape) ? 0:1;
    for (uint32_t i=0;i<count;i++) {
        b3ShapeId shape=body->pieces ? body->pieces[i]:body->shape;
        if (old->density != next->density) b3Shape_SetDensity(shape,next->density,true);
        if (old->friction != next->friction) b3Shape_SetFriction(shape,next->friction);
        if (old->restitution != next->restitution) b3Shape_SetRestitution(shape,next->restitution);
        if (old->layers != next->layers || old->mask != next->mask) {
            b3Filter filter = b3DefaultFilter(); filter.categoryBits = next->layers; filter.maskBits = next->mask;
            b3Shape_SetFilter(shape,filter,false);
        }
    }
    if (old->density!=next->density) apply_mesh_mass(body,next);
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
    p=b3Add(p,b3Neg(grid_offset(&body->spec,q)));
    *out = (KatlaPose){body->spec.id,{p.x,p.y,p.z},{q.v.x,q.v.y,q.v.z,q.s},{v.x,v.y,v.z}};
}
KATLA_API int32_t katla_box3d_overlaps(void* owner, uint64_t* ids, int32_t capacity) {
    KatlaBody* body = owner;
    if (!body->spec.sensor || B3_IS_NULL(body->shape)) return 0;
    uint32_t shapes=body->pieces ? body->piece_count:1; int count=0;
    for (uint32_t i=0;i<shapes;i++) { b3ShapeId shape=body->pieces ? body->pieces[i]:body->shape; count+=b3Shape_GetSensorCapacity(shape); if (count>1000000) return -1; }
    if (!count) return 0;
    b3ShapeId* visitors=malloc((size_t)count*sizeof(*visitors)); uint64_t* unique=malloc((size_t)count*sizeof(*unique));
    if (!visitors || !unique) { free(visitors); free(unique); return -1; }
    int valid=0;
    for (uint32_t i=0;i<shapes;i++) {
        b3ShapeId shape=body->pieces ? body->pieces[i]:body->shape; int written=b3Shape_GetSensorData(shape,visitors,count);
        for (int j=0;j<written;j++) {
            if (!b3Shape_IsValid(visitors[j])) continue;
            KatlaBody* other=b3Shape_GetUserData(visitors[j]); if (!other) continue;
            bool duplicate=false; for (int k=0;k<valid;k++) if (unique[k]==other->spec.id) { duplicate=true; break; }
            if (!duplicate) unique[valid++]=other->spec.id;
        }
    }
    if (ids && capacity>=valid) memcpy(ids,unique,(size_t)valid*sizeof(*ids));
    free(visitors); free(unique); return valid;
}

static b3Vec3 local_anchor(const float anchor[3]) { return (b3Vec3){anchor[0],anchor[1],anchor[2]}; }
static bool hinge_interval(const KatlaJointSpec* spec,float* center,float* half_width) {
    if (!spec->has_limits) return false;
    double width=(double)spec->limits[1]-(double)spec->limits[0];
    const double tau=6.283185307179586476925286766559;
    if (width>=tau) return false;
    double midpoint=((double)spec->limits[0]+(double)spec->limits[1])*0.5;
    *center=(float)atan2(sin(midpoint),cos(midpoint));
    *half_width=(float)(width*0.5);
    return true;
}
static b3JointDef joint_base(const KatlaJoint* joint) {
    b3JointDef def = b3DefaultSphericalJointDef().base;
    def.bodyIdA = joint->a->id; def.bodyIdB = joint->b->id;
    def.localFrameA.p = local_anchor(joint->spec.anchor_a);
    def.localFrameB.p = local_anchor(joint->spec.anchor_b);
    if (joint->spec.kind==1) {
        def.localFrameA.q = (b3Quat){{-0.7071067811865475f,0,0},0.7071067811865475f};
        def.localFrameB.q = def.localFrameA.q;
        float center,half_width;
        if (hinge_interval(&joint->spec,&center,&half_width)) {
            b3Quat phase={{0,0,sinf(center*0.5f)},cosf(center*0.5f)};
            def.localFrameA.q=b3MulQuat(def.localFrameA.q,phase);
        }
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
    float rest=spec->has_limits ? (float)(((double)spec->limits[0]+(double)spec->limits[1])*0.5) : 0.5f;
    if (spec->kind==2 && !isfinite(rest)) return NULL;
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
    case 1: { b3RevoluteJointDef def=b3DefaultRevoluteJointDef(); def.base=base; float center,half_width; def.enableLimit=hinge_interval(&joint->spec,&center,&half_width);
        def.lowerAngle=def.enableLimit ? -half_width : 0; def.upperAngle=def.enableLimit ? half_width : 0; joint->id=b3CreateRevoluteJoint(joint->world->id,&def); break; }
    case 2: { b3DistanceJointDef def=b3DefaultDistanceJointDef(); def.base=base; def.enableSpring=true; def.enableLimit=false;
        def.length=joint->spec.has_limits ? (float)(((double)joint->spec.limits[0]+(double)joint->spec.limits[1])*0.5) : 0.5f;
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

typedef struct { uint64_t id; float point[3],normal[3],distance; uint32_t hit; } KatlaRay;
typedef struct { KatlaRay* result; b3Vec3 direction; b3Pos origin; float distance; bool sensors; } RayContext;
_Static_assert(sizeof(KatlaRay)==40,"Ray ABI");
static bool query_accept(b3ShapeId shape,const RayContext* context) {
    KatlaBody* body=b3Shape_GetUserData(shape);
    return body && (context->sensors || !body->spec.sensor);
}
static bool ray_inside(b3ShapeId shape,void* opaque) {
    RayContext* context=opaque; if (!query_accept(shape,context)) return true;
    KatlaBody* body=b3Shape_GetUserData(shape); KatlaRay* out=context->result;
    if (!out->hit || body->spec.id<out->id) {
        out->id=body->spec.id; out->hit=1; out->distance=0;
        out->point[0]=context->origin.x; out->point[1]=context->origin.y; out->point[2]=context->origin.z;
        b3Vec3 normal=b3Neg(b3Normalize(context->direction));
        out->normal[0]=normal.x; out->normal[1]=normal.y; out->normal[2]=normal.z;
    }
    return true;
}
static float ray_closest(b3ShapeId shape,b3Pos point,b3Vec3 normal,float fraction,uint64_t material,int triangle,int child,void* opaque) {
    (void)material; (void)triangle; (void)child;
    RayContext* context=opaque; if (!query_accept(shape,context)) return -1;
    KatlaBody* body=b3Shape_GetUserData(shape); KatlaRay* out=context->result; float distance=fraction*context->distance;
    if (!out->hit || distance<out->distance || (distance==out->distance && body->spec.id<out->id)) {
        out->id=body->spec.id; out->hit=1; out->distance=distance;
        out->point[0]=point.x; out->point[1]=point.y; out->point[2]=point.z;
        out->normal[0]=normal.x; out->normal[1]=normal.y; out->normal[2]=normal.z;
    }
    return fraction;
}
KATLA_API int32_t katla_box3d_raycast(void* owner,const float origin[3],const float direction[3],float distance,uint32_t layers,uint32_t mask,uint32_t sensors,KatlaRay* result) {
    KatlaWorld* world=owner; if (!world || !result || !b3World_IsValid(world->id)) return 0;
    memset(result,0,sizeof(*result));
    RayContext context={result,{direction[0],direction[1],direction[2]},{origin[0],origin[1],origin[2]},distance,sensors!=0};
    b3QueryFilter filter=b3DefaultQueryFilter(); filter.categoryBits=layers; filter.maskBits=mask;
    b3Vec3 local={0,0,0}; b3ShapeProxy point={.points=&local,.count=1,.radius=0};
    b3World_OverlapShape(world->id,context.origin,&point,filter,ray_inside,&context);
    if (!result->hit) b3World_CastRay(world->id,context.origin,b3MulSV(distance,context.direction),filter,ray_closest,&context);
    return 1;
}
KATLA_API int32_t katla_box3d_body_motion(void* owner,uint32_t kind,const float vector[3]) {
    KatlaBody* body=owner; if (!body || !b3Body_IsValid(body->id) || kind>2) return 0;
    b3Vec3 value={vector[0],vector[1],vector[2]};
    if (kind==0) b3Body_SetLinearVelocity(body->id,value);
    else if (kind==1) b3Body_ApplyForceToCenter(body->id,value,true);
    else b3Body_ApplyLinearImpulseToCenter(body->id,value,true);
    return 1;
}

// Queries share the native narrow phase with simulation; proxies stay within Box3D's 64-point limit.
typedef struct { RayContext ray; uint64_t* ids; int count,capacity; bool overflow; } ShapeQuery;
static bool shape_overlap(b3ShapeId shape,void* opaque) {
    ShapeQuery* query=opaque;
    if (!query_accept(shape,&query->ray)) return true;
    uint64_t id=((KatlaBody*)b3Shape_GetUserData(shape))->spec.id;
    for (int i=0;i<query->count;i++) if (query->ids[i]==id) return true;
    if (query->count==query->capacity) { query->overflow=true; return false; }
    query->ids[query->count++]=id; return true;
}
static void shape_proxy_query(KatlaWorld* world,const KatlaBodySpec* shape,ShapeQuery* query,
                              b3Vec3* points,int count,float radius,b3QueryFilter filter) {
    b3Quat rotation={{shape->rotation[0],shape->rotation[1],shape->rotation[2]},shape->rotation[3]};
    for (int i=0;i<count;i++) points[i]=b3RotateVector(rotation,points[i]);
    b3ShapeProxy proxy={.points=points,.count=count,.radius=radius};
    if (!query->ray.result) { b3World_OverlapShape(world->id,query->ray.origin,&proxy,filter,shape_overlap,query); return; }
    b3World_OverlapShape(world->id,query->ray.origin,&proxy,filter,ray_inside,&query->ray);
    if (!query->ray.result->hit || query->ray.result->distance>0)
        b3World_CastShape(world->id,query->ray.origin,&proxy,b3MulSV(query->ray.distance,query->ray.direction),filter,ray_closest,&query->ray);
}
static b3Vec3 height_point(const KatlaBodySpec* shape,uint32_t row,uint32_t col) {
    return (b3Vec3){((float)col-.5f*(shape->cols-1))*shape->height_scale[0],
                   shape->heights[(uint64_t)row*shape->cols+col]*shape->height_scale[1],
                   ((float)row-.5f*(shape->rows-1))*shape->height_scale[2]};
}
KATLA_API int32_t katla_box3d_shape_query(void* owner,const KatlaBodySpec* shape,const float direction[3],float distance,
    uint32_t layers,uint32_t mask,uint32_t sensors,KatlaRay* result,uint64_t* ids,int32_t capacity) {
    KatlaWorld* world=owner;
    if (!world || !shape || !b3World_IsValid(world->id) || shape->shape==5 || capacity<0 || (!result && capacity && !ids)) return -1;
    if (result) memset(result,0,sizeof(*result));
    ShapeQuery query={.ray={result,{direction[0],direction[1],direction[2]},
        {shape->position[0],shape->position[1],shape->position[2]},distance,sensors!=0},.ids=ids,.capacity=capacity};
    b3QueryFilter filter=b3DefaultQueryFilter(); filter.categoryBits=layers; filter.maskBits=mask;
    b3Vec3 points[B3_MAX_SHAPE_CAST_POINTS];
    if (shape->shape==0) {
        for (int i=0;i<8;i++) points[i]=(b3Vec3){(i&1 ? 1:-1)*shape->half_extents[0],(i&2 ? 1:-1)*shape->half_extents[1],(i&4 ? 1:-1)*shape->half_extents[2]};
        shape_proxy_query(world,shape,&query,points,8,0,filter);
    } else if (shape->shape==1 || shape->shape==2) {
        points[0]=(b3Vec3){0,shape->shape==2 ? -shape->half_height:0,0}; points[1]=(b3Vec3){0,shape->half_height,0};
        shape_proxy_query(world,shape,&query,points,shape->shape==2 ? 2:1,shape->radius,filter);
    } else if (shape->shape==4) {
        b3HullData* hull=b3CreateHull((const b3Vec3*)shape->vertices,(int)shape->vertex_count,(int)shape->vertex_count);
        if (!hull) return -1;
        const b3Vec3* vertices=b3GetHullPoints(hull);
        if (hull->vertexCount<=B3_MAX_SHAPE_CAST_POINTS) {
            memcpy(points,vertices,(size_t)hull->vertexCount*sizeof(*points));
            shape_proxy_query(world,shape,&query,points,hull->vertexCount,0,filter);
        } else {
            const b3HullHalfEdge* edges=b3GetHullEdges(hull); const b3HullFace* faces=b3GetHullFaces(hull);
            for (int face=0;face<hull->faceCount;face++) {
                int start=faces[face].edge,edge=edges[start].next;
                while (edges[edge].next!=start) {
                    points[0]=hull->center; points[1]=vertices[edges[start].origin];
                    points[2]=vertices[edges[edge].origin]; points[3]=vertices[edges[edges[edge].next].origin];
                    shape_proxy_query(world,shape,&query,points,4,0,filter); edge=edges[edge].next;
                }
            }
        }
        b3DestroyHull(hull);
    } else if (shape->shape==3) {
        for (uint32_t i=0;i<shape->index_count;i+=3) {
            for (int j=0;j<3;j++) { const float* v=shape->vertices[shape->indices[i+j]]; points[j]=(b3Vec3){v[0],v[1],v[2]}; }
            shape_proxy_query(world,shape,&query,points,3,0,filter);
        }
    } else if (shape->shape==6) {
        for (uint32_t row=0;row+1<shape->rows;row++) for (uint32_t col=0;col+1<shape->cols;col++) {
            points[0]=height_point(shape,row,col); points[1]=height_point(shape,row+1,col); points[2]=height_point(shape,row,col+1);
            shape_proxy_query(world,shape,&query,points,3,0,filter);
            points[0]=height_point(shape,row+1,col+1); points[1]=height_point(shape,row,col+1); points[2]=height_point(shape,row+1,col);
            shape_proxy_query(world,shape,&query,points,3,0,filter);
        }
    } else return -1;
    if (result && result->hit) {
        b3Pos origin=b3Add(query.ray.origin,b3MulSV(result->distance,query.ray.direction));
        result->point[0]=origin.x; result->point[1]=origin.y; result->point[2]=origin.z;
    }
    return query.overflow ? -1 : result ? 0 : query.count;
}

typedef struct { uint64_t a,b; float point[3],normal[3],separation,normal_impulse; } KatlaContact;
_Static_assert(sizeof(KatlaContact)==48,"Contact ABI");
KATLA_API int32_t katla_box3d_body_contacts(void* owner,KatlaContact* output,int32_t capacity) {
    KatlaBody* body=owner;
    if (!body || !b3Body_IsValid(body->id) || capacity<0 || (capacity && !output)) return -1;
    int count=b3Body_GetContactCapacity(body->id);
    if (count<0 || count>1000000) return -1;
    if (!count) return 0;
    b3ContactData* data=malloc((size_t)count*sizeof(*data)); if (!data) return -1;
    int written=b3Body_GetContactData(body->id,data,count),total=0;
    for (int contact=0;contact<written;contact++) {
        const b3ContactData* entry=&data[contact];
        KatlaBody* a=b3Shape_GetUserData(entry->shapeIdA); KatlaBody* b=b3Shape_GetUserData(entry->shapeIdB);
        if (!a || !b || body->spec.id!=(a->spec.id<b->spec.id ? a->spec.id:b->spec.id)) continue;
        b3Pos centerA=b3Body_GetWorldCenterOfMass(a->id),centerB=b3Body_GetWorldCenterOfMass(b->id);
        for (int manifold=0;manifold<entry->manifoldCount;manifold++) {
            const b3Manifold* m=&entry->manifolds[manifold];
            for (int point=0;point<m->pointCount;point++) {
                const b3ManifoldPoint* p=&m->points[point];
                if (output && total<capacity) {
                    b3Vec3 location=b3MulSV(.5f,b3Add(b3Add(centerA,p->anchorA),b3Add(centerB,p->anchorB)));
                    b3Vec3 normal=a->spec.id<b->spec.id ? m->normal:b3Neg(m->normal);
                    output[total]=(KatlaContact){a->spec.id<b->spec.id ? a->spec.id:b->spec.id,a->spec.id<b->spec.id ? b->spec.id:a->spec.id,
                        {location.x,location.y,location.z},{normal.x,normal.y,normal.z},p->separation,p->normalImpulse};
                }
                total++;
            }
        }
    }
    free(data); return total;
}


typedef struct { float start[3],end[3]; } KatlaEdge;
_Static_assert(sizeof(KatlaEdge)==24,"Edge ABI");
KATLA_API int32_t katla_box3d_hull_edges(void* owner,KatlaEdge* output,int32_t capacity) {
    KatlaBody* body=owner;
    if (!body || body->spec.shape!=4 || !b3Body_IsValid(body->id) || capacity<0 || (capacity && !output)) return -1;
    const b3HullData* hull=b3Shape_GetHull(body->shape); if (!hull) return -1;
    const b3Vec3* points=b3GetHullPoints(hull); const b3HullHalfEdge* edges=b3GetHullEdges(hull);
    int count=hull->edgeCount/2;
    if (output && capacity>=count) {
        b3WorldTransform transform=b3Body_GetTransform(body->id);
        for (int i=0;i<count;i++) {
            b3Pos a=b3TransformWorldPoint(transform,points[edges[i*2].origin]);
            b3Pos b=b3TransformWorldPoint(transform,points[edges[i*2+1].origin]);
            output[i]=(KatlaEdge){{a.x,a.y,a.z},{b.x,b.y,b.z}};
        }
    }
    return count;
}
