#include "bridge.c"

static int test_partial_publication_restores_sleep_and_motion(int unpublished_first) {
    KatlaWorld* world=katla_box3d_create();
    if (!world) return 1;
    KatlaBodySpec spec={.type=0,.shape=1,.rotation={0,0,0,1},.radius=.1f,.density=1,.layers=1,.mask=1};
    spec.id=1; KatlaBody* a=katla_box3d_body_create(world,&spec);
    spec.id=2; spec.position[0]=10; spec.velocity[0]=7; KatlaBody* b=katla_box3d_body_create(world,&spec);
    spec.id=3; spec.position[0]=20; spec.velocity[0]=0; KatlaBody* c=katla_box3d_body_create(world,&spec);
    if (!a || !b || !c) return 2;
    b3Body_SetAwake(a->id,false);
    KatlaJointSpec descriptor={.id=10,.a=1,.b=2,.kind=0};
    KatlaJoint* published=katla_box3d_joint_prepare(world,&descriptor,a,b);
    descriptor.id=11; descriptor.b=3;
    KatlaJoint* unpublished=katla_box3d_joint_prepare(world,&descriptor,a,c);
    if (!published || !unpublished || !katla_box3d_joint_publish(published)) return 3;
    KatlaJoint* first=unpublished_first ? unpublished : published;
    KatlaJoint* second=unpublished_first ? published : unpublished;
    katla_box3d_joint_rollback(first); katla_box3d_joint_rollback(second);
    katla_box3d_joint_restore(first); katla_box3d_joint_restore(second);
    int failed=world->joints!=NULL || b3Body_GetLinearVelocity(b->id).x!=7
        || b3Body_IsAwake(a->id) || !b3Body_IsAwake(b->id);
    katla_box3d_body_destroy(a); katla_box3d_body_destroy(b); katla_box3d_body_destroy(c);
    katla_box3d_destroy(world);
    return failed;
}

static int test_foreign_world_joint_endpoints_are_rejected(void) {
    KatlaWorld* first=katla_box3d_create(); KatlaWorld* second=katla_box3d_create();
    if (!first || !second) return 1;
    KatlaBodySpec spec={.id=1,.type=0,.shape=1,.rotation={0,0,0,1},.radius=.1f,.density=1};
    KatlaBody* a=katla_box3d_body_create(first,&spec); spec.id=2;
    KatlaBody* b=katla_box3d_body_create(second,&spec);
    if (!a || !b) return 2;
    KatlaJointSpec descriptor={.id=10,.a=1,.b=2,.kind=0};
    KatlaJoint* prepared=katla_box3d_joint_prepare(first,&descriptor,a,b);
    int failed=prepared!=NULL;
    if (prepared) katla_box3d_joint_destroy(prepared);
    katla_box3d_body_destroy(a); katla_box3d_body_destroy(b);
    katla_box3d_destroy(first); katla_box3d_destroy(second);
    return failed;
}

static int test_full_periodic_hinge_ranges(void) {
    const float ranges[][2]={{3,4},{4,5},{-5,-4},{-.999f*B3_PI,.999f*B3_PI},{-B3_PI,B3_PI},{-4,4},{1e30f,1e30f}};
    for (unsigned i=0;i<sizeof(ranges)/sizeof(ranges[0]);++i) {
        KatlaWorld* world=katla_box3d_create();
        KatlaBodySpec spec={.id=1,.type=1,.shape=1,.rotation={0,0,0,1},.radius=.1f,.density=1};
        KatlaBody* a=katla_box3d_body_create(world,&spec); spec.id=2; spec.type=0; spec.position[1]=-1;
        KatlaBody* b=katla_box3d_body_create(world,&spec);
        KatlaJointSpec descriptor={.id=10,.a=1,.b=2,.kind=1,.has_limits=1,.anchor_a={0,-1,0},.limits={ranges[i][0],ranges[i][1]}};
        KatlaJoint* joint=katla_box3d_joint_prepare(world,&descriptor,a,b);
        if (!joint || !katla_box3d_joint_publish(joint)) return 1;
        float center,half;
        bool limited=hinge_interval(&descriptor,&center,&half);
        if (b3RevoluteJoint_IsLimitEnabled(joint->id)!=limited) return 2;
        if (limited && (b3RevoluteJoint_GetLowerLimit(joint->id)!=-half || b3RevoluteJoint_GetUpperLimit(joint->id)!=half)) return 3;
        katla_box3d_joint_destroy(joint); katla_box3d_body_destroy(a); katla_box3d_body_destroy(b); katla_box3d_destroy(world);
    }
    return 0;
}

static int test_distance_rest_lengths_are_exact(void) {
    const float ranges[][2]={{0,0},{.001f,.001f},{-.001f,-.001f},{-1,1},{3.402823466e38f,3.402823466e38f},{-3.402823466e38f,-3.402823466e38f},{-3.402823466e38f,3.402823466e38f}};
    for (unsigned i=0;i<sizeof(ranges)/sizeof(ranges[0]);++i) {
        KatlaWorld* world=katla_box3d_create();
        KatlaBodySpec spec={.id=1,.type=1,.shape=1,.rotation={0,0,0,1},.radius=.1f,.density=1};
        KatlaBody* a=katla_box3d_body_create(world,&spec); spec.id=2; spec.type=0; spec.position[1]=-1;
        KatlaBody* b=katla_box3d_body_create(world,&spec);
        KatlaJointSpec descriptor={.id=10,.a=1,.b=2,.kind=2,.has_limits=1,.limits={ranges[i][0],ranges[i][1]}};
        KatlaJoint* joint=katla_box3d_joint_prepare(world,&descriptor,a,b);
        if (!joint || !katla_box3d_joint_publish(joint)) return 1;
        float expected=(float)(((double)ranges[i][0]+ranges[i][1])*.5);
        if (b3DistanceJoint_GetLength(joint->id)!=expected) return 2;
        b3DistanceJoint_SetLength(joint->id,expected);
        if (b3DistanceJoint_GetLength(joint->id)!=expected) return 3;
        katla_box3d_joint_destroy(joint); katla_box3d_body_destroy(a); katla_box3d_body_destroy(b); katla_box3d_destroy(world);
    }
    return 0;
}

int main(void) {
    return test_partial_publication_restores_sleep_and_motion(0)
        || test_partial_publication_restores_sleep_and_motion(1)
        || test_foreign_world_joint_endpoints_are_rejected()
        || test_full_periodic_hinge_ranges()
        || test_distance_rest_lengths_are_exact();
}
