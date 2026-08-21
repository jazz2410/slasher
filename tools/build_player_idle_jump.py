"""Extract the player's idle and jump rows from ``spartan_idle_jump.png``.

The source is a labelled presentation sheet on an opaque white background.
Headings and frame numbers are excluded, the six figures in each action are
isolated, and all poses are normalised to the game's feet baseline. Physics
provides the jump arc; baking the source image's arc into the atlas as well
would move the character twice.
"""

from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage


SOURCE = Path("assets/sprites/spartan_idle_jump.png")
OUTPUTS = (
    Path("assets/sprites/player_idle_game.png"),
    Path("assets/sprites/player_jump_game.png"),
)
ACTION_BANDS = ((75, 450), (590, 975))

FRAMES = 6
CELL_W = 128
CELL_H = 96
BASELINE_Y = 78
TARGET_IDLE_HEIGHT = 60.0
BACKGROUND_LEVEL = 254.0
BACKGROUND_RGB = np.array([254.0, 254.0, 254.0], dtype=np.float32)
CORE_SCORE = 25.0
EDGE_SCORE = 5.0
MIN_BODY_AREA = 5_000
MIN_FRAGMENT_AREA = 20
EDGE_RADIUS = 3


def bbox(component):
    ys, xs = np.where(component)
    return int(ys.min()), int(xs.min()), int(ys.max() + 1), int(xs.max() + 1)


def bbox_distance(a, b):
    ay0, ax0, ay1, ax1 = a
    by0, bx0, by1, bx1 = b
    dx = max(ax0 - bx1, bx0 - ax1, 0)
    dy = max(ay0 - by1, by0 - ay1, 0)
    return dx * dx + dy * dy


def anchor_x(body):
    ys, xs = np.where(body)
    y0, y1 = ys.min(), ys.max() + 1
    torso = body[y0 + (y1 - y0) // 4 : y0 + (y1 - y0) * 3 // 4]
    _, torso_xs = np.where(torso)
    return float(np.median(torso_xs if len(torso_xs) else xs))


def extract_row(rgba, score, y0, y1):
    local_score = score[y0:y1]
    labels, count = ndimage.label(local_score > CORE_SCORE)
    sizes = np.bincount(labels.ravel())
    bodies = [label for label in range(1, count + 1) if sizes[label] >= MIN_BODY_AREA]
    if len(bodies) != FRAMES:
        raise SystemExit(f"band {y0}..{y1}: expected {FRAMES} figures, found {len(bodies)}")

    centres = {label: ndimage.center_of_mass(labels == label) for label in bodies}
    boxes = {label: bbox(labels == label) for label in bodies}
    bodies.sort(key=lambda label: centres[label][1])
    assigned = {label: [label] for label in bodies}

    for label in range(1, count + 1):
        if label in assigned or sizes[label] < MIN_FRAGMENT_AREA:
            continue
        fragment = labels == label
        fragment_box = bbox(fragment)
        fragment_centre = ndimage.center_of_mass(fragment)
        owner = min(
            bodies,
            key=lambda body: (
                bbox_distance(fragment_box, boxes[body]),
                (fragment_centre[0] - centres[body][0]) ** 2
                + (fragment_centre[1] - centres[body][1]) ** 2,
            ),
        )
        assigned[owner].append(label)

    result = []
    for body_label in bodies:
        core = np.isin(labels, assigned[body_label])
        support = ndimage.binary_dilation(core, iterations=EDGE_RADIUS) & (local_score > EDGE_SCORE)
        cy0, cx0, cy1, cx1 = bbox(support)
        crop = rgba[y0 + cy0 : y0 + cy1, cx0:cx1].copy()
        matte = np.clip(
            (local_score[cy0:cy1, cx0:cx1] - EDGE_SCORE)
            / (CORE_SCORE - EDGE_SCORE),
            0.0,
            1.0,
        )
        a = matte[:, :, None]
        foreground = (
            crop[:, :, :3].astype(np.float32) - BACKGROUND_RGB * (1.0 - a)
        ) / np.maximum(a, 0.05)
        crop[:, :, :3] = np.clip(foreground, 0.0, 255.0).astype(np.uint8)
        crop[:, :, 3] = np.where(support[cy0:cy1, cx0:cx1], matte * 255.0, 0).astype(
            np.uint8
        )
        body = labels[cy0:cy1, cx0:cx1] == body_label
        result.append((Image.fromarray(crop), anchor_x(body)))
    return result, boxes, bodies


def main():
    source = Image.open(SOURCE).convert("RGB")
    rgb = np.asarray(source).astype(np.float32)
    rgba = np.dstack([rgb.astype(np.uint8), np.full(rgb.shape[:2], 255, dtype=np.uint8)])
    luminance = rgb.mean(axis=2)
    chroma = rgb.max(axis=2) - rgb.min(axis=2)
    score = np.maximum(BACKGROUND_LEVEL - luminance, chroma * 0.6)

    actions = []
    idle_heights = None
    for action, (y0, y1) in enumerate(ACTION_BANDS):
        row, boxes, bodies = extract_row(rgba, score, y0, y1)
        actions.append(row)
        if action == 0:
            idle_heights = [boxes[label][2] - boxes[label][0] for label in bodies]

    scale = TARGET_IDLE_HEIGHT / float(np.mean(idle_heights))
    for output, row in zip(OUTPUTS, actions):
        sheet = Image.new("RGBA", (CELL_W * FRAMES, CELL_H), (0, 0, 0, 0))
        for frame_number, (image, unscaled_anchor) in enumerate(row):
            image = image.resize(
                (max(1, round(image.width * scale)), max(1, round(image.height * scale))),
                Image.Resampling.LANCZOS,
            )
            x = CELL_W // 2 - round(unscaled_anchor * scale)
            y = BASELINE_Y - image.height
            if x < 1 or x + image.width >= CELL_W or y < 1 or y + image.height >= CELL_H:
                raise SystemExit(
                    f"{output} frame {frame_number} does not fit: "
                    f"{image.width}x{image.height} at ({x}, {y})"
                )
            sheet.alpha_composite(image, (frame_number * CELL_W + x, y))
        sheet.save(output)
        print(f"wrote {output} ({sheet.width}x{sheet.height})")


if __name__ == "__main__":
    main()
