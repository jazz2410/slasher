//! Loads the LDtk project and turns it into a collision grid plus spawn points.
//!
//! LDtk bakes the *resolved* tile placements into the saved project, so there is
//! nothing here a tilemap library would do for us that reading the JSON does not
//! — and `serde_json` is already in the dependency tree.
//!
//! Coordinates: LDtk's origin is the level's top-left with y growing downward;
//! Bevy's grows upward. The level is placed with its bottom-left corner on
//! `(0, GROUND_Y)`, so everything flips through [`Level::to_world`].

use bevy::prelude::*;
use serde::Deserialize;

use crate::run::{run_scoped, Run};
use crate::world::GROUND_Y;

const LEVELS_DIR: &str = "assets/levels";

/// IntGrid values, as defined in the LDtk project.
const EMPTY: u8 = 0;
const SOLID: u8 = 1;
const PLATFORM: u8 = 2;
#[allow(dead_code)]
const HAZARD: u8 = 3;

const TILESET_PATH: &str = "tiles/village.png";
const TILESET_TILE: UVec2 = UVec2::splat(16);
const TILESET_COLUMNS: u32 = 16;
const TILESET_ROWS: u32 = 8;

/// First tile of each nine-slice block in `village.png`, per the tileset spec.
const DIRT_BLOCK: usize = 0;
const STONE_BLOCK: usize = 16;
/// Offsets within a nine-slice block.
const TOP_LEFT: usize = 0;
const TOP: usize = 1;
const TOP_RIGHT: usize = 2;
const LEFT: usize = 3;
const FILL: usize = 4;
const RIGHT: usize = 5;
const BOTTOM_LEFT: usize = 6;
const BOTTOM: usize = 7;
const BOTTOM_RIGHT: usize = 8;
const ISOLATED: usize = 15;

/// Inset applied to the axis *not* being resolved.
///
/// A body at rest has its feet exactly on a tile's top edge. Without this
/// inset, the horizontal pass sees that floor cell inside the body's box,
/// mistakes the ground for a wall, and shoves the body sideways the moment it
/// tries to walk. Half a pixel is far below anything visible and comfortably
/// above floating-point drift.
const SKIN: f32 = 0.5;

pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        // PreStartup so the grid and spawn points exist before anything spawns.
        app.add_systems(PreStartup, load_levels)
            .add_systems(OnEnter(Run::Playing), (draw_background, draw_tiles));
    }
}

// --- the LDtk file, only the parts we use -----------------------------------

#[derive(Deserialize)]
struct Project {
    levels: Vec<RawLevel>,
}

#[derive(Deserialize)]
struct RawLevel {
    identifier: String,
    #[serde(rename = "pxWid")]
    px_wid: i32,
    #[serde(rename = "pxHei")]
    px_hei: i32,
    #[serde(rename = "layerInstances")]
    layers: Vec<RawLayer>,
    /// Level background image, set in LDtk's level properties. Relative to the
    /// project file, not to `assets/`.
    #[serde(default, rename = "bgRelPath")]
    bg_rel_path: Option<String>,
}

#[derive(Deserialize)]
struct RawLayer {
    #[serde(rename = "__identifier")]
    identifier: String,
    #[serde(rename = "__type")]
    kind: String,
    #[serde(rename = "__cWid")]
    c_wid: i32,
    #[serde(rename = "__cHei")]
    c_hei: i32,
    #[serde(rename = "__gridSize")]
    grid_size: i32,
    #[serde(default, rename = "intGridCsv")]
    int_grid: Vec<u8>,
    #[serde(default, rename = "entityInstances")]
    entities: Vec<RawEntity>,
    /// Hand-painted tiles.
    #[serde(default, rename = "gridTiles")]
    grid_tiles: Vec<RawTile>,
    /// Tiles LDtk's auto-rules produced. Already resolved in the saved file.
    #[serde(default, rename = "autoLayerTiles")]
    auto_tiles: Vec<RawTile>,
}

/// Pick the IntGrid that represents gameplay collision.
///
/// LDtk allows several IntGrid layers. The project uses an 8px helper layer
/// named `SmallIntgrid` alongside the 16px collision layer named `IntGrid`, so
/// taking the first layer by type can leave the game with an empty floor. Keep
/// the conventional collision name authoritative; for projects that rename
/// it, fall back to the grid containing the most painted collision cells.
fn collision_layer(layers: &[RawLayer]) -> Option<&RawLayer> {
    layers
        .iter()
        .find(|layer| layer.kind == "IntGrid" && layer.identifier == "IntGrid")
        .or_else(|| {
            layers
                .iter()
                .filter(|layer| layer.kind == "IntGrid")
                .max_by_key(|layer| {
                    layer
                        .int_grid
                        .iter()
                        .filter(|&&value| value != EMPTY)
                        .count()
                })
        })
}

