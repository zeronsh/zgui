# zgx: makes the JPEGs `tests/scaled.rs` decodes at 1/2, 1/4 and 1/8.
# Run from this directory: python3 make_fixtures.py (needs Pillow and
# libjpeg-turbo's jpegtran).
#
# `photo` is photo-like: smooth gradients, soft-edged shapes, a few thin
# lines and grain. The small sizes are crops of the large one, so they keep
# its detail (they straddle the edge of a bar) and have partial blocks and
# MCUs on both edges. `graphic` is not:
# flat saturated discs with pixel-sharp edges and one-pixel white lines, the
# worst case for chroma; its files get every check but the error bounds.
import random
import subprocess

from PIL import Image, ImageChops, ImageDraw, ImageFilter


def base(w, h):
    r = Image.linear_gradient("L").resize((w, h)).rotate(30, expand=False)
    g = Image.radial_gradient("L").resize((w, h))
    b = ImageChops.invert(Image.linear_gradient("L").transpose(Image.Transpose.ROTATE_90).resize((w, h)))
    return Image.merge("RGB", (r, g, b))


def shapes(img, rnd, soft):
    w, h = img.size
    layer = img.copy()
    d = ImageDraw.Draw(layer)
    for _ in range(max(3, w * h // 20000)):
        x, y = rnd.randrange(w), rnd.randrange(h)
        s = rnd.randrange(2, max(3, min(w, h) // 3))
        c = tuple(rnd.randrange(256) for _ in range(3))
        if soft:  # photo colours: less saturated
            c = tuple((v + 128) // 2 for v in c)
        if rnd.random() < 0.5:
            d.ellipse((x - s, y - s, x + s, y + s), fill=c)
        else:
            d.line((x, y, x + rnd.randrange(-s, s + 1), y + rnd.randrange(-s, s + 1)), fill=c, width=max(1, s // 8))
    return layer.filter(ImageFilter.GaussianBlur(3.0 if soft else 1.2))


def grain(img, sigma):
    noise = Image.effect_noise(img.size, sigma).convert("RGB")
    return ImageChops.add(img, noise, scale=1.0, offset=-128)


def photo(w, h, seed=7):
    rnd = random.Random(seed)
    img = shapes(base(w, h), rnd, soft=True)
    d = ImageDraw.Draw(img)
    for _ in range(12):  # thin detail: twigs, wires
        x, y = rnd.randrange(w), rnd.randrange(h)
        c = tuple(rnd.randrange(40, 220) for _ in range(3))
        d.line((x, y, x + rnd.randrange(-80, 81), y + rnd.randrange(-80, 81)), fill=c, width=2)
    return grain(img.filter(ImageFilter.GaussianBlur(0.6)), 4)


def graphic(w, h, seed=7):
    img = grain(shapes(base(w, h), random.Random(seed), soft=False), 4)
    d = ImageDraw.Draw(img)
    for x in range(0, w, 41):
        d.line((x, 0, x, h), fill=(250, 250, 250))
    return img


def save(img, name, **kw):
    img.save(name, "JPEG", quality=75, **kw)


big = photo(1001, 667)
crops = {(w, h): big.crop((600 - w // 2, 275 - h // 2, 600 - w // 2 + w, 275 - h // 2 + h)) for w, h in [(37, 23), (17, 11), (7, 5)]}
for (w, h), img in [((1001, 667), big)] + list(crops.items()):
    for sub, tag in [(0, "444"), (2, "420"), (1, "422")]:
        save(img, f"p{tag}_{w}x{h}.jpg", subsampling=sub)
    save(img.convert("L"), f"grey_{w}x{h}.jpg")

small = crops[(37, 23)]
save(big, "prog420_1001x667.jpg", subsampling=2, progressive=True)
save(small, "prog444_37x23.jpg", subsampling=0, progressive=True)
save(big.convert("L"), "proggrey_1001x667.jpg", progressive=True)
save(big, "rst420_1001x667.jpg", subsampling=2, restart_marker_blocks=5)
save(small, "rst444_37x23.jpg", subsampling=0, restart_marker_rows=1)
save(big.resize((501, 333), Image.Resampling.LANCZOS).convert("CMYK"), "cmyk_501x333.jpg")
save(small.convert("CMYK"), "cmyk_37x23.jpg")
save(graphic(501, 333), "graphic420_501x333.jpg", subsampling=2)
save(graphic(37, 23), "graphic420_37x23.jpg", subsampling=2)

# Samplings Pillow can't write, from libjpeg-turbo's cjpeg: 4:4:0, 4:1:1 (a
# ratio of 4, which 1/2 can't make by a larger IDCT and up-samples), and Cb
# and Cr sampled differently.
big.save("big.ppm")
small.save("small.ppm")
for tag, sample in [("440", "1x2"), ("411", "4x1"), ("mixed", "2x2,1x1,1x2")]:
    for src, size in [("big.ppm", "1001x667"), ("small.ppm", "37x23")]:
        subprocess.run(["cjpeg", "-quality", "75", "-sample", sample, "-outfile", f"p{tag}_{size}.jpg", src], check=True)

# A baseline file with one scan per component (zune-jpeg's
# `finish_baseline_decoding` path): jpegtran rewrites the scans losslessly.
# Only 4:4:4: zune-jpeg 0.5.15 decodes a subsampled one wrongly at full size.
with open("scans.txt", "w") as f:
    f.write("0: 0 63 0 0;\n1: 0 63 0 0;\n2: 0 63 0 0;\n")
for name in ["p444_1001x667", "p444_37x23"]:
    subprocess.run(["jpegtran", "-scans", "scans.txt", "-outfile", f"scans_{name}.jpg", f"{name}.jpg"], check=True)

import os

for f in ["scans.txt", "big.ppm", "small.ppm"]:
    os.remove(f)
