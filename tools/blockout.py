"""Write a starter blockout into assets/levels/village.ldtk.

Paints the Collision IntGrid from an ASCII map and places the entities, then
re-validates the whole project against LDtk's JSON schema.

Close LDtk before running: it holds the project in memory and will overwrite
this on its next save.

Usage:  python3 tools/blockout.py
"""
import json
import sys
import uuid

PROJECT = "assets/levels/village.ldtk"

# '.' empty (walkable air)   '#' Solid   '=' Platform (one-way)   '^' Hazard
#
# Reads left to right: village edge, a jumpable ditch, then open ground for the
# fight. Platforms sit within one jump (~4.6 tiles) of the surface below them.
MAP = """\
........................................
........................................
........................................
........................................
........................................
........................................
........................................
........................................
........................................
........................................
........................................
........................................
..................======................
........................................
........................................
.........#####..............######......
........................................
........................................
........................................
####################......##############
####################......##############
########################################
"""

VALUES = {".": 0, "#": 1, "=": 2, "^": 3}

# (identifier, grid column, grid row of the surface the entity stands on)
ENTITIES = [
    ("PlayerSpawn", 3, 19),
    ("Enemy", 15, 19),
    ("Enemy", 31, 19),
    ("Torch", 8, 19),
    ("Torch", 27, 19),
    ("LevelExit", 37, 19),
]


def main():
    project = json.load(open(PROJECT))
    grid = project["defaultGridSize"]
    level = project["levels"][0]

    rows = [r for r in MAP.splitlines() if r]
    cols = len(rows[0])
    if any(len(r) != cols for r in rows):
        sys.exit("map rows are not all the same width")

    layers = {li["__identifier"]: li for li in level["layerInstances"]}
    collision = layers["Collision"]
    if (collision["__cWid"], collision["__cHei"]) != (cols, len(rows)):
        sys.exit(
            f"map is {cols}x{len(rows)} but the level is "
            f"{collision['__cWid']}x{collision['__cHei']}"
        )

    collision["intGridCsv"] = [VALUES[ch] for row in rows for ch in row]

    defs = {e["identifier"]: e for e in project["defs"]["entities"]}
    instances = []
    for identifier, cx, surface_row in ENTITIES:
        d = defs[identifier]
        # Entity pivots are bottom-centre, matching the sprite's feet origin, so
        # the pivot sits exactly on the surface it stands on.
        px = [cx * grid + grid // 2, surface_row * grid]
        instances.append({
            "__identifier": identifier,
            "__grid": [px[0] // grid, px[1] // grid],
            "__pivot": [d["pivotX"], d["pivotY"]],
            "__tags": d["tags"],
            "__smartColor": d["color"],
            "__tile": None,
            "__worldX": level["worldX"] + px[0],
            "__worldY": level["worldY"] + px[1],
            "iid": str(uuid.uuid4()),
            "defUid": d["uid"],
            "px": px,
            "width": d["width"],
            "height": d["height"],
            "fieldInstances": [],
        })
    layers["Entities"]["entityInstances"] = instances

    json.dump(project, open(PROJECT, "w"), indent=2)

    counts = {}
    for v in collision["intGridCsv"]:
        counts[v] = counts.get(v, 0) + 1
    print(f"wrote {PROJECT}")
    print(f"  collision {cols}x{len(rows)}  " + "  ".join(
        f"{name}={counts.get(val, 0)}" for name, val in
        [("empty", 0), ("solid", 1), ("platform", 2), ("hazard", 3)]))
    for identifier, cx, row in ENTITIES:
        print(f"  {identifier:12} grid ({cx}, {row})")
    return project


if __name__ == "__main__":
    main()
