"""Convert spartan_new.png into the game's 6x3 texture atlas.

The source contains six walk, six thrust, and six block poses, but it is not a
real grid: the rows have uneven spacing and several thrust spears cross the
nominal 256px column boundaries. This script removes only the border-connected
black background, separates those overlapping poses, applies one uniform scale,
and anchors every frame on a common body root and feet baseline.

Usage: python3 tools/process_spartan_new.py
"""

from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage


SOURCE = Path("assets/sprites/spartan_new.png")
OUTPUT = Path("assets/sprites/spartan_new_game.png")

SOURCE_SIZE = (1536, 1024)
FRAME_COLUMNS = 6
FRAME_ROWS = 3
CELL_WIDTH = 108
CELL_HEIGHT = 64
TARGET_HEIGHT = 60
BASELINE_MARGIN = 2
BACKGROUND_TOLERANCE = 30
PALETTE_COLOURS = 64

# Source-space row bounds and torso/root anchors. The figures are deliberately
# not centered in a regular grid, especially during the thrust.
ROW_BOUNDS = ((40, 310), (380, 610), (660, 930))
ROOT_X = (
    (135, 395, 655, 905, 1160, 1405),
    (130, 420, 675, 945, 1205, 1445),
    (125, 385, 645, 905, 1165, 1420),
)


def foreground_mask(rgb: np.ndarray) -> np.ndarray:
    """Keep dark pixels inside sprites while removing connected background."""
    black_like = rgb.sum(axis=2) <= BACKGROUND_TOLERANCE
    labels, _ = ndimage.label(black_like)
    border_labels = (
        set(labels[0, :])
        | set(labels[-1, :])
        | set(labels[:, 0])
        | set(labels[:, -1])
    )
    border_labels.discard(0)
    return ~np.isin(labels, list(border_labels))


def frame_region(row: int, column: int, shape: tuple[int, int]) -> np.ndarray:
    """Select one pose without clipping spears that cross nominal cells."""
    height, width = shape
    y0, y1 = ROW_BOUNDS[row]
    yy, xx = np.ogrid[:height, :width]
    vertical = (yy >= y0) & (yy < y1)

    if row != 1:
        x0 = column * 256
        x1 = (column + 1) * 256
        return vertical & (xx >= x0) & (xx < x1)

    # The first four thrusts have clear gaps even though their spears extend
    # beyond 256px slots. The last two overlap in X but occupy separate regions
    # around the horizontal and raised spear tips.
    attack_x = ((0, 256), (256, 568), (568, 835), (835, 1112))
    if column < 4:
        x0, x1 = attack_x[column]
        return vertical & (xx >= x0) & (xx < x1)
    if column == 4:
        return vertical & (xx >= 1112) & (xx < 1385) & ((xx < 1320) | (yy >= 484))
    return vertical & (xx >= 1320) & ((xx >= 1385) | (yy < 484))


source = Image.open(SOURCE).convert("RGB")
if source.size != SOURCE_SIZE:
    raise SystemExit(f"expected {SOURCE} to be {SOURCE_SIZE}, got {source.size}")

rgb = np.asarray(source)
foreground = foreground_mask(rgb.astype(int))

frames = []
walk_heights = []
for row in range(FRAME_ROWS):
    for column in range(FRAME_COLUMNS):
        mask = foreground & frame_region(row, column, foreground.shape)
        ys, xs = np.where(mask)
        if not len(xs):
            raise SystemExit(f"frame r{row}c{column} is empty")
        bounds = (xs.min(), ys.min(), xs.max() + 1, ys.max() + 1)
        if row == 0:
            walk_heights.append(bounds[3] - bounds[1])
        frames.append((row, column, bounds, mask))

scale = TARGET_HEIGHT / float(np.mean(walk_heights))
atlas = Image.new(
    "RGBA",
    (CELL_WIDTH * FRAME_COLUMNS, CELL_HEIGHT * FRAME_ROWS),
    (0, 0, 0, 0),
)

for row, column, (x0, y0, x1, y1), mask in frames:
    rgba = np.dstack((rgb, np.where(mask, 255, 0).astype(np.uint8)))
    frame = Image.fromarray(rgba[y0:y1, x0:x1])
    frame = frame.resize(
        (max(1, round(frame.width * scale)), max(1, round(frame.height * scale))),
        Image.Resampling.BOX,
    )
    alpha = frame.getchannel("A").point(lambda value: 255 if value >= 96 else 0)
    frame.putalpha(alpha)

    root_in_crop = ROOT_X[row][column] - x0
    scaled_root = round(root_in_crop * scale)
    dest_x = column * CELL_WIDTH + CELL_WIDTH // 2 - scaled_root
    dest_y = row * CELL_HEIGHT + CELL_HEIGHT - BASELINE_MARGIN - frame.height
    atlas.alpha_composite(frame, (dest_x, dest_y))

    left = column * CELL_WIDTH
    right = (column + 1) * CELL_WIDTH
    top = row * CELL_HEIGHT
    bottom = (row + 1) * CELL_HEIGHT
    if dest_x <= left or dest_x + frame.width >= right:
        raise SystemExit(f"frame r{row}c{column} clips horizontally")
    if dest_y <= top or dest_y + frame.height >= bottom:
        raise SystemExit(f"frame r{row}c{column} clips vertically")

# Reduce colors introduced by downsampling while retaining the original alpha.
alpha = atlas.getchannel("A")
atlas = atlas.convert("RGB").quantize(
    colors=PALETTE_COLOURS, method=Image.Quantize.MEDIANCUT
).convert("RGBA")
atlas.putalpha(alpha)
atlas.save(OUTPUT)

print(
    f"wrote {OUTPUT} ({atlas.width}x{atlas.height}), "
    f"{FRAME_COLUMNS}x{FRAME_ROWS} cells of {CELL_WIDTH}x{CELL_HEIGHT}"
)
