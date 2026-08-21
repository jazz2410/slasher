"""Build the player atlases from ``spartan_sprites.png``.

The supplied image contains four rows of six loosely spaced figures rather
than cells that can safely be sliced at every 256 pixels: capes and spears cross
those nominal boundaries. Its alpha channel also has a very faint backdrop.
This importer finds the 24 solid silhouettes, restores only their neighbouring
antialiased pixels, and places every frame on one shared feet baseline.

Row contract:
    0 walk, 1 normal thrust, 2 special throw, 3 death
"""

from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage


SOURCE = Path("assets/sprites/spartan_sprites.png")
ACTION_OUTPUTS = (
    Path("assets/sprites/player_walk_game.png"),
    Path("assets/sprites/player_thrust_game.png"),
    Path("assets/sprites/player_throw_game.png"),
    Path("assets/sprites/player_dies_game.png"),
)
COMBAT_OUTPUT = Path("assets/sprites/player_combat.png")
SUPPLEMENTAL_ACTIONS = (
    Path("assets/sprites/player_idle_game.png"),
    Path("assets/sprites/player_jump_game.png"),
)

ROWS = 4
FRAMES = 6
CELL_W = 128
CELL_H = 96
BASELINE_Y = 78
TARGET_WALK_HEIGHT = 60.0

# The body and weapon interiors sit above this value. Lower-alpha pixels are
# restored only near a confirmed silhouette, which removes the faint painted
# backdrop without turning the antialiased edges jagged.
CORE_ALPHA = 224
EDGE_ALPHA = 8
MIN_BODY_AREA = 10_000
MIN_FRAGMENT_AREA = 20
EDGE_RADIUS = 3


def bbox_distance(a, b):
    """Squared distance between two (y0, x0, y1, x1) bounding boxes."""
    ay0, ax0, ay1, ax1 = a
    by0, bx0, by1, bx1 = b
    dx = max(ax0 - bx1, bx0 - ax1, 0)
    dy = max(ay0 - by1, by0 - ay1, 0)
    return dx * dx + dy * dy


def component_bbox(component):
    ys, xs = np.where(component)
    return int(ys.min()), int(xs.min()), int(ys.max() + 1), int(xs.max() + 1)