#[derive(Deserialize)]
struct RawTile {
    /// Top-left of the tile, in level pixels.
    px: [f32; 2],
    /// Tile id within the tileset — the atlas index directly.
    t: usize,
    /// Flip bits: 1 = horizontal, 2 = vertical.
    #[serde(default)]
    f: u8,
}

#[derive(Deserialize)]
struct RawEntity {
    #[serde(rename = "__identifier")]
    identifier: String,
    /// Position of the entity's *pivot*, in level pixels — wherever that pivot
    /// happens to be. See [`feet_of`].
    px: [f32; 2],
    width: f32,
    height: f32,
    #[serde(rename = "__pivot")]
    pivot: [f32; 2],
}

/// Where an entity's feet are, given LDtk's pivot-relative position.
///
/// LDtk records the pivot point, and the pivot is a per-entity-definition
/// choice: top-left `(0, 0)` is the default, bottom-centre `(0.5, 1)` is what
/// suits a character. Converting here means a spawn marker stands on the ground
/// whichever the level author picked — assuming bottom-centre would put a
/// top-left entity a whole tile too high.
fn feet_of(px: [f32; 2], width: f32, height: f32, pivot: [f32; 2]) -> Vec2 {
    Vec2::new(
        px[0] + width * (0.5 - pivot[0]),
        px[1] + height * (1.0 - pivot[1]),
    )
}

/// Turn a path stored relative to the `.ldtk` file into one the `AssetServer`
/// understands, i.e. relative to `assets/`.
///
/// LDtk writes things like `../tiles/village.png`; Bevy wants `tiles/village.png`.
fn asset_path(project: &std::path::Path, relative: &str) -> Option<String> {
    use std::path::Component;

    let joined = project.parent()?.join(relative);
    let mut parts: Vec<std::ffi::OsString> = Vec::new();
    for component in joined.components() {
        match component {
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::Normal(part) => parts.push(part.to_os_string()),
            _ => {}
        }
    }
    let path: std::path::PathBuf = parts.iter().collect();
    Some(
        path.strip_prefix("assets")
            .ok()?
            .to_string_lossy()
            .replace('\\', "/"),
    )
}

// --- what the game actually uses --------------------------------------------

/// A named place something should be spawned, already in world coordinates.
#[derive(Clone)]
pub struct SpawnPoint {
    pub identifier: String,
    pub at: Vec2,
}

/// The loaded level: its collision grid and its spawn points.
#[derive(Resource, Clone)]
pub struct Level {
    pub name: String,
    width: usize,
    height: usize,
    tile: f32,
    /// World position of the level's bottom-left corner.
    origin: Vec2,
    cells: Vec<u8>,
    /// Additional, finer IntGrid layers are treated as one-way platforms.
    /// They retain their own grid size instead of being rounded onto the main
    /// collision grid.
    one_way_grids: Vec<OneWayGrid>,
    pub spawns: Vec<SpawnPoint>,
    /// Tiles painted in LDtk, already in world space. When this is non-empty it
    /// is what gets drawn; the nine-slice derivation is only for a level whose
    /// art has not been painted yet.
    painted: Vec<PaintedTile>,
    /// A single image covering the whole level, set in LDtk's level properties.
    /// An alternative to tiles entirely: paint the level as one picture and let
    /// the IntGrid carry the collision.
    background: Option<String>,
}

#[derive(Clone)]
pub struct PaintedTile {
    pub centre: Vec2,
    pub index: usize,
    pub flip_x: bool,
    pub flip_y: bool,
}

/// An auxiliary LDtk IntGrid whose occupied cells are jump-through platforms.
/// Keeping this separate lets an 8px ledge coexist with a 16px world grid.
#[derive(Clone)]
struct OneWayGrid {
    width: usize,
    height: usize,
    tile: f32,
    origin: Vec2,
    cells: Vec<u8>,
}

impl OneWayGrid {
    fn value(&self, cx: isize, cy: isize) -> u8 {
        if cx < 0 || cx >= self.width as isize || cy < 0 || cy >= self.height as isize {
            return EMPTY;
        }
        self.cells[cy as usize * self.width + cx as usize]
    }

    fn cell_corner(&self, cx: isize, cy: isize) -> Vec2 {
        self.origin
            + Vec2::new(
                cx as f32 * self.tile,
                (self.height as isize - 1 - cy) as f32 * self.tile,
            )
    }

    fn cells_over(&self, min: Vec2, max: Vec2) -> (isize, isize, isize, isize) {
        let local_min = min - self.origin;
        let local_max = max - self.origin;
        let top = self.height as f32 * self.tile;
        (
            (local_min.x / self.tile).floor() as isize,
            ((local_max.x - f32::EPSILON) / self.tile).floor() as isize,
            ((top - local_max.y) / self.tile).floor() as isize,
            ((top - local_min.y - f32::EPSILON) / self.tile).floor() as isize,
        )
    }

