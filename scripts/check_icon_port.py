#!/usr/bin/env python3
"""Check compiled Odin icon values and precache order against every Rust export."""

from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def main():
    source = (ROOT / "katla_icons/src/lib.rs").read_text()
    pairs = re.findall(r"pub const (\w+): char = '\\u\{([0-9A-Fa-f]+)\}';", source)
    reference = [(name, int(code, 16)) for name, code in pairs]
    values = dict(reference)
    common_source = source.split("pub fn common_icons()")[1].split("#[cfg(test)]")[0]
    common = [values[name] for name in re.findall(r"Self::(\w+)", common_source)]
    with tempfile.TemporaryDirectory(prefix="katla-icons-") as directory:
        binary = str(Path(directory) / "icons")
        subprocess.run(["odin", "build", "odin/icon_reference", f"-out:{binary}", "-vet", "-strict-style"], cwd=ROOT, check=True)
        output = subprocess.check_output([binary], cwd=ROOT, text=True)
    catalogue, actual_common = output.split("COMMON\n")
    actual = [(name, int(code, 16)) for name, code in (line.split() for line in catalogue.splitlines())]
    if not reference or actual != reference:
        raise AssertionError("Odin icon names, values or catalogue order differ from Rust")
    if [int(code, 16) for code in actual_common.splitlines()] != common:
        raise AssertionError("Odin icon precache order differs from Rust")
    print(f"Icons: {len(reference)} exported values and {len(common)} common entries match Rust")


if __name__ == "__main__":
    main()