def body_anchor_x(body):
    """Find the fighter's root without letting a long spear pull it sideways."""
    ys, xs = np.where(body)
    y0, y1 = ys.min(), ys.max() + 1
    torso = body[y0 + (y1 - y0) // 4 : y0 + (y1 - y0) * 3 // 4]
    _, torso_xs = np.where(torso)
    if len(torso_xs):
        return float(np.median(torso_xs))
    return float(np.median(xs))


def main():
    source = Image.open(SOURCE).convert("RGBA")
    rgba = np.asarray(source)
    alpha = rgba[:, :, 3]
    labels, count = ndimage.label(alpha > CORE_ALPHA)
    sizes = np.bincount(labels.ravel())

    body_labels = [label for label in range(1, count + 1) if sizes[label] >= MIN_BODY_AREA]
    if len(body_labels) != ROWS * FRAMES:
        raise SystemExit(
            f"expected {ROWS * FRAMES} figures, found {len(body_labels)}; "
            "the source matte or layout changed"
        )

    centres = {}
    boxes = {}
    for label in body_labels:
        component = labels == label
        boxes[label] = component_bbox(component)
        centres[label] = ndimage.center_of_mass(component)

    # The art is visually arranged as four rows of six. Sorting the 24 large
    # silhouettes avoids unsafe fixed-width crops when a spear crosses a cell.
    ordered = sorted(body_labels, key=lambda label: centres[label][0])
    rows = []
    for row in range(ROWS):
        group = ordered[row * FRAMES : (row + 1) * FRAMES]
        rows.append(sorted(group, key=lambda label: centres[label][1]))

    assigned = {label: [label] for label in body_labels}
    for label in range(1, count + 1):
        if label in assigned or sizes[label] < MIN_FRAGMENT_AREA:
            continue
        fragment = labels == label
        fragment_box = component_bbox(fragment)
        fragment_centre = ndimage.center_of_mass(fragment)
        # Distance between boxes normally identifies a detached weapon piece.
        # If it touches two neighbouring frames (the airborne special spear
        # does), its centre keeps it with the pose that actually threw it.
        owner = min(
            body_labels,
            key=lambda body: (
                bbox_distance(fragment_box, boxes[body]),
                (fragment_centre[0] - centres[body][0]) ** 2
                + (fragment_centre[1] - centres[body][1]) ** 2,
            ),
        )
        assigned[owner].append(label)

    frames = []
    walk_heights = [boxes[label][2] - boxes[label][0] for label in rows[0]]
    scale = TARGET_WALK_HEIGHT / float(np.mean(walk_heights))

    for action, row in enumerate(rows):
        action_frames = []
        for frame_number, body_label in enumerate(row):
            core = np.isin(labels, assigned[body_label])
            support = ndimage.binary_dilation(core, iterations=EDGE_RADIUS) & (alpha > EDGE_ALPHA)
            y0, x0, y1, x1 = component_bbox(support)

            crop = rgba[y0:y1, x0:x1].copy()
            crop[:, :, 3] = np.where(support[y0:y1, x0:x1], crop[:, :, 3], 0)
            # The supplied image tops out at alpha 254. Normalise it so solid
            # armour stays fully opaque after import.
            crop[:, :, 3] = np.minimum(crop[:, :, 3].astype(np.uint16) * 255 // 254, 255).astype(
                np.uint8
            )
            image = Image.fromarray(crop)
            new_size = (
                max(1, round(image.width * scale)),
                max(1, round(image.height * scale)),
            )
            image = image.resize(new_size, Image.Resampling.LANCZOS)

            body = labels[y0:y1, x0:x1] == body_label
            anchor = body_anchor_x(body) * scale
            # Once prone, anchor near the hips rather than the visual centre;
            # otherwise the body jumps sideways as the legs extend.
            if action == 3 and image.width > image.height * 2:
                anchor = image.width * 0.63

            x = CELL_W // 2 - round(anchor)
            y = BASELINE_Y - image.height
            if x < 1 or x + image.width >= CELL_W or y < 1 or y + image.height >= CELL_H:
                raise SystemExit(
                    f"row {action} frame {frame_number} does not fit: "
                    f"{image.width}x{image.height} at ({x}, {y})"
                )

            cell = Image.new("RGBA", (CELL_W, CELL_H), (0, 0, 0, 0))
            cell.alpha_composite(image, (x, y))
            action_frames.append(cell)
        frames.append(action_frames)

    action_sheets = []
    for output_path, action_frames in zip(ACTION_OUTPUTS, frames):
        sheet = Image.new("RGBA", (CELL_W * FRAMES, CELL_H), (0, 0, 0, 0))
        for column, frame in enumerate(action_frames):
            sheet.alpha_composite(frame, (column * CELL_W, 0))
        sheet.save(output_path)
        action_sheets.append(sheet)
        print(f"wrote {output_path} ({sheet.width}x{sheet.height})")

    supplemental = [Image.open(path).convert("RGBA") for path in SUPPLEMENTAL_ACTIONS]
    for path, sheet in zip(SUPPLEMENTAL_ACTIONS, supplemental):
        if sheet.size != (CELL_W * FRAMES, CELL_H):
            raise SystemExit(f"{path} must be {CELL_W * FRAMES}x{CELL_H}")

    combat_rows = action_sheets[:3] + supplemental
    combat = Image.new(
        "RGBA", (CELL_W * FRAMES, CELL_H * len(combat_rows)), (0, 0, 0, 0)
    )
    for row, sheet in enumerate(combat_rows):
        combat.alpha_composite(sheet, (0, row * CELL_H))
    combat.save(COMBAT_OUTPUT)
    print(f"wrote {COMBAT_OUTPUT} ({combat.width}x{combat.height})")


if __name__ == "__main__":
    main()