    /// Highest platform top crossed by the body's feet this frame.
    fn landing_top(&self, centre: Vec2, half: Vec2, previous_bottom: f32) -> Option<f32> {
        let probe = Vec2::new(half.x - SKIN, half.y);
        let current_bottom = centre.y - half.y;
        let (x0, x1, y0, y1) = self.cells_over(centre - probe, centre + probe);
        let mut landing: Option<f32> = None;

        for cy in y0..=y1 {
            for cx in x0..=x1 {
                if self.value(cx, cy) == EMPTY {
                    continue;
                }
                let top = self.cell_corner(cx, cy).y + self.tile;
                if previous_bottom >= top - 0.001 && current_bottom <= top {
                    landing = Some(landing.map_or(top, |current| current.max(top)));
                }
            }
        }
        landing
    }
}

/// Every level found in the LDtk project and which one is currently active.
/// The active [`Level`] is also installed as its own resource because all
/// gameplay systems already consume that directly.
#[derive(Resource)]
pub struct LevelCatalog {
    levels: Vec<Level>,
    selected: usize,
}

impl LevelCatalog {
    pub fn levels(&self) -> impl Iterator<Item = (usize, &str)> {
        self.levels
            .iter()
            .enumerate()
            .map(|(index, level)| (index, level.name.as_str()))
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Select and clone a level into the standalone runtime resource.
    pub fn select(&mut self, index: usize) -> Option<Level> {
        let level = self.levels.get(index)?.clone();
        self.selected = index;
        Some(level)
    }

    #[cfg(test)]
    pub fn for_test(levels: Vec<Level>) -> Self {
        assert!(!levels.is_empty());
        Self {
            levels,
            selected: 0,
        }
    }
}

impl Level {
    pub fn tile_size(&self) -> f32 {
        self.tile
    }

    /// The level's extent in world space, for keeping the camera inside it.
    pub fn bounds(&self) -> Rect {
        Rect::from_corners(
            self.origin,
            self.origin
                + Vec2::new(
                    self.width as f32 * self.tile,
                    self.height as f32 * self.tile,
                ),
        )
    }

    /// LDtk pixel coordinates (y down, from the level's top-left) to world.
    fn to_world(&self, px: Vec2) -> Vec2 {
        self.origin + Vec2::new(px.x, self.height as f32 * self.tile - px.y)
    }

    fn value(&self, cx: isize, cy: isize) -> u8 {
        // Outside the level's sides is solid, so nobody walks into the void.
        // Above and below is open, so a fall is still a fall.
        if cx < 0 || cx >= self.width as isize {
            return SOLID;
        }
        if cy < 0 || cy >= self.height as isize {
            return EMPTY;
        }
        self.cells[cy as usize * self.width + cx as usize]
    }

    /// Bottom-left corner of a cell, in world space.
    fn cell_corner(&self, cx: isize, cy: isize) -> Vec2 {
        self.origin
            + Vec2::new(
                cx as f32 * self.tile,
                (self.height as isize - 1 - cy) as f32 * self.tile,
            )
    }

    /// Inclusive cell range covering a world-space box.
    fn cells_over(&self, min: Vec2, max: Vec2) -> (isize, isize, isize, isize) {
        let local_min = min - self.origin;
        let local_max = max - self.origin;
        let top = self.height as f32 * self.tile;
        (
            (local_min.x / self.tile).floor() as isize,
            ((local_max.x - f32::EPSILON) / self.tile).floor() as isize,
            ((top - local_max.y) / self.tile).floor() as isize,
            ((top - local_min.y - f32::EPSILON) / self.tile).floor() as isize,
        )
    }

    /// Every solid or platform cell, for drawing the blockout.
    fn occupied(&self) -> impl Iterator<Item = (isize, isize, u8)> + '_ {
        (0..self.height as isize).flat_map(move |cy| {
            (0..self.width as isize).filter_map(move |cx| match self.value(cx, cy) {
                v @ (SOLID | PLATFORM) => Some((cx, cy, v)),
                _ => None,
            })
        })
    }

    /// A bare level carrying only spawn markers, for tests in other modules
    /// that need a level to exist without caring about its geometry.
    ///
    /// Deliberately huge, and solid below `y = 0` so a character stands where
    /// the no-level fallback puts him. An empty test level looks harmless and
    /// is not: bodies sink through it, drift out of bounds, and take anything
    /// positioned relative to them — a freshly loosed arrow, say — with them.
    #[cfg(test)]
    pub fn with_spawns(spawns: Vec<SpawnPoint>) -> Self {
        const CELLS: usize = 400;
        let tile = 16.0;
        let mut cells = vec![EMPTY; CELLS * CELLS];
        for cy in CELLS / 2..CELLS {
            for cx in 0..CELLS {
                cells[cy * CELLS + cx] = SOLID;
            }
        }
        Self {
            name: "test".into(),
            width: CELLS,
            height: CELLS,
            tile,
            origin: Vec2::splat(-(CELLS as f32) * tile / 2.0),
            cells,
            one_way_grids: Vec::new(),
            spawns,
            painted: Vec::new(),
            background: None,
        }
    }

