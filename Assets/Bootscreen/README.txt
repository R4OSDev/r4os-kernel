Bootscreen Asset
================

BOOTSCREEN.BMP is the editable bootscreen source image.

Contract:

- exact resolution: 1280x720;
- uncompressed 24-bpp or 32-bpp BMP;
- no palette;
- no BMP compression.

The BMP is build input, not a boot-time file. The build converts it to
Generated/BOOTSCREEN.R4B and embeds that artifact in the kernel. The progress
bar remains dynamically rendered over the image.

On a 1280x720 framebuffer the image starts at 0,0. Larger framebuffers center
it pixel-perfect without scaling; at 1920x1080 it starts at 320,180. The image
is not shown on smaller framebuffers.

BOOTSCREEN.BMP is mandatory. The kernel has no BMP decoder and never reads the
source image at boot time. Generated/BOOTSCREEN.R4B is rebuilt and remains
unversioned.
