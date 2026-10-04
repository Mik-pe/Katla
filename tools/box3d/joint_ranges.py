"""Preserve canonical scene joint ranges in a clean pinned Box3D source build."""
from pathlib import Path


def replace_once(text: str, before: str, after: str) -> str:
    if text.count(before) != 1:
        raise SystemExit("Pinned Box3D joint range contract changed")
    return text.replace(before, after, 1)


def prepare(source: Path, output: Path) -> dict[str, Path]:
    output.mkdir(parents=True, exist_ok=True)
    results = {}
    for name in ("joint.c", "distance_joint.c", "revolute_joint.c"):
        text = (source / "src" / name).read_text()
        if name == "joint.c":
            text = replace_once(text, "B3_ASSERT( b3IsValidFloat( def->length ) && def->length > 0.0f );", "B3_ASSERT( b3IsValidFloat( def->length ) );")
            text = replace_once(text, "joint->distanceJoint.length = b3MaxFloat( def->length, B3_LINEAR_SLOP );", "joint->distanceJoint.length = def->length;")
            for bound in ("lower", "upper"):
                text = replace_once(text, f"joint->revoluteJoint.{bound}Angle = b3ClampFloat( {bound}Angle, -0.99f * B3_PI, 0.99f * B3_PI );", f"joint->revoluteJoint.{bound}Angle = {bound}Angle;")
        elif name == "distance_joint.c":
            text = replace_once(text, "joint->length = b3ClampFloat( length, B3_LINEAR_SLOP, B3_HUGE );", "B3_ASSERT( b3IsValidFloat( length ) );\n\tjoint->length = length;")
        else:
            for bound in ("lower", "upper"):
                text = replace_once(text, f"base->revoluteJoint.{bound}Angle = b3ClampFloat( {bound}Angle, -0.99f * B3_PI, 0.99f * B3_PI );", f"base->revoluteJoint.{bound}Angle = {bound}Angle;")
        target = output / name
        target.write_text(text)
        results[name] = target
    return results