    /// Whether a world point is inside solid ground. Used by anything that
    /// travels and should stop at a wall — an arrow, say — without needing the
    /// full body-resolution machinery.
    pub fn is_solid_at(&self, point: Vec2) -> bool {
        let local = point - self.origin;
        let top = self.height as f32 * self.tile;
        let cx = (local.x / self.tile).floor() as isize;
        let cy = ((top - local.y) / self.tile).floor() as isize;
        self.value(cx, cy) == SOLID
    }

    /// Somewhere sensible to stand when the level names no spawn point: the
    /// leftmost open cell with solid ground directly beneath it. Without this a
    /// missing `PlayerSpawn` drops the player outside the level, where
    /// [`Level::value`] reports solid and he is stuck inside the wall.
    pub fn default_spawn(&self) -> Vec2 {
        for cx in 0..self.width as isize {
            for cy in 0..self.height as isize {
                if self.value(cx, cy) == EMPTY && self.value(cx, cy + 1) == SOLID {
                    let corner = self.cell_corner(cx, cy);
                    return Vec2::new(corner.x + self.tile / 2.0, corner.y);
                }
            }
        }
        self.origin + Vec2::new(self.width as f32 * self.tile / 2.0, 0.0)
    }

    pub fn spawn(&self, identifier: &str) -> Option<Vec2> {
        self.spawns
            .iter()
            .find(|s| s.identifier == identifier)
            .map(|s| s.at)
    }

    pub fn all_spawns<'a>(&'a self, identifier: &'a str) -> impl Iterator<Item = Vec2> + 'a {
        self.spawns
            .iter()
            .filter(move |s| s.identifier == identifier)
            .map(|s| s.at)
    }

    /// Push a body out of any solid cell it overlaps horizontally.
    ///
    /// Axis-separated resolution: callers move on one axis, resolve, then the
    /// other. Simple and stable at these speeds; it would need sweeping if
    /// anything ever moved more than a tile per frame.
    pub fn resolve_horizontal(&self, centre: &mut Vec2, half: Vec2, velocity_x: &mut f32) {
        if *velocity_x == 0.0 {
            return;
        }
        // Shortened vertically so the floor underfoot is not read as a wall.
        let probe = Vec2::new(half.x, half.y - SKIN);
        let (x0, x1, y0, y1) = self.cells_over(*centre - probe, *centre + probe);
        for cy in y0..=y1 {
            for cx in x0..=x1 {
                if self.value(cx, cy) != SOLID {
                    continue;
                }
                let corner = self.cell_corner(cx, cy);
                centre.x = if *velocity_x > 0.0 {
                    corner.x - half.x
                } else {
                    corner.x + self.tile + half.x
                };
                *velocity_x = 0.0;
                return;
            }
        }
    }

