//! Placeholder scenery so the scrolling is actually visible. Swap this out for
//! real tilemap/level loading later.

use bevy::prelude::*;

/// Y coordinate of the ground surface. Everything that stands on the floor
/// positions itself relative to this.
pub const GROUND_Y: f32 = 0.0;

const GROUND_THICKNESS: f32 = 400.0;
const GROUND_WIDTH: f32 = 20_000.0;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_ground);
    }
}

fn spawn_ground(mut commands: Commands) {
    commands.spawn((
        Name::new("Ground"),
        Sprite::from_color(
            Color::srgb(0.20, 0.17, 0.15),
            Vec2::new(GROUND_WIDTH, GROUND_THICKNESS),
        ),
        Transform::from_xyz(0.0, GROUND_Y - GROUND_THICKNESS / 2.0, 0.0),
    ));

    // Evenly spaced pillars give the eye something to parallax against.
    for i in -20..=20 {
        commands.spawn((
            Name::new(format!("Pillar {i}")),
            Sprite::from_color(Color::srgb(0.26, 0.25, 0.30), Vec2::new(24.0, 180.0)),
            Transform::from_xyz(i as f32 * 300.0, GROUND_Y + 90.0, -5.0),
        ));
    }
}
