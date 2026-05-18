"""hanabifx icon generator.

A kiku-style hanabi (chrysanthemum firework) bursting on a deep night-sky
squircle. Renders at 4x then downsamples for crisp AA, and builds a .icns
via iconutil.
"""

from __future__ import annotations

import math
import random
import subprocess
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

HERE = Path(__file__).parent
ICONSET = HERE / "hanabifx.iconset"
ICONSET.mkdir(exist_ok=True)

# Render at 4x for AA.
SCALE = 4
BASE = 1024
SIZE = BASE * SCALE

# --- Palette --------------------------------------------------------------
SKY_TOP = (20, 28, 70)        # richer night blue (more contrast)
SKY_BOTTOM = (3, 4, 14)       # near-black

# Hanabi petal palette — punchy, saturated.
PETAL_COLORS = [
    (255, 225, 110),   # gold
    (255, 110, 80),    # coral red
    (255, 70, 140),    # magenta
    (110, 200, 255),   # cyan
    (200, 130, 255),   # violet
]

CORE = (255, 250, 230)
STAR = (255, 255, 245)


def squircle_mask(size: int, radius_ratio: float = 0.2237) -> Image.Image:
    r = int(size * radius_ratio)
    m = Image.new("L", (size, size), 0)
    ImageDraw.Draw(m).rounded_rectangle(
        (0, 0, size - 1, size - 1), radius=r, fill=255,
    )
    return m


def vertical_gradient(size: int, top: tuple, bottom: tuple) -> Image.Image:
    img = Image.new("RGB", (size, size), top)
    px = img.load()
    for y in range(size):
        t = y / (size - 1)
        t = t * t * (3 - 2 * t)
        r = int(top[0] + (bottom[0] - top[0]) * t)
        g = int(top[1] + (bottom[1] - top[1]) * t)
        b = int(top[2] + (bottom[2] - top[2]) * t)
        for x in range(size):
            px[x, y] = (r, g, b)
    return img


def _apply_mask(img: Image.Image, mask: Image.Image) -> Image.Image:
    out = Image.new("RGBA", img.size, (0, 0, 0, 0))
    out.paste(img, (0, 0), mask)
    return out


def draw_starfield(img: Image.Image, n: int, rng: random.Random) -> None:
    """Sparse faint stars across the night sky."""
    d = ImageDraw.Draw(img)
    for _ in range(n):
        x = rng.randint(0, SIZE - 1)
        y = rng.randint(0, int(SIZE * 0.7))
        r = rng.choice([1, 1, 1, 2, 2, 3]) * SCALE // 2
        a = rng.randint(60, 180)
        d.ellipse((x - r, y - r, x + r, y + r), fill=STAR + (a,))


def draw_petal(img: Image.Image, cx: int, cy: int, angle: float,
                length: float, color: tuple, width: int) -> None:
    """Draw one radial spark: a fading streak from center outward, ending
    in a bright tip (the trailing 'star' of the burst)."""
    d = ImageDraw.Draw(img)
    steps = 24
    # Inner gap so streaks don't crowd the bright core.
    r0 = length * 0.18
    r1 = length
    for i in range(steps):
        t0 = i / steps
        t1 = (i + 1) / steps
        rr0 = r0 + (r1 - r0) * t0
        rr1 = r0 + (r1 - r0) * t1
        x0 = cx + rr0 * math.cos(angle)
        y0 = cy + rr0 * math.sin(angle)
        x1 = cx + rr1 * math.cos(angle)
        y1 = cy + rr1 * math.sin(angle)
        # Fade outward but stay visible most of the way; brighten at tip.
        if t1 < 0.85:
            alpha = int(230 * (1 - t1 * 0.55))
        else:
            alpha = int(255 * (1 - (t1 - 0.85) / 0.15))
        w = max(1, int(width * (1 - t1 * 0.35)))
        d.line([(x0, y0), (x1, y1)], fill=color + (alpha,), width=w)

    # Trailing tip spark.
    tip_r = max(2, int(width * 1.1))
    tx = cx + r1 * math.cos(angle)
    ty = cy + r1 * math.sin(angle)
    d.ellipse((tx - tip_r, ty - tip_r, tx + tip_r, ty + tip_r),
              fill=color + (255,))


def draw_hanabi(cx: int, cy: int, radius: float, petals: int,
                 base_color: tuple, rng: random.Random,
                 width: int, color_mix: float = 0.0) -> Image.Image:
    """Render one chrysanthemum burst as its own layer for compositing."""
    layer = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    for i in range(petals):
        a = (i / petals) * 2 * math.pi + rng.uniform(-0.01, 0.01)
        # Slight length jitter for life.
        length = radius * rng.uniform(0.92, 1.0)
        if color_mix > 0 and rng.random() < color_mix:
            color = rng.choice(PETAL_COLORS)
        else:
            color = base_color
        draw_petal(layer, cx, cy, a, length, color, width)
    return layer


def add_glow(layer: Image.Image, blur: float, intensity: float = 1.0) -> Image.Image:
    """Outer glow = blurred copy laid under the original."""
    glow = layer.filter(ImageFilter.GaussianBlur(blur))
    if intensity != 1.0:
        # Boost alpha for visible bloom.
        r, g, b, a = glow.split()
        a = a.point(lambda v: min(255, int(v * intensity)))
        glow = Image.merge("RGBA", (r, g, b, a))
    return Image.alpha_composite(glow, layer)


