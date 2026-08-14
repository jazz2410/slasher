"""Turn a supplied spartan sheet into a game-ready spritesheet.

The source sheets are opaque images: white margins, a black backdrop, and twelve
hand-placed figures that share no common grid. This script:
  1. keys out the background by flood-filling inward from the borders, so dark
     outlines *inside* the figure survive,
  2. auto-detects the row bands and per-figure columns,
  3. crops each figure and rescales everything by one uniform factor, sized to
     the game's ~60px character,
  4. re-anchors each frame on feet-centre so nothing pops between frames,
  5. picks a cell width wide enough that no frame clips, keeping the anchor
     centred so `flip_x` mirrors in place,
  6. quantises to kill the resampling noise (the sources carry ~80k colours).

Usage:  python3 tools/process_sprite.py [source.png]
"""
import sys

import numpy as np
from PIL import Image
from scipy import ndimage

SRC = sys.argv[1] if len(sys.argv) > 1 else (
    "assets/sprites/spartan_sprite_all.png"
)
OUT = "assets/sprites/spartan_combat.png"

CELL_H = 64
TARGET_WALK_HEIGHT = 60.0  # character height in game units
BASELINE_MARGIN = 2        # px of empty space under the feet
SIDE_MARGIN = 3            # px of clearance beyond the widest reach
PALETTE_COLOURS = 32
MIN_BAND_HEIGHT = 20       # ignore 1px anti-aliasing seams
MIN_FIGURE_WIDTH = 20
FRAMES_PER_ROW = 6
# Sum-of-channels distance from a backdrop colour still counted as backdrop.
# Must stay well under the darkest outline: black backdrop is 0, the outline
# lands around 58, so this keeps the outline while absorbing its anti-aliasing.
BG_TOLERANCE = 30
BORDER_COLOUR_SHARE = 0.02


def foreground_mask(rgb: np.ndarray) -> np.ndarray:
    """True where the image is figure rather than backdrop.

    The backdrop colours are read off the image's own border rather than
    assumed. A fixed luminance threshold is dangerous here: the character's
    outline is only just brighter than a black backdrop, so a generous cut
    swallows the outline, then the flood fill pours through the hole and eats
    the shading inside too, leaving the figure in pieces.
    """
    edges = np.concatenate([rgb[0, :], rgb[-1, :], rgb[:, 0], rgb[:, -1]])
    colours, counts = np.unique(edges.reshape(-1, 3), axis=0, return_counts=True)
    # Only colours that genuinely tile the border count as backdrop.
    backdrop = colours[counts >= counts.max() * BORDER_COLOUR_SHARE]
    print(f"backdrop colours from border: {[tuple(c) for c in backdrop]}")

    distance = np.abs(rgb[:, :, None, :] - backdrop[None, None, :, :]).sum(3).min(2)
    bg_like = distance <= BG_TOLERANCE

    labels, _ = ndimage.label(bg_like)
    border = set(labels[0, :]) | set(labels[-1, :]) | set(labels[:, 0]) | set(labels[:, -1])
    border.discard(0)
    return ~np.isin(labels, list(border))


def runs(indices, gap: int):
    """Group sorted indices into (start, end) runs split on gaps."""
    out, start, prev = [], indices[0], indices[0]
    for i in indices[1:]:
        if i - prev > gap:
            out.append((start, prev + 1))
            start = i
        prev = i
    out.append((start, prev + 1))
    return out


src = Image.open(SRC).convert("RGB")
rgb = np.asarray(src).astype(int)
fg = foreground_mask(rgb)
height, width = fg.shape

def merge_to(figures, target):
    """Collapse the closest neighbours until `target` frames remain.

    A frame can break into several column runs when part of it is detached —
    the thrust row separates the spear tip from the hand. The gap inside a
    frame is always far smaller than the gap between frames, so merging the
    tightest pair first reassembles them.
    """
    figs = list(figures)
    while len(figs) > target:
        gap, i = min((figs[i + 1][0] - figs[i][1], i) for i in range(len(figs) - 1))
        figs[i] = (figs[i][0], figs[i + 1][1])
        del figs[i + 1]
    return figs


bands = [
    (y0, y1)
    for y0, y1 in runs(np.where(fg.any(1))[0], gap=10)
    if y1 - y0 >= MIN_BAND_HEIGHT
]
if not bands:
    sys.exit("found no row bands — is the background really black/white?")
ROWS = len(bands)
print(f"{ROWS} row bands, {FRAMES_PER_ROW} frames each")

