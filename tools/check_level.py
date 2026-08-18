"""Overlay a level's collision on its background art, and measure the gap.

The one thing the background-image approach cannot tell you by itself is whether
the collision matches the picture. The art says the ground is here; the IntGrid
says it is there; only playing reveals the mismatch. This draws them together and
reports the offset in pixels.

    python3 tools/check_level.py
    python3 tools/check_level.py --out /tmp/check.png --zoom 2
"""
import argparse
import glob
import json
import pathlib

import numpy as np
from PIL import Image, ImageDraw

LEVELS_DIR = pathlib.Path("assets/levels")
SOLID, PLATFORM, HAZARD = 1, 2, 3
COLOURS = {SOLID: (255, 60, 60, 90), PLATFORM: (255, 170, 40, 100), HAZARD: (255, 0, 200, 110)}


def find_project():
    found = sorted(glob.glob(str(LEVELS_DIR / "*.ldtk")))
    return found[0] if found else None


def ground_row(image, grid):
    """Guess where the art's walkable surface is.

    A lit ledge shows up as the strongest downward-to-upward jump in row
    brightness across the lower half of the picture.
    """
    lum = np.asarray(image.convert("RGB")).astype(float).mean(2)
    rows = lum.mean(1)
    change = np.diff(rows)
    lo = len(change) // 2
    peak = lo + int(np.argmax(change[lo:]))
    return peak, peak / grid


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", default="level_check.png")
    ap.add_argument("--zoom", type=int, default=2)
    args = ap.parse_args()

    path = find_project()
    if not path:
        raise SystemExit(f"no .ldtk in {LEVELS_DIR}")
    project = json.load(open(path))
    level = project["levels"][0]
    grid = project["defaultGridSize"]
    w, h = level["pxWid"], level["pxHei"]
    print(f"{path}  level '{level['identifier']}'  {w}x{h}  grid {grid}")

    bg_rel = level.get("bgRelPath")
    if bg_rel:
        bg_path = (pathlib.Path(path).parent / bg_rel).resolve()
        base = Image.open(bg_path).convert("RGBA")
        print(f"background {bg_rel}  {base.size[0]}x{base.size[1]}")
        if base.size != (w, h):
            print(f"  !! background is {base.size[0]}x{base.size[1]} but the level "
                  f"is {w}x{h} — it will be stretched")
            base = base.resize((w, h), Image.LANCZOS)
    else:
        print("background: none set in LDtk")
        base = Image.new("RGBA", (w, h), (20, 22, 28, 255))

    intgrid = next((l for l in level["layerInstances"] if l["__type"] == "IntGrid"), None)
    if intgrid is None:
        raise SystemExit("level has no IntGrid layer")
    cw, ch, csv = intgrid["__cWid"], intgrid["__cHei"], intgrid["intGridCsv"]

    over = Image.new("RGBA", base.size, (0, 0, 0, 0))
    draw = ImageDraw.Draw(over)
    tops = {}
    for cy in range(ch):
        for cx in range(cw):
            v = csv[cy * cw + cx]
            if v == 0:
                continue
            draw.rectangle(
                [cx * grid, cy * grid, cx * grid + grid - 1, cy * grid + grid - 1],
                fill=COLOURS.get(v, (255, 255, 255, 80)),
            )
            tops.setdefault(cx, cy)

    for layer in level["layerInstances"]:
        if layer["__type"] != "Entities":
            continue
        for e in layer["entityInstances"]:
            x, y = e["px"]
            draw.ellipse([x - 4, y - 4, x + 4, y + 4], fill=(90, 255, 120, 230))
            draw.text((x + 6, y - 6), e["__identifier"], fill=(200, 255, 210, 255))

    base.alpha_composite(over)
    out = base.resize((w * args.zoom, h * args.zoom), Image.NEAREST)
    out.save(args.out)

    # --- the numbers that matter ---
    print()
    if tops:
        highest = min(tops.values())
        print(f"collision top       row {highest}  (y {highest * grid})")
    if bg_rel:
        peak, row = ground_row(Image.open(bg_path), grid)
        print(f"art's ground line   row {row:.2f}  (y {peak})")
        if tops:
            gap = highest * grid - peak
            verdict = "aligned" if abs(gap) <= grid else f"OFF BY {abs(gap)}px ({abs(gap)/grid:.1f} rows)"
            direction = "collision is below the art" if gap > 0 else "collision is above the art"
            print(f"gap                 {gap:+}px  -> {verdict}"
                  + (f", {direction}" if abs(gap) > grid else ""))
    print(f"\nwrote {args.out}")


if __name__ == "__main__":
    main()