def draw_core(cx: int, cy: int, r: float) -> Image.Image:
    """Bright central flash with strong falloff."""
    layer = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    for rr, a in [
        (r * 3.0, 35),
        (r * 2.0, 90),
        (r * 1.3, 170),
        (r * 0.8, 235),
        (r * 0.4, 255),
    ]:
        d.ellipse((cx - rr, cy - rr, cx + rr, cy + rr),
                  fill=CORE + (a,))
    return layer.filter(ImageFilter.GaussianBlur(SIZE * 0.004))


def render() -> Image.Image:
    rng = random.Random(7)

    # 1. Sky.
    sky = vertical_gradient(SIZE, SKY_TOP, SKY_BOTTOM)
    mask = squircle_mask(SIZE)
    icon = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    icon.paste(sky, (0, 0), mask)

    # Faint top vignette (a brushed wash of color, like residual smoke).
    wash = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    ImageDraw.Draw(wash).ellipse(
        (int(SIZE * -0.2), int(SIZE * -0.55),
         int(SIZE * 1.2), int(SIZE * 0.45)),
        fill=(70, 90, 180, 35),
    )
    wash = wash.filter(ImageFilter.GaussianBlur(SIZE * 0.05))
    icon = Image.alpha_composite(icon, _apply_mask(wash, mask))

    # 2. Stars.
    stars = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    draw_starfield(stars, n=70, rng=rng)
    icon = Image.alpha_composite(icon, _apply_mask(stars, mask))

    cx = SIZE // 2
    cy = int(SIZE * 0.52)   # slightly below visual center so burst feels grounded

    # 3. Secondary smaller bursts (background, blurred for depth).
    for sx_frac, sy_frac, rad_frac, color in [
        (0.20, 0.28, 0.11, PETAL_COLORS[3]),  # cyan, upper-left
        (0.80, 0.32, 0.09, PETAL_COLORS[1]),  # coral, upper-right
        (0.26, 0.80, 0.08, PETAL_COLORS[4]),  # violet, lower-left
    ]:
        sx = int(SIZE * sx_frac)
        sy = int(SIZE * sy_frac)
        petals = 12
        burst = draw_hanabi(sx, sy, SIZE * rad_frac, petals, color, rng,
                             width=max(3, int(SIZE * 0.005)),
                             color_mix=0.0)
        burst = add_glow(burst, SIZE * 0.008, intensity=1.6)
        burst = burst.filter(ImageFilter.GaussianBlur(SIZE * 0.0015))
        icon = Image.alpha_composite(icon, _apply_mask(burst, mask))

    # 4. Main hanabi — fewer, thicker petals so it reads at 32px.
    main_r = SIZE * 0.40
    main = draw_hanabi(cx, cy, main_r, petals=18,
                        base_color=PETAL_COLORS[0], rng=rng,
                        width=max(5, int(SIZE * 0.011)),
                        color_mix=0.7)

    # Inner shorter ring — thicker too.
    inner_ring = draw_hanabi(cx, cy, main_r * 0.55, petals=14,
                              base_color=PETAL_COLORS[2], rng=rng,
                              width=max(4, int(SIZE * 0.008)),
                              color_mix=0.7)
    main = Image.alpha_composite(main, inner_ring)

    # Strong bloom around the burst.
    main = add_glow(main, SIZE * 0.018, intensity=2.0)
    icon = Image.alpha_composite(icon, _apply_mask(main, mask))

    # 5. Bigger, brighter core flash — anchors the icon at small sizes.
    core = draw_core(cx, cy, SIZE * 0.075)
    icon = Image.alpha_composite(icon, _apply_mask(core, mask))

    # 6. A few scattered embers floating around the main burst.
    embers = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    ed = ImageDraw.Draw(embers)
    for _ in range(30):
        a = rng.uniform(0, 2 * math.pi)
        rr = main_r * rng.uniform(1.02, 1.18)
        ex = cx + rr * math.cos(a)
        ey = cy + rr * math.sin(a)
        sz = rng.choice([2, 2, 3, 4]) * SCALE
        color = rng.choice(PETAL_COLORS)
        ed.ellipse((ex - sz, ey - sz, ex + sz, ey + sz),
                   fill=color + (rng.randint(180, 240),))
    embers = embers.filter(ImageFilter.GaussianBlur(SIZE * 0.002))
    icon = Image.alpha_composite(icon, _apply_mask(embers, mask))

    # 7. Final squircle re-mask (safety).
    icon = _apply_mask(icon, mask)
    return icon


def main() -> None:
    full = render()
    ref = full.resize((BASE, BASE), Image.LANCZOS)
    ref.save(HERE / "icon_1024.png")

    sizes = [
        ("icon_16x16.png", 16),
        ("icon_16x16@2x.png", 32),
        ("icon_32x32.png", 32),
        ("icon_32x32@2x.png", 64),
        ("icon_128x128.png", 128),
        ("icon_128x128@2x.png", 256),
        ("icon_256x256.png", 256),
        ("icon_256x256@2x.png", 512),
        ("icon_512x512.png", 512),
        ("icon_512x512@2x.png", 1024),
    ]
    for name, px in sizes:
        ref.resize((px, px), Image.LANCZOS).save(ICONSET / name)

    icns_path = HERE / "hanabifx.icns"
    subprocess.run(
        ["iconutil", "-c", "icns", str(ICONSET), "-o", str(icns_path)],
        check=True,
    )
    print(f"wrote {icns_path}")
    print(f"wrote {HERE / 'icon_1024.png'}")


if __name__ == "__main__":
    main()
