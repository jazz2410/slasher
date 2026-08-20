"""Trim a single-image weapon and scale it to the size it is drawn at.

Source art arrives far larger than it will ever appear. Handing Bevy a 2000px
texture to draw at 64px wastes memory and, with nearest-neighbour sampling,
aliases into noise. Downscaling once, with a filter that averages, keeps it
readable.

    python3 tools/import_weapon.py assets/weapons/spear.png --out fire_spear.png \
        --length 64
"""
import argparse
import pathlib

from PIL import Image

WEAPONS = pathlib.Path("assets/weapons")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("source")
    ap.add_argument("--out", required=True, help="filename inside assets/weapons")
    ap.add_argument("--length", type=int, default=64, help="width in game pixels")
    args = ap.parse_args()

    src = Image.open(args.source).convert("RGBA")
    box = src.getbbox()
    if box is None:
        raise SystemExit(f"{args.source} is entirely transparent")
    trimmed = src.crop(box)
    print(f"source  {src.size[0]}x{src.size[1]}  -> trimmed {trimmed.size[0]}x{trimmed.size[1]}")

    aspect = trimmed.width / trimmed.height
    height = max(1, round(args.length / aspect))
    out = trimmed.resize((args.length, height), Image.LANCZOS)

    # Downscaling feathers the edges; harden them so the silhouette stays crisp
    # against the level rather than fading into a halo.
    alpha = out.getchannel("A").point(lambda v: 255 if v > 96 else 0)
    out.putalpha(alpha)

    dest = WEAPONS / args.out
    dest.parent.mkdir(parents=True, exist_ok=True)
    out.save(dest)
    print(f"wrote {dest}  {out.size[0]}x{out.size[1]}  (aspect {aspect:.2f})")
    print(f"draw it at {out.size[0]}x{out.size[1]}; anything larger will look soft")


if __name__ == "__main__":
    main()
