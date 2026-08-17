"""Generate assets/levels/village.ldtk and validate it against LDtk's schema."""
import json
import sys
import uuid

GRID = 16
COLS, ROWS = 40, 22          # one screen: 640 x 352
PX_W, PX_H = COLS * GRID, ROWS * GRID
TILESET_PATH = "../tiles/village.png"   # relative to the .ldtk file
TILESET_W, TILESET_H = 256, 128

UID = iter(range(1, 500))
def uid():
    return next(UID)

def iid():
    return str(uuid.uuid4())

TS_UID = uid()
L_COLLISION, L_DECO, L_ENTITIES = uid(), uid(), uid()
E_SPAWN, E_ENEMY, E_EXIT, E_TORCH = uid(), uid(), uid(), uid()
LEVEL_UID = uid()


def layer_def(uid_, ident, type_, **over):
    d = {
        "__type": type_, "type": type_, "identifier": ident, "uid": uid_,
        "gridSize": GRID, "displayOpacity": 1.0, "inactiveOpacity": 0.6,
        "intGridValues": [], "intGridValuesGroups": [], "autoRuleGroups": [],
        "parallaxFactorX": 0.0, "parallaxFactorY": 0.0, "parallaxScaling": True,
        "pxOffsetX": 0, "pxOffsetY": 0, "guideGridWid": 0, "guideGridHei": 0,
        "canSelectWhenInactive": True, "hideFieldsWhenInactive": False,
        "hideInList": False, "renderInWorldView": True, "useAsyncRender": False,
        "excludedTags": [], "requiredTags": [], "uiFilterTags": [],
        "tilePivotX": 0.0, "tilePivotY": 0.0,
    }
    d.update(over)
    return d


def entity_def(uid_, ident, colour, w, h):
    return {
        "identifier": ident, "uid": uid_, "color": colour,
        "width": w, "height": h,
        "pivotX": 0.5, "pivotY": 1.0,          # feet, matching the sprite origin
        "tileRenderMode": "FitInside", "renderMode": "Rectangle",
        "nineSliceBorders": [], "fieldDefs": [], "tags": [],
        "allowOutOfBounds": False, "exportToToc": False,
        "fillOpacity": 0.4, "lineOpacity": 1.0, "tileOpacity": 1.0,
        "hollow": False, "keepAspectRatio": False,
        "limitBehavior": "MoveLastOne", "limitScope": "PerLevel", "maxCount": 0,
        "resizableX": False, "resizableY": False, "showName": True,
    }


int_grid_values = [
    {"value": 1, "identifier": "Solid",    "color": "#5A4F45", "groupUid": 0, "tile": None},
    {"value": 2, "identifier": "Platform", "color": "#8E4529", "groupUid": 0, "tile": None},
    {"value": 3, "identifier": "Hazard",   "color": "#A31C18", "groupUid": 0, "tile": None},
]

layers = [
    # First in the list renders on top.
    layer_def(L_ENTITIES, "Entities", "Entities"),
    layer_def(L_DECO, "Deco", "Tiles", tilesetDefUid=TS_UID),
    layer_def(
        L_COLLISION, "Collision", "IntGrid",
        intGridValues=int_grid_values,
        autoTilesetDefUid=TS_UID,
    ),
]

entities = [
    entity_def(E_SPAWN, "PlayerSpawn", "#7AC943", GRID, GRID * 2),
    entity_def(E_ENEMY, "Enemy", "#A31C18", GRID, GRID * 2),
    entity_def(E_EXIT, "LevelExit", "#F7C64F", GRID, GRID * 2),
    entity_def(E_TORCH, "Torch", "#E8842A", GRID, GRID),
]

tilesets = [{
    "identifier": "Village", "uid": TS_UID, "relPath": TILESET_PATH,
    "pxWid": TILESET_W, "pxHei": TILESET_H,
    "__cWid": TILESET_W // GRID, "__cHei": TILESET_H // GRID,
    "tileGridSize": GRID, "spacing": 0, "padding": 0,
    "customData": [], "enumTags": [], "tags": [], "savedSelections": [],
    "cachedPixelData": None, "embedAtlas": None, "tagsSourceEnumUid": None,
}]