layout = []
for y0, y1 in bands:
    cols = np.where(fg[y0:y1].any(0))[0]
    figures = [(x0, x1) for x0, x1 in runs(cols, gap=10) if x1 - x0 >= MIN_FIGURE_WIDTH]
    if len(figures) < FRAMES_PER_ROW:
        sys.exit(f"band {y0}..{y1}: found only {len(figures)} figures")
    if len(figures) > FRAMES_PER_ROW:
        print(f"  band {y0}..{y1}: {len(figures)} runs, merging fragments")
        figures = merge_to(figures, FRAMES_PER_ROW)
    row = []
    for x0, x1 in figures:
        ys, xs = np.where(fg[y0:y1, x0:x1])
        row.append((x0 + xs.min(), x0 + xs.max() + 1, y0 + ys.min(), y0 + ys.max() + 1))
    layout.append(row)

walk_h = np.mean([b[3] - b[2] for b in layout[0]])
scale = TARGET_WALK_HEIGHT / walk_h
print(f"source {SRC}")
print(f"walk mean height {walk_h:.0f}px -> uniform scale {scale:.4f}")

alpha = np.where(fg, 255, 0).astype(np.uint8)
keyed = Image.fromarray(np.dstack([rgb.astype(np.uint8), alpha]))


def torso_x(mask: np.ndarray) -> float:
    """Horizontal centroid of the upper body.

    Measured against the alternatives, this is the steadiest root for both rows.
    Anchoring on the feet swings the walk about (they alternate), and anchoring
    on the leading foot throws the thrust around by ~19px. Using one anchor for
    both rows also means the body cannot jump sideways when an attack starts.
    """
    xs = np.where(mask[: max(2, mask.shape[0] // 2), :])[1]
    return xs.mean() if len(xs) else mask.shape[1] / 2


prepared = []
for r, row in enumerate(layout):
    for c, (x0, x1, y0, y1) in enumerate(row):
        crop = keyed.crop((x0, y0, x1, y1))
        small = crop.resize(
            (max(1, round(crop.width * scale)), max(1, round(crop.height * scale))),
            Image.BOX,
        )
        # Harden the alpha now rather than at save time. Downscaling feathers
        # the edges, and thresholding later would silently drop a faint bottom
        # row *after* placement — which reads in game as a 1-2px vertical bob.
        hard = np.asarray(small).copy()
        hard[:, :, 3] = np.where(hard[:, :, 3] > 128, 255, 0)
        small = Image.fromarray(hard)
        small = small.crop(small.getbbox())

        mask = np.asarray(small)[:, :, 3] > 0
        prepared.append((r, c, small, int(round(torso_x(mask)))))

reach = max(max(anchor, small.width - anchor) for _, _, small, anchor in prepared)
cell_w = 2 * (reach + SIDE_MARGIN)
cell_w += cell_w % 2
print(f"widest reach from the feet anchor: {reach}px -> {cell_w}x{CELL_H} cells")

sheet = Image.new("RGBA", (cell_w * FRAMES_PER_ROW, CELL_H * ROWS), (0, 0, 0, 0))
for r, c, small, anchor in prepared:
    sheet.alpha_composite(
        small,
        (c * cell_w + cell_w // 2 - anchor, r * CELL_H + CELL_H - BASELINE_MARGIN - small.height),
    )
    print(f"  r{r} c{c}: {small.width:>3}x{small.height:<3} anchor_x={anchor}")

quantised = sheet.convert("RGB").quantize(colors=PALETTE_COLOURS, method=Image.MEDIANCUT)
final = quantised.convert("RGBA")
final.putalpha(sheet.getchannel("A").point(lambda v: 255 if v > 128 else 0))
final.save(OUT)

# The whole point of the anchor maths is that nothing crosses a cell edge.
check = np.asarray(final)[:, :, 3]
for r in range(ROWS):
    for c in range(FRAMES_PER_ROW):
        cell = check[r * CELL_H:(r + 1) * CELL_H, c * cell_w:(c + 1) * cell_w]
        ys, xs = np.where(cell > 0)
        assert xs.min() > 0 and xs.max() < cell_w - 1, f"r{r}c{c} clips horizontally"
        assert ys.min() > 0 and ys.max() < CELL_H - 1, f"r{r}c{c} clips vertically"

feet = [
    CELL_H - 1 - np.where(check[r * CELL_H:(r + 1) * CELL_H, c * cell_w:(c + 1) * cell_w] > 0)[0].max()
    for r in range(ROWS)
    for c in range(FRAMES_PER_ROW)
]
print(f"\nwrote {OUT} {final.size}")
print(f"feet sit {set(feet)} px above each cell bottom -> HALF_HEIGHT = {CELL_H // 2 - BASELINE_MARGIN}")
print(f"set FRAME_SIZE to UVec2::new({cell_w}, {CELL_H}), FRAME_ROWS = {ROWS}")
