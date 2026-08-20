"""Stack the processed Archer walk and shoot sheets into one combat atlas.

Run after processing the source sheets with the commands documented in the
README. Both inputs must use 108x66 cells and five columns.
"""

from pathlib import Path

from PIL import Image


CELL = (108, 66)
COLUMNS = 5
SOURCES = [
    Path("assets/sprites/archer_walk_game.png"),
    Path("assets/sprites/archer_shoot_game.png"),
]
OUTPUT = Path("assets/sprites/archer_combat.png")


sheets = [Image.open(path).convert("RGBA") for path in SOURCES]
for path, sheet in zip(SOURCES, sheets):
    if sheet.width != CELL[0] * COLUMNS or sheet.height % CELL[1]:
        raise SystemExit(f"{path} is not a five-column grid of {CELL[0]}x{CELL[1]} cells")

output = Image.new(
    "RGBA",
    (CELL[0] * COLUMNS, sum(sheet.height for sheet in sheets)),
    (0, 0, 0, 0),
)
y = 0
for sheet in sheets:
    output.alpha_composite(sheet, (0, y))
    y += sheet.height

output.save(OUTPUT)
print(f"wrote {OUTPUT} ({output.width}x{output.height}), 5 columns x 10 rows")
