//! Adapt pinned Box3D validation and joint ranges without modifying upstream sources.
package katla_build

adapt_box :: proc(source,folder:string) {
    text:=string(read(join(source,"src/hull.c")))
    prefix,tail,found:=cut(text,"bool b3IsValidHull( const b3HullData* hull )")
    require(found,"Box3D hull validation signature changed")
    original,suffix,closed:=cut(tail,"\n#else"); require(closed,"Box3D hull validation boundary changed")
    edited:=replace_once(original,"\tint f = hull->faceCount;","\tint f = hull->faceCount;\n\tbool planar = v == 3 && e == 3 && f == 2 && hull->volume == 0.0f && hull->innerRadius == 0.0f;")
    edited=replace_once(edited,"if ( b3PlaneSeparation( plane, hull->center ) >= 0.0f )","if ( planar ? fabsf( b3PlaneSeparation( plane, hull->center ) ) > B3_LINEAR_SLOP : b3PlaneSeparation( plane, hull->center ) >= 0.0f )")
    edited=replace_once(edited,"if ( hull->volume <= 0.0f )","if ( !planar && hull->volume <= 0.0f )")
    edited=replace_once(edited,"if ( hull->innerRadius <= 0.0f )","if ( !planar && hull->innerRadius <= 0.0f )")
    text=cat(prefix,"bool b3IsValidHull( const b3HullData* hull )",edited,"\n#else",suffix)
    text=replace_once(text,`#include "shape.h"`,`#include "shape.h"
#include "simd.h"`)
    signature:="b3CastOutput b3RayCastHull( const b3HullData* shape, const b3RayCastInput* input )\n{"
    text=replace_once(text,signature,cat(signature,"\n    if (shape->vertexCount == 3 && shape->faceCount == 2 && shape->volume == 0) {\n        const b3Vec3* p=b3GetHullPoints(shape);\n        float fraction=b3IntersectRayTriangle(b3LoadV(&input->origin.x),b3LoadV(&input->translation.x),b3LoadV(&p[0].x),b3LoadV(&p[1].x),b3LoadV(&p[2].x));\n        b3CastOutput hit={0};\n        if (fraction < 1.0f && fraction <= input->maxFraction) {\n            hit.hit=true; hit.fraction=fraction; hit.point=b3Add(input->origin,b3MulSV(fraction,input->translation));\n            hit.normal=b3Normalize(b3Cross(b3Sub(p[1],p[0]),b3Sub(p[2],p[0])));\n        }\n        return hit;\n    }\n"))
    write(join(folder,"hull.c"),text)
    for name in ([]string{"joint.c","distance_joint.c","revolute_joint.c"}) {
        text=string(read(join(source,"src",name)))
        if name=="joint.c" {
            text=replace_once(text,"B3_ASSERT( b3IsValidFloat( def->length ) && def->length > 0.0f );","B3_ASSERT( b3IsValidFloat( def->length ) );")
            text=replace_once(text,"joint->distanceJoint.length = b3MaxFloat( def->length, B3_LINEAR_SLOP );","joint->distanceJoint.length = def->length;")
            for bound in ([]string{"lower","upper"}) { text=replace_once(text,cat("joint->revoluteJoint.",bound,"Angle = b3ClampFloat( ",bound,"Angle, -0.99f * B3_PI, 0.99f * B3_PI );"),cat("joint->revoluteJoint.",bound,"Angle = ",bound,"Angle;")) }
        } else if name=="distance_joint.c" { text=replace_once(text,"joint->length = b3ClampFloat( length, B3_LINEAR_SLOP, B3_HUGE );","B3_ASSERT( b3IsValidFloat( length ) );\n\tjoint->length = length;") }
        else { for bound in ([]string{"lower","upper"}) { text=replace_once(text,cat("base->revoluteJoint.",bound,"Angle = b3ClampFloat( ",bound,"Angle, -0.99f * B3_PI, 0.99f * B3_PI );"),cat("base->revoluteJoint.",bound,"Angle = ",bound,"Angle;")) } }
        write(join(folder,name),text)
    }
}
