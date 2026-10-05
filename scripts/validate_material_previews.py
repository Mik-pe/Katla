#!/usr/bin/env python3
"""Validate material thumbnails in native interaction GPU readback captures."""
import argparse
import hashlib
import json
import math
from pathlib import Path
from PIL import Image, ImageChops, ImageStat


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('--baseline', type=Path, help='Pre-antialiasing default scene capture directory')
    args = parser.parse_args()
    receipt = json.loads((args.directory / 'receipt.json').read_text())
    assert receipt['complete'] and all(check['passed'] for check in receipt['checks']), 'Native walkthrough failed'
    regions = receipt['preview_regions']
    frame = Image.open(args.directory / '01_default.png').convert('RGB')
    hashes = []
    for index, bounds in enumerate(regions['library']):
        assert bounds is not None, f'Preview {index} was not mounted'
        sphere = frame.crop(bounds)
        assert max(ImageStat.Stat(sphere).stddev) > 10, f'Preview {index} is blank/flat'
        hashes.append(hashlib.sha256(sphere.tobytes()).hexdigest())
    assert len(set(hashes)) == 6, 'Material previews are duplicated'
    bounds = regions['inspector']
    assert bounds is not None, 'Inspector has no material preview'
    samples = [Image.open(args.directory / f'{name}.png').convert('RGB').crop(bounds) for name in (
        '11_material_preset', '12_material_drag', '13_material_undo', '14_material_redo')]
    change = sum(ImageStat.Stat(ImageChops.difference(samples[0], samples[1])).mean) / 3
    assert change > 2, f'Roughness drag did not change GPU preview pixels: {change}'
    assert ImageChops.difference(samples[0], samples[2]).getbbox() is None, 'Undo did not restore preview pixels'
    assert ImageChops.difference(samples[1], samples[3]).getbbox() is None, 'Redo did not restore preview pixels'
    proof = {'passed': True, 'distinct_material_previews': len(set(hashes)),
             'roughness_pixel_change_mean': change, 'undo_exact': True, 'redo_exact': True,
             'library_hashes': hashes, 'preview_regions': regions}
    imported_bounds = regions['imported']
    assert imported_bounds is not None, 'Imported material preview is missing'
    imported = [Image.open(args.directory / f'{name}.png').convert('RGB').crop(imported_bounds) for name in (
        '23_imported_maps', '24_imported_factors', '25_imported_restored')]
    map_change = sum(ImageStat.Stat(ImageChops.difference(imported[0], imported[1])).mean) / 3
    assert map_change > 2, f'Imported maps did not change native preview pixels: {map_change}'
    assert ImageChops.difference(imported[0], imported[2]).getbbox() is None, 'Map restoration did not regenerate identical preview pixels'
    proof['imported_map_pixel_change_mean'] = map_change
    proof['imported_map_restoration_exact'] = True
    if args.baseline is not None:
        before = Image.open(args.baseline / '01_default.png').convert('RGB')
        assert before.size == frame.size == (2560, 1440), 'Edge fixture requires the default 1280×720 layout at 2× DPI'
        def edge_jump(image):
            jumps = []
            for x in range(720, 950):
                column = [image.getpixel((x, y)) for y in range(370, 425)]
                jumps.append(max(math.sqrt(sum((a - b) ** 2 for a, b in zip(first, second)))
                                 for first, second in zip(column, column[1:])))
            return sum(jumps) / len(jumps)
        old_jump, new_jump = edge_jump(before), edge_jump(frame)
        assert new_jump < old_jump * 0.95, f'Static floor silhouette did not become smoother: {old_jump} → {new_jump}'
        sky = (1500, 170, 1700, 230)
        assert ImageChops.difference(before.crop(sky), frame.crop(sky)).getbbox() is None, 'Flat sky changed under scene filtering'
        proof['scene_edge'] = {'before_jump': old_jump, 'after_jump': new_jump,
                               'reduction_percent': (1 - new_jump / old_jump) * 100, 'flat_sky_exact': True}
    (args.directory / 'material-preview-pixels.json').write_text(json.dumps(proof, indent=2) + '\n')
    print(json.dumps({key: value for key, value in proof.items() if key not in ('library_hashes', 'preview_regions')}))


if __name__ == '__main__':
    main()