    /// Push a body out of anything it landed on. Returns whether it is standing.
    ///
    /// `previous_bottom` is where the body's feet were before this frame's
    /// movement — a one-way platform only catches you if you were above it.
    pub fn resolve_vertical(
        &self,
        centre: &mut Vec2,
        half: Vec2,
        velocity_y: &mut f32,
        previous_bottom: f32,
    ) -> bool {
        // Narrowed horizontally, so brushing a wall is not read as standing on
        // it — the mirror of the inset in `resolve_horizontal`.
        let probe = Vec2::new(half.x - SKIN, half.y);
        let (x0, x1, y0, y1) = self.cells_over(*centre - probe, *centre + probe);
        for cy in y0..=y1 {
            for cx in x0..=x1 {
                let value = self.value(cx, cy);
                let corner = self.cell_corner(cx, cy);
                let top = corner.y + self.tile;

                let blocks = match value {
                    SOLID => true,
                    // Jump up through it, land on top of it.
                    PLATFORM => *velocity_y <= 0.0 && previous_bottom >= top - 0.001,
                    _ => false,
                };
                if !blocks {
                    continue;
                }

                if *velocity_y > 0.0 {
                    centre.y = corner.y - half.y;
                    *velocity_y = 0.0;
                    return false;
                }
                centre.y = top + half.y;
                *velocity_y = 0.0;
                return true;
            }
        }

        // Auxiliary grids represent thin ledges: pass through while rising,
        // but catch feet that cross their top edge while falling.
        if *velocity_y <= 0.0 {
            let landing = self
                .one_way_grids
                .iter()
                .filter_map(|grid| grid.landing_top(*centre, half, previous_bottom))
                .max_by(f32::total_cmp);
            if let Some(top) = landing {
                centre.y = top + half.y;
                *velocity_y = 0.0;
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TILE: f32 = 16.0;
    /// Half-extents of a spartan-sized body.
    const HALF: Vec2 = Vec2::new(9.0, 30.0);

    /// Build a level from ASCII, top row first — the same orientation LDtk uses.
    fn level(rows: &[&str]) -> Level {
        let width = rows[0].len();
        assert!(rows.iter().all(|r| r.len() == width));
        Level {
            name: "test".into(),
            width,
            height: rows.len(),
            tile: TILE,
            origin: Vec2::ZERO,
            cells: rows
                .iter()
                .flat_map(|r| r.chars())
                .map(|c| match c {
                    '#' => SOLID,
                    '=' => PLATFORM,
                    _ => EMPTY,
                })
                .collect(),
            one_way_grids: Vec::new(),
            spawns: Vec::new(),
            painted: Vec::new(),
            background: None,
        }
    }

    #[test]
    fn ldtk_relative_paths_resolve_to_asset_paths() {
        let project = std::path::Path::new("assets/levels/level1.ldtk");
        assert_eq!(
            asset_path(project, "../tiles/village.png").as_deref(),
            Some("tiles/village.png")
        );
        assert_eq!(
            asset_path(project, "backdrop.png").as_deref(),
            Some("levels/backdrop.png")
        );
        // Anything that climbs out of `assets/` cannot be served.
        assert_eq!(asset_path(project, "../../outside.png"), None);
    }

    #[test]
    fn catalog_selection_returns_the_chosen_level() {
        let mut first = level(&[".", "#"]);
        first.name = "Level_0".into();
        let mut second = level(&[".", "#"]);
        second.name = "Level_1".into();
        let mut catalog = LevelCatalog {
            levels: vec![first, second],
            selected: 0,
        };

        assert_eq!(
            catalog.levels().collect::<Vec<_>>(),
            vec![(0, "Level_0"), (1, "Level_1")]
        );
        let chosen = catalog.select(1).unwrap();
        assert_eq!(chosen.name, "Level_1");
        assert_eq!(catalog.selected(), 1);
    }

    #[test]
    fn named_collision_grid_wins_over_an_earlier_empty_helper_grid() {
        let layer = |identifier: &str, grid_size: i32, cells: Vec<u8>| RawLayer {
            identifier: identifier.into(),
            kind: "IntGrid".into(),
            c_wid: 2,
            c_hei: 2,
            grid_size,
            int_grid: cells,
            entities: Vec::new(),
            grid_tiles: Vec::new(),
            auto_tiles: Vec::new(),
        };
        let layers = vec![
            layer("SmallIntgrid", 8, vec![EMPTY; 4]),
            layer("IntGrid", 16, vec![EMPTY, EMPTY, SOLID, SOLID]),
        ];

        let selected = collision_layer(&layers).unwrap();
        assert_eq!(selected.identifier, "IntGrid");
        assert_eq!(selected.grid_size, 16);
        assert_eq!(selected.int_grid, vec![EMPTY, EMPTY, SOLID, SOLID]);
    }

    /// A spawn marker must land on the ground whichever pivot its definition
    /// uses. LDtk defaults new entities to top-left, so assuming bottom-centre
    /// silently floats every spawn by its own height.
    #[test]
    fn entity_feet_are_found_from_any_pivot() {
        // Top-left pivot: px is the box's upper-left corner.
        assert_eq!(
            feet_of([80.0, 272.0], 16.0, 16.0, [0.0, 0.0]),
            Vec2::new(88.0, 288.0)
        );
        // Bottom-centre pivot: px is already the feet.
        assert_eq!(
            feet_of([88.0, 288.0], 16.0, 32.0, [0.5, 1.0]),
            Vec2::new(88.0, 288.0)
        );
        // Centre pivot: half the height below the recorded point.
        assert_eq!(
            feet_of([100.0, 100.0], 16.0, 32.0, [0.5, 0.5]),
            Vec2::new(100.0, 116.0)
        );
    }

    /// LDtk's y grows downward from the level's top; Bevy's grows up from the
    /// bottom. Getting this backwards renders a level upside down, which is the
    /// classic first bug when wiring a tile editor to an engine.
    #[test]
    fn ldtk_pixels_flip_into_world_space() {
        let level = level(&["...", "...", "..."]); // 48px tall
        assert_eq!(level.to_world(Vec2::new(0.0, 0.0)), Vec2::new(0.0, 48.0));
        assert_eq!(level.to_world(Vec2::new(16.0, 48.0)), Vec2::new(16.0, 0.0));
        assert_eq!(level.to_world(Vec2::new(16.0, 32.0)), Vec2::new(16.0, 16.0));
    }

    #[test]
    fn bottom_row_sits_at_the_world_floor() {
        let level = level(&["...", "...", "###"]);
        // Bottom-left cell spans y 0..16.
        assert_eq!(level.cell_corner(0, 2), Vec2::new(0.0, 0.0));
        assert_eq!(level.cell_corner(0, 0), Vec2::new(0.0, 32.0));
    }

    #[test]
    fn a_falling_body_lands_on_solid_ground() {
        let level = level(&["...", "...", "###"]);
        let mut centre = Vec2::new(24.0, 40.0); // feet at 10, inside the tile
        let mut velocity = -100.0;

        let grounded = level.resolve_vertical(&mut centre, HALF, &mut velocity, 50.0);

        assert!(grounded);
        assert_eq!(velocity, 0.0);
        assert_eq!(centre.y, 16.0 + HALF.y, "feet should rest on the tile top");
    }

    #[test]
    fn walking_into_a_wall_stops_at_its_face() {
        let level = level(&["..#", "..#", "###"]);
        let mut centre = Vec2::new(28.0, 40.0);
        let mut velocity = 120.0;

        level.resolve_horizontal(&mut centre, HALF, &mut velocity);

        assert_eq!(velocity, 0.0);
        assert_eq!(centre.x, 32.0 - HALF.x, "should stop at the wall's left face");
    }

    #[test]
    fn nothing_happens_when_the_way_is_clear() {
        let level = level(&["...", "...", "###"]);
        let mut centre = Vec2::new(24.0, 100.0);
        let mut velocity = 120.0;

        level.resolve_horizontal(&mut centre, HALF, &mut velocity);

        assert_eq!(centre.x, 24.0);
        assert_eq!(velocity, 120.0);
    }

    #[test]
    fn a_one_way_platform_is_passed_from_below_and_landed_on_from_above() {
        // Platform row spans y 16..32, so its top is 32.
        let level = level(&["...", "===", "..."]);
        let feet_on_top = 32.0;

        // Rising through it: ignored.
        let mut centre = Vec2::new(24.0, 40.0);
        let mut velocity = 200.0;
        let grounded = level.resolve_vertical(&mut centre, HALF, &mut velocity, 20.0);
        assert!(!grounded, "a platform must not stop a jump from below");
        assert_eq!(velocity, 200.0);

        // Falling onto it from above: caught.
        let mut centre = Vec2::new(24.0, 60.0);
        let mut velocity = -200.0;
        let grounded = level.resolve_vertical(&mut centre, HALF, &mut velocity, feet_on_top + 1.0);
        assert!(grounded, "falling from above should land on the platform");
        assert_eq!(centre.y, feet_on_top + HALF.y);

        // Falling while already below it: ignored, so you do not snap upward.
        let mut centre = Vec2::new(24.0, 40.0);
        let mut velocity = -200.0;
        let grounded = level.resolve_vertical(&mut centre, HALF, &mut velocity, 5.0);
        assert!(!grounded, "a platform must not catch a body already beneath it");
    }

    #[test]
    fn a_finer_auxiliary_grid_provides_one_way_platforms() {
        let mut level = level(&["......", "......", "......", "......", "......", "......"]);
        let mut cells = vec![EMPTY; 12 * 12];
        // On an 8px grid, row 6 has its top at y=48.
        for cx in 0..12 {
            cells[6 * 12 + cx] = SOLID;
        }
        level.one_way_grids.push(OneWayGrid {
            width: 12,
            height: 12,
            tile: 8.0,
            origin: Vec2::ZERO,
            cells,
        });

        let mut falling = Vec2::new(24.0, 46.0 + HALF.y);
        let mut fall_speed = -200.0;
        assert!(level.resolve_vertical(&mut falling, HALF, &mut fall_speed, 52.0));
        assert_eq!(falling.y, 48.0 + HALF.y);

        let mut rising = Vec2::new(24.0, 46.0 + HALF.y);
        let mut rise_speed = 200.0;
        assert!(!level.resolve_vertical(&mut rising, HALF, &mut rise_speed, 40.0));
        assert_eq!(rise_speed, 200.0);
    }

    /// Walking across flat ground must be uneventful. The body rests with its
    /// feet exactly on a tile's top edge, which is the boundary case for
    /// `cells_over` — if that range includes the floor cell, horizontal
    /// resolution treats the ground as a wall and shoves the body sideways.
    #[test]
    fn walking_along_flat_ground_does_not_displace() {
        let mut rows: Vec<String> = vec![".".repeat(40); 18];
        rows.extend(vec!["#".repeat(40); 4]);
        let refs: Vec<&str> = rows.iter().map(String::as_str).collect();
        let level = level(&refs);

        let surface = level.cell_corner(0, 18).y + TILE;
        let mut centre = Vec2::new(88.0, surface + HALF.y);
        let mut speed = 160.0;
        let dt = 1.0 / 60.0;

        for frame in 0..60 {
            let previous_bottom = centre.y - HALF.y;
            let before = centre.x;

            centre.x += speed * dt;
            level.resolve_horizontal(&mut centre, HALF, &mut speed);

            let mut fall = -20.0;
            centre.y += fall * dt;
            let grounded = level.resolve_vertical(&mut centre, HALF, &mut fall, previous_bottom);

            assert!(grounded, "frame {frame}: should stay on the ground");
            assert_eq!(centre.y, surface + HALF.y, "frame {frame}: lifted off");
            assert_eq!(speed, 160.0, "frame {frame}: flat ground stopped him");
            assert!(centre.x > before, "frame {frame}: pushed backwards");
        }
    }

    #[test]
    fn the_level_sides_are_walls() {
        let level = level(&["...", "...", "###"]);
        assert_eq!(level.value(-1, 1), SOLID, "left of the level is solid");
        assert_eq!(level.value(3, 1), SOLID, "right of the level is solid");
        assert_eq!(level.value(1, -1), EMPTY, "above the level is open");
        assert_eq!(level.value(1, 5), EMPTY, "below the level is open");
    }
}

/// The first `.ldtk` in `assets/levels`. Discovered rather than hardcoded so
/// renaming a level file does not silently fall back to flat ground.
fn find_project() -> Option<std::path::PathBuf> {
    let mut found: Vec<_> = std::fs::read_dir(LEVELS_DIR)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "ldtk"))
        .collect();
    found.sort();
    found.into_iter().next()
}

fn load_levels(mut commands: Commands) {
    let Some(path) = find_project() else {
        warn!("no .ldtk in {LEVELS_DIR}; falling back to flat ground");
        return;
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            warn!("cannot read {}: {error}", path.display());
            return;
        }
    };
    let project: Project = match serde_json::from_str(&text) {
        Ok(project) => project,
        Err(error) => {
            error!("{} is not a readable LDtk project: {error}", path.display());
            return;
        }
    };
    let levels: Vec<Level> = project
        .levels
        .into_iter()
        .filter_map(|raw| build_level(raw, &path))
        .collect();
    let Some(level) = levels.first().cloned() else {
        error!("{} contains no levels", path.display());
        return;
    };

