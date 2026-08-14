"""Stack already-gridded sprite sheets into the single atlas the game indexes.

Unlike `process_sprite.py` — which rescues raw art off an opaque backdrop — this
expects sheets that are already transparent, on a uniform grid, and baselined.
It only checks that they agree with each other and concatenates their rows, so
one `TextureAtlasLayout` can address the lot.

Usage:  python3 tools/build_sheet.py
"""
import sys

import numpy as np
from PIL import Image

CELL_W, CELL_H = 108, 64
OUT = "assets/sprites/spartan_combat.png"

# Row order defines the atlas indices the clips in `src/character.rs` refer to.
SOURCES = [
    ("assets/sprites/spartan_walk.png", "walk"),
    ("assets/sprites/spartan_attack_game.png", "attack"),
]


def grid_of(path):
    image = Image.open(path).convert("RGBA")
    if image.width % CELL_W or image.height % CELL_H:
        sys.exit(f"{path}: {image.size} is not a whole number of {CELL_W}x{CELL_H} cells")
    return image, image.width // CELL_W, image.height // CELL_H


sheets = []
columns = None
for path, label in SOURCES:
    image, cols, rows = grid_of(path)
    if columns is None:
        columns = cols
    elif cols != columns:
        sys.exit(f"{path}: {cols} columns, but {SOURCES[0][0]} has {columns}")
    sheets.append((image, cols, rows, label, path))

total_rows = sum(s[2] for s in sheets)
atlas = Image.new("RGBA", (columns * CELL_W, total_rows * CELL_H), (0, 0, 0, 0))

index = 0
row_offset = 0
for image, cols, rows, label, path in sheets:
    atlas.alpha_composite(image, (0, row_offset * CELL_H))
    frames = cols * rows
    print(f"{label:8} {path}")
    print(f"         rows {row_offset}..{row_offset + rows - 1}  atlas indices {index}..{index + frames - 1}")
    index += frames
    row_offset += rows

atlas.save(OUT)

# Baseline and containment checks — a frame drifting between sheets shows up in
# game as the character popping when the animation changes.
alpha = np.asarray(atlas)[:, :, 3]
feet, empty = set(), []
for r in range(total_rows):
    for c in range(columns):
        cell = alpha[r * CELL_H:(r + 1) * CELL_H, c * CELL_W:(c + 1) * CELL_W]
        ys, xs = np.where(cell > 0)
        if len(ys) == 0:
            empty.append((r, c))
            continue
        assert xs.min() >= 0 and xs.max() < CELL_W, f"r{r}c{c} clips horizontally"
        feet.add(CELL_H - 1 - int(ys.max()))

if empty:
    print(f"warning: empty cells {empty}")
print(f"\nwrote {OUT} {atlas.size}  ({columns} columns x {total_rows} rows)")
print(f"feet sit {sorted(feet)} px above each cell bottom")
print(f"set FRAME_COLUMNS = {columns}, FRAME_ROWS = {total_rows}")
