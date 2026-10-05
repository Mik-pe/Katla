The progressive JPEG fixtures are authored 31×19 RGB patterns encoded with
libjpeg through Pillow at quality 87 (sampling 0/1/2). The restart fixture uses
quality 90, 4:2:0 and restart_marker_blocks=2. Their .rgba files are independent
libjpeg decoded RGBA references. These checked-in fixtures need no generator or
Python at build/test time; decoder tests allow at most three channel levels of
IDCT/upsampling rounding difference.

PNG16 and TIFF fixtures retain integer/float samples, endian, planar, tile and
orientation coverage. Original image values are asserted in Odin tests.