    info!(
        "LDtk catalog: {} level(s), starting with '{}'",
        levels.len(),
        level.name
    );
    commands.insert_resource(LevelCatalog {
        levels,
        selected: 0,
    });
    commands.insert_resource(level);
}

fn build_level(raw: RawLevel, path: &std::path::Path) -> Option<Level> {
    let Some(collision) = collision_layer(&raw.layers) else {
        error!(
            "level '{}' in {} has no IntGrid layer to take collision from",
            raw.identifier,
            path.display()
        );
        return None;
    };
    let (width, height) = (collision.c_wid as usize, collision.c_hei as usize);
    if collision.int_grid.len() != width * height {
        error!(
            "'{}' has {} cells but is {width}x{height}",
            collision.identifier,
            collision.int_grid.len()
        );
        return None;
    }

    let one_way_grids: Vec<OneWayGrid> = raw
        .layers
        .iter()
        .filter(|layer| layer.kind == "IntGrid" && !std::ptr::eq(*layer, collision))
        .filter_map(|layer| {
            let width = layer.c_wid as usize;
            let height = layer.c_hei as usize;
            if layer.int_grid.len() != width * height {
                warn!(
                    "ignoring auxiliary IntGrid '{}' in level '{}': {} cells but {width}x{height}",
                    layer.identifier,
                    raw.identifier,
                    layer.int_grid.len()
                );
                return None;
            }
            Some(OneWayGrid {
                width,
                height,
                tile: layer.grid_size as f32,
                origin: Vec2::new(0.0, GROUND_Y),
                cells: layer.int_grid.clone(),
            })
        })
        .collect();
    let one_way_cells = one_way_grids
        .iter()
        .flat_map(|grid| grid.cells.iter())
        .filter(|&&value| value != EMPTY)
        .count();

    let mut level = Level {
        name: raw.identifier,
        width,
        height,
        tile: collision.grid_size as f32,
        origin: Vec2::new(0.0, GROUND_Y),
        cells: collision.int_grid.clone(),
        one_way_grids,
        spawns: Vec::new(),
        painted: Vec::new(),
        background: raw
            .bg_rel_path
            .as_deref()
            .and_then(|rel| asset_path(path, rel)),
    };

    level.spawns = raw
        .layers
        .iter()
        .flat_map(|layer| layer.entities.iter())
        .map(|entity| SpawnPoint {
            identifier: entity.identifier.clone(),
            at: level.to_world(feet_of(
                entity.px,
                entity.width,
                entity.height,
                entity.pivot,
            )),
        })
        .collect();

    let half = level.tile / 2.0;
    level.painted = raw
        .layers
        .iter()
        .rev() // LDtk lists topmost first; draw in reverse so upper layers win
        .flat_map(|layer| layer.grid_tiles.iter().chain(layer.auto_tiles.iter()))
        .map(|tile| PaintedTile {
            centre: level.to_world(Vec2::new(tile.px[0] + half, tile.px[1] + half)),
            index: tile.t,
            flip_x: tile.f & 1 != 0,
            flip_y: tile.f & 2 != 0,
        })
        .collect();

    let solid = level.cells.iter().filter(|&&v| v == SOLID).count();
    let platform = level.cells.iter().filter(|&&v| v == PLATFORM).count();
    info!(
        "level '{}' from {}: {}x{} tiles ({}px), {solid} solid, {platform} platform, \
         {one_way_cells} auxiliary platform cells, \
         {} painted tiles, {} spawns",
        level.name,
        path.display(),
        width,
        height,
        level.tile,
        level.painted.len(),
        level.spawns.len()
    );
    debug_assert_eq!(raw.px_wid as usize, width * collision.grid_size as usize);
    debug_assert_eq!(raw.px_hei as usize, height * collision.grid_size as usize);

    Some(level)
}

