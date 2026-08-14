"""Build the in-game attack atlas from spartan_attack.png.

The AutoSprite source is a transparent 5x3 grid of 256px cells. All fifteen
frames are retained, uniformly scaled to match the walking character, anchored
on the lower body, and placed on the shared 108x64 action-frame contract.

Usage: python3 tools/process_attack_sprite.py
"""

from pathlib import Path

from PIL import Image


SOURCE = Path("assets/sprites/spartan_attack.png")
OUTPUT = Path("assets/sprites/spartan_attack_game.png")

SOURCE_CELL_SIZE = 256
FRAME_COLUMNS = 5
FRAME_ROWS = 3
FRAME_COUNT = FRAME_COLUMNS * FRAME_ROWS
CELL_WIDTH = 108
CELL_HEIGHT = 64
BASELINE_MARGIN = 2
WALK_SOURCE_HEIGHT = 160
TARGET_WALK_HEIGHT = 60
PALETTE_COLOURS = 64


def body_anchor_x(frame: Image.Image) -> int:
    """Find the lower-body centre without letting the long spear skew it."""
    alpha = frame.getchannel("A")
    lower_body = alpha.crop((0, frame.height // 2, frame.width, frame.height))
    weighted_x = 0
    total_alpha = 0
    for y in range(lower_body.height):
        for x in range(lower_body.width):
            value = lower_body.getpixel((x, y))
            weighted_x += x * value
            total_alpha += value
    return round(weighted_x / total_alpha) if total_alpha else frame.width // 2


source = Image.open(SOURCE).convert("RGBA")
expected_size = (
    SOURCE_CELL_SIZE * FRAME_COLUMNS,
    SOURCE_CELL_SIZE * FRAME_ROWS,
)
if source.size != expected_size:
    raise SystemExit(f"expected {SOURCE} to be {expected_size}, got {source.size}")

scale = TARGET_WALK_HEIGHT / WALK_SOURCE_HEIGHT
output = Image.new(
    "RGBA",
    (CELL_WIDTH * FRAME_COLUMNS, CELL_HEIGHT * FRAME_ROWS),
    (0, 0, 0, 0),
)

for index in range(FRAME_COUNT):
    source_column = index % FRAME_COLUMNS
    source_row = index // FRAME_COLUMNS
    cell = source.crop(
        (
            source_column * SOURCE_CELL_SIZE,
            source_row * SOURCE_CELL_SIZE,
            (source_column + 1) * SOURCE_CELL_SIZE,
            (source_row + 1) * SOURCE_CELL_SIZE,
        )
    )
    bounds = cell.getchannel("A").getbbox()
    if bounds is None:
        raise SystemExit(f"attack frame {index} is empty")

    frame = cell.crop(bounds)
    anchor_x = body_anchor_x(frame)
    frame = frame.resize(
        (
            max(1, round(frame.width * scale)),
            max(1, round(frame.height * scale)),
        ),
        Image.Resampling.LANCZOS,
    )
    alpha = frame.getchannel("A").point(lambda value: 255 if value >= 96 else 0)
    frame.putalpha(alpha)

    scaled_anchor = round(anchor_x * scale)
    destination_column = index % FRAME_COLUMNS
    destination_row = index // FRAME_COLUMNS
    x = destination_column * CELL_WIDTH + CELL_WIDTH // 2 - scaled_anchor
    y = (
        destination_row * CELL_HEIGHT
        + CELL_HEIGHT
        - BASELINE_MARGIN
        - frame.height
    )
    output.alpha_composite(frame, (x, y))

    left = destination_column * CELL_WIDTH
    right = (destination_column + 1) * CELL_WIDTH
    top = destination_row * CELL_HEIGHT
    bottom = (destination_row + 1) * CELL_HEIGHT
    if x <= left or x + frame.width >= right:
        raise SystemExit(f"attack frame {index} clips horizontally")
    if y <= top or y + frame.height >= bottom:
        raise SystemExit(f"attack frame {index} clips vertically")

alpha = output.getchannel("A")
output = output.convert("RGB").quantize(
    colors=PALETTE_COLOURS, method=Image.Quantize.MEDIANCUT
).convert("RGBA")
output.putalpha(alpha)
output.save(OUTPUT)

print(
    f"wrote {OUTPUT} ({output.width}x{output.height}), "
    f"{FRAME_COUNT} frames in {FRAME_COLUMNS}x{FRAME_ROWS} cells"
)
