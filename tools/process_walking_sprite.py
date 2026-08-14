"""Build the in-game walk cycle from the first row of spartan_walking.png.

The source is a 5-column grid of 256px cells. Only the first five frames are
used. They are cropped, scaled to the common 60px action height, and placed in
108x64 cells with a shared torso/feet anchor so future action sheets can switch
without moving the player on screen.

Usage: python3 tools/process_walking_sprite.py
"""

from pathlib import Path

from PIL import Image


SOURCE = Path("assets/sprites/spartan_walking.png")
OUTPUT = Path("assets/sprites/spartan_walk.png")

SOURCE_CELL_SIZE = 256
FRAME_COUNT = 5
CELL_WIDTH = 108
CELL_HEIGHT = 64
TARGET_HEIGHT = 60
BASELINE_MARGIN = 2
PALETTE_COLOURS = 64


def torso_anchor_x(frame: Image.Image) -> int:
    """Return the alpha-weighted horizontal centre of the upper body."""
    alpha = frame.getchannel("A")
    upper_body = alpha.crop((0, 0, frame.width, max(1, frame.height // 2)))
    weighted_x = 0
    total_alpha = 0
    for y in range(upper_body.height):
        for x in range(upper_body.width):
            value = upper_body.getpixel((x, y))
            weighted_x += x * value
            total_alpha += value
    return round(weighted_x / total_alpha) if total_alpha else frame.width // 2


source = Image.open(SOURCE).convert("RGBA")
expected_width = SOURCE_CELL_SIZE * FRAME_COUNT
if source.width < expected_width or source.height < SOURCE_CELL_SIZE:
    raise SystemExit(
        f"{SOURCE} must contain at least one {expected_width}x{SOURCE_CELL_SIZE} row"
    )

output = Image.new(
    "RGBA", (CELL_WIDTH * FRAME_COUNT, CELL_HEIGHT), (0, 0, 0, 0)
)

for index in range(FRAME_COUNT):
    cell = source.crop(
        (
            index * SOURCE_CELL_SIZE,
            0,
            (index + 1) * SOURCE_CELL_SIZE,
            SOURCE_CELL_SIZE,
        )
    )
    bounds = cell.getchannel("A").getbbox()
    if bounds is None:
        raise SystemExit(f"walk frame {index} is empty")

    frame = cell.crop(bounds)
    scale = TARGET_HEIGHT / frame.height
    frame = frame.resize(
        (max(1, round(frame.width * scale)), TARGET_HEIGHT),
        Image.Resampling.LANCZOS,
    )

    # Keep transparent resampling fringe from becoming a faint outline in game.
    alpha = frame.getchannel("A").point(lambda value: 255 if value >= 96 else 0)
    frame.putalpha(alpha)
    frame = frame.crop(frame.getchannel("A").getbbox())

    anchor_x = torso_anchor_x(frame)
    x = index * CELL_WIDTH + CELL_WIDTH // 2 - anchor_x
    y = CELL_HEIGHT - BASELINE_MARGIN - frame.height
    output.alpha_composite(frame, (x, y))

    if x <= index * CELL_WIDTH or x + frame.width >= (index + 1) * CELL_WIDTH:
        raise SystemExit(f"walk frame {index} clips its {CELL_WIDTH}px cell")

alpha = output.getchannel("A")
output = output.convert("RGB").quantize(
    colors=PALETTE_COLOURS, method=Image.Quantize.MEDIANCUT
).convert("RGBA")
output.putalpha(alpha)
output.save(OUTPUT)
print(
    f"wrote {OUTPUT} ({output.width}x{output.height}), "
    f"{FRAME_COUNT} frames of {CELL_WIDTH}x{CELL_HEIGHT}"
)