impl Level {
    /// Pick a nine-slice tile for a cell from which of its neighbours match.
    ///
    /// This is the same job LDtk's auto-rules do. Doing it here means a level
    /// renders correctly straight from painted collision, with no rule setup —
    /// and any tiles actually painted in LDtk take priority over it.
    fn nine_slice(&self, cx: isize, cy: isize, block: usize) -> usize {
        let same = |x: isize, y: isize| self.value(x, y) == self.value(cx, cy);
        let (up, down, left, right) = (
            same(cx, cy - 1),
            same(cx, cy + 1),
            same(cx - 1, cy),
            same(cx + 1, cy),
        );
        block
            + match (up, down, left, right) {
                (false, true, false, true) => TOP_LEFT,
                (false, true, true, true) => TOP,
                (false, true, true, false) => TOP_RIGHT,
                (true, true, false, true) => LEFT,
                (true, true, true, true) => FILL,
                (true, true, true, false) => RIGHT,
                (true, false, false, true) => BOTTOM_LEFT,
                (true, false, true, true) => BOTTOM,
                (true, false, true, false) => BOTTOM_RIGHT,
                // Thin runs: a one-tile-wide ledge or column has no interior.
                (false, false, true, true) => TOP,
                (false, false, false, true) => TOP_LEFT,
                (false, false, true, false) => TOP_RIGHT,
                (false, true, false, false) => TOP,
                (true, false, false, false) => BOTTOM,
                (true, true, false, false) => FILL,
                (false, false, false, false) => ISOLATED,
            }
    }
}

