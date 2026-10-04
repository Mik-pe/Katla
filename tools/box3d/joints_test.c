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

int main(void) {
    return test_partial_publication_restores_sleep_and_motion(0)
        || test_partial_publication_restores_sleep_and_motion(1)
        || test_foreign_world_joint_endpoints_are_rejected();
}
