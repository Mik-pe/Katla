"""Extend pinned Box3D validation for exact zero-volume, two-sided triangle hulls."""
from pathlib import Path


def prepare(source: Path, output: Path) -> Path:
    text = (source / "src/hull.c").read_text()
    start = text.index("bool b3IsValidHull( const b3HullData* hull )")
    end = text.index("\n#else", start)
    original = text[start:end]
    edited = original.replace("\tint f = hull->faceCount;", "\tint f = hull->faceCount;\n\tbool planar = v == 3 && e == 3 && f == 2 && hull->volume == 0.0f && hull->innerRadius == 0.0f;")
    edited = edited.replace("if ( b3PlaneSeparation( plane, hull->center ) >= 0.0f )", "if ( planar ? fabsf( b3PlaneSeparation( plane, hull->center ) ) > B3_LINEAR_SLOP : b3PlaneSeparation( plane, hull->center ) >= 0.0f )")
    edited = edited.replace("if ( hull->volume <= 0.0f )", "if ( !planar && hull->volume <= 0.0f )")
    edited = edited.replace("if ( hull->innerRadius <= 0.0f )", "if ( !planar && hull->innerRadius <= 0.0f )")
    if edited == original or edited.count("!planar") != 2:
        raise SystemExit("Pinned Box3D hull validation contract changed")
    output.mkdir(parents=True, exist_ok=True)
    target = output / "hull.c"
    text = text[:start] + edited + text[end:]
    text = text.replace('#include "shape.h"', '#include "shape.h"\n#include "simd.h"',1)
    signature = "b3CastOutput b3RayCastHull( const b3HullData* shape, const b3RayCastInput* input )\n{"
    triangle_ray = """
    if (shape->vertexCount == 3 && shape->faceCount == 2 && shape->volume == 0) {
        const b3Vec3* p=b3GetHullPoints(shape);
        float fraction=b3IntersectRayTriangle(b3LoadV(&input->origin.x),b3LoadV(&input->translation.x),b3LoadV(&p[0].x),b3LoadV(&p[1].x),b3LoadV(&p[2].x));
        b3CastOutput hit={0};
        if (fraction < 1.0f && fraction <= input->maxFraction) {
            hit.hit=true; hit.fraction=fraction; hit.point=b3Add(input->origin,b3MulSV(fraction,input->translation));
            hit.normal=b3Normalize(b3Cross(b3Sub(p[1],p[0]),b3Sub(p[2],p[0])));
        }
        return hit;
    }
"""
    if text.count(signature) != 1:
        raise SystemExit("Pinned Box3D hull ray-cast contract changed")
    text = text.replace(signature,signature+triangle_ray,1)
    target.write_text(text)
    return target
