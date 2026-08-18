"""Turn a generated image into a level background of exactly the right size.

Image generators emit 1024x1024, 1536x1024 or 1792x1024 — none of which match a
level's aspect. Resizing straight to the target would squash the art and, worse,
move the painted ground line away from the collision under it. So: crop to the
level's aspect first, then scale.

    python3 tools/import_level_art.py generated.png --out village.png
    python3 tools/import_level_art.py generated.png --out l2.png --anchor bottom
    python3 tools/import_level_art.py generated.png --out l2.png --palette

Options:
    --anchor  which part of the image to keep when cropping (default: centre).
              `bottom` keeps the ground, which is usually what a level wants.
    --palette snap to assets/palettes/slasher.gpl, forcing a shared look across
              levels that were generated in different sessions.
    --size    override the target, e.g. 640x352. Defaults to the level size read
              from the LDtk project.
"""
import argparse
import glob
import json
import pathlib
import sys

from PIL import Image

LEVELS_DIR = pathlib.Path("assets/levels")
PALETTE = pathlib.Path("assets/palettes/slasher.gpl")


def level_size(default=(640, 352)):
    """Target size from the LDtk project, so the two can never disagree."""
    for path in sorted(glob.glob(str(LEVELS_DIR / "*.ldtk"))):
        try:
            levels = json.load(open(path))["levels"]
        except (OSError, ValueError, KeyError):
            continue
        if levels:
            return levels[0]["pxWid"], levels[0]["pxHei"], path
    return default[0], default[1], None


def load_palette():
    if not PALETTE.exists():
        sys.exit(f"{PALETTE} not found")
    colours = []
    for line in PALETTE.read_text().splitlines():
        parts = line.split()
        if len(parts) >= 3 and all(p.isdigit() for p in parts[:3]):
            colours.append(tuple(int(p) for p in parts[:3]))
    return colours


def crop_to_aspect(image, aspect, anchor):
    """Trim the long axis until the image matches `aspect`, keeping `anchor`."""
    w, h = image.size
    if abs(w / h - aspect) < 1e-4:
        return image, "none"

    if w / h > aspect:                      # too wide: trim width
        new_w = round(h * aspect)
        offset = {"start": 0, "centre": (w - new_w) // 2, "end": w - new_w}
        x = offset["centre" if anchor in ("top", "bottom", "centre") else anchor]
        return image.crop((x, 0, x + new_w, h)), f"width {w}->{new_w}"

    new_h = round(w / aspect)               # too tall: trim height
    offset = {"top": 0, "centre": (h - new_h) // 2, "bottom": h - new_h}
    y = offset.get(anchor, (h - new_h) // 2)
    return image.crop((0, y, w, y + new_h)), f"height {h}->{new_h}"


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("source")
    ap.add_argument("--out", required=True, help="filename inside assets/levels")
    ap.add_argument("--anchor", default="centre",
                    choices=["top", "centre", "bottom", "start", "end"])
    ap.add_argument("--palette", action="store_true")
    ap.add_argument("--size", help="WxH, overriding the level size")
    args = ap.parse_args()

    if args.size:
        tw, th = (int(v) for v in args.size.lower().split("x"))
        origin = "--size"
    else:
        tw, th, origin = level_size()
        origin = origin or "default"

    src = Image.open(args.source).convert("RGB")
    print(f"source  {args.source}  {src.size[0]}x{src.size[1]}  aspect {src.size[0]/src.size[1]:.4f}")
    print(f"target  {tw}x{th}  aspect {tw/th:.4f}   (from {origin})")

    cropped, what = crop_to_aspect(src, tw / th, args.anchor)
    if what != "none":
        print(f"crop    {what}  (anchor {args.anchor})")

    # LANCZOS: this is painted art, not pixel art. Nearest would alias it badly.
    out = cropped.resize((tw, th), Image.LANCZOS)

    if args.palette:
        colours = load_palette()
        ref = Image.new("P", (1, 1))
        flat = [c for rgb in colours for c in rgb]
        ref.putpalette(flat + [0] * (768 - len(flat)))
        out = out.quantize(palette=ref, dither=Image.FLOYDSTEINBERG).convert("RGB")
        print(f"palette snapped to {len(colours)} colours")

    dest = LEVELS_DIR / args.out
    dest.parent.mkdir(parents=True, exist_ok=True)
    out.save(dest)
    print(f"\nwrote {dest}  {out.size[0]}x{out.size[1]}")
    print(f"set it in LDtk: Level properties -> Background image -> {args.out}")


if __name__ == "__main__":
    main()