def layer_instance(def_uid, ident, type_):
    inst = {
        "__identifier": ident, "__type": type_, "__gridSize": GRID,
        "__cWid": COLS, "__cHei": ROWS, "__opacity": 1.0,
        "__pxTotalOffsetX": 0, "__pxTotalOffsetY": 0,
        "__tilesetDefUid": TS_UID if type_ in ("Tiles", "IntGrid") else None,
        "__tilesetRelPath": TILESET_PATH if type_ in ("Tiles", "IntGrid") else None,
        "iid": iid(), "levelId": LEVEL_UID, "layerDefUid": def_uid,
        "pxOffsetX": 0, "pxOffsetY": 0, "visible": True, "seed": 1234,
        "autoLayerTiles": [], "gridTiles": [], "entityInstances": [],
        "intGridCsv": [], "optionalRules": [],
    }
    if type_ == "IntGrid":
        inst["intGridCsv"] = [0] * (COLS * ROWS)
    return inst


level = {
    "identifier": "Village_01", "iid": iid(), "uid": LEVEL_UID,
    "pxWid": PX_W, "pxHei": PX_H,
    "worldX": 0, "worldY": 0, "worldDepth": 0,
    "__bgColor": "#0D0F13", "__smartColor": "#77685A",
    "bgColor": "#0D0F13",
    "bgPivotX": 0.5, "bgPivotY": 0.5,
    "__neighbours": [], "fieldInstances": [],
    "useAutoIdentifier": False,
    "externalRelPath": None, "bgRelPath": None, "bgPos": None,
    "__bgPos": None, "layerInstances": [
        layer_instance(L_ENTITIES, "Entities", "Entities"),
        layer_instance(L_DECO, "Deco", "Tiles"),
        layer_instance(L_COLLISION, "Collision", "IntGrid"),
    ],
}

world_iid = iid()
project = {
    "__header__": {
        "fileType": "LDtk Project JSON", "app": "LDtk", "doc": "https://ldtk.io/json",
        "schema": "https://ldtk.io/files/JSON_SCHEMA.json",
        "appAuthor": "Sebastien 'deepnight' Benard",
        "appVersion": "1.5.3", "url": "https://ldtk.io",
    },
    "iid": iid(), "jsonVersion": "1.5.3", "appBuildId": 473451,
    "nextUid": 500,
    "identifierStyle": "Capitalize", "toc": [], "worlds": [],
    "dummyWorldIid": world_iid,
    "worldLayout": "Free", "worldGridWidth": PX_W, "worldGridHeight": PX_H,
    "defaultLevelWidth": PX_W, "defaultLevelHeight": PX_H,
    "defaultGridSize": GRID,
    "defaultEntityWidth": GRID, "defaultEntityHeight": GRID * 2,
    "defaultPivotX": 0.5, "defaultPivotY": 1.0,
    "bgColor": "#0D0F13", "defaultLevelBgColor": "#0D0F13",
    "externalLevels": False, "exportLevelBg": True,
    "exportTiled": False, "imageExportMode": "None",
    "simplifiedExport": False, "minifyJson": False,
    "backupOnSave": False, "backupLimit": 10, "backupRelPath": None,
    "levelNamePattern": "Level_%idx",
    "customCommands": [], "flags": [], "tutorialDesc": None,
    "defs": {
        "layers": layers, "entities": entities, "tilesets": tilesets,
        "enums": [], "externalEnums": [], "levelFields": [],
    },
    "levels": [level],
}

out = "assets/levels/village.ldtk"
with open(out, "w") as fh:
    json.dump(project, fh, indent=2)

# ---- validate ----
import jsonschema
schema = json.load(open(
    "/private/tmp/claude-501/-Users-tonidhaliwal-projects-slasher/"
    "1f50789c-84e0-4d7b-beba-8262aca08819/scratchpad/ldtk_schema.json"
))
resolver_store = {"": schema}
validator = jsonschema.Draft7Validator(
    {"$ref": "#/LdtkJsonRoot", **{k: v for k, v in schema.items() if not k.startswith("$")}}
)
errors = sorted(validator.iter_errors(project), key=lambda e: list(e.path))
if errors:
    print(f"{len(errors)} schema error(s):")
    for e in errors[:25]:
        path = "/".join(str(p) for p in e.path) or "<root>"
        print(f"  {path}: {e.message[:150]}")
    sys.exit(1)

print(f"wrote {out}")
print(f"  schema-valid against LDtk {schema['version']}")
print(f"  level Village_01  {PX_W}x{PX_H}px  ({COLS}x{ROWS} cells of {GRID}px)")
print(f"  layers: Collision (IntGrid 1/2/3), Deco (Tiles), Entities")
print(f"  entities: PlayerSpawn, Enemy, LevelExit, Torch")