/// Z ordering. Characters sit at 10, so everything here stays behind them.
const BACKGROUND_Z: f32 = -10.0;
const TILE_Z: f32 = 0.0;

/// Draw the level's background image, if it has one.
///
/// This is the tileset-free route: paint the level as a single picture the size
/// of the level, and let the IntGrid carry collision independently. Stretched to
/// the level's bounds, so an image whose pixel size matches the level maps 1:1.
fn draw_background(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    level: Option<Res<Level>>,
) {
    let Some(level) = level else { return };
    let Some(path) = level.background.clone() else {
        return;
    };
    let bounds = level.bounds();

    info!("level background: {path}");
    commands.spawn((
        Name::new("Level Background"),
        run_scoped(),
        Sprite {
            image: asset_server.load(path),
            custom_size: Some(bounds.size()),
            ..default()
        },
        Transform::from_xyz(bounds.center().x, bounds.center().y, BACKGROUND_Z),
    ));
}

/// Draw the level's tiles. Painted tiles are used as-is; a level with neither
/// tiles nor a background falls back to deriving terrain from its collision, so
/// blocked-out geometry is still visible before any art exists.
fn draw_tiles(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    level: Option<Res<Level>>,
) {
    let Some(level) = level else { return };

    // A level painted as a single background needs no tileset at all. Loading
    // one anyway asks the asset server for a file that may not exist, which is
    // just noise in the log.
    let derive_terrain = level.painted.is_empty() && level.background.is_none();
    if level.painted.is_empty() && !derive_terrain {
        return;
    }

    let file = std::path::Path::new("assets").join(TILESET_PATH);
    if !file.exists() {
        warn!(
            "level wants tiles but {} is missing; drawing nothing",
            file.display()
        );
        return;
    }

    let image = asset_server.load(TILESET_PATH);
    let layout = layouts.add(TextureAtlasLayout::from_grid(
        TILESET_TILE,
        TILESET_COLUMNS,
        TILESET_ROWS,
        None,
        None,
    ));
    let sprite = |index: usize| {
        Sprite::from_atlas_image(
            image.clone(),
            TextureAtlas {
                layout: layout.clone(),
                index,
            },
        )
    };

    if !level.painted.is_empty() {
        for tile in &level.painted {
            let mut s = sprite(tile.index);
            s.flip_x = tile.flip_x;
            s.flip_y = tile.flip_y;
            commands.spawn((
                Name::new("Painted Tile"),
                run_scoped(),
                s,
                Transform::from_xyz(tile.centre.x, tile.centre.y, TILE_Z),
            ));
        }
        return;
    }

    let half = level.tile_size() / 2.0;
    for (cx, cy, value) in level.occupied() {
        let index = match value {
            SOLID => level.nine_slice(cx, cy, DIRT_BLOCK),
            // Platforms are thin, so they only ever want the lit top edge.
            _ => STONE_BLOCK + TOP,
        };
        let corner = level.cell_corner(cx, cy);
        commands.spawn((
            Name::new(format!("Tile {cx},{cy}")),
            run_scoped(),
            sprite(index),
            Transform::from_xyz(corner.x + half, corner.y + half, TILE_Z),
        ));
    }
}
